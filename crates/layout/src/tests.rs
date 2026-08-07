use rosettacue_domain::{BlockSource, WritingMode};

use super::{
    BlockOrder, LayoutDoubt, LayoutOptions, Mask, ModeRule, Rect, analyze_mask, decode_mask,
};

/// Paints glyph-shaped ink so the analyzer sees what a real crop shows it.
///
/// A glyph is drawn as an em box inset on every side: the inset is what leaves
/// a blank run between neighbours, which is exactly how projection profiles
/// separate glyphs on a real bitmap. Ink therefore measures slightly less than
/// one em, and the em the analyzer recovers is correspondingly slightly small —
/// the systematic bias that makes the glyph count a soft signal.
struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<bool>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![false; (width * height) as usize],
        }
    }

    fn fill(&mut self, rect: Rect) {
        for y in rect.y..rect.bottom().min(self.height) {
            for x in rect.x..rect.right().min(self.width) {
                self.pixels[(y * self.width + x) as usize] = true;
            }
        }
    }

    /// Draws `count` glyphs of size `em` running from `(x, y)`.
    fn run(&mut self, x: u32, y: u32, em: u32, count: u32, writing_mode: WritingMode) -> &mut Self {
        self.scaled_run(x, y, em, count, writing_mode, 100)
    }

    /// Draws a run whose glyphs are `percent` of `em` — ruby, or a narrow bracket.
    fn scaled_run(
        &mut self,
        x: u32,
        y: u32,
        em: u32,
        count: u32,
        writing_mode: WritingMode,
        percent: u32,
    ) -> &mut Self {
        let inset = (em / 16).max(1);
        let size = (em * percent / 100).saturating_sub(inset * 2).max(1);
        for index in 0..count {
            let step = em * index;
            let (glyph_x, glyph_y) = if writing_mode.is_vertical() {
                (x + inset, y + step + inset)
            } else {
                (x + step + inset, y + inset)
            };
            self.fill(Rect {
                x: glyph_x,
                y: glyph_y,
                width: size,
                height: size,
            });
        }
        self
    }

    fn finish(&self) -> Mask {
        Mask::new(self.width, self.height, self.pixels.clone()).expect("mask")
    }
}

fn japanese() -> LayoutOptions {
    LayoutOptions {
        block_order: BlockOrder::RightToLeft,
        ..LayoutOptions::default()
    }
}

#[test]
fn reads_one_horizontal_row_as_a_single_block() {
    let mut canvas = Canvas::new(600, 120);
    canvas.run(20, 30, 52, 8, WritingMode::HorizontalTb);

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 1);
    let block = &layout.blocks[0];
    assert_eq!(block.writing_mode, WritingMode::HorizontalTb);
    assert_eq!(block.source, BlockSource::Detected);
    assert_eq!(block.units, Some(1));
    assert_eq!(layout.total_units(), Some(1));
}

#[test]
fn reads_two_horizontal_rows_as_two_units_of_one_block() {
    let mut canvas = Canvas::new(600, 200);
    canvas.run(20, 20, 52, 8, WritingMode::HorizontalTb).run(
        20,
        90,
        52,
        6,
        WritingMode::HorizontalTb,
    );

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 1);
    assert_eq!(layout.blocks[0].writing_mode, WritingMode::HorizontalTb);
    assert_eq!(layout.blocks[0].units, Some(2));
}

#[test]
fn reads_one_vertical_column_as_a_single_unit() {
    let mut canvas = Canvas::new(200, 400);
    canvas.run(70, 40, 52, 5, WritingMode::VerticalRl);

    let layout = analyze_mask(&canvas.finish(), &japanese());

    assert_eq!(layout.blocks.len(), 1);
    let block = &layout.blocks[0];
    assert_eq!(block.writing_mode, WritingMode::VerticalRl);
    assert_eq!(block.evidence.rule, ModeRule::Aspect);
    assert_eq!(block.units, Some(1));
    assert_eq!(block.expected_glyphs, vec![5]);
}

