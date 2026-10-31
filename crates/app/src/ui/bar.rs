//! A barra do player, embaixo. Os botões vêm do arranjo escolhido pela pessoa
//! (`bar_layout`); aqui fica só onde cada coluna começa e como cada botão se
//! desenha. A de sempre, no arranjo do YouTube Music, tem a música no meio e
//! a linha do tempo como borda de cima da barra inteira; a dos painéis tem a
//! música à esquerda e a linha do tempo sob os controles.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::{Command, Repeat, Snapshot, Status};
use musicfast_ytm::LikeStatus;

use super::icons;
use super::kit;
use super::page::{download_button, track_menu};
use super::theme;
use super::{Action, Cx, Side, View};
use crate::bar_layout::{Item, Layout, Layouts, Zone, gap};
use crate::discord::Link;
use musicfast_player::offline::State as Download;

pub const HEIGHT: f32 = 72.0;
/// O cinza da barra do site (#212121), um tom acima do fundo da página.
pub const BG: Color32 = Color32::from_rgb(33, 33, 33);
/// O azul do Discord, com a música no perfil.
const DISCORD: Color32 = Color32::from_rgb(88, 101, 242);
/// Quanto os botões de voltar e avançar saltam (s).
const SEEK_STEP: f64 = 10.0;
/// A borda das colunas das pontas.
const EDGE: f32 = 12.0;
/// O mínimo que a música fica; os botões que passarem disso saem da barra.
const TRACK_MIN: f32 = 190.0;

pub struct BarState {
    /// Onde a linha do tempo está sendo arrastada (s).
    pub seeking: Option<f64>,
    /// O volume antes do mudo, para voltar a ele.
    pub unmuted: f32,
    /// Os botões de cada organização da janela.
    pub layouts: Layouts,
    /// A música no perfil do Discord: ligada, e como está a ligação.
    pub discord_on: bool,
    pub discord_link: Link,
}

/// O que todo botão precisa saber para se desenhar.
#[derive(Clone, Copy)]
struct Now<'a> {
    snap: &'a Snapshot,
    position: f64,
    duration: Option<f64>,
    side: Option<Side>,
    expanded: bool,
    panels: bool,
}

/// Os botões que aparecem de verdade numa janela de `width`.
pub fn shown(layout: &Layout, panels: bool, signed_in: bool, width: f32) -> Layout {
    layout.fitted(panels, signed_in, width - EDGE * 2.0, TRACK_MIN)
}

/// O clique com o botão direito no vazio da barra leva ao editor: é o caminho
/// que sobra quando o ⋮ foi tirado dela.
fn bar_menu(response: &egui::Response, cx: &mut Cx) {
    response.context_menu(|ui| {
        if menu_row(ui, icons::PANEL_BOTTOM, t().bar_buttons) {
            cx.act(Action::CustomizeBar);
        }
    });
}

/// Os botões de uma coluna, da esquerda para a direita a partir de `x`.
#[allow(clippy::too_many_arguments)]
fn zone(ui: &mut Ui, cx: &mut Cx, layout: &Layout, which: Zone, x: f32, cy: f32, now: &Now, state: &mut BarState) {
    let mut x = x;
    let hover_volume = layout.hover_volume(now.panels);
    if which == Zone::Right && hover_volume {
        x += crate::bar_layout::HOVER_SLIDER_W;
    }
    let gap = gap(which, now.panels);
    for &item in layout.zone(which) {
        let w = layout.item_width(item, now.panels);
        button(ui, cx, item, pos2(x, cy), w, !hover_volume, now, state);
        x += w + gap;
    }
}

pub fn show(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, position: f64, side: Option<Side>, expanded: bool, state: &mut BarState) {
    let rect = ui.max_rect();
    let duration = snap.duration.or_else(|| snap.current().and_then(|t| t.duration_secs.map(f64::from)));
    let layout = shown(&state.layouts.classic, false, cx.me.signed_in, rect.width());
    let now = Now { snap, position, duration, side, expanded, panels: false };
    let left = Rect::from_min_max(rect.min, pos2(rect.left() + EDGE + layout.zone_width(Zone::Left, false), rect.bottom()));
    let right = Rect::from_min_max(pos2(rect.right() - EDGE - layout.zone_width(Zone::Right, false), rect.top()), rect.max);
    let center = Rect::from_min_max(pos2(left.right(), rect.top()), pos2(right.left(), rect.bottom()));

    // O fundo da barra abre (ou fecha) o player com a fila, como no site.
    // Vem antes de tudo: botões, links e a linha do tempo, registrados
    // depois, ficam por cima e pegam o clique primeiro; só o vazio sobra
    // para cá.
    let back = ui.interact(rect, Id::new("barra-fundo"), Sense::click());
    if back.clicked() && snap.current().is_some() {
        cx.act(Action::Expand(!expanded));
    }
    bar_menu(&back, cx);

    let cy = rect.center().y;
    zone(ui, cx, &layout, Zone::Left, rect.left() + EDGE, cy, &now, state);
    now_playing(ui, cx, center, &layout, &now, state);
    zone(ui, cx, &layout, Zone::Right, right.left(), cy, &now, state);
    // Por último: a linha do tempo fica por cima e pega o mouse primeiro.
    timeline(ui, cx, rect, position, duration, snap.current().is_some(), state);
}

