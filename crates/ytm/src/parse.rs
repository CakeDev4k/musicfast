//! Respostas do InnerTube → `Page`, `Watch`, `Lyrics`.
//!
//! Tudo aqui é tolerante: campo que falta vira vazio, renderer desconhecido é
//! ignorado. O teste de cada página real gravada é a prova de que o que
//! importa continua sendo lido.

use serde_json::Value;

use crate::json::{at, find, list, renderer, str_at, text};
use crate::model::*;

/// "3:14" ou "1:02:03" em segundos.
pub fn duration(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return None;
    }
    let mut total = 0u32;
    for (i, p) in parts.iter().enumerate() {
        let max_len = if i == 0 { 3 } else { 2 };
        if p.is_empty() || p.len() > max_len || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        total = total * 60 + p.parse::<u32>().ok()?;
    }
    Some(total)
}

pub fn thumbs(v: &Value) -> Thumbs {
    let mut out: Vec<Thumb> = find(v, "thumbnails", 6)
        .map(list)
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            let url = t.get("url")?.as_str()?;
            let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_owned() };
            let width = t.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
            Some(Thumb { url, width })
        })
        .collect();
    out.sort_by_key(|t| t.width);
    Thumbs(out)
}

struct Browse {
    id: String,
    params: Option<String>,
    kind: Kind,
}

fn browse_of(endpoint: &Value) -> Option<Browse> {
    let b = at(endpoint, "browseEndpoint");
    let id = str_at(b, "browseId")?.to_owned();
    let page_type = str_at(
        b,
        "browseEndpointContextSupportedConfigs.browseEndpointContextMusicConfig.pageType",
    )
    .unwrap_or("");
    let kind = match Kind::from_page_type(page_type) {
        Kind::Other => kind_of_id(&id),
        kind => kind,
    };
    Some(Browse { id, params: str_at(b, "params").map(Into::into), kind })
}

/// O tipo pela forma do id, quando a resposta não diz.
pub fn kind_of_id(id: &str) -> Kind {
    if id.starts_with("MPRE") {
        Kind::Album
    } else if id.starts_with("UC") {
        Kind::Artist
    } else if id.starts_with("VL") || id.starts_with("PL") || id.starts_with("RD") || id.starts_with("OLAK") {
        Kind::Playlist
    } else if id.starts_with("MPSP") {
        Kind::Podcast
    } else {
        Kind::Other
    }
}

fn play_of(endpoint: &Value) -> Option<Play> {
    let w = at(endpoint, "watchEndpoint");
    if w.is_object() {
        return Some(Play {
            video_id: str_at(w, "videoId").map(Into::into),
            playlist_id: str_at(w, "playlistId").map(Into::into),
            params: str_at(w, "params").map(Into::into),
        });
    }
    let p = at(endpoint, "watchPlaylistEndpoint");
    let playlist_id = str_at(p, "playlistId")?;
    Some(Play {
        video_id: None,
        playlist_id: Some(playlist_id.into()),
        params: str_at(p, "params").map(Into::into),
    })
}

fn run_text(r: &Value) -> &str {
    r.get("text").and_then(Value::as_str).unwrap_or("")
}

/// Os pedaços de uma linha "Anitta • Downtown • 3:14".
fn segments(runs: &[Value]) -> Vec<&[Value]> {
    runs.split(|r| run_text(r).trim() == "•").filter(|s| !s.is_empty()).collect()
}

/// O rótulo de tipo que a busca "Tudo" põe na frente da linha.
const LABELS: &[&str] = &[
    "Música", "Song", "Vídeo", "Video", "Episódio", "Episode", "Single", "EP", "Álbum", "Album",
    "Artista", "Artist", "Playlist", "Podcast", "Perfil", "Profile",
];

/// Palavras de contagem ("1,7 bi visualizações", "38 mi ouvintes"): o pedaço
/// não é um nome de artista.
const COUNT_WORDS: &[&str] = &[
    "visualiza", "views", "ouvintes", "listeners", "inscrit", "subscriber", "músicas", "songs",
    "Tocou", "plays", "reproduç", "faixas", "tracks",
];

fn is_joiner(s: &str) -> bool {
    matches!(s.trim(), "" | "," | "&" | "e" | "and" | "x" | "y" | "feat." | "part.")
}

