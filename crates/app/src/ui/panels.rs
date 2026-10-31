//! A interface em painéis: cartões arredondados sobre o preto, a biblioteca à
//! esquerda, a página no meio e a fila à direita. Os dados são os mesmos do
//! YouTube Music; muda só como eles são organizados. O `app` escolhe entre
//! esta interface e a clássica ao montar a janela (`Shell`), e as páginas
//! perguntam a `theme::panels()` nos poucos pontos em que diferem.

use egui::epaint::Mesh;
use egui::{Align2, Color32, CornerRadius, CursorIcon, Frame, Id, Margin, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::Command;
use musicfast_player::offline::State as Download;
use musicfast_ytm::{Card, Header, Kind, LikeStatus, Page, Track};

use super::icons;
use super::kit;
use super::nav::{self, SearchBox};
use super::page;
use super::theme;
use super::{Action, Cx, Dialog, LibraryTab, Me, View};

pub const TOP: f32 = 60.0;
pub const GAP: i8 = 8;
pub const LIBRARY: f32 = 320.0;
pub const LIBRARY_RANGE: std::ops::RangeInclusive<f32> = 260.0..=480.0;
/// A biblioteca recolhida: só as capas.
pub const LIBRARY_MINI: f32 = 80.0;
const LIBRARY_ROW: f32 = 64.0;
const LINE: f32 = 56.0;
const ART: f32 = 216.0;

/// O cartão de um painel. `margin`: o vão até os vizinhos.
pub fn frame(margin: Margin) -> Frame {
    Frame::new().fill(theme::PANEL).corner_radius(CornerRadius::same(theme::RADIUS)).outer_margin(margin)
}

/// Cobre os cantos de um painel: o que rola dentro dele é recortado reto.
pub fn round_corners(ui: &Ui, rect: Rect) {
    ui.painter().rect_stroke(rect, CornerRadius::same(theme::RADIUS), Stroke::new(8.0, Color32::BLACK), StrokeKind::Outside);
}

/// A barra de cima: a marca do app, voltar e avançar, o início, a busca
/// (com o Explorar na ponta) e a conta.
pub fn top(ui: &mut Ui, cx: &mut Cx, view: &View, search: &mut SearchBox, can_back: bool, can_forward: bool, suggest: &mut Option<String>) {
    let rect = ui.max_rect();
    let cy = rect.center().y;
    let logo = pos2(rect.left() + 32.0, cy);
    ui.painter().circle_filled(logo, 16.0, theme::accent());
    icons::paint_filled(ui.painter(), Rect::from_center_size(logo + vec2(1.0, 0.0), vec2(14.0, 14.0)), icons::PLAY, Color32::BLACK);
    let mut x = logo.x + 44.0;
    for (icon, enabled, action, tip) in [
        (icons::CHEVRON_LEFT, can_back, Action::Back, t().back_tip),
        (icons::CHEVRON_RIGHT, can_forward, Action::Forward, t().forward_tip),
    ] {
        let r = Rect::from_center_size(pos2(x, cy), vec2(32.0, 32.0));
        let response = ui.interact(r, Id::new(("nav", tip)), if enabled { Sense::click() } else { Sense::hover() });
        kit::paint_icon_button(ui, r, &response, icon, if enabled { theme::TEXT } else { theme::TEXT_FAINT });
        if enabled && response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip).clicked() {
            cx.act(action);
        }
        x += 36.0;
    }

    nav::account(ui, cx, rect);

    let width = (rect.width() - 520.0).clamp(220.0, 480.0);
    let field = Rect::from_center_size(pos2(rect.center().x + 28.0, cy), vec2(width, 48.0));
    let field = field.translate(vec2((x + 44.0 - field.left()).max(0.0), 0.0));
    let home = Rect::from_center_size(pos2(field.left() - 32.0, cy), vec2(48.0, 48.0));
    let response = ui.interact(home, Id::new("inicio-paineis"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    ui.painter().circle_filled(home.center(), 24.0, if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS });
    let at_home = *view == View::Home;
    let icon = Rect::from_center_size(home.center(), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon, icons::HOUSE, if at_home { theme::TEXT } else { theme::TEXT_DIM });
    if response.on_hover_text(t().home).clicked() && !at_home {
        cx.act(Action::Open(View::Home));
    }

    nav::search_field(ui, cx, search, field, 24, 92.0, suggest);
    let line = field.right() - 52.0;
    ui.painter().vline(line, (cy - 12.0)..=(cy + 12.0), Stroke::new(1.0, theme::TEXT_FAINT));
    let explore = Rect::from_center_size(pos2(field.right() - 26.0, cy), vec2(36.0, 36.0));
    let response = ui.interact(explore, Id::new("explorar-paineis"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let color = if *view == View::Explore { theme::TEXT } else { theme::TEXT_DIM };
    kit::paint_icon_button(ui, explore, &response, icons::COMPASS, color);
    if response.on_hover_text(t().explore).clicked() {
        cx.act(Action::Open(View::Explore));
    }
}

/// O que a coluna da biblioteca lembra.
#[derive(Default)]
pub struct Library {
    tab: LibraryTab,
    query: String,
    searching: bool,
}

/// Uma linha da biblioteca.
#[derive(Clone, Copy)]
enum Entry<'a> {
    Liked,
    Downloads,
    Card(&'a Card),
}

impl Library {
    pub fn tab(&self) -> LibraryTab {
        self.tab
    }
}

impl Entry<'_> {
    fn title(&self) -> &str {
        match self {
            Self::Liked => t().liked_songs,
            Self::Downloads => t().downloads,
            Self::Card(card) => &card.title,
        }
    }

    fn subtitle(&self, cx: &Cx) -> String {
        match self {
            Self::Liked => t().auto_playlist.to_owned(),
            Self::Downloads => cx.offline.map_or_else(String::new, |o| t().songs_count(o.entries().len())),
            Self::Card(card) => card.subtitle.clone(),
        }
    }

    fn target(&self) -> View {
        match self {
            Self::Liked => View::browse("VLLM", None),
            Self::Downloads => View::Downloads,
            Self::Card(card) => View::browse(&card.browse_id, card.params.as_deref()),
        }
    }
}

/// `shelf`: a página da aba escolhida, quando não é a das playlists.
pub fn library(ui: &mut Ui, cx: &mut Cx, view: &View, mini: bool, state: &mut Library, shelf: Option<&Page>) {
    let rect = ui.max_rect();
    let me: &Me = cx.me;
    let needle = state.query.trim().to_lowercase();
    let mut entries: Vec<Entry> = Vec::new();
    if state.tab == LibraryTab::Playlists {
        if me.signed_in {
            entries.push(Entry::Liked);
        }
        entries.push(Entry::Downloads);
        entries.extend(me.playlists.iter().filter(|c| c.browse_id != "VLLM").map(Entry::Card));
    } else if let Some(shelf) = shelf {
        entries.extend(shelf.shelves.iter().flat_map(|s| &s.items).filter_map(|item| match item {
            musicfast_ytm::Item::Card(card) => Some(Entry::Card(card)),
            musicfast_ytm::Item::Track(_) => None,
        }));
    }
    if !needle.is_empty() {
        entries.retain(|e| e.title().to_lowercase().contains(&needle));
    }

    if mini {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            let (r, response) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
            let response = response.on_hover_cursor(CursorIcon::PointingHand);
            kit::paint_icon_button(ui, r, &response, icons::LIBRARY, theme::TEXT_DIM);
            if response.on_hover_text(t().expand_menu).clicked() {
                cx.act(Action::ToggleSidebar);
            }
        });
        ui.add_space(8.0);
    } else {
        library_head(ui, cx, state);
    }

    let id = ("biblioteca-paineis", state.tab);
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show_rows(ui, LIBRARY_ROW, entries.len(), |ui, range| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for i in range {
            library_row(ui, cx, entries[i], view, mini);
        }
        if !mini && !me.signed_in {
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                nav::sign_in_invite(ui, cx);
            });
        }
    });
    round_corners(ui, rect);
}

