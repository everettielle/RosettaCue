use std::io::Cursor;

#[derive(Debug, Clone, Copy)]
struct Pixel {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

/// Estimates the number of large subtitle rows in an extracted PGS bitmap.
///
/// The cue crop has a flat background. Main glyph rows are substantially taller
/// than ruby rows, so a horizontal foreground projection can provide a reliable
/// validation hint without attempting to recognize any characters.
pub(crate) fn estimate_main_rows(png_bytes: &[u8]) -> Option<usize> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let width = usize::try_from(info.width).ok()?;
    let height = usize::try_from(info.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let bytes = &buffer[..info.buffer_size()];
    let pixel_at = |x: usize, y: usize| decode_pixel(bytes, width, x, y, info.color_type);
    let corners = [
        pixel_at(0, 0)?,
        pixel_at(width - 1, 0)?,
        pixel_at(0, height - 1)?,
        pixel_at(width - 1, height - 1)?,
    ];
    let background = Pixel {
        red: channel_average(&corners, |pixel| pixel.red),
        green: channel_average(&corners, |pixel| pixel.green),
        blue: channel_average(&corners, |pixel| pixel.blue),
        alpha: channel_average(&corners, |pixel| pixel.alpha),
    };

    let minimum_row_pixels = (width / 600).max(2);
    let mut active_rows = vec![false; height];
    for (y, active) in active_rows.iter_mut().enumerate() {
        let foreground = (0..width)
            .filter_map(|x| pixel_at(x, y))
            .filter(|pixel| is_foreground(*pixel, background))
            .count();
        *active = foreground >= minimum_row_pixels;
    }

    let clusters = row_clusters(&active_rows, 2);
    let maximum_height = clusters.iter().map(|(start, end)| end - start).max()?;
    if maximum_height < 4 {
        return None;
    }
    let main_rows = clusters
        .iter()
        .filter(|(start, end)| (end - start) * 100 >= maximum_height * 72)
        .count();
    (1..=6).contains(&main_rows).then_some(main_rows)
}

fn decode_pixel(
    bytes: &[u8],
    width: usize,
    x: usize,
    y: usize,
    color_type: png::ColorType,
) -> Option<Pixel> {
    let channels = match color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let offset = (y.checked_mul(width)?.checked_add(x)?).checked_mul(channels)?;
    match color_type {
        png::ColorType::Grayscale => {
            let value = *bytes.get(offset)?;
            Some(Pixel {
                red: value,
                green: value,
                blue: value,
                alpha: u8::MAX,
            })
        }
        png::ColorType::Rgb => Some(Pixel {
            red: *bytes.get(offset)?,
            green: *bytes.get(offset + 1)?,
            blue: *bytes.get(offset + 2)?,
            alpha: u8::MAX,
        }),
        png::ColorType::GrayscaleAlpha => {
            let value = *bytes.get(offset)?;
            Some(Pixel {
                red: value,
                green: value,
                blue: value,
                alpha: *bytes.get(offset + 1)?,
            })
        }
        png::ColorType::Rgba => Some(Pixel {
            red: *bytes.get(offset)?,
            green: *bytes.get(offset + 1)?,
            blue: *bytes.get(offset + 2)?,
            alpha: *bytes.get(offset + 3)?,
        }),
        png::ColorType::Indexed => None,
    }
}

fn channel_average(corners: &[Pixel; 4], channel: impl Fn(Pixel) -> u8) -> u8 {
    let total = corners
        .iter()
        .copied()
        .map(channel)
        .map(u32::from)
        .sum::<u32>();
    u8::try_from(total / 4).unwrap_or(u8::MAX)
}

fn is_foreground(pixel: Pixel, background: Pixel) -> bool {
    if pixel.alpha < 16 && background.alpha >= 16 {
        return false;
    }
    pixel.red.abs_diff(background.red) > 14
        || pixel.green.abs_diff(background.green) > 14
        || pixel.blue.abs_diff(background.blue) > 14
        || pixel.alpha.abs_diff(background.alpha) > 14
}

fn row_clusters(rows: &[bool], maximum_gap: usize) -> Vec<(usize, usize)> {
    let mut clusters = Vec::new();
    let mut start = None;
    let mut last_active = 0;
    for (index, active) in rows.iter().copied().enumerate() {
        if active {
            if start.is_none() {
                start = Some(index);
            }
            last_active = index;
        } else if let Some(cluster_start) = start
            && index.saturating_sub(last_active) > maximum_gap
        {
            clusters.push((cluster_start, last_active + 1));
            start = None;
        }
    }
    if let Some(cluster_start) = start {
        clusters.push((cluster_start, last_active + 1));
    }
    clusters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_large_main_rows_from_small_ruby_rows() {
        let width = 80;
        let height = 60;
        let mut pixels = vec![0_u8; width * height * 3];
        for (start, end) in [(4, 10), (14, 27), (33, 47), (51, 57)] {
            for y in start..end {
                for x in 8..72 {
                    let offset = (y * width + x) * 3;
                    pixels[offset..offset + 3].fill(220);
                }
            }
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(
                &mut png_bytes,
                u32::try_from(width).expect("PNG width"),
                u32::try_from(height).expect("PNG height"),
            );
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            writer.write_image_data(&pixels).expect("PNG pixels");
        }
        assert_eq!(estimate_main_rows(&png_bytes), Some(2));
    }

    #[test]
    fn clusters_rows_across_small_raster_gaps() {
        assert_eq!(
            row_clusters(&[false, true, false, true, false, false, false], 2),
            vec![(1, 4)]
        );
    }
}