fn split_names(s: &str) -> Vec<Link> {
    let mut names = vec![s.to_owned()];
    for sep in [", ", " & ", " e ", " and "] {
        names = names.iter().flat_map(|n| n.split(sep).map(str::to_owned).collect::<Vec<_>>()).collect();
    }
    names
        .into_iter()
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .map(|name| Link { name, id: None })
        .collect()
}

#[derive(Default)]
struct Byline {
    label: Option<String>,
    artists: Vec<Link>,
    album: Option<Link>,
    duration: Option<u32>,
}

fn byline(runs: &[Value]) -> Byline {
    let mut b = Byline::default();
    let mut plain: Option<String> = None;
    for (i, seg) in segments(runs).into_iter().enumerate() {
        let joined: String = seg.iter().map(run_text).collect();
        let trimmed = joined.trim();
        if i == 0 && LABELS.contains(&trimmed) {
            b.label = Some(trimmed.to_owned());
            continue;
        }
        if let Some(d) = duration(trimmed) {
            b.duration = Some(d);
            continue;
        }
        let mut has_artist = false;
        let mut has_album = false;
        for r in seg {
            if let Some(br) = browse_of(&r["navigationEndpoint"]) {
                match br.kind {
                    Kind::Artist => has_artist = true,
                    Kind::Album => {
                        has_album = true;
                        b.album = Some(Link { name: run_text(r).trim().to_owned(), id: Some(br.id) });
                    }
                    _ => {}
                }
            }
        }
        if has_artist && b.artists.is_empty() {
            b.artists = seg
                .iter()
                .filter(|r| !is_joiner(run_text(r)))
                .map(|r| Link {
                    name: run_text(r).trim().to_owned(),
                    id: browse_of(&r["navigationEndpoint"]).filter(|br| br.kind == Kind::Artist).map(|br| br.id),
                })
                .collect();
        } else if !has_album
            && plain.is_none()
            && !(trimmed.len() == 4 && trimmed.bytes().all(|c| c.is_ascii_digit()))
            && !COUNT_WORDS.iter().any(|w| trimmed.contains(w))
        {
            plain = Some(trimmed.to_owned());
        }
    }
    if b.artists.is_empty() {
        if let Some(p) = plain {
            b.artists = split_names(&p);
        }
    }
    b
}

fn explicit(r: &Value) -> bool {
    list(&r["badges"])
        .iter()
        .chain(list(&r["subtitleBadges"]))
        .any(|badge| str_at(badge, "musicInlineBadgeRenderer.icon.iconType") == Some("MUSIC_EXPLICIT_BADGE"))
}

const OVERLAY_PLAY: &str = "musicItemThumbnailOverlayRenderer.content.musicPlayButtonRenderer.playNavigationEndpoint";

/// Item de lista (busca, faixas, quick picks), com o rótulo de tipo da busca.
fn list_item(r: &Value) -> Option<(Item, Option<String>)> {
    let flex: Vec<&[Value]> = list(&r["flexColumns"])
        .iter()
        .map(|c| list(at(c, "musicResponsiveListItemFlexColumnRenderer.text.runs")))
        .collect();
    let title_runs = flex.first().copied().unwrap_or(&[]);
    let title: String = title_runs.iter().map(run_text).collect();
    if title.is_empty() {
        return None;
    }
    let line = byline(flex.get(1).copied().unwrap_or(&[]));
    let thumbs = thumbs(&r["thumbnail"]);

    if let Some(b) = browse_of(&r["navigationEndpoint"]) {
        let subtitle: Vec<String> = flex[1..]
            .iter()
            .map(|runs| runs.iter().map(run_text).collect::<String>())
            .filter(|s| !s.trim().is_empty())
            .collect();
        let card = Card {
            kind: b.kind,
            browse_id: b.id,
            params: b.params,
            title,
            subtitle: subtitle.join(" • "),
            thumbs,
            play: play_of(at(&r["overlay"], OVERLAY_PLAY)),
        };
        return Some((Item::Card(card), line.label));
    }

    let video_id = str_at(r, "playlistItemData.videoId")
        .or_else(|| title_runs.first().and_then(|t| str_at(t, "navigationEndpoint.watchEndpoint.videoId")))
        .or_else(|| str_at(at(&r["overlay"], OVERLAY_PLAY), "watchEndpoint.videoId"))?;
    let mut album = line.album;
    let mut duration_secs = line.duration;
    // A coluna 3 é o álbum nas playlists e a contagem de plays na busca: só
    // o que tem endereço de álbum ou cara de duração é aproveitado.
    for runs in flex.iter().skip(2) {
        let extra = byline(runs);
        album = album.or(extra.album);
        duration_secs = duration_secs.or(extra.duration);
    }
    if duration_secs.is_none() {
        duration_secs = list(&r["fixedColumns"])
            .iter()
            .find_map(|c| duration(&text(at(c, "musicResponsiveListItemFixedColumnRenderer.text"))));
    }
    let track = Track {
        video_id: video_id.to_owned(),
        title,
        artists: line.artists,
        album,
        duration_secs,
        thumbs,
        explicit: explicit(r),
    };
    Some((Item::Track(track), line.label))
}