/// O título, o "Criar", as abas e a busca dentro da biblioteca.
fn library_head(ui: &mut Ui, cx: &mut Cx, state: &mut Library) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
    let title = ui.interact(Rect::from_min_size(row.min, vec2(row.width() - 120.0, 60.0)), Id::new("biblioteca-titulo"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let color = if title.hovered() { theme::TEXT } else { theme::TEXT_SUB };
    ui.painter().text(pos2(row.left() + 20.0, row.center().y), Align2::LEFT_CENTER, t().your_library, theme::bold(16.0), color);
    if title.on_hover_text(t().collapse_menu).clicked() {
        cx.act(Action::ToggleSidebar);
    }
    if cx.me.signed_in {
        let mut right = ui.new_child(UiBuilder::new().max_rect(row.shrink2(vec2(16.0, 0.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if kit::pill(&mut right, Some(icons::PLUS), t().create, false).clicked() {
            cx.act(Action::Dialog(Dialog::NewPlaylist { title: String::new(), privacy: musicfast_ytm::Privacy::Private, track: None }));
        }
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            for tab in [LibraryTab::Playlists, LibraryTab::Albums, LibraryTab::Artists] {
                if kit::chip(ui, tab.label(), state.tab == tab).clicked() && state.tab != tab {
                    state.tab = tab;
                    if tab != LibraryTab::Playlists {
                        cx.act(Action::LoadLibrary(tab));
                    }
                }
            }
        });
    }
    ui.add_space(8.0);
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let glass = Rect::from_center_size(pos2(line.left() + 34.0, line.center().y), vec2(32.0, 32.0));
    let response = ui.interact(glass, Id::new("biblioteca-busca"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    kit::paint_icon_button(ui, glass, &response, icons::SEARCH, theme::TEXT_DIM);
    if response.clicked() {
        state.searching = !state.searching;
        state.query.clear();
    }
    if state.searching {
        let edit = Rect::from_min_max(pos2(glass.right() + 6.0, line.top()), pos2(line.right() - 16.0, line.bottom()));
        let mut child = ui.new_child(UiBuilder::new().max_rect(edit).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.add(
            egui::TextEdit::singleline(&mut state.query)
                .id(Id::new("biblioteca-filtro"))
                .frame(Frame::NONE)
                .hint_text(egui::RichText::new(t().filter_library).color(theme::TEXT_FAINT))
                .font(theme::regular(14.0))
                .desired_width(edit.width()),
        );
    }
    ui.add_space(4.0);
}

fn library_row(ui: &mut Ui, cx: &mut Cx, entry: Entry, view: &View, mini: bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), LIBRARY_ROW), Sense::click());
    let item = rect.shrink2(vec2(8.0, 0.0));
    let target = entry.target();
    let selected = target == *view;
    if selected {
        ui.painter().rect_filled(item, CornerRadius::same(6), theme::GLASS_HOVER);
    } else if response.hovered() {
        ui.painter().rect_filled(item, CornerRadius::same(6), theme::GLASS);
    }
    let art = if mini {
        Rect::from_center_size(item.center(), vec2(48.0, 48.0))
    } else {
        Rect::from_min_size(pos2(item.left() + 8.0, item.center().y - 24.0), vec2(48.0, 48.0))
    };
    match entry {
        Entry::Liked => pinned_art(ui, art, Color32::from_rgb(68, 36, 200), Color32::from_rgb(150, 200, 190), icons::HEART),
        Entry::Downloads => pinned_art(ui, art, Color32::from_rgb(4, 80, 62), Color32::from_rgb(20, 150, 100), icons::DOWNLOAD),
        Entry::Card(card) => {
            let radius = if card.kind == Kind::Artist { 24 } else { 4 };
            cx.covers.paint(ui, art, &card.thumbs, radius);
        }
    }
    if !mini {
        let x = art.right() + 12.0;
        let w = item.right() - x - 8.0;
        let color = if selected { theme::accent() } else { theme::TEXT };
        let title = kit::one_line(ui, entry.title(), theme::medium(15.0), color, w);
        ui.painter().galley(pos2(x, item.center().y - 20.0), title, color);
        let sub = kit::one_line(ui, &entry.subtitle(cx), theme::regular(13.0), theme::TEXT_DIM, w);
        ui.painter().galley(pos2(x, item.center().y + 2.0), sub, theme::TEXT_DIM);
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let response = if mini { response.on_hover_text(entry.title()) } else { response };
    if response.clicked() && !selected {
        cx.act(Action::Open(target));
    }
}

/// A capa desenhada das linhas fixas (curtidas, baixadas), em diagonal.
fn pinned_art(ui: &Ui, rect: Rect, from: Color32, to: Color32, icon: icons::Icon) {
    // O fundo liso dá a borda suave que a malha do degradê não tem.
    ui.painter().rect_filled(rect, CornerRadius::same(4), from.lerp_to_gamma(to, 0.5));
    rounded_gradient(ui.painter(), rect.shrink(0.5), 4.0, from, to, vec2(1.0, 1.0));
    icons::paint_filled(ui.painter(), rect.shrink(14.0), icon, Color32::WHITE);
}

/// Um retângulo arredondado com degradê de `from` a `to` na direção `along`:
/// um leque a partir do centro, com a cor calculada em cada ponto da borda.
pub fn rounded_gradient(painter: &egui::Painter, rect: Rect, radius: f32, from: Color32, to: Color32, along: egui::Vec2) {
    const STEPS: usize = 5;
    let along = along.normalized();
    let reach = (rect.width() * along.x.abs() + rect.height() * along.y.abs()).max(1.0);
    let color = |at: egui::Pos2| {
        let t = ((at - rect.center()).dot(along) / reach + 0.5).clamp(0.0, 1.0);
        from.lerp_to_gamma(to, t)
    };
    let inner = rect.shrink(radius);
    let corners = [(inner.right_bottom(), 0.0), (inner.left_bottom(), 0.25), (inner.left_top(), 0.5), (inner.right_top(), 0.75)];
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), color(rect.center()));
    for (corner, turn) in corners {
        for step in 0..=STEPS {
            let angle = (turn + 0.25 * step as f32 / STEPS as f32) * std::f32::consts::TAU;
            let at = corner + vec2(angle.cos(), angle.sin()) * radius;
            mesh.colored_vertex(at, color(at));
        }
    }
    let ring = 4 * (STEPS as u32 + 1);
    for i in 0..ring {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % ring);
    }
    painter.add(mesh);
}

/// A cor que desce do alto de um painel: forte em cima, some em `height`.
pub fn top_glow(ui: &Ui, rect: Rect, tint: Color32, height: f32, strength: f32) {
    let stops = [(0.0, 1.0), (0.45, 0.45), (1.0, 0.0)];
    let mut mesh = Mesh::default();
    for (y, power) in stops {
        let y = rect.top() + height * y;
        let color = tint.gamma_multiply(strength * power);
        mesh.colored_vertex(pos2(rect.left(), y), color);
        mesh.colored_vertex(pos2(rect.right(), y), color);
    }
    for at in [0, 2] {
        mesh.add_triangle(at, at + 1, at + 2);
        mesh.add_triangle(at + 1, at + 2, at + 3);
    }
    ui.painter().add(mesh);
}

/// A capa sob o mouse pede a cor do alto da página (`page_glow`).
pub fn offer_tint(ui: &Ui, tint: Color32) {
    ui.data_mut(|d| d.insert_temp(Id::new(TINT_OFFER), tint));
}

const TINT_OFFER: &str = "paineis-cor-pedida";

/// O alto das páginas sem capa própria: a cor da capa sob o mouse, ou a de
/// `fallback`, chegando aos poucos em vez de trocar de uma vez.
pub fn page_glow(ui: &Ui, rect: Rect, fallback: Option<Color32>) {
    let offer = Id::new(TINT_OFFER);
    let state = Id::new("paineis-cor-agora");
    // A capa pede de novo a cada quadro em que o mouse continua nela.
    let hovered = ui.data_mut(|d| d.remove_temp::<Color32>(offer));
    let target = hovered.or(fallback).unwrap_or(Color32::from_gray(90)).to_array().map(f32::from);
    let mut now = ui.data(|d| d.get_temp::<[f32; 4]>(state)).unwrap_or(target);
    let dt = ui.input(|i| i.stable_dt).min(0.1);
    let step = 1.0 - (-dt * 7.0).exp();
    let mut moving = false;
    for (value, goal) in now.iter_mut().zip(target) {
        *value += (goal - *value) * step;
        if (goal - *value).abs() > 1.0 {
            moving = true;
        } else {
            *value = goal;
        }
    }
    ui.data_mut(|d| d.insert_temp(state, now));
    if moving {
        ui.ctx().request_repaint();
    }
    let [r, g, b, a] = now.map(|v| v.round() as u8);
    top_glow(ui, rect, Color32::from_rgba_premultiplied(r, g, b, a), 340.0, 0.42);
}

/// O cabeçalho das páginas: a cor da capa do alto do painel até sumir, a capa,
/// o título enorme e a linha de botões.
pub fn header(ui: &mut Ui, cx: &mut Cx, header: &Header, page: &Page) {
    let artist = header.kind == Kind::Artist;
    let tint = cx.covers.color(&header.thumbs).unwrap_or(Color32::from_gray(90));
    wash(ui, tint);
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(ART, ART), Sense::hover());
        cx.covers.paint(ui, rect, &header.thumbs, if artist { (ART / 2.0) as u8 } else { 6 });
        ui.add_space(24.0);
        ui.vertical(|ui| {
            let width = ui.available_width();
            ui.set_max_width(width);
            let font = [72.0, 56.0, 44.0]
                .into_iter()
                .map(theme::bold)
                .find(|font| ui.painter().layout_no_wrap(header.title.clone(), font.clone(), theme::TEXT).size().x <= width)
                .unwrap_or(theme::bold(32.0));
            let title = kit::one_line(ui, &header.title, font, theme::TEXT, width);
            let block = 22.0 + title.size().y + 30.0;
            ui.add_space((ART - block).max(0.0));
            let mut parts = header.subtitle.split(" • ");
            let kind = parts.next().unwrap_or_default();
            ui.label(egui::RichText::new(kind).font(theme::medium(14.0)));
            let (at, _) = ui.allocate_exact_size(title.size(), Sense::hover());
            ui.painter().galley(at.min, title, theme::TEXT);
            let mut line: Vec<&str> = header.authors.iter().map(|a| a.name.as_str()).collect();
            line.extend(parts);
            if !header.detail.is_empty() {
                line.push(&header.detail);
            }
            ui.add(egui::Label::new(egui::RichText::new(line.join(" • ")).color(theme::TEXT_SUB).font(theme::regular(14.0))).truncate());
        });
    });
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        header_actions(ui, cx, header, page);
    });
}

