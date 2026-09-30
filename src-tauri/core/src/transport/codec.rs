//! Framed reading and writing over a TCP half-connection.
//!
//! The reader owns the decode buffer, so a partial frame that arrives across several reads
//! lives here rather than in the connection loop. The writer owns the write half and
//! serialises frames one at a time, which is what makes "send a goodbye, then close" a
//! single ordered operation.
//!
//! Both halves are generic over nothing: they wrap a `TcpStream`'s owned halves. When the
//! encryption layer of `docs/ARCHITECTURE.md` §5.6 is added, this is the module it plugs
//! into — the reader and writer become generic over `AsyncRead`/`AsyncWrite`, and nothing
//! above them changes.

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

use crate::error::{ProtocolError, TransportError};
use crate::protocol::limits::PROTOCOL_VERSION;
use crate::protocol::{Frame, FrameDecoder, decode, encode, peek_version};

/// Size of the scratch buffer used per read.
///
/// Independent of the frame size cap: a peer that sends one enormous frame in many packets
/// still occupies only this much memory in this task, and the decoder rejects the frame on
/// its length prefix before anything proportional is allocated.
const READ_CHUNK: usize = 8 * 1024;

/// Reads frames from one direction of a connection.
#[derive(Debug)]
pub struct FrameReader {
    inner: BufReader<OwnedReadHalf>,
    decoder: FrameDecoder,
    /// Reused across reads: allocating (and zeroing) 8 KiB per frame would be a pointless
    /// cost at message rates, and this buffer is touched only by this task.
    scratch: Box<[u8; READ_CHUNK]>,
}

impl FrameReader {
    /// Wraps the read half.
    #[must_use]
    pub fn new(read: OwnedReadHalf) -> Self {
        Self {
            inner: BufReader::new(read),
            decoder: FrameDecoder::new(),
            scratch: Box::new([0_u8; READ_CHUNK]),
        }
    }

    /// Reads the next frame.
    ///
    /// Returns `Ok(None)` on a clean end of stream between frames, which is a normal way for
    /// a connection to end and must be distinguished from an error.
    ///
    /// # Errors
    ///
    /// * [`TransportError::Io`] — the socket failed.
    /// * [`TransportError::TruncatedFrame`] — the stream ended in the middle of a frame.
    /// * [`TransportError::Protocol`] — the peer sent something that is not a valid frame for
    ///   this protocol version. The connection is not recoverable from any of these, but the
    ///   distinction is what lets the caller answer with a specific error code.
    pub async fn next(&mut self) -> Result<Option<Frame>, TransportError> {
        loop {
            if let Some(payload) = self.decoder.next_frame()? {
                // The version check happens before the full decode: if the peer speaks a
                // different version, its schema may not even parse, and reporting
                // "malformed" instead of "wrong version" would send the user hunting for a
                // bug that does not exist.
                let announced = peek_version(&payload)?;
                if announced != PROTOCOL_VERSION {
                    return Err(TransportError::Protocol(ProtocolError::VersionMismatch {
                        got: announced,
                        supported: PROTOCOL_VERSION,
                    }));
                }
                return Ok(Some(decode(&payload)?));
            }

            let read = self.inner.read(self.scratch.as_mut_slice()).await?;
            if read == 0 {
                if self.decoder.buffered() > 0 {
                    // Half a frame followed by a closed socket: a peer that died mid-write.
                    // Reporting end-of-stream here would silently drop a message.
                    return Err(TransportError::TruncatedFrame {
                        buffered: self.decoder.buffered(),
                    });
                }
                return Ok(None);
            }
            let filled = self.scratch.get(..read).unwrap_or(&[]);
            self.decoder.push(filled)?;
        }
    }
}

/// Writes frames to the other direction of a connection.
#[derive(Debug)]
pub struct FrameWriter {
    inner: OwnedWriteHalf,
}

impl FrameWriter {
    /// Wraps the write half.
    #[must_use]
    pub fn new(write: OwnedWriteHalf) -> Self {
        Self { inner: write }
    }

    /// Encodes and sends one frame.
    ///
    /// # Errors
    ///
    /// [`TransportError::Io`] on a socket failure, or [`TransportError::Protocol`] if the
    /// frame does not encode — which cannot happen for a frame built from domain values.
    pub async fn send(&mut self, frame: &Frame) -> Result<(), TransportError> {
        let bytes = encode(frame)?;
        self.inner.write_all(&bytes).await?;
        self.inner.flush().await?;
        Ok(())
    }

