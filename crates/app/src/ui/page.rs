//! Uma página do catálogo: o cabeçalho (álbum, artista, playlist) e as
//! prateleiras, em linhas (músicas) ou em carrossel (capas). Início, busca e
//! as páginas abertas são todas desenhadas por aqui: o `musicfast-ytm`
//! converte qualquer resposta no mesmo `Page`.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use musicfast_player::Command;
use musicfast_ytm::{Card, Header, Item, Kind, LikeStatus, Page, Shelf, ShelfStyle, Track};

use super::icons;
use super::kit;
use super::theme;
use super::{Action, Cx, PageState, View, play_command};

const GUTTER: f32 = 32.0;
/// As medidas do site: a linha de música de uma playlist tem 68 de altura
/// (76 de uma para a outra); a do "Escolha a dedo", 48 (64).
const ROW: f32 = 68.0;
const ROW_GAP: f32 = 8.0;
const PICK_ROW: f32 = 48.0;
const PICK_GAP: f32 = 16.0;
/// "Escolha a dedo": músicas por coluna e a largura de cada coluna no site
/// (a de verdade estica um pouco para as colunas inteiras encherem a tela).
const PICKS_ROWS: usize = 4;
const PICKS_COLUMN: f32 = 312.0;
const TILE: f32 = 176.0;

pub fn show(ui: &mut Ui, cx: &mut Cx, view: &View, state: &PageState) {
    let Some(page) = state.page.clone() else {
        ui.add_space(80.0);
        ui.vertical_centered(|ui| match &state.error {
            Some(error) => {
                ui.label(egui::RichText::new("Não deu para carregar").font(theme::semibold(18.0)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(error).color(theme::TEXT_DIM));
                ui.add_space(12.0);
                if kit::pill(ui, Some(icons::REFRESH_CW), "Tentar de novo", false).clicked() {
                    cx.act(Action::Reload);
                }
            }
            None => kit::spinner(ui, 28.0),
        });
        return;
    };
    egui::ScrollArea::vertical()
        .id_salt(("pagina", view))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let inner = ui.max_rect().shrink2(vec2(GUTTER, 0.0));
            ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                ui.add_space(20.0);
                if let View::Search { .. } = view
                    && page.shelves.iter().all(|s| s.items.is_empty())
                {
                    ui.label(egui::RichText::new("Nada encontrado.").color(theme::TEXT_DIM));
                }
                if !page.chips.is_empty() {
                    chips(ui, cx, &page, view);
                }
                if let Some(header) = &page.header {
                    header_block(ui, cx, header, &page);
                    ui.add_space(24.0);
                }
                for (si, shelf) in page.shelves.iter().enumerate() {
                    if shelf.items.is_empty() {
                        continue;
                    }
                    shelf_block(ui, cx, &page, view, si, shelf, state);
                    ui.add_space(28.0);
                }
                if page.continuation.is_some() {
                    more_sentinel(ui, cx, state.loading_more == Some(None), Action::MoreSections);
                }
                ui.add_space(24.0);
            });
        });
}

/// Carrega mais quando o fim aparece na tela.
fn more_sentinel(ui: &mut Ui, cx: &mut Cx, loading: bool, action: Action) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        if !loading {
            cx.act(action);
        }
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Center)));
        kit::spinner(&mut child, 20.0);
    }
}