fn header_actions(ui: &mut Ui, cx: &mut Cx, header: &Header, page: &Page) {
    let has_tracks = page.tracks().next().is_some();
    let artist = header.kind == Kind::Artist;
    if (header.play.is_some() || has_tracks) && green_play(ui, 56.0).clicked() {
        cx.act(Action::Cmd(Command::SetShuffle(false)));
        match &header.play {
            Some(play) => cx.act(Action::Play(play.clone(), None)),
            None => cx.act(Action::Cmd(Command::PlayTracks { tracks: page.tracks().cloned().collect(), index: 0 })),
        }
    }
    if has_tracks && !artist && action_icon(ui, icons::SHUFFLE, t().shuffle, false) {
        let tracks: Vec<Track> = page.tracks().cloned().collect();
        cx.act(Action::Cmd(Command::SetShuffle(true)));
        let index = (crate::state::unix_now() as usize) % tracks.len();
        cx.act(Action::Cmd(Command::PlayTracks { tracks, index }));
    }
    if let Some(radio) = &header.radio
        && action_icon(ui, icons::RADIO, t().radio, false)
    {
        cx.act(Action::Cmd(Command::SetShuffle(false)));
        cx.act(Action::Play(radio.clone(), None));
    }
    if !cx.me.signed_in {
        return;
    }
    if let Some(channel) = &header.channel_id {
        let on = cx.me.subscribed.get(channel).copied().or(header.subscribed).unwrap_or(false);
        let label = if on { t().subscribed } else { t().subscribe };
        if kit::pill(ui, None, label, false).clicked() {
            cx.act(Action::Subscribe(channel.clone(), !on));
        }
    } else if let Some(playlist) = header.play.as_ref().and_then(|p| p.playlist_id.clone())
        && matches!(header.kind, Kind::Album | Kind::Playlist)
        && playlist != "LM"
    {
        let on = cx.me.saved.get(&playlist).copied().unwrap_or(false);
        let (icon, tip) = if on { (icons::BOOKMARK_CHECK, t().in_library) } else { (icons::BOOKMARK_PLUS, t().save_to_library) };
        if action_icon(ui, icon, tip, on) {
            cx.act(Action::SaveCollection(playlist, !on));
        }
    }
}