/// A altura da barra nos painéis.
pub const PANELS_HEIGHT: f32 = 80.0;

/// A barra nos painéis: a música à esquerda; os controles e a linha do tempo
/// no meio; os outros botões à direita.
pub fn show_panels(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, position: f64, side: Option<Side>, expanded: bool, state: &mut BarState) {
    let rect = ui.max_rect();
    let duration = snap.duration.or_else(|| snap.current().and_then(|t| t.duration_secs.map(f64::from)));
    let layout = shown(&state.layouts.panels, true, cx.me.signed_in, rect.width());
    let now = Now { snap, position, duration, side, expanded, panels: true };
    let right_w = layout.zone_width(Zone::Right, true) + EDGE + 4.0;
    let left_w = layout.zone_width(Zone::Left, true) + TRACK_MIN.min(rect.width() * 0.2);
    let third = (rect.width() * 0.3).clamp(200.0, 420.0).max(right_w).max(left_w);
    let left = Rect::from_min_max(rect.min, pos2(rect.left() + third, rect.bottom()));
    let right = Rect::from_min_max(pos2(rect.right() - third, rect.top()), rect.max);
    let center = Rect::from_min_max(pos2(left.right(), rect.top()), pos2(right.left(), rect.bottom()));
    bar_menu(&ui.interact(rect, Id::new("barra-fundo"), Sense::click()), cx);
    panels_track(ui, cx, left, &layout, &now, state);
    panels_controls(ui, cx, center, &layout, &now, state);
    let x = rect.right() - EDGE - layout.zone_width(Zone::Right, true);
    zone(ui, cx, &layout, Zone::Right, x, rect.center().y, &now, state);
}

fn panels_track(ui: &mut Ui, cx: &mut Cx, rect: Rect, layout: &Layout, now: &Now, state: &mut BarState) {
    let Some(track) = now.snap.current() else { return };
    let cy = rect.center().y;
    let art = Rect::from_min_size(pos2(rect.left() + 16.0, cy - 28.0), vec2(56.0, 56.0));
    cx.covers.paint(ui, art, &track.thumbs, 4);
    let cover = ui.interact(art, Id::new("barra-capa"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    if cover.on_hover_text(t().now_playing).clicked() {
        cx.act(Action::Expand(!now.expanded));
    }
    let x = art.right() + 14.0;
    let buttons_w = layout.zone_width(Zone::Left, true);
    let width = (rect.right() - x - buttons_w - 12.0).max(40.0);
    let title = kit::one_line(ui, &track.title, theme::medium(14.0), theme::TEXT, width);
    let title_w = title.size().x;
    let album_id = track.album.as_ref().and_then(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(x, cy - 20.0), title, theme::TEXT, Id::new("barra-titulo"), album_id.is_some())
        && r.clicked()
        && let Some(id) = &album_id
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
    let sub = kit::one_line(ui, &track.artist_names(), theme::regular(12.5), theme::TEXT_DIM, width);
    let artist_id = track.artists.iter().find_map(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(x, cy + 2.0), sub, theme::TEXT_DIM, Id::new("barra-artista"), artist_id.is_some())
        && r.clicked()
        && let Some(id) = &artist_id
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
    zone(ui, cx, layout, Zone::Left, x + title_w + 8.0, cy, now, state);
}

fn panels_controls(ui: &mut Ui, cx: &mut Cx, rect: Rect, layout: &Layout, now: &Now, state: &mut BarState) {
    let has = now.snap.current().is_some();
    let cx_x = rect.center().x;
    zone(ui, cx, layout, Zone::Center, cx_x - layout.zone_width(Zone::Center, true) / 2.0, rect.top() + 28.0, now, state);

    // A linha do tempo, com o tempo de cada lado.
    let (position, duration) = (now.position, now.duration);
    let y = rect.top() + 60.0;
    let width = rect.width().min(620.0) - 96.0;
    let line = Rect::from_center_size(pos2(cx_x, y), vec2(width.max(80.0), 12.0));
    let shown = state.seeking.unwrap_or(position);
    let side_text = |text: String, at: Pos2, align: Align2| {
        ui.painter().text(at, align, text, theme::regular(12.0), theme::TEXT_DIM);
    };
    side_text(if has { kit::time(shown) } else { "-:--".to_owned() }, pos2(line.left() - 10.0, y), Align2::RIGHT_CENTER);
    side_text(duration.filter(|_| has).map_or_else(|| "-:--".to_owned(), kit::time), pos2(line.right() + 10.0, y), Align2::LEFT_CENTER);
    progress(ui, cx, line, position, duration.filter(|d| *d > 0.0 && has), state);
}

/// Um botão de ligar e desligar: da cor do tema com o pontinho embaixo.
fn switch_button(ui: &Ui, rect: Rect, response: &egui::Response, icon: icons::Icon, on: bool, panels: bool) {
    let color = match (on, response.hovered()) {
        (true, _) if panels => theme::accent(),
        (true, _) | (false, true) => theme::TEXT,
        (false, false) => theme::TEXT_DIM,
    };
    if !panels && response.hovered() {
        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, Color32::from_white_alpha(18));
    }
    let (side, dot_y, dot) = if panels { (18.0, 14.0, 1.5) } else { (22.0, 15.0, 2.0) };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(side, side)), icon, color);
    if on {
        ui.painter().circle_filled(rect.center() + vec2(0.0, dot_y), dot, color);
    }
}