fn header_block(ui: &mut Ui, cx: &mut Cx, header: &Header, page: &Page) {
    let artist = header.kind == Kind::Artist;
    let side = if artist { 180.0 } else { 220.0 };
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
        if artist {
            // O artista é redondo, como no site.
            cx.covers.paint(ui, rect, &header.thumbs, (side / 2.0) as u8);
        } else {
            cx.covers.paint(ui, rect, &header.thumbs, theme::RADIUS);
        }
        ui.add_space(24.0);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width());
            ui.add_space(if artist { 36.0 } else { 20.0 });
            ui.add(egui::Label::new(egui::RichText::new(&header.title).font(theme::bold(34.0))).wrap());
            ui.add_space(4.0);
            let mut line = header.subtitle.clone();
            if !header.authors.is_empty() && !artist {
                let names: Vec<&str> = header.authors.iter().map(|a| a.name.as_str()).collect();
                if !line.contains(names[0]) {
                    line = if line.is_empty() { names.join(", ") } else { format!("{line} • {}", names.join(", ")) };
                }
            }
            if !line.is_empty() {
                ui.label(egui::RichText::new(line).color(theme::TEXT_DIM));
            }
            if let Some(author) = header.authors.iter().find(|a| a.id.is_some())
                && !artist
            {
                let r = ui
                    .add(egui::Label::new(egui::RichText::new(&author.name).font(theme::medium(14.0))).sense(Sense::click()))
                    .on_hover_cursor(CursorIcon::PointingHand);
                if r.clicked()
                    && let Some(id) = &author.id
                {
                    cx.act(Action::Open(View::browse(id, None)));
                }
            }
            if !header.detail.is_empty() {
                ui.label(egui::RichText::new(&header.detail).color(theme::TEXT_DIM));
            }
            if !header.description.is_empty() {
                ui.add_space(6.0);
                let text: String = header.description.chars().take(320).collect();
                let text = if header.description.chars().count() > 320 { format!("{text}…") } else { text };
                ui.add(egui::Label::new(egui::RichText::new(text).color(theme::TEXT_DIM).font(theme::regular(13.0))).wrap());
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                // As músicas só são copiadas no clique.
                let has_tracks = page.tracks().next().is_some();
                if let Some(play) = &header.play {
                    if kit::pill(ui, Some(icons::PLAY), "Tocar", true).clicked() {
                        cx.act(Action::Cmd(Command::SetShuffle(false)));
                        cx.act(Action::Play(play.clone(), None));
                    }
                } else if has_tracks && kit::pill(ui, Some(icons::PLAY), "Tocar", true).clicked() {
                    cx.act(Action::Cmd(Command::SetShuffle(false)));
                    cx.act(Action::Cmd(Command::PlayTracks { tracks: page.tracks().cloned().collect(), index: 0 }));
                }
                if has_tracks && !artist && kit::pill(ui, Some(icons::SHUFFLE), "Aleatório", false).clicked() {
                    let tracks: Vec<Track> = page.tracks().cloned().collect();
                    cx.act(Action::Cmd(Command::SetShuffle(true)));
                    let index = (crate::state::unix_now() as usize) % tracks.len();
                    cx.act(Action::Cmd(Command::PlayTracks { tracks, index }));
                }
                if let Some(radio) = &header.radio
                    && kit::pill(ui, Some(icons::RADIO), "Rádio", false).clicked()
                {
                    cx.act(Action::Cmd(Command::SetShuffle(false)));
                    cx.act(Action::Play(radio.clone(), None));
                }
                if !cx.me.signed_in {
                    return;
                }
                // Com conta: inscrever-se no artista, salvar o álbum ou a
                // playlist na biblioteca.
                if let Some(channel) = &header.channel_id {
                    let on = cx.me.subscribed.get(channel).copied().or(header.subscribed).unwrap_or(false);
                    let (icon, label) = if on { (icons::CHECK, "Inscrito") } else { (icons::PLUS, "Inscrever-se") };
                    if kit::pill(ui, Some(icon), label, false).clicked() {
                        cx.act(Action::Subscribe(channel.clone(), !on));
                    }
                } else if let Some(playlist) = header.play.as_ref().and_then(|p| p.playlist_id.clone())
                    && matches!(header.kind, Kind::Album | Kind::Playlist)
                    && playlist != "LM"
                {
                    let on = cx.me.saved.get(&playlist).copied().unwrap_or(false);
                    let (icon, label) = if on {
                        (icons::BOOKMARK_CHECK, "Na biblioteca")
                    } else {
                        (icons::BOOKMARK_PLUS, "Salvar na biblioteca")
                    };
                    if kit::pill(ui, Some(icon), label, false).clicked() {
                        cx.act(Action::SaveCollection(playlist, !on));
                    }
                }
            });
        });
    });
}

/// O título da prateleira, como no site: 34 em negrito e, à direita, o
/// "Mais", o "Tocar tudo" do "Escolha a dedo" e as setas das que rolam de
/// lado (`scroller` com o mesmo `id`).
fn shelf_title(ui: &mut Ui, cx: &mut Cx, shelf: &Shelf, arrows: Option<Id>, play_all: bool) {
    if shelf.title.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        ui.set_min_height(41.0);
        ui.label(egui::RichText::new(&shelf.title).font(theme::bold(34.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if let Some(id) = arrows {
                let [back, forward] = ui.data(|d| d.get_temp::<[bool; 2]>(id.with("pode"))).unwrap_or_default();
                if arrow(ui, icons::CHEVRON_RIGHT, forward) {
                    ui.data_mut(|d| d.insert_temp(id.with("rolar"), 1.0f32));
                }
                ui.add_space(16.0);
                if arrow(ui, icons::CHEVRON_LEFT, back) {
                    ui.data_mut(|d| d.insert_temp(id.with("rolar"), -1.0f32));
                }
                ui.add_space(24.0);
            }
            if play_all && outline_pill(ui, "Tocar tudo") {
                cx.act(Action::Cmd(Command::SetShuffle(false)));
                cx.act(Action::Cmd(Command::PlayTracks { tracks: shelf_tracks(shelf), index: 0 }));
            }
            if let Some(more) = &shelf.more {
                if play_all {
                    ui.add_space(8.0);
                }
                if outline_pill(ui, "Mais") {
                    cx.act(Action::Open(View::browse(&more.id, more.params.as_deref())));
                }
            }
        });
    });
    ui.add_space(16.0);
}

