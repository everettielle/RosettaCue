use std::collections::HashMap;

use crate::PgsError;
use crate::model::{BitmapObject, CueImage, ObjectReference, Presentation, Segment};
use crate::rle::decode_rle;
use crate::segments::{DISPLAY_END_SEGMENT, OBJECT_SEGMENT, PALETTE_SEGMENT, PRESENTATION_SEGMENT};

type Rgba = [u8; 4];
type Palette = HashMap<u8, Rgba>;

#[derive(Debug)]
struct RenderedEvent {
    pts: u64,
    canvas_width: u32,
    canvas_height: u32,
    bbox: (u32, u32, u32, u32),
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    forced: bool,
}

#[derive(Debug)]
struct Placement {
    reference: ObjectReference,
    bitmap_width: u32,
    indices: Vec<u8>,
    crop: (u32, u32, u32, u32),
}

#[derive(Debug, Clone, Copy)]
struct Region {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

pub(crate) struct Decoder {
    padding: u32,
    palettes: HashMap<u8, Palette>,
    objects: HashMap<u16, BitmapObject>,
    presentation: Option<Presentation>,
    pending: Option<RenderedEvent>,
    last_event_pts: u64,
}

impl Decoder {
    pub(crate) fn new(padding: u32) -> Self {
        Self {
            padding,
            palettes: HashMap::new(),
            objects: HashMap::new(),
            presentation: None,
            pending: None,
            last_event_pts: 0,
        }
    }

    pub(crate) fn consume(&mut self, segment: &Segment) -> Result<Option<CueImage>, PgsError> {
        match segment.kind {
            PRESENTATION_SEGMENT => {
                let presentation = parse_presentation(&segment.payload, segment.pts)?;
                if presentation.composition_state != 0 {
                    self.palettes.clear();
                    self.objects.clear();
                }
                self.presentation = Some(presentation);
                Ok(None)
            }
            PALETTE_SEGMENT => {
                self.parse_palette(&segment.payload)?;
                Ok(None)
            }
            OBJECT_SEGMENT => {
                self.parse_object(&segment.payload)?;
                Ok(None)
            }
            DISPLAY_END_SEGMENT => self.finish_display_set(),
            _ => Ok(None),
        }
    }

    pub(crate) fn finish(
        &mut self,
        fallback_duration_ms: u64,
    ) -> Result<Option<CueImage>, PgsError> {
        let Some(pending) = self.pending.take() else {
            return Ok(None);
        };
        let fallback_ticks = fallback_duration_ms.saturating_mul(90);
        let end_pts = self
            .last_event_pts
            .max(pending.pts.saturating_add(fallback_ticks));
        finish_cue(pending, end_pts, true).map(Some)
    }

    fn finish_display_set(&mut self) -> Result<Option<CueImage>, PgsError> {
        let presentation = self.presentation.clone().ok_or_else(|| {
            PgsError::InvalidStream("display end before presentation segment".to_owned())
        })?;
        self.last_event_pts = presentation.pts;
        let completed = self
            .pending
            .take()
            .map(|event| finish_cue(event, presentation.pts, false))
            .transpose()?;
        if !presentation.objects.is_empty() {
            self.pending = Some(self.render(&presentation)?);
        }
        Ok(completed)
    }

    fn parse_palette(&mut self, payload: &[u8]) -> Result<(), PgsError> {
        if payload.len() < 2 || !(payload.len() - 2).is_multiple_of(5) {
            return Err(PgsError::InvalidStream(
                "malformed palette segment".to_owned(),
            ));
        }
        let palette_id = payload[0];
        let hd = self
            .presentation
            .as_ref()
            .is_none_or(|presentation| presentation.canvas_height > 576);
        let palette = self.palettes.entry(palette_id).or_default();
        for entry in payload[2..].chunks_exact(5) {
            let [color_id, y, cr, cb, alpha] = entry else {
                unreachable!("chunks_exact always yields five bytes")
            };
            palette.insert(*color_id, ycbcr_to_rgba(*y, *cb, *cr, *alpha, hd));
        }
        Ok(())
    }