/// A linha fina do tempo: branca, fica verde e ganha a bolinha sob o mouse.
fn progress(ui: &mut Ui, cx: &mut Cx, area: Rect, position: f64, duration: Option<f64>, state: &mut BarState) {
    let response = ui.interact(area, Id::new("linha-do-tempo"), Sense::click_and_drag());
    let hovered = response.hovered() || response.dragged();
    let line = Rect::from_center_size(area.center(), vec2(area.width(), 4.0));
    ui.painter().rect_filled(line, CornerRadius::same(2), Color32::from_white_alpha(60));
    let Some(duration) = duration else { return };
    let at_pointer = |x: f32| (((x - area.left()) / area.width()).clamp(0.0, 1.0) as f64) * duration;
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.is_pointer_button_down_on())
    {
        state.seeking = Some(at_pointer(p.x));
    }
    if (response.drag_stopped() || response.clicked())
        && let Some(target) = state.seeking.take().or_else(|| response.interact_pointer_pos().map(|p| at_pointer(p.x)))
    {
        cx.act(Action::Cmd(Command::Seek(target)));
    }
    let shown = state.seeking.unwrap_or(position).min(duration);
    let x = line.left() + line.width() * (shown / duration) as f32;
    let color = if hovered { theme::accent() } else { theme::TEXT };
    ui.painter().rect_filled(Rect::from_min_max(line.min, pos2(x, line.bottom())), CornerRadius::same(2), color);
    if hovered {
        ui.painter().circle_filled(pos2(x, line.center().y), 6.0, theme::TEXT);
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
}

/// Uma área clicável quadrada centrada em `center`.
fn hit(ui: &mut Ui, center: Pos2, side: f32, id: impl std::hash::Hash + std::fmt::Debug) -> (Rect, egui::Response) {
    let r = Rect::from_center_size(center, vec2(side, side));
    (r, ui.interact(r, Id::new(id), Sense::click()).on_hover_cursor(CursorIcon::PointingHand))
}

/// O ícone com o círculo claro sob o mouse, como os botões do site.
fn round_button(ui: &Ui, rect: Rect, response: &egui::Response, icon: icons::Icon, color: Color32, filled: bool, icon_side: f32) {
    if response.hovered() {
        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, Color32::from_white_alpha(18));
    }
    let r = Rect::from_center_size(rect.center(), vec2(icon_side, icon_side));
    if filled {
        icons::paint_filled(ui.painter(), r, icon, color);
    } else {
        icons::paint(ui.painter(), r, icon, color);
    }
}

