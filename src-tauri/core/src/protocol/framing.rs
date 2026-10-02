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
use crate::protocol::limits::{LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES};

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
/// The buffer never holds more than one frame plus a partial prefix, and a partial frame stays
/// in the buffer across calls.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// # Errors
    ///
    /// Returns [`ProtocolError::FrameSize`] if the declared length of the frame at the head of
    /// the buffer is invalid, or if the buffer exceeds what a valid frame plus its prefix could
    /// occupy.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
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

        if self.buffer.len() > MAX_FRAME_BYTES + LENGTH_PREFIX_BYTES {
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
        if self.buffer.len() < LENGTH_PREFIX_BYTES {
            return Ok(None);
        }
        let Some(prefix) = self.buffer.get(..LENGTH_PREFIX_BYTES) else {
            return Ok(None);
        };
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
        let total = LENGTH_PREFIX_BYTES + declared;
        if self.buffer.len() < total {
            return Ok(None);
        }

        // `drain` keeps the remaining bytes (the start of the next frame) in place.
        let frame: Vec<u8> = self
            .buffer
            .drain(..total)
            .skip(LENGTH_PREFIX_BYTES)
            .collect();
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
    fn the_buffer_cannot_grow_past_one_frame() {
        // A peer that declares a legal length and then floods must be cut off once the
        // buffer holds more than a legal frame plus its prefix.
        let mut decoder = FrameDecoder::new();
        let declared = u32::try_from(MAX_FRAME_BYTES).expect("fits");
        decoder.push(&declared.to_be_bytes()).expect("push");
        decoder
            .push(&vec![0u8; MAX_FRAME_BYTES])
            .expect("exactly one frame is legal");
        let err = decoder
            .push(&[0u8; 8])
            .expect_err("an extra frame's worth is not");
        assert!(matches!(err, ProtocolError::FrameSize { .. }));
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
}
