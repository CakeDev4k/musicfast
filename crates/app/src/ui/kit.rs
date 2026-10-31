//! As peças repetidas: botão de ícone, botão com texto, chip, texto cortado
//! com reticências e o formato do tempo.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Color32, CornerRadius, CursorIcon, FontId, Galley, Pos2, Rect, Response, Sense, Ui, pos2, vec2};

use super::icons::{self, Icon};
use super::theme;

/// O fundo redondo que acende sob o mouse, e o ícone.
pub fn paint_icon_button(ui: &Ui, rect: Rect, response: &Response, icon: Icon, color: Color32) {
    if response.hovered() {
        let fill = if response.is_pointer_button_down_on() { theme::GLASS_ACTIVE } else { theme::GLASS_HOVER };
        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, fill);
    }
    let inner = rect.shrink(rect.width() * 0.25);
    icons::paint(ui.painter(), inner, icon, color);
}

/// Botão arredondado com ícone e texto. `primary`: branco sobre preto, como
/// o "Tocar" do site.
pub fn pill(ui: &mut Ui, icon: Option<Icon>, text: &str, primary: bool) -> Response {
    let font = theme::medium(14.0);
    let (fg, bg, bg_hover) = if primary {
        (Color32::BLACK, theme::TEXT, Color32::from_gray(220))
    } else {
        (theme::TEXT, theme::GLASS, theme::GLASS_HOVER)
    };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, fg);
    let icon_w = if icon.is_some() { 22.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 32.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if response.hovered() { bg_hover } else { bg };
    ui.painter().rect_filled(rect, CornerRadius::same(18), fill);
    let mut x = rect.left() + 16.0;
    if let Some(icon) = icon {
        let r = Rect::from_center_size(pos2(x + 9.0, rect.center().y), vec2(18.0, 18.0));
        if primary {
            icons::paint_filled(ui.painter(), r, icon, fg);
        } else {
            icons::paint(ui.painter(), r, icon, fg);
        }
        x += icon_w;
    }
    let y = rect.center().y - galley.size().y / 2.0;
    ui.painter().galley(pos2(x, y), galley, fg);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// O filtro da busca.
pub fn chip(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let font = theme::medium(13.5);
    let fg = if selected { Color32::BLACK } else { theme::TEXT };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, fg);
    let size = vec2(galley.size().x + 24.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = match (selected, response.hovered()) {
        (true, _) => theme::TEXT,
        (false, true) => theme::GLASS_HOVER,
        (false, false) => theme::GLASS,
    };
    let radius = if theme::panels() { 16 } else { 8 };
    ui.painter().rect_filled(rect, CornerRadius::same(radius), fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Uma linha só, cortada com "…" em `width`.
pub fn one_line(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping {
        max_width: width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// Até duas linhas, a segunda cortada com "…" em `width`.
pub fn two_lines(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), font, color, width.max(1.0));
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    ui.painter().layout_job(job)
}

/// Texto cortado que reage ao clique (o nome do artista numa linha).
/// Devolve a resposta só quando há para onde ir.
pub fn text_link(ui: &mut Ui, pos: Pos2, galley: Arc<Galley>, color: Color32, id: egui::Id, linked: bool) -> Option<Response> {
    let rect = Rect::from_min_size(pos, galley.size());
    if !linked {
        ui.painter().galley(pos, galley, color);
        return None;
    }
    let response = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let underline = response.hovered();
    let color = if underline { theme::TEXT } else { color };
    ui.painter().galley(pos, galley, color);
    if underline {
        let y = rect.bottom() - 1.0;
        ui.painter().hline(rect.x_range(), y, egui::Stroke::new(1.0, color));
    }
    Some(response)
}

/// 3:07, 1:02:45.
pub fn time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// 42 min, 3 h 12 min.
pub fn long_time(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs / 60) % 60);
    if h > 0 { format!("{h} h {m} min") } else { format!("{m} min") }
}

/// O círculo que gira enquanto algo carrega.
pub fn spinner(ui: &mut Ui, side: f32) {
    ui.add(egui::Spinner::new().size(side).color(theme::TEXT_DIM));
}

/// Abre o endereço no navegador padrão (o eframe daqui vem sem o
/// `webbrowser`, então quem abre é o sistema).
pub fn open_in_browser(url: &str) {
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(error) = spawned {
        log::warn!("navegador: {error}");
    }
}
