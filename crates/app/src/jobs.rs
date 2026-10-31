//! O trabalho de rede fora da thread da interface: cada pedido roda numa
//! thread curta (o `Client` é bloqueante e reaproveita a conexão HTTP/2), e a
//! resposta volta por um canal lido no começo do quadro.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use musicfast_ytm::{Card, Client, Item, Kind, Page, Track, Watch};

use crate::lyrics::Song;

use crate::ui::View;

/// O que refazer depois de uma ação da conta dar certo.
#[derive(Clone, Debug, PartialEq)]
pub enum Refresh {
    Nothing,
    Playlists,
    /// A playlist ou página mudou (uma música entrou nela).
    Page(View),
}

pub enum Reply {
    Page { view: View, result: Result<Page, String> },
    /// Mais prateleiras da página (`shelf: None`) ou mais itens de uma.
    More { view: View, shelf: Option<usize>, result: Result<Page, String> },
    Suggestions { input: String, result: Vec<String> },
    /// A fila do YouTube para a música atual: curtida, letra, relacionados.
    TrackInfo { video_id: String, result: Result<Watch, String> },
    Lyrics { video_id: String, result: Result<Option<Song>, String> },
    /// A letra com a tradução (`lyrics::translate`).
    Translated { video_id: String, result: Result<Song, String> },
    /// As letras que dá para escolher em "Procurar outra letra".
    LyricAlternatives { video_id: String, result: Result<Vec<Song>, String> },
    Related { video_id: String, result: Result<Page, String> },
    /// A capa da música atual chegou ao disco.
    Art { video_id: String, path: PathBuf },
    Version(String),
    /// A janela de login fechou: os cookies, ou `None` se foi cancelada.
    /// `silent`: foi a renovação sem janela. `slot`: a conta (`Profile::id`).
    Login { silent: bool, slot: String, result: Result<Option<String>, String> },
    Account(Result<musicfast_ytm::Session, String>),
    Playlists(Result<Vec<Card>, String>),
    /// Um trabalho que não tem o que dizer à tela (a busca adiantada que
    /// não achou nada sincronizado).
    Quiet,
    /// O tamanho do cache, para "Armazenamento e cache".
    Storage(crate::storage::Usage),
    /// A limpeza do cache terminou: quanto liberou e o que sobrou.
    CacheCleared { freed: u64, usage: crate::storage::Usage },
    /// Uma ação da conta terminou (curtir, salvar, inscrever-se).
    Done { message: String, result: Result<(), String>, refresh: Refresh },
}

impl Reply {
    /// A resposta só vale para a conta que pediu: página personalizada,
    /// playlists, curtidas. Entregue depois da troca, mostraria a conta
    /// anterior.
    fn belongs_to_account(&self) -> bool {
        matches!(self, Self::Page { .. } | Self::More { .. } | Self::TrackInfo { .. } | Self::Related { .. } | Self::Account(_) | Self::Playlists(_))
    }
}

pub struct Jobs {
    client: Arc<Client>,
    http: Arc<reqwest::blocking::Client>,
    tx: Sender<Reply>,
    ctx: egui::Context,
    /// Muda a cada troca de conta (`new_account`).
    account_epoch: Arc<AtomicU64>,
}

impl Jobs {
    pub fn new(client: Arc<Client>, http: Arc<reqwest::blocking::Client>, ctx: egui::Context) -> (Self, Receiver<Reply>) {
        let (tx, rx) = channel();
        (Self { client, http, tx, ctx, account_epoch: Arc::new(AtomicU64::new(0)) }, rx)
    }

    /// A partir daqui, o que estava a caminho para a conta anterior é jogado
    /// fora. Vem antes de trocar a sessão do `Client`.
    pub fn new_account(&self) {
        self.account_epoch.fetch_add(1, Ordering::Relaxed);
    }

    fn spawn(&self, name: &str, work: impl FnOnce(&Client, &reqwest::blocking::Client) -> Reply + Send + 'static) {
        self.spawn_or(name, None, work);
    }

