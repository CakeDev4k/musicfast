//! A página "Baixadas": as músicas no disco, que tocam sem internet, em lista
//! ou agrupadas por álbum e por artista, com filtro e ordem. Os grupos só são
//! refeitos quando a lista, o filtro ou a ordem mudam, não a cada quadro.

use std::collections::HashMap;
use std::sync::Arc;

use egui::epaint::Mesh;
use egui::{Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::Command;
use musicfast_ytm::{Thumbs, Track};

use super::icons;
use super::kit;
use super::page::{self, GUTTER, TILE};
use super::theme;
use super::{Action, Cx};

/// O lado da capa no alto da página.
const HERO: f32 = 168.0;
const TILE_GAP: f32 = 24.0;
/// O que a capa de um grupo passa da grade quando acende.
const TILE_BLEED: f32 = 8.0;
const ROW_GAP: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tab {
    #[default]
    Songs,
    Albums,
    Artists,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    /// A ordem do `Offline`: a mais nova primeiro.
    #[default]
    Recent,
    Title,
    Artist,
}

impl Sort {
    const ALL: [Self; 3] = [Self::Recent, Self::Title, Self::Artist];

    fn label(self) -> &'static str {
        match self {
            Self::Recent => t().sort_recent,
            Self::Title => t().sort_title,
            Self::Artist => t().sort_artist,
        }
    }
}

/// O que a pessoa escolheu na página, e os grupos já montados.
#[derive(Default)]
pub struct Browser {
    tab: Tab,
    sort: Sort,
    query: String,
    /// A chave do álbum ou artista aberto (`Group::key`).
    open: Option<String>,
    made: Option<Arc<Made>>,
}

impl Browser {
    /// Os grupos para esta lista, este filtro e esta ordem.
    fn made(&mut self, tracks: &Arc<Vec<Track>>) -> Arc<Made> {
        match &self.made {
            Some(made) if Arc::ptr_eq(&made.tracks, tracks) && made.query == self.query && made.sort == self.sort => made.clone(),
            _ => {
                let made = Arc::new(Made::new(tracks.clone(), &self.query, self.sort));
                self.made = Some(made.clone());
                made
            }
        }
    }
}

/// Um álbum ou artista: guarda as posições em `Made::tracks`.
#[derive(Debug)]
struct Group {
    /// Vazia nas músicas sem álbum (ou sem artista).
    key: String,
    title: String,
    /// O artista do álbum; `None` quando as músicas são de vários.
    artist: Option<String>,
    /// Para onde "Ir para o álbum" (ou artista) leva.
    link: Option<String>,
    tracks: Vec<usize>,
    /// A posição da música mais nova: a ordem de "Mais recentes".
    newest: usize,
    secs: u64,
    /// Alguma música do grupo passa no filtro.
    hit: bool,
}

impl Group {
    fn title(&self, tab: Tab) -> &str {
        match (self.key.is_empty(), tab) {
            (false, _) => &self.title,
            (true, Tab::Artists) => t().unknown_artist,
            (true, _) => t().no_album,
        }
    }

    /// "Gorillaz • 3 músicas" (no artista, só as músicas).
    fn line(&self, tab: Tab) -> String {
        let songs = t().songs_count(self.tracks.len());
        match tab {
            Tab::Artists => songs,
            _ if self.key.is_empty() => songs,
            _ => format!("{} • {songs}", self.artist.as_deref().unwrap_or(t().various_artists)),
        }
    }

    fn list(&self, tracks: &[Track]) -> Vec<Track> {
        self.tracks.iter().map(|&i| tracks[i].clone()).collect()
    }
}

/// As baixadas organizadas para um filtro e uma ordem.
struct Made {
    tracks: Arc<Vec<Track>>,
    query: String,
    sort: Sort,
    /// As músicas que passam no filtro, na ordem pedida.
    shown: Vec<usize>,
    albums: Vec<Group>,
    artists: Vec<Group>,
    /// As capas do alto da página: as de até quatro álbuns recentes.
    cover: Vec<usize>,
    secs: u64,
}

