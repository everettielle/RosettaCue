use crate::{Mask, Rect};

/// A contiguous run of ink along one axis, as a half-open range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Band {
    pub start: u32,
    pub end: u32,
}

impl Band {
    pub(crate) const fn extent(self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

/// Raster gaps of up to this many pixels stay inside one band.
///
/// Antialiasing leaves single blank scanlines inside a glyph; a slightly larger
/// tolerance keeps those from being read as separate rows, while remaining far
/// below the spacing that separates two glyphs.
pub(crate) const RASTER_GAP: u32 = 2;

/// Bands shorter than this share of the longest one are annotations, not units.
///
/// Ruby is set at roughly half the main size on both axes, so the same
/// threshold separates ruby rows from main rows in horizontal writing and ruby
/// columns from main columns in vertical writing.
const MAIN_BAND_PERCENT: u32 = 72;

/// Groups active positions into bands, tolerating gaps of `maximum_gap`.
pub(crate) fn cluster(active: &[bool], offset: u32, maximum_gap: u32) -> Vec<Band> {
    let mut bands = Vec::new();
    let mut start = None;
    let mut last_active = 0_u32;
    for (index, occupied) in active.iter().copied().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        if occupied {
            if start.is_none() {
                start = Some(index);
            }
            last_active = index;
        } else if let Some(band_start) = start
            && index.saturating_sub(last_active) > maximum_gap
        {
            bands.push(Band {
                start: offset + band_start,
                end: offset + last_active + 1,
            });
            start = None;
        }
    }
    if let Some(band_start) = start {
        bands.push(Band {
            start: offset + band_start,
            end: offset + last_active + 1,
        });
    }
    bands
}

/// Marks the rows of `area` that carry enough ink to count as occupied.
pub(crate) fn row_activity(mask: &Mask, area: Rect) -> Vec<bool> {
    let minimum = minimum_ink(area.width);
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .filter(|x| mask.is_foreground(*x, y))
                .take(minimum)
                .count()
                >= minimum
        })
        .collect()
}

/// Marks the columns of `area` that carry enough ink to count as occupied.
pub(crate) fn column_activity(mask: &Mask, area: Rect) -> Vec<bool> {
    let minimum = minimum_ink(area.height);
    (area.x..area.right())
        .map(|x| {
            (area.y..area.bottom())
                .filter(|y| mask.is_foreground(x, *y))
                .take(minimum)
                .count()
                >= minimum
        })
        .collect()
}

fn minimum_ink(span: u32) -> usize {
    usize::try_from(span / 600).unwrap_or(2).max(2)
}

/// The bands large enough to be main text rather than annotation.
pub(crate) fn main_bands(bands: &[Band]) -> Vec<Band> {
    let Some(longest) = bands.iter().map(|band| band.extent()).max() else {
        return Vec::new();
    };
    bands
        .iter()
        .copied()
        .filter(|band| band.extent() * 100 >= longest * MAIN_BAND_PERCENT)
        .collect()
}

/// The middle band extent, or zero when there are no bands.
pub(crate) fn median_extent(bands: &[Band]) -> u32 {
    median(bands.iter().map(|band| band.extent()).collect())
}

/// The middle blank run between consecutive bands, or zero when there is none.
///
/// The median is what makes this usable: a subtitle row contains a handful of
/// word or ideographic spaces among dozens of ordinary inter-glyph gaps, so the
/// middle value is the ordinary gap and the spaces cannot pull it.
pub(crate) fn median_gap(bands: &[Band]) -> u32 {
    median(
        bands
            .windows(2)
            .map(|pair| pair[1].start.saturating_sub(pair[0].end))
            .collect(),
    )
}

fn median(mut values: Vec<u32>) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// The smallest rectangle inside `area` that still contains every foreground pixel.
pub(crate) fn tight_bounds(mask: &Mask, area: Rect) -> Option<Rect> {
    let mut left = u32::MAX;
    let mut top = u32::MAX;
    let mut right = 0;
    let mut bottom = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if mask.is_foreground(x, y) {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
    }
    (right > left && bottom > top).then(|| Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_across_small_raster_gaps() {
        assert_eq!(
            cluster(
                &[false, true, false, true, false, false, false],
                0,
                RASTER_GAP
            ),
            vec![Band { start: 1, end: 4 }]
        );
    }

    #[test]
    fn splits_when_a_gap_exceeds_the_tolerance() {
        assert_eq!(
            cluster(&[true, false, false, false, true], 0, RASTER_GAP),
            vec![Band { start: 0, end: 1 }, Band { start: 4, end: 5 }]
        );
    }

    #[test]
    fn keeps_main_bands_and_drops_annotation_bands() {
        let bands = [
            Band { start: 0, end: 10 },
            Band { start: 20, end: 24 },
            Band { start: 30, end: 39 },
        ];

        assert_eq!(
            main_bands(&bands),
            vec![Band { start: 0, end: 10 }, Band { start: 30, end: 39 }]
        );
    }

    #[test]
    fn reports_the_middle_band_extent() {
        let bands = [
            Band { start: 0, end: 2 },
            Band { start: 10, end: 60 },
            Band { start: 70, end: 80 },
        ];

        assert_eq!(median_extent(&bands), 10);
        assert_eq!(median_extent(&[]), 0);
    }
}