/// O botão redondo de tocar.
pub fn green_play(ui: &mut Ui, side: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let grow = if response.hovered() { 1.5 } else { 0.0 };
    ui.painter().circle_filled(rect.center(), side / 2.0 + grow, theme::accent());
    let icon = Rect::from_center_size(rect.center() + vec2(1.0, 0.0), vec2(side * 0.42, side * 0.42));
    icons::paint_filled(ui.painter(), icon, icons::PLAY, Color32::BLACK);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn action_icon(ui: &mut Ui, icon: icons::Icon, tip: &str, on: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
    let color = match (on, response.hovered()) {
        (true, _) => theme::accent(),
        (false, true) => theme::TEXT,
        (false, false) => theme::TEXT_SUB,
    };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(30.0, 30.0)), icon, color);
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

/// A cor que desce do alto do painel e some no cinza dele.
fn wash(ui: &Ui, tint: Color32) {
    let clip = ui.clip_rect();
    let top = ui.cursor().top() - 20.0;
    let stops = [(0.0, 0.62), (ART + 44.0, 0.42), (ART + 300.0, 0.0)];
    let mut mesh = Mesh::default();
    for (y, strength) in stops {
        mesh.colored_vertex(pos2(clip.left(), top + y), tint.gamma_multiply(strength));
        mesh.colored_vertex(pos2(clip.right(), top + y), tint.gamma_multiply(strength));
    }
    for i in 0..2 {
        let at = i * 2;
        mesh.add_triangle(at, at + 1, at + 2);
        mesh.add_triangle(at + 1, at + 2, at + 3);
    }
    ui.painter().add(mesh);
}

