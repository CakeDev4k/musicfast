//! O mini player: a janela vira um controlador pequeno, sem bordas e sempre
//! no topo, como o player solto do Produtividade Prime. Capa, título e
//! artista, letra, anterior, tocar e próxima, e o botão que volta para a
//! janela inteira. Arrasta pelo fundo; dois cliques no fundo também voltam.
//!
//! A letra abre para cima: a janela cresce mantendo o pé no lugar, e os
//! botões não saem de baixo do mouse.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, ViewportCommand, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::{Command, Snapshot, Status};

use super::side::{self, LyricsState};
use super::{Action, Cx, icons, kit, theme};

pub const WIDTH: f32 = 400.0;
/// A linha dos controles.
pub const ROW: f32 = 68.0;
/// A letra aberta, acima da linha.
pub const LYRICS: f32 = 170.0;
const PAD: f32 = 10.0;
const COVER: f32 = 48.0;
pub const FILL: Color32 = Color32::from_rgb(24, 24, 27);

pub fn height(lyrics: bool) -> f32 {
    if lyrics { ROW + LYRICS } else { ROW }
}

/// O que o mini player pede ao app (além das ações de sempre).
#[derive(Default)]
pub struct Out {
    pub toggle_lyrics: bool,
}

/// `backdrop`: o fundo personalizado ("Personalizar o player"); sem ele, o
/// cinza de sempre.
pub fn show(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, position: f64, lyrics: Option<Option<&LyricsState>>, backdrop: Option<&dyn Fn(&Ui, Rect)>) -> Out {
    let mut out = Out::default();
    let rect = ui.max_rect();
    let p = ui.painter().clone();
    match backdrop {
        Some(paint) => paint(ui, rect),
        None => {
            p.rect_filled(rect, CornerRadius::ZERO, FILL);
        }
    }
    p.rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_white_alpha(20)), StrokeKind::Inside);

    // O fundo arrasta a janela; os botões, desenhados depois, pegam o
    // clique antes dele.
    let bg = ui.interact(rect, Id::new("mini-fundo"), Sense::click_and_drag());
    if bg.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if bg.double_clicked() {
        cx.act(Action::Mini(false));
    }
    if bg.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }

    let row = Rect::from_min_max(pos2(rect.left() + PAD, rect.bottom() - ROW), pos2(rect.right() - PAD, rect.bottom()));
    let track = snap.current();

    if let Some(state) = lyrics {
        let area = Rect::from_min_max(pos2(rect.left() + PAD, rect.top() + 8.0), pos2(rect.right() - PAD, row.top() - 2.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(area);
        side::lyrics_body(&mut child, cx, snap, state, position, false);
        p.hline(area.x_range(), row.top() - 0.5, Stroke::new(1.0, Color32::from_white_alpha(18)));
    }

    // Os botões, da direita para a esquerda.
    let cy = row.center().y;
    let mut x = row.right() - 16.0;
    let button = |ui: &mut Ui, x: f32, key: &str, side: f32| {
        let r = Rect::from_center_size(pos2(x, cy), vec2(side, side));
        (r, ui.interact(r, Id::new(("mini", key)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand))
    };

    let (r, b) = button(ui, x, "voltar", 32.0);
    kit::paint_icon_button(ui, r, &b, icons::MAXIMIZE_2, theme::TEXT_DIM);
    if b.on_hover_text(t().back_to_window).clicked() {
        cx.act(Action::Mini(false));
    }
    x -= 24.0;
    p.vline(x, (cy - 9.0)..=(cy + 9.0), Stroke::new(1.0, Color32::from_white_alpha(50)));
    x -= 22.0;

    let has = track.is_some();
    let (r, b) = button(ui, x, "proxima", 32.0);
    kit::paint_icon_button(ui, r, &b, icons::SKIP_FORWARD, if has { theme::TEXT } else { theme::TEXT_FAINT });
    if b.on_hover_text(t().next).clicked() && has {
        cx.act(Action::Cmd(Command::Next));
    }
    x -= 40.0;

    let (r, b) = button(ui, x, "tocar", 38.0);
    let fill = if b.hovered() { Color32::from_gray(225) } else { theme::TEXT };
    p.circle_filled(r.center(), 19.0, if has { fill } else { theme::TEXT_FAINT });
    let playing = snap.status == Status::Playing;
    if snap.status == Status::Loading {
        // O arco girando no lugar do ícone, como na barra de baixo.
        let t = ui.input(|i| i.time) as f32;
        let arc = r.shrink(11.0);
        let points: Vec<egui::Pos2> = (0..=20)
            .map(|i| {
                let a = t * 6.0 + i as f32 / 20.0 * std::f32::consts::PI * 1.5;
                arc.center() + arc.width() / 2.0 * vec2(a.cos(), a.sin())
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(2.5, Color32::BLACK)));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    } else {
        let (icon, nudge) = if playing { (icons::PAUSE, 0.0) } else { (icons::PLAY, 1.5) };
        icons::paint_filled(&p, r.shrink(11.0).translate(vec2(nudge, 0.0)), icon, Color32::BLACK);
    }
    if b.on_hover_text(if playing { t().pause } else { t().play }).clicked() && has {
        cx.act(Action::Cmd(Command::Toggle));
    }
    x -= 40.0;

    let (r, b) = button(ui, x, "anterior", 32.0);
    kit::paint_icon_button(ui, r, &b, icons::SKIP_BACK, if has { theme::TEXT } else { theme::TEXT_FAINT });
    if b.on_hover_text(t().previous).clicked() && has {
        cx.act(Action::Cmd(Command::Previous));
    }
    x -= 36.0;

    let open = lyrics.is_some();
    let (r, b) = button(ui, x, "letra", 32.0);
    if open {
        p.circle_filled(r.center(), 16.0, Color32::from_white_alpha(28));
    }
    kit::paint_icon_button(ui, r, &b, icons::MIC_VOCAL, if open { theme::TEXT } else { theme::TEXT_DIM });
    if b.on_hover_text(if open { t().close_lyrics } else { t().show_lyrics }).clicked() {
        out.toggle_lyrics = true;
    }
    let buttons_left = r.left() - 6.0;

    // A capa e o texto à esquerda.
    let cover = Rect::from_min_size(pos2(row.left(), cy - COVER / 2.0), vec2(COVER, COVER));
    match track {
        Some(t) => {
            cx.covers.paint(ui, cover, &t.thumbs, 6);
            let tx = cover.right() + 12.0;
            let w = (buttons_left - tx).max(20.0);
            let title = kit::one_line(ui, &t.title, theme::bold(15.0), theme::TEXT, w);
            p.galley(pos2(tx, cy - 19.0), title, theme::TEXT);
            let artist = kit::one_line(ui, &t.artist_names(), theme::regular(13.0), theme::TEXT_DIM, w);
            p.galley(pos2(tx, cy + 2.0), artist, theme::TEXT_DIM);
        }
        None => {
            p.rect_filled(cover, CornerRadius::same(6), theme::PLACEHOLDER);
            icons::paint(&p, cover.shrink(14.0), icons::MUSIC_2, theme::TEXT_FAINT);
            p.text(pos2(cover.right() + 12.0, cy), Align2::LEFT_CENTER, t().nothing_playing, theme::medium(14.0), theme::TEXT_DIM);
        }
    }

    // O progresso: um fio no pé da janela.
    let duration = snap.duration.or_else(|| track.and_then(|t| t.duration_secs.map(f64::from)));
    if let Some(d) = duration.filter(|d| *d > 0.0) {
        let f = (position / d).clamp(0.0, 1.0) as f32;
        let line = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), pos2(rect.left() + rect.width() * f, rect.bottom()));
        p.rect_filled(line, CornerRadius::ZERO, theme::ACCENT);
    }
    out
}