/// A linha do tempo ocupa a borda de cima da barra inteira, como no site:
/// fina e vermelha, engrossa com a bolinha sob o mouse.
fn timeline(ui: &mut Ui, cx: &mut Cx, bar: Rect, position: f64, duration: Option<f64>, active: bool, state: &mut BarState) {
    let hit = Rect::from_min_max(bar.min - vec2(0.0, 6.0), pos2(bar.right(), bar.top() + 8.0));
    let response = ui.interact(hit, Id::new("linha-do-tempo"), Sense::click_and_drag());
    let hovered = response.hovered() || response.dragged();
    // A linha fica na borda de cima da barra: metade dela, a bolinha e a
    // dica do tempo caem fora do painel e o recorte dele as cortava. Numa
    // camada própria, acima dos painéis (e abaixo de menus e diálogos),
    // nada as corta.
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, Id::new("linha-do-tempo-camada")));
    let line_h = if hovered { 4.0 } else { 2.0 };
    let line = Rect::from_min_max(pos2(bar.left(), bar.top() - line_h / 2.0), pos2(bar.right(), bar.top() + line_h / 2.0));
    painter.rect_filled(line, CornerRadius::ZERO, Color32::from_white_alpha(28));
    let Some(duration) = duration.filter(|d| *d > 0.0 && active) else { return };

    let at_pointer = |x: f32| (((x - bar.left()) / bar.width()).clamp(0.0, 1.0) as f64) * duration;
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.is_pointer_button_down_on())
    {
        state.seeking = Some(at_pointer(p.x));
    }
    if (response.drag_stopped() || response.clicked())
        && let Some(target) = state.seeking.take().or_else(|| response.interact_pointer_pos().map(|p| at_pointer(p.x)))
    {
        cx.act(Action::Cmd(Command::Seek(target)));
    }
    let shown = state.seeking.unwrap_or(position).min(duration);
    let x = bar.left() + bar.width() * (shown / duration) as f32;
    let done = Rect::from_min_max(line.min, pos2(x, line.bottom()));
    painter.rect_filled(done, CornerRadius::ZERO, theme::accent());
    if hovered {
        painter.circle_filled(pos2(x, bar.top()), 7.0, theme::accent());
        if let Some(p) = response.hover_pos() {
            let at = state.seeking.unwrap_or_else(|| at_pointer(p.x));
            let tip = Rect::from_center_size(pos2(p.x.clamp(bar.left() + 24.0, bar.right() - 24.0), bar.top() - 20.0), vec2(48.0, 20.0));
            painter.rect_filled(tip, CornerRadius::same(4), theme::ACTIVE);
            painter.text(tip.center(), Align2::CENTER_CENTER, kit::time(at), theme::regular(12.0), theme::TEXT);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
}

/// O que cada botão faz e como se desenha, com a esquerda dele em `at`.
#[allow(clippy::too_many_arguments)]
fn button(ui: &mut Ui, cx: &mut Cx, item: Item, at: Pos2, width: f32, fixed_volume: bool, now: &Now, state: &mut BarState) {
    let Now { snap, position, duration, side, expanded, panels } = *now;
    let has = snap.current().is_some();
    let dim = if !has {
        theme::TEXT_FAINT
    } else if panels {
        theme::TEXT_SUB
    } else {
        theme::TEXT
    };
    let (hit_side, icon_side) = if panels { (32.0, 18.0) } else { (40.0, 22.0) };
    let center = pos2(at.x + width / 2.0, at.y);
    match item {
        Item::Previous | Item::Next => {
            let (icon, tip, key, cmd) = if item == Item::Previous {
                (icons::SKIP_BACK, t().previous, "anterior", Command::Previous)
            } else {
                (icons::SKIP_FORWARD, t().next, "proxima", Command::Next)
            };
            let (r, resp) = hit(ui, center, hit_side, ("ctrl", key));
            round_button(ui, r, &resp, icon, dim, true, icon_side);
            if resp.on_hover_text(tip).clicked() {
                cx.act(Action::Cmd(cmd));
            }
        }
        Item::PlayPause => play_pause(ui, cx, center, snap, panels),
        Item::Shuffle => {
            let (r, resp) = hit(ui, center, hit_side, ("ctrl", "aleatorio"));
            switch_button(ui, r, &resp, icons::SHUFFLE, snap.shuffle, panels);
            if resp.on_hover_text(if snap.shuffle { t().shuffle_on } else { t().shuffle }).clicked() {
                cx.act(Action::Cmd(Command::SetShuffle(!snap.shuffle)));
            }
        }
        Item::Repeat => {
            let (r, resp) = hit(ui, center, hit_side, ("ctrl", "repetir"));
            let (icon, on, next, tip) = match snap.repeat {
                Repeat::Off => (icons::REPEAT, false, Repeat::All, t().repeat_queue),
                Repeat::All => (icons::REPEAT, true, Repeat::One, t().repeat_one),
                Repeat::One => (icons::REPEAT_1, true, Repeat::Off, t().no_repeat),
            };
            switch_button(ui, r, &resp, icon, on, panels);
            if resp.on_hover_text(tip).clicked() {
                cx.act(Action::Cmd(Command::SetRepeat(next)));
            }
        }
        Item::Time => {
            // O tempo, ou o que está demorando.
            let text = match (snap.status, snap.step) {
                (Status::Loading, Some(step)) => step.to_owned(),
                _ if has => match duration {
                    Some(d) => format!("{} / {}", kit::time(position.min(d)), kit::time(d)),
                    None => kit::time(position),
                },
                _ => String::new(),
            };
            let galley = kit::one_line(ui, &text, theme::regular(12.5), theme::TEXT_DIM, width);
            ui.painter().galley(pos2(at.x, at.y - galley.size().y / 2.0), galley, theme::TEXT_DIM);
        }
        Item::Like => {
            let Some(track) = snap.current() else { return };
            let on = cx.me.like(&track.video_id) == LikeStatus::Like;
            let (r, resp) = hit(ui, center, hit_side, ("gostei", true));
            if panels {
                let inner = r.shrink(8.0);
                if on {
                    icons::paint_filled(ui.painter(), inner, icons::HEART, theme::accent());
                } else {
                    icons::paint(ui.painter(), inner, icons::HEART, if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM });
                }
            } else {
                round_button(ui, r, &resp, icons::THUMBS_UP, theme::TEXT, on, icon_side);
            }
            if resp.on_hover_text(t().liked).clicked() {
                cx.act(Action::Rate(track.video_id.clone(), if on { LikeStatus::Indifferent } else { LikeStatus::Like }));
            }
        }
        Item::Dislike => {
            let Some(track) = snap.current() else { return };
            let on = cx.me.like(&track.video_id) == LikeStatus::Dislike;
            let (r, resp) = hit(ui, center, hit_side, ("gostei", false));
            round_button(ui, r, &resp, icons::THUMBS_DOWN, theme::TEXT, on, icon_side);
            if resp.on_hover_text(t().disliked).clicked() {
                cx.act(Action::Rate(track.video_id.clone(), if on { LikeStatus::Indifferent } else { LikeStatus::Dislike }));
            }
        }
        Item::Menu => track_popup(ui, cx, center, hit_side, icon_side, snap, side),
        Item::Volume => {
            let icon_at = if fixed_volume { pos2(at.x + 20.0, at.y) } else { center };
            volume(ui, cx, icon_at, snap, state, fixed_volume);
        }
        Item::Queue | Item::Lyrics => {
            let (icon, which, show, close, key) = if item == Item::Queue {
                (icons::LIST_MUSIC, Side::Queue, t().show_queue, t().close_queue, "barra-fila")
            } else {
                (icons::MIC_VOCAL, Side::Lyrics, t().show_lyrics, t().close_lyrics, "barra-letra")
            };
            let (r, resp) = hit(ui, center, hit_side, key);
            let on = side == Some(which);
            switch_button(ui, r, &resp, icon, on, panels);
            if resp.on_hover_text(if on { close } else { show }).clicked() {
                cx.act(Action::ToggleSide(which));
            }
        }
        Item::Mini => {
            let (r, resp) = hit(ui, center, hit_side, "barra-mini-botao");
            switch_button(ui, r, &resp, icons::PIN, false, panels);
            if resp.on_hover_text(t().pin_mini_tip).clicked() {
                cx.act(Action::Mini(true));
            }
        }
        Item::Expand => {
            let (r, resp) = hit(ui, center, hit_side, "expandir");
            if panels {
                let icon = if expanded { icons::MINIMIZE_2 } else { icons::MAXIMIZE_2 };
                switch_button(ui, r, &resp, icon, false, true);
            } else {
                // O triângulo cheio do site: para cima abre, para baixo fecha.
                if resp.hovered() {
                    ui.painter().circle_filled(r.center(), 20.0, Color32::from_white_alpha(18));
                }
                let c = r.center();
                let tri = if expanded {
                    vec![c + vec2(-9.0, -4.5), c + vec2(9.0, -4.5), c + vec2(0.0, 5.5)]
                } else {
                    vec![c + vec2(-9.0, 4.5), c + vec2(9.0, 4.5), c + vec2(0.0, -5.5)]
                };
                let color = if has { theme::TEXT } else { theme::TEXT_FAINT };
                ui.painter().add(Shape::convex_polygon(tri, color, Stroke::NONE));
            }
            if resp.on_hover_text(if expanded { t().close_player } else { t().open_player }).clicked() && has {
                cx.act(Action::Expand(!expanded));
            }
        }
        Item::Discord => {
            let (r, resp) = hit(ui, center, hit_side, "barra-discord");
            if resp.hovered() {
                ui.painter().circle_filled(r.center(), r.width() / 2.0, Color32::from_white_alpha(18));
            }
            let on = state.discord_on;
            let live = on && state.discord_link == Link::Connected;
            let color = match (live, resp.hovered()) {
                (true, _) => DISCORD,
                (false, true) => theme::TEXT,
                (false, false) => theme::TEXT_DIM,
            };
            icons::paint_discord(ui.painter(), Rect::from_center_size(r.center(), vec2(icon_side, icon_side)), color);
            if on {
                let (dot_y, dot) = if panels { (14.0, 1.5) } else { (15.0, 2.0) };
                ui.painter().circle_filled(r.center() + vec2(0.0, dot_y), dot, color);
            }
            let tip = match (on, state.discord_link) {
                (false, _) => t().discord_show,
                (true, Link::Connected) => t().discord_showing,
                (true, Link::Refused) => t().discord_refused,
                (true, _) => t().discord_waiting,
            };
            if resp.on_hover_text(tip).clicked() {
                cx.act(Action::ToggleDiscord);
            }
        }
        Item::Download => {
            let Some(track) = snap.current() else { return };
            let state = cx.download(&track.video_id);
            if let Download::Done(path) = &state {
                // Já baixada: o botão leva ao arquivo.
                let (r, resp) = hit(ui, center, hit_side, "barra-baixada");
                round_button(ui, r, &resp, icons::CHECK, dim, false, icon_side);
                if resp.on_hover_text(t().show_in_folder).clicked() {
                    cx.act(Action::Reveal(path.clone()));
                }
            } else {
                let rect = Rect::from_center_size(center, vec2(hit_side, hit_side));
                download_button(ui, cx, rect, Id::new("barra-baixar"), track, &state);
            }
        }
        Item::Save | Item::Radio | Item::CopyLink | Item::Album | Item::Artist => {
            let Some(track) = snap.current() else { return };
            let album = track.album.as_ref().and_then(|a| a.id.as_ref());
            let artist = track.artists.iter().find_map(|a| a.id.as_ref());
            let (icon, tip, ready) = match item {
                Item::Save => (icons::BOOKMARK_PLUS, t().save_to_playlist, true),
                Item::Radio => (icons::RADIO, t().start_radio, true),
                Item::CopyLink => (icons::LINK, t().copy_link, true),
                Item::Album => (icons::DISC_3, t().go_to_album, album.is_some()),
                _ => (icons::USER, t().go_to_artist, artist.is_some()),
            };
            let (r, resp) = hit(ui, center, hit_side, ("barra-musica", item));
            round_button(ui, r, &resp, icon, if ready { dim } else { theme::TEXT_FAINT }, false, icon_side);
            if resp.on_hover_text(tip).clicked() && ready {
                cx.act(match item {
                    Item::Save => Action::Dialog(super::Dialog::SaveTo { track: track.clone() }),
                    Item::Radio => Action::Cmd(Command::PlayRadio { seed: track.clone() }),
                    Item::CopyLink => Action::CopyLink(track.video_id.clone()),
                    Item::Album => Action::Open(View::browse(album.map_or("", String::as_str), None)),
                    _ => Action::Open(View::browse(artist.map_or("", String::as_str), None)),
                });
            }
        }
        Item::SeekBack | Item::SeekForward => {
            let (icon, tip, step) = if item == Item::SeekBack {
                (icons::REWIND, t().item_seek_back, -SEEK_STEP)
            } else {
                (icons::FAST_FORWARD, t().item_seek_forward, SEEK_STEP)
            };
            let (r, resp) = hit(ui, center, hit_side, ("barra-saltar", item));
            round_button(ui, r, &resp, icon, dim, false, icon_side);
            if resp.on_hover_text(tip).clicked() && has {
                let target = (position + step).max(0.0);
                cx.act(Action::Cmd(Command::Seek(duration.map_or(target, |d| target.min(d)))));
            }
        }
        Item::Personalize => {
            let (r, resp) = hit(ui, center, hit_side, "barra-personalizar");
            round_button(ui, r, &resp, icons::PALETTE, dim, false, icon_side);
            if resp.on_hover_text(t().customize_player).clicked() {
                cx.act(Action::Personalize);
            }
        }
    }
}

