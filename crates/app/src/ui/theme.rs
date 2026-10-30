//! As cores e medidas, as do site do YouTube Music: fundo quase preto,
//! texto branco e cinzas, o vermelho só onde algo toca.

use egui::{Color32, CornerRadius, FontId, Stroke, Style, vec2};
use fastframe_fonts::Weight;

pub const BG: Color32 = Color32::from_rgb(3, 3, 3);
pub const SURFACE: Color32 = Color32::from_rgb(28, 28, 28);
pub const HOVER: Color32 = Color32::from_rgb(38, 38, 38);
pub const ACTIVE: Color32 = Color32::from_rgb(52, 52, 52);
pub const BORDER: Color32 = Color32::from_rgb(40, 40, 40);
pub const PLACEHOLDER: Color32 = Color32::from_rgb(34, 34, 34);
pub const FIELD: Color32 = Color32::from_rgb(36, 36, 36);
pub const TEXT: Color32 = Color32::from_rgb(255, 255, 255);
pub const TEXT_DIM: Color32 = Color32::from_rgb(170, 170, 170);
/// O branco a 70% do site (artistas, álbum, duração) sobre o fundo.
pub const TEXT_SUB: Color32 = Color32::from_rgb(179, 179, 179);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(118, 118, 118);
pub const ACCENT: Color32 = Color32::from_rgb(255, 0, 51);
/// Os menus suspensos e o fundo da linha sob o mouse neles.
pub const MENU: Color32 = Color32::from_rgb(33, 33, 33);
pub const MENU_HOVER: Color32 = Color32::from_rgb(52, 52, 52);
/// O azul dos links do site.
pub const LINK: Color32 = Color32::from_rgb(62, 166, 255);
/// Branco translúcido sobre o degradê do fundo, como os chips e a busca do
/// site: o fundo aparece através.
pub const GLASS: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);
pub const GLASS_HOVER: Color32 = Color32::from_rgba_premultiplied(38, 38, 38, 38);
pub const GLASS_ACTIVE: Color32 = Color32::from_rgba_premultiplied(56, 56, 56, 56);

pub const RADIUS: u8 = 8;

pub fn regular(size: f32) -> FontId {
    Weight::Regular.font_id(size)
}

pub fn medium(size: f32) -> FontId {
    Weight::Medium.font_id(size)
}

pub fn semibold(size: f32) -> FontId {
    Weight::SemiBold.font_id(size)
}

pub fn bold(size: f32) -> FontId {
    Weight::Bold.font_id(size)
}

pub fn apply(style: &mut Style) {
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(RADIUS);
    v.menu_corner_radius = CornerRadius::same(RADIUS);
    v.extreme_bg_color = FIELD;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = TEXT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.text_cursor.stroke = Stroke::new(2.0, TEXT);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(RADIUS);
        w.bg_stroke = Stroke::NONE;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.active.weak_bg_fill = ACTIVE;
    v.widgets.active.bg_fill = ACTIVE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);

    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.interaction.selectable_labels = false;
    style
        .text_styles
        .insert(egui::TextStyle::Body, regular(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, medium(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, regular(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, bold(22.0));
}