/// O contorno branco a 20% dos botões do título no site.
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(51, 51, 51, 51);

/// O botão contornado do título ("Tocar tudo", "Mais"): 36 de altura,
/// texto 14 médio.
fn outline_pill(ui: &mut Ui, text: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), theme::medium(14.0), theme::TEXT);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 30.0, 36.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(18), theme::HOVER);
    }
    ui.painter().rect_stroke(rect.shrink(0.5), CornerRadius::same(18), egui::Stroke::new(1.0, OUTLINE), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, theme::TEXT);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// A seta redonda de rolar a prateleira, contornada como no site; a 40%
/// quando já está no começo (ou no fim).
fn arrow(ui: &mut Ui, icon: icons::Icon, enabled: bool) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 36.0), sense);
    let c = rect.center();
    if enabled && response.hovered() {
        ui.painter().circle_filled(c, 18.0, theme::HOVER);
    }
    let (stroke, color) = if enabled {
        (OUTLINE, theme::TEXT)
    } else {
        (OUTLINE.gamma_multiply(0.4), Color32::from_gray(155).gamma_multiply(0.4))
    };
    ui.painter().circle_stroke(c, 17.5, egui::Stroke::new(1.0, stroke));
    icons::paint(ui.painter(), rect.shrink(10.0), icon, color);
    enabled && response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// A rolagem das setas em andamento: de onde, para onde e quando começou.
#[derive(Clone, Copy, Default)]
struct Glide {
    from: f32,
    to: f32,
    start: f64,
}

/// Quanto dura a rolagem das setas (a do site leva por volta de meio segundo).
const GLIDE_SECS: f64 = 0.55;

/// A linha que rola de lado (carrossel, "Escolha a dedo"). Cada seta do
/// título anda as capas (ou colunas) inteiras que cabem na tela, parando na
/// borda de uma delas e nunca além do fim, e desacelera no final como no
/// site. `pitch`: de uma capa (coluna) para a próxima. Daqui sai para o
/// próximo quadro se ainda dá para rolar em cada sentido.
fn scroller(ui: &mut Ui, id: Id, pitch: f32, add_contents: impl FnOnce(&mut Ui)) {
    let visible = ui.available_width();
    let now = ui.input(|i| i.time);
    let max = ui.data(|d| d.get_temp::<f32>(id.with("max"))).unwrap_or(0.0);
    let offset = ui.data(|d| d.get_temp::<f32>(id.with("x"))).unwrap_or(0.0);
    let mut glide = ui.data(|d| d.get_temp::<Glide>(id.with("glide")));
    if let Some(dir) = ui.data_mut(|d| d.remove_temp::<f32>(id.with("rolar"))) {
        // Um clique no meio da rolagem continua de onde ela ia parar.
        let base = glide.map_or(offset, |g| g.to);
        let page = ((visible + 1.0) / pitch).floor().max(1.0) * pitch;
        let to = (((base + dir * page) / pitch).round() * pitch).clamp(0.0, max);
        glide = Some(Glide { from: offset, to, start: now });
    }
    let mut area = egui::ScrollArea::horizontal()
        .id_salt(id)
        .auto_shrink([false, true])
        // Sem barra, como no site: rola pelas setas ou pela roda de lado.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(g) = glide {
        let t = ((now - g.start) / GLIDE_SECS).clamp(0.0, 1.0) as f32;
        // Sai rápido e chega devagar (cúbica), como o `smooth` do navegador.
        let eased = 1.0 - (1.0 - t).powi(3);
        area = area.horizontal_scroll_offset(g.from + (g.to - g.from) * eased);
        if t < 1.0 {
            ui.data_mut(|d| d.insert_temp(id.with("glide"), g));
            ui.ctx().request_repaint();
        } else {
            ui.data_mut(|d| d.remove_temp::<Glide>(id.with("glide")));
        }
    }
    let out = area.show(ui, add_contents);
    let max = (out.content_size.x - out.inner_rect.width()).max(0.0);
    ui.data_mut(|d| {
        d.insert_temp(id.with("max"), max);
        d.insert_temp(id.with("x"), out.state.offset.x);
    });
    let can = [out.state.offset.x > 1.0, out.state.offset.x < max - 1.0];
    // O título já foi desenhado com o valor velho: mudou, desenha de novo.
    if ui.data(|d| d.get_temp::<[bool; 2]>(id.with("pode"))) != Some(can) {
        ui.data_mut(|d| d.insert_temp(id.with("pode"), can));
        ui.ctx().request_repaint();
    }
}

