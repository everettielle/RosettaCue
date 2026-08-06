mod decoder;
mod error;
mod model;
mod rle;
mod segments;

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use decoder::Decoder;
pub use error::PgsError;
pub use model::CueImage;
use sha2::{Digest, Sha256};

pub struct DecodedCues {
    source: BufReader<File>,
    decoder: Decoder,
    finished: bool,
}

impl Iterator for DecodedCues {
    type Item = Result<CueImage, PgsError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            match segments::read_segment(&mut self.source) {
                Ok(Some(segment)) => match self.decoder.consume(&segment) {
                    Ok(Some(cue)) => return Some(Ok(cue)),
                    Ok(None) => {}
                    Err(error) => {
                        self.finished = true;
                        return Some(Err(error));
                    }
                },
                Ok(None) => {
                    self.finished = true;
                    return self.decoder.finish(5_000).transpose();
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            }
        }
    }
}

/// Opens an HDMV PGS SUP stream and yields one decoded cue at a time.
///
/// # Errors
///
/// Returns an error when the source does not exist or cannot be opened.
pub fn decode_sup(path: impl AsRef<Path>, padding: u32) -> Result<DecodedCues, PgsError> {
    let path = path.as_ref();
    if !path.is_file() {
        return Err(PgsError::SourceNotFound(path.to_path_buf()));
    }
    Ok(DecodedCues {
        source: BufReader::new(File::open(path)?),
        decoder: Decoder::new(padding),
        finished: false,
    })
}

/// Encodes a decoded cue as an RGB PNG and returns its SHA-256 digest.
///
/// # Errors
///
/// Returns an error when PNG encoding or writing the destination fails.
pub fn write_cue_png(path: impl AsRef<Path>, cue: &CueImage) -> Result<String, PgsError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut encoded = Vec::new();
    {
        let mut png_encoder = png::Encoder::new(Cursor::new(&mut encoded), cue.width, cue.height);
        png_encoder.set_color(png::ColorType::Rgb);
        png_encoder.set_depth(png::BitDepth::Eight);
        let mut writer = png_encoder.write_header()?;
        writer.write_image_data(&cue.rgb)?;
    }
    std::fs::write(path, &encoded)?;
    Ok(format!("{:x}", Sha256::digest(&encoded)))
}

#[derive(Debug, Clone)]
pub struct CueFile {
    pub index: u32,
    pub image_path: PathBuf,
    pub image_sha256: String,
    pub cue: CueImage,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(pts: u32, segment_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(b"PG");
        encoded.extend_from_slice(&pts.to_be_bytes());
        encoded.extend_from_slice(&0_u32.to_be_bytes());
        encoded.push(segment_type);
        encoded.extend_from_slice(
            &u16::try_from(payload.len())
                .expect("test payload length")
                .to_be_bytes(),
        );
        encoded.extend_from_slice(payload);
        encoded
    }

    #[test]
    fn decodes_synthetic_display_set_and_writes_png() {
        let mut presentation = Vec::new();
        presentation.extend_from_slice(&1_920_u16.to_be_bytes());
        presentation.extend_from_slice(&1_080_u16.to_be_bytes());
        presentation.push(0x10);
        presentation.extend_from_slice(&0_u16.to_be_bytes());
        presentation.extend_from_slice(&[0x80, 0, 0, 1]);
        presentation.extend_from_slice(&1_u16.to_be_bytes());
        presentation.extend_from_slice(&[0, 0]);
        presentation.extend_from_slice(&100_u16.to_be_bytes());
        presentation.extend_from_slice(&200_u16.to_be_bytes());
        let palette = [0, 0, 1, 235, 128, 128, 255, 2, 126, 128, 128, 255];
        let rle = [1, 2, 0, 0];
        let mut object = Vec::new();
        object.extend_from_slice(&1_u16.to_be_bytes());
        object.extend_from_slice(&[0, 0xc0]);
        object.extend_from_slice(&[0, 0, 8]);
        object.extend_from_slice(&2_u16.to_be_bytes());
        object.extend_from_slice(&1_u16.to_be_bytes());
        object.extend_from_slice(&rle);
        let mut clear = Vec::new();
        clear.extend_from_slice(&1_920_u16.to_be_bytes());
        clear.extend_from_slice(&1_080_u16.to_be_bytes());
        clear.push(0x10);
        clear.extend_from_slice(&1_u16.to_be_bytes());
        clear.extend_from_slice(&[0, 0, 0, 0]);
        let sup = [
            segment(0, 0x16, &presentation),
            segment(0, 0x14, &palette),
            segment(0, 0x15, &object),
            segment(0, 0x80, &[]),
            segment(90_000, 0x16, &clear),
            segment(90_000, 0x80, &[]),
        ]
        .concat();

        let temporary = tempfile::tempdir().expect("temporary directory");
        let sup_path = temporary.path().join("sample.sup");
        std::fs::write(&sup_path, sup).expect("write SUP");
        let cues = decode_sup(&sup_path, 0)
            .expect("open SUP")
            .collect::<Result<Vec<_>, _>>()
            .expect("decode SUP");
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start_ms(), 0);
        assert_eq!(cues[0].end_ms(), 1_000);
        assert_eq!(cues[0].bbox, (100, 200, 2, 1));

        let png_path = temporary.path().join("cue.png");
        let digest = write_cue_png(&png_path, &cues[0]).expect("write PNG");
        assert_eq!(digest.len(), 64);
        assert!(
            std::fs::read(png_path)
                .expect("read PNG")
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
    }
}
