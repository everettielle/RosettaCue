use std::io::Cursor;

use crate::{LayoutError, Rect};

#[derive(Debug, Clone, Copy)]
struct Pixel {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

/// A decoded cue bitmap reduced to one bit per pixel.
///
/// Every layout decision is made from this map alone. Keeping the analysis on
/// the mask rather than on RGB pixels is what lets the whole algorithm be
/// exercised by literal test grids, with no image decoding in the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    width: u32,
    height: u32,
    pixels: Vec<bool>,
}

impl Mask {
    /// Builds a mask from a row-major foreground map.
    ///
    /// # Errors
    ///
    /// Returns an error when the dimensions are zero or do not match `pixels`.
    pub fn new(width: u32, height: u32, pixels: Vec<bool>) -> Result<Self, LayoutError> {
        let expected = usize::try_from(width)
            .ok()
            .and_then(|width| usize::try_from(height).ok().map(|height| width * height))
            .ok_or(LayoutError::Empty)?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err(LayoutError::Empty);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Parses a grid of `#` foreground and `.` background characters.
    ///
    /// # Errors
    ///
    /// Returns an error when the grid is empty or its rows differ in length.
    pub fn parse(grid: &str) -> Result<Self, LayoutError> {
        let rows = grid
            .lines()
            .map(str::trim)
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();
        let width = rows.first().map_or(0, |row| row.chars().count());
        if width == 0 || rows.iter().any(|row| row.chars().count() != width) {
            return Err(LayoutError::Empty);
        }
        let pixels = rows
            .iter()
            .flat_map(|row| row.chars().map(|cell| cell == '#'))
            .collect();
        Self::new(
            u32::try_from(width).map_err(|_| LayoutError::Empty)?,
            u32::try_from(rows.len()).map_err(|_| LayoutError::Empty)?,
            pixels,
        )
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The rectangle covering the whole bitmap.
    #[must_use]
    pub const fn area(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        }
    }

    #[must_use]
    pub fn is_foreground(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let index = usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x));
        index.is_ok_and(|index| self.pixels.get(index).copied().unwrap_or(false))
    }
}

/// Reduces a cue PNG to a foreground mask.
///
/// The background is estimated from the four corners and a pixel counts as
/// foreground when any channel differs from it appreciably — the rule the
/// previous row estimator used, kept because it is proven on real PGS crops.
///
/// # Errors
///
/// Returns an error when the PNG cannot be decoded or has no pixels.
pub fn decode_mask(png_bytes: &[u8]) -> Result<Mask, LayoutError> {
    let image = decode_rgba(png_bytes)?;
    let corners = [
        image.pixel(0, 0),
        image.pixel(image.width - 1, 0),
        image.pixel(0, image.height - 1),
        image.pixel(image.width - 1, image.height - 1),
    ];
    let background = Pixel {
        red: channel_average(corners, |pixel| pixel.red),
        green: channel_average(corners, |pixel| pixel.green),
        blue: channel_average(corners, |pixel| pixel.blue),
        alpha: channel_average(corners, |pixel| pixel.alpha),
    };
    let pixels = (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .map(|(x, y)| is_foreground(image.pixel(x, y), background))
        .collect();
    Mask::new(image.width, image.height, pixels)
}

/// Cuts `rect` out of a cue PNG, widened by `padding` on every side.
///
/// The padding keeps antialiased glyph edges inside the crop; clipping them
/// costs recognition accuracy on exactly the small text this is meant to help.
///
/// # Errors
///
/// Returns an error when the PNG cannot be decoded, the rectangle does not
/// overlap it, or the crop cannot be re-encoded.
pub fn crop_png(png_bytes: &[u8], rect: Rect, padding: u32) -> Result<Vec<u8>, LayoutError> {
    let image = decode_rgba(png_bytes)?;
    if rect.x >= image.width || rect.y >= image.height || rect.width == 0 || rect.height == 0 {
        return Err(LayoutError::CropOutOfBounds);
    }
    let left = rect.x.saturating_sub(padding);
    let top = rect.y.saturating_sub(padding);
    let right = rect
        .x
        .saturating_add(rect.width)
        .saturating_add(padding)
        .min(image.width);
    let bottom = rect
        .y
        .saturating_add(rect.height)
        .saturating_add(padding)
        .min(image.height);
    if right <= left || bottom <= top {
        return Err(LayoutError::CropOutOfBounds);
    }
    let width = right - left;
    let height = bottom - top;
    let mut cropped = Vec::with_capacity(
        usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or_default(),
    );
    for y in top..bottom {
        for x in left..right {
            let pixel = image.pixel(x, y);
            cropped.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| LayoutError::Encode(error.to_string()))?;
        writer
            .write_image_data(&cropped)
            .map_err(|error| LayoutError::Encode(error.to_string()))?;
    }
    Ok(output)
}

struct RgbaImage {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl RgbaImage {
    fn pixel(&self, x: u32, y: u32) -> Pixel {
        let offset = usize::try_from((u64::from(y) * u64::from(self.width) + u64::from(x)) * 4)
            .unwrap_or(usize::MAX);
        let channel = |shift: usize| self.bytes.get(offset + shift).copied().unwrap_or(0);
        Pixel {
            red: channel(0),
            green: channel(1),
            blue: channel(2),
            alpha: channel(3),
        }
    }
}

fn decode_rgba(png_bytes: &[u8]) -> Result<RgbaImage, LayoutError> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(
        png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .map_err(|error| LayoutError::Decode(error.to_string()))?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or(LayoutError::Empty)?];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| LayoutError::Decode(error.to_string()))?;
    if info.width == 0 || info.height == 0 {
        return Err(LayoutError::Empty);
    }
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => {
            return Err(LayoutError::Decode(
                "indexed PNG was not expanded".to_owned(),
            ));
        }
    };
    let pixels = usize::try_from(u64::from(info.width) * u64::from(info.height))
        .map_err(|_| LayoutError::Empty)?;
    let source = &buffer[..info.buffer_size()];
    let mut bytes = Vec::with_capacity(pixels * 4);
    for index in 0..pixels {
        let offset = index * channels;
        let channel = |shift: usize| source.get(offset + shift).copied().unwrap_or(0);
        let (red, green, blue, alpha) = match channels {
            1 => (channel(0), channel(0), channel(0), u8::MAX),
            2 => (channel(0), channel(0), channel(0), channel(1)),
            3 => (channel(0), channel(1), channel(2), u8::MAX),
            _ => (channel(0), channel(1), channel(2), channel(3)),
        };
        bytes.extend_from_slice(&[red, green, blue, alpha]);
    }
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        bytes,
    })
}