/// A capa com título e subtítulo dos carrosséis.
fn two_row(r: &Value) -> Option<Item> {
    let title = text(&r["title"]);
    if title.is_empty() {
        return None;
    }
    let nav = &r["navigationEndpoint"];
    let thumbs = thumbs(&r["thumbnailRenderer"]);
    if let Some(video_id) = str_at(nav, "watchEndpoint.videoId") {
        let line = byline(list(at(r, "subtitle.runs")));
        return Some(Item::Track(Track {
            video_id: video_id.to_owned(),
            title,
            artists: line.artists,
            album: line.album,
            duration_secs: line.duration,
            thumbs,
            explicit: explicit(r),
        }));
    }
    let b = browse_of(nav)?;
    Some(Item::Card(Card {
        kind: b.kind,
        browse_id: b.id,
        params: b.params,
        title,
        subtitle: text(&r["subtitle"]),
        thumbs,
        play: play_of(at(&r["thumbnailOverlay"], OVERLAY_PLAY)),
    }))
}

fn panel_video(r: &Value) -> Option<Track> {
    let video_id = str_at(r, "videoId")?;
    let mut line = byline(list(at(r, "longBylineText.runs")));
    if line.artists.is_empty() {
        line.artists = split_names(&text(&r["shortBylineText"]));
    }
    Some(Track {
        video_id: video_id.to_owned(),
        title: text(&r["title"]),
        artists: line.artists,
        album: line.album,
        duration_secs: duration(&text(&r["lengthText"])).or(line.duration),
        thumbs: thumbs(&r["thumbnail"]),
        explicit: explicit(r),
    })
}

/// Os botões de "Momentos e gêneros": só texto, levam a uma página.
fn navigation_button(r: &Value) -> Option<Item> {
    let b = browse_of(&r["clickCommand"])?;
    Some(Item::Card(Card {
        kind: Kind::Other,
        browse_id: b.id,
        params: b.params,
        title: text(&r["buttonText"]),
        subtitle: String::new(),
        thumbs: Thumbs::default(),
        play: None,
    }))
}

fn item(v: &Value) -> Option<(Item, Option<String>)> {
    let (name, r) = renderer(v)?;
    match name {
        "musicResponsiveListItemRenderer" => list_item(r),
        "musicNavigationButtonRenderer" => navigation_button(r).map(|i| (i, None)),
        "musicTwoRowItemRenderer" => two_row(r).map(|i| (i, None)),
        "playlistPanelVideoRenderer" => panel_video(r).map(|t| (Item::Track(t), None)),
        "playlistPanelVideoWrapperRenderer" => item(at(r, "primaryRenderer")),
        _ => None,
    }
}

fn items(contents: &[Value]) -> Vec<Item> {
    contents.iter().filter_map(item).map(|(i, _)| i).collect()
}

/// O token de "mais itens" no fim de uma lista (`continuationItemRenderer`)
/// ou no campo antigo `continuations`.
fn browse_ref(endpoint: &Value) -> Option<BrowseRef> {
    browse_of(endpoint).map(|b| BrowseRef { id: b.id, params: b.params })
}

