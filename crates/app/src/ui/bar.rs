//! A barra do player, embaixo, no arranjo do YouTube Music: os controles e o
//! tempo à esquerda; a música no meio (capa, título, "Artista • Álbum",
//! gostei/não gostei e o menu ⋮); volume, repetir, aleatório e a seta do
//! player à direita. A linha do tempo é a borda de cima da barra inteira.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use musicfast_player::{Command, Repeat, Snapshot, Status};
use musicfast_ytm::LikeStatus;

use super::icons;
use super::kit;
use super::page::track_menu;
use super::theme;
use super::{Action, Cx, Side, View};

pub const HEIGHT: f32 = 72.0;
/// O cinza da barra do site (#212121), um tom acima do fundo da página.
pub const BG: Color32 = Color32::from_rgb(33, 33, 33);

/// As colunas da esquerda e da direita têm largura fixa; a música fica no
/// meio do que sobra.
const LEFT_W: f32 = 270.0;
const RIGHT_W: f32 = 250.0;

pub struct BarState {
    /// Onde a linha do tempo está sendo arrastada (s).
    pub seeking: Option<f64>,
    /// O volume antes do mudo, para voltar a ele.
    pub unmuted: f32,
}

pub fn show(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, position: f64, side: Option<Side>, expanded: bool, state: &mut BarState) {
    let rect = ui.max_rect();
    let duration = snap.duration.or_else(|| snap.current().and_then(|t| t.duration_secs.map(f64::from)));
    let left = Rect::from_min_max(rect.min, pos2(rect.left() + LEFT_W, rect.bottom()));
    let right = Rect::from_min_max(pos2(rect.right() - RIGHT_W, rect.top()), rect.max);
    let center = Rect::from_min_max(pos2(left.right(), rect.top()), pos2(right.left(), rect.bottom()));

    // O fundo da barra abre (ou fecha) o player com a fila, como no site.
    // Vem antes de tudo: botões, links e a linha do tempo, registrados
    // depois, ficam por cima e pegam o clique primeiro; só o vazio sobra
    // para cá.
    if snap.current().is_some() {
        let back = ui.interact(rect, Id::new("barra-fundo"), Sense::click());
        if back.clicked() {
            cx.act(Action::Expand(!expanded));
        }
    }

    controls(ui, cx, left, snap, position, duration);
    now_playing(ui, cx, center, snap, side, expanded);
    extras(ui, cx, right, snap, expanded, state);
    // Por último: a linha do tempo fica por cima e pega o mouse primeiro.
    timeline(ui, cx, rect, position, duration, snap.current().is_some(), state);
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
    painter.rect_filled(done, CornerRadius::ZERO, theme::ACCENT);
    if hovered {
        painter.circle_filled(pos2(x, bar.top()), 7.0, theme::ACCENT);
        if let Some(p) = response.hover_pos() {
            let at = state.seeking.unwrap_or_else(|| at_pointer(p.x));
            let tip = Rect::from_center_size(pos2(p.x.clamp(bar.left() + 24.0, bar.right() - 24.0), bar.top() - 20.0), vec2(48.0, 20.0));
            painter.rect_filled(tip, CornerRadius::same(4), theme::ACTIVE);
            painter.text(tip.center(), Align2::CENTER_CENTER, kit::time(at), theme::regular(12.0), theme::TEXT);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand);
}

/// À esquerda: anterior, tocar/pausar (o triângulo grande, sem círculo),
/// próxima e o tempo.
fn controls(ui: &mut Ui, cx: &mut Cx, rect: Rect, snap: &Snapshot, position: f64, duration: Option<f64>) {
    let has = snap.current().is_some();
    let color = if has { theme::TEXT } else { theme::TEXT_FAINT };
    let cy = rect.center().y;
    let x0 = rect.left() + 12.0;

    let (r, resp) = hit(ui, pos2(x0 + 20.0, cy), 40.0, ("ctrl", "anterior"));
    round_button(ui, r, &resp, icons::SKIP_BACK, color, true, 22.0);
    if resp.on_hover_text("Anterior").clicked() {
        cx.act(Action::Cmd(Command::Previous));
    }

    let (r, resp) = hit(ui, pos2(x0 + 74.0, cy), 52.0, ("ctrl", "tocar"));
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 26.0, Color32::from_white_alpha(18));
    }
    match snap.status {
        Status::Loading => {
            // A fila espera o yt-dlp: o arco gira no lugar do ícone.
            let t = ui.ctx().input(|i| i.time) as f32;
            let arc = r.shrink(14.0);
            let points: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let a = t * 6.0 + i as f32 / 24.0 * std::f32::consts::PI * 1.5;
                    arc.center() + vec2(a.cos(), a.sin()) * arc.width() / 2.0
                })
                .collect();
            ui.painter().add(Shape::line(points, Stroke::new(3.0, theme::TEXT)));
            ui.ctx().request_repaint();
        }
        Status::Playing => icons::paint_filled(ui.painter(), Rect::from_center_size(r.center(), vec2(34.0, 34.0)), icons::PAUSE, color),
        _ => icons::paint_filled(ui.painter(), Rect::from_center_size(r.center() + vec2(2.0, 0.0), vec2(36.0, 36.0)), icons::PLAY, color),
    }
    let tip = if snap.status == Status::Playing { "Pausar (espaço)" } else { "Tocar (espaço)" };
    if resp.on_hover_text(tip).clicked() {
        cx.act(Action::Cmd(Command::Toggle));
    }

    let (r, resp) = hit(ui, pos2(x0 + 128.0, cy), 40.0, ("ctrl", "proxima"));
    round_button(ui, r, &resp, icons::SKIP_FORWARD, color, true, 22.0);
    if resp.on_hover_text("Próxima").clicked() {
        cx.act(Action::Cmd(Command::Next));
    }

    // O tempo, ou o que está demorando.
    let text = match (snap.status, snap.step) {
        (Status::Loading, Some(step)) => step.to_owned(),
        _ if has => match duration {
            Some(d) => format!("{} / {}", kit::time(position.min(d)), kit::time(d)),
            None => kit::time(position),
        },
        _ => String::new(),
    };
    let galley = kit::one_line(ui, &text, theme::regular(12.5), theme::TEXT_DIM, rect.right() - (x0 + 160.0) - 8.0);
    ui.painter().galley(pos2(x0 + 160.0, cy - galley.size().y / 2.0), galley, theme::TEXT_DIM);
}