impl Made {
    fn new(tracks: Arc<Vec<Track>>, query: &str, sort: Sort) -> Self {
        let needle = query.trim().to_lowercase();
        let hit: Vec<bool> = tracks.iter().map(|track| matches(track, &needle)).collect();
        let mut order: Vec<usize> = (0..tracks.len()).collect();
        match sort {
            Sort::Recent => {}
            Sort::Title => order.sort_by_cached_key(|&i| tracks[i].title.to_lowercase()),
            Sort::Artist => order.sort_by_cached_key(|&i| (tracks[i].artist_names().to_lowercase(), tracks[i].title.to_lowercase())),
        }
        let mut albums = groups(&tracks, &order, &hit, album_of);
        let artists = sorted(groups(&tracks, &order, &hit, artist_of), sort);
        albums.sort_by_key(|g| g.newest);
        let cover = match albums.iter().filter(|g| !g.key.is_empty()).map(|g| g.tracks[0]).take(4).collect::<Vec<_>>() {
            four if four.len() == 4 => four,
            _ => order.first().copied().into_iter().collect(),
        };
        let albums = sorted(albums, sort);
        let shown = order.into_iter().filter(|&i| hit[i]).collect();
        let secs = tracks.iter().filter_map(|t| t.duration_secs).map(u64::from).sum();
        Self { tracks, query: query.to_owned(), sort, shown, albums, artists, cover, secs }
    }

    fn groups(&self, tab: Tab) -> &[Group] {
        match tab {
            Tab::Songs => &[],
            Tab::Albums => &self.albums,
            Tab::Artists => &self.artists,
        }
    }

    fn album_count(&self) -> usize {
        self.albums.iter().filter(|g| !g.key.is_empty()).count()
    }
}

fn matches(track: &Track, needle: &str) -> bool {
    let has = |text: &str| text.to_lowercase().contains(needle);
    needle.is_empty() || has(&track.title) || track.artists.iter().any(|a| has(&a.name)) || track.album.as_ref().is_some_and(|a| has(&a.name))
}

/// A chave de grupo da música, o nome e o link. Sem o id do YouTube, o mesmo
/// nome de álbum só é o mesmo álbum se o artista também for.
struct Key {
    key: String,
    title: String,
    link: Option<String>,
}

fn album_of(track: &Track) -> Option<Key> {
    let album = track.album.as_ref()?;
    let artist = track.artists.first().map_or("", |a| a.name.as_str());
    let key = album.id.clone().unwrap_or_else(|| format!("{}\0{}", album.name, artist).to_lowercase());
    Some(Key { key, title: album.name.clone(), link: album.id.clone() })
}

fn artist_of(track: &Track) -> Option<Key> {
    let artist = track.artists.first()?;
    Some(Key { key: artist.name.to_lowercase(), title: artist.name.clone(), link: artist.id.clone() })
}

fn groups(tracks: &[Track], order: &[usize], hit: &[bool], key_of: fn(&Track) -> Option<Key>) -> Vec<Group> {
    let mut found: Vec<Group> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for &i in order {
        let track = &tracks[i];
        let key = key_of(track).unwrap_or(Key { key: String::new(), title: String::new(), link: None });
        let first = track.artists.first().map(|a| a.name.as_str());
        let slot = *seen.entry(key.key.clone()).or_insert_with(|| {
            found.push(Group {
                key: key.key,
                title: key.title,
                artist: first.map(str::to_owned),
                link: key.link,
                tracks: Vec::new(),
                newest: i,
                secs: 0,
                hit: false,
            });
            found.len() - 1
        });
        let group = &mut found[slot];
        if group.artist.as_deref() != first {
            group.artist = None;
        }
        group.tracks.push(i);
        group.newest = group.newest.min(i);
        group.secs += u64::from(track.duration_secs.unwrap_or(0));
        group.hit |= hit[i];
    }
    found
}