/// Tocar/pausar: o triângulo grande sem círculo, ou o círculo branco dos
/// painéis. Na espera do yt-dlp, um arco gira no lugar do ícone.
fn play_pause(ui: &mut Ui, cx: &mut Cx, center: Pos2, snap: &Snapshot, panels: bool) {
    let has = snap.current().is_some();
    let (r, resp) = hit(ui, center, if panels { 36.0 } else { 52.0 }, ("ctrl", "tocar"));
    let color = if panels {
        let radius = if resp.hovered() { 18.0 } else { 17.0 };
        ui.painter().circle_filled(r.center(), radius, if has { theme::TEXT } else { theme::TEXT_FAINT });
        Color32::BLACK
    } else {
        if resp.hovered() {
            ui.painter().circle_filled(r.center(), 26.0, Color32::from_white_alpha(18));
        }
        if has { theme::TEXT } else { theme::TEXT_FAINT }
    };
    match snap.status {
        Status::Loading => {
            let t = ui.ctx().input(|i| i.time) as f32;
            let (arc, line) = if panels { (8.0, 2.0) } else { (12.0, 3.0) };
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = t * 6.0 + i as f32 / 24.0 * std::f32::consts::PI * 1.5;
                    r.center() + vec2(a.cos(), a.sin()) * arc
                })
                .collect();
            ui.painter().add(Shape::line(points, Stroke::new(line, color)));
            // ~30 quadros por segundo bastam para o arco girar liso. Sem o
            // prazo, a janela inteira era refeita na taxa do monitor (144 Hz
            // num de 144) durante toda a resolução do yt-dlp, que na primeira
            // instalação passa de um minuto.
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
        Status::Playing => {
            let side = if panels { 18.0 } else { 34.0 };
            icons::paint_filled(ui.painter(), Rect::from_center_size(r.center(), vec2(side, side)), icons::PAUSE, color);
        }
        _ => {
            let (side, nudge) = if panels { (18.0, 1.0) } else { (36.0, 2.0) };
            icons::paint_filled(ui.painter(), Rect::from_center_size(r.center() + vec2(nudge, 0.0), vec2(side, side)), icons::PLAY, color);
        }
    }
    let tip = if snap.status == Status::Playing { t().pause_tip } else { t().play_tip };
    if resp.on_hover_text(tip).clicked() {
        cx.act(Action::Cmd(Command::Toggle));
    }
}