/// No meio: capa, título e "Artista • Álbum", gostei/não gostei e o menu ⋮.
fn now_playing(ui: &mut Ui, cx: &mut Cx, rect: Rect, snap: &Snapshot, side: Option<Side>, expanded: bool) {
    let Some(track) = snap.current() else { return };
    const ART: f32 = 40.0;
    let buttons_w = if cx.me.signed_in { 3.0 * 40.0 } else { 40.0 };
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
    if art_click.on_hover_text("Tocando agora").clicked() {
        cx.act(Action::Expand(!expanded));
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
        if resp.on_hover_text("Fixar no mini player (Ctrl+Shift+M)").clicked() {
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

    let mut bx = tx + text_w + 16.0 + 20.0;
    if cx.me.signed_in {
        let status = cx.me.like(&track.video_id);
        for (icon, mine, tip) in [(icons::THUMBS_UP, LikeStatus::Like, "Gostei"), (icons::THUMBS_DOWN, LikeStatus::Dislike, "Não gostei")] {
            let (r, resp) = hit(ui, pos2(bx, cy), 40.0, ("gostei", tip));
            let on = status == mine;
            round_button(ui, r, &resp, icon, theme::TEXT, on, 22.0);
            if resp.on_hover_text(tip).clicked() {
                let next = if on { LikeStatus::Indifferent } else { mine };
                cx.act(Action::Rate(track.video_id.clone(), next));
            }
            bx += 40.0;
        }
    }

    // O menu ⋮: as ações da música, e a fila e a letra.
    let (r, resp) = hit(ui, pos2(bx, cy), 40.0, "barra-menu");
    round_button(ui, r, &resp, icons::ELLIPSIS_VERTICAL, theme::TEXT, false, 22.0);
    let track = track.clone();
    egui::Popup::menu(&resp).align(egui::RectAlign::TOP_START).width(super::page::MENU_W).show(|ui| {
        track_menu(ui, cx, &track);
        ui.separator();
        for (icon, label, which) in [(icons::LIST_MUSIC, "Fila", Side::Queue), (icons::MIC_VOCAL, "Letra", Side::Lyrics)] {
            let label = if side == Some(which) { format!("Fechar {}", label.to_lowercase()) } else { format!("Mostrar {}", label.to_lowercase()) };
            if menu_row(ui, icon, &label) {
                cx.act(Action::ToggleSide(which));
            }
        }
        if menu_row(ui, icons::PIN, "Fixar no mini player") {
            cx.act(Action::Mini(true));
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

/// À direita: volume (a barrinha aparece com o mouse em cima, como no
/// site), repetir, aleatório e o triângulo que abre o player.
fn extras(ui: &mut Ui, cx: &mut Cx, rect: Rect, snap: &Snapshot, expanded: bool, state: &mut BarState) {
    let cy = rect.center().y;
    let has = snap.current().is_some();
    let mut x = rect.right() - 32.0;

    // O triângulo cheio do site: para cima abre, para baixo fecha.
    let (r, resp) = hit(ui, pos2(x, cy), 40.0, "expandir");
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 20.0, Color32::from_white_alpha(18));
    }
    let c = r.center();
    let color = if has { theme::TEXT } else { theme::TEXT_FAINT };
    let tri = if expanded {
        vec![c + vec2(-9.0, -4.5), c + vec2(9.0, -4.5), c + vec2(0.0, 5.5)]
    } else {
        vec![c + vec2(-9.0, 4.5), c + vec2(9.0, 4.5), c + vec2(0.0, -5.5)]
    };
    ui.painter().add(Shape::convex_polygon(tri, color, Stroke::NONE));
    if resp.on_hover_text(if expanded { "Fechar o player" } else { "Abrir o player" }).clicked() && has {
        cx.act(Action::Expand(!expanded));
    }
    x -= 48.0;

    let (r, resp) = hit(ui, pos2(x, cy), 40.0, ("ctrl", "aleatorio"));
    round_button(ui, r, &resp, icons::SHUFFLE, if snap.shuffle { theme::TEXT } else { theme::TEXT_DIM }, false, 22.0);
    if snap.shuffle {
        ui.painter().circle_filled(r.center() + vec2(0.0, 15.0), 2.0, theme::TEXT);
    }
    if resp.on_hover_text(if snap.shuffle { "Aleatório ligado" } else { "Aleatório" }).clicked() {
        cx.act(Action::Cmd(Command::SetShuffle(!snap.shuffle)));
    }
    x -= 44.0;

    let (r, resp) = hit(ui, pos2(x, cy), 40.0, ("ctrl", "repetir"));
    let (icon, on, next, tip) = match snap.repeat {
        Repeat::Off => (icons::REPEAT, false, Repeat::All, "Repetir a fila"),
        Repeat::All => (icons::REPEAT, true, Repeat::One, "Repetir esta música"),
        Repeat::One => (icons::REPEAT_1, true, Repeat::Off, "Não repetir"),
    };
    round_button(ui, r, &resp, icon, if on { theme::TEXT } else { theme::TEXT_DIM }, false, 22.0);
    if on {
        ui.painter().circle_filled(r.center() + vec2(0.0, 15.0), 2.0, theme::TEXT);
    }
    if resp.on_hover_text(tip).clicked() {
        cx.act(Action::Cmd(Command::SetRepeat(next)));
    }
    x -= 44.0;

    volume(ui, cx, pos2(x, cy), snap, state);
}

/// O ícone do volume; com o mouse em cima (ou arrastando), a barrinha abre
/// à esquerda dele. Clicar no ícone emudece.
fn volume(ui: &mut Ui, cx: &mut Cx, icon_at: Pos2, snap: &Snapshot, state: &mut BarState) {
    const SLIDER_W: f32 = 96.0;
    let open_id = Id::new("volume-aberto");
    let was_open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let slider = Rect::from_min_max(pos2(icon_at.x - 22.0 - SLIDER_W, icon_at.y - 10.0), pos2(icon_at.x - 22.0, icon_at.y + 10.0));
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
        ui.painter().circle_filled(pos2(fill_x, icon_at.y), 6.0, theme::TEXT);
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
    if mute.on_hover_text(format!("Volume {}%", (volume * 100.0).round())).clicked() {
        if volume > 0.001 {
            state.unmuted = volume;
            cx.act(Action::Cmd(Command::SetVolume(0.0)));
        } else {
            cx.act(Action::Cmd(Command::SetVolume(state.unmuted.max(0.1))));
        }
    }
    let open = dragging || ui.rect_contains_pointer(zone) && (was_open || ui.rect_contains_pointer(r));
    if open != was_open {
        ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
        ui.ctx().request_repaint();
    }
}