fn grid_continuation(grid: &Value) -> Option<String> {
    list(&grid["items"])
        .last()
        .and_then(|last| str_at(last, "continuationItemRenderer.continuationEndpoint.continuationCommand.token"))
        .or_else(|| str_at(grid, "continuations.0.nextContinuationData.continuation"))
        .map(Into::into)
}

fn item_continuation(shelf: &Value) -> Option<String> {
    list(&shelf["contents"])
        .last()
        .and_then(|last| str_at(last, "continuationItemRenderer.continuationEndpoint.continuationCommand.token"))
        .or_else(|| str_at(shelf, "continuations.0.nextContinuationData.continuation"))
        .map(Into::into)
}

/// A busca "Tudo" chega como uma lista plana de itens com rótulo. Agrupar
/// por tipo é o que o site faz na tela.
#[derive(Default)]
struct Groups {
    order: Vec<&'static str>,
    shelves: std::collections::HashMap<&'static str, Vec<Item>>,
}

impl Groups {
    fn push(&mut self, item: Item, label: Option<&str>) {
        let title = match (&item, label) {
            (Item::Track(_), Some("Vídeo" | "Video")) => "Vídeos",
            (Item::Track(_), Some("Episódio" | "Episode")) => "Episódios",
            (Item::Track(_), _) => "Músicas",
            (Item::Card(c), _) => match c.kind {
                Kind::Artist => "Artistas",
                Kind::Album => "Álbuns",
                Kind::Playlist => "Playlists",
                Kind::Podcast => "Podcasts",
                Kind::Other => "Outros",
            },
        };
        if !self.order.contains(&title) {
            self.order.push(title);
        }
        self.shelves.entry(title).or_default().push(item);
    }

    fn flush(&mut self, page: &mut Page) {
        for title in self.order.drain(..) {
            let items = self.shelves.remove(title).unwrap_or_default();
            let style = if items.iter().all(|i| matches!(i, Item::Card(_))) {
                ShelfStyle::Carousel
            } else {
                ShelfStyle::List
            };
            page.shelves.push(Shelf { title: title.to_owned(), style, items, ..Shelf::default() });
        }
    }
}

fn header(name: &str, h: &Value) -> Option<Header> {
    match name {
        "musicResponsiveHeaderRenderer" | "musicDetailHeaderRenderer" => {
            let strapline = list(at(h, "straplineTextOne.runs"));
            let mut authors: Vec<Link> = strapline
                .iter()
                .filter(|r| !is_joiner(run_text(r)))
                .map(|r| Link {
                    name: run_text(r).trim().to_owned(),
                    id: browse_of(&r["navigationEndpoint"]).map(|b| b.id),
                })
                .collect();
            if authors.is_empty() {
                authors = byline(list(at(h, "subtitle.runs"))).artists;
            }
            let play = list(&h["buttons"])
                .iter()
                .find_map(|b| play_of(at(b, "musicPlayButtonRenderer.playNavigationEndpoint")));
            let description = text(at(h, "description.musicDescriptionShelfRenderer.description"));
            Some(Header {
                kind: Kind::Other,
                title: text(&h["title"]),
                subtitle: text(&h["subtitle"]),
                detail: text(&h["secondSubtitle"]),
                description: if description.is_empty() { text(&h["description"]) } else { description },
                authors,
                thumbs: thumbs(&h["thumbnail"]),
                play,
                radio: None,
                channel_id: None,
                subscribed: None,
            })
        }
        "musicImmersiveHeaderRenderer" | "musicVisualHeaderRenderer" => Some(Header {
            kind: Kind::Artist,
            title: text(&h["title"]),
            subtitle: text(at(h, "subscriptionButton.subscribeButtonRenderer.subscriberCountText")),
            detail: text(&h["monthlyListenerCount"]),
            description: text(&h["description"]),
            authors: vec![],
            thumbs: thumbs(if h["thumbnail"].is_object() { &h["thumbnail"] } else { &h["foregroundThumbnail"] }),
            play: play_of(at(h, "playButton.buttonRenderer.navigationEndpoint")),
            radio: play_of(at(h, "startRadioButton.buttonRenderer.navigationEndpoint")),
            channel_id: str_at(h, "subscriptionButton.subscribeButtonRenderer.channelId").map(Into::into),
            subscribed: at(h, "subscriptionButton.subscribeButtonRenderer.subscribed").as_bool(),
        }),
        "musicEditablePlaylistDetailHeaderRenderer" => {
            let (inner, r) = renderer(&h["header"])?;
            header(inner, r)
        }
        _ => None,
    }
}