/// No meio: capa, título e "Artista • Álbum", e os botões junto da música.
fn now_playing(ui: &mut Ui, cx: &mut Cx, rect: Rect, layout: &Layout, now: &Now, state: &mut BarState) {
    let Some(track) = now.snap.current() else { return };
    const ART: f32 = 40.0;
    let buttons_w = layout.zone_width(Zone::Center, false);
    // O bloco é centrado; o texto ganha o que couber.
    let text_max = (rect.width() - ART - 16.0 - buttons_w - 16.0 - 24.0).max(60.0);
    let title = kit::one_line(ui, &track.title, theme::medium(15.5), theme::TEXT, text_max);
    let mut line = track.artist_names();
    if let Some(album) = &track.album {
        line = format!("{line} • {}", album.name);
    }
    let sub = kit::one_line(ui, &line, theme::regular(14.0), theme::TEXT_DIM, text_max);
    let text_w = title.size().x.max(sub.size().x);
    let block_w = ART + 16.0 + text_w + 16.0 + buttons_w;
    let x0 = (rect.center().x - block_w / 2.0).max(rect.left() + 12.0);
    let cy = rect.center().y;

    let art = Rect::from_min_size(pos2(x0, cy - ART / 2.0), vec2(ART, ART));
    cx.covers.paint(ui, art, &track.thumbs, 2);
    let art_click = ui.interact(art, Id::new("barra-capa"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    if art_click.on_hover_text(t().now_playing).clicked() {
        cx.act(Action::Expand(!now.expanded));
    }

    let tx = art.right() + 16.0;
    // Com o mouse sobre a capa e o texto, o botãozinho do mini player
    // aparece no canto da capa (depois da capa: pega o clique antes dela).
    let block = Rect::from_min_max(pos2(x0 - 6.0, rect.top()), pos2(tx + text_w + 8.0, rect.bottom()));
    if ui.rect_contains_pointer(block) {
        let r = Rect::from_center_size(art.right_top() + vec2(-3.0, 3.0), vec2(20.0, 20.0));
        let resp = ui.interact(r, Id::new("barra-mini"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let fill = if resp.hovered() { Color32::from_gray(70) } else { Color32::from_rgba_unmultiplied(20, 20, 20, 230) };
        ui.painter().circle_filled(r.center(), 10.0, fill);
        ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.0, Color32::from_white_alpha(40)));
        icons::paint(ui.painter(), r.shrink(4.5), icons::PIN, theme::TEXT);
        if resp.on_hover_text(t().pin_mini_tip).clicked() {
            cx.act(Action::Mini(true));
        }
    }
    let title_h = title.size().y;
    let album_id = track.album.as_ref().and_then(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(tx, cy - title_h - 1.0), title, theme::TEXT, Id::new("barra-titulo"), album_id.is_some())
        && r.clicked()
        && let Some(id) = &album_id
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
    let artist_id = track.artists.iter().find_map(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(tx, cy + 1.0), sub, theme::TEXT_DIM, Id::new("barra-artista"), artist_id.is_some())
        && r.clicked()
        && let Some(id) = &artist_id
    {
        cx.act(Action::Open(View::browse(id, None)));
    }

    zone(ui, cx, layout, Zone::Center, tx + text_w + 16.0, cy, now, state);
}

/// O menu ⋮: as ações da música, e a fila e a letra.
fn track_popup(ui: &mut Ui, cx: &mut Cx, center: Pos2, hit_side: f32, icon_side: f32, snap: &Snapshot, side: Option<Side>) {
    let Some(track) = snap.current() else { return };
    let (r, resp) = hit(ui, center, hit_side, "barra-menu");
    round_button(ui, r, &resp, icons::ELLIPSIS_VERTICAL, theme::TEXT, false, icon_side);
    let track = track.clone();
    egui::Popup::menu(&resp).align(egui::RectAlign::TOP_START).width(super::page::MENU_W).show(|ui| {
        track_menu(ui, cx, &track);
        ui.separator();
        for (icon, show, close, which) in [
            (icons::LIST_MUSIC, t().show_queue, t().close_queue, Side::Queue),
            (icons::MIC_VOCAL, t().show_lyrics, t().close_lyrics, Side::Lyrics),
        ] {
            let label = if side == Some(which) { close } else { show };
            if menu_row(ui, icon, label) {
                cx.act(Action::ToggleSide(which));
            }
        }
        if menu_row(ui, icons::PIN, t().pin_mini) {
            cx.act(Action::Mini(true));
        }
        if menu_row(ui, icons::PALETTE, t().customize_player) {
            cx.act(Action::Personalize);
        }
        if menu_row(ui, icons::PANEL_BOTTOM, t().bar_buttons) {
            cx.act(Action::CustomizeBar);
        }
    });
}

fn menu_row(ui: &mut Ui, icon: icons::Icon, text: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(super::page::MENU_W, 34.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), theme::HOVER);
    }
    let r = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0));
    icons::paint(ui.painter(), r, icon, theme::TEXT_DIM);
    ui.painter().text(pos2(rect.left() + 38.0, rect.center().y), Align2::LEFT_CENTER, text, theme::regular(14.0), theme::TEXT);
    let clicked = response.on_hover_cursor(CursorIcon::PointingHand).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// O ícone do volume; com o mouse em cima (ou arrastando), a barrinha abre
