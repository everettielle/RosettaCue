use std::io::Read;

use crate::PgsError;
use crate::model::Segment;

pub(crate) const PALETTE_SEGMENT: u8 = 0x14;
pub(crate) const OBJECT_SEGMENT: u8 = 0x15;
pub(crate) const PRESENTATION_SEGMENT: u8 = 0x16;
pub(crate) const DISPLAY_END_SEGMENT: u8 = 0x80;
const HEADER_LENGTH: usize = 13;

pub(crate) fn read_segment(source: &mut impl Read) -> Result<Option<Segment>, PgsError> {
    let mut header = [0_u8; HEADER_LENGTH];
    let mut read = 0;
    while read < header.len() {
        let count = source.read(&mut header[read..])?;
        if count == 0 {
            if read == 0 {
                return Ok(None);
            }
            return Err(PgsError::InvalidStream(
                "truncated PGS segment header".to_owned(),
            ));
        }
        read += count;
    }
    if &header[0..2] != b"PG" {
        return Err(PgsError::InvalidStream(
            "invalid PGS segment signature".to_owned(),
        ));
    }
    let pts = u64::from(u32::from_be_bytes(
        header[2..6].try_into().expect("four bytes"),
    ));
    let kind = header[10];
    let payload_length = usize::from(u16::from_be_bytes(
        header[11..13].try_into().expect("two bytes"),
    ));
    let mut payload = vec![0; payload_length];
    source.read_exact(&mut payload).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            PgsError::InvalidStream("truncated PGS segment payload".to_owned())
        } else {
            PgsError::Io(error)
        }
    })?;
    Ok(Some(Segment { pts, kind, payload }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_segment_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PG");
        bytes.extend_from_slice(&90_000_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.push(DISPLAY_END_SEGMENT);
        bytes.extend_from_slice(&0_u16.to_be_bytes());
        let segment = read_segment(&mut bytes.as_slice())
            .expect("read segment")
            .expect("segment");
        assert_eq!(segment.pts, 90_000);
        assert_eq!(segment.kind, DISPLAY_END_SEGMENT);
    }
}