    fn parse_object(&mut self, payload: &[u8]) -> Result<(), PgsError> {
        if payload.len() < 4 {
            return Err(PgsError::InvalidStream(
                "object segment is too short".to_owned(),
            ));
        }
        let object_id = be16(payload, 0)?;
        let sequence_descriptor = payload[3];
        let object_data = &payload[4..];
        if sequence_descriptor & 0x80 != 0 {
            if object_data.len() < 7 {
                return Err(PgsError::InvalidStream(
                    "initial object segment is too short".to_owned(),
                ));
            }
            let declared_length = be24(object_data, 0)?;
            let expected_rle_length = declared_length
                .checked_sub(4)
                .ok_or_else(|| PgsError::InvalidStream("invalid object data length".to_owned()))?;
            let bitmap = BitmapObject {
                width: u32::from(be16(object_data, 3)?),
                height: u32::from(be16(object_data, 5)?),
                expected_rle_length,
                rle: object_data[7..].to_vec(),
            };
            self.objects.insert(object_id, bitmap);
        } else {
            self.objects
                .get_mut(&object_id)
                .ok_or_else(|| {
                    PgsError::InvalidStream(format!("continuation for unknown object {object_id}"))
                })?
                .rle
                .extend_from_slice(object_data);
        }
        let bitmap = self.objects.get(&object_id).expect("object was inserted");
        if bitmap.rle.len() > bitmap.expected_rle_length {
            return Err(PgsError::InvalidStream(format!(
                "object {object_id} exceeds declared RLE length"
            )));
        }
        Ok(())
    }

    fn render(&self, presentation: &Presentation) -> Result<RenderedEvent, PgsError> {
        let palette = self.palettes.get(&presentation.palette_id).ok_or_else(|| {
            PgsError::InvalidStream(format!("missing palette {}", presentation.palette_id))
        })?;
        let placements = self.prepare_placements(presentation)?;
        let (left, top, right, bottom) = placement_bounds(&placements)?;
        let region = Region {
            left,
            top,
            width: right - left,
            height: bottom - top,
        };
        let mut rgba = vec![0; buffer_len(region.width, region.height, 4)?];
        composite_placements(
            &placements,
            palette,
            region.left,
            region.top,
            region.width,
            &mut rgba,
        )?;
        self.crop_to_rgb(presentation, &placements, region, &rgba)
    }

    fn prepare_placements(&self, presentation: &Presentation) -> Result<Vec<Placement>, PgsError> {
        presentation
            .objects
            .iter()
            .map(|reference| {
                let bitmap = self.objects.get(&reference.object_id).ok_or_else(|| {
                    PgsError::InvalidStream(format!("missing object {}", reference.object_id))
                })?;
                if !bitmap.complete() {
                    return Err(PgsError::InvalidStream(format!(
                        "incomplete object {}",
                        reference.object_id
                    )));
                }
                let crop = reference
                    .crop
                    .unwrap_or((0, 0, bitmap.width, bitmap.height));
                validate_crop(reference.object_id, crop, bitmap.width, bitmap.height)?;
                Ok(Placement {
                    reference: reference.clone(),
                    bitmap_width: bitmap.width,
                    indices: decode_rle(&bitmap.rle, bitmap.width, bitmap.height)?,
                    crop,
                })
            })
            .collect()
    }

