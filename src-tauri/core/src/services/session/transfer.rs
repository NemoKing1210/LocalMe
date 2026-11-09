//! The file-transfer state machine.
//!
//! Everything a transfer needs beyond the byte-level work in [`crate::services::attachment`]:
//! what a `file_chunk`, `file_done`, `file_ack`, `file_cancel` or `file_request` frame does to
//! the transfer, how a transfer is started, resumed, paced and failed, and what is persisted
//! along the way. The session's actor loop stays in `session.rs` and calls in here.
//!
//! The state itself lives on [`PeerEntry`](super::PeerEntry) — one outgoing transfer and a map
//! of incoming ones per peer — and the invariants are the ones the session documents: the part
//! file on disk is the authority on how much arrived, and a transfer is only finished once the
//! recipient's digest agrees.

use super::*;

/// One step of a peer's outgoing transfer.
pub(super) enum PumpOutcome {
    /// A chunk went out; the loop may try another.
    Sent,
    /// The reader returned nothing; try again on the next turn.
    Idle,
    /// Nothing more to do for this peer this turn.
    Halt,
    /// The last chunk is out and `file_done` has been sent.
    Finished,
    /// A real failure; the caller fails the transfer and starts the next one.
    Failed(String),
}

impl<C: Clock, S: Store> Session<C, S> {
    /// Removes a part file off the actor. Called when a transfer is abandoned, so the unlink
    /// (which a Windows filter driver can make slow) cannot stall the peers that are fine.
    async fn discard_transfer(mut transfer: IncomingTransfer) {
        let _ = offload(move || transfer.discard()).await;
    }

