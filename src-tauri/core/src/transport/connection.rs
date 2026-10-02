//! Handshakes and the per-connection loop.
//!
//! One task per connection owns both halves of the socket, which is what makes the write
//! order well defined without any locking.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, timeout};

use crate::domain::ids::DeviceId;
use crate::domain::peer::Handshake;
use crate::error::{ProtocolError, TransportError};
use crate::protocol::limits::{
    CONNECT_TIMEOUT, DATA_BURST_BYTES, DATA_RATE_PER_SECOND, FILE_CHUNK_BURST,
    FILE_CHUNK_RATE_PER_SECOND, HANDSHAKE_TIMEOUT, HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT,
    RATE_LIMIT_BURST, RATE_LIMIT_PER_SECOND,
};
use crate::protocol::{ErrorCode, Frame, GoodbyeReason, TokenBucket};

use super::{
    DisconnectReason, FrameReader, FrameWriter, LinkCommand, LiveConnectionGuard, LiveConnections,
    PeerLink, Role, TransportEvent,
};

#[derive(Debug)]
pub struct ConnectionContext {
    pub events: mpsc::Sender<TransportEvent>,
    /// Shared with the listener so the limit is global.
    pub live: LiveConnections,
    pub max_peers: usize,
}

impl ConnectionContext {
    #[must_use]
    pub fn new(events: mpsc::Sender<TransportEvent>, max_peers: usize) -> Self {
        Self {
            events,
            live: LiveConnections::new(),
            max_peers,
        }
    }
}

#[derive(Debug)]
pub struct Handshaken {
    pub role: Role,
    /// The peer's announced identity, taken from the handshake and not from discovery.
    pub peer: Handshake,
    pub reader: FrameReader,
    pub writer: FrameWriter,
    /// Keeps the connection slot reserved for as long as the connection lives.
    pub guard: LiveConnectionGuard,
    /// Kept because a peer multi-homed on several interfaces may be reachable on only one of
    /// them, and remembering which one worked makes the next dial immediate.
    pub remote: SocketAddr,
}

/// Opens a connection to `address` and performs the dialer side of the handshake.
pub async fn dial(
    address: SocketAddr,
    own: &Handshake,
    context: &ConnectionContext,
    expected: Option<DeviceId>,
) -> Result<Handshaken, TransportError> {
    let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| TransportError::HandshakeTimeout)??;
    stream.set_nodelay(true)?;

    let (read, write) = stream.into_split();
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    writer.send(&Frame::Hello(own.clone())).await?;

    let greeting = timeout(HANDSHAKE_TIMEOUT, reader.next())
        .await
        .map_err(|_| TransportError::HandshakeTimeout)??;

    let peer = match greeting {
        Some(Frame::Welcome(peer)) => peer,
        Some(Frame::Error { code, message }) => {
            return Err(TransportError::Failed(format!(
                "peer refused the connection: {} ({message})",
                code.as_str()
            )));
        }
        Some(other) => {
            return Err(TransportError::UnexpectedFrame { got: other.kind() });
        }
        None => return Err(TransportError::HandshakeClosed),
    };

    if peer.device_id == own.device_id {
        return Err(TransportError::SelfConnection);
    }
    if let Some(expected) = expected
        && peer.device_id != expected
    {
        return Err(TransportError::IdentityMismatch {
            expected: expected.to_string(),
            got: peer.device_id.to_string(),
        });
    }

    let guard = context
        .live
        .try_reserve(context.max_peers)
        .ok_or(TransportError::PeerLimit {
            max: context.max_peers,
        })?;

    tracing::debug!(peer = %peer.device_id, %address, "outbound connection established");
    Ok(Handshaken {
        role: Role::Dialer,
        peer,
        reader,
        writer,
        guard,
        remote: address,
    })
}

/// Completes the acceptor side of the handshake on an already-accepted socket.
///
/// # Errors
///
/// Returns [`TransportError`] when the peer failed the handshake. Every refusal that the peer
/// could act on is answered with an [`Frame::Error`] first, so the other side sees a reason
/// rather than a dropped socket.
pub async fn accept(
    stream: TcpStream,
    own: &Handshake,
    context: &ConnectionContext,
) -> Result<Handshaken, TransportError> {
    stream.set_nodelay(true)?;
    let remote = stream.peer_addr()?;

    let (read, write) = stream.into_split();
    let mut reader = FrameReader::new(read);
    let mut writer = FrameWriter::new(write);

    let greeting = match timeout(HANDSHAKE_TIMEOUT, reader.next()).await {
        Ok(Ok(greeting)) => greeting,
        Ok(Err(error)) => {
            return Err(refuse(&mut writer, &error).await);
        }
        Err(_) => {
            refuse_with(
                &mut writer,
                ErrorCode::HandshakeTimeout,
                "no handshake within the deadline",
            )
            .await;
            return Err(TransportError::HandshakeTimeout);
        }
    };

    let peer = match greeting {
        Some(Frame::Hello(peer)) => peer,
        Some(other) => {
            refuse_with(
                &mut writer,
                ErrorCode::Malformed,
                &format!("expected hello, got {}", other.kind()),
            )
            .await;
            return Err(TransportError::UnexpectedFrame { got: other.kind() });
        }
        None => return Err(TransportError::HandshakeClosed),
    };

    if peer.device_id == own.device_id {
        tracing::warn!(peer = %peer.device_id, "rejecting a connection from our own device id");
        refuse_with(
            &mut writer,
            ErrorCode::SelfConnection,
            "device id collides with ours",
        )
        .await;
        return Err(TransportError::SelfConnection);
    }

    let Some(guard) = context.live.try_reserve(context.max_peers) else {
        tracing::warn!(
            max = context.max_peers,
            "refusing a connection: peer limit reached"
        );
        refuse_with(
            &mut writer,
            ErrorCode::PeerLimit,
            "this instance already has as many peers as it accepts",
        )
        .await;
        return Err(TransportError::PeerLimit {
            max: context.max_peers,
        });
    };

    writer.send(&Frame::Welcome(own.clone())).await?;

    tracing::debug!(peer = %peer.device_id, %remote, "inbound connection established");
    Ok(Handshaken {
        role: Role::Acceptor,
        peer,
        reader,
        writer,
        guard,
        remote,
    })
}