/// Os grupos na ordem pedida, com o das músicas soltas sempre por último.
fn sorted(mut groups: Vec<Group>, sort: Sort) -> Vec<Group> {
    match sort {
        Sort::Recent => groups.sort_by_key(|g| g.newest),
        Sort::Title => groups.sort_by_cached_key(|g| g.title.to_lowercase()),
        Sort::Artist => groups.sort_by_cached_key(|g| (g.artist.as_deref().unwrap_or("\u{10FFFF}").to_lowercase(), g.title.to_lowercase())),
    }
    groups.sort_by_key(|g| g.key.is_empty());
    groups
}

/// `dir`: para onde vão os próximos downloads; `chosen`: a pessoa a escolheu.
pub fn show(ui: &mut Ui, cx: &mut Cx, tracks: &Arc<Vec<Track>>, dir: &std::path::Path, chosen: bool, page: &mut Browser) {
    let made = page.made(tracks);
    // O grupo aberto pode ter sumido (a última música dele foi apagada).
    let open = page.open.as_ref().and_then(|key| made.groups(page.tab).iter().find(|g| g.key == *key));
    if page.open.is_some() && open.is_none() {
        page.open = None;
    }
    let salt = ("pagina-baixadas", page.tab, page.open.clone());
    egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(ui, |ui| {
        let inner = ui.max_rect().shrink2(vec2(GUTTER, 0.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
            ui.add_space(20.0);
            if let Some(group) = open {
                group_page(ui, cx, &made, page, group);
            } else {
                overview(ui, cx, tracks, &made, chosen, dir, page);
            }
            ui.add_space(24.0);
        });
    });
}

fn overview(ui: &mut Ui, cx: &mut Cx, tracks: &Arc<Vec<Track>>, made: &Made, chosen: bool, dir: &std::path::Path, page: &mut Browser) {
    let art: Vec<&Thumbs> = made.cover.iter().map(|&i| &tracks[i].thumbs).collect();
    let mut line = t().downloads_count(tracks.len());
    if !tracks.is_empty() {
        line = format!("{line} • {} • {}", t().albums_count(made.album_count()), kit::long_time(made.secs));
    }
    hero(ui, cx, &art, false, t().downloads, &line, |ui, cx| {
        if !tracks.is_empty() {
            play_pills(ui, cx, || tracks.to_vec());
        }
        let folder = t().saving_to(&dir.display().to_string());
        if kit::pill(ui, Some(icons::FOLDER), t().open_folder, false).on_hover_text(folder).clicked() {
            cx.act(Action::OpenDownloads);
        }
        if kit::pill(ui, None, t().change_folder, false).on_hover_text(t().change_folder_hint).clicked() {
            cx.act(Action::PickDownloadsFolder);
        }
        if chosen && kit::pill(ui, None, t().default_folder, false).clicked() {
            cx.act(Action::ResetDownloadsFolder);
        }
    });
    ui.add_space(28.0);
    if tracks.is_empty() {
        empty(ui);
    } else {
        toolbar(ui, made, page);
        ui.add_space(20.0);
        // O filtro e a ordem valem já neste quadro.
        let made = page.made(tracks);
        let tab = page.tab;
        match tab {
            Tab::Songs if made.shown.is_empty() => nothing_found(ui),
            Tab::Songs => songs(ui, cx, &made, &made.shown, false),
            Tab::Albums | Tab::Artists => {
                let hits: Vec<&Group> = made.groups(tab).iter().filter(|g| g.hit).collect();
                if hits.is_empty() {
                    nothing_found(ui);
                } else {
                    grid(ui, cx, &made, tab, &hits, page);
                }
            }
        }
    }
    ui.add_space(32.0);
    let y = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), y, egui::Stroke::new(1.0, theme::GLASS_HOVER));
    ui.add_space(14.0);
    let foot = |ui: &mut Ui, text: String| ui.add(egui::Label::new(egui::RichText::new(text).color(theme::TEXT_FAINT).font(theme::regular(13.0))).wrap());
    foot(ui, t().saving_to(&dir.display().to_string()));
    if !tracks.is_empty() {
        foot(ui, t().drag_out_hint.to_owned());
    }
}