/// Os chips de humor do início ("Relax", "Energia"…).
fn chips(ui: &mut Ui, cx: &mut Cx, page: &Page, view: &View) {
    let current = match view {
        View::Browse { params, .. } => params.clone(),
        _ => None,
    };
    ui.horizontal_wrapped(|ui| {
        for chip in &page.chips {
            let selected = chip.selected || (current.is_some() && chip.target.params == current);
            if kit::chip(ui, &chip.title, selected).clicked() {
                // Clicar no chip escolhido volta ao início sem filtro.
                if selected {
                    cx.act(Action::Open(View::Home));
                } else {
                    cx.act(Action::Open(View::browse(&chip.target.id, chip.target.params.as_deref())));
                }
            }
        }
    });
    ui.add_space(20.0);
}

fn shelf_block(ui: &mut Ui, cx: &mut Cx, page: &Page, view: &View, si: usize, shelf: &Shelf, state: &PageState) {
    let buttons = shelf.items.iter().all(|i| matches!(i, Item::Card(c) if c.thumbs.is_empty() && c.play.is_none()));
    // "Escolha a dedo" e as outras listas soltas do início: colunas de
    // músicas que rolam de lado, como no site, em vez de uma lista comprida
    // com o álbum lá na ponta da tela.
    let columns = page.header.is_none()
        && matches!(view, View::Home | View::Explore | View::Browse { .. })
        && shelf.style == ShelfStyle::List
        && shelf.continuation.is_none()
        && shelf.items.len() > PICKS_ROWS
        && shelf.items.iter().all(|i| matches!(i, Item::Track(_)));
    let rolls = !buttons && (columns || shelf.style == ShelfStyle::Carousel);
    let id = Id::new(("prateleira", si, &shelf.title));
    shelf_title(ui, cx, shelf, rolls.then_some(id), columns);
    match shelf.style {
        _ if buttons => mood_grid(ui, cx, shelf),
        _ if columns => picks(ui, cx, si, shelf, id),
        ShelfStyle::Carousel => carousel(ui, cx, si, shelf, id),
        ShelfStyle::Grid => {
            grid(ui, cx, si, shelf);
            if shelf.continuation.is_some() {
                more_sentinel(ui, cx, state.loading_more == Some(Some(si)), Action::MoreItems(si));
            }
        }
        ShelfStyle::List => {
            // Num álbum ou playlist, a linha toca a lista daqui em diante; na
            // busca e no início, o rádio da música (como o site).
            let list_page = page.header.as_ref().is_some_and(|h| matches!(h.kind, Kind::Album | Kind::Playlist))
                || matches!(view, View::Library(_));
            let numbered = page.header.as_ref().is_some_and(|h| h.kind == Kind::Album);
            let playlist_id = page.header.as_ref().and_then(|h| h.play.as_ref()).and_then(|p| p.playlist_id.as_deref());
            let artist_page = page.header.as_ref().is_some_and(|h| h.kind == Kind::Artist);
            // O comando de tocar só é montado no clique: montá-lo por linha
            // e por quadro copiava a lista inteira em cada linha (o quadro
            // de uma playlist de 2.000 músicas custava 1,6 s).
            let play = |track: &Track, index: usize| {
                if list_page
                    && shelf.continuation.is_some()
                    && let Some(playlist_id) = playlist_id
                {
                    // A playlist ainda não veio inteira: a fila do YouTube
                    // a traz toda.
                    Command::PlayList { seed: Some(track.clone()), playlist_id: playlist_id.to_owned(), params: None }
                } else if list_page || artist_page {
                    Command::PlayTracks { tracks: shelf_tracks(shelf), index }
                } else {
                    Command::PlayRadio { seed: track.clone() }
                }
            };
            // No site as linhas ficam a 8 uma da outra.
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = ROW_GAP;
                let mut n = 0;
                for item in &shelf.items {
                    match item {
                        Item::Track(track) => {
                            let index = n;
                            n += 1;
                            let number = numbered.then_some(index + 1);
                            if track_row(ui, cx, track, number, false, Id::new(("linha", si, index))) {
                                cx.act(Action::Cmd(play(track, index)));
                            }
                        }
                        Item::Card(card) => card_row(ui, cx, card),
                    }
                }
            });
            if shelf.continuation.is_some() {
                more_sentinel(ui, cx, state.loading_more == Some(Some(si)), Action::MoreItems(si));
            }
        }
    }
}

/// As músicas da prateleira, na ordem (para tocar a lista).
fn shelf_tracks(shelf: &Shelf) -> Vec<Track> {
    shelf
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Track(t) => Some(t.clone()),
            Item::Card(_) => None,
        })
        .collect()
}