/// As colunas de uma lista de músicas.
#[derive(Clone, Copy)]
pub struct Columns {
    /// Mostra a coluna "#".
    pub numbered: bool,
    pub cover: bool,
    pub album: bool,
}

impl Columns {
    /// A coluna do álbum só com largura para ela.
    fn album_at(self, rect: Rect) -> Option<f32> {
        (self.album && rect.width() > 620.0).then(|| rect.left() + rect.width() * 0.56)
    }

    fn title_at(self, rect: Rect) -> f32 {
        rect.left() + if self.numbered { 52.0 } else { 12.0 }
    }
}

/// A linha de títulos da tabela, com o fio embaixo.
pub fn table_head(ui: &mut Ui, columns: Columns) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    let cy = rect.top() + 14.0;
    let p = ui.painter();
    let font = theme::regular(14.0);
    if columns.numbered {
        p.text(pos2(rect.left() + 26.0, cy), Align2::CENTER_CENTER, "#", font.clone(), theme::TEXT_DIM);
    }
    p.text(pos2(columns.title_at(rect), cy), Align2::LEFT_CENTER, t().sort_title, font.clone(), theme::TEXT_DIM);
    if let Some(x) = columns.album_at(rect) {
        p.text(pos2(x, cy), Align2::LEFT_CENTER, t().album, font, theme::TEXT_DIM);
    }
    let clock = Rect::from_center_size(pos2(rect.right() - 62.0, cy), vec2(16.0, 16.0));
    icons::paint(p, clock, icons::HISTORY, theme::TEXT_DIM);
    p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, theme::GLASS_HOVER));
    ui.add_space(4.0);
}