/// O álbum ou artista aberto: as músicas dele, tocando só elas.
fn group_page(ui: &mut Ui, cx: &mut Cx, made: &Made, page: &mut Browser, group: &Group) {
    let tab = page.tab;
    let back = if tab == Tab::Artists { t().artists } else { t().albums };
    if kit::pill(ui, Some(icons::CHEVRON_LEFT), back, false).clicked() {
        page.open = None;
        ui.ctx().request_repaint();
    }
    ui.add_space(16.0);
    let art = group_art(made, group);
    let line = format!("{} • {}", group.line(tab), kit::long_time(group.secs));
    hero(ui, cx, &art, tab == Tab::Artists, group.title(tab), &line, |ui, cx| {
        play_pills(ui, cx, || group.list(&made.tracks));
        if let Some(link) = &group.link {
            let (icon, label) = if tab == Tab::Artists { (icons::USER, t().go_to_artist) } else { (icons::DISC_3, t().go_to_album) };
            if kit::pill(ui, Some(icon), label, false).clicked() {
                cx.act(Action::Open(super::View::browse(link, None)));
            }
        }
    });
    ui.add_space(28.0);
    songs(ui, cx, made, &group.tracks, tab == Tab::Albums && !group.key.is_empty());
}

/// As capas de um grupo: a do álbum, ou quatro nas músicas soltas.
fn group_art<'a>(made: &'a Made, group: &Group) -> Vec<&'a Thumbs> {
    let count = if group.key.is_empty() && group.tracks.len() >= 4 { 4 } else { 1 };
    group.tracks.iter().take(count).map(|&i| &made.tracks[i].thumbs).collect()
}

/// "Tocar tudo" e "Aleatório". A lista só é montada no clique.
fn play_pills(ui: &mut Ui, cx: &mut Cx, tracks: impl Fn() -> Vec<Track>) {
    if kit::pill(ui, Some(icons::PLAY), t().play_all, true).clicked() {
        cx.act(Action::Cmd(Command::SetShuffle(false)));
        cx.act(Action::Cmd(Command::PlayTracks { tracks: tracks(), index: 0 }));
    }
    if kit::pill(ui, Some(icons::SHUFFLE), t().shuffle, false).clicked() {
        cx.act(Action::Cmd(Command::SetShuffle(true)));
        let tracks = tracks();
        let index = (crate::state::unix_now() as usize) % tracks.len();
        cx.act(Action::Cmd(Command::PlayTracks { tracks, index }));
    }
}

/// O alto da página: a capa, o título e os botões, sobre a cor da capa.
fn hero(ui: &mut Ui, cx: &mut Cx, art: &[&Thumbs], round: bool, title: &str, line: &str, actions: impl FnOnce(&mut Ui, &mut Cx)) {
    if let Some(tint) = art.first().and_then(|thumbs| cx.covers.color(thumbs)) {
        glow(ui, tint);
    }
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(HERO, HERO), Sense::hover());
        cover(ui, cx, rect, art, if round { (HERO / 2.0) as u8 } else { theme::RADIUS });
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width());
            ui.add_space(20.0);
            ui.add(egui::Label::new(egui::RichText::new(title).font(theme::bold(34.0))).truncate());
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(line).color(theme::TEXT_SUB)).truncate());
            ui.add_space(18.0);
            ui.horizontal_wrapped(|ui| actions(ui, cx));
        });
    });
}

/// A luz da cor da capa atrás do alto da página: some em todas as bordas, para
/// não marcar uma linha sob a barra de cima.
fn glow(ui: &Ui, tint: Color32) {
    const ROWS: [(f32, f32); 3] = [(0.0, 0.0), (0.3, 1.0), (1.0, 0.0)];
    const COLUMNS: [(f32, f32); 3] = [(0.0, 1.0), (0.55, 0.55), (1.0, 0.0)];
    let clip = ui.clip_rect();
    let top = ui.cursor().top() - 20.0;
    let area = Rect::from_min_max(pos2(clip.left(), top), pos2(clip.right(), top + 360.0));
    let mut mesh = Mesh::default();
    for (y, down) in ROWS {
        for (x, across) in COLUMNS {
            mesh.colored_vertex(area.lerp_inside(vec2(x, y)), tint.gamma_multiply(0.36 * down * across));
        }
    }
    for row in 0..2 {
        for column in 0..2 {
            let corner = row * 3 + column;
            mesh.add_triangle(corner, corner + 1, corner + 3);
            mesh.add_triangle(corner + 1, corner + 3, corner + 4);
        }
    }
    ui.painter().add(mesh);
}

