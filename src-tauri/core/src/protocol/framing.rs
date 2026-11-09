//! Length-prefixed framing.
//!
//! ```text
//! ┌────────────────┬───────────────────────────────┐
//! │ u32 BE length  │ payload (length bytes)        │
//! └────────────────┴───────────────────────────────┘
//! ```
//!
//! [`FrameDecoder`] never allocates for a length a peer merely claimed, and reports a malformed
//! length instead of stalling.

use crate::error::ProtocolError;
use crate::protocol::limits::{LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES, READ_CHUNK_BYTES};

/// # Errors
///
/// Returns [`ProtocolError::FrameSize`] for an empty or oversized payload.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameSize {
            len: payload.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    let len = u32::try_from(payload.len()).map_err(|_| ProtocolError::FrameSize {
        len: payload.len(),
        max: MAX_FRAME_BYTES,
    })?;
    let mut out = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Incremental decoder: feed it bytes, pull whole frames out.
///
/// The buffer never holds more than one frame plus one read chunk, and a partial frame stays
/// in the buffer across calls.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    /// Read cursor: everything before it has been handed out. The buffer shifts only in `push`,
    /// once per read rather than once per frame, so a burst of small frames stays linear.
    start: usize,
}

impl FrameDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len() - self.start
    }

    /// # Errors
    ///
    /// Returns [`ProtocolError::FrameSize`] if the declared length of the frame at the head of
    /// the buffer is invalid, or if the buffer exceeds what a valid frame plus one read could
    /// occupy.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        // Drop the bytes already handed out: this is the only place the buffer shifts, so
        // decoding a batch of frames does not move the remainder per frame.
        if self.start > 0 {
            self.buffer.drain(..self.start);
            self.start = 0;
        }
        self.buffer.extend_from_slice(bytes);

        // Validate the claimed length as soon as the prefix is complete: this is where a
        // hostile peer would otherwise make us wait for a frame that can never be legal.
        if let Some(prefix) = self.buffer.get(..LENGTH_PREFIX_BYTES) {
            let declared =
                u32::from_be_bytes(prefix.try_into().map_err(|_| ProtocolError::FrameSize {
                    len: self.buffer.len(),
                    max: MAX_FRAME_BYTES,
                })?) as usize;
            if declared == 0 || declared > MAX_FRAME_BYTES {
                return Err(ProtocolError::FrameSize {
                    len: declared,
                    max: MAX_FRAME_BYTES,
                });
            }
        }

        // A single read can complete the head frame and still carry the first bytes of the next
        // one, so the bound allows one frame plus one whole read chunk. A peer that declares a
        // legal length and then floods is still cut off before the buffer can grow further,
        // because the reader decodes and drains the head frame before reading again.
        if self.buffer.len() > MAX_FRAME_BYTES + LENGTH_PREFIX_BYTES + READ_CHUNK_BYTES {
            return Err(ProtocolError::FrameSize {
                len: self.buffer.len(),
                max: MAX_FRAME_BYTES,
            });
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Returns [`ProtocolError::FrameSize`] if the buffer's declared length became invalid,
    /// which cannot happen after a successful [`FrameDecoder::push`] but is checked rather
    /// than assumed.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ProtocolError> {
        let remaining = self.buffered();
        if remaining < LENGTH_PREFIX_BYTES {
            return Ok(None);
        }
        let Some(prefix) = self
            .buffer
            .get(self.start..self.start + LENGTH_PREFIX_BYTES)
        else {
            return Ok(None);
        };
        let declared =
            u32::from_be_bytes(prefix.try_into().map_err(|_| ProtocolError::FrameSize {
                len: remaining,
                max: MAX_FRAME_BYTES,
            })?) as usize;
        if declared == 0 || declared > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameSize {
                len: declared,
                max: MAX_FRAME_BYTES,
            });
        }
        let total = LENGTH_PREFIX_BYTES + declared;
        if remaining < total {
            return Ok(None);
        }

        // Copy the payload and advance the cursor; the rest of the buffer stays where it is
        // until the next `push`, which compacts once. The range is known to be in bounds — it was
        // compared against `buffered()` above — but it is read through `get` rather than indexed.
        let Some(frame) = self
            .buffer
            .get(self.start + LENGTH_PREFIX_BYTES..self.start + total)
        else {
            return Ok(None);
        };
        let frame = frame.to_vec();
        self.start += total;
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(payload: &[u8]) -> Vec<u8> {
        encode_frame(payload).expect("payload is within limits")
    }

    #[test]
    fn a_single_frame_round_trips() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&framed(b"hello")).expect("push");
        assert_eq!(
            decoder.next_frame().expect("decode"),
            Some(b"hello".to_vec())
        );
        assert_eq!(decoder.next_frame().expect("decode"), None);
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn several_frames_in_one_read_are_all_returned_in_order() {
        let mut buffer = framed(b"one");
        buffer.extend_from_slice(&framed(b"two"));
        buffer.extend_from_slice(&framed(b"three"));

        let mut decoder = FrameDecoder::new();
        decoder.push(&buffer).expect("push");

        let mut seen = Vec::new();
        while let Some(frame) = decoder.next_frame().expect("decode") {
            seen.push(frame);
        }
        assert_eq!(
            seen,
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
        );
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn a_frame_split_across_many_reads_is_reassembled() {
        let payload = b"a moderately long body that will not arrive at once";
        let wire = framed(payload);

        let mut decoder = FrameDecoder::new();
        for byte in wire.iter().take(wire.len() - 1) {
            decoder.push(std::slice::from_ref(byte)).expect("push");
            assert_eq!(
                decoder.next_frame().expect("decode"),
                None,
                "no frame may be produced before the last byte arrives"
            );
        }
        decoder.push(wire.last().copied().as_slice()).expect("push");
        assert_eq!(
            decoder.next_frame().expect("decode"),
            Some(payload.to_vec())
        );
    }

    #[test]
    fn the_length_prefix_is_big_endian() {
        let wire = framed(b"xy");
        assert_eq!(wire.get(..4), Some([0, 0, 0, 2].as_slice()));
        assert_eq!(wire.get(4..), Some(b"xy".as_slice()));
    }

    #[test]
    fn an_oversized_claimed_length_fails_without_allocating() {
        // 4 GiB claimed in the header: refused on the header alone.
        let mut decoder = FrameDecoder::new();
        let err = decoder
            .push(&[0xff, 0xff, 0xff, 0xff])
            .expect_err("must be refused");
        assert!(matches!(
            err,
            ProtocolError::FrameSize {
                len: 4_294_967_295,
                ..
            }
        ));
    }

    #[test]
    fn a_length_just_over_the_cap_is_refused() {
        let mut decoder = FrameDecoder::new();
        let declared = u32::try_from(MAX_FRAME_BYTES + 1).expect("fits");
        let err = decoder
            .push(&declared.to_be_bytes())
            .expect_err("must be refused");
        assert!(matches!(err, ProtocolError::FrameSize { .. }));
    }

    #[test]
    fn the_cap_itself_is_accepted() {
        let mut decoder = FrameDecoder::new();
        let declared = u32::try_from(MAX_FRAME_BYTES).expect("fits");
        decoder.push(&declared.to_be_bytes()).expect("accepted");
        assert_eq!(decoder.buffered(), 4);
    }

    #[test]
    fn a_zero_length_frame_is_refused() {
        let mut decoder = FrameDecoder::new();
        let err = decoder.push(&[0, 0, 0, 0]).expect_err("must be refused");
        assert!(matches!(err, ProtocolError::FrameSize { len: 0, .. }));
    }

    #[test]
    fn a_partial_prefix_does_not_commit_to_a_length() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&[0, 0]).expect("push");
        assert_eq!(decoder.next_frame().expect("decode"), None);
        assert_eq!(decoder.buffered(), 2);
    }

    #[test]
    fn the_buffer_cannot_grow_past_one_frame_plus_a_read() {
        // A peer that declares a legal length and then floods must be cut off once the buffer
        // holds more than one frame plus the largest single read.
        let mut decoder = FrameDecoder::new();
        let declared = u32::try_from(MAX_FRAME_BYTES).expect("fits");
        decoder.push(&declared.to_be_bytes()).expect("push");
        decoder
            .push(&vec![0u8; MAX_FRAME_BYTES + READ_CHUNK_BYTES])
            .expect("one frame plus one read is legal");
        let err = decoder.push(&[0u8; 1]).expect_err("one byte more is not");
        assert!(matches!(err, ProtocolError::FrameSize { .. }));
    }

    #[test]
    fn a_max_frame_followed_by_another_frame_in_one_read_is_accepted() {
        // The regression: one read can complete a maximum frame and carry the start of the
        // next, so the buffer legitimately reaches `MAX_FRAME_BYTES + prefix + read`.
        let mut buffer = encode_frame(&vec![7u8; MAX_FRAME_BYTES]).expect("max frame");
        buffer.extend_from_slice(&framed(b"next"));

        let mut decoder = FrameDecoder::new();
        decoder.push(&buffer).expect("push");
        assert_eq!(
            decoder
                .next_frame()
                .expect("decode")
                .map(|frame| frame.len()),
            Some(MAX_FRAME_BYTES)
        );
        assert_eq!(
            decoder.next_frame().expect("decode"),
            Some(b"next".to_vec())
        );
    }

    #[test]
    fn encode_frame_rejects_empty_and_oversized_payloads() {
        assert!(encode_frame(&[]).is_err());
        assert!(encode_frame(&vec![0u8; MAX_FRAME_BYTES + 1]).is_err());
        assert!(encode_frame(&vec![0u8; MAX_FRAME_BYTES]).is_ok());
    }

    #[test]
    fn a_trailing_partial_frame_survives_into_the_next_read() {
        let mut buffer = framed(b"complete");
        let partial = framed(b"incomplete");
        buffer.extend_from_slice(partial.get(..3).expect("three bytes"));

        let mut decoder = FrameDecoder::new();
        decoder.push(&buffer).expect("push");
        assert_eq!(
            decoder.next_frame().expect("decode"),
            Some(b"complete".to_vec())
        );
        assert_eq!(decoder.next_frame().expect("decode"), None);
        assert_eq!(decoder.buffered(), 3);

        decoder
            .push(partial.get(3..).expect("the rest"))
            .expect("push");
        assert_eq!(
            decoder.next_frame().expect("decode"),
            Some(b"incomplete".to_vec())
        );
    }

    #[test]
    fn a_buffer_that_bypassed_push_is_still_refused() {
        // `push` validates the length prefix, so these states are unreachable through the public
        // API. The check in `next_frame` is belt-and-braces, and this pins that it refuses an
        // impossible length rather than slicing a frame that cannot exist.
        let mut zero = FrameDecoder {
            buffer: vec![0, 0, 0, 0],
            start: 0,
        };
        assert!(matches!(
            zero.next_frame(),
            Err(ProtocolError::FrameSize { len: 0, .. })
        ));

        let mut oversized = FrameDecoder {
            buffer: u32::try_from(MAX_FRAME_BYTES + 1)
                .expect("fits")
                .to_be_bytes()
                .to_vec(),
            start: 0,
        };
        assert!(matches!(
            oversized.next_frame(),
            Err(ProtocolError::FrameSize { .. })
        ));
    }
}
