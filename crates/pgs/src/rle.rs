use crate::PgsError;

pub(crate) fn decode_rle(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PgsError> {
    let expected_pixels = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| PgsError::InvalidStream("bitmap dimensions are too large".to_owned()))?;
    let width = usize::try_from(width)
        .map_err(|_| PgsError::InvalidStream("bitmap width is too large".to_owned()))?;
    let height = usize::try_from(height)
        .map_err(|_| PgsError::InvalidStream("bitmap height is too large".to_owned()))?;
    let mut pixels = Vec::with_capacity(expected_pixels);
    let mut cursor = 0;
    let mut line_count = 0;

    while cursor < data.len() && line_count < height {
        let mut color = data[cursor];
        cursor += 1;
        let mut run = 1_usize;

        if color == 0 {
            let flags = *data.get(cursor).ok_or_else(|| {
                PgsError::InvalidStream("truncated RLE control sequence".to_owned())
            })?;
            cursor += 1;
            run = usize::from(flags & 0x3f);
            if flags & 0x40 != 0 {
                let tail = *data
                    .get(cursor)
                    .ok_or_else(|| PgsError::InvalidStream("truncated long RLE run".to_owned()))?;
                cursor += 1;
                run = (run << 8) | usize::from(tail);
            }
            if flags & 0x80 != 0 {
                color = *data.get(cursor).ok_or_else(|| {
                    PgsError::InvalidStream("truncated colored RLE run".to_owned())
                })?;
                cursor += 1;
            }
        }

        if run > 0 {
            if pixels.len() + run > expected_pixels {
                return Err(PgsError::InvalidStream(
                    "RLE run exceeds bitmap dimensions".to_owned(),
                ));
            }
            pixels.resize(pixels.len() + run, color);
        } else {
            if pixels.len() % width != 0 {
                return Err(PgsError::InvalidStream(
                    "RLE line does not match bitmap width".to_owned(),
                ));
            }
            line_count += 1;
        }
    }

    if pixels.len() != expected_pixels {
        return Err(PgsError::InvalidStream(format!(
            "RLE decoded {} pixels; expected {expected_pixels}",
            pixels.len()
        )));
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_literal_and_run_length_pixels() {
        let encoded = [1, 0, 0x82, 2, 0, 0, 0, 3, 0, 0];
        assert_eq!(
            decode_rle(&encoded, 3, 2).expect("decode RLE"),
            [1, 2, 2, 0, 0, 0]
        );
    }
}