/// à esquerda dele. Clicar no ícone emudece. `fixed`: a barrinha fica sempre
/// à vista, à direita do ícone (os painéis).
fn volume(ui: &mut Ui, cx: &mut Cx, icon_at: Pos2, snap: &Snapshot, state: &mut BarState, fixed: bool) {
    const SLIDER_W: f32 = 96.0;
    let open_id = Id::new("volume-aberto");
    let was_open = fixed || ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let start = if fixed { icon_at.x + 22.0 } else { icon_at.x - 22.0 - SLIDER_W };
    let slider = Rect::from_min_max(pos2(start, icon_at.y - 10.0), pos2(start + SLIDER_W, icon_at.y + 10.0));
    let zone = Rect::from_min_max(slider.min - vec2(8.0, 10.0), pos2(icon_at.x + 22.0, icon_at.y + 20.0));

    let mut volume = snap.volume;
    let mut dragging = false;
    if was_open {
        let resp = ui.interact(slider, Id::new("volume"), Sense::click_and_drag()).on_hover_cursor(CursorIcon::PointingHand);
        dragging = resp.dragged();
        if let Some(p) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on())
        {
            volume = ((p.x - slider.left()) / slider.width()).clamp(0.0, 1.0);
            if (volume - snap.volume).abs() > 0.001 {
                cx.act(Action::Cmd(Command::SetVolume(volume)));
            }
        }
        let track = Rect::from_center_size(slider.center(), vec2(slider.width(), 4.0));
        ui.painter().rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(40));
        let fill_x = track.left() + track.width() * volume;
        ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(fill_x, track.bottom())), CornerRadius::same(2), theme::TEXT);
        if !fixed || ui.rect_contains_pointer(slider) {
            ui.painter().circle_filled(pos2(fill_x, icon_at.y), 6.0, theme::TEXT);
        }
    }

    let (r, mute) = hit(ui, icon_at, 40.0, "mudo");
    let icon = if volume <= 0.001 {
        icons::VOLUME_X
    } else if volume < 0.5 {
        icons::VOLUME_1
    } else {
        icons::VOLUME_2
    };
    round_button(ui, r, &mute, icon, theme::TEXT_DIM, false, 22.0);
    if mute.on_hover_text(t().volume(volume * 100.0)).clicked() {
        if volume > 0.001 {
            state.unmuted = volume;
            cx.act(Action::Cmd(Command::SetVolume(0.0)));
        } else {
            cx.act(Action::Cmd(Command::SetVolume(state.unmuted.max(0.1))));
        }
    }
    let open = dragging || ui.rect_contains_pointer(zone) && (was_open || ui.rect_contains_pointer(r));
    if !fixed && open != was_open {
        ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
        ui.ctx().request_repaint();
    }
}