/// Uma capa só, ou quatro em mosaico.
fn cover(ui: &Ui, cx: &mut Cx, rect: Rect, art: &[&Thumbs], radius: u8) {
    const GAP: f32 = 3.0;
    if art.len() < 4 {
        let none = Thumbs::default();
        cx.covers.paint(ui, rect, art.first().copied().unwrap_or(&none), radius);
        return;
    }
    let side = (rect.width() - GAP) / 2.0;
    for (i, thumbs) in art.iter().take(4).enumerate() {
        let at = rect.min + vec2((i % 2) as f32, (i / 2) as f32) * (side + GAP);
        cx.covers.paint(ui, Rect::from_min_size(at, vec2(side, side)), thumbs, radius / 2);
    }
}

/// As abas à esquerda; o filtro e a ordem à direita.
fn toolbar(ui: &mut Ui, made: &Made, page: &mut Browser) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    let mut left = ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    // A soma de cada aba não conta o grupo das músicas soltas.
    let hits = |tab: Tab| made.groups(tab).iter().filter(|g| g.hit && !g.key.is_empty()).count();
    for (tab, name, count) in [
        (Tab::Songs, t().songs, made.shown.len()),
        (Tab::Albums, t().albums, hits(Tab::Albums)),
        (Tab::Artists, t().artists, hits(Tab::Artists)),
    ] {
        if kit::chip(&mut left, &format!("{name}  {count}"), page.tab == tab).clicked() {
            page.tab = tab;
        }
    }
    let mut right = ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
    sort_menu(&mut right, page);
    let free = right.available_width() - left.min_rect().width() - 16.0;
    filter_field(&mut right, &mut page.query, free.clamp(120.0, 280.0));
}

fn sort_menu(ui: &mut Ui, page: &mut Browser) {
    let button = kit::pill(ui, Some(icons::CHEVRON_DOWN), page.sort.label(), false);
    egui::Popup::menu(&button).show(|ui| {
        ui.set_width(page::MENU_W);
        for sort in Sort::ALL {
            let icon: icons::Icon = if page.sort == sort { icons::CHECK } else { &[] };
            if page::menu_item(ui, icon, sort.label()) {
                page.sort = sort;
            }
        }
    });
}

