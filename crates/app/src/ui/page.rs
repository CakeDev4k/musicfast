//! Uma página do catálogo: o cabeçalho (álbum, artista, playlist) e as
//! prateleiras, em linhas (músicas) ou em carrossel (capas). Início, busca e
//! as páginas abertas são todas desenhadas por aqui: o `musicfast-ytm`
//! converte qualquer resposta no mesmo `Page`.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::Command;
use musicfast_player::offline::{State as Download, Step};
use musicfast_ytm::{Card, Header, Item, Kind, LikeStatus, Page, Shelf, ShelfStyle, Track};

use super::icons;
use super::kit;
use super::theme;
use super::panels::Columns;
use super::{Action, Cx, PageState, View, play_command};

pub(super) const GUTTER: f32 = 32.0;
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
/// Quanto a linha acesa da página "Baixadas" passa da coluna, de cada lado: a
/// capa fica alinhada com o título da página.
const ROW_BLEED: f32 = 12.0;
pub(super) const TILE: f32 = 176.0;
/// A margem de dentro do cartão dos painéis, que acende sob o mouse.
const TILE_PAD: f32 = 12.0;

pub fn show(ui: &mut Ui, cx: &mut Cx, view: &View, state: &PageState) {
    let Some(page) = state.page.clone() else {
        ui.add_space(80.0);
        ui.vertical_centered(|ui| match &state.error {
            Some(error) => {
                ui.label(egui::RichText::new(t().load_failed).font(theme::semibold(18.0)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(error).color(theme::TEXT_DIM));
                ui.add_space(12.0);
                if kit::pill(ui, Some(icons::REFRESH_CW), t().try_again, false).clicked() {
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
                    ui.label(egui::RichText::new(t().nothing_found).color(theme::TEXT_DIM));
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
    if theme::panels() {
        super::panels::header(ui, cx, header, page);
        return;
    }
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
                    if kit::pill(ui, Some(icons::PLAY), t().play, true).clicked() {
                        cx.act(Action::Cmd(Command::SetShuffle(false)));
                        cx.act(Action::Play(play.clone(), None));
                    }
                } else if has_tracks && kit::pill(ui, Some(icons::PLAY), t().play, true).clicked() {
                    cx.act(Action::Cmd(Command::SetShuffle(false)));
                    cx.act(Action::Cmd(Command::PlayTracks { tracks: page.tracks().cloned().collect(), index: 0 }));
                }
                if has_tracks && !artist && kit::pill(ui, Some(icons::SHUFFLE), t().shuffle, false).clicked() {
                    let tracks: Vec<Track> = page.tracks().cloned().collect();
                    cx.act(Action::Cmd(Command::SetShuffle(true)));
                    let index = (crate::state::unix_now() as usize) % tracks.len();
                    cx.act(Action::Cmd(Command::PlayTracks { tracks, index }));
                }
                if let Some(radio) = &header.radio
                    && kit::pill(ui, Some(icons::RADIO), t().radio, false).clicked()
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
                    let (icon, label) = if on { (icons::CHECK, t().subscribed) } else { (icons::PLUS, t().subscribe) };
                    if kit::pill(ui, Some(icon), label, false).clicked() {
                        cx.act(Action::Subscribe(channel.clone(), !on));
                    }
                } else if let Some(playlist) = header.play.as_ref().and_then(|p| p.playlist_id.clone())
                    && matches!(header.kind, Kind::Album | Kind::Playlist)
                    && playlist != "LM"
                {
                    let on = cx.me.saved.get(&playlist).copied().unwrap_or(false);
                    let (icon, label) = if on {
                        (icons::BOOKMARK_CHECK, t().in_library)
                    } else {
                        (icons::BOOKMARK_PLUS, t().save_to_library)
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
    // Nos painéis, o título é menor e o "Mais" vira um link.
    let panels = theme::panels();
    let title_font = if panels { theme::bold(24.0) } else { theme::bold(34.0) };
    // Os botões primeiro, da direita para a esquerda; o título fica com o
    // que sobra, cortado com reticências, em vez de passar por baixo deles.
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), if panels { 36.0 } else { 41.0 }), Sense::hover());
    let mut buttons = ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
    {
        let ui = &mut buttons;
        ui.spacing_mut().item_spacing.x = 0.0;
        {
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
            if play_all && outline_pill(ui, t().play_all) {
                cx.act(Action::Cmd(Command::SetShuffle(false)));
                cx.act(Action::Cmd(Command::PlayTracks { tracks: shelf_tracks(shelf), index: 0 }));
            }
            if let Some(more) = &shelf.more {
                if play_all {
                    ui.add_space(8.0);
                }
                let open = if panels { show_all_link(ui) } else { outline_pill(ui, t().more) };
                if open {
                    cx.act(Action::Open(View::browse(&more.id, more.params.as_deref())));
                }
            }
        }
    }
    let used = buttons.min_rect().width();
    let gap = if used > 0.0 { 24.0 } else { 0.0 };
    let width = (row.width() - used - gap).max(40.0);
    let title = kit::one_line(ui, &shelf.title, title_font, theme::TEXT, width);
    let resp = ui.interact(Rect::from_min_size(row.min, vec2(title.size().x, row.height())), Id::new(("titulo-prateleira", &shelf.title)), Sense::hover());
    if title.size().x >= width - 1.0 {
        // Cortado: o nome inteiro na dica.
        resp.on_hover_text(&shelf.title);
    }
    ui.painter().galley(pos2(row.left(), row.center().y - title.size().y / 2.0), title, theme::TEXT);
    ui.add_space(16.0);
}

/// O "Mostrar tudo" dos painéis.
fn show_all_link(ui: &mut Ui) -> bool {
    let galley = ui.painter().layout_no_wrap(t().show_all.to_owned(), theme::medium(14.0), theme::TEXT_DIM);
    let (rect, response) = ui.allocate_exact_size(galley.size() + vec2(8.0, 12.0), Sense::click());
    let color = if response.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    let at = rect.center() - galley.size() / 2.0;
    if response.hovered() {
        ui.painter().hline(at.x..=at.x + galley.size().x, at.y + galley.size().y, egui::Stroke::new(1.0, color));
    }
    ui.painter().galley_with_override_text_color(at, galley, color);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
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
            let panels = theme::panels();
            // A tabela dos painéis: álbum sem capa repetida; playlist com a
            // coluna do álbum; busca e início, só capa e título.
            let columns = Columns { numbered: list_page, cover: !numbered, album: list_page && !numbered };
            let mut items = shelf.items.as_slice();
            if panels && matches!(view, View::Search { .. }) && si == 0
                && let Some(Item::Card(card)) = items.first()
            {
                super::panels::top_result(ui, cx, card);
                ui.add_space(12.0);
                items = &items[1..];
            }
            if panels && list_page {
                super::panels::table_head(ui, columns);
            }
            // No site as linhas ficam a 8 uma da outra.
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = if panels { 0.0 } else { ROW_GAP };
                let mut n = 0;
                for item in items {
                    match item {
                        Item::Track(track) => {
                            let index = n;
                            n += 1;
                            let id = Id::new(("linha", si, index));
                            let clicked = if panels {
                                super::panels::track_line(ui, cx, track, index + 1, columns, id)
                            } else {
                                track_row(ui, cx, track, numbered.then_some(index + 1), false, id)
                            };
                            if clicked {
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
    track_row_with(ui, cx, track, number, compact, id, false)
}

/// `track_row`, e com `deletable` a lixeira de apagar o download e a linha
/// que acende aos poucos sob o mouse (a página "Baixadas").
pub(super) fn track_row_with(ui: &mut Ui, cx: &mut Cx, track: &Track, number: Option<usize>, compact: bool, id: Id, deletable: bool) -> bool {
    let width = ui.available_width();
    let (height, pad, radius) = match (compact, deletable) {
        (true, _) => (PICK_ROW, 8.0, 4),
        (false, true) => (ROW, 0.0, 6),
        (false, false) => (ROW, 16.0, 2),
    };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let lit_rect = if deletable { rect.expand2(vec2(ROW_BLEED, 0.0)) } else { rect };
    let lit_row = deletable || theme::panels();
    if !ui.is_rect_visible(lit_rect) {
        return false;
    }
    // A música baixada se arrasta para fora do app pela linha inteira
    // (soltar no Premiere importa, no Explorer copia, num chat envia).
    let download = cx.download(&track.video_id);
    let sense = if matches!(download, Download::Done(_)) { Sense::click_and_drag() } else { Sense::click() };
    let response = ui.interact(lit_rect, id, sense);
    if response.drag_started()
        && let Download::Done(path) = &download
    {
        cx.act(Action::DragOut(path.clone()));
    }
    let menu_open = response.context_menu_opened();
    let current = cx.current.as_deref() == Some(track.video_id.as_str());
    // O site não pinta a linha sob o mouse: escurece a capa e mostra os botões.
    // Nas baixadas a linha acende, e tudo isso entra aos poucos (`glow`).
    let hovered = response.hovered() || menu_open || ui.rect_contains_pointer(lit_rect);
    let glow = if lit_row { ui.ctx().animate_bool_with_time(id.with("luz"), hovered, 0.14) } else { f32::from(u8::from(hovered)) };
    if response.hovered() {
        cx.act(Action::Warm(track.video_id.clone()));
    }
    if compact && hovered && theme::panels()
        && let Some(tint) = cx.covers.color(&track.thumbs)
    {
        super::panels::offer_tint(ui, tint);
    }
    let p = ui.painter().clone();
    if lit_row {
        if current {
            p.rect_filled(lit_rect, CornerRadius::same(theme::RADIUS), theme::accent().gamma_multiply(0.1));
        }
        let fill = if response.is_pointer_button_down_on() { theme::GLASS_ACTIVE } else { theme::GLASS_HOVER };
        p.rect_filled(lit_rect, CornerRadius::same(theme::RADIUS), fill.gamma_multiply(glow));
    }
    let lead = Rect::from_min_size(pos2(rect.left() + pad, rect.center().y - 24.0), vec2(48.0, 48.0));
    let lit = theme::TEXT.gamma_multiply(glow);
    match &number {
        Some(n) => {
            if current {
                equalizer(&p, lead.shrink(14.0), cx.playing);
            } else if hovered {
                icons::paint_filled(&p, lead.shrink(12.0), icons::PLAY, lit);
            } else {
                p.text(lead.center(), Align2::CENTER_CENTER, n.to_string(), theme::regular(16.0), theme::TEXT_SUB);
            }
        }
        None => {
            // A capa cresce um pouco com a luz da linha.
            let art = if deletable { lead.expand(2.0 * glow) } else { lead };
            cx.covers.paint(ui, art, &track.thumbs, radius);
            let shade = if current { 1.0 } else { glow };
            if shade > 0.0 {
                p.rect_filled(art, CornerRadius::same(radius), Color32::from_black_alpha((150.0 * shade) as u8));
                if current {
                    equalizer(&p, lead.shrink(14.0), cx.playing);
                } else {
                    icons::paint_filled(&p, lead.shrink(12.0), icons::PLAY, lit);
                }
            }
        }
    }
    let x = lead.right() + 16.0;
    let right = rect.right() - pad;

    // Os botões do mouse: curtir e não curtir só com conta.
    const BUTTON: f32 = 36.0;
    // O de baixar fica à vista enquanto baixa (e se falhou); baixada, some.
    let downloaded = matches!(download, Download::Done(_));
    let download_shown = matches!(download, Download::Working(_) | Download::Failed(_));
    let buttons = if cx.me.signed_in { 3.0 } else { 1.0 } + if deletable { 1.0 } else { 0.0 } + if downloaded { 0.0 } else { 1.0 };
    let reserve = if hovered {
        buttons * (BUTTON + 4.0)
    } else {
        let duration = if compact { 0.0 } else { 64.0 };
        duration + if download_shown { BUTTON + 4.0 } else { 0.0 }
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
        kit::paint_icon_button(ui, menu_rect, &menu, icons::ELLIPSIS, lit);
        egui::Popup::menu(&menu).show(|ui| track_menu(ui, cx, track));
        if cx.me.signed_in {
            let status = cx.me.like(&track.video_id);
            for (icon, mine, key) in [(icons::THUMBS_DOWN, LikeStatus::Dislike, "nao"), (icons::THUMBS_UP, LikeStatus::Like, "sim")] {
                bx -= BUTTON + 4.0;
                let r = Rect::from_min_size(pos2(bx, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
                let b = ui.interact(r, id.with(key), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                kit::paint_icon_button(ui, r, &b, icon, lit);
                if status == mine {
                    icons::paint_filled(ui.painter(), r.shrink(r.width() * 0.25), icon, lit);
                }
                if b.clicked() {
                    let next = if status == mine { LikeStatus::Indifferent } else { mine };
                    cx.act(Action::Rate(track.video_id.clone(), next));
                }
            }
        }
        if deletable {
            bx -= BUTTON + 4.0;
            let r = Rect::from_min_size(pos2(bx, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
            let b = ui.interact(r, id.with("apagar"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            kit::paint_icon_button(ui, r, &b, icons::TRASH_2, lit);
            if b.on_hover_text(t().delete_download).clicked() {
                cx.act(Action::DeleteDownload(track.video_id.clone()));
            }
        }
        if !downloaded {
            bx -= BUTTON + 4.0;
            let r = Rect::from_min_size(pos2(bx, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
            download_button(ui, cx, r, id.with("baixar"), track, &download);
        }
    } else {
        if !compact && let Some(d) = track.duration_secs {
            // Volta aos poucos quando a luz da linha se apaga.
            let color = theme::TEXT_SUB.gamma_multiply(1.0 - glow);
            p.text(pos2(right, rect.center().y), Align2::RIGHT_CENTER, kit::time(d as f64), theme::regular(16.0), color);
        }
        if download_shown {
            let x = right - if compact { 0.0 } else { 64.0 } - BUTTON;
            let r = Rect::from_min_size(pos2(x, rect.center().y - BUTTON / 2.0), vec2(BUTTON, BUTTON));
            download_button(ui, cx, r, id.with("baixar"), track, &download);
        }
    }
    let clicked = response.clicked();
    response.context_menu(|ui| track_menu(ui, cx, track));
    clicked
}

/// O botão de baixar: a seta, e o anel de progresso enquanto baixa. A
/// música baixada não tem botão: a linha dela é que se arrasta.
pub fn download_button(ui: &mut Ui, cx: &mut Cx, rect: Rect, id: Id, track: &Track, state: &Download) {
    let p = ui.painter().clone();
    match state {
        Download::None | Download::Failed(_) => {
            let response = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            let failed = matches!(state, Download::Failed(_));
            kit::paint_icon_button(ui, rect, &response, icons::DOWNLOAD, if failed { theme::accent() } else { theme::TEXT });
            let tip = match state {
                Download::Failed(error) => t().download_failed_retry(error),
                _ => t().download.to_owned(),
            };
            if response.on_hover_text(tip).clicked() {
                cx.act(Action::Download(track.clone()));
            }
        }
        Download::Working(step) => {
            let response = ui.interact(rect, id, Sense::hover());
            let fraction = match step {
                Step::Downloading(f) => *f,
                Step::Saving => Some(1.0),
                Step::Queued | Step::Resolving => None,
            };
            progress_ring(&p, rect.center(), rect.width() * 0.3, fraction);
            let tip = match step {
                Step::Queued => t().download_queued.to_owned(),
                Step::Resolving => t().download_preparing.to_owned(),
                Step::Downloading(Some(f)) => t().downloading_percent(*f),
                Step::Downloading(None) => t().downloading.to_owned(),
                Step::Saving => t().download_saving.to_owned(),
            };
            response.on_hover_text(tip);
        }
        Download::Done(_) => {}
    }
}

/// O anel do download: a fração baixada, ou um arco que gira enquanto o
/// tamanho não se sabe.
fn progress_ring(p: &egui::Painter, center: egui::Pos2, radius: f32, fraction: Option<f32>) {
    use std::f32::consts::TAU;
    p.circle_stroke(center, radius, egui::Stroke::new(2.5, Color32::from_white_alpha(40)));
    let (start, sweep) = match fraction {
        Some(f) => (-TAU / 4.0, TAU * f.clamp(0.02, 1.0)),
        None => {
            let time = p.ctx().input(|i| i.time) as f32;
            p.ctx().request_repaint_after(std::time::Duration::from_millis(30));
            (time * 5.0, TAU * 0.28)
        }
    };
    let steps = ((sweep / TAU) * 48.0).ceil().max(2.0) as usize;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    p.add(egui::Shape::line(points, egui::Stroke::new(2.5, theme::TEXT)));
}

/// As barrinhas da música que toca (paradas quando pausada).
pub(super) fn equalizer(p: &egui::Painter, rect: Rect, moving: bool) {
    let t = if moving { p.ctx().input(|i| i.time) } else { 0.0 };
    let w = rect.width() / 5.0;
    for (i, phase) in [0.0f64, 1.7, 0.9].iter().enumerate() {
        let level = if moving { 0.35 + 0.65 * ((t * 7.0 + phase).sin() * 0.5 + 0.5) } else { 0.4 } as f32;
        let h = rect.height() * level;
        let x = rect.left() + w * (i as f32 * 2.0);
        p.rect_filled(Rect::from_min_max(pos2(x, rect.bottom() - h), pos2(x + w, rect.bottom())), CornerRadius::same(1), theme::accent());
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
        let label = if liked { t().remove_from_liked } else { t().like };
        if menu_item(ui, icons::THUMBS_UP, label) {
            let status = if liked { LikeStatus::Indifferent } else { LikeStatus::Like };
            cx.act(Action::Rate(track.video_id.clone(), status));
        }
        if menu_item(ui, icons::LIST_PLUS, t().save_to_playlist) {
            cx.act(Action::Dialog(super::Dialog::SaveTo { track: track.clone() }));
        }
        ui.separator();
    }
    if menu_item(ui, icons::LIST_PLUS, t().play_next) {
        cx.act(Action::Cmd(Command::PlayNext(track.clone())));
    }
    if menu_item(ui, icons::LIST_END, t().add_to_queue) {
        cx.act(Action::Cmd(Command::Enqueue(track.clone())));
    }
    if menu_item(ui, icons::RADIO, t().start_radio) {
        cx.act(Action::Cmd(Command::PlayRadio { seed: track.clone() }));
    }
    match cx.download(&track.video_id) {
        Download::None | Download::Failed(_) => {
            if menu_item(ui, icons::DOWNLOAD, t().download) {
                cx.act(Action::Download(track.clone()));
            }
        }
        Download::Working(_) => {}
        Download::Done(path) => {
            if menu_item(ui, icons::FOLDER, t().show_in_folder) {
                cx.act(Action::Reveal(path));
            }
            if menu_item(ui, icons::TRASH_2, t().delete_download) {
                cx.act(Action::DeleteDownload(track.video_id.clone()));
            }
        }
    }
    if let Some(id) = track.album.as_ref().and_then(|a| a.id.as_ref())
        && menu_item(ui, icons::DISC_3, t().go_to_album)
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
    if let Some(id) = track.artists.iter().find_map(|a| a.id.as_ref())
        && menu_item(ui, icons::USER, t().go_to_artist)
    {
        cx.act(Action::Open(View::browse(id, None)));
    }
}

pub(super) fn menu_item(ui: &mut Ui, icon: icons::Icon, text: &str) -> bool {
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

pub(super) fn open_card(cx: &mut Cx, card: &Card) {
    if !card.browse_id.is_empty() {
        cx.act(Action::Open(View::browse(&card.browse_id, card.params.as_deref())));
    } else if let Some(play) = &card.play {
        cx.act(Action::Play(play.clone(), None));
    }
}

/// As capas em grade que quebra linha (a biblioteca).
fn grid(ui: &mut Ui, cx: &mut Cx, si: usize, shelf: &Shelf) {
    ui.horizontal_wrapped(|ui| {
        let gap = if theme::panels() { 0.0 } else { 18.0 };
        ui.spacing_mut().item_spacing = vec2(gap, gap);
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
    let panels = theme::panels();
    let (gap, pitch) = if panels { (0.0, TILE + 2.0 * TILE_PAD) } else { (18.0, TILE + 18.0) };
    let mut body = |ui: &mut Ui| {
        scroller(ui, id, pitch, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (i, item) in shelf.items.iter().enumerate() {
                    tile(ui, cx, item, Id::new(("tile", si, i)));
                }
            });
        });
    };
    if panels {
        // O cartão tem margem por dentro: a fila sai dela para a capa ficar
        // alinhada com o título da prateleira.
        // Num filho à parte: alargar a própria `ui` empurraria tudo que vem
        // depois para a esquerda.
        let wider = ui.available_rect_before_wrap().expand2(vec2(TILE_PAD, 0.0));
        let mut row = ui.new_child(UiBuilder::new().max_rect(wider));
        body(&mut row);
        let height = row.min_rect().height();
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    } else {
        body(ui);
    }
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
    let panels = theme::panels();
    let (pad, text_h) = if panels { (TILE_PAD, 84.0) } else { (0.0, 56.0) };
    let (rect, _) = ui.allocate_exact_size(vec2(TILE + 2.0 * pad, TILE + pad + text_h), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let art = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(TILE, TILE));
    let hit_area = if panels { rect } else { art };
    let response = ui.interact(hit_area, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
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
    let over = ui.rect_contains_pointer(hit_area);
    // Aparecer e sumir em ~120 ms, em vez de piscar de um quadro para o outro.
    let shade = ui.ctx().animate_bool_with_time(id.with("hover"), over, 0.12);
    if panels && shade > 0.0 {
        // O cartão acende mais em cima, e o alto da página pega a cor da capa.
        let top = theme::GLASS_HOVER.gamma_multiply(shade);
        super::panels::rounded_gradient(ui.painter(), rect, 6.0, top, top.gamma_multiply(0.35), vec2(0.0, 1.0));
        if over && let Some(tint) = cx.covers.color(thumbs) {
            super::panels::offer_tint(ui, tint);
        }
    }
    cx.covers.paint(ui, art, thumbs, if round { (TILE / 2.0) as u8 } else { 6 });
    if !panels && shade > 0.0 && !round {
        ui.painter().rect_filled(art, CornerRadius::same(6), Color32::from_black_alpha((90.0 * shade) as u8));
    }
    let mut play_clicked = false;
    if let Some(play) = play {
        let shown = if current { 1.0 } else { shade };
        if shown > 0.0 {
            if panels {
                // O botão verde sobe enquanto aparece.
                let at = button.translate(vec2(0.0, 8.0 * (1.0 - shown)));
                ui.painter().circle_filled(at.center(), if play.hovered() { 22.0 } else { 20.0 }, theme::accent().gamma_multiply(shown));
                icons::paint_filled(ui.painter(), at.shrink(11.0), icons::PLAY, Color32::BLACK.gamma_multiply(shown));
            } else {
                let lit = ui.ctx().animate_bool_with_time(id.with("tocar-luz"), play.hovered(), 0.1);
                let fill = Color32::from_black_alpha(200).lerp_to_gamma(Color32::WHITE, lit).gamma_multiply(shown);
                let fg = Color32::WHITE.lerp_to_gamma(Color32::BLACK, lit).gamma_multiply(shown);
                ui.painter().circle_filled(button.center(), 20.0, fill);
                icons::paint_filled(ui.painter(), button.shrink(11.0), icons::PLAY, fg);
            }
        }
        play_clicked = play.clicked();
    }
    let below = pos2(art.left(), art.bottom() + 8.0);
    let (t, s) = if panels {
        (kit::two_lines(ui, title, theme::regular(15.0), theme::TEXT, TILE), kit::two_lines(ui, &subtitle, theme::regular(13.0), theme::TEXT_DIM, TILE))
    } else {
        (kit::one_line(ui, title, theme::medium(14.0), theme::TEXT, TILE), kit::one_line(ui, &subtitle, theme::regular(13.0), theme::TEXT_DIM, TILE))
    };
    let th = t.size().y;
    ui.painter().galley(below, t, theme::TEXT);
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