/// Uma música em linha, como no site: capa 48 (ou o número da faixa), título
/// em 16 médio e, embaixo, artistas • álbum em 16 a 70%, cada um com metade
/// da largura; à direita a duração, que sob o mouse dá lugar a curtir, não
/// curtir e o menu. `compact`: a do "Escolha a dedo" (48 de altura, sem
/// duração). No álbum (`number`), o álbum não se repete. `true` no clique
/// (quem chama monta o comando de tocar só então). Fora da tela, só reserva
/// o espaço.
pub fn track_row(ui: &mut Ui, cx: &mut Cx, track: &Track, number: Option<usize>, compact: bool, id: Id) -> bool {
    let width = ui.available_width();
    let (height, pad, radius) = if compact { (PICK_ROW, 8.0, 4) } else { (ROW, 16.0, 2) };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let response = ui.interact(rect, id, Sense::click());
    let menu_open = response.context_menu_opened();
    let current = cx.current.as_deref() == Some(track.video_id.as_str());
    // O site não pinta a linha sob o mouse: escurece a capa e mostra os botões.
    let hovered = response.hovered() || menu_open || ui.rect_contains_pointer(rect);
    if response.hovered() {
        cx.act(Action::Warm(track.video_id.clone()));
    }
    let p = ui.painter().clone();
    let lead = Rect::from_min_size(pos2(rect.left() + pad, rect.center().y - 24.0), vec2(48.0, 48.0));
    match &number {
        Some(n) => {
            if current {
                equalizer(&p, lead.shrink(14.0), cx.playing);
            } else if hovered {
                icons::paint_filled(&p, lead.shrink(12.0), icons::PLAY, theme::TEXT);
            } else {
                p.text(lead.center(), Align2::CENTER_CENTER, n.to_string(), theme::regular(16.0), theme::TEXT_SUB);
            }
        }
        None => {
            cx.covers.paint(ui, lead, &track.thumbs, radius);
            if current || hovered {
                p.rect_filled(lead, CornerRadius::same(radius), Color32::from_black_alpha(150));
                if current {
                    equalizer(&p, lead.shrink(14.0), cx.playing);
                } else {
                    icons::paint_filled(&p, lead.shrink(12.0), icons::PLAY, theme::TEXT);
                }
            }
        }
    }
    let x = lead.right() + 16.0;
    let right = rect.right() - pad;

    // Os botões do mouse: curtir e não curtir só com conta.
    const BUTTON: f32 = 36.0;
    let buttons = if cx.me.signed_in { 3.0 } else { 1.0 };
    let reserve = if hovered {
        buttons * (BUTTON + 4.0)
    } else if compact {
        0.0
    } else {
        64.0
    };
    let text_w = (right - reserve - x - 8.0).max(40.0);
    let (title_y, sub_y) = if compact { (rect.top() + 3.0, rect.top() + 25.0) } else { (rect.top() + 11.0, rect.top() + 38.0) };

    let title = kit::one_line(ui, &track.title, theme::medium(16.0), theme::TEXT, text_w);
    p.galley(pos2(x, title_y), title, theme::TEXT);

    // A segunda linha: o "E" vem antes dos artistas, como no site.
    let mut sx = x;
    if track.explicit {
        let badge = Rect::from_min_size(pos2(sx, sub_y + 2.5), vec2(14.0, 14.0));
        p.rect_filled(badge, CornerRadius::same(2), theme::TEXT_SUB);
        p.text(badge.center(), Align2::CENTER_CENTER, "E", theme::bold(10.0), Color32::BLACK);
        sx += 20.0;
    }
    let sub_w = (x + text_w - sx).max(20.0);
    let album = track.album.as_ref().filter(|_| number.is_none());
    let bullet = " • ";
    let bullet_w = if album.is_some() {
        ui.painter().layout_no_wrap(bullet.to_owned(), theme::regular(16.0), theme::TEXT_SUB).size().x
    } else {
        0.0
    };
    let half = if album.is_some() { (sub_w - 4.0) / 2.0 } else { sub_w };
    let artists = kit::one_line(ui, &track.artist_names(), theme::regular(16.0), theme::TEXT_SUB, half - bullet_w);
    let artists_w = artists.size().x;
    let artist_id = track.artists.iter().find_map(|a| a.id.clone());
    if let Some(r) = kit::text_link(ui, pos2(sx, sub_y), artists, theme::TEXT_SUB, id.with("artista"), artist_id.is_some())
        && r.clicked()
        && let Some(artist) = &artist_id
    {
        cx.act(Action::Open(View::browse(artist, None)));
    }
    if let Some(album) = album {
        p.text(pos2(sx + artists_w, sub_y), Align2::LEFT_TOP, bullet, theme::regular(16.0), theme::TEXT_SUB);
        // O álbum vem logo depois do "•", sem buraco quando o artista é curto.
        let album_x = sx + artists_w + bullet_w;
        let galley = kit::one_line(ui, &album.name, theme::regular(16.0), theme::TEXT_SUB, sx + sub_w - album_x);
        if let Some(r) = kit::text_link(ui, pos2(album_x, sub_y), galley, theme::TEXT_SUB, id.with("album"), album.id.is_some())
            && r.clicked()
            && let Some(id) = &album.id
        {
            cx.act(Action::Open(View::browse(id, None)));
        }
    }

    if hovered {
        let mut bx = right - BUTTON;
        let menu_rect = Rect::from_min_size(pos2(bx, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
        let menu = ui.interact(menu_rect, id.with("menu"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        kit::paint_icon_button(ui, menu_rect, &menu, icons::ELLIPSIS, theme::TEXT);
        egui::Popup::menu(&menu).show(|ui| track_menu(ui, cx, track));
        if cx.me.signed_in {
            let status = cx.me.like(&track.video_id);
            for (icon, mine, key) in [(icons::THUMBS_DOWN, LikeStatus::Dislike, "nao"), (icons::THUMBS_UP, LikeStatus::Like, "sim")] {
                bx -= BUTTON + 4.0;
                let r = Rect::from_min_size(pos2(bx, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
                let b = ui.interact(r, id.with(key), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                kit::paint_icon_button(ui, r, &b, icon, theme::TEXT);
                if status == mine {
                    icons::paint_filled(ui.painter(), r.shrink(r.width() * 0.25), icon, theme::TEXT);
                }
                if b.clicked() {
                    let next = if status == mine { LikeStatus::Indifferent } else { mine };
                    cx.act(Action::Rate(track.video_id.clone(), next));
                }
            }
        }
    } else if !compact && let Some(d) = track.duration_secs {
        p.text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, kit::time(d as f64), theme::regular(16.0), theme::TEXT_SUB);
    }
    let clicked = response.clicked();
    response.context_menu(|ui| track_menu(ui, cx, track));
    clicked
}

/// As barrinhas da música que toca (paradas quando pausada).
fn equalizer(p: &egui::Painter, rect: Rect, moving: bool) {
    let t = if moving { p.ctx().input(|i| i.time) } else { 0.0 };
    let w = rect.width() / 5.0;
    for (i, phase) in [0.0f64, 1.7, 0.9].iter().enumerate() {
        let level = if moving { 0.35 + 0.65 * ((t * 7.0 + phase).sin() * 0.5 + 0.5) } else { 0.4 } as f32;
        let h = rect.height() * level;
        let x = rect.left() + w * (i as f32 * 2.0);
        p.rect_filled(Rect::from_min_max(pos2(x, rect.bottom() - h), pos2(x + w, rect.bottom())), CornerRadius::same(1), theme::ACCENT);
    }
    if moving {
        p.ctx().request_repaint_after(std::time::Duration::from_millis(60));
    }
}

/// A largura dos menus de música: fixa, para o popup não esticar até a
/// largura da janela (como acontecia no menu ⋮ da barra de baixo).
pub const MENU_W: f32 = 240.0;

pub fn track_menu(ui: &mut Ui, cx: &mut Cx, track: &Track) {
    ui.set_width(MENU_W);
    if cx.me.signed_in {
        let liked = cx.me.like(&track.video_id) == LikeStatus::Like;
        let label = if liked { "Remover das curtidas" } else { "Curtir" };
        if menu_item(ui, icons::THUMBS_UP, label) {
            let status = if liked { LikeStatus::Indifferent } else { LikeStatus::Like };
            cx.act(Action::Rate(track.video_id.clone(), status));
        }
        if menu_item(ui, icons::LIST_PLUS, "Salvar em uma playlist") {
            cx.act(Action::Dialog(super::Dialog::SaveTo { track: track.clone() }));
        }
        ui.separator();
    }
    if menu_item(ui, icons::LIST_PLUS, "Tocar a seguir") {
        cx.act(Action::Cmd(Command::PlayNext(track.clone())));
    }
    if menu_item(ui, icons::LIST_END, "Adicionar à fila") {
        cx.act(Action::Cmd(Command::Enqueue(track.clone())));
    }
    if menu_item(ui, icons::RADIO, "Iniciar rádio") {
        cx.act(Action::Cmd(Command::PlayRadio { seed: track.clone() }));
    }
    if let Some(id) = track.album.as_ref().and_then(|a| a.id.as_ref())
        && menu_item(ui, icons::DISC_3, "Ir para o álbum")
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
    if let Some(id) = track.artists.iter().find_map(|a| a.id.as_ref())
        && menu_item(ui, icons::USER, "Ir para o artista")
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
}

fn menu_item(ui: &mut Ui, icon: icons::Icon, text: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(MENU_W, 34.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), theme::HOVER);
    }
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0));
    icons::paint(ui.painter(), icon_rect, icon, theme::TEXT_DIM);
    ui.painter().text(pos2(rect.left() + 38.0, rect.center().y), Align2::LEFT_CENTER, text, theme::regular(14.0), theme::TEXT);
    let clicked = response.on_hover_cursor(CursorIcon::PointingHand).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Um álbum, artista ou playlist numa lista (resultado de busca).
fn card_row(ui: &mut Ui, cx: &mut Cx, card: &Card) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), theme::SURFACE);
    }
    let lead = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 24.0), vec2(48.0, 48.0));
    let radius = if card.kind == Kind::Artist { 24 } else { 4 };
    cx.covers.paint(ui, lead, &card.thumbs, radius);
    let x = lead.right() + 14.0;
    let w = rect.right() - x - 48.0;
    let title = kit::one_line(ui, &card.title, theme::medium(14.5), theme::TEXT, w);
    ui.painter().galley(pos2(x, rect.center().y - 19.0), title, theme::TEXT);
    let sub = kit::one_line(ui, &card.subtitle, theme::regular(13.0), theme::TEXT_DIM, w);
    ui.painter().galley(pos2(x, rect.center().y + 2.0), sub, theme::TEXT_DIM);
    let chevron = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(18.0, 18.0));
    icons::paint(ui.painter(), chevron, icons::CHEVRON_RIGHT, theme::TEXT_FAINT);
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        open_card(cx, card);
    }
}

