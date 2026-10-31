//! O mini player: a janela vira um controlador pequeno, sem bordas e sempre
//! no topo, como o player solto do Produtividade Prime. Capa, título e
//! artista, letra, anterior, tocar e próxima, e o botão que volta para a
//! janela inteira. Arrasta pelo fundo; dois cliques no fundo também voltam.
//!
//! A letra abre para cima: a janela cresce mantendo o pé no lugar, e os
//! botões não saem de baixo do mouse.
//!
//! O tamanho e a ordem dos botões vêm de "Personalizar" (`MiniLayout`).

use egui::{Color32, CornerRadius, CursorIcon, Id, Painter, Rect, Response, Sense, Stroke, StrokeKind, Ui, UiBuilder, ViewportCommand, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::{Command, Snapshot, Status};

use super::side::{self, LyricsState};
use super::{Action, Cx, icons, kit, theme};
use crate::look::{MiniButton, MiniLayout};

/// As medidas no tamanho 100%; a escala escolhida multiplica todas.
pub const WIDTH: f32 = 400.0;
/// A linha dos controles.
pub const ROW: f32 = 68.0;
/// A letra aberta, acima da linha.
const LYRICS: f32 = 170.0;
const PAD: f32 = 10.0;
const COVER: f32 = 48.0;
pub const FILL: Color32 = Color32::from_rgb(24, 24, 27);

pub fn width(scale: f32) -> f32 {
    (WIDTH * scale).round()
}

pub fn row_height(scale: f32) -> f32 {
    (ROW * scale).round()
}

pub fn height(lyrics: bool, scale: f32) -> f32 {
    row_height(scale) + if lyrics { (LYRICS * scale).round() } else { 0.0 }
}

/// Onde cada botão fica na linha (o centro em x e o lado) e os fios que
/// separam o "voltar à janela" dos vizinhos. O último encosta em `right`.
struct Slots {
    buttons: Vec<(MiniButton, f32, f32)>,
    separators: Vec<f32>,
}

fn side_of(b: MiniButton) -> f32 {
    if b == MiniButton::Play { 38.0 } else { 32.0 }
}

fn slots(order: &[MiniButton], right: f32, s: f32) -> Slots {
    let mut buttons = Vec::with_capacity(order.len());
    let mut separators = Vec::new();
    let mut x = right - 16.0 * s;
    for (i, b) in order.iter().enumerate().rev() {
        buttons.push((*b, x, side_of(*b) * s));
        if i == 0 {
            break;
        }
        let left = order[i - 1];
        let gap = if *b == MiniButton::Expand || left == MiniButton::Expand {
            separators.push(x - 23.0 * s);
            46.0
        } else if *b == MiniButton::Play || left == MiniButton::Play {
            40.0
        } else {
            36.0
        };
        x -= gap * s;
    }
    buttons.reverse();
    Slots { buttons, separators }
}

/// O círculo branco com tocar/pausar (ou o arco girando enquanto carrega).
fn paint_play(ui: &Ui, p: &Painter, r: Rect, s: f32, hovered: bool, enabled: bool, status: Status) {
    let fill = if hovered { Color32::from_gray(225) } else { theme::TEXT };
    p.circle_filled(r.center(), 19.0 * s, if enabled { fill } else { theme::TEXT_FAINT });
    if status == Status::Loading {
        // O arco girando no lugar do ícone, como na barra de baixo.
        let t = ui.input(|i| i.time) as f32;
        let arc = r.shrink(11.0 * s);
        let points: Vec<egui::Pos2> = (0..=20)
            .map(|i| {
                let a = t * 6.0 + i as f32 / 20.0 * std::f32::consts::PI * 1.5;
                arc.center() + arc.width() / 2.0 * vec2(a.cos(), a.sin())
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(2.5 * s, Color32::BLACK)));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    } else {
        let (icon, nudge) = if status == Status::Playing { (icons::PAUSE, 0.0) } else { (icons::PLAY, 1.5 * s) };
        icons::paint_filled(p, r.shrink(11.0 * s).translate(vec2(nudge, 0.0)), icon, Color32::BLACK);
    }
}

/// Um botão no seu lugar, sem tratar o clique.
#[allow(clippy::too_many_arguments)]
fn paint_button(ui: &Ui, p: &Painter, b: MiniButton, r: Rect, resp: &Response, s: f32, has: bool, status: Status, lyrics_open: bool) {
    let on = if has { theme::TEXT } else { theme::TEXT_FAINT };
    match b {
        MiniButton::Expand => kit::paint_icon_button(ui, r, resp, icons::MAXIMIZE_2, theme::TEXT_DIM),
        MiniButton::Next => kit::paint_icon_button(ui, r, resp, icons::SKIP_FORWARD, on),
        MiniButton::Previous => kit::paint_icon_button(ui, r, resp, icons::SKIP_BACK, on),
        MiniButton::Play => paint_play(ui, p, r, s, resp.hovered(), has, status),
        MiniButton::Lyrics => {
            if lyrics_open {
                p.circle_filled(r.center(), 16.0 * s, Color32::from_white_alpha(28));
            }
            kit::paint_icon_button(ui, r, resp, icons::MIC_VOCAL, if lyrics_open { theme::TEXT } else { theme::TEXT_DIM });
        }
    }
}

/// O que o mini player pede ao app (além das ações de sempre).
#[derive(Default)]
pub struct Out {
    pub toggle_lyrics: bool,
}

/// `backdrop`: o fundo personalizado ("Personalizar o player"); sem ele, o
/// cinza de sempre.
pub fn show(
    ui: &mut Ui,
    cx: &mut Cx,
    snap: &Snapshot,
    position: f64,
    lyrics: Option<Option<&LyricsState>>,
    backdrop: Option<&dyn Fn(&Ui, Rect)>,
    layout: &MiniLayout,
) -> Out {
    let mut out = Out::default();
    let s = layout.scale();
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

    let pad = PAD * s;
    let row = Rect::from_min_max(pos2(rect.left() + pad, rect.bottom() - row_height(s)), pos2(rect.right() - pad, rect.bottom()));
    let track = snap.current();

    if let Some(state) = lyrics {
        let area = Rect::from_min_max(pos2(rect.left() + pad, rect.top() + 8.0), pos2(rect.right() - pad, row.top() - 2.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(area).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(area);
        side::lyrics_body(&mut child, cx, snap, state, position, false);
        if layout.lyrics_divider {
            p.hline(area.x_range(), row.top() - 0.5, Stroke::new(1.0, Color32::from_white_alpha(18)));
        }
    }

    // Os botões, na ordem escolhida, encostados à direita.
    let cy = row.center().y;
    let has = track.is_some();
    let open = lyrics.is_some();
    let playing = snap.status == Status::Playing;
    let laid = slots(&layout.visible(), row.right(), s);
    for x in &laid.separators {
        p.vline(*x, (cy - 9.0 * s)..=(cy + 9.0 * s), Stroke::new(1.0, Color32::from_white_alpha(50)));
    }
    let mut buttons_left = row.right();
    for &(b, x, side) in &laid.buttons {
        let r = Rect::from_center_size(pos2(x, cy), vec2(side, side));
        buttons_left = buttons_left.min(r.left() - 6.0 * s);
        let resp = ui.interact(r, Id::new(("mini", b as u8)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        paint_button(ui, &p, b, r, &resp, s, has, snap.status, open);
        let tip = match b {
            MiniButton::Play if playing => t().pause,
            MiniButton::Lyrics if open => t().close_lyrics,
            MiniButton::Lyrics => t().show_lyrics,
            other => other.label(),
        };
        if resp.on_hover_text(tip).clicked() {
            match b {
                MiniButton::Expand => cx.act(Action::Mini(false)),
                MiniButton::Lyrics => out.toggle_lyrics = true,
                MiniButton::Next if has => cx.act(Action::Cmd(Command::Next)),
                MiniButton::Previous if has => cx.act(Action::Cmd(Command::Previous)),
                MiniButton::Play if has => cx.act(Action::Cmd(Command::Toggle)),
                _ => {}
            }
        }
    }

    // A capa e o texto à esquerda.
    let side = COVER * s;
    let cover = Rect::from_min_size(pos2(row.left(), cy - side / 2.0), vec2(side, side));
    let tx = cover.right() + 12.0 * s;
    let w = (buttons_left - tx).max(20.0);
    match track {
        Some(t) => {
            cx.covers.paint(ui, cover, &t.thumbs, (6.0 * s) as u8);
            let title = kit::one_line(ui, &t.title, theme::bold(15.0 * s), theme::TEXT, w);
            p.galley(pos2(tx, cy - 19.0 * s), title, theme::TEXT);
            let artist = kit::one_line(ui, &t.artist_names(), theme::regular(13.0 * s), theme::TEXT_DIM, w);
            p.galley(pos2(tx, cy + 2.0 * s), artist, theme::TEXT_DIM);
        }
        None => {
            p.rect_filled(cover, CornerRadius::same((6.0 * s) as u8), theme::PLACEHOLDER);
            icons::paint(&p, cover.shrink(14.0 * s), icons::MUSIC_2, theme::TEXT_FAINT);
            let text = kit::one_line(ui, t().nothing_playing, theme::medium(14.0 * s), theme::TEXT_DIM, w);
            p.galley(pos2(tx, cy - text.size().y / 2.0), text, theme::TEXT_DIM);
        }
    }

    // O progresso: um fio no pé da janela.
    let duration = snap.duration.or_else(|| track.and_then(|t| t.duration_secs.map(f64::from)));
    if let Some(d) = duration.filter(|d| *d > 0.0) {
        let f = (position / d).clamp(0.0, 1.0) as f32;
        let line = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), pos2(rect.left() + rect.width() * f, rect.bottom()));
        p.rect_filled(line, CornerRadius::ZERO, theme::accent());
    }
    out
}

/// A prévia em "Personalizar": a linha do mini player no tamanho escolhido
/// (reduzida se não couber em `width`), com os botões arrastáveis. A nova
/// ordem dos botões à mostra, quando a pessoa solta um em outro lugar.
pub fn preview(ui: &mut Ui, layout: &MiniLayout, width: f32) -> Option<Vec<MiniButton>> {
    let s = layout.scale().min(width / WIDTH);
    let size = vec2(WIDTH * s, ROW * s);
    let (outer, _) = ui.allocate_exact_size(vec2(width, size.y + 8.0), Sense::hover());
    let rect = Rect::from_center_size(outer.center(), size);
    let p = ui.painter().clone();
    p.rect_filled(rect, CornerRadius::same(10), FILL);
    p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, Color32::from_white_alpha(20)), StrokeKind::Inside);
    let row = rect.shrink2(vec2(PAD * s, 0.0));
    let cy = row.center().y;

    let visible = layout.visible();
    let drag_id = Id::new("mini-previa-arrastando");
    let dragging: Option<MiniButton> = ui.data(|d| d.get_temp(drag_id));
    let pointer_x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x);

    // Enquanto arrasta, os outros já abrem espaço onde o botão vai cair.
    let base = slots(&visible, row.right(), s);
    let order = match (dragging, pointer_x) {
        (Some(d), Some(px)) if visible.contains(&d) => {
            let mut rest: Vec<MiniButton> = visible.iter().copied().filter(|b| *b != d).collect();
            let at = base.buttons.iter().filter(|(b, x, _)| *b != d && *x < px).count();
            rest.insert(at.min(rest.len()), d);
            rest
        }
        _ => visible.clone(),
    };
    let laid = slots(&order, row.right(), s);
    for x in &laid.separators {
        p.vline(*x, (cy - 9.0 * s)..=(cy + 9.0 * s), Stroke::new(1.0, Color32::from_white_alpha(50)));
    }

    let mut result = None;
    let mut left = row.right();
    let mut lifted = None;
    for &(b, x, side) in &laid.buttons {
        // A área de pegar fica onde o botão estava antes de arrastar, para
        // o egui não perder o botão no meio do caminho.
        let home = base.buttons.iter().find(|(o, _, _)| *o == b).map_or(x, |(_, x, _)| *x);
        let hit = Rect::from_center_size(pos2(home, cy), vec2(side, side));
        let resp = ui.interact(hit, Id::new(("mini-previa", b as u8)), Sense::click_and_drag()).on_hover_cursor(CursorIcon::Grab).on_hover_text(b.label());
        if resp.drag_started() {
            ui.data_mut(|d| d.insert_temp(drag_id, b));
        }
        if resp.drag_stopped() {
            ui.data_mut(|d| d.remove::<MiniButton>(drag_id));
            if order != visible {
                result = Some(order.clone());
            }
        }
        let r = Rect::from_center_size(pos2(x, cy), vec2(side, side));
        left = left.min(r.left() - 6.0 * s);
        if dragging == Some(b) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            lifted = Some((b, r, resp));
            continue;
        }
        paint_button(ui, &p, b, r, &resp, s, true, Status::Paused, false);
    }
    // O que está sendo arrastado segue o mouse, por cima dos outros.
    if let (Some((b, r, resp)), Some(px)) = (lifted, pointer_x) {
        let x = px.clamp(row.left() + r.width() / 2.0, row.right() - r.width() / 2.0);
        let r = Rect::from_center_size(pos2(x, cy), r.size());
        p.circle_filled(r.center(), r.width() / 2.0 + 4.0 * s, Color32::from_white_alpha(30));
        paint_button(ui, &p, b, r, &resp, s, true, Status::Paused, false);
    }

    // Uma capa e um texto de mentira, para ver o espaço que sobra.
    let side = COVER * s;
    let cover = Rect::from_min_size(pos2(row.left(), cy - side / 2.0), vec2(side, side));
    p.rect_filled(cover, CornerRadius::same((6.0 * s) as u8), theme::PLACEHOLDER);
    icons::paint(&p, cover.shrink(14.0 * s), icons::MUSIC_2, theme::TEXT_FAINT);
    let tx = cover.right() + 12.0 * s;
    let w = (left - tx).max(0.0);
    let bar = |y: f32, len: f32, h: f32, alpha: u8| {
        let len = len.min(w);
        if len > 4.0 {
            p.rect_filled(Rect::from_min_size(pos2(tx, y), vec2(len, h)), CornerRadius::same(3), Color32::from_white_alpha(alpha));
        }
    };
    bar(cy - 14.0 * s, 110.0 * s, 10.0 * s, 70);
    bar(cy + 5.0 * s, 72.0 * s, 8.0 * s, 35);
    result
}