fn filter_field(ui: &mut Ui, query: &mut String, width: f32) {
    let (field, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
    let cy = field.center().y;
    ui.painter().rect(field, CornerRadius::same(18), theme::GLASS, egui::Stroke::new(1.0, theme::GLASS_HOVER), egui::StrokeKind::Inside);
    let icon = Rect::from_center_size(pos2(field.left() + 20.0, cy), vec2(16.0, 16.0));
    icons::paint(ui.painter(), icon, icons::SEARCH, theme::TEXT_DIM);
    let edit = Rect::from_min_max(pos2(field.left() + 36.0, field.top()), pos2(field.right() - 32.0, field.bottom()));
    let mut child = ui.new_child(UiBuilder::new().max_rect(edit).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add(
        egui::TextEdit::singleline(query)
            .id(Id::new("filtro-baixadas"))
            .frame(egui::Frame::NONE)
            .hint_text(egui::RichText::new(t().filter_downloads).color(theme::TEXT_FAINT))
            .font(theme::regular(14.0))
            .desired_width(edit.width()),
    );
    if !query.is_empty() {
        let clear = Rect::from_center_size(pos2(field.right() - 18.0, cy), vec2(24.0, 24.0));
        let response = ui.interact(clear, Id::new("filtro-baixadas-limpar"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        kit::paint_icon_button(ui, clear, &response, icons::X, theme::TEXT_DIM);
        if response.clicked() {
            query.clear();
        }
    }
}

/// As linhas de música; cada uma toca esta lista a partir dela.
fn songs(ui: &mut Ui, cx: &mut Cx, made: &Made, list: &[usize], numbered: bool) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        for (n, &i) in list.iter().enumerate() {
            let track = &made.tracks[i];
            let number = numbered.then_some(n + 1);
            if page::track_row_with(ui, cx, track, number, false, Id::new(("baixada", &track.video_id)), true) {
                let tracks = list.iter().map(|&i| made.tracks[i].clone()).collect();
                cx.act(Action::Cmd(Command::PlayTracks { tracks, index: n }));
            }
        }
    });
}

/// As capas dos grupos, tantas por linha quantas cabem, esticadas para encher
/// a largura. Os grupos que não passam no filtro já vêm de fora.
fn grid(ui: &mut Ui, cx: &mut Cx, made: &Made, tab: Tab, hits: &[&Group], page: &mut Browser) {
    let width = ui.available_width();
    let columns = ((width + TILE_GAP) / (TILE + TILE_GAP)).floor().max(1.0);
    let side = (width - (columns - 1.0) * TILE_GAP) / columns;
    for row in hits.chunks(columns as usize) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = TILE_GAP;
            for group in row {
                if group_tile(ui, cx, made, tab, group, side) {
                    page.open = Some(group.key.clone());
                    ui.ctx().request_repaint();
                }
            }
        });
        ui.add_space(TILE_GAP - TILE_BLEED);
    }
}