#[test]
fn counts_columns_in_a_multi_column_vertical_block() {
    let mut canvas = Canvas::new(300, 500);
    canvas.run(100, 40, 52, 6, WritingMode::VerticalRl).run(
        152,
        40,
        52,
        6,
        WritingMode::VerticalRl,
    );

    let layout = analyze_mask(&canvas.finish(), &japanese());

    assert_eq!(layout.blocks.len(), 1);
    assert_eq!(layout.blocks[0].writing_mode, WritingMode::VerticalRl);
    assert_eq!(layout.blocks[0].units, Some(2));
}

/// The cue that motivated blocks: `000327.png`, a 縦書き interjection at the top
/// right over two horizontal rows at the bottom left.
///
/// The rectangles come from the measurement of that bitmap — block A at
/// x990-1041/y21-236 and block B at x20-556/y614-726 on a 1062×747 canvas — and
/// are painted here rather than committed as pixels, because the analyzer only
/// ever sees the geometry and the source bitmap is not ours to redistribute.
#[test]
fn separates_the_measured_mixed_direction_cue() {
    let mut canvas = Canvas::new(1062, 747);
    canvas
        .run(990, 21, 54, 4, WritingMode::VerticalRl)
        .run(20, 614, 53, 10, WritingMode::HorizontalTb)
        .run(20, 675, 53, 9, WritingMode::HorizontalTb);

    let layout = analyze_mask(&canvas.finish(), &japanese());

    assert_eq!(layout.blocks.len(), 2, "{:?}", layout.blocks);
    let vertical = &layout.blocks[0];
    let horizontal = &layout.blocks[1];

    // The vertical interjection is read first: the two blocks do not share a
    // band of the screen, so reading order is top to bottom.
    assert_eq!(vertical.writing_mode, WritingMode::VerticalRl);
    assert_eq!(vertical.units, Some(1));
    assert_eq!(vertical.expected_glyphs, vec![4]);
    assert!(vertical.bounds.x >= 990);

    assert_eq!(horizontal.writing_mode, WritingMode::HorizontalTb);
    assert_eq!(horizontal.units, Some(2));
    assert!(horizontal.bounds.right() <= 557);

    // Three units in total, against the eight row bands the old whole-cue
    // projection would have demanded.
    assert_eq!(layout.total_units(), Some(3));

    // The glyph count is derived from ink length over em, so it lands within
    // one of the truth rather than on it.
    for (estimated, actual) in horizontal.expected_glyphs.iter().zip([10, 9]) {
        assert!(
            estimated.abs_diff(actual) <= 1,
            "expected {actual} glyphs, estimated {estimated}"
        );
    }
}

#[test]
fn orders_stacked_horizontal_blocks_top_to_bottom() {
    let mut canvas = Canvas::new(700, 700);
    canvas.run(40, 500, 52, 8, WritingMode::HorizontalTb).run(
        60,
        40,
        52,
        6,
        WritingMode::HorizontalTb,
    );

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 2);
    assert!(layout.blocks[0].bounds.y < layout.blocks[1].bounds.y);
}

#[test]
fn orders_side_by_side_blocks_by_the_script_direction() {
    let mut columns = Canvas::new(900, 400);
    columns.run(100, 40, 52, 5, WritingMode::VerticalRl).run(
        700,
        40,
        52,
        5,
        WritingMode::VerticalRl,
    );
    let mask = columns.finish();

    let japanese_layout = analyze_mask(&mask, &japanese());
    let latin_layout = analyze_mask(&mask, &LayoutOptions::default());

    assert_eq!(japanese_layout.blocks.len(), 2);
    assert!(japanese_layout.blocks[0].bounds.x > japanese_layout.blocks[1].bounds.x);
    assert!(latin_layout.blocks[0].bounds.x < latin_layout.blocks[1].bounds.x);
}

