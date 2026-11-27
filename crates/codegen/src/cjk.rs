//! The Traditional Chinese fallback font: Noto Sans CJK TC (SIL Open Font
//! License 1.1) is embedded in LVGLab. On export, the characters a font
//! lacks but Noto Sans CJK TC has are rendered into a small companion font
//! that LVGL uses as `fallback`, so only the characters the UI shows end up
//! in the firmware.
//!
//! * Built-in Montserrat `N`: `<prefix>_font_cjk_N` contains the extra
//!   characters and falls back to `lv_font_montserrat_N`; styles that use
//!   Montserrat `N` reference the companion instead.
//! * A font asset: `<prefix>_font_<Name>_cjk` contains the extra characters
//!   and the asset's font falls back to it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _};

use crate::font::{FontSpec, LineMetrics, RasterGlyph, emit, glyph_line_metrics};

/// Noto Sans CJK TC Regular (OpenType/CFF).
pub static NOTO_SANS_CJK_TC: &[u8] = include_bytes!("../../app/assets/fonts/NotoSansCJKtc-Regular.otf");

/// Name of the embedded font, for comments and messages.
pub const FONT_NAME: &str = "Noto Sans CJK TC";

fn font() -> Option<&'static FontRef<'static>> {
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(NOTO_SANS_CJK_TC).ok()).as_ref()
}

/// Whether the embedded font has a glyph for `c`.
pub fn covers(c: char) -> bool {
    !c.is_whitespace() && font().is_some_and(|f| f.glyph_id(c).0 != 0)
}

/// `(line_height, base_line)` of LVGL's built-in Montserrat fonts.
const MONTSERRAT_METRICS: &[(u16, i32, i32)] = &[
    (8, 10, 2),
    (10, 11, 2),
    (12, 15, 3),
    (14, 16, 3),
    (16, 18, 3),
    (18, 21, 4),
    (20, 22, 4),
    (22, 24, 4),
    (24, 27, 5),
    (26, 29, 5),
    (28, 30, 5),
    (30, 33, 6),
    (32, 35, 6),
    (34, 38, 7),
    (36, 40, 7),
    (38, 41, 7),
    (40, 44, 8),
    (42, 46, 8),
    (44, 49, 9),
    (46, 50, 9),
    (48, 52, 9),
];

fn montserrat_metrics(size: u16) -> Option<LineMetrics> {
    MONTSERRAT_METRICS
        .iter()
        .find(|(s, _, _)| *s == size)
        .map(|&(_, line_height, base_line)| LineMetrics { line_height, base_line })
}

/// A line that fits both: the higher ascent and the deeper descent.
fn union(a: LineMetrics, b: LineMetrics) -> LineMetrics {
    let above = (a.line_height - a.base_line).max(b.line_height - b.base_line);
    let below = a.base_line.max(b.base_line);
    LineMetrics { line_height: above + below, base_line: below }
}