    /// Sends the pending bytes and closes the write direction.
    ///
    /// Called with a goodbye so the peer sees a clean end of stream rather than a reset.
    pub async fn finish(&mut self) {
        if let Err(error) = self.inner.shutdown().await {
            tracing::debug!(%error, "failed to shut down the write half cleanly");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{AvatarSeed, MessageId};
    use crate::domain::message::MessageBody;
    use crate::domain::nickname::Nickname;
    use crate::protocol::{GoodbyeReason, MAX_FRAME_BYTES};
    use tokio::net::TcpListener;

    /// Sends `bytes` over a real socket pair and returns what the reader produces.
    async fn round_trip_bytes(bytes: Vec<u8>) -> (Option<Frame>, Option<TransportError>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let writer = tokio::spawn(async move {
            let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
            let (_, mut write) = stream.into_split();
            write.write_all(&bytes).await.expect("write");
            write.shutdown().await.expect("shutdown");
        });

        let (stream, _) = listener.accept().await.expect("accept");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);

        let outcome = reader.next().await;
        writer.await.expect("writer task");

        match outcome {
            Ok(frame) => (frame, None),
            Err(error) => (None, Some(error)),
        }
    }

    #[tokio::test]
    async fn a_frame_is_read_back_after_being_written() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let sent = Frame::Profile {
            nickname: Nickname::parse("Аня").expect("valid"),
            avatar_seed: AvatarSeed::parse("seed").expect("valid"),
        };
        let expected = sent.clone();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, write) = stream.into_split();
            let mut writer = FrameWriter::new(write);
            writer.send(&sent).await.expect("send");
            writer.finish().await;
            // Keep the read half alive until the client has read, so the close is clean.
            drop(read);
        });

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);

        let received = reader.next().await.expect("read").expect("a frame");
        assert_eq!(received, expected);
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn a_clean_end_of_stream_is_not_an_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            drop(stream);
        });

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);
        assert!(reader.next().await.expect("no error").is_none());
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn several_frames_arrive_in_order() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (_, write) = stream.into_split();
            let mut writer = FrameWriter::new(write);
            for seq in 0..5_u64 {
                writer.send(&Frame::Heartbeat { seq }).await.expect("send");
            }
            writer.finish().await;
        });

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);

        for expected in 0..5_u64 {
            assert_eq!(
                reader.next().await.expect("read").expect("a frame"),
                Frame::Heartbeat { seq: expected }
            );
        }
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn an_oversized_claim_is_refused_without_allocating() {
        let (_, error) = round_trip_bytes(vec![0xff, 0xff, 0xff, 0xff]).await;
        let error = error.expect("must fail");
        assert!(
            matches!(
                error,
                TransportError::Protocol(ProtocolError::FrameSize { .. })
            ),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn a_frame_from_another_protocol_version_is_reported_as_such() {
        let payload = br#"{"v":99,"t":"heartbeat","seq":1}"#;
        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(payload);

        let (_, error) = round_trip_bytes(bytes).await;
        let error = error.expect("must fail");
        assert!(
            matches!(
                error,
                TransportError::Protocol(ProtocolError::VersionMismatch { got: 99, .. })
            ),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn garbage_is_refused_as_a_protocol_error() {
        let payload = b"this is not json";
        let mut bytes = (payload.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(payload);

        let (_, error) = round_trip_bytes(bytes).await;
        assert!(matches!(
            error.expect("must fail"),
            TransportError::Protocol(ProtocolError::Malformed(_))
        ));
    }

    #[tokio::test]
    async fn a_zero_length_prefix_is_refused() {
        let (_, error) = round_trip_bytes(vec![0, 0, 0, 0]).await;
        assert!(matches!(
            error.expect("must fail"),
            TransportError::Protocol(ProtocolError::FrameSize { len: 0, .. })
        ));
    }

    #[tokio::test]
    async fn a_truncated_frame_ends_the_stream_rather_than_hanging() {
        // A legal length prefix, but the declared payload never arrives: the reader must
        // report the truncation instead of waiting forever or silently dropping the frame.
        let declared = u32::try_from(MAX_FRAME_BYTES).expect("fits");
        let mut bytes = declared.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"only a few bytes");

        let (frame, error) = round_trip_bytes(bytes).await;
        assert!(frame.is_none());
        let error = error.expect("must fail");
        assert!(
            matches!(error, TransportError::TruncatedFrame { buffered } if buffered > 0),
            "unexpected error: {error:?}"
        );
    }

    #[tokio::test]
    async fn a_goodbye_frame_survives_the_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (_, write) = stream.into_split();
            let mut writer = FrameWriter::new(write);
            writer
                .send(&Frame::Goodbye {
                    reason: GoodbyeReason::Shutdown,
                })
                .await
                .expect("send");
            writer.finish().await;
        });

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);
        assert_eq!(
            reader.next().await.expect("read").expect("a frame"),
            Frame::Goodbye {
                reason: GoodbyeReason::Shutdown
            }
        );
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn a_maximum_size_frame_is_accepted() {
        // 8 000 three-byte characters is the largest legal body; it must survive intact.
        let body = "я".repeat(crate::protocol::MAX_BODY_CHARS);
        let frame = Frame::Chat {
            id: MessageId::generate(),
            body: MessageBody::parse(&body).expect("valid"),
        };
        let expected = frame.clone();

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (_, write) = stream.into_split();
            let mut writer = FrameWriter::new(write);
            writer.send(&frame).await.expect("send");
            writer.finish().await;
        });

        let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        let (read, _) = stream.into_split();
        let mut reader = FrameReader::new(read);
        assert_eq!(
            reader.next().await.expect("read").expect("a frame"),
            expected
        );
        server.await.expect("server task");
    }
}