fn section(name: &str, r: &Value, page: &mut Page, groups: &mut Groups) {
    match name {
        "musicShelfRenderer" | "musicPlaylistShelfRenderer" => page.shelves.push(Shelf {
            title: text(&r["title"]),
            style: ShelfStyle::List,
            items: items(list(&r["contents"])),
            continuation: item_continuation(r),
            more: browse_ref(at(r, "bottomEndpoint")).or_else(|| browse_ref(at(r, "title.runs.0.navigationEndpoint"))),
        }),
        "musicCarouselShelfRenderer" => {
            let items = items(list(&r["contents"]));
            // "Escolhas rápidas" vem em linhas de música, não em capas.
            let style = if list(&r["contents"]).iter().any(|c| c.get("musicResponsiveListItemRenderer").is_some()) {
                ShelfStyle::List
            } else {
                ShelfStyle::Carousel
            };
            let head = at(r, "header.musicCarouselShelfBasicHeaderRenderer");
            page.shelves.push(Shelf {
                title: text(&head["title"]),
                style,
                items,
                continuation: None,
                more: browse_ref(at(head, "moreContentButton.buttonRenderer.navigationEndpoint"))
                    .or_else(|| browse_ref(at(head, "title.runs.0.navigationEndpoint"))),
            });
        }
        "musicCardShelfRenderer" => {
            // O melhor resultado da busca: o próprio cartão e as músicas dele.
            let mut top = Vec::new();
            let title_run = at(r, "title.runs.0");
            let nav = &title_run["navigationEndpoint"];
            if let Some(b) = browse_of(nav) {
                top.push(Item::Card(Card {
                    kind: b.kind,
                    browse_id: b.id,
                    params: b.params,
                    title: text(&r["title"]),
                    subtitle: text(&r["subtitle"]),
                    thumbs: thumbs(&r["thumbnail"]),
                    play: list(&r["buttons"])
                        .iter()
                        .find_map(|b| play_of(at(b, "buttonRenderer.command"))),
                }));
            } else if let Some(video_id) = str_at(nav, "watchEndpoint.videoId") {
                let line = byline(list(at(r, "subtitle.runs")));
                top.push(Item::Track(Track {
                    video_id: video_id.to_owned(),
                    title: text(&r["title"]),
                    artists: line.artists,
                    album: line.album,
                    duration_secs: line.duration,
                    thumbs: thumbs(&r["thumbnail"]),
                    explicit: explicit(r),
                }));
            }
            // As músicas embaixo do melhor resultado não repetem o artista
            // dele ("Música • 2:57"): vêm do próprio cartão.
            let owner: Vec<Link> = match top.first() {
                Some(Item::Card(c)) if c.kind == Kind::Artist => {
                    vec![Link { name: c.title.clone(), id: Some(c.browse_id.clone()) }]
                }
                Some(Item::Card(_)) => byline(list(at(r, "subtitle.runs"))).artists,
                _ => Vec::new(),
            };
            let mut rest = items(list(&r["contents"]));
            for item in &mut rest {
                if let Item::Track(t) = item
                    && t.artists.is_empty()
                {
                    t.artists = owner.clone();
                }
            }
            top.extend(rest);
            let title = text(at(r, "header.musicCardShelfHeaderBasicRenderer.title"));
            page.shelves.push(Shelf {
                title: if title.is_empty() { "Melhor resultado".into() } else { title },
                style: ShelfStyle::List,
                items: top,
                ..Shelf::default()
            });
        }
        "itemSectionRenderer" => {
            for c in list(&r["contents"]) {
                // A biblioteca embrulha a grade e as listas numa seção.
                if let Some((inner, ir)) = renderer(c)
                    && matches!(inner, "gridRenderer" | "musicShelfRenderer" | "musicPlaylistShelfRenderer")
                {
                    section(inner, ir, page, groups);
                } else if let Some((item, label)) = item(c) {
                    groups.push(item, label.as_deref());
                }
            }
        }
        "gridRenderer" => page.shelves.push(Shelf {
            title: text(at(r, "header.gridHeaderRenderer.title")),
            style: ShelfStyle::Grid,
            items: items(list(&r["items"])),
            continuation: grid_continuation(r),
            more: None,
        }),
        "musicResponsiveHeaderRenderer" | "musicImmersiveHeaderRenderer" | "musicVisualHeaderRenderer"
        | "musicDetailHeaderRenderer" | "musicEditablePlaylistDetailHeaderRenderer" => {
            if page.header.is_none() {
                page.header = header(name, r);
            }
        }
        "musicDescriptionShelfRenderer" => {
            if let Some(h) = page.header.as_mut().filter(|h| h.description.is_empty()) {
                h.description = text(&r["description"]);
            }
        }
        _ => {}
    }
}