fn channel_average(corners: [Pixel; 4], channel: impl Fn(Pixel) -> u8) -> u8 {
    let total = corners.into_iter().map(channel).map(u32::from).sum::<u32>();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_png(width: u32, height: u32, painted: &[(u32, u32, u32, u32)]) -> Vec<u8> {
        let mut pixels = vec![0_u8; (width * height * 3) as usize];
        for (x, y, block_width, block_height) in painted.iter().copied() {
            for row in y..y + block_height {
                for column in x..x + block_width {
                    let offset = ((row * width + column) * 3) as usize;
                    pixels[offset..offset + 3].fill(220);
                }
            }
        }
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, width, height);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            writer.write_image_data(&pixels).expect("PNG pixels");
        }
        output
    }

    #[test]
    fn parses_a_literal_grid() {
        let mask = Mask::parse(
            "
            ..##..
            .####.
            ",
        )
        .expect("mask grid");

        assert_eq!((mask.width(), mask.height()), (6, 2));
        assert!(mask.is_foreground(2, 0));
        assert!(!mask.is_foreground(0, 0));
        assert!(!mask.is_foreground(99, 99));
    }

    #[test]
    fn rejects_a_ragged_grid() {
        assert!(Mask::parse("##\n###").is_err());
    }

    #[test]
    fn reads_foreground_against_the_corner_background() {
        let mask = decode_mask(&solid_png(40, 20, &[(10, 6, 20, 8)])).expect("decode mask");

        assert!(mask.is_foreground(15, 8));
        assert!(!mask.is_foreground(1, 1));
    }

    #[test]
    fn crops_with_padding_and_clamps_to_the_bitmap() {
        let png = solid_png(40, 20, &[(10, 6, 20, 8)]);

        let cropped = crop_png(
            &png,
            Rect {
                x: 10,
                y: 6,
                width: 20,
                height: 8,
            },
            4,
        )
        .expect("crop");
        let mask = decode_mask(&cropped).expect("decode crop");

        assert_eq!((mask.width(), mask.height()), (28, 16));
    }

    #[test]
    fn rejects_a_crop_outside_the_bitmap() {
        let png = solid_png(40, 20, &[(10, 6, 20, 8)]);

        assert!(
            crop_png(
                &png,
                Rect {
                    x: 80,
                    y: 80,
                    width: 4,
                    height: 4,
                },
                0,
            )
            .is_err()
        );
    }
}