fn open_card(cx: &mut Cx, card: &Card) {
    if !card.browse_id.is_empty() {
        cx.act(Action::Open(View::browse(&card.browse_id, card.params.as_deref())));
    } else if let Some(play) = &card.play {
        cx.act(Action::Play(play.clone(), None));
    }
}

/// As capas em grade que quebra linha (a biblioteca).
fn grid(ui: &mut Ui, cx: &mut Cx, si: usize, shelf: &Shelf) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(18.0, 18.0);
        for (i, item) in shelf.items.iter().enumerate() {
            tile(ui, cx, item, Id::new(("grade", si, i)));
        }
    });
}

/// "Momentos e gêneros": botões de texto com a faixa de cor à esquerda.
fn mood_grid(ui: &mut Ui, cx: &mut Cx, shelf: &Shelf) {
    const PALETTE: [Color32; 8] = [
        Color32::from_rgb(232, 64, 87),
        Color32::from_rgb(255, 160, 0),
        Color32::from_rgb(76, 175, 80),
        Color32::from_rgb(3, 169, 244),
        Color32::from_rgb(156, 39, 176),
        Color32::from_rgb(255, 87, 34),
        Color32::from_rgb(0, 150, 136),
        Color32::from_rgb(233, 30, 99),
    ];
    let width = ui.available_width();
    let columns = ((width + 16.0) / 196.0).floor().max(1.0);
    let button_w = (width - (columns - 1.0) * 16.0) / columns;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 14.0);
        for (i, item) in shelf.items.iter().enumerate() {
            let Item::Card(card) = item else { continue };
            let (rect, response) = ui.allocate_exact_size(vec2(button_w, 48.0), Sense::click());
            let fill = if response.hovered() { theme::ACTIVE } else { theme::HOVER };
            ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
            let stripe = Rect::from_min_size(rect.min, vec2(6.0, rect.height()));
            ui.painter().rect_filled(stripe, CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 }, PALETTE[i % PALETTE.len()]);
            let t = kit::one_line(ui, &card.title, theme::medium(14.5), theme::TEXT, button_w - 32.0);
            ui.painter().galley(pos2(rect.left() + 20.0, rect.center().y - t.size().y / 2.0), t, theme::TEXT);
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                open_card(cx, card);
            }
        }
    });
}

