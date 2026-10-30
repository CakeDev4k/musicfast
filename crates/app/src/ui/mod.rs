//! A interface. Cada parte desenha com o que recebe e devolve o que a pessoa
//! pediu como `Action`; o `app` aplica tudo depois do quadro, então nenhuma
//! tela segura o estado enquanto outra o muda.

pub mod bar;
pub mod dialog;
mod icon_data;
pub mod icons;
pub mod kit;
pub mod lyrics_view;
pub mod mini;
pub mod nav;
pub mod page;
pub mod player;
pub mod side;
pub mod theme;

use std::collections::HashMap;
use std::sync::Arc;

use musicfast_i18n::{Lang, t};
use musicfast_player::Command;
use musicfast_ytm::{Account, Card, LikeStatus, Page, Play, SearchFilter, Track};

use crate::covers::Covers;
pub use dialog::Dialog;

/// As abas da biblioteca, cada uma uma página da conta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LibraryTab {
    Playlists,
    Songs,
    Albums,
    Artists,
    Subscriptions,
    History,
}

impl LibraryTab {
    pub const ALL: [Self; 6] = [Self::Playlists, Self::Songs, Self::Albums, Self::Artists, Self::Subscriptions, Self::History];

    pub fn label(self) -> &'static str {
        match self {
            Self::Playlists => t().playlists,
            Self::Songs => t().songs,
            Self::Albums => t().albums,
            Self::Artists => t().artists,
            Self::Subscriptions => t().subscriptions,
            Self::History => t().history,
        }
    }

    pub fn browse_id(self) -> &'static str {
        match self {
            Self::Playlists => "FEmusic_liked_playlists",
            Self::Songs => "FEmusic_liked_videos",
            Self::Albums => "FEmusic_liked_albums",
            Self::Artists => "FEmusic_library_corpus_track_artists",
            Self::Subscriptions => "FEmusic_library_corpus_artists",
            Self::History => "FEmusic_history",
        }
    }
}

/// Uma tela, e a chave da página carregada para ela.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Home,
    Explore,
    Library(LibraryTab),
    Search { query: String, filter: SearchFilter },
    Browse { id: String, params: Option<String> },
}

impl View {
    pub fn browse(id: &str, params: Option<&str>) -> Self {
        Self::Browse { id: id.to_owned(), params: params.map(str::to_owned) }
    }

    /// As telas que só existem com uma conta.
    pub fn needs_account(&self) -> bool {
        match self {
            Self::Library(_) => true,
            Self::Browse { id, .. } => id == "VLLM" || id.starts_with("FEmusic_liked") || id.starts_with("FEmusic_library") || id == "FEmusic_history",
            _ => false,
        }
    }
}

#[derive(Default)]
pub struct PageState {
    pub page: Option<Arc<Page>>,
    pub error: Option<String>,
    /// Mais prateleiras, ou mais itens da prateleira `n`, a caminho.
    pub loading_more: Option<Option<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Queue,
    Lyrics,
}

impl Side {
    pub fn key(self) -> &'static str {
        match self {
            Self::Queue => "fila",
            Self::Lyrics => "letra",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "fila" => Some(Self::Queue),
            "letra" => Some(Self::Lyrics),
            _ => None,
        }
    }
}

/// O que a interface sabe da conta. As escolhas feitas aqui (curtir,
/// inscrever-se, salvar) valem na tela na hora, antes de o YouTube responder.
#[derive(Default)]
pub struct Me {
    pub signed_in: bool,
    /// A janela de login está aberta.
    pub signing_in: bool,
    pub account: Option<Account>,
    pub playlists: Vec<Card>,
    pub likes: HashMap<String, LikeStatus>,
    pub subscribed: HashMap<String, bool>,
    pub saved: HashMap<String, bool>,
}

impl Me {
    pub fn like(&self, video_id: &str) -> LikeStatus {
        self.likes.get(video_id).copied().unwrap_or_default()
    }

    /// As playlists em que dá para pôr música (sem as automáticas).
    pub fn editable_playlists(&self) -> impl Iterator<Item = &Card> {
        self.playlists.iter().filter(|c| !matches!(c.browse_id.as_str(), "VLLM" | "VLSE" | "VLRDPN"))
    }
}

pub enum Action {
    Open(View),
    Back,
    Forward,
    Reload,
    Cmd(Command),
    /// Tocar o que o YouTube chama de "play" (álbum, playlist, rádio).
    Play(Play, Option<Track>),
    Warm(String),
    MoreSections,
    MoreItems(usize),
    ToggleSide(Side),
    /// Recolher ou abrir a coluna da esquerda.
    ToggleSidebar,
    /// A tela "Tocando agora".
    Expand(bool),
    /// A janela vira o mini player (ou volta a ser inteira).
    Mini(bool),
    SignIn,
    SignOut,
    Dialog(Dialog),
    /// Abrir "Armazenamento e cache" (e medir o cache).
    Storage,
    /// Apagar o cache do disco.
    ClearCache,
    Rate(String, LikeStatus),
    /// Salvar (ou tirar) um álbum ou playlist da biblioteca.
    SaveCollection(String, bool),
    Subscribe(String, bool),
    AddToPlaylist { playlist_id: String, title: String, video_id: String },
    CreatePlaylist { title: String, privacy: musicfast_ytm::Privacy, video_id: Option<String> },
    SubmitCookies(String),
    /// Fechar e deixar o ajudante instalar a versão nova já baixada.
    RestartToUpdate,
    /// Trocar o idioma (`None`: o do sistema).
    SetLanguage(Option<Lang>),
    /// Ligar ou desligar a tradução da letra.
    ToggleTranslation,
    /// Abrir "Personalizar o player" na aba da letra.
    PersonalizeLyrics,
    /// Trocar a letra da música (`video_id`) pela próxima candidata.
    OtherLyrics(String),
    /// Abrir "Personalizar o player" (com o player aberto atrás).
    Personalize,
    /// Escolher no computador a imagem de fundo de uma superfície.
    PickBackdropFile(crate::look::Surface),
    /// O fundo do player escolhido.
    SetLook(crate::look::Look),
}

/// O que as telas recebem para desenhar.
pub struct Cx<'a> {
    pub covers: &'a mut Covers,
    pub actions: &'a mut Vec<Action>,
    pub me: &'a Me,
    /// O idioma escolhido; `None` segue o do sistema.
    pub language: Option<Lang>,
    /// Quanto o som entregue demora a sair no fone (s): a letra segue o que
    /// se ouve, não o que foi entregue.
    pub latency: f64,
    /// Mostrar a tradução da letra.
    pub translate_lyrics: bool,
    /// O estilo da letra (fonte, cor, desfoque, tradução).
    pub lyrics_style: &'a crate::look::LyricsStyle,
    /// O `video_id` da música atual, e se está tocando.
    pub current: Option<String>,
    pub playing: bool,
}

impl Cx<'_> {
    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }
}

/// O comando de tocar para um "play" do YouTube.
pub fn play_command(play: &Play, seed: Option<Track>) -> Option<Command> {
    let seed = seed.or_else(|| {
        play.video_id.as_ref().map(|v| Track { video_id: v.clone(), ..Track::default() })
    });
    match (&play.playlist_id, seed) {
        (Some(playlist_id), seed) => Some(Command::PlayList {
            seed,
            playlist_id: playlist_id.clone(),
            params: play.params.clone(),
        }),
        (None, Some(seed)) => Some(Command::PlayRadio { seed }),
        (None, None) => None,
    }
}