const SECTION_LISTS: &[&str] = &[
    "singleColumnBrowseResultsRenderer.tabs.0.tabRenderer.content.sectionListRenderer",
    "twoColumnBrowseResultsRenderer.tabs.0.tabRenderer.content.sectionListRenderer",
    "twoColumnBrowseResultsRenderer.secondaryContents.sectionListRenderer",
    "tabbedSearchResultsRenderer.tabs.0.tabRenderer.content.sectionListRenderer",
    "sectionListRenderer",
];

pub fn page(v: &Value) -> Page {
    let mut page = Page::default();
    if let Some((name, h)) = renderer(&v["header"]) {
        page.header = header(name, h);
    }
    let mut groups = Groups::default();
    for path in SECTION_LISTS {
        let sl = at(&v["contents"], path);
        sections_into(sl, &mut page, &mut groups);
    }
    groups.flush(&mut page);
    fill_from_header(&mut page);
    page
}

fn sections_into(sl: &Value, page: &mut Page, groups: &mut Groups) {
    if page.chips.is_empty() {
        page.chips = list(at(sl, "header.chipCloudRenderer.chips"))
            .iter()
            .filter_map(|c| {
                let c = &c["chipCloudChipRenderer"];
                let b = browse_of(&c["navigationEndpoint"])?;
                Some(Chip {
                    title: text(&c["text"]),
                    target: BrowseRef { id: b.id, params: b.params },
                    selected: c["isSelected"].as_bool().unwrap_or(false),
                })
            })
            .collect();
    }
    for s in list(&sl["contents"]) {
        if let Some((name, r)) = renderer(s) {
            section(name, r, page, groups);
        }
    }
    if page.continuation.is_none() {
        page.continuation = str_at(sl, "continuations.0.nextContinuationData.continuation").map(Into::into);
    }
}

/// As faixas de um álbum não repetem artista nem capa: vêm do cabeçalho.
fn fill_from_header(page: &mut Page) {
    let Some(header) = &page.header else { return };
    for shelf in &mut page.shelves {
        if shelf.style != ShelfStyle::List {
            continue;
        }
        for item in &mut shelf.items {
            if let Item::Track(t) = item {
                if t.artists.is_empty() {
                    t.artists = header.authors.clone();
                }
                if t.thumbs.is_empty() {
                    t.thumbs = header.thumbs.clone();
                }
            }
        }
    }
}

/// A resposta de uma continuação: prateleiras novas (o início que rola) ou
/// itens novos de uma prateleira (uma playlist longa), que voltam numa
/// prateleira só, com o próximo token.
pub fn continuation(v: &Value) -> Page {
    let mut page = Page::default();
    let mut groups = Groups::default();
    let sl = at(v, "continuationContents.sectionListContinuation");
    if sl.is_object() {
        sections_into(sl, &mut page, &mut groups);
        return page;
    }
    let grid = at(v, "continuationContents.gridContinuation");
    if grid.is_object() {
        page.shelves.push(Shelf {
            items: items(list(&grid["items"])),
            continuation: grid_continuation(grid),
            style: ShelfStyle::Grid,
            ..Shelf::default()
        });
        return page;
    }
    for key in ["musicPlaylistShelfContinuation", "musicShelfContinuation"] {
        let shelf = at(v, &format!("continuationContents.{key}"));
        if shelf.is_object() {
            page.shelves.push(Shelf {
                items: items(list(&shelf["contents"])),
                continuation: item_continuation(shelf),
                ..Shelf::default()
            });
            return page;
        }
    }
    for action in list(&v["onResponseReceivedActions"]) {
        let appended = at(action, "appendContinuationItemsAction.continuationItems");
        if appended.is_array() {
            let wrapper = serde_json::json!({ "contents": appended });
            page.shelves.push(Shelf {
                items: items(list(appended)),
                continuation: item_continuation(&wrapper),
                ..Shelf::default()
            });
        }
    }
    page
}