fn carousel(ui: &mut Ui, cx: &mut Cx, si: usize, shelf: &Shelf, id: Id) {
    scroller(ui, id, TILE + 18.0, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            for (i, item) in shelf.items.iter().enumerate() {
                tile(ui, cx, item, Id::new(("tile", si, i)));
            }
        });
    });
}

/// "Escolha a dedo": colunas de `PICKS_ROWS` músicas, tantas inteiras
/// quantas cabem na largura, e o resto rolando de lado. Cada linha toca o
/// rádio da música, como no site.
fn picks(ui: &mut Ui, cx: &mut Cx, si: usize, shelf: &Shelf, id: Id) {
    const GAP: f32 = 24.0;
    let width = ui.available_width();
    let fit = ((width + GAP) / (PICKS_COLUMN + GAP)).floor().max(1.0);
    let column_w = (width - GAP * (fit - 1.0)) / fit;
    let height = PICKS_ROWS as f32 * (PICK_ROW + PICK_GAP) - PICK_GAP;
    scroller(ui, id, column_w + GAP, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (c, chunk) in shelf.items.chunks(PICKS_ROWS).enumerate() {
                let (rect, _) = ui.allocate_exact_size(vec2(column_w, height), Sense::hover());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let mut column = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
                column.spacing_mut().item_spacing.y = PICK_GAP;
                for (r, item) in chunk.iter().enumerate() {
                    let Item::Track(track) = item else { continue };
                    let index = c * PICKS_ROWS + r;
                    if track_row(&mut column, cx, track, None, true, Id::new(("linha", si, index))) {
                        cx.act(Action::Cmd(Command::PlayRadio { seed: track.clone() }));
                    }
                }
            }
        });
    });
}