/// Renders `chars` at `size` px (em size, like the font converter).
fn rasterize(size: u16, chars: &BTreeSet<char>) -> Vec<RasterGlyph> {
    let Some(font) = font() else { return Vec::new() };
    let units = font.units_per_em().unwrap_or(1000.0);
    // ab_glyph scales by the font's height (ascent - descent), LVGL's
    // converter by the em.
    let scale = PxScale::from(f32::from(size.max(1)) * font.height_unscaled() / units);
    let scaled = font.as_scaled(scale);
    let mut out = Vec::new();
    for &c in chars {
        let id = font.glyph_id(c);
        if id.0 == 0 {
            continue;
        }
        let advance = scaled.h_advance(id);
        let glyph = id.with_scale_and_position(scale, ab_glyph::point(0.0, 0.0));
        let Some(outlined) = font.outline_glyph(glyph) else {
            out.push(RasterGlyph {
                code_point: u32::from(c),
                advance,
                width: 0,
                height: 0,
                xmin: 0,
                ymin: 0,
                coverage: Vec::new(),
            });
            continue;
        };
        let bounds = outlined.px_bounds();
        let (x0, y0) = (bounds.min.x.floor() as i32, bounds.min.y.floor() as i32);
        let (x1, y1) = (bounds.max.x.ceil() as i32, bounds.max.y.ceil() as i32);
        let (width, height) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let mut coverage = vec![0_u8; width * height];
        // `draw` passes coordinates relative to `px_bounds().min`.
        let (dx, dy) = ((bounds.min.x - x0 as f32).round() as u32, (bounds.min.y - y0 as f32).round() as u32);
        outlined.draw(|x, y, value| {
            let (x, y) = ((x + dx) as usize, (y + dy) as usize);
            if x < width && y < height {
                coverage[y * width + x] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
        out.push(RasterGlyph {
            code_point: u32::from(c),
            advance,
            width,
            height,
            xmin: x0,
            // Pixel rows grow downwards; LVGL measures the bottom from the baseline upwards.
            ymin: -y1,
            coverage,
        });
    }
    out
}

/// C source of a companion font with `chars` at `size` px. `fallback` is
/// the C expression of the font it falls back to (a built-in Montserrat
/// font), `builtin` its size: the line then also fits Montserrat's.
#[allow(clippy::too_many_arguments)]
pub fn render(
    symbol: &str,
    size: u16,
    bpp: u8,
    chars: &BTreeSet<char>,
    fallback: Option<&str>,
    builtin: Option<u16>,
    header: &str,
    include: &str,
) -> String {
    let glyphs = rasterize(size, chars);
    let size_px = f32::from(size.max(1));
    let own = glyph_line_metrics(&glyphs)
        .unwrap_or(LineMetrics { line_height: i32::from(size), base_line: (size_px * 0.12).ceil() as i32 });
    let metrics = match builtin.and_then(montserrat_metrics) {
        Some(montserrat) => union(own, montserrat),
        None => own,
    };
    let name = format!("{FONT_NAME} (fallback; © 2014-2021 Adobe, SIL Open Font License 1.1)");
    let spec = FontSpec { name: &name, symbol, size: size_px, bpp, header, include, fallback };
    emit(&spec, &glyphs, metrics)
}

/// Which companion fonts an export needs and the characters of each.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Built-in Montserrat size → characters.
    pub builtin: BTreeMap<u16, BTreeSet<char>>,
    /// Font asset name → characters.
    pub assets: BTreeMap<String, BTreeSet<char>>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.builtin.is_empty() && self.assets.is_empty()
    }

    /// Characters of `font` (as named in styles) the companion provides.
    pub fn chars(&self, font: &str) -> Option<&BTreeSet<char>> {
        match lvglab_model::style::builtin_font_size(font) {
            Some(size) => self.builtin.get(&size),
            None => self.assets.get(font),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_traditional_chinese_but_not_control_characters() {
        for c in "握力測量左右手，：（）臺灣".chars() {
            assert!(covers(c), "{c}");
        }
        assert!(!covers(' '));
        assert!(!covers('\u{F8FF}'));
    }

    #[test]
    fn renders_glyphs_with_sensible_metrics() {
        let chars: BTreeSet<char> = "握力".chars().collect();
        let glyphs = rasterize(20, &chars);
        assert_eq!(glyphs.len(), 2);
        for g in &glyphs {
            assert!((18.0..=22.0).contains(&g.advance), "advance {}", g.advance);
            assert!(g.width > 10 && g.width <= 22 && g.height > 10 && g.height <= 22, "{}x{}", g.width, g.height);
            assert!(g.ymin < 0 && g.ymin > -6, "ymin {}", g.ymin);
            assert_eq!(g.coverage.len(), g.width * g.height);
            assert!(g.coverage.iter().any(|&c| c > 200), "ink");
        }
    }

    #[test]
    fn companion_of_a_builtin_font_keeps_montserrats_line() {
        let chars: BTreeSet<char> = "握力".chars().collect();
        let c = render("ui_font_cjk_20", 20, 4, &chars, Some("&lv_font_montserrat_20"), Some(20), "// h\n", "ui.h");
        assert!(c.contains("const lv_font_t ui_font_cjk_20 = {"));
        assert!(c.contains(".fallback = &lv_font_montserrat_20,"));
        assert!(c.contains("2 glyphs"));
        // At least Montserrat 20's line (22 px, base line 4).
        let number = |key: &str| -> i32 {
            let at = c.find(key).unwrap() + key.len();
            c[at..].trim_start_matches([' ', '=']).split(',').next().unwrap().trim().parse().unwrap()
        };
        assert!(number(".line_height") >= 22 && number(".base_line") >= 4);
        assert!(c.contains(".range_start = 21147, .range_length = 1"), "力 U+529B");
    }

    #[test]
    fn unions_line_metrics() {
        let a = LineMetrics { line_height: 20, base_line: 3 };
        let b = LineMetrics { line_height: 22, base_line: 4 };
        assert_eq!(union(a, b), LineMetrics { line_height: 22, base_line: 4 });
        let tall = LineMetrics { line_height: 24, base_line: 2 };
        assert_eq!(union(tall, b), LineMetrics { line_height: 26, base_line: 4 });
    }
}
