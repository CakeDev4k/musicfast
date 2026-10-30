//! O que o app sabe do catálogo. Pequeno de propósito: a interface só
//! desenha isto, e o cache em disco guarda isto (não o JSON do YouTube, que
//! tem 200–800 kB por página e 95% de rastreamento e menus).

use serde::{Deserialize, Serialize};

/// Uma imagem em vários tamanhos, do menor ao maior.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thumbs(pub Vec<Thumb>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thumb {
    pub url: String,
    pub width: u32,
}

impl Thumbs {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// O endereço para desenhar com `px` pixels de largura.
    ///
    /// As capas do YouTube Music (`*.googleusercontent.com`, `*.ggpht.com`)
    /// aceitam o tamanho no próprio endereço (`=w226-h226-l90-rj`): pedir o
    /// tamanho exato baixa ~8 kB em vez dos ~60 kB do maior da lista e poupa
    /// o redimensionamento. As miniaturas de vídeo (`i.ytimg.com`) não
    /// aceitam; delas vem a menor que cobre o pedido.
    pub fn url_for(&self, px: u32) -> Option<String> {
        let largest = self.0.last()?;
        if let Some(base) = sizable_base(&largest.url) {
            return Some(format!("{base}=w{px}-h{px}-l90-rj"));
        }
        self.0
            .iter()
            .find(|t| t.width >= px)
            .or(self.0.last())
            .map(|t| t.url.clone())
    }
}

fn sizable_base(url: &str) -> Option<&str> {
    let host = url.split("://").nth(1)?.split('/').next()?;
    if !(host.ends_with("googleusercontent.com") || host.ends_with("ggpht.com")) {
        return None;
    }
    // O sufixo de tamanho vem depois do último `=` do caminho.
    let path_start = url.find(host)? + host.len();
    match url[path_start..].rfind('=') {
        Some(i) => Some(&url[..path_start + i]),
        None => Some(url),
    }
}

/// Um nome que pode levar a uma página (artista, álbum).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub artists: Vec<Link>,
    pub album: Option<Link>,
    pub duration_secs: Option<u32>,
    pub thumbs: Thumbs,
    pub explicit: bool,
}

impl Track {
    pub fn artist_names(&self) -> String {
        let names: Vec<&str> = self.artists.iter().map(|a| a.name.as_str()).collect();
        names.join(", ")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Album,
    Playlist,
    Artist,
    Podcast,
    #[default]
    Other,
}

impl Kind {
    pub fn from_page_type(page_type: &str) -> Self {
        match page_type {
            "MUSIC_PAGE_TYPE_ALBUM" | "MUSIC_PAGE_TYPE_AUDIOBOOK" => Self::Album,
            "MUSIC_PAGE_TYPE_PLAYLIST" => Self::Playlist,
            "MUSIC_PAGE_TYPE_ARTIST" | "MUSIC_PAGE_TYPE_USER_CHANNEL" => Self::Artist,
            "MUSIC_PAGE_TYPE_PODCAST_SHOW_DETAIL_PAGE" => Self::Podcast,
            _ => Self::Other,
        }
    }
}

/// O que tocar: uma fila (`playlist_id`) ou uma música dentro dela.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Play {
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
    pub params: Option<String>,
}

/// Um álbum, playlist ou artista: leva a uma página.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub kind: Kind,
    pub browse_id: String,
    pub params: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub thumbs: Thumbs,
    pub play: Option<Play>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    Track(Track),
    Card(Card),
}

/// Para onde um botão leva (um chip de humor, o "Mais" de uma prateleira).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrowseRef {
    pub id: String,
    pub params: Option<String>,
}

/// Os chips do topo do início ("Relax", "Energia", "Treino").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chip {
    pub title: String,
    pub target: BrowseRef,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShelfStyle {
    /// Linhas de música (resultado de busca, faixas do álbum).
    #[default]
    List,
    /// Capas lado a lado (o carrossel do início).
    Carousel,
    /// Capas em grade que quebra linha (a biblioteca, os gêneros).
    Grid,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shelf {
    pub title: String,
    pub style: ShelfStyle,
    pub items: Vec<Item>,
    /// Mais itens desta prateleira (a lista de uma playlist longa).
    pub continuation: Option<String>,
    /// A página inteira desta prateleira (o "Mais" do título).
    pub more: Option<BrowseRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub kind: Kind,
    pub title: String,
    /// "Álbum • 2019", "Playlist • YouTube Music".
    pub subtitle: String,
    /// "12 músicas • 41 minutos".
    pub detail: String,
    pub description: String,
    /// Quem fez (o artista do álbum, o dono da playlist).
    pub authors: Vec<Link>,
    pub thumbs: Thumbs,
    pub play: Option<Play>,
    pub radio: Option<Play>,
    /// O canal do artista e se a conta está inscrita nele.
    pub channel_id: Option<String>,
    pub subscribed: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub header: Option<Header>,
    pub shelves: Vec<Shelf>,
    /// Mais prateleiras (o início rola).
    pub continuation: Option<String>,
    pub chips: Vec<Chip>,
}

impl Page {
    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.shelves.iter().flat_map(|s| &s.items).filter_map(|i| match i {
            Item::Track(t) => Some(t),
            Item::Card(_) => None,
        })
    }
}

/// O que `next` devolve: a fila de reprodução e onde estão a letra e os
/// relacionados daquela música.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    pub tracks: Vec<Track>,
    pub playlist_id: Option<String>,
    pub lyrics_id: Option<String>,
    pub related_id: Option<String>,
    pub continuation: Option<String>,
    /// Só com conta: o que a conta acha da primeira música.
    pub like: Option<LikeStatus>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LikeStatus {
    Like,
    Dislike,
    #[default]
    Indifferent,
}

/// A conta com que o app entrou.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub handle: String,
    pub photo: Thumbs,
}

/// O que a conferência da conta concluiu. `Unknown` é o YouTube ter
/// respondido numa forma que o leitor não reconhece: não é motivo para sair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Session {
    Active(Account),
    /// O YouTube disse com todas as letras que ninguém está logado.
    SignedOut,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Privacy {
    #[default]
    Private,
    Unlisted,
    Public,
}

impl Privacy {
    pub const ALL: [Self; 3] = [Self::Private, Self::Unlisted, Self::Public];

    pub fn label(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Private => t.private,
            Self::Unlisted => t.unlisted,
            Self::Public => t.public,
        }
    }

    pub fn api(self) -> &'static str {
        match self {
            Self::Private => "PRIVATE",
            Self::Unlisted => "UNLISTED",
            Self::Public => "PUBLIC",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lyrics {
    pub text: String,
    pub source: String,
}