/// Uma capa grande com título e subtítulo.
fn tile(ui: &mut Ui, cx: &mut Cx, item: &Item, id: Id) {
    let (title, subtitle, thumbs, round) = match item {
        Item::Track(t) => (t.title.as_str(), t.artist_names(), &t.thumbs, false),
        Item::Card(c) => (c.title.as_str(), c.subtitle.clone(), &c.thumbs, c.kind == Kind::Artist),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(TILE, TILE + 56.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let art = Rect::from_min_size(rect.min, vec2(TILE, TILE));
    let response = ui.interact(art, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    cx.covers.paint(ui, art, thumbs, if round { (TILE / 2.0) as u8 } else { 6 });
    let current = matches!(item, Item::Track(t) if cx.current.as_deref() == Some(t.video_id.as_str()));
    let playable = match item {
        Item::Track(_) => true,
        Item::Card(c) => c.play.is_some(),
    };
    // O botão de tocar no canto, como no site. Ele existe em todo quadro, e
    // o "mouse sobre a capa" vem da posição do ponteiro, não do `hovered()`
    // da capa: o botão por cima tira o hover dela. Quando o botão só nascia
    // com a capa em hover, entrar nele apagava o hover, o botão sumia no
    // quadro seguinte, a capa voltava a ter hover, e assim por diante: o
    // card piscava, e o clique caía na capa e abria o álbum em vez de tocar.
    let button = Rect::from_center_size(art.right_bottom() - vec2(30.0, 30.0), vec2(40.0, 40.0));
    let play = playable.then(|| ui.interact(button, id.with("tocar"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand));
    let over = ui.rect_contains_pointer(art);
    // Aparecer e sumir em ~120 ms, em vez de piscar de um quadro para o outro.
    let shade = ui.ctx().animate_bool_with_time(id.with("hover"), over, 0.12);
    if shade > 0.0 && !round {
        ui.painter().rect_filled(art, CornerRadius::same(6), Color32::from_black_alpha((90.0 * shade) as u8));
    }
    let mut play_clicked = false;
    if let Some(play) = play {
        let shown = if current { 1.0 } else { shade };
        if shown > 0.0 {
            let lit = ui.ctx().animate_bool_with_time(id.with("tocar-luz"), play.hovered(), 0.1);
            let fill = Color32::from_black_alpha(200).lerp_to_gamma(Color32::WHITE, lit).gamma_multiply(shown);
            let fg = Color32::WHITE.lerp_to_gamma(Color32::BLACK, lit).gamma_multiply(shown);
            ui.painter().circle_filled(button.center(), 20.0, fill);
            icons::paint_filled(ui.painter(), button.shrink(11.0), icons::PLAY, fg);
        }
        play_clicked = play.clicked();
    }
    let below = pos2(rect.left(), art.bottom() + 8.0);
    let t = kit::one_line(ui, title, theme::medium(14.0), theme::TEXT, TILE);
    let th = t.size().y;
    ui.painter().galley(below, t, theme::TEXT);
    let s = kit::one_line(ui, &subtitle, theme::regular(13.0), theme::TEXT_DIM, TILE);
    ui.painter().galley(below + vec2(0.0, th + 2.0), s, theme::TEXT_DIM);

    match item {
        Item::Track(track) => {
            if over {
                cx.act(Action::Warm(track.video_id.clone()));
            }
            if response.clicked() || play_clicked {
                cx.act(Action::Cmd(Command::PlayRadio { seed: track.clone() }));
            }
            response.context_menu(|ui| track_menu(ui, cx, track));
        }
        Item::Card(card) => {
            if play_clicked {
                if let Some(cmd) = card.play.as_ref().and_then(|play| play_command(play, None)) {
                    cx.act(Action::Cmd(cmd));
                }
            } else if response.clicked() {
                open_card(cx, card);
            }
        }
    }
}