/// Uma música na tabela: número (o tocar sob o mouse, as barrinhas se é a
/// atual), capa, título e artista, álbum, curtir, duração e o menu. `true` no
/// clique. A linha baixada se arrasta para fora, como na interface clássica.
pub fn track_line(ui: &mut Ui, cx: &mut Cx, track: &Track, number: usize, columns: Columns, id: Id) -> bool {
    const BUTTON: f32 = 32.0;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), LINE), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let download = cx.download(&track.video_id);
    let sense = if matches!(download, Download::Done(_)) { Sense::click_and_drag() } else { Sense::click() };
    let response = ui.interact(rect, id, sense);
    if response.drag_started()
        && let Download::Done(path) = &download
    {
        cx.act(Action::DragOut(path.clone()));
    }
    let menu_open = response.context_menu_opened();
    let current = cx.current.as_deref() == Some(track.video_id.as_str());
    let hovered = response.hovered() || menu_open || ui.rect_contains_pointer(rect);
    if response.hovered() {
        cx.act(Action::Warm(track.video_id.clone()));
    }
    let p = ui.painter().clone();
    if hovered {
        p.rect_filled(rect, CornerRadius::same(4), theme::GLASS);
    }
    let cy = rect.center().y;
    if columns.numbered {
        let lead = Rect::from_center_size(pos2(rect.left() + 26.0, cy), vec2(20.0, 20.0));
        if current {
            page::equalizer(&p, lead.shrink(3.0), cx.playing);
        } else if hovered {
            icons::paint_filled(&p, lead.shrink(3.0), icons::PLAY, theme::TEXT);
        } else {
            p.text(lead.center(), Align2::CENTER_CENTER, number.to_string(), theme::regular(15.0), theme::TEXT_DIM);
        }
    }
    let mut x = columns.title_at(rect);
    if columns.cover {
        let art = Rect::from_min_size(pos2(x, cy - 20.0), vec2(40.0, 40.0));
        cx.covers.paint(ui, art, &track.thumbs, 4);
        if !columns.numbered && (hovered || current) {
            p.rect_filled(art, CornerRadius::same(4), Color32::from_black_alpha(140));
            if current {
                page::equalizer(&p, art.shrink(11.0), cx.playing);
            } else {
                icons::paint_filled(&p, art.shrink(11.0), icons::PLAY, theme::TEXT);
            }
        }
        x = art.right() + 12.0;
    }
    let album_x = columns.album_at(rect);
    let right = rect.right() - 12.0;
    let text_end = album_x.unwrap_or(right - 4.0 * BUTTON).min(right - 4.0 * BUTTON) - 16.0;
    let width = (text_end - x).max(40.0);
    let color = if current { theme::accent() } else { theme::TEXT };
    let title = kit::one_line(ui, &track.title, theme::medium(15.0), color, width);
    p.galley(pos2(x, cy - 20.0), title, color);
    let mut sx = x;
    if track.explicit {
        let badge = Rect::from_min_size(pos2(sx, cy + 3.0), vec2(14.0, 14.0));
        p.rect_filled(badge, CornerRadius::same(2), theme::TEXT_DIM);
        p.text(badge.center(), Align2::CENTER_CENTER, "E", theme::bold(10.0), Color32::BLACK);
        sx += 20.0;
    }
    let artists = kit::one_line(ui, &track.artist_names(), theme::regular(13.5), theme::TEXT_DIM, (x + width - sx).max(20.0));
    let artist_id = track.artists.iter().find_map(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(sx, cy + 1.0), artists, theme::TEXT_DIM, id.with("artista"), artist_id.is_some())
        && r.clicked()
        && let Some(artist) = &artist_id
    {
        cx.act(Action::Open(View::browse(artist, None)));
    }
    if let (Some(album_x), Some(album)) = (album_x, &track.album) {
        let galley = kit::one_line(ui, &album.name, theme::regular(14.0), theme::TEXT_DIM, right - 4.0 * BUTTON - album_x - 16.0);
        let at = pos2(album_x, cy - galley.size().y / 2.0);
        if let Some(r) = kit::text_link(ui, at, galley, theme::TEXT_DIM, id.with("album"), album.id.is_some())
            && r.clicked()
            && let Some(album) = &album.id
        {
            cx.act(Action::Open(View::browse(album, None)));
        }
    }

    if let Some(d) = track.duration_secs {
        p.text(pos2(right - BUTTON - 4.0, cy), Align2::RIGHT_CENTER, kit::time(d as f64), theme::regular(14.0), theme::TEXT_DIM);
    }
    let button = |at: f32| Rect::from_center_size(pos2(at, cy), vec2(BUTTON, BUTTON));
    if hovered {
        let r = button(right - BUTTON / 2.0);
        let menu = ui.interact(r, id.with("menu"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        kit::paint_icon_button(ui, r, &menu, icons::ELLIPSIS, theme::TEXT);
        egui::Popup::menu(&menu).show(|ui| page::track_menu(ui, cx, track));
    }
    let liked = cx.me.signed_in && cx.me.like(&track.video_id) == LikeStatus::Like;
    if cx.me.signed_in && (hovered || liked) {
        let r = button(right - 2.0 * BUTTON - 28.0);
        let b = ui.interact(r, id.with("curtir"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let inner = r.shrink(8.0);
        if liked {
            icons::paint_filled(&p, inner, icons::HEART, theme::accent());
        } else {
            icons::paint(&p, inner, icons::HEART, if b.hovered() { theme::TEXT } else { theme::TEXT_DIM });
        }
        if b.clicked() {
            cx.act(Action::Rate(track.video_id.clone(), if liked { LikeStatus::Indifferent } else { LikeStatus::Like }));
        }
    }
    if hovered && !matches!(download, Download::Done(_)) || matches!(download, Download::Working(_) | Download::Failed(_)) {
        let r = button(right - 3.0 * BUTTON - 28.0);
        page::download_button(ui, cx, r, id.with("baixar"), track, &download);
    }
    let clicked = response.clicked();
    response.context_menu(|ui| page::track_menu(ui, cx, track));
    clicked
}

/// O melhor resultado da busca: a capa, o nome e o tocar num cartão largo.
pub fn top_result(ui: &mut Ui, cx: &mut Cx, card: &Card) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 112.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let fill = if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS };
    ui.painter().rect_filled(rect, CornerRadius::same(theme::RADIUS), fill);
    let art = Rect::from_min_size(rect.min + vec2(16.0, 16.0), vec2(80.0, 80.0));
    cx.covers.paint(ui, art, &card.thumbs, if card.kind == Kind::Artist { 40 } else { 4 });
    let x = art.right() + 20.0;
    let width = (rect.right() - x - 90.0).max(40.0);
    let title = kit::one_line(ui, &card.title, theme::bold(28.0), theme::TEXT, width);
    ui.painter().galley(pos2(x, rect.center().y - 28.0), title, theme::TEXT);
    let sub = kit::one_line(ui, &card.subtitle, theme::regular(14.0), theme::TEXT_DIM, width);
    ui.painter().galley(pos2(x, rect.center().y + 10.0), sub, theme::TEXT_DIM);
    let mut played = false;
    if card.play.is_some() {
        let spot = Rect::from_center_size(pos2(rect.right() - 44.0, rect.center().y), vec2(48.0, 48.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(spot));
        played = green_play(&mut child, 48.0).clicked();
    }
    if played {
        if let Some(command) = card.play.as_ref().and_then(|play| super::play_command(play, None)) {
            cx.act(Action::Cmd(command));
        }
    } else if response.clicked() {
        page::open_card(cx, card);
    }
}