/// Runs a handshaken connection until it ends, reporting everything through the context.
pub async fn serve(handshaken: Handshaken, context: Arc<ConnectionContext>) {
    let Handshaken {
        role,
        peer,
        mut reader,
        mut writer,
        guard,
        remote,
    } = handshaken;
    let peer_id = peer.device_id;

    let (link, mut commands) = PeerLink::channel();
    let link_id = link.id();
    if context
        .events
        .send(TransportEvent::Connected {
            role,
            handshake: peer,
            link,
            remote,
        })
        .await
        .is_err()
    {
        tracing::debug!(peer = %peer_id, "session is gone; closing the new connection");
        return;
    }

    let reason = pump(&mut reader, &mut writer, &mut commands, &context, peer_id).await;
    drop(guard);

    if context
        .events
        .send(TransportEvent::Disconnected {
            peer: peer_id,
            link: link_id,
            reason: reason.clone(),
        })
        .await
        .is_err()
    {
        tracing::debug!(peer = %peer_id, "session is gone; disconnect not reported");
    }
    tracing::debug!(peer = %peer_id, role = role.as_str(), ?reason, "connection ended");
}

/// Reads, writes and times out until the connection is over.
async fn pump(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    commands: &mut mpsc::Receiver<LinkCommand>,
    context: &ConnectionContext,
    peer: DeviceId,
) -> DisconnectReason {
    // Three budgets, because they bound three different resources: frames of content (a chat
    // message), bytes of file payload (a transfer), and the number of chunk frames those bytes
    // are cut into.
    let mut rate = TokenBucket::new(RATE_LIMIT_BURST, RATE_LIMIT_PER_SECOND, Instant::now());
    let mut chunk_rate =
        TokenBucket::new(FILE_CHUNK_BURST, FILE_CHUNK_RATE_PER_SECOND, Instant::now());
    let mut bulk = TokenBucket::new(DATA_BURST_BYTES, DATA_RATE_PER_SECOND, Instant::now());
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    // A delayed runtime must not make the connection send a burst of catch-up heartbeats.
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    heartbeat.tick().await;

    let mut sequence: u64 = 0;
    let mut last_frame = Instant::now();

    loop {
        tokio::select! {
            frame = reader.next() => {
                match frame {
                    Ok(Some(frame)) => {
                        last_frame = Instant::now();
                        match handle_frame(frame, writer, peer, &mut rate, &mut chunk_rate, &mut bulk).await {
                            FrameOutcome::Forward(frame) => {
                                if context
                                    .events
                                    .send(TransportEvent::Frame { peer, frame })
                                    .await
                                    .is_err()
                                {
                                    return DisconnectReason::Closed;
                                }
                            }
                            FrameOutcome::Close(reason) => return reason,
                        }
                    }
                    Ok(None) => return DisconnectReason::Closed,
                    Err(error) => {
                        report_protocol_error(writer, &error).await;
                        return DisconnectReason::Failed(error.to_string());
                    }
                }
            }

            _ = heartbeat.tick() => {
                sequence = sequence.wrapping_add(1);
                if writer.send(&Frame::Heartbeat { seq: sequence }).await.is_err() {
                    return DisconnectReason::Closed;
                }
                // A socket can stay open forever while the peer behind it is gone — a
                // suspended laptop, an expired NAT rule. Silence is the only signal.
                if last_frame.elapsed() > HEARTBEAT_TIMEOUT {
                    return DisconnectReason::Stalled;
                }
            }

            command = commands.recv() => {
                match command {
                    Some(LinkCommand::Send(frame)) => {
                        if writer.send(&frame).await.is_err() {
                            return DisconnectReason::Closed;
                        }
                    }
                    Some(LinkCommand::Close(reason)) => {
                        let _ = writer.send(&Frame::Goodbye { reason }).await;
                        writer.finish().await;
                        return DisconnectReason::LocalGoodbye(reason);
                    }
                    None => {
                        // Every handle was dropped: the session no longer wants this
                        // connection. Say goodbye so the peer goes offline at once instead of
                        // waiting for the heartbeat timeout.
                        let _ = writer
                            .send(&Frame::Goodbye {
                                reason: GoodbyeReason::Shutdown,
                            })
                            .await;
                        writer.finish().await;
                        return DisconnectReason::LocalGoodbye(GoodbyeReason::Shutdown);
                    }
                }
            }
        }
    }
}