    fn crop_to_rgb(
        &self,
        presentation: &Presentation,
        placements: &[Placement],
        region: Region,
        rgba: &[u8],
    ) -> Result<RenderedEvent, PgsError> {
        let (min_x, min_y, max_x, max_y) = visible_bounds(rgba, region.width, region.height)?;
        let content_width = max_x - min_x + 1;
        let content_height = max_y - min_y + 1;
        let output_width = content_width + self.padding * 2;
        let output_height = content_height + self.padding * 2;
        let mut rgb = vec![0; buffer_len(output_width, output_height, 3)?];
        copy_visible_rgb(
            rgba,
            &mut rgb,
            region.width,
            output_width,
            min_x,
            min_y,
            content_width,
            content_height,
            self.padding,
        )?;
        Ok(RenderedEvent {
            pts: presentation.pts,
            canvas_width: presentation.canvas_width,
            canvas_height: presentation.canvas_height,
            bbox: (
                region.left + min_x,
                region.top + min_y,
                content_width,
                content_height,
            ),
            width: output_width,
            height: output_height,
            rgb,
            forced: placements
                .iter()
                .any(|placement| placement.reference.forced()),
        })
    }
}

fn parse_presentation(payload: &[u8], pts: u64) -> Result<Presentation, PgsError> {
    if payload.len() < 11 {
        return Err(PgsError::InvalidStream(
            "presentation segment is too short".to_owned(),
        ));
    }
    let object_count = usize::from(payload[10]);
    let mut cursor = 11;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        if cursor + 8 > payload.len() {
            return Err(PgsError::InvalidStream(
                "truncated presentation object reference".to_owned(),
            ));
        }
        let composition_flag = payload[cursor + 3];
        let mut reference = ObjectReference {
            object_id: be16(payload, cursor)?,
            composition_flag,
            x: u32::from(be16(payload, cursor + 4)?),
            y: u32::from(be16(payload, cursor + 6)?),
            crop: None,
        };
        cursor += 8;
        if composition_flag & 0x80 != 0 {
            if cursor + 8 > payload.len() {
                return Err(PgsError::InvalidStream(
                    "truncated object crop rectangle".to_owned(),
                ));
            }
            reference.crop = Some((
                u32::from(be16(payload, cursor)?),
                u32::from(be16(payload, cursor + 2)?),
                u32::from(be16(payload, cursor + 4)?),
                u32::from(be16(payload, cursor + 6)?),
            ));
            cursor += 8;
        }
        objects.push(reference);
    }
    Ok(Presentation {
        pts,
        canvas_width: u32::from(be16(payload, 0)?),
        canvas_height: u32::from(be16(payload, 2)?),
        composition_state: payload[7] >> 6,
        palette_id: payload[9],
        objects,
    })
}

fn validate_crop(
    object_id: u16,
    (x, y, width, height): (u32, u32, u32, u32),
    bitmap_width: u32,
    bitmap_height: u32,
) -> Result<(), PgsError> {
    if width == 0
        || height == 0
        || x.saturating_add(width) > bitmap_width
        || y.saturating_add(height) > bitmap_height
    {
        return Err(PgsError::InvalidStream(format!(
            "invalid crop for object {object_id}"
        )));
    }
    Ok(())
}

fn placement_bounds(placements: &[Placement]) -> Result<(u32, u32, u32, u32), PgsError> {
    let first = placements
        .first()
        .ok_or_else(|| PgsError::InvalidStream("presentation contains no objects".to_owned()))?;
    let mut bounds = (
        first.reference.x,
        first.reference.y,
        first.reference.x + first.crop.2,
        first.reference.y + first.crop.3,
    );
    for placement in &placements[1..] {
        bounds.0 = bounds.0.min(placement.reference.x);
        bounds.1 = bounds.1.min(placement.reference.y);
        bounds.2 = bounds.2.max(placement.reference.x + placement.crop.2);
        bounds.3 = bounds.3.max(placement.reference.y + placement.crop.3);
    }
    Ok(bounds)
}

fn composite_placements(
    placements: &[Placement],
    palette: &Palette,
    left: u32,
    top: u32,
    region_width: u32,
    rgba: &mut [u8],
) -> Result<(), PgsError> {
    for placement in placements {
        let (crop_x, crop_y, crop_width, crop_height) = placement.crop;
        for source_y in crop_y..crop_y + crop_height {
            for source_x in crop_x..crop_x + crop_width {
                let source = pixel_offset(source_x, source_y, placement.bitmap_width, 1)?;
                let color = palette
                    .get(&placement.indices[source])
                    .copied()
                    .unwrap_or([0, 0, 0, 0]);
                if color[3] == 0 {
                    continue;
                }
                let destination_x = placement.reference.x - left + source_x - crop_x;
                let destination_y = placement.reference.y - top + source_y - crop_y;
                let destination = pixel_offset(destination_x, destination_y, region_width, 4)?;
                rgba[destination..destination + 4].copy_from_slice(&color);
            }
        }
    }
    Ok(())
}