#[test]
fn does_not_count_a_vertical_ruby_column_as_a_main_column() {
    let mut canvas = Canvas::new(300, 400);
    canvas
        .run(100, 40, 52, 5, WritingMode::VerticalRl)
        .scaled_run(156, 40, 52, 5, WritingMode::VerticalRl, 50);

    let layout = analyze_mask(&canvas.finish(), &japanese());

    assert_eq!(layout.blocks.len(), 1);
    let block = &layout.blocks[0];
    assert_eq!(block.writing_mode, WritingMode::VerticalRl);
    assert_eq!(block.evidence.column_bands, 2, "the ruby column is visible");
    assert_eq!(block.units, Some(1), "but it is not a main column");
}

#[test]
fn keeps_a_narrow_bracket_within_one_glyph_of_the_true_count() {
    let mut canvas = Canvas::new(600, 120);
    canvas
        .scaled_run(20, 30, 52, 1, WritingMode::HorizontalTb, 40)
        .run(72, 30, 52, 6, WritingMode::HorizontalTb);

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    let estimated = layout.blocks[0].expected_glyphs[0];
    assert!(estimated.abs_diff(7) <= 1, "estimated {estimated} glyphs");
}

#[test]
fn folds_a_stray_speck_into_its_neighbour() {
    let mut canvas = Canvas::new(900, 300);
    canvas.run(40, 100, 52, 6, WritingMode::HorizontalTb);
    canvas.fill(Rect {
        x: 700,
        y: 140,
        width: 6,
        height: 6,
    });

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 1);
    assert!(
        layout
            .doubts
            .iter()
            .any(|doubt| matches!(doubt, LayoutDoubt::TinyBlockMerged { .. }))
    );
}

#[test]
fn degrades_to_one_block_when_the_cue_has_no_foreground() {
    let layout = analyze_mask(&Canvas::new(200, 100).finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 1);
    assert_eq!(layout.blocks[0].source, BlockSource::WholeCue);
    assert_eq!(layout.blocks[0].units, None);
    assert_eq!(layout.total_units(), None);
    assert!(layout.is_degraded());
    assert_eq!(layout.doubts, vec![LayoutDoubt::NoForeground]);
}

#[test]
fn degrades_to_one_block_when_the_cut_finds_too_many() {
    let mut canvas = Canvas::new(2400, 200);
    for index in 0..10 {
        canvas.run(40 + index * 240, 80, 40, 1, WritingMode::HorizontalTb);
    }

    let layout = analyze_mask(&canvas.finish(), &LayoutOptions::default());

    assert_eq!(layout.blocks.len(), 1);
    assert_eq!(layout.blocks[0].source, BlockSource::WholeCue);
    assert_eq!(layout.blocks[0].bounds, layout.image);
    assert!(
        layout
            .doubts
            .iter()
            .any(|doubt| matches!(doubt, LayoutDoubt::BlockCountCapped { .. }))
    );
}

#[test]
fn a_degraded_cue_asks_for_exactly_what_the_old_pipeline_asked_for() {
    let layout = analyze_mask(&Canvas::new(200, 100).finish(), &LayoutOptions::default());

    // One request, the whole bitmap, no unit constraint — byte for byte the
    // behaviour that predates blocks.
    assert_eq!(layout.blocks.len(), 1);
    assert_eq!(layout.blocks[0].bounds, layout.image);
    assert_eq!(layout.blocks[0].writing_mode, WritingMode::HorizontalTb);
}

#[test]
fn analyzes_a_decoded_png_the_same_way_as_its_mask() {
    let mut pixels = vec![0_u8; 600 * 120 * 3];
    let mut canvas = Canvas::new(600, 120);
    canvas.run(20, 30, 52, 8, WritingMode::HorizontalTb);
    let mask = canvas.finish();
    for y in 0..120_u32 {
        for x in 0..600_u32 {
            if mask.is_foreground(x, y) {
                let offset = ((y * 600 + x) * 3) as usize;
                pixels[offset..offset + 3].fill(230);
            }
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 600, 120);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(&pixels).expect("PNG pixels");
    }

    let decoded = decode_mask(&png_bytes).expect("decode mask");

    assert_eq!(
        analyze_mask(&decoded, &LayoutOptions::default()),
        analyze_mask(&mask, &LayoutOptions::default())
    );
}