/// What the connection should do with an inbound frame.
///
/// There is no "handled and forgotten" arm on purpose: every frame is either content the session
/// must see — including a liveness beat, which the presence machine measures — or a reason to
/// close.
enum FrameOutcome {
    Forward(Frame),
    Close(DisconnectReason),
}

async fn handle_frame(
    frame: Frame,
    writer: &mut FrameWriter,
    peer: DeviceId,
    rate: &mut TokenBucket,
    chunk_rate: &mut TokenBucket,
    bulk: &mut TokenBucket,
) -> FrameOutcome {
    // File payload is charged in bytes rather than in frames: a 40 MiB file is a thousand
    // frames and would trip the per-frame budget on its first second. The chunk count is
    // bounded alongside it so that budget cannot be spent on millions of one-byte frames.
    if let Frame::FileChunk { data, .. } = &frame {
        let now = Instant::now();
        let bytes = u64::try_from(data.len()).unwrap_or(u64::MAX) as f64;
        if !bulk.try_acquire_n(bytes, now) || !chunk_rate.try_acquire(now) {
            tracing::warn!(%peer, "peer exceeded the inbound file budget");
            let _ = writer
                .send(&Frame::error(
                    ErrorCode::RateLimited,
                    "too much file data per second",
                ))
                .await;
            return FrameOutcome::Close(DisconnectReason::Failed(
                "peer exceeded the file budget".to_owned(),
            ));
        }
        return FrameOutcome::Forward(frame);
    }

    match frame {
        // Liveness is the session's business as much as the connection's: the presence machine
        // measures silence, and it can only do that if the beats reach it. They are exempt from
        // the rate limit, because a peer's five-second cadence must never be the thing that
        // trips it.
        Frame::Heartbeat { .. } => FrameOutcome::Forward(frame),

        Frame::Goodbye { reason } => {
            tracing::debug!(%peer, ?reason, "peer said goodbye");
            FrameOutcome::Close(DisconnectReason::Goodbye(reason))
        }

        Frame::Error { code, message } => {
            tracing::warn!(%peer, code = code.as_str(), %message, "peer reported a protocol error");
            FrameOutcome::Close(DisconnectReason::Failed(format!(
                "peer error {}",
                code.as_str()
            )))
        }

        content => {
            if !rate.try_acquire(Instant::now()) {
                tracing::warn!(%peer, "peer exceeded the inbound rate limit");
                let _ = writer
                    .send(&Frame::error(
                        ErrorCode::RateLimited,
                        "too many frames per second",
                    ))
                    .await;
                return FrameOutcome::Close(DisconnectReason::Failed(
                    "peer exceeded the rate limit".to_owned(),
                ));
            }
            FrameOutcome::Forward(content)
        }
    }
}

async fn report_protocol_error(writer: &mut FrameWriter, error: &TransportError) {
    let (code, message) = match error {
        TransportError::Protocol(ProtocolError::VersionMismatch { got, supported }) => (
            ErrorCode::VersionMismatch,
            format!("this instance speaks protocol version {supported}, you announced {got}"),
        ),
        TransportError::Protocol(ProtocolError::FrameSize { len, max }) => (
            ErrorCode::Malformed,
            format!("frame length {len} is outside the accepted range 1..={max}"),
        ),
        TransportError::Protocol(ProtocolError::Domain(_))
        | TransportError::Protocol(ProtocolError::Malformed(_)) => (
            ErrorCode::Malformed,
            "the frame could not be decoded".to_owned(),
        ),
        TransportError::TruncatedFrame { buffered } => (
            ErrorCode::Malformed,
            format!("the connection ended inside a frame with {buffered} bytes buffered"),
        ),
        _ => (ErrorCode::Internal, "the connection failed".to_owned()),
    };
    let _ = writer.send(&Frame::error(code, message)).await;
    writer.finish().await;
}

async fn refuse(writer: &mut FrameWriter, error: &TransportError) -> TransportError {
    let (code, message) = match error {
        TransportError::Protocol(ProtocolError::VersionMismatch { got, supported }) => (
            ErrorCode::VersionMismatch,
            format!("this instance speaks protocol version {supported}, you announced {got}"),
        ),
        _ => (
            ErrorCode::Malformed,
            "the handshake could not be decoded".to_owned(),
        ),
    };
    refuse_with(writer, code, &message).await;
    TransportError::Failed(error.to_string())
}

async fn refuse_with(writer: &mut FrameWriter, code: ErrorCode, message: &str) {
    let _ = writer.send(&Frame::error(code, message)).await;
    writer.finish().await;
}