fn visible_bounds(rgba: &[u8], width: u32, height: u32) -> Result<(u32, u32, u32, u32), PgsError> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for y in 0..height {
        for x in 0..width {
            if rgba[pixel_offset(x, y, width, 4)? + 3] == 0 {
                continue;
            }
            bounds = Some(match bounds {
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
                None => (x, y, x, y),
            });
        }
    }
    bounds.ok_or_else(|| {
        PgsError::InvalidStream("rendered subtitle contains no visible pixels".to_owned())
    })
}

#[allow(clippy::too_many_arguments)]
fn copy_visible_rgb(
    rgba: &[u8],
    rgb: &mut [u8],
    source_width: u32,
    output_width: u32,
    min_x: u32,
    min_y: u32,
    content_width: u32,
    content_height: u32,
    padding: u32,
) -> Result<(), PgsError> {
    for y in 0..content_height {
        for x in 0..content_width {
            let source = pixel_offset(min_x + x, min_y + y, source_width, 4)?;
            let destination = pixel_offset(padding + x, padding + y, output_width, 3)?;
            let alpha = u16::from(rgba[source + 3]);
            for channel in 0..3 {
                rgb[destination + channel] =
                    u8::try_from(u16::from(rgba[source + channel]) * alpha / 255)
                        .expect("alpha composition remains in byte range");
            }
        }
    }
    Ok(())
}

fn finish_cue(
    event: RenderedEvent,
    end_pts: u64,
    inferred_end: bool,
) -> Result<CueImage, PgsError> {
    if end_pts <= event.pts {
        return Err(PgsError::InvalidStream(
            "subtitle cue has a non-positive duration".to_owned(),
        ));
    }
    Ok(CueImage {
        start_pts: event.pts,
        end_pts,
        canvas_width: event.canvas_width,
        canvas_height: event.canvas_height,
        bbox: event.bbox,
        width: event.width,
        height: event.height,
        rgb: event.rgb,
        forced: event.forced,
        inferred_end,
    })
}

fn be16(data: &[u8], offset: usize) -> Result<u16, PgsError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| PgsError::InvalidStream("truncated two-byte integer".to_owned()))?;
    Ok(u16::from_be_bytes(bytes.try_into().expect("two bytes")))
}

fn be24(data: &[u8], offset: usize) -> Result<usize, PgsError> {
    let bytes = data
        .get(offset..offset + 3)
        .ok_or_else(|| PgsError::InvalidStream("truncated three-byte integer".to_owned()))?;
    Ok((usize::from(bytes[0]) << 16) | (usize::from(bytes[1]) << 8) | usize::from(bytes[2]))
}

fn buffer_len(width: u32, height: u32, channels: usize) -> Result<usize, PgsError> {
    usize::try_from(u64::from(width) * u64::from(height))
        .ok()
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| PgsError::InvalidStream("image buffer is too large".to_owned()))
}

fn pixel_offset(x: u32, y: u32, width: u32, channels: usize) -> Result<usize, PgsError> {
    usize::try_from(u64::from(y) * u64::from(width) + u64::from(x))
        .ok()
        .and_then(|pixel| pixel.checked_mul(channels))
        .ok_or_else(|| PgsError::InvalidStream("pixel offset is too large".to_owned()))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn clamp_color(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn ycbcr_to_rgba(y: u8, cb: u8, cr: u8, alpha: u8, hd: bool) -> Rgba {
    let luminance = 1.164_383 * (f64::from(y) - 16.0);
    let (red, green, blue) = if hd {
        (
            luminance + 1.792_741 * (f64::from(cr) - 128.0),
            luminance - 0.213_249 * (f64::from(cb) - 128.0) - 0.532_909 * (f64::from(cr) - 128.0),
            luminance + 2.112_402 * (f64::from(cb) - 128.0),
        )
    } else {
        (
            luminance + 1.596_027 * (f64::from(cr) - 128.0),
            luminance - 0.391_762 * (f64::from(cb) - 128.0) - 0.812_968 * (f64::from(cr) - 128.0),
            luminance + 2.017_232 * (f64::from(cb) - 128.0),
        )
    };
    [
        clamp_color(red),
        clamp_color(green),
        clamp_color(blue),
        alpha,
    ]
}
