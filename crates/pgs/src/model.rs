#[derive(Debug, Clone)]
pub struct CueImage {
    pub start_pts: u64,
    pub end_pts: u64,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub bbox: (u32, u32, u32, u32),
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    pub forced: bool,
    pub inferred_end: bool,
}

impl CueImage {
    #[must_use]
    pub fn start_ms(&self) -> u64 {
        (self.start_pts * 1_000 + 45_000) / 90_000
    }

    #[must_use]
    pub fn end_ms(&self) -> u64 {
        (self.end_pts * 1_000 + 45_000) / 90_000
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Segment {
    pub pts: u64,
    pub kind: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct ObjectReference {
    pub object_id: u16,
    pub composition_flag: u8,
    pub x: u32,
    pub y: u32,
    pub crop: Option<(u32, u32, u32, u32)>,
}

impl ObjectReference {
    pub const fn forced(&self) -> bool {
        self.composition_flag & 0x40 != 0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Presentation {
    pub pts: u64,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub composition_state: u8,
    pub palette_id: u8,
    pub objects: Vec<ObjectReference>,
}

#[derive(Debug, Clone)]
pub(crate) struct BitmapObject {
    pub width: u32,
    pub height: u32,
    pub expected_rle_length: usize,
    pub rle: Vec<u8>,
}

impl BitmapObject {
    pub fn complete(&self) -> bool {
        self.rle.len() == self.expected_rle_length
    }
}