/// Um álbum ou artista: o cartão acende e a capa cresce sob o mouse, com o
/// botão de tocar no canto. `true` no clique (abre o grupo).
fn group_tile(ui: &mut Ui, cx: &mut Cx, made: &Made, tab: Tab, group: &Group, side: f32) -> bool {
    let round = tab == Tab::Artists;
    let (rect, _) = ui.allocate_exact_size(vec2(side, side + 56.0), Sense::hover());
    let card = rect.expand(TILE_BLEED);
    if !ui.is_rect_visible(card) {
        return false;
    }
    let id = Id::new(("grupo-baixado", tab, &group.key));
    let response = ui.interact(card, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let art = Rect::from_min_size(rect.min, vec2(side, side));
    // Como na `tile` do catálogo: o botão existe em todo quadro e o "mouse
    // sobre o cartão" vem da posição do ponteiro, senão ele pisca.
    let button = Rect::from_center_size(art.right_bottom() - vec2(30.0, 30.0), vec2(40.0, 40.0));
    let play = ui.interact(button, id.with("tocar"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let over = ui.rect_contains_pointer(card);
    let glow = ui.ctx().animate_bool_with_time(id.with("luz"), over, 0.14);
    if glow > 0.0 {
        ui.painter().rect_filled(card, CornerRadius::same(theme::RADIUS + 2), theme::GLASS.gamma_multiply(glow));
    }
    let grown = art.expand(3.0 * glow);
    let radius = if round { (side / 2.0) as u8 } else { 6 };
    cover(ui, cx, grown, &group_art(made, group), radius);
    if glow > 0.0 && !round {
        ui.painter().rect_filled(grown, CornerRadius::same(radius), Color32::from_black_alpha((70.0 * glow) as u8));
    }
    if glow > 0.0 {
        let lit = ui.ctx().animate_bool_with_time(id.with("tocar-luz"), play.hovered(), 0.1);
        let fill = Color32::from_black_alpha(200).lerp_to_gamma(Color32::WHITE, lit).gamma_multiply(glow);
        let fg = Color32::WHITE.lerp_to_gamma(Color32::BLACK, lit).gamma_multiply(glow);
        ui.painter().circle_filled(button.center(), 20.0, fill);
        icons::paint_filled(ui.painter(), button.shrink(11.0), icons::PLAY, fg);
    }
    let below = pos2(rect.left(), art.bottom() + 10.0);
    let title = kit::one_line(ui, group.title(tab), theme::medium(14.0), theme::TEXT, side);
    let height = title.size().y;
    ui.painter().galley(below, title, theme::TEXT);
    let line = kit::one_line(ui, &group.line(tab), theme::regular(13.0), theme::TEXT_DIM, side);
    ui.painter().galley(below + vec2(0.0, height + 2.0), line, theme::TEXT_DIM);
    if play.clicked() {
        cx.act(Action::Cmd(Command::SetShuffle(false)));
        cx.act(Action::Cmd(Command::PlayTracks { tracks: group.list(&made.tracks), index: 0 }));
    }
    response.clicked()
}

fn nothing_found(ui: &mut Ui) {
    ui.add_space(28.0);
    ui.vertical_centered(|ui| ui.label(egui::RichText::new(t().nothing_found).color(theme::TEXT_DIM)));
}

fn empty(ui: &mut Ui) {
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
        icons::paint(ui.painter(), rect, icons::DOWNLOAD, theme::TEXT_FAINT);
        ui.add_space(10.0);
        ui.label(egui::RichText::new(t().no_downloads).color(theme::TEXT_DIM));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use musicfast_ytm::Link;

    fn link(name: &str, id: Option<&str>) -> Link {
        Link { name: name.to_owned(), id: id.map(str::to_owned) }
    }

    fn track(title: &str, artist: &str, album: Option<(&str, Option<&str>)>) -> Track {
        Track {
            video_id: title.to_owned(),
            title: title.to_owned(),
            artists: vec![link(artist, None)],
            album: album.map(|(name, id)| link(name, id)),
            duration_secs: Some(60),
            ..Track::default()
        }
    }

    fn library() -> Arc<Vec<Track>> {
        Arc::new(vec![
            track("Zebra", "Gorillaz", Some(("Plastic Beach", Some("a1")))),
            track("Solta", "Blur", None),
            track("Abelha", "Blur", Some(("Parklife", Some("a2")))),
            track("Melancholy Hill", "Gorillaz", Some(("Plastic Beach", Some("a1")))),
        ])
    }

    #[test]
    fn agrupa_por_album_e_deixa_as_soltas_por_ultimo() {
        let made = Made::new(library(), "", Sort::Title);
        let names: Vec<&str> = made.albums.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(names, ["Parklife", "Plastic Beach", ""]);
        assert_eq!(made.albums[1].tracks.len(), 2);
        assert_eq!(made.album_count(), 2);
        assert_eq!(made.artists.len(), 2);
        assert_eq!(made.secs, 240);
    }

    #[test]
    fn o_album_com_varios_artistas_nao_tem_um_so_artista() {
        let tracks = Arc::new(vec![track("A", "Gorillaz", Some(("Mix", Some("m")))), track("B", "Blur", Some(("Mix", Some("m"))))]);
        let made = Made::new(tracks, "", Sort::Recent);
        assert_eq!(made.albums[0].artist, None);
    }

    #[test]
    fn o_filtro_mantem_o_grupo_inteiro_para_abrir() {
        let made = Made::new(library(), "melancholy", Sort::Recent);
        assert_eq!(made.shown, [3]);
        let hits: Vec<&Group> = made.albums.iter().filter(|g| g.hit).collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].tracks.len(), 2);
    }

    #[test]
    fn a_ordem_recente_e_a_do_disco() {
        let made = Made::new(library(), "", Sort::Recent);
        assert_eq!(made.shown, [0, 1, 2, 3]);
        assert_eq!(made.artists[0].title, "Gorillaz");
    }

    #[test]
    fn a_capa_de_cima_vira_mosaico_com_quatro_albuns() {
        let tracks: Vec<Track> = ["1", "2", "3", "4", "5"].iter().map(|n| track(n, "X", Some((n, Some(n))))).collect();
        assert_eq!(Made::new(Arc::new(tracks), "", Sort::Title).cover, [0, 1, 2, 3]);
        assert_eq!(Made::new(library(), "", Sort::Recent).cover, [0]);
    }
}