    /// The checks both incoming attachment frames need, and the answer when there is nothing to
    /// do with the frame.
    ///
    /// One copy of the validation — the row exists, it is this peer's, it is an incoming
    /// transfer — and one place that answers a retransmission for a file that is already
    /// complete, so the chunk and completion paths cannot drift apart.
    pub(super) async fn accept_incoming_attachment(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
    ) -> Option<(PeerLink, Attachment)> {
        let link = self.peers.get(&peer).and_then(|entry| entry.link.clone())?;
        let row = match self.store.attachment(id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                self.refuse(peer, id, FileCancelReason::Unknown);
                return None;
            }
            Err(error) => {
                tracing::warn!(%error, "failed to read an attachment");
                return None;
            }
        };
        if row.peer != peer || row.direction != Direction::Incoming {
            self.refuse(peer, id, FileCancelReason::Unknown);
            return None;
        }
        if row.state == AttachmentState::Complete {
            // A retransmission after the recipient already had the file: say so and stop.
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received: row.size,
                state: FileAckState::Complete,
            });
            return None;
        }
        Some((link, row))
    }

    /// Opens, or adopts, the part file for an incoming attachment.
    ///
    /// Returns `false` when the transfer has been failed and the peer told, so the caller stops.
    pub(super) async fn open_part_file(&mut self, peer: DeviceId, attachment: &Attachment) -> bool {
        let opened = {
            // The borrow is scoped to the call: `files_root` and `peers` are different fields,
            // and holding it across an await below would keep `self` borrowed.
            let files_root = self.files_root.as_path();
            self.peers
                .get_mut(&peer)
                .map(|entry| entry.ensure_incoming(attachment, files_root))
        };
        match opened {
            Some(Ok(())) => true,
            Some(Err(error)) => {
                tracing::warn!(%error, id = %attachment.id, "could not open the destination file");
                self.fail_incoming(peer, attachment.id, FileCancelReason::Failed)
                    .await;
                false
            }
            None => false,
        }
    }

    /// One chunk of a file from `peer`.
    pub(super) async fn on_file_chunk(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        offset: u64,
        data: Vec<u8>,
    ) {
        let Some((link, row)) = self.accept_incoming_attachment(peer, id).await else {
            return;
        };
        if row.size > MAX_ATTACHMENT_BYTES {
            self.set_attachment_state(peer, id, AttachmentState::Cancelled)
                .await;
            self.refuse(peer, id, FileCancelReason::TooLarge);
            return;
        }

        // A row whose transfer had ended is revived: the sender is trying again, and the fastest
        // way to agree on where to continue is to let it send and correct it. An offered row is
        // revived too: a sender that streams without being asked is welcome, and the row must
        // stop claiming nothing has been requested.
        let mut attachment = row;
        if attachment.state.is_finished() || attachment.state == AttachmentState::Offered {
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.incoming.remove(&id);
            }
            attachment.state = AttachmentState::Receiving;
            attachment.transferred = 0;
            if self
                .store
                .set_attachment_state(id, AttachmentState::Receiving)
                .await
                .is_err()
            {
                return;
            }
            self.persist_attachment_progress(id, 0).await;
            self.emit_attachment(peer, attachment.clone());
        }

        if !self.open_part_file(peer, &attachment).await {
            return;
        }

        let entry = match self.peers.get_mut(&peer) {
            Some(entry) => entry,
            None => return,
        };
        let Some(transfer) = entry.incoming.remove(&id) else {
            return;
        };
        let too_much = offset.saturating_add(u64::try_from(data.len()).unwrap_or(u64::MAX))
            > transfer.attachment.size;
        if too_much {
            let received = transfer.received;
            entry.incoming.insert(id, transfer);
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
            return;
        }

        // The write leaves the actor: a stalled disk must not hold up every other peer.
        let Some((transfer, result)) = offload(move || {
            let mut transfer = transfer;
            let result = transfer.append(offset, &data);
            (transfer, result)
        })
        .await
        else {
            return;
        };
        let mut transfer = transfer;

        match result {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {
                // Not the next chunk: report where the file really ends and let the sender
                // rewind. This is the mechanism that makes a resumed transfer converge.
                let received = transfer.received;
                if let Some(entry) = self.peers.get_mut(&peer) {
                    entry.incoming.insert(id, transfer);
                }
                tracing::debug!(%peer, %id, received, offset, "the sender is out of step");
                let _ = link.try_send(Frame::FileAck {
                    attachment: id,
                    received,
                    state: FileAckState::Receiving,
                });
                return;
            }
            Err(error) => {
                tracing::warn!(%error, %id, "writing a received chunk failed");
                if let Some(entry) = self.peers.get_mut(&peer) {
                    entry.incoming.insert(id, transfer);
                }
                self.fail_incoming(peer, id, FileCancelReason::Failed).await;
                return;
            }
        }

        let received = transfer.received;
        let report = received.saturating_sub(transfer.acked) >= FILE_ACK_EVERY_BYTES;
        if report {
            transfer.acked = received;
        }
        let mut progress = transfer.attachment.clone();
        progress.transferred = received;
        progress.state = AttachmentState::Receiving;
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.incoming.insert(id, transfer);
        }

        if report {
            self.persist_attachment_progress(id, received).await;
            self.emit_attachment(peer, progress);
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
        }
    }

    /// The sender says its last chunk is out and gives the digest of the whole file.
    pub(super) async fn on_file_done(&mut self, peer: DeviceId, id: AttachmentId, digest: Sha256) {
        let Some((link, row)) = self.accept_incoming_attachment(peer, id).await else {
            return;
        };

        let mut attachment = row;
        if attachment.state.is_finished() || attachment.state == AttachmentState::Offered {
            if let Some(entry) = self.peers.get_mut(&peer) {
                entry.incoming.remove(&id);
            }
            attachment.state = AttachmentState::Receiving;
            attachment.transferred = 0;
            if self
                .store
                .set_attachment_state(id, AttachmentState::Receiving)
                .await
                .is_err()
            {
                return;
            }
            self.persist_attachment_progress(id, 0).await;
            self.emit_attachment(peer, attachment.clone());
        }
        if !self.open_part_file(peer, &attachment).await {
            return;
        }

        let entry = match self.peers.get_mut(&peer) {
            Some(entry) => entry,
            None => return,
        };
        let Some(transfer) = entry.incoming.get_mut(&id) else {
            return;
        };
        if transfer.expected.is_some() {
            // Already being checked; a second announcement is a retransmission of the first.
            return;
        }
        if transfer.received != transfer.attachment.size {
            // Not everything arrived. Saying so is cheaper than hashing a file we know is short.
            let received = transfer.received;
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received,
                state: FileAckState::Receiving,
            });
            return;
        }

        // The whole file is on disk: read it back and check it. That is a whole-file read on a
        // blocking task, so the actor keeps answering while it happens.
        transfer.expected = Some(digest);
        transfer.release();
        let part = transfer.part.clone();
        let prep = self.prep.clone();
        tokio::task::spawn_blocking(move || {
            let digest = attachment::hash_file(&part).map_err(|error| error.to_string());
            let _ = prep.blocking_send(Prep::ReceiverDigest {
                peer,
                attachment: id,
                digest,
            });
        });
    }

    /// What the recipient is doing, in bytes.
    pub(super) async fn on_file_ack(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        received: u64,
        state: FileAckState,
    ) {
        let outgoing = self
            .peers
            .get(&peer)
            .and_then(|entry| entry.transfer.as_ref())
            .is_some_and(|transfer| transfer.id() == id);

        if state == FileAckState::Complete {
            if outgoing && let Some(entry) = self.peers.get_mut(&peer) {
                entry.transfer = None;
            }
            match self.store.attachment(id).await {
                Ok(Some(row)) => {
                    self.persist_attachment_progress(id, row.size).await;
                    self.persist_attachment_state(id, AttachmentState::Complete)
                        .await;
                    self.emit_attachment_row(peer, id).await;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, %id, "failed to read an attachment after a completion ack");
                }
            }
            self.maybe_start_transfer(peer).await;
            return;
        }

        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };
        let Some(transfer) = entry.transfer.as_mut() else {
            return;
        };
        if transfer.id() != id {
            return;
        }
        if received != transfer.sent {
            tracing::debug!(%peer, %id, received, sent = transfer.sent, "the recipient corrected the offset");
            transfer.rewind(received);
        } else {
            transfer.acked = transfer.acked.max(received);
            transfer.confirmed = transfer.confirmed.max(received);
        }
        let mut progress = transfer.attachment.clone();
        progress.transferred = transfer.sent;
        progress.state = AttachmentState::Sending;
        let sent = transfer.sent;
        self.persist_attachment_progress(id, sent).await;
        self.emit_attachment(peer, progress);
    }

    /// The other end has given up on this transfer, or refuses to start it.
    pub(super) async fn on_file_cancel(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        reason: FileCancelReason,
    ) {
        let state = match reason {
            FileCancelReason::Cancelled => AttachmentState::Cancelled,
            _ => AttachmentState::Failed,
        };
        tracing::info!(%peer, %id, ?reason, "a transfer was cancelled");
        if let Some(entry) = self.peers.get_mut(&peer) {
            if entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
            {
                entry.transfer = None;
            }
        }
        if let Some(transfer) = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.remove(&id))
        {
            Self::discard_transfer(transfer).await;
        }
        self.persist_attachment_state(id, state).await;
        self.persist_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
        self.maybe_start_transfer(peer).await;
    }

    /// "Send me this file": a recipient that has the metadata but not the bytes.
    pub(super) async fn on_file_request(&mut self, peer: DeviceId, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.peer != peer || row.direction != Direction::Outgoing {
            return;
        }
        // A complete file is not sent again; anything else — including a failed or cancelled
        // transfer — is revived, because asking again is how a retry works.
        if row.state == AttachmentState::Complete {
            return;
        }
        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        self.persist_attachment_state(id, AttachmentState::Queued)
            .await;
        self.emit_attachment_row(peer, id).await;
        self.maybe_start_transfer(peer).await;
    }

    /// Stops a transfer, from whichever side the user is looking at it.
    pub(super) async fn cancel_attachment(&mut self, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.state.is_finished() {
            return;
        }
        let peer = row.peer;
        if let Some(entry) = self.peers.get_mut(&peer) {
            if entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
            {
                entry.transfer = None;
            }
        }
        if let Some(transfer) = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.remove(&id))
        {
            Self::discard_transfer(transfer).await;
        }
        self.persist_attachment_state(id, AttachmentState::Cancelled)
            .await;
        self.persist_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
        self.refuse(peer, id, FileCancelReason::Cancelled);
        self.maybe_start_transfer(peer).await;
    }

    /// Starts a transfer again, from the side the user is on.
    pub(super) async fn retry_attachment(&mut self, id: AttachmentId) {
        let Ok(Some(row)) = self.store.attachment(id).await else {
            return;
        };
        if row.state == AttachmentState::Complete {
            return;
        }
        let peer = row.peer;

        if row.direction == Direction::Incoming {
            // Ask the sender to start again. Whatever was on disk is dropped first, because the
            // offset the sender resumes from is measured against a file that is now empty.
            if let Some(transfer) = self
                .peers
                .get_mut(&peer)
                .and_then(|entry| entry.incoming.remove(&id))
            {
                Self::discard_transfer(transfer).await;
            }
            self.persist_attachment_state(id, AttachmentState::Receiving)
                .await;
            self.persist_attachment_progress(id, 0).await;
            self.emit_attachment_row(peer, id).await;
            if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone())
                && let Err(error) = link.try_send(Frame::FileRequest { attachment: id })
            {
                tracing::debug!(%peer, %id, %error, "failed to ask for the file again");
            }
            return;
        }

        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        self.persist_attachment_state(id, AttachmentState::Queued)
            .await;
        self.emit_attachment_row(peer, id).await;

        // Announce it again: the recipient's row may be a terminal one, and only a fresh
        // `chat` frame tells it that this transfer is starting over.
        let messages = match self.store.messages_with_unfinished_attachments(peer).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, %peer, "failed to read unfinished attachments; not re-announcing");
                return;
            }
        };
        if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) {
            for message in messages
                .iter()
                .filter(|message| message.attachments.iter().any(|file| file.id == id))
            {
                if let Err(error) = link.try_send(Frame::Chat {
                    id: message.id,
                    text: message.body.clone(),
                    attachments: message.metas(),
                }) {
                    tracing::debug!(%peer, %error, "failed to re-announce a message with files");
                }
            }
        }
        self.maybe_start_transfer(peer).await;
    }

    /// The result of a whole-file read that could not be done on the actor.
    pub(super) async fn on_prepared(&mut self, prepared: Prep) {
        match prepared {
            Prep::SenderDigest {
                peer,
                attachment,
                digest,
            } => {
                let digest = match digest {
                    Ok(digest) => digest,
                    Err(reason) => {
                        self.fail_outgoing(peer, attachment, &reason).await;
                        self.maybe_start_transfer(peer).await;
                        return;
                    }
                };
                if self
                    .store
                    .set_attachment_digest(attachment, digest)
                    .await
                    .is_err()
                {
                    return;
                }
                let Some(entry) = self.peers.get_mut(&peer) else {
                    return;
                };
                let Some(transfer) = entry.transfer.as_mut() else {
                    return;
                };
                if transfer.id() != attachment {
                    return;
                }
                transfer.attachment.sha256 = Some(digest);
                transfer.attachment.state = AttachmentState::Sending;
                let progress = transfer.attachment.clone();
                self.persist_attachment_state(attachment, AttachmentState::Sending)
                    .await;
                self.emit_attachment(peer, progress);
            }
            Prep::ReceiverDigest {
                peer,
                attachment,
                digest,
            } => self.finish_incoming(peer, attachment, digest).await,
        }
    }

    /// The digest of what was received: keep the file or throw it away.
    pub(super) async fn finish_incoming(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        digest: Result<Sha256, String>,
    ) {
        let expected = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.get_mut(&id))
            .and_then(|transfer| transfer.expected.take());
        let Some(expected) = expected else {
            return;
        };

        let agreed = matches!(&digest, Ok(actual) if *actual == expected);
        if !agreed {
            let why = match digest {
                Ok(_) => "the digest does not match".to_owned(),
                Err(reason) => reason,
            };
            tracing::warn!(%peer, %id, why, "the received file did not verify");
            self.fail_incoming(peer, id, FileCancelReason::Checksum)
                .await;
            return;
        }

        let Some(entry) = self.peers.get_mut(&peer) else {
            return;
        };
        let Some(transfer) = entry.incoming.remove(&id) else {
            return;
        };
        let size = transfer.attachment.size;
        // Flushing and renaming a completed file is filesystem work on a file that may be large.
        let finished = offload(move || {
            let mut transfer = transfer;
            let result = transfer.finish();
            (transfer, result)
        })
        .await;
        let Some((transfer, result)) = finished else {
            self.store_fail_incoming(peer, id).await;
            return;
        };
        if let Err(error) = result {
            tracing::warn!(%error, %id, "could not move the received file into place");
            self.store_fail_incoming(peer, id).await;
            return;
        }
        let mut done = transfer.attachment.clone();
        done.state = AttachmentState::Complete;
        done.transferred = size;
        done.path = Some(transfer.final_path.to_string_lossy().into_owned());
        self.persist_attachment_progress(id, size).await;
        self.persist_attachment_state(id, AttachmentState::Complete)
            .await;
        self.emit_attachment(peer, done);
        if let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) {
            let _ = link.try_send(Frame::FileAck {
                attachment: id,
                received: size,
                state: FileAckState::Complete,
            });
        }
    }

    /// Marks an incoming attachment finished-but-failed and tells the sender.
    pub(super) async fn fail_incoming(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        reason: FileCancelReason,
    ) {
        if let Some(transfer) = self
            .peers
            .get_mut(&peer)
            .and_then(|entry| entry.incoming.remove(&id))
        {
            Self::discard_transfer(transfer).await;
        }
        self.store_fail_incoming(peer, id).await;
        self.refuse(peer, id, reason);
    }

    pub(super) async fn store_fail_incoming(&mut self, peer: DeviceId, id: AttachmentId) {
        self.persist_attachment_state(id, AttachmentState::Failed)
            .await;
        self.persist_attachment_progress(id, 0).await;
        self.emit_attachment_row(peer, id).await;
    }

    /// Gives up on an outgoing transfer: a missing source, an unreadable one, a failed write.
    ///
    /// Deliberately does not look for the next file — the caller decides whether to carry on, so
    /// the call graph stays finite.
    pub(super) async fn fail_outgoing(&mut self, peer: DeviceId, id: AttachmentId, reason: &str) {
        tracing::warn!(%peer, %id, reason, "the outgoing transfer failed");
        if let Some(entry) = self.peers.get_mut(&peer)
            && entry
                .transfer
                .as_ref()
                .is_some_and(|transfer| transfer.id() == id)
        {
            entry.transfer = None;
        }
        self.persist_attachment_state(id, AttachmentState::Failed)
            .await;
        self.emit_attachment_row(peer, id).await;
        self.refuse(peer, id, FileCancelReason::Failed);
    }

    pub(super) async fn set_attachment_state(
        &mut self,
        peer: DeviceId,
        id: AttachmentId,
        state: AttachmentState,
    ) {
        self.persist_attachment_state(id, state).await;
        self.emit_attachment_row(peer, id).await;
    }

    /// Persists an attachment state, logging a failure instead of dropping it silently: a row
    /// the interface updates but the database does not is a bug that would otherwise be invisible.
    pub(super) async fn persist_attachment_state(
        &mut self,
        id: AttachmentId,
        state: AttachmentState,
    ) {
        if let Err(error) = self.store.set_attachment_state(id, state).await {
            tracing::warn!(%error, %id, "failed to persist the attachment state");
        }
    }

    /// Persists how much of a transfer has arrived or been sent.
    pub(super) async fn persist_attachment_progress(&mut self, id: AttachmentId, transferred: u64) {
        if let Err(error) = self.store.set_attachment_progress(id, transferred).await {
            tracing::warn!(%error, %id, transferred, "failed to persist the attachment progress");
        }
    }

    /// Tells the peer a transfer is over. Best effort: the socket may already be gone.
    pub(super) fn refuse(&self, peer: DeviceId, id: AttachmentId, reason: FileCancelReason) {
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        if let Err(error) = link.try_send(Frame::FileCancel {
            attachment: id,
            reason,
        }) {
            tracing::debug!(%peer, %id, %error, "failed to refuse an attachment");
        }
    }

    /// Starts sending this peer's oldest unfinished attachment, if it is not already busy.
    ///
    /// Only a message that has been announced can be streamed: a `chat` frame still waiting in
    /// the outbox means the recipient has no idea what these chunks belong to.
    pub(super) async fn maybe_start_transfer(&mut self, peer: DeviceId) {
        let Some(entry) = self.peers.get(&peer) else {
            return;
        };
        if entry.transfer.is_some() || entry.link.is_none() {
            return;
        }

        // One query, then a local walk: a file that cannot be opened is marked failed and the
        // next candidate is tried, instead of re-reading the whole list for every failure.
        let messages = match self.store.messages_with_unfinished_attachments(peer).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, "failed to read the unfinished attachments");
                return;
            }
        };
        let candidates: Vec<Attachment> = messages
            .iter()
            .flat_map(|message| message.attachments.iter())
            .filter(|attachment| {
                attachment.direction == Direction::Outgoing && attachment.state.is_in_flight()
            })
            .cloned()
            .collect();

        for attachment in candidates {
            let Some(entry) = self.peers.get(&peer) else {
                return;
            };
            if entry.transfer.is_some() || entry.link.is_none() {
                return;
            }

            let Some(path) = attachment.path.clone().map(PathBuf::from) else {
                self.fail_outgoing(peer, attachment.id, "the source of the file is not known")
                    .await;
                continue;
            };

            let needs_digest = attachment.sha256.is_none();
            let mut transfer = OutgoingTransfer::new(attachment.clone(), path.clone());
            if !needs_digest {
                transfer.attachment.state = AttachmentState::Sending;
            }
            let Some(entry) = self.peers.get_mut(&peer) else {
                return;
            };
            if entry.transfer.is_some() {
                return;
            }
            entry.transfer = Some(transfer);

            if !needs_digest {
                self.persist_attachment_state(attachment.id, AttachmentState::Sending)
                    .await;
                self.emit_attachment(peer, attachment);
                return;
            }

            // The digest has to exist before the first chunk, because the recipient checks the
            // file it assembled against it; reading a large file is a blocking task of its own.
            let prep = self.prep.clone();
            let id = attachment.id;
            tokio::task::spawn_blocking(move || {
                let digest = attachment::hash_file(&path).map_err(|error| error.to_string());
                let _ = prep.blocking_send(Prep::SenderDigest {
                    peer,
                    attachment: id,
                    digest,
                });
            });
            return;
        }
    }

    /// Re-offers every unfinished attachment of a peer that has just come back.
    ///
    /// The metadata is repeated because the recipient's copy of it may be gone (a cleared
    /// history, a reinstalled application) while ours says the message was delivered; the
    /// chunks then resume from wherever the recipient's file ends, which the first
    /// acknowledgement corrects.
    pub(super) async fn resume_attachments(&mut self, peer: DeviceId) {
        let messages = match self.store.messages_with_unfinished_attachments(peer).await {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, "failed to read the unfinished attachments");
                return;
            }
        };
        let Some(link) = self.peers.get(&peer).and_then(|entry| entry.link.clone()) else {
            return;
        };
        for message in &messages {
            // A message still in the outbox is announced by the drain itself, in order.
            if message.status == MessageStatus::Queued {
                continue;
            }
            if let Err(error) = link.try_send(Frame::Chat {
                id: message.id,
                text: message.body.clone(),
                attachments: message.metas(),
            }) {
                tracing::debug!(%peer, %error, "failed to re-announce a message with files");
                break;
            }
        }
        self.maybe_start_transfer(peer).await;
    }

    /// When the next chunk may be written, or `None` when nothing is waiting.
    pub(super) fn transfer_deadline(&self) -> Option<tokio::time::Instant> {
        let waiting = self.peers.values().any(|entry| {
            entry.link.is_some()
                && entry
                    .transfer
                    .as_ref()
                    .is_some_and(OutgoingTransfer::wants_chunk)
        });
        waiting.then(|| tokio::time::Instant::from_std(Instant::now() + TRANSFER_POLL))
    }

    /// Writes what the pacing and the send queue allow, for every peer at once.
    ///
    /// Bounded per turn: a transfer is a background job, and a message arriving on another
    /// connection must not wait for a whole file to be written out.
    pub(super) async fn pump_transfers(&mut self) {
        let busy: Vec<DeviceId> = self
            .peers
            .iter()
            .filter(|(_, entry)| {
                entry.link.is_some()
                    && entry
                        .transfer
                        .as_ref()
                        .is_some_and(OutgoingTransfer::wants_chunk)
            })
            .map(|(device_id, _)| *device_id)
            .collect();

        for peer in busy {
            let mut failure: Option<String> = None;
            for _ in 0..TRANSFER_CHUNKS_PER_TURN {
                match self.pump_one_chunk(peer).await {
                    PumpOutcome::Sent | PumpOutcome::Idle => continue,
                    PumpOutcome::Halt | PumpOutcome::Finished => break,
                    PumpOutcome::Failed(reason) => {
                        failure = Some(reason);
                        break;
                    }
                }
            }

            if let Some(reason) = failure {
                let id = self
                    .peers
                    .get(&peer)
                    .and_then(|entry| entry.transfer.as_ref())
                    .map(OutgoingTransfer::id);
                if let Some(id) = id {
                    self.fail_outgoing(peer, id, &reason).await;
                    self.maybe_start_transfer(peer).await;
                }
            }
        }
    }

    /// Reads and queues one chunk for `peer`, leaving the actor while the disk read happens.
    pub(super) async fn pump_one_chunk(&mut self, peer: DeviceId) -> PumpOutcome {
        let now = Instant::now();
        let Some(entry) = self.peers.get_mut(&peer) else {
            return PumpOutcome::Halt;
        };
        let Some(link) = entry.link.clone() else {
            return PumpOutcome::Halt;
        };
        let Some(transfer) = entry.transfer.as_ref() else {
            return PumpOutcome::Halt;
        };
        if !transfer.wants_chunk() {
            return PumpOutcome::Halt;
        }
        let complete = transfer.is_complete();
        let digest = transfer.attachment.sha256;
        let id = transfer.id();

        // Charged at the full chunk size: the read that follows is at most that big, and a
        // budget that under-charges is a budget that gets refused by the recipient.
        if !entry
            .file_budget
            .try_acquire_n(FILE_CHUNK_BYTES as f64, now)
        {
            return PumpOutcome::Halt;
        }

        if complete {
            let Some(sha256) = digest else {
                return PumpOutcome::Halt;
            };
            let Some(mut transfer) = entry.transfer.take() else {
                return PumpOutcome::Halt;
            };
            transfer.finished = true;
            entry.transfer = Some(transfer);
            if let Err(error) = link.try_send(Frame::FileDone {
                attachment: id,
                sha256,
            }) {
                tracing::debug!(%peer, %error, "could not announce a finished file");
            }
            return PumpOutcome::Finished;
        }

        let Some(mut transfer) = entry.transfer.take() else {
            return PumpOutcome::Halt;
        };
        let Some((transfer, result)) = offload(move || {
            let result = transfer.read_chunk();
            (transfer, result)
        })
        .await
        else {
            return PumpOutcome::Failed("the file reader task failed".to_owned());
        };
        let transfer = transfer;

        let outcome = match result {
            Ok(Some(data)) => {
                let offset = transfer.sent - u64::try_from(data.len()).unwrap_or(0);
                if let Err(error) = link.try_send(Frame::FileChunk {
                    attachment: id,
                    offset,
                    data,
                }) {
                    tracing::debug!(%peer, %error, "could not queue a file chunk");
                    PumpOutcome::Halt
                } else {
                    PumpOutcome::Sent
                }
            }
            Ok(None) => PumpOutcome::Idle,
            Err(error) => PumpOutcome::Failed(error.to_string()),
        };

        // The transfer lives in the entry between steps, so a disconnect or a failure sees it.
        if let Some(entry) = self.peers.get_mut(&peer) {
            entry.transfer = Some(transfer);
        }
        outcome
    }
}
