//! Handshakes and the per-connection loop.
//!
//! One task per connection owns both halves of the socket, which is what makes the write
//! order well defined without any locking.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

    let reason = pump(
        &mut reader,
        &mut writer,
        &mut commands,
        &context,
        peer_id,
        PumpTiming::default(),
    )
    .await;
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

/// Timing knobs for [`pump`]. Production always uses the protocol constants; the struct exists
/// so the stall path can be exercised without waiting out the real fifteen-second timeout.
#[derive(Debug, Clone, Copy)]
struct PumpTiming {
    heartbeat: Duration,
    stall_after: Duration,
}

impl Default for PumpTiming {
    fn default() -> Self {
        Self {
            heartbeat: HEARTBEAT_INTERVAL,
            stall_after: HEARTBEAT_TIMEOUT,
        }
    }
}

/// Reads, writes and times out until the connection is over.
async fn pump(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    commands: &mut mpsc::Receiver<LinkCommand>,
    context: &ConnectionContext,
    peer: DeviceId,
    timing: PumpTiming,
) -> DisconnectReason {
    // Three budgets, because they bound three different resources: frames of content (a chat
    // message), bytes of file payload (a transfer), and the number of chunk frames those bytes
    // are cut into.
    let mut rate = TokenBucket::new(RATE_LIMIT_BURST, RATE_LIMIT_PER_SECOND, Instant::now());
    let mut chunk_rate =
        TokenBucket::new(FILE_CHUNK_BURST, FILE_CHUNK_RATE_PER_SECOND, Instant::now());
    let mut bulk = TokenBucket::new(DATA_BURST_BYTES, DATA_RATE_PER_SECOND, Instant::now());
    let mut heartbeat = tokio::time::interval(timing.heartbeat);
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
                if last_frame.elapsed() > timing.stall_after {
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::future::Future;

    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::tcp::OwnedWriteHalf;

    use crate::domain::ids::{AttachmentId, MessageId};
    use crate::domain::nickname::Nickname;
    use crate::domain::peer::PeerProfile;
    use crate::protocol::encode;
    use crate::protocol::limits::{MAX_FRAME_BYTES, PROTOCOL_VERSION};

    fn device(value: u128) -> DeviceId {
        DeviceId::from_uuid(uuid::Uuid::from_u128(value))
    }

    fn handshake(value: u128, name: &str) -> Handshake {
        let profile = PeerProfile::new(device(value), Nickname::parse(name).expect("nickname"));
        Handshake::new(PROTOCOL_VERSION, &profile, 0)
    }

    fn context(
        capacity: usize,
        max_peers: usize,
    ) -> (ConnectionContext, mpsc::Receiver<TransportEvent>) {
        let (events, receiver) = mpsc::channel(capacity);
        (ConnectionContext::new(events, max_peers), receiver)
    }

    fn buckets() -> (TokenBucket, TokenBucket, TokenBucket) {
        (
            TokenBucket::new(RATE_LIMIT_BURST, RATE_LIMIT_PER_SECOND, Instant::now()),
            TokenBucket::new(FILE_CHUNK_BURST, FILE_CHUNK_RATE_PER_SECOND, Instant::now()),
            TokenBucket::new(DATA_BURST_BYTES, DATA_RATE_PER_SECOND, Instant::now()),
        )
    }

    fn length_prefixed(payload: &[u8]) -> Vec<u8> {
        let mut bytes = u32::try_from(payload.len())
            .expect("payload fits a u32")
            .to_be_bytes()
            .to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    /// The raw client side of a loopback socket: enough to send any bytes and read frames back.
    struct Client {
        reader: FrameReader,
        write: OwnedWriteHalf,
    }

    impl Client {
        async fn send(&mut self, frame: &Frame) {
            let bytes = encode(frame).expect("encode");
            self.write.write_all(&bytes).await.expect("client write");
        }

        async fn send_raw(&mut self, bytes: &[u8]) {
            self.write.write_all(bytes).await.expect("client write");
        }

        async fn half_close(&mut self) {
            self.write.shutdown().await.expect("client shutdown");
        }

        async fn next(&mut self) -> Option<Frame> {
            self.reader.next().await.expect("client read")
        }
    }

    async fn raw_pair() -> (TcpStream, Client) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let client = TcpStream::connect(address).await.expect("connect");
        let (server, _) = listener.accept().await.expect("accept");
        let (read, write) = client.into_split();
        (
            server,
            Client {
                reader: FrameReader::new(read),
                write,
            },
        )
    }

    /// Our side of a loopback connection, already split for `handle_frame` and `pump`.
    async fn endpoint_pair() -> (FrameReader, FrameWriter, Client) {
        let (server, client) = raw_pair().await;
        let (read, write) = server.into_split();
        (FrameReader::new(read), FrameWriter::new(write), client)
    }

    /// A handshaken dialer/acceptor pair plus the dialer's context and event stream.
    async fn handshaken_pair(
        max_peers: usize,
    ) -> (
        Handshaken,
        Handshaken,
        Arc<ConnectionContext>,
        mpsc::Receiver<TransportEvent>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let own_dialer = handshake(1, "Alice");
        let own_acceptor = handshake(2, "Bob");
        let acceptor_ctx = Arc::new(ConnectionContext::new(mpsc::channel(4).0, max_peers));
        let acceptor_ctx_task = Arc::clone(&acceptor_ctx);
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            accept(stream, &own_acceptor, &acceptor_ctx_task).await
        });

        let (dialer_ctx, dialer_events) = context(16, max_peers);
        let dialer_ctx = Arc::new(dialer_ctx);
        let dialed = dial(address, &own_dialer, &dialer_ctx, Some(device(2)))
            .await
            .expect("dial");
        let accepted = task.await.expect("accept task").expect("accept");
        (dialed, accepted, dialer_ctx, dialer_events)
    }

    async fn run_pump<F>(future: F) -> DisconnectReason
    where
        F: Future<Output = DisconnectReason>,
    {
        timeout(Duration::from_secs(5), future)
            .await
            .expect("the pump must end on its own")
    }

    fn ack(value: u128) -> Frame {
        Frame::ChatAck {
            id: MessageId::from_uuid(uuid::Uuid::from_u128(value)),
        }
    }

    // ---- happy path -----------------------------------------------------------------------

    #[tokio::test]
    async fn a_dial_and_an_accept_agree_on_both_identities() {
        let (dialed, accepted, dialer_ctx, _events) = handshaken_pair(8).await;

        assert_eq!(dialed.role, Role::Dialer);
        assert_eq!(dialed.peer, handshake(2, "Bob"));
        assert_eq!(dialed.remote.ip().to_string(), "127.0.0.1");
        assert!(dialed.remote.port() > 0);
        assert_eq!(dialer_ctx.live.count(), 1, "the dial reserved a slot");

        assert_eq!(accepted.role, Role::Acceptor);
        assert_eq!(accepted.peer, handshake(1, "Alice"));
        assert!(accepted.remote.port() > 0, "the peer address is reported");
    }

    // ---- acceptor refusals, each preceded by a Frame::Error --------------------------------

    #[tokio::test]
    async fn a_hello_from_another_version_is_refused_with_version_mismatch() {
        let (server, mut client) = raw_pair().await;
        let (ctx, _events) = context(4, 8);
        // A heartbeat-shaped envelope that announces a version we do not speak: the reader
        // rejects it on the version before it ever decodes the frame.
        client
            .send_raw(&length_prefixed(br#"{"v":99,"t":"heartbeat","seq":1}"#))
            .await;

        let result = accept(server, &handshake(2, "Bob"), &ctx).await;
        assert!(matches!(result, Err(TransportError::Failed(_))));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::VersionMismatch,
                ..
            })
        ));
        assert_eq!(ctx.live.count(), 0);
    }

    #[tokio::test]
    async fn a_first_frame_that_is_not_a_hello_is_refused_as_malformed() {
        let (server, mut client) = raw_pair().await;
        let (ctx, _events) = context(4, 8);
        client.send(&Frame::Heartbeat { seq: 1 }).await;

        let result = accept(server, &handshake(2, "Bob"), &ctx).await;
        assert!(matches!(
            result,
            Err(TransportError::UnexpectedFrame { got: "heartbeat" })
        ));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::Malformed,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_peer_that_leaves_before_the_hello_is_reported_as_closed() {
        let (server, mut client) = raw_pair().await;
        let (ctx, _events) = context(4, 8);
        client.half_close().await;

        let result = accept(server, &handshake(2, "Bob"), &ctx).await;
        assert!(matches!(result, Err(TransportError::HandshakeClosed)));
        assert_eq!(ctx.live.count(), 0);
    }

    #[tokio::test]
    async fn a_silent_peer_is_refused_with_a_handshake_timeout() {
        tokio::time::pause();
        let (server, mut client) = raw_pair().await;
        let (ctx, _events) = context(4, 8);

        let result = accept(server, &handshake(2, "Bob"), &ctx).await;
        assert!(matches!(result, Err(TransportError::HandshakeTimeout)));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::HandshakeTimeout,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_hello_carrying_our_own_device_id_is_refused() {
        let (server, mut client) = raw_pair().await;
        let (ctx, _events) = context(4, 8);
        let own = handshake(2, "Bob");
        client.send(&Frame::Hello(own.clone())).await;

        let result = accept(server, &own, &ctx).await;
        assert!(matches!(result, Err(TransportError::SelfConnection)));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::SelfConnection,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_peer_beyond_the_limit_is_refused_before_the_welcome() {
        let (ctx, _events) = context(4, 1);
        // Occupy the single slot, as an existing connection would.
        let held = ctx.live.try_reserve(1).expect("the only slot");

        let (server, mut client) = raw_pair().await;
        client.send(&Frame::Hello(handshake(3, "Carol"))).await;

        let result = accept(server, &handshake(2, "Bob"), &ctx).await;
        assert!(matches!(result, Err(TransportError::PeerLimit { max: 1 })));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::PeerLimit,
                ..
            })
        ));
        drop(held);
    }

    // ---- dialer-side failures --------------------------------------------------------------

    async fn dial_against<F, Fut>(
        own: &Handshake,
        context: &ConnectionContext,
        expected: Option<DeviceId>,
        server: F,
    ) -> Result<Handshaken, TransportError>
    where
        F: FnOnce(FrameReader, FrameWriter) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, write) = stream.into_split();
            server(FrameReader::new(read), FrameWriter::new(write)).await;
        });
        let result = dial(address, own, context, expected).await;
        task.await.expect("server task");
        result
    }

    #[tokio::test]
    async fn a_refusal_from_the_peer_reaches_the_dialer_as_a_failure() {
        let (ctx, _events) = context(4, 8);
        let result = dial_against(
            &handshake(1, "Alice"),
            &ctx,
            None,
            |_reader, mut writer| async move {
                writer
                    .send(&Frame::error(ErrorCode::PeerLimit, "full"))
                    .await
                    .expect("send");
                writer.finish().await;
            },
        )
        .await;

        match result {
            Err(TransportError::Failed(message)) => {
                assert!(
                    message.contains("peer_limit"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(ctx.live.count(), 0);
    }

    #[tokio::test]
    async fn a_non_welcome_reply_is_an_unexpected_frame() {
        let (ctx, _events) = context(4, 8);
        let result = dial_against(
            &handshake(1, "Alice"),
            &ctx,
            None,
            |_reader, mut writer| async move {
                writer
                    .send(&Frame::Heartbeat { seq: 1 })
                    .await
                    .expect("send");
            },
        )
        .await;

        assert!(matches!(
            result,
            Err(TransportError::UnexpectedFrame { got: "heartbeat" })
        ));
    }

    #[tokio::test]
    async fn a_peer_that_hangs_up_during_the_handshake_is_reported_as_closed() {
        let (ctx, _events) = context(4, 8);
        let result = dial_against(
            &handshake(1, "Alice"),
            &ctx,
            None,
            |mut reader, writer| async move {
                // Read the hello, then hang up without answering it.
                let _ = reader.next().await;
                drop(writer);
            },
        )
        .await;

        assert!(matches!(result, Err(TransportError::HandshakeClosed)));
    }

    #[tokio::test]
    async fn a_welcome_with_our_own_device_id_is_a_self_connection() {
        let (ctx, _events) = context(4, 8);
        let own = handshake(4, "Dana");
        let welcome = own.clone();
        let result = dial_against(&own, &ctx, None, move |_reader, mut writer| async move {
            writer.send(&Frame::Welcome(welcome)).await.expect("send");
        })
        .await;

        assert!(matches!(result, Err(TransportError::SelfConnection)));
    }

    #[tokio::test]
    async fn a_welcome_from_an_unexpected_device_is_an_identity_mismatch() {
        let (ctx, _events) = context(4, 8);
        let welcome = handshake(5, "Eve");
        let result = dial_against(
            &handshake(1, "Alice"),
            &ctx,
            Some(device(99)),
            move |_reader, mut writer| async move {
                writer.send(&Frame::Welcome(welcome)).await.expect("send");
            },
        )
        .await;

        match result {
            Err(TransportError::IdentityMismatch { expected, got }) => {
                assert_eq!(expected, device(99).to_string());
                assert_eq!(got, device(5).to_string());
            }
            other => panic!("expected an identity mismatch, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_dialer_refuses_a_welcome_once_its_own_limit_is_reached() {
        let (ctx, _events) = context(4, 0);
        let welcome = handshake(5, "Eve");
        let result = dial_against(
            &handshake(1, "Alice"),
            &ctx,
            None,
            move |_reader, mut writer| async move {
                writer.send(&Frame::Welcome(welcome)).await.expect("send");
            },
        )
        .await;

        assert!(matches!(result, Err(TransportError::PeerLimit { max: 0 })));
    }

    #[tokio::test]
    async fn dialling_a_closed_port_fails_instead_of_hanging() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        drop(listener);

        let (ctx, _events) = context(4, 8);
        // Windows answers a refused loopback connect after a couple of retransmits, so the
        // guard sits above that but below the connection's own five-second connect timeout.
        let result = timeout(
            Duration::from_secs(4),
            dial(address, &handshake(1, "Alice"), &ctx, None),
        )
        .await
        .expect("dial must return rather than hang");

        assert!(matches!(result, Err(TransportError::Io(_))));
    }

    // ---- serve ----------------------------------------------------------------------------

    #[tokio::test]
    async fn serve_reports_connect_frames_and_disconnect() {
        let (dialed, mut accepted, dialer_ctx, mut events) = handshaken_pair(8).await;

        let serving = tokio::spawn(serve(dialed, Arc::clone(&dialer_ctx)));

        let Some(TransportEvent::Connected {
            role,
            handshake: peer,
            link,
            ..
        }) = events.recv().await
        else {
            panic!("expected a Connected event");
        };
        assert_eq!(role, Role::Dialer);
        assert_eq!(peer.device_id, device(2));

        // A frame from the peer reaches the session.
        accepted
            .writer
            .send(&Frame::Heartbeat { seq: 7 })
            .await
            .expect("send");
        let Some(TransportEvent::Frame { frame, .. }) = events.recv().await else {
            panic!("expected a Frame event");
        };
        assert_eq!(frame, Frame::Heartbeat { seq: 7 });

        // A frame queued over the link reaches the peer.
        link.send(Frame::Heartbeat { seq: 8 })
            .await
            .expect("link send");
        assert_eq!(
            accepted.reader.next().await.expect("read"),
            Some(Frame::Heartbeat { seq: 8 })
        );

        // Dropping the last handle says goodbye and ends the connection.
        drop(link);
        let Some(TransportEvent::Disconnected { reason, .. }) = events.recv().await else {
            panic!("expected a Disconnected event");
        };
        assert_eq!(
            reason,
            DisconnectReason::LocalGoodbye(GoodbyeReason::Shutdown)
        );
        assert_eq!(
            accepted.reader.next().await.expect("read"),
            Some(Frame::Goodbye {
                reason: GoodbyeReason::Shutdown
            })
        );
        serving.await.expect("serve task");
        assert_eq!(dialer_ctx.live.count(), 0, "the slot is released");
    }

    #[tokio::test]
    async fn serve_releases_the_slot_when_the_session_is_already_gone() {
        let (dialed, _accepted, dialer_ctx, events) = handshaken_pair(8).await;
        assert_eq!(dialer_ctx.live.count(), 1);

        drop(events);
        serve(dialed, Arc::clone(&dialer_ctx)).await;

        assert_eq!(dialer_ctx.live.count(), 0);
    }

    #[tokio::test]
    async fn serve_tolerates_the_session_vanishing_mid_connection() {
        let (dialed, mut accepted, dialer_ctx, mut events) = handshaken_pair(8).await;
        let serving = tokio::spawn(serve(dialed, Arc::clone(&dialer_ctx)));

        let Some(TransportEvent::Connected { .. }) = events.recv().await else {
            panic!("expected a Connected event");
        };
        // The session goes away, then the peer hangs up: the disconnect cannot be reported
        // anywhere, which must not leave the task stuck.
        drop(events);
        accepted.writer.finish().await;

        serving.await.expect("serve task");
        assert_eq!(dialer_ctx.live.count(), 0);
    }

    // ---- pump -----------------------------------------------------------------------------

    #[tokio::test]
    async fn a_peer_that_closes_its_write_half_ends_the_pump_as_closed() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client.half_close().await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Closed);
        drop(link);
    }

    #[tokio::test]
    async fn a_write_error_on_a_heartbeat_ends_the_pump_as_closed() {
        let (mut reader, mut writer, _client) = endpoint_pair().await;
        // The write half is already gone: the next write must be reported, not retried.
        writer.finish().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        let timing = PumpTiming {
            heartbeat: Duration::from_millis(2),
            stall_after: Duration::from_secs(30),
        };

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            timing,
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Closed);
        drop(link);
    }

    #[tokio::test]
    async fn a_write_error_on_a_queued_frame_ends_the_pump_as_closed() {
        let (mut reader, mut writer, _client) = endpoint_pair().await;
        writer.finish().await;
        let (link, mut commands) = PeerLink::channel();
        link.send(Frame::Heartbeat { seq: 1 })
            .await
            .expect("the frame is queued before the pump starts");
        let (ctx, _events) = context(4, 8);
        let timing = PumpTiming {
            heartbeat: Duration::from_secs(30),
            stall_after: Duration::from_secs(30),
        };

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            timing,
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Closed);
        drop(link);
    }

    #[tokio::test]
    async fn a_queued_frame_is_written_and_a_close_says_goodbye() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        let sender = link.clone();

        let (reason, ()) = tokio::join!(
            run_pump(pump(
                &mut reader,
                &mut writer,
                &mut commands,
                &ctx,
                device(2),
                PumpTiming::default(),
            )),
            async move {
                sender
                    .send(Frame::Heartbeat { seq: 9 })
                    .await
                    .expect("link send");
                assert_eq!(
                    client.next().await,
                    Some(Frame::Heartbeat { seq: 9 }),
                    "the queued frame is written"
                );

                sender.close(GoodbyeReason::Superseded);
                assert_eq!(
                    client.next().await,
                    Some(Frame::Goodbye {
                        reason: GoodbyeReason::Superseded
                    })
                );
            }
        );

        assert_eq!(
            reason,
            DisconnectReason::LocalGoodbye(GoodbyeReason::Superseded)
        );
    }

    #[tokio::test]
    async fn dropping_every_link_handle_says_shutdown() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);

        let (reason, ()) = tokio::join!(
            run_pump(pump(
                &mut reader,
                &mut writer,
                &mut commands,
                &ctx,
                device(2),
                PumpTiming::default(),
            )),
            async move {
                drop(link);
                assert_eq!(
                    client.next().await,
                    Some(Frame::Goodbye {
                        reason: GoodbyeReason::Shutdown
                    }),
                    "the peer is told the session went away"
                );
            }
        );

        assert_eq!(
            reason,
            DisconnectReason::LocalGoodbye(GoodbyeReason::Shutdown)
        );
    }

    #[tokio::test]
    async fn a_frame_from_the_peer_is_forwarded_to_the_session() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, mut events) = context(4, 8);
        client.send(&Frame::Heartbeat { seq: 5 }).await;

        let (reason, ()) = tokio::join!(
            run_pump(pump(
                &mut reader,
                &mut writer,
                &mut commands,
                &ctx,
                device(2),
                PumpTiming::default(),
            )),
            async move {
                let Some(TransportEvent::Frame { peer, frame }) = events.recv().await else {
                    panic!("expected a Frame event");
                };
                assert_eq!(peer, device(2));
                assert_eq!(frame, Frame::Heartbeat { seq: 5 });
                drop(link);
            }
        );

        assert_eq!(
            reason,
            DisconnectReason::LocalGoodbye(GoodbyeReason::Shutdown)
        );
    }

    #[tokio::test]
    async fn a_frame_with_no_session_listener_ends_the_pump_as_closed() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, events) = context(4, 8);
        drop(events);
        client.send(&ack(1)).await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Closed);
        drop(link);
    }

    #[tokio::test]
    async fn a_goodbye_from_the_peer_ends_the_pump() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client
            .send(&Frame::Goodbye {
                reason: GoodbyeReason::Error,
            })
            .await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Goodbye(GoodbyeReason::Error));
        drop(link);
    }

    #[tokio::test]
    async fn a_peer_error_frame_ends_the_pump() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client
            .send(&Frame::error(ErrorCode::Internal, "boom"))
            .await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert_eq!(
            reason,
            DisconnectReason::Failed("peer error internal".to_owned())
        );
        drop(link);
    }

    #[tokio::test]
    async fn a_malformed_frame_is_answered_and_ends_the_pump() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client.send_raw(&length_prefixed(b"this is not json")).await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert!(
            matches!(&reason, DisconnectReason::Failed(message) if message.contains("malformed")),
            "unexpected reason: {reason:?}"
        );
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::Malformed,
                ..
            })
        ));
        // The connection is shut down after the refusal.
        assert!(client.next().await.is_none());
        drop(link);
    }

    #[tokio::test]
    async fn an_oversized_frame_is_refused_on_its_length_prefix() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client.send_raw(&[0xff, 0xff, 0xff, 0xff]).await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert!(
            matches!(&reason, DisconnectReason::Failed(message) if message.contains("outside the accepted range")),
            "unexpected reason: {reason:?}"
        );
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::Malformed,
                ..
            })
        ));
        drop(link);
    }

    #[tokio::test]
    async fn a_frame_left_half_read_at_the_end_of_stream_is_reported() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);
        client
            .send_raw(&u32::try_from(MAX_FRAME_BYTES).expect("fits").to_be_bytes())
            .await;
        client.send_raw(b"short").await;
        client.half_close().await;

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert!(
            matches!(&reason, DisconnectReason::Failed(message) if message.contains("partial frame")),
            "unexpected reason: {reason:?}"
        );
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::Malformed,
                ..
            })
        ));
        drop(link);
    }

    #[tokio::test]
    async fn too_many_content_frames_trip_the_rate_limit() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        // Room for every frame that could be forwarded before the limiter fires.
        let (ctx, mut events) = context(128, 8);

        let burst = RATE_LIMIT_BURST as usize;
        // Twice the burst, so a token or two of refill cannot let the whole stream through.
        let total = burst * 2;
        for index in 0..total {
            client.send(&ack(index as u128)).await;
        }

        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            PumpTiming::default(),
        ))
        .await;

        assert!(
            matches!(&reason, DisconnectReason::Failed(message) if message.contains("rate limit")),
            "unexpected reason: {reason:?}"
        );
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::RateLimited,
                ..
            })
        ));

        // The burst was forwarded; the denied frame was not.
        let mut forwarded = 0;
        while let Ok(TransportEvent::Frame { .. }) = events.try_recv() {
            forwarded += 1;
        }
        assert!(
            (burst..total).contains(&forwarded),
            "{forwarded} frames forwarded out of {total}"
        );
        drop(link);
    }

    #[tokio::test]
    async fn a_silent_peer_is_disconnected_after_the_stall_timeout() {
        let (mut reader, mut writer, mut client) = endpoint_pair().await;
        let (link, mut commands) = PeerLink::channel();
        let (ctx, _events) = context(4, 8);

        let timing = PumpTiming {
            heartbeat: Duration::from_millis(2),
            stall_after: Duration::from_millis(20),
        };
        let reason = run_pump(pump(
            &mut reader,
            &mut writer,
            &mut commands,
            &ctx,
            device(2),
            timing,
        ))
        .await;

        assert_eq!(reason, DisconnectReason::Stalled);
        assert!(
            matches!(client.next().await, Some(Frame::Heartbeat { .. })),
            "the peer saw the keep-alive that timed it out"
        );
        drop(link);
    }

    // ---- handle_frame ---------------------------------------------------------------------

    #[tokio::test]
    async fn handle_frame_forwards_heartbeats_and_exempts_them_from_the_rate_limit() {
        let (_reader, mut writer, _client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, mut bulk) = buckets();

        let beat = Frame::Heartbeat { seq: 1 };
        let outcome = handle_frame(
            beat.clone(),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(outcome, FrameOutcome::Forward(frame) if frame == beat));

        // Spend the whole frame budget, then send one more beat: it must still be forwarded.
        let now = Instant::now();
        while rate.try_acquire(now) {}
        let outcome = handle_frame(
            Frame::Heartbeat { seq: 2 },
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Forward(Frame::Heartbeat { seq: 2 })
        ));
    }

    #[tokio::test]
    async fn handle_frame_forwards_content_within_the_budget() {
        let (_reader, mut writer, _client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, mut bulk) = buckets();
        let content = ack(7);

        let outcome = handle_frame(
            content.clone(),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(outcome, FrameOutcome::Forward(frame) if frame == content));
    }

    #[tokio::test]
    async fn content_over_the_frame_rate_limit_is_refused_with_an_error() {
        let (_reader, mut writer, mut client) = endpoint_pair().await;
        let peer = device(2);
        let (mut chunk_rate, mut bulk) = {
            let (_, chunk_rate, bulk) = buckets();
            (chunk_rate, bulk)
        };
        // A budget with a single token, already spent.
        let mut rate = TokenBucket::new(1.0, RATE_LIMIT_PER_SECOND, Instant::now());
        assert!(rate.try_acquire(Instant::now()));

        let outcome = handle_frame(
            ack(1),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Close(DisconnectReason::Failed(message)) if message.contains("rate limit")
        ));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::RateLimited,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn handle_frame_closes_on_a_goodbye() {
        let (_reader, mut writer, _client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, mut bulk) = buckets();

        let outcome = handle_frame(
            Frame::Goodbye {
                reason: GoodbyeReason::Superseded,
            },
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Close(DisconnectReason::Goodbye(GoodbyeReason::Superseded))
        ));
    }

    #[tokio::test]
    async fn handle_frame_closes_on_a_peer_error() {
        let (_reader, mut writer, _client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, mut bulk) = buckets();

        let outcome = handle_frame(
            Frame::error(ErrorCode::BadBody, "too long"),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Close(DisconnectReason::Failed(message)) if message == "peer error bad_body"
        ));
    }

    fn file_chunk(len: usize) -> Frame {
        Frame::FileChunk {
            attachment: AttachmentId::generate(),
            offset: 0,
            data: vec![0_u8; len],
        }
    }

    #[tokio::test]
    async fn a_file_chunk_within_the_byte_budget_is_forwarded() {
        let (_reader, mut writer, _client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, mut bulk) = buckets();
        let chunk = file_chunk(64);

        let outcome = handle_frame(
            chunk.clone(),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(outcome, FrameOutcome::Forward(frame) if frame == chunk));
    }

    #[tokio::test]
    async fn a_file_chunk_over_the_byte_budget_is_refused() {
        let (_reader, mut writer, mut client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, mut chunk_rate, _) = buckets();
        // A byte budget far smaller than one chunk: the first chunk cannot be admitted.
        let mut bulk = TokenBucket::new(4.0, 1.0, Instant::now());

        let outcome = handle_frame(
            file_chunk(64),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Close(DisconnectReason::Failed(message)) if message.contains("file budget")
        ));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::RateLimited,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn a_file_chunk_over_the_chunk_count_is_refused() {
        let (_reader, mut writer, mut client) = endpoint_pair().await;
        let peer = device(2);
        let (mut rate, _chunk_rate, mut bulk) = buckets();
        // The byte budget is generous; the chunk-count bucket is already spent.
        let mut chunk_rate = TokenBucket::new(1.0, 1.0, Instant::now());
        assert!(chunk_rate.try_acquire(Instant::now()));

        let outcome = handle_frame(
            file_chunk(64),
            &mut writer,
            peer,
            &mut rate,
            &mut chunk_rate,
            &mut bulk,
        )
        .await;
        assert!(matches!(
            outcome,
            FrameOutcome::Close(DisconnectReason::Failed(message)) if message.contains("file budget")
        ));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::RateLimited,
                ..
            })
        ));
    }

    // ---- protocol-error reporting ---------------------------------------------------------

    #[tokio::test]
    async fn report_protocol_error_maps_each_failure_to_a_code() {
        let cases = [
            (
                TransportError::Protocol(ProtocolError::VersionMismatch {
                    got: 1,
                    supported: PROTOCOL_VERSION,
                }),
                ErrorCode::VersionMismatch,
            ),
            (
                TransportError::Protocol(ProtocolError::FrameSize {
                    len: 0,
                    max: MAX_FRAME_BYTES,
                }),
                ErrorCode::Malformed,
            ),
            (
                TransportError::Protocol(ProtocolError::Malformed("bad".to_owned())),
                ErrorCode::Malformed,
            ),
            (
                TransportError::TruncatedFrame { buffered: 3 },
                ErrorCode::Malformed,
            ),
            (TransportError::HandshakeClosed, ErrorCode::Internal),
        ];

        for (error, expected) in cases {
            let (_reader, mut writer, mut client) = endpoint_pair().await;
            report_protocol_error(&mut writer, &error).await;

            match client.next().await {
                Some(Frame::Error { code, .. }) => assert_eq!(code, expected, "for {error}"),
                other => panic!("expected an error frame for {error}, got {other:?}"),
            }
            assert!(client.next().await.is_none(), "the writer is shut down");
        }
    }

    #[tokio::test]
    async fn refuse_reports_a_version_mismatch_and_defaults_to_malformed() {
        let (_reader, mut writer, mut client) = endpoint_pair().await;
        let error = refuse(
            &mut writer,
            &TransportError::Protocol(ProtocolError::VersionMismatch {
                got: 1,
                supported: PROTOCOL_VERSION,
            }),
        )
        .await;
        assert!(matches!(error, TransportError::Failed(_)));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::VersionMismatch,
                ..
            })
        ));

        let (_reader, mut writer, mut client) = endpoint_pair().await;
        let error = refuse(&mut writer, &TransportError::HandshakeClosed).await;
        assert!(matches!(error, TransportError::Failed(_)));
        assert!(matches!(
            client.next().await,
            Some(Frame::Error {
                code: ErrorCode::Malformed,
                ..
            })
        ));
    }
}
