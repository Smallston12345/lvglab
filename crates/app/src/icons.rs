//! Icons of the editor: the Phosphor icon font (MIT), one consistent set
//! instead of emoji and symbol characters.

use eframe::egui::{FontData, FontDefinitions, FontFamily};
pub use egui_phosphor::regular::*;
use lvglab_model::WidgetKind;

/// egui's fonts plus the icon font; every `set_fonts` of the editor starts here.
pub fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // Bundle a Traditional Chinese fallback so the UI also works on systems
    // without CJK fonts. Keep the editor's Latin and icon fonts first.
    let name = "Noto Sans CJK TC";
    fonts
        .font_data
        .insert(name.into(), FontData::from_static(include_bytes!("../assets/fonts/NotoSansCJKtc-Regular.otf")).into());
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(name.into());
    }
    fonts
}

/// The icon of a widget type (palette, layers, parameters).
pub fn widget(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Screen => MONITOR,
        WidgetKind::Panel => SQUARE,
        WidgetKind::Container => SELECTION,
        WidgetKind::Label => TEXT_T,
        WidgetKind::Button => CURSOR_CLICK,
        WidgetKind::Image => IMAGE,
        WidgetKind::ImgButton => IMAGE_SQUARE,
        WidgetKind::Line => LINE_SEGMENT,
        WidgetKind::Arc => CIRCLE_NOTCH,
        WidgetKind::Bar => CHART_BAR,
        WidgetKind::Slider => SLIDERS_HORIZONTAL,
        WidgetKind::Switch => TOGGLE_RIGHT,
        WidgetKind::Checkbox => CHECK_SQUARE,
        WidgetKind::Dropdown => CARET_DOWN,
        WidgetKind::Roller => LIST,
        WidgetKind::Textarea => TEXTBOX,
        WidgetKind::Keyboard => KEYBOARD,
        WidgetKind::BtnMatrix => DOTS_NINE,
        WidgetKind::Spinbox => PLUS_MINUS,
        WidgetKind::Colorwheel => PALETTE,
        WidgetKind::Spinner => SPINNER,
        WidgetKind::Led => LIGHTBULB,
        WidgetKind::Meter => GAUGE,
        WidgetKind::Chart => CHART_LINE,
        WidgetKind::Table => TABLE,
        WidgetKind::Calendar => CALENDAR,
        WidgetKind::Canvas => PAINT_BRUSH,
        WidgetKind::AnimImg => FILM_STRIP,
        WidgetKind::Spangroup => PARAGRAPH,
        WidgetKind::Tabview => TABS,
        WidgetKind::TabPage => FILE,
        WidgetKind::Tileview => SQUARES_FOUR,
        WidgetKind::Tile => GRID_FOUR,
        WidgetKind::Win => APP_WINDOW,
        WidgetKind::List => LIST_BULLETS,
        WidgetKind::Menu => CARDS,
        WidgetKind::Msgbox => CHAT_TEXT,
        WidgetKind::Component => PACKAGE,
    }
}

#[cfg(test)]
mod tests {
    use crate::i18n::Language;
    use eframe::egui::{Context, FontFamily, FontId};

    #[test]
    fn editor_fonts_cover_traditional_chinese() {
        let ctx = Context::default();
        ctx.set_fonts(super::fonts());
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    let id = FontId::new(13.5, family);
                    let language = Language::TraditionalChinese;
                    for text in language.table().unwrap().values().map(String::as_str).chain([language.native_name()]) {
                        for c in text.chars().filter(|c| !c.is_control()) {
                            assert!(fonts.has_glyph(&id, c), "Missing glyph {c:?} in {:?}", id.family);
                        }
                    }
                }
                assert!(fonts.has_glyphs(&FontId::proportional(13.5), super::GEAR));
            });
        });
    }
}