pub fn watch(v: &Value) -> Watch {
    let tabs = list(at(
        v,
        "contents.singleColumnMusicWatchNextResultsRenderer.tabbedRenderer.watchNextTabbedResultsRenderer.tabs",
    ));
    let panel = tabs
        .first()
        .map(|t| at(t, "tabRenderer.content.musicQueueRenderer.content.playlistPanelRenderer"))
        .unwrap_or(&Value::Null);
    let mut watch = panel_watch(panel);
    watch.like = find(&v["playerOverlays"], "likeButtonRenderer", 8)
        .and_then(|b| b["likeStatus"].as_str())
        .map(|s| match s {
            "LIKE" => LikeStatus::Like,
            "DISLIKE" => LikeStatus::Dislike,
            _ => LikeStatus::Indifferent,
        });
    for tab in tabs.iter().skip(1) {
        if let Some(id) = str_at(tab, "tabRenderer.endpoint.browseEndpoint.browseId") {
            if id.starts_with("MPLY") {
                watch.lyrics_id = Some(id.into());
            } else if id.starts_with("MPTR") {
                watch.related_id = Some(id.into());
            }
        }
    }
    watch
}

fn panel_watch(panel: &Value) -> Watch {
    Watch {
        tracks: list(&panel["contents"])
            .iter()
            .filter_map(item)
            .filter_map(|(i, _)| match i {
                Item::Track(t) => Some(t),
                Item::Card(_) => None,
            })
            .collect(),
        playlist_id: str_at(panel, "playlistId").map(Into::into),
        continuation: str_at(panel, "continuations.0.nextRadioContinuationData.continuation")
            .or_else(|| str_at(panel, "continuations.0.nextContinuationData.continuation"))
            .map(Into::into),
        ..Watch::default()
    }
}

pub fn watch_continuation(v: &Value) -> Watch {
    panel_watch(at(v, "continuationContents.playlistPanelContinuation"))
}

pub fn lyrics(v: &Value) -> Option<Lyrics> {
    let shelf = find(&v["contents"], "musicDescriptionShelfRenderer", 8)?;
    let text = text(&shelf["description"]);
    if text.trim().is_empty() {
        return None;
    }
    Some(Lyrics { text, source: crate::json::text(&shelf["footer"]) })
}

pub fn suggestions(v: &Value) -> Vec<String> {
    list(&v["contents"])
        .iter()
        .flat_map(|section| list(at(section, "searchSuggestionsSectionRenderer.contents")))
        .filter_map(|c| {
            let s = text(at(c, "searchSuggestionRenderer.suggestion"));
            (!s.is_empty()).then_some(s)
        })
        .collect()
}

/// O nome e a foto da conta (`account/account_menu`); `None` sem conta.
/// O `logged_in` que toda resposta traz nos `serviceTrackingParams`: o sinal
/// que o próprio site usa, e que não depende do desenho do menu da conta.
pub fn logged_in(v: &Value) -> Option<bool> {
    list(at(v, "responseContext.serviceTrackingParams"))
        .iter()
        .flat_map(|service| list(&service["params"]))
        .find(|p| p["key"].as_str() == Some("logged_in"))
        .and_then(|p| match p["value"].as_str()? {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        })
}

pub fn session(v: &Value) -> Session {
    match (account(v), logged_in(v)) {
        (_, Some(false)) => Session::SignedOut,
        (Some(account), _) => Session::Active(account),
        (None, _) => Session::Unknown,
    }
}

pub fn account(v: &Value) -> Option<Account> {
    let h = find(&v["actions"], "activeAccountHeaderRenderer", 12)?;
    let name = text(&h["accountName"]);
    if name.is_empty() {
        return None;
    }
    Some(Account { name, handle: text(&h["channelHandle"]), photo: thumbs(&h["accountPhoto"]) })
}