    /// Como `spawn`, com a resposta para o caso de a thread entrar em
    /// pânico (o JSON do YouTube numa forma que o leitor não esperava): sem
    /// ela, a tela que esperava ficaria carregando para sempre.
    fn spawn_or(
        &self,
        name: &str,
        on_panic: Option<Reply>,
        work: impl FnOnce(&Client, &reqwest::blocking::Client) -> Reply + Send + 'static,
    ) {
        let (client, http, tx, ctx) = (self.client.clone(), self.http.clone(), self.tx.clone(), self.ctx.clone());
        let thread = name.to_owned();
        let epoch = self.account_epoch.clone();
        let asked_in = epoch.load(Ordering::Relaxed);
        let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
            let reply = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&client, &http))) {
                Ok(reply) => Some(reply),
                Err(_) => {
                    // O endereço do pânico já foi para o panicos.log.
                    log::error!("thread {thread} entrou em pânico");
                    on_panic
                }
            };
            let stale = epoch.load(Ordering::Relaxed) != asked_in;
            if let Some(reply) = reply.filter(|r| !(stale && r.belongs_to_account()))
                && tx.send(reply).is_ok()
            {
                ctx.request_repaint();
            }
        });
        if let Err(error) = spawned {
            log::warn!("thread {name}: {error}");
        }
    }

    pub fn load(&self, view: View) {
        let failed = Reply::Page { view: view.clone(), result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        self.spawn_or("pagina", Some(failed), move |client, _| {
            let result = match &view {
                View::Home => client.home(),
                View::Explore => client.browse("FEmusic_explore", None),
                View::Library(tab) => client.browse(tab.browse_id(), None),
                View::Search { query, filter } => client.search(query, *filter),
                View::Browse { id, params } => client.browse(id, params.as_deref()),
                // Lida do disco (`Offline`), nunca pedida ao YouTube.
                View::Downloads => return Reply::Page { view, result: Err(String::new()) },
            };
            Reply::Page { view, result: result.map_err(|e| e.to_string()) }
        });
    }

    pub fn more_sections(&self, view: View, token: String) {
        let failed = Reply::More { view: view.clone(), shelf: None, result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        self.spawn_or("mais", Some(failed), move |client, _| Reply::More {
            view,
            shelf: None,
            result: client.more_sections(&token).map_err(|e| e.to_string()),
        });
    }

    pub fn more_items(&self, view: View, shelf: usize, token: String) {
        let failed = Reply::More { view: view.clone(), shelf: Some(shelf), result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        self.spawn_or("mais", Some(failed), move |client, _| Reply::More {
            view,
            shelf: Some(shelf),
            result: client.more_items(&token).map_err(|e| e.to_string()),
        });
    }

    pub fn suggestions(&self, input: String) {
        self.spawn("sugestoes", move |client, _| {
            let result = client.suggestions(&input).unwrap_or_default();
            Reply::Suggestions { input, result }
        });
    }

    pub fn track_info(&self, video_id: String) {
        let failed = Reply::TrackInfo { video_id: video_id.clone(), result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        self.spawn_or("musica", Some(failed), move |client, _| {
            let result = client.watch(Some(&video_id), None, None).map_err(|e| e.to_string());
            Reply::TrackInfo { video_id, result }
        });
    }

    /// A letra da música: a guardada em `cache`, senão a sincronizada das
    /// fontes, ou a do YouTube Music de reserva (e guarda o que achou).
    pub fn lyrics(&self, video_id: String, track: Track, lyrics_id: Option<String>, cache: PathBuf) {
        let failed = Reply::Lyrics { video_id: video_id.clone(), result: Err(musicfast_i18n::t().lyrics_internal_error.to_owned()) };
        self.spawn_or("letra", Some(failed), move |client, http| {
            if let Some(hit) = crate::lyrics::cached(&cache, &video_id) {
                return Reply::Lyrics { result: Ok(hit), video_id };
            }
            let result = crate::lyrics::fetch(client, http, &track, lyrics_id.as_deref());
            match &result {
                Ok(Some(song)) => crate::lyrics::store(&cache, &video_id, Some(song)),
                // "Não tem" só depois de perguntar também ao YouTube Music.
                Ok(None) if lyrics_id.is_some() => crate::lyrics::store(&cache, &video_id, None),
                _ => {}
            }
            Reply::Lyrics { result, video_id }
        });
    }

    /// A letra da próxima da fila, adiantada: só entra (e só é guardada) se
    /// for sincronizada; o resto fica para quando ela tocar, com a reserva do
    /// YouTube Music.
    pub fn prefetch_lyrics(&self, video_id: String, track: Track, cache: PathBuf) {
        self.spawn("letra-proxima", move |client, http| {
            if let Some(hit) = crate::lyrics::cached(&cache, &video_id) {
                return match hit {
                    Some(song) => Reply::Lyrics { result: Ok(Some(song)), video_id },
                    None => Reply::Quiet,
                };
            }
            match crate::lyrics::fetch(client, http, &track, None) {
                Ok(Some(song)) if !song.lines.is_empty() => {
                    crate::lyrics::store(&cache, &video_id, Some(&song));
                    Reply::Lyrics { result: Ok(Some(song)), video_id }
                }
                _ => Reply::Quiet,
            }
        });
    }

    /// Traduz a letra da música para `target` ("pt") pelo `service`.
    pub fn translate_lyrics(&self, video_id: String, song: Song, target: String, service: crate::look::TranslateService, local_model: Option<String>) {
        let failed = Reply::Translated { video_id: video_id.clone(), result: Err(musicfast_i18n::t().lyrics_internal_error.to_owned()) };
        self.spawn_or("traducao", Some(failed), move |_, http| Reply::Translated {
            result: crate::lyrics::translate(http, &song, &target, service, local_model.as_deref()),
            video_id,
        });
    }

    /// A faxina do cache de letras (uma vez, ao abrir).
    pub fn measure_storage(&self, paths: crate::paths::Paths) {
        self.spawn("cache-medida", move |_, _| Reply::Storage(crate::storage::measure(&paths)));
    }

    /// `browser`: também o cache da janela de login (fechada).
    pub fn clear_cache(&self, paths: crate::paths::Paths, browser: bool) {
        self.spawn("cache-faxina", move |_, _| {
            let freed = crate::storage::clear(&paths, browser);
            Reply::CacheCleared { freed, usage: crate::storage::measure(&paths) }
        });
    }

    pub fn clean_lyrics(&self, cache: PathBuf) {
        self.spawn("letras-faxina", move |_, _| {
            crate::lyrics::clean_cache(&cache);
            Reply::Quiet
        });
    }

    /// Todas as letras candidatas da música (para "Procurar outra letra").
    pub fn lyric_alternatives(&self, video_id: String, track: Track, lyrics_id: Option<String>) {
        let failed = Reply::LyricAlternatives { video_id: video_id.clone(), result: Err(musicfast_i18n::t().lyrics_internal_error.to_owned()) };
        self.spawn_or("outras-letras", Some(failed), move |client, http| Reply::LyricAlternatives {
            result: crate::lyrics::alternatives(client, http, &track, lyrics_id.as_deref()),
            video_id,
        });
    }

    pub fn related(&self, video_id: String, related_id: String) {
        let failed = Reply::Related { video_id: video_id.clone(), result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        self.spawn_or("relacionados", Some(failed), move |client, _| Reply::Related {
            result: client.browse(&related_id, None).map_err(|e| e.to_string()),
            video_id,
        });
    }

    /// Baixa a capa para `path` (os controles de mídia só leem arquivo).
    pub fn art(&self, video_id: String, url: String, path: PathBuf) {
        self.spawn("capa-atual", move |_, http| {
            if let Some(dir) = path.parent() {
                crate::app::clean_art(dir);
            }
            let bytes = crate::covers::read(&http, &url);
            match bytes {
                Ok(bytes) => {
                    if let Err(error) = std::fs::write(&path, &bytes) {
                        log::warn!("capa em disco: {error}");
                    }
                }
                Err(error) => log::warn!("capa atual: {error}"),
            }
            Reply::Art { video_id, path }
        });
    }

    pub fn refresh_version(&self) {
        self.spawn("versao", move |client, _| match client.refresh_version() {
            Ok(version) => Reply::Version(version),
            Err(error) => {
                log::info!("versão do cliente não lida: {error}");
                Reply::Version(String::new())
            }
        });
    }

    /// `silent`: renova a sessão pelo perfil guardado, sem mostrar janela.
    pub fn sign_in(&self, silent: bool, slot: String, profile: PathBuf) {
        let failed = Reply::Login { silent, slot: slot.clone(), result: Err(musicfast_i18n::t().login_window_crashed.into()) };
        self.spawn_or("login", Some(failed), move |_, _| Reply::Login { silent, slot, result: crate::login::sign_in(silent, &profile) });
    }

    pub fn account(&self) {
        self.spawn("conta", |client, _| Reply::Account(client.account().map_err(|e| e.to_string())));
    }

    /// As playlists da conta, para a coluna da esquerda e o "Salvar em".
    pub fn playlists(&self) {
        self.spawn("playlists", |client, _| {
            let result = client
                .browse("FEmusic_liked_playlists", None)
                .map(|page| {
                    page.shelves
                        .into_iter()
                        .flat_map(|s| s.items)
                        .filter_map(|i| match i {
                            Item::Card(c) if c.kind == Kind::Playlist && !c.browse_id.is_empty() => Some(c),
                            _ => None,
                        })
                        .collect()
                })
                .map_err(|e| e.to_string());
            Reply::Playlists(result)
        });
    }

    /// Uma ação da conta, com a mensagem que aparece quando dá certo.
    pub fn act(
        &self,
        message: impl Into<String>,
        refresh: Refresh,
        work: impl FnOnce(&Client) -> musicfast_ytm::client::Result<()> + Send + 'static,
    ) {
        let message = message.into();
        let failed = Reply::Done { message: String::new(), result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()), refresh: Refresh::Nothing };
        self.spawn_or("conta", Some(failed), move |client, _| Reply::Done {
            result: work(client).map_err(|e| e.to_string()),
            message,
            refresh,
        });
    }

    pub fn add_history(&self, video_id: String) {
        let client = self.client.clone();
        let _ = std::thread::Builder::new().name("historico".into()).spawn(move || {
            if let Err(error) = client.add_history(&video_id) {
                log::info!("histórico da conta: {error}");
            }
        });
    }
}
