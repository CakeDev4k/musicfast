//! A janela: junta o catálogo, o player, as capas e as telas, aplica o que
//! a pessoa pediu e guarda a sessão.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Align2, CornerRadius, Frame, Key, Modifiers, PointerButton, Sense, Ui, ViewportCommand, vec2};
use fastframe_instance::Guard;
use fastframe_now_playing as np;
use musicfast_player::{Command, Config, Offline, Player, Repeat, Snapshot, Status};
use musicfast_i18n::{Lang, t};
use musicfast_ytm::{Auth, Client, LikeStatus, Session};

use crate::Wake;
use crate::covers::Covers;
use crate::jobs::{Jobs, Refresh, Reply};
use crate::paths::Paths;
use crate::state::{Saved, unix_now};
use crate::ui::bar::{self, BarState};
use crate::ui::nav::{self, SearchBox};
use crate::ui::player::{self, Tab};
use crate::ui::side::{self, LyricsState};
use crate::ui::{Action, Cx, Dialog, Me, PageState, Side, View, dialog, kit, mini, page, play_command, theme};

/// O mini player aberto.
struct Mini {
    lyrics: bool,
    /// A janela estava maximizada antes: volta maximizada.
    was_maximized: bool,
}

/// O que a fila do YouTube diz da música atual.
struct TrackInfo {
    /// A resposta chegou (a entrada vazia é posta no pedido).
    answered: bool,
    lyrics_id: Option<String>,
    related_id: Option<String>,
}

pub struct Startup {
    pub paths: Paths,
    pub saved: Saved,
    pub wake: Arc<Wake>,
    pub instance: Guard,
    /// O app acabou de ser atualizado: confirmar ao ajudante que abriu.
    pub update_receipt: Option<fastframe_update::Receipt>,
    /// A atualização não abriu e o ajudante voltou a versão anterior.
    pub update_error: Option<String>,
}

pub struct MusicApp {
    paths: Paths,
    wake: Arc<Wake>,
    _instance: Guard,
    client: Arc<Client>,
    player: Player,
    jobs: Jobs,
    replies: Receiver<Reply>,
    covers: Covers,

    snap: Snapshot,
    snap_rev: u64,
    history: Vec<View>,
    at: usize,
    pages: HashMap<View, PageState>,
    search: SearchBox,
    side: Option<Side>,
    sidebar_mini: bool,
    sidebar_width: f32,
    /// O mini player, enquanto a janela é ele.
    mini: Option<Mini>,
    mini_anchor: Option<[f32; 2]>,
    /// Os cantos da janela a acertar no próximo quadro (redondos no mini).
    round_corners: Option<bool>,
    /// A última posição e tamanho com a janela normal (`state::WindowGeom`).
    window: Option<crate::state::WindowGeom>,
    lyrics: HashMap<String, LyricsState>,
    /// As letras que falharam por erro (rede, LRCLIB ocupado): quando e
    /// quantas vezes, para pedir de novo sozinho.
    lyrics_failed: HashMap<String, (Instant, u8)>,
    /// As letras candidatas de cada música (as de "Procurar outra letra") e
    /// se a troca está esperando a lista chegar.
    lyric_alts: HashMap<String, Vec<crate::lyrics::Song>>,
    lyric_alts_waiting: Option<String>,
    /// A tradução da letra: ligada, as músicas sendo traduzidas e as que
    /// falharam (não pede de novo nesta sessão).
    translate_lyrics: bool,
    translating: std::collections::HashSet<String>,
    translate_failed: std::collections::HashSet<String>,
    /// As fontes da letra e da tradução já postas no egui.
    lyrics_fonts: crate::look::LyricsFonts,
    /// A próxima da fila cuja letra já foi pedida adiantada.
    lyrics_prefetched: Option<String>,
    info: HashMap<String, TrackInfo>,
    related: HashMap<String, PageState>,
    bar: BarState,
    warmed: Option<String>,
    me: Me,
    dialog: Option<Dialog>,
    expanded: bool,
    player_tab: Tab,
    /// A renovação silenciosa da sessão já foi tentada desde a última
    /// conferência boa (uma por vencimento, para não girar em falso).
    renew_tried: bool,
    /// A última conferência da conta: o app fica aberto dias, e a sessão
    /// pode vencer no meio.
    account_checked: Instant,
    /// A música já contada no histórico da conta.
    history_for: Option<String>,
    toast: Option<(String, f64)>,
    last_notice: u64,

    controls: Option<np::NowPlaying>,
    /// A capa da música atual em disco, e para qual música foi pedida.
    art: Option<(String, PathBuf)>,
    art_for: Option<String>,
    last_position: (f64, Instant),
    /// O que os controles de mídia receberam por último: a revisão do
    /// `Snapshot`, se já tinha a capa, e quando.
    controls_sent: (u64, bool, Instant),
    title_for: Option<String>,

    /// A última sessão gravada, sem a fila (ela fica em `saved_queue`, por
    /// ponteiro: a comparação não copia 2.000 músicas).
    saved: Saved,
    saved_queue: Arc<Vec<musicfast_ytm::Track>>,
    saved_at: Instant,
    scrolling: fastframe_scroll::Scrolling,
    first_frame_logged: bool,
    updates: crate::update::Updates,
    /// Quando perguntar de novo se saiu versão nova.
    next_update_check: Instant,
    update_receipt: Option<fastframe_update::Receipt>,
    toast_on_start: Option<String>,
    /// O idioma escolhido pela pessoa; `None` segue o do sistema.
    language: Option<Lang>,
    /// O fundo do player, e a imagem dele quando é de um link.
    look: crate::look::Look,
    backdrops: crate::look::Images,
    /// O som de agora, para o visualizador.
    analyzer: crate::viz::Analyzer,
    /// A janela de escolher arquivo aberta, e para qual superfície.
    picking: Option<std::sync::mpsc::Receiver<(crate::look::Surface, Option<PathBuf>)>>,
    /// As músicas baixadas para ouvir sem internet.
    offline: Arc<Offline>,
    /// As baixadas como a página as desenha, e de qual revisão do `Offline`.
    download_tracks: (u64, Arc<Vec<musicfast_ytm::Track>>),
    /// A organização da janela.
    shell: crate::ui::Shell,
    /// A biblioteca da interface em painéis.
    library: crate::ui::panels::Library,
    /// A aba, o filtro e a ordem escolhidos na página das baixadas.
    downloads_page: crate::ui::downloads::Browser,
    /// A janela de escolher a pasta das baixadas, aberta.
    picking_folder: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    /// A música no perfil do Discord: o canal, as escolhas e o que foi
    /// mandado por último (`video_id`, tocando, quando começou em ms).
    discord: crate::discord::Discord,
    discord_settings: crate::discord::Settings,
    discord_sent: Option<(String, bool, u64)>,
    /// O HWND da janela (o arrasto do arquivo para fora precisa dele).
    window_handle: Option<isize>,
    /// Um arrasto de arquivo acabou: o laço do Windows comeu o soltar do
    /// botão, e o egui precisa ouvi-lo (`raw_input_hook`).
    release_pointer: bool,
}

impl MusicApp {
    pub fn new(ctx: &egui::Context, startup: Startup) -> Self {
        let Startup { paths, saved, wake, instance, update_receipt, update_error } = startup;
        crate::ui::theme::set_panels(saved.shell == crate::ui::Shell::Panels);
        let language = saved.language.as_deref().and_then(Lang::from_tag);
        let locale = musicfast_i18n::apply(language);
        log::info!("idioma {} ({}), sistema {:?}", locale.tag(), if language.is_some() { "escolhido" } else { "automático" }, musicfast_i18n::system());
        let client = Arc::new(Client::new(locale.lang.code(), &locale.region).expect("cliente HTTP"));
        client.set_visitor(saved.visitor.clone());
        if let Some(version) = &saved.client_version {
            client.set_version(version);
        }
        let http = Arc::new(
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                .connect_timeout(Duration::from_secs(6))
                .pool_idle_timeout(Duration::from_secs(90))
                .build()
                .expect("cliente HTTP"),
        );
        let repaint = ctx.clone();
        let look = saved.look.clone();
        let backdrops = crate::look::Images::new(ctx, http.clone(), &look);
        let offline_waker = ctx.clone();
        let folders = musicfast_player::offline::Folders {
            index: paths.download_index.clone(),
            default_dir: paths.downloads.clone(),
            covers: paths.download_covers.clone(),
            bin: paths.bin.clone(),
            cookies: paths.root.join("tmp"),
        };
        let offline = Offline::open(folders, saved.downloads_dir.clone(), Arc::new(move || offline_waker.request_repaint()));
        let player = Player::start(
            Config {
                bin_dir: paths.bin.clone(),
                cache_file: paths.resolved.clone(),
                client: client.clone(),
                volume: saved.volume,
                offline: Some(offline.clone()),
            },
            Arc::new(move || repaint.request_repaint()),
        );
        // A conta vai antes da fila restaurada: a música dela já é pedida
        // como a conta (256 kbps no Premium).
        let mut accounts = crate::accounts::sane(saved.accounts.clone());
        let mut active = saved.active_account.clone().filter(|id| accounts.iter().any(|p| p.id == *id));
        // Sem lista, vale o lugar de antes das várias contas.
        let slot = active.clone().or_else(|| accounts.is_empty().then(String::new));
        let session_cookie = slot.as_deref().and_then(|id| crate::vault::load(&paths.session_of(id))).filter(|c| Auth::from_cookie_header(c).is_some());
        if session_cookie.is_some() && accounts.is_empty() {
            accounts.push(crate::accounts::Profile::new(""));
            active = Some(String::new());
        }
        player.send(Command::SetAccount(session_cookie.clone()));
        offline.set_account(session_cookie.clone());
        player.send(Command::SetShuffle(saved.shuffle));
        player.send(Command::SetRepeat(saved.repeat));
        if !saved.queue.is_empty() {
            let index = saved.index.unwrap_or(0).min(saved.queue.len() - 1);
            player.send(Command::Restore { tracks: saved.queue.clone(), index, position: saved.position });
        }
        let (jobs, replies) = Jobs::new(client.clone(), http.clone(), ctx.clone());
        // A versão do cliente muda a cada poucos dias; a padrão vale por
        // meses, então a leitura fica em segundo plano e uma vez por dia.
        if unix_now().saturating_sub(saved.version_checked) > 24 * 3600 {
            jobs.refresh_version();
        }
        // A sessão da conta, se houver: as chamadas já saem logadas, e a
        // conferência (`account`) diz se ela ainda vale.
        let shown = active.as_ref().and_then(|id| accounts.iter().find(|p| p.id == *id)).and_then(|p| p.account());
        let mut me = Me { accounts, active, ..Me::default() };
        if let Some(auth) = session_cookie.as_deref().and_then(Auth::from_cookie_header) {
            client.set_auth(Some(auth));
            me.signed_in = true;
            me.account = shown;
            jobs.account();
            jobs.playlists();
        }
        jobs.load(View::Home);
        jobs.clean_lyrics(paths.lyrics.clone());
        let waker = ctx.clone();
        let controls = np::NowPlaying::start(np::App::new("musicfast", "MusicFast"), move || waker.request_repaint());
        let waker = ctx.clone();
        let discord = crate::discord::Discord::start(move || waker.request_repaint());
        Self {
            covers: Covers::new(ctx, http),
            wake,
            _instance: instance,
            client,
            player,
            jobs,
            replies,
            snap: Snapshot::default(),
            snap_rev: u64::MAX,
            history: vec![View::Home],
            at: 0,
            pages: HashMap::from([(View::Home, PageState::default())]),
            search: SearchBox::default(),
            side: saved.side.as_deref().and_then(Side::from_key),
            sidebar_mini: saved.sidebar_mini,
            sidebar_width: nav::sidebar_width(saved.sidebar_width),
            mini: None,
            mini_anchor: saved.mini_anchor,
            round_corners: None,
            window: saved.window.filter(crate::state::WindowGeom::is_sane),
            lyrics: HashMap::new(),
            lyrics_failed: HashMap::new(),
            lyric_alts: HashMap::new(),
            lyric_alts_waiting: None,
            lyrics_prefetched: None,
            translate_lyrics: saved.translate_lyrics,
            translating: std::collections::HashSet::new(),
            translate_failed: std::collections::HashSet::new(),
            lyrics_fonts: crate::look::LyricsFonts::default(),
            bar: BarState {
                seeking: None,
                unmuted: saved.volume.max(0.1),
                layouts: saved.bar.clone().sane(),
                discord_on: saved.discord.on,
                discord_link: crate::discord::Link::Off,
            },
            discord,
            discord_settings: saved.discord.clone(),
            discord_sent: None,
            warmed: None,
            toast: None,
            last_notice: 0,
            controls: Some(controls),
            info: HashMap::new(),
            related: HashMap::new(),
            me,
            dialog: None,
            expanded: false,
            player_tab: Tab::default(),
            history_for: None,
            renew_tried: false,
            account_checked: Instant::now(),
            art: None,
            art_for: None,
            last_position: (0.0, Instant::now()),
            controls_sent: (u64::MAX, false, Instant::now()),
            title_for: None,
            saved: Saved { queue: Vec::new(), ..saved },
            saved_queue: Arc::new(Vec::new()),
            saved_at: Instant::now(),
            scrolling: fastframe_scroll::Scrolling::default(),
            first_frame_logged: false,
            updates: crate::update::Updates::new(ctx),
            // Uns segundos depois de abrir: o primeiro quadro e a música vêm antes.
            next_update_check: Instant::now() + Duration::from_secs(8),
            update_receipt,
            toast_on_start: update_error,
            language,
            look,
            backdrops,
            analyzer: crate::viz::Analyzer::default(),
            picking: None,
            offline,
            download_tracks: (u64::MAX, Arc::new(Vec::new())),
            shell: saved.shell,
            library: Default::default(),
            downloads_page: Default::default(),
            picking_folder: None,
            window_handle: None,
            release_pointer: false,
            paths,
        }
    }

    fn view(&self) -> &View {
        &self.history[self.at]
    }

    fn ensure_loaded(&mut self, view: &View) {
        // As baixadas vêm do disco (`Offline`), não do YouTube.
        if *view == View::Downloads {
            return;
        }
        let needs = self.pages.get(view).is_none_or(|s| s.page.is_none() && s.error.is_some());
        if needs {
            self.pages.insert(view.clone(), PageState::default());
            self.jobs.load(view.clone());
        }
    }

    fn open(&mut self, view: View) {
        if let View::Search { query, .. } = &view {
            self.search.text = query.clone();
            // Uma busca refeita traz o resultado de agora.
            if *self.view() != view {
                self.pages.remove(&view);
            }
        }
        if *self.view() == view {
            return;
        }
        self.history.truncate(self.at + 1);
        self.history.push(view.clone());
        self.at = self.history.len() - 1;
        // O histórico não cresce sem fim; as páginas de fora dele saem.
        if self.history.len() > 60 {
            self.history.remove(0);
            self.at -= 1;
        }
        self.pages.retain(|v, _| self.history.contains(v));
        self.ensure_loaded(&view);
    }

    fn go(&mut self, to: usize) {
        if to < self.history.len() && to != self.at {
            self.at = to;
            let view = self.view().clone();
            if let View::Search { query, .. } = &view {
                self.search.text = query.clone();
            }
            self.ensure_loaded(&view);
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open(view) => self.open(view),
            Action::Back => self.go(self.at.saturating_sub(1)),
            Action::Forward => self.go(self.at + 1),
            Action::Reload => {
                let view = self.view().clone();
                self.pages.insert(view.clone(), PageState::default());
                self.jobs.load(view);
            }
            Action::Cmd(cmd) => self.player.send(cmd),
            Action::Play(play, seed) => match play_command(&play, seed) {
                Some(cmd) => self.player.send(cmd),
                None => self.say(ctx, t().nothing_to_play),
            },
            Action::Warm(video_id) => {
                if self.warmed.as_deref() != Some(&video_id) {
                    self.player.send(Command::Warm(video_id.clone()));
                    self.warmed = Some(video_id);
                }
            }
            Action::MoreSections => {
                let view = self.view().clone();
                if let Some(state) = self.pages.get_mut(&view)
                    && state.loading_more.is_none()
                    && let Some(token) = state.page.as_ref().and_then(|p| p.continuation.clone())
                {
                    state.loading_more = Some(None);
                    self.jobs.more_sections(view, token);
                }
            }
            Action::MoreItems(shelf) => {
                let view = self.view().clone();
                if let Some(state) = self.pages.get_mut(&view)
                    && state.loading_more.is_none()
                    && let Some(token) = state.page.as_ref().and_then(|p| p.shelves.get(shelf)).and_then(|s| s.continuation.clone())
                {
                    state.loading_more = Some(Some(shelf));
                    self.jobs.more_items(view, shelf, token);
                }
            }
            Action::ToggleSidebar => self.sidebar_mini = !self.sidebar_mini,
            Action::SetShell(shell) => {
                self.shell = shell;
                crate::ui::theme::set_panels(shell == crate::ui::Shell::Panels);
            }
            Action::LoadLibrary(tab) => self.ensure_loaded(&View::Library(tab)),
            Action::ToggleSide(which) => {
                self.side = if self.side == Some(which) { None } else { Some(which) };
                self.expanded = false;
            }
            Action::Expand(on) => self.expanded = on && self.snap.current().is_some(),
            Action::Mini(true) => self.enter_mini(ctx),
            Action::Mini(false) => self.leave_mini(ctx),
            Action::SignIn => {
                let slot = self.me.active.clone().unwrap_or_else(|| crate::accounts::free_id(&self.me.accounts));
                self.open_login(ctx, slot);
            }
            Action::AddAccount => self.open_login(ctx, crate::accounts::new_id()),
            Action::SwitchAccount(id) => self.switch_account(ctx, &id),
            Action::SignOut => self.sign_out(ctx, t().signed_out),
            Action::SubmitCookies(cookie) => self.start_session(ctx, &cookie, false),
            Action::RestartToUpdate => match self.updates.restart() {
                Ok(true) => {
                    log::info!("fechando para atualizar");
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                Ok(false) => {}
                Err(error) => {
                    log::warn!("atualização: {error}");
                    self.say(ctx, t().update_install_failed);
                }
            },
            Action::Dialog(d) => self.dialog = Some(d),
            Action::Storage => {
                self.dialog = Some(Dialog::Storage { usage: None, busy: false });
                self.jobs.measure_storage(self.paths.clone());
            }
            Action::ClearCache => self.jobs.clear_cache(self.paths.clone(), !self.me.signing_in),
            Action::Rate(video_id, status) => {
                self.me.likes.insert(video_id.clone(), status);
                let message = match status {
                    LikeStatus::Like => t().added_to_liked,
                    LikeStatus::Dislike => t().thanks_for_feedback,
                    LikeStatus::Indifferent => t().removed_from_liked,
                };
                let refresh = Refresh::Page(View::browse("VLLM", None));
                self.jobs.act(message, refresh, move |c| c.rate_song(&video_id, status));
            }
            Action::SaveCollection(playlist_id, on) => {
                self.me.saved.insert(playlist_id.clone(), on);
                let message = if on { t().saved_to_library } else { t().removed_from_library };
                self.jobs.act(message, Refresh::Playlists, move |c| c.rate_playlist(&playlist_id, on));
            }
            Action::Subscribe(channel, on) => {
                self.me.subscribed.insert(channel.clone(), on);
                let message = if on { t().subscription_done } else { t().subscription_cancelled };
                self.jobs.act(message, Refresh::Nothing, move |c| c.subscribe(&channel, on));
            }
            Action::AddToPlaylist { playlist_id, title, video_id } => {
                let refresh = Refresh::Page(View::browse(&playlist_id, None));
                self.jobs.act(t().saved_in(&title), refresh, move |c| c.add_to_playlist(&playlist_id, &video_id));
            }
            Action::CreatePlaylist { title, privacy, video_id } => {
                let videos: Vec<String> = video_id.into_iter().collect();
                let message = t().playlist_created(&title);
                self.jobs.act(message, Refresh::Playlists, move |c| c.create_playlist(&title, privacy, &videos).map(|_| ()));
            }
            Action::SetLanguage(choice) => self.set_language(choice),
            Action::OtherLyrics(video_id) => self.other_lyrics(ctx, video_id),
            Action::ToggleTranslation => {
                self.translate_lyrics = !self.translate_lyrics;
                // Ligar de novo tenta de novo as que tinham falhado.
                self.translate_failed.clear();
            }
            Action::PersonalizeLyrics => {
                self.apply(ctx, Action::Personalize);
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("personalizar-secao"), 2usize));
            }
            Action::Personalize => {
                self.expanded = self.snap.current().is_some();
                let skin = &self.look.player;
                self.dialog = Some(Dialog::Look {
                    look: self.look.clone(),
                    surface: crate::look::Surface::Player,
                    url: if skin.image_url.starts_with("http") { skin.image_url.clone() } else { String::new() },
                    hsv: dialog::to_hsv(skin.color),
                    custom: !crate::look::PRESETS.contains(&skin.color),
                });
            }
            Action::SetLook(look) => {
                // Outro idioma ou serviço de tradução: as que falharam tentam de novo.
                if look.lyrics.translation_key() != self.look.lyrics.translation_key() {
                    self.translate_failed.clear();
                }
                let rescaled = look.mini_layout.scale() != self.look.mini_layout.scale();
                self.look = look;
                self.backdrops.sync(ctx, &self.look);
                if rescaled && self.mini.is_some() {
                    let scale = self.look.mini_layout.scale();
                    ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(mini::width(scale), mini::row_height(scale))));
                    self.place_mini(ctx);
                }
            }
            Action::PickBackdropFile(surface) => {
                if self.picking.is_some() {
                    return;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                let waker = ctx.clone();
                let filter = t().image_files;
                let _ = std::thread::Builder::new().name("escolher-fundo".into()).spawn(move || {
                    let picked = rfd::FileDialog::new().add_filter(filter, &["png", "jpg", "jpeg", "webp", "gif"]).pick_file();
                    let _ = tx.send((surface, picked));
                    waker.request_repaint();
                });
                self.picking = Some(rx);
            }
            Action::CustomizeBar => {
                self.dialog = Some(Dialog::Bar { layouts: self.bar.layouts.clone(), discord: self.discord_settings.clone(), link: self.discord.link() });
            }
            Action::SetBar(layouts) => self.bar.layouts = layouts,
            Action::ToggleDiscord => {
                let on = !self.discord_settings.on;
                let settings = crate::discord::Settings { on, ..self.discord_settings.clone() };
                self.set_discord(settings);
                self.say(ctx, if on { t().discord_turned_on } else { t().discord_turned_off });
            }
            Action::SetDiscord(settings) => self.set_discord(settings),
            Action::Download(track) => {
                self.offline.dismiss(&track.video_id);
                self.offline.download(track);
            }
            Action::CopyLink(video_id) => {
                ctx.copy_text(format!("https://music.youtube.com/watch?v={video_id}"));
                self.say(ctx, t().link_copied);
            }
            Action::DeleteDownload(video_id) => {
                if let Err(error) = self.offline.delete(&video_id) {
                    self.say(ctx, &t().delete_download_failed(&error));
                }
            }
            Action::DragOut(path) => {
                match crate::dragout::start(self.window_handle, &path) {
                    Ok(crate::dragout::Outcome::Unsupported) => crate::dragout::reveal(&path),
                    Ok(outcome) => {
                        log::info!("arrasto de {}: {outcome:?}", path.display());
                        self.release_pointer = true;
                        ctx.request_repaint();
                    }
                    Err(error) => {
                        log::warn!("arrasto de {}: {error}", path.display());
                        self.release_pointer = true;
                        self.say(ctx, &t().drag_out_failed(&error));
                    }
                }
            }
            Action::Reveal(path) => crate::dragout::reveal(&path),
            Action::OpenDownloads => crate::dragout::open_folder(&self.offline.dir()),
            Action::PickDownloadsFolder => {
                if self.picking_folder.is_some() {
                    return;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                let waker = ctx.clone();
                let start = self.offline.dir();
                let _ = std::thread::Builder::new().name("escolher-pasta".into()).spawn(move || {
                    let picked = rfd::FileDialog::new().set_directory(start).pick_folder();
                    let _ = tx.send(picked);
                    waker.request_repaint();
                });
                self.picking_folder = Some(rx);
            }
            Action::ResetDownloadsFolder => self.set_downloads_dir(ctx, None),
        }
    }

    /// As escolhas do Discord, que valem na hora: a música volta a ser
    /// mandada com elas, ou o canal fecha e ela sai do perfil.
    fn set_discord(&mut self, settings: crate::discord::Settings) {
        self.bar.discord_on = settings.on;
        self.discord_settings = settings;
        self.discord_sent = None;
        if !self.discord_settings.on {
            self.discord.stop();
        }
    }

    /// Avisa o Discord quando a música muda, pausa ou salta. Em pausa ela
    /// sai do perfil, como no Spotify; no carregamento fica como está, para
    /// não piscar entre uma música e outra.
    fn discord_presence(&mut self) {
        self.bar.discord_link = self.discord.link();
        if let Some(Dialog::Bar { link, .. }) = self.dialog.as_mut() {
            *link = self.bar.discord_link;
        }
        if !self.bar.discord_on {
            return;
        }
        let snap = &self.snap;
        let track = snap.current().filter(|t| !t.video_id.is_empty());
        if track.is_some() && snap.status == Status::Loading {
            return;
        }
        let playing = track.is_some() && snap.status == Status::Playing;
        let id = track.map(|t| t.video_id.clone()).unwrap_or_default();
        let position_ms = (self.player.position().max(0.0) * 1000.0) as u64;
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let start_ms = now_ms.saturating_sub(position_ms);
        // Só um salto de verdade reenvia: a conta oscila uns milissegundos.
        if let Some((sent_id, sent_playing, sent_start)) = &self.discord_sent
            && *sent_id == id
            && *sent_playing == playing
            && (!playing || sent_start.abs_diff(start_ms) < 3000)
        {
            return;
        }
        let activity = track.filter(|_| playing).map(|song| {
            let duration = snap.duration.or(song.duration_secs.map(f64::from));
            let shown = crate::discord::Song {
                video_id: song.video_id.clone(),
                title: song.title.clone(),
                artist: song.artist_names(),
                album: song.album.as_ref().map(|a| a.name.clone()),
                cover: song.thumbs.url_for(512),
                start_ms,
                end_ms: duration.filter(|d| *d > 0.0).map(|d| start_ms + (d * 1000.0) as u64),
            };
            crate::discord::activity(&shown, self.discord_settings.promote.then_some(t().discord_site_button), t().discord_listen)
        });
        self.discord.set(crate::discord::APP_ID, activity);
        self.discord_sent = Some((id, playing, start_ms));
    }

    /// A pasta dos próximos downloads (`None`: a do app), guardada já na
    /// sessão para valer mesmo se o app fechar de repente.
    fn set_downloads_dir(&mut self, ctx: &egui::Context, dir: Option<PathBuf>) {
        let dir = dir.filter(|d| d.as_path() != self.offline.default_dir());
        if let Some(d) = &dir
            && let Err(error) = std::fs::create_dir_all(d)
        {
            self.say(ctx, &t().folder_unusable(&error.to_string()));
            return;
        }
        self.offline.set_dir(dir);
        self.session().save_now(&self.paths.state);
    }

    /// "Procurar outra letra": a próxima candidata diferente da que está na
    /// tela. A lista é buscada na primeira vez; a troca espera ela chegar.
    fn other_lyrics(&mut self, ctx: &egui::Context, video_id: String) {
        let Some(alts) = self.lyric_alts.get(&video_id) else {
            if self.lyric_alts_waiting.as_deref() == Some(video_id.as_str()) {
                return;
            }
            let Some(track) = self.snap.current().filter(|t| t.video_id == video_id).cloned() else { return };
            let lyrics_id = self.info.get(&video_id).and_then(|i| i.lyrics_id.clone());
            self.lyric_alts_waiting = Some(video_id.clone());
            self.jobs.lyric_alternatives(video_id, track, lyrics_id);
            self.say(ctx, t().lyrics_searching);
            return;
        };
        let current = match self.lyrics.get(&video_id) {
            Some(LyricsState::Ready(Some(song))) => crate::lyrics::fingerprint(song),
            _ => String::new(),
        };
        let at = alts.iter().position(|s| crate::lyrics::fingerprint(s) == current);
        // A seguinte à da tela (ou a primeira), dando a volta.
        let next = match at {
            Some(i) if alts.len() > 1 => Some((i + 1) % alts.len()),
            Some(_) => None,
            None => (!alts.is_empty()).then_some(0),
        };
        match next {
            Some(i) => {
                let message = t().lyrics_choice(i + 1, alts.len());
                let mut chosen = alts[i].clone();
                // Quem trocou de letra segue podendo trocar.
                chosen.doubtful = true;
                let dir = self.paths.lyrics.clone();
                let id = video_id.clone();
                let keep = chosen.clone();
                let _ = std::thread::Builder::new().name("letra-escolhida".into()).spawn(move || crate::lyrics::store(&dir, &id, Some(&keep)));
                self.lyrics.insert(video_id.clone(), LyricsState::Ready(Some(chosen)));
                self.say(ctx, &message);
            }
            None => self.say(ctx, t().lyrics_no_other),
        }
    }

    /// Troca o idioma da interface e dos títulos do YouTube, e recarrega as
    /// páginas (os nomes das prateleiras vêm no idioma pedido).
    fn set_language(&mut self, choice: Option<Lang>) {
        let before = musicfast_i18n::current();
        self.language = choice;
        let locale = musicfast_i18n::apply(choice);
        log::info!("idioma trocado para {}", locale.tag());
        if locale.lang == before {
            return;
        }
        self.client.set_locale(locale.lang.code(), &locale.region);
        if self.me.signed_in {
            self.jobs.playlists();
        }
        self.search.suggestions.clear();
        self.search.suggested_for.clear();
        self.reload_all();
    }

    /// A imagem escolhida no computador (pela janela ou arrastada): copiada
    /// para a pasta do app e posta como o fundo da superfície.
    fn adopt_backdrop(&mut self, ctx: &egui::Context, surface: crate::look::Surface, path: &std::path::Path) {
        match crate::look::adopt(&self.paths.backdrops, surface, path) {
            Ok(copy) => {
                let skin = self.look.skin_mut(surface);
                skin.backdrop = crate::look::Backdrop::Image;
                skin.image_url = copy;
                self.backdrops.sync(ctx, &self.look);
                // O painel aberto edita uma cópia: ela segue a escolha.
                if let Some(Dialog::Look { look, .. }) = self.dialog.as_mut() {
                    *look = self.look.clone();
                }
            }
            Err(error) => self.say(ctx, &t().image_load_failed(&error)),
        }
    }

    /// Entra com os cookies (da janela de login ou colados) e refaz o que
    /// muda com a conta: início, biblioteca, playlists.
    /// `quiet`: a renovação silenciosa, que não avisa nada a ninguém.
    fn start_session(&mut self, ctx: &egui::Context, cookie: &str, quiet: bool) {
        self.me.signing_in = false;
        let Some(auth) = Auth::from_cookie_header(cookie) else {
            self.say(ctx, t().session_missing);
            return;
        };
        // Os cookies colados não passam pela janela: entram num lugar aqui.
        if self.me.active.is_none() {
            self.enter_slot(&crate::accounts::free_id(&self.me.accounts));
        }
        self.client.set_auth(Some(auth));
        self.player.send(Command::SetAccount(Some(cookie.to_owned())));
        self.offline.set_account(Some(cookie.to_owned()));
        crate::vault::save(&self.session_file(), cookie);
        self.me.signed_in = true;
        self.jobs.account();
        self.jobs.playlists();
        self.account_checked = Instant::now();
        self.reload_all();
        // A lista de contas não espera a gravação periódica: fechar agora
        // reabriria numa conta que já não existe.
        self.session().save_now(&self.paths.state);
        if !quiet {
            self.say(ctx, t().connected);
        }
    }

    /// Onde a sessão da conta aberta fica guardada.
    fn session_file(&self) -> PathBuf {
        self.paths.session_of(self.me.active.as_deref().unwrap_or_default())
    }

    /// A janela de login de uma conta (nova, ou uma guardada que venceu).
    fn open_login(&mut self, ctx: &egui::Context, slot: String) {
        self.me.signing_in = true;
        let profile = self.paths.webview_of(&slot);
        self.jobs.sign_in(false, slot, profile);
        self.say(ctx, t().sign_in_in_window);
    }

    /// Passa a estar na conta `id`: tudo que era da anterior sai da tela e o
    /// que vinha a caminho para ela é descartado. A música não para.
    fn enter_slot(&mut self, id: &str) {
        self.jobs.new_account();
        let mut accounts = std::mem::take(&mut self.me.accounts);
        if !accounts.iter().any(|p| p.id == id) {
            accounts.push(crate::accounts::Profile::new(id));
        }
        let account = accounts.iter().find(|p| p.id == id).and_then(|p| p.account());
        self.me = Me { accounts, active: Some(id.to_owned()), account, ..Me::default() };
        self.renew_tried = false;
    }

    /// Um clique na foto de outra conta. Sem sessão guardada (venceu), abre
    /// o login dela.
    fn switch_account(&mut self, ctx: &egui::Context, id: &str) {
        if self.me.signed_in && self.me.active.as_deref() == Some(id) {
            return;
        }
        let cookie = crate::vault::load(&self.paths.session_of(id)).filter(|c| Auth::from_cookie_header(c).is_some());
        let Some(cookie) = cookie else {
            self.open_login(ctx, id.to_owned());
            return;
        };
        self.enter_slot(id);
        self.start_session(ctx, &cookie, true);
        // A curtida da música atual é da conta nova.
        if let Some(track) = self.snap.current() {
            self.jobs.track_info(track.video_id.clone());
        }
        let name = self.me.account.as_ref().map(|a| a.name.clone());
        if let Some(name) = name {
            self.say(ctx, &t().now_in_account(&name));
        }
    }

    /// Apaga a sessão e o perfil de login de uma conta.
    fn forget_account(&mut self, id: &str) {
        crate::vault::clear(&self.paths.session_of(id));
        let profile = self.paths.webview_of(id);
        let _ = std::thread::Builder::new().name("sair".into()).spawn(move || crate::login::forget(&profile));
        self.me.accounts.retain(|p| p.id != id);
    }

    /// Sair de verdade: esquece a conta aberta, sessão e perfil do login, e
    /// a próxima entrada pede a conta de novo. Havendo outra guardada, o app
    /// passa para ela.
    fn sign_out(&mut self, ctx: &egui::Context, message: &str) {
        let id = self.me.active.take().unwrap_or_default();
        self.forget_account(&id);
        let next = self.me.accounts.iter().map(|p| p.id.clone()).find(|other| {
            crate::vault::load(&self.paths.session_of(other)).is_some_and(|c| Auth::from_cookie_header(&c).is_some())
        });
        match next {
            Some(next) => self.switch_account(ctx, &next),
            None => self.drop_session(ctx, message),
        }
    }

    /// A sessão venceu e a renovação não deu: sai, mas guarda o perfil do
    /// login (entrar de novo costuma ser só escolher a conta).
    fn drop_session(&mut self, ctx: &egui::Context, message: &str) {
        self.client.set_auth(None);
        self.player.send(Command::SetAccount(None));
        self.offline.set_account(None);
        crate::vault::clear(&self.session_file());
        let accounts = std::mem::take(&mut self.me.accounts);
        let active = self.me.active.take();
        self.me = Me { accounts, active, ..Me::default() };
        if self.view().needs_account() {
            self.history = vec![View::Home];
            self.at = 0;
        }
        self.reload_all();
        self.session().save_now(&self.paths.state);
        self.say(ctx, message);
    }

    /// Esquece as páginas carregadas (elas mudam com a conta) e recarrega a
    /// atual.
    fn reload_all(&mut self) {
        self.pages.clear();
        self.related.clear();
        self.info.clear();
        let view = self.view().clone();
        self.ensure_loaded(&view);
        // A aba de álbuns ou artistas da biblioteca em painéis vem das páginas.
        let tab = self.library.tab();
        if self.me.signed_in && tab != crate::ui::LibraryTab::Playlists {
            self.ensure_loaded(&View::Library(tab));
        }
    }

    fn say(&mut self, ctx: &egui::Context, text: &str) {
        let now = ctx.input(|i| i.time);
        self.toast = Some((text.to_owned(), now + 4.5));
    }

    fn handle_replies(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.replies.try_recv() {
            match reply {
                Reply::Page { view, result } => {
                    let state = self.pages.entry(view).or_default();
                    match result {
                        Ok(page) => {
                            state.page = Some(Arc::new(page));
                            state.error = None;
                        }
                        Err(error) => {
                            log::warn!("página: {error}");
                            state.error = Some(error);
                        }
                    }
                }
                Reply::More { view, shelf, result } => {
                    let Some(state) = self.pages.get_mut(&view) else { continue };
                    state.loading_more = None;
                    let Some(page) = state.page.as_mut() else { continue };
                    let page = Arc::make_mut(page);
                    match (result, shelf) {
                        (Ok(more), None) => {
                            page.shelves.extend(more.shelves);
                            page.continuation = more.continuation;
                        }
                        (Ok(more), Some(si)) => {
                            if let Some(target) = page.shelves.get_mut(si) {
                                match more.shelves.into_iter().next() {
                                    Some(s) if !s.items.is_empty() => {
                                        target.items.extend(s.items);
                                        target.continuation = s.continuation;
                                    }
                                    _ => target.continuation = None,
                                }
                            }
                        }
                        (Err(error), si) => {
                            // Sem repetir sem fim: a continuação some.
                            log::warn!("mais itens: {error}");
                            match si {
                                None => page.continuation = None,
                                Some(si) => {
                                    if let Some(s) = page.shelves.get_mut(si) {
                                        s.continuation = None;
                                    }
                                }
                            }
                            self.say(ctx, t().load_more_failed);
                        }
                    }
                }
                Reply::Suggestions { input, result } => {
                    if input == self.search.requested_for {
                        self.search.suggestions = result;
                        self.search.suggested_for = input;
                        self.search.selected = None;
                    }
                }
                Reply::Lyrics { video_id, result } => {
                    let state = match result {
                        Ok(lyrics) => {
                            self.lyrics_failed.remove(&video_id);
                            LyricsState::Ready(lyrics)
                        }
                        Err(error) => {
                            log::info!("letra: {error}");
                            let tries = self.lyrics_failed.get(&video_id).map_or(0, |f| f.1);
                            self.lyrics_failed.insert(video_id.clone(), (Instant::now(), tries + 1));
                            LyricsState::Failed(error)
                        }
                    };
                    self.lyrics.insert(video_id, state);
                }
                Reply::Quiet => {}
                Reply::Translated { video_id, result } => {
                    self.translating.remove(&video_id);
                    match result {
                        // Só se a letra na tela ainda é a que foi traduzida
                        // (a pessoa pode ter trocado com "Outra letra").
                        Ok(song) => {
                            let same = matches!(self.lyrics.get(&video_id), Some(LyricsState::Ready(Some(now))) if crate::lyrics::fingerprint(now) == crate::lyrics::fingerprint(&song));
                            if same {
                                let (dir, id, keep) = (self.paths.lyrics.clone(), video_id.clone(), song.clone());
                                let _ = std::thread::Builder::new().name("letra-traduzida".into()).spawn(move || crate::lyrics::store(&dir, &id, Some(&keep)));
                                self.lyrics.insert(video_id, LyricsState::Ready(Some(song)));
                            }
                        }
                        Err(error) => {
                            log::info!("tradução da letra: {error}");
                            self.translate_failed.insert(video_id);
                        }
                    }
                }
                Reply::LyricAlternatives { video_id, result } => {
                    let waiting = self.lyric_alts_waiting.as_deref() == Some(video_id.as_str());
                    if waiting {
                        self.lyric_alts_waiting = None;
                    }
                    match result {
                        Ok(alts) => {
                            if self.lyric_alts.len() >= 20 {
                                self.lyric_alts.clear();
                            }
                            self.lyric_alts.insert(video_id.clone(), alts);
                            if waiting {
                                self.other_lyrics(ctx, video_id);
                            }
                        }
                        Err(error) => {
                            log::info!("outras letras: {error}");
                            self.say(ctx, t().lyrics_no_other);
                        }
                    }
                }
                Reply::Art { video_id, path } => {
                    if self.snap.current().is_some_and(|t| t.video_id == video_id) && path.exists() {
                        self.art = Some((video_id, path));
                    }
                }
                Reply::Storage(measured) => {
                    if let Some(Dialog::Storage { usage, .. }) = self.dialog.as_mut() {
                        *usage = Some(measured);
                    }
                }
                Reply::CacheCleared { freed, usage: left } => {
                    if let Some(Dialog::Storage { usage, busy }) = self.dialog.as_mut() {
                        *usage = Some(left);
                        *busy = false;
                    }
                    // A capa da atual saiu do disco: os controles do Windows
                    // pedem de novo.
                    self.art_for = None;
                    self.art = None;
                    self.say(ctx, &t().cache_cleared(&crate::storage::human(freed)));
                }
                Reply::Version(version) => {
                    self.saved.version_checked = unix_now();
                    if !version.is_empty() {
                        log::info!("cliente WEB_REMIX {version}");
                        self.saved.client_version = Some(version);
                    }
                }
                Reply::TrackInfo { video_id, result } => match result {
                    Ok(watch) => {
                        if let Some(like) = watch.like {
                            // A escolha feita aqui vale mais que a resposta.
                            self.me.likes.entry(video_id.clone()).or_insert(like);
                        }
                        self.info.insert(video_id, TrackInfo { answered: true, lyrics_id: watch.lyrics_id, related_id: watch.related_id });
                    }
                    Err(error) => {
                        log::info!("dados da música: {error}");
                        self.info.insert(video_id, TrackInfo { answered: true, lyrics_id: None, related_id: None });
                    }
                },
                Reply::Related { video_id, result } => {
                    let state = match result {
                        Ok(page) => PageState { page: Some(Arc::new(page)), ..PageState::default() },
                        Err(error) => PageState { error: Some(error), ..PageState::default() },
                    };
                    self.related.insert(video_id, state);
                }
                // A pessoa trocou de conta enquanto a renovação corria: o que
                // veio é só da conta que pediu.
                Reply::Login { silent: true, slot, result } if self.me.active.as_ref() != Some(&slot) => {
                    if let Ok(Some(cookie)) = result {
                        crate::vault::save(&self.paths.session_of(&slot), &cookie);
                    }
                }
                Reply::Login { silent: true, result, .. } => match result {
                    Ok(Some(cookie)) => {
                        log::info!("sessão renovada sem janela");
                        self.start_session(ctx, &cookie, true);
                    }
                    Ok(None) => self.drop_session(ctx, t().session_expired),
                    Err(error) => {
                        log::warn!("renovação da sessão: {error}");
                        self.drop_session(ctx, t().session_expired);
                    }
                },
                Reply::Login { silent: false, slot, result } => {
                    self.me.signing_in = false;
                    let keep = matches!(result, Ok(Some(_))) || self.me.accounts.iter().any(|p| p.id == slot);
                    // A conta nova desistida não deixa perfil para trás.
                    if !keep && !slot.is_empty() {
                        let profile = self.paths.webview_of(&slot);
                        let _ = std::thread::Builder::new().name("sair".into()).spawn(move || crate::login::forget(&profile));
                    }
                    match result {
                        Ok(Some(cookie)) => {
                            self.enter_slot(&slot);
                            self.start_session(ctx, &cookie, false);
                        }
                        Ok(None) => self.say(ctx, t().login_cancelled),
                        Err(error) => {
                            log::warn!("login: {error}");
                            self.say(ctx, t().login_window_failed);
                        }
                    }
                }
                Reply::Account(result) => match result {
                    Ok(Session::Active(account)) => {
                        log::info!("conta conectada");
                        let active = self.me.active.clone().unwrap_or_default();
                        // Entrou de novo numa conta já guardada: fica a nova.
                        let repeated: Vec<String> = self.me.accounts.iter().filter(|p| p.id != active && p.same_as(&account)).map(|p| p.id.clone()).collect();
                        for id in repeated {
                            self.forget_account(&id);
                        }
                        if let Some(profile) = self.me.accounts.iter_mut().find(|p| p.id == active) {
                            profile.update(&account);
                        }
                        self.me.account = Some(account);
                        self.renew_tried = false;
                    }
                    // Vencida: primeiro tenta renovar pelo perfil guardado,
                    // sem janela; só se isso falhar a pessoa é deslogada.
                    Ok(Session::SignedOut) if self.me.signed_in && !self.renew_tried => {
                        log::info!("sessão vencida; renovando sem janela");
                        self.renew_tried = true;
                        let slot = self.me.active.clone().unwrap_or_default();
                        let profile = self.paths.webview_of(&slot);
                        self.jobs.sign_in(true, slot, profile);
                    }
                    Ok(Session::SignedOut) if self.me.signed_in => self.drop_session(ctx, t().session_expired),
                    Ok(Session::SignedOut) => {}
                    // O YouTube mudou a resposta: não é motivo para deslogar.
                    Ok(Session::Unknown) => log::warn!("conta: resposta não reconhecida; a sessão fica como está"),
                    // Sem rede não é sessão vencida: fica como está.
                    Err(error) => log::info!("conta: {error}"),
                },
                Reply::Playlists(result) => match result {
                    Ok(list) => self.me.playlists = list,
                    Err(error) => log::info!("playlists da conta: {error}"),
                },
                Reply::Done { message, result, refresh } => match result {
                    Ok(()) => {
                        self.say(ctx, &message);
                        match refresh {
                            Refresh::Nothing => {}
                            Refresh::Playlists => {
                                self.jobs.playlists();
                                self.pages.remove(&View::Library(crate::ui::LibraryTab::Playlists));
                            }
                            Refresh::Page(view) => {
                                self.pages.remove(&view);
                                if *self.view() == view {
                                    self.ensure_loaded(&view);
                                }
                            }
                        }
                    }
                    Err(error) => {
                        log::warn!("ação da conta: {error}");
                        self.say(ctx, &t().action_failed(&error));
                    }
                },
            }
        }
    }

    fn refresh_snapshot(&mut self, ctx: &egui::Context) {
        let rev = self.player.revision();
        if rev == self.snap_rev {
            return;
        }
        self.snap_rev = rev;
        self.snap = self.player.snapshot();
        if let Some((id, text)) = self.snap.notice.clone()
            && id != self.last_notice
        {
            self.last_notice = id;
            self.say(ctx, &text);
        }
        let current = self.snap.current().cloned();
        if self.me.signed_in
            && self.snap.status == Status::Playing
            && let Some(track) = &current
            && self.history_for.as_deref() != Some(&track.video_id)
        {
            self.history_for = Some(track.video_id.clone());
            self.jobs.add_history(track.video_id.clone());
        }
        // A semente do rádio chega sem artistas; a fila do YouTube os traz
        // depois, para a mesma música. Por isso compara o título pronto.
        let title = match &current {
            Some(t) if t.artists.is_empty() => format!("{} · MusicFast", t.title),
            Some(t) => format!("{} – {} · MusicFast", t.title, t.artist_names()),
            None => "MusicFast".to_owned(),
        };
        if self.title_for.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title_for = Some(title);
        }
        // A capa da atual vai para o disco, para os controles do Windows.
        if let Some(track) = &current
            && self.art_for.as_deref() != Some(&track.video_id)
        {
            self.art_for = Some(track.video_id.clone());
            self.art = None;
            if let Some(url) = track.thumbs.url_for(544) {
                let path = self.paths.art.join(format!("{}.jpg", track.video_id));
                if path.exists() {
                    self.art = Some((track.video_id.clone(), path));
                } else {
                    // A faxina da pasta (ler e apagar arquivos) vai junto
                    // com o download, fora da thread da janela.
                    self.jobs.art(track.video_id.clone(), url, path);
                }
            }
        }
    }

    fn handle_wake(&mut self, ctx: &egui::Context) {
        for request in self.wake.take() {
            match request.as_str() {
                "show" => raise(ctx),
                "toggle" => self.player.send(Command::Toggle),
                "next" => self.player.send(Command::Next),
                "previous" => self.player.send(Command::Previous),
                "quit" => ctx.send_viewport_cmd(ViewportCommand::Close),
                _ => {}
            }
        }
    }

    /// Os controles de mídia do Windows (a sobreposição do volume, as teclas
    /// de mídia do teclado e do fone).
    fn media_controls(&mut self, ctx: &egui::Context) {
        let Some(controls) = self.controls.as_mut() else { return };
        for command in controls.commands() {
            let cmd = match command {
                np::Command::Play => Some(Command::Resume),
                np::Command::Pause | np::Command::Stop => Some(Command::Pause),
                np::Command::PlayPause => Some(Command::Toggle),
                np::Command::Next => Some(Command::Next),
                np::Command::Previous => Some(Command::Previous),
                np::Command::SeekBy(ms) => Some(Command::SeekBy(ms as f64 / 1000.0)),
                np::Command::SetPosition { position, .. } => Some(Command::Seek(position.as_secs_f64())),
                np::Command::SetVolume(v) => Some(Command::SetVolume(v as f32)),
                np::Command::SetShuffle(on) => Some(Command::SetShuffle(on)),
                np::Command::SetRepeat(r) => Some(Command::SetRepeat(match r {
                    np::Repeat::Off => Repeat::Off,
                    np::Repeat::Track => Repeat::One,
                    np::Repeat::Playlist => Repeat::All,
                })),
                np::Command::Raise => {
                    raise(ctx);
                    None
                }
                np::Command::Quit => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    None
                }
                np::Command::OpenUri(_) => None,
            };
            if let Some(cmd) = cmd {
                self.player.send(cmd);
            }
        }
        let position = self.player.position();
        // O estado só é remontado (cópias do título, artistas, endereço)
        // quando algo mudou, ou uma vez por segundo para a posição: antes ia
        // a cada quadro, inclusive nos 60 por segundo de uma rolagem. O
        // `update` já só repassava a posição uma vez por segundo.
        let has_art = self.art.is_some();
        let (rev, art_sent, at) = self.controls_sent;
        let (last, last_at) = self.last_position;
        let expected = if self.snap.status == Status::Playing { last + last_at.elapsed().as_secs_f64() } else { last };
        let jumped = (position - expected).abs() > 1.5;
        if rev == self.snap_rev && art_sent == has_art && !jumped && at.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.controls_sent = (self.snap_rev, has_art, Instant::now());
        let snap = &self.snap;
        let playback = match snap.status {
            Status::Playing => np::Playback::Playing,
            Status::Paused | Status::Loading if snap.current().is_some() => np::Playback::Paused,
            _ => np::Playback::Stopped,
        };
        let track = snap.current().map(|t| np::Track {
            id: t.video_id.clone(),
            title: t.title.clone(),
            artists: t.artists.iter().map(|a| a.name.clone()).collect(),
            album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            duration: snap.duration.or(t.duration_secs.map(f64::from)).map(Duration::from_secs_f64),
            art_file: self.art.as_ref().filter(|(id, _)| *id == t.video_id).map(|(_, p)| p.clone()),
            art_url: t.thumbs.url_for(544),
            url: Some(format!("https://music.youtube.com/watch?v={}", t.video_id)),
            ..np::Track::default()
        });
        let has_track = track.is_some();
        controls.update(np::State {
            playback,
            track,
            position: Duration::from_secs_f64(position.max(0.0)),
            volume: Some(snap.volume as f64),
            shuffle: Some(snap.shuffle),
            repeat: Some(match snap.repeat {
                Repeat::Off => np::Repeat::Off,
                Repeat::One => np::Repeat::Track,
                Repeat::All => np::Repeat::Playlist,
            }),
            controls: np::Controls { seek: has_track, ..np::Controls::default() },
        });
        // Um salto (busca na linha do tempo) avisa, para os controles pularem
        // em vez de deslizar.
        if has_track && jumped {
            controls.seeked(Duration::from_secs_f64(position.max(0.0)));
        }
        self.last_position = (position, Instant::now());
    }

    /// O que a música atual precisa da rede: a curtida da conta, e a letra e
    /// os relacionados quando estão na tela.
    fn track_wanted(&mut self) {
        let Some(track) = self.snap.current() else { return };
        let id = track.video_id.clone();
        // A semente ainda sem fila não tem id de verdade para pedir.
        if id.is_empty() {
            return;
        }
        let Some(info) = self.info.get(&id) else {
            // Uma entrada por música tocada: sem teto, a sessão de um dia
            // inteiro de rádio só cresceria (OTIMIZACAO.md, lei 8).
            if self.info.len() >= 300 {
                self.info.retain(|k, _| *k == id);
                self.lyrics.retain(|k, _| *k == id);
                self.lyrics_failed.retain(|k, _| *k == id);
            }
            if self.related.len() >= 12 {
                self.related.retain(|k, _| *k == id);
            }
            self.info.insert(id.clone(), TrackInfo { answered: false, lyrics_id: None, related_id: None });
            self.jobs.track_info(id);
            return;
        };
        let lyrics_shown = self.side == Some(Side::Lyrics)
            || (self.expanded && self.player_tab == Tab::Lyrics)
            || self.mini.as_ref().is_some_and(|m| m.lyrics);
        // A que falhou por erro (o LRCLIB vive ocupado) é pedida de novo
        // depois de um tempo, até três vezes.
        if lyrics_shown
            && let Some((at, tries)) = self.lyrics_failed.get(&id)
            && *tries < 3
            && at.elapsed() > Duration::from_secs(20)
            && matches!(self.lyrics.get(&id), Some(LyricsState::Failed(_)))
        {
            self.lyrics.remove(&id);
        }
        // Espera a resposta da fila: a letra do YouTube Music (a reserva)
        // vem com o id dela.
        if lyrics_shown && !self.lyrics.contains_key(&id) && info.answered {
            let (track, lyrics_id) = (track.clone(), info.lyrics_id.clone());
            self.lyrics.insert(id.clone(), LyricsState::Loading);
            self.jobs.lyrics(id.clone(), track, lyrics_id, self.paths.lyrics.clone());
        }
        // A tradução, quando ligada e a letra ainda não foi ao tradutor (ou
        // foi para outro idioma do app). A marca do idioma é montada só aqui:
        // antes eram duas `String` por quadro, com ou sem letra na tela.
        if lyrics_shown
            && self.translate_lyrics
            && !self.translating.contains(&id)
            && !self.translate_failed.contains(&id)
            && let Some(LyricsState::Ready(Some(song))) = self.lyrics.get(&id)
            && !song.lines.is_empty()
            && song.translated_to.as_deref() != Some(self.look.lyrics.translation_key().as_str())
        {
            self.translating.insert(id.clone());
            self.jobs.translate_lyrics(id.clone(), song.clone(), self.look.lyrics.target(), self.look.lyrics.service, self.look.lyrics.local_model.clone());
        }
        // Com a letra na tela e a da atual pronta, a da próxima vem adiantada.
        if lyrics_shown
            && matches!(self.lyrics.get(&id), Some(LyricsState::Ready(_)))
            && let Some(next) = self.snap.index.and_then(|i| self.snap.queue.get(i + 1))
            && !next.video_id.is_empty()
            && !self.lyrics.contains_key(&next.video_id)
            && self.lyrics_prefetched.as_deref() != Some(next.video_id.as_str())
        {
            let next = next.clone();
            self.lyrics_prefetched = Some(next.video_id.clone());
            self.jobs.prefetch_lyrics(next.video_id.clone(), next, self.paths.lyrics.clone());
        }
        let info = &self.info[&id];
        if self.expanded
            && self.player_tab == Tab::Related
            && !self.related.contains_key(&id)
            && let Some(related_id) = info.related_id.clone()
        {
            self.related.insert(id.clone(), PageState::default());
            self.jobs.related(id, related_id);
        }
    }

    fn session(&self) -> Saved {
        Saved { queue: self.snap.queue.as_ref().clone(), ..self.light_session() }
    }

    /// A sessão sem a fila.
    fn light_session(&self) -> Saved {
        let snap = &self.snap;
        Saved {
            volume: snap.volume,
            shuffle: snap.shuffle,
            repeat: snap.repeat,
            queue: Vec::new(),
            index: snap.index,
            position: if snap.current().is_some() { self.player.position() } else { 0.0 },
            visitor: self.client.visitor(),
            client_version: self.saved.client_version.clone(),
            version_checked: self.saved.version_checked,
            side: self.side.map(|s| s.key().to_owned()),
            sidebar_mini: self.sidebar_mini,
            sidebar_width: self.sidebar_width,
            mini_anchor: self.mini_anchor,
            window: self.window,
            language: self.language.map(|l| l.code().to_owned()),
            look: self.look.clone(),
            translate_lyrics: self.translate_lyrics,
            downloads_dir: self.offline.has_chosen_dir().then(|| self.offline.dir()),
            shell: self.shell,
            accounts: self.me.accounts.clone(),
            active_account: self.me.active.clone(),
            bar: self.bar.layouts.clone(),
            discord: self.discord_settings.clone(),
        }
    }

    /// Lembra a janela só enquanto ela está normal: minimizada, o Windows a
    /// põe em -32000,-32000 com 0×0, e maximizada o tamanho é o da tela (o
    /// de antes de maximizar fica guardado).
    fn track_window(&mut self, ctx: &egui::Context) {
        // A janela pequena do mini player não é "a janela normal".
        if self.mini.is_some() {
            return;
        }
        let (minimized, maximized, outer, inner) = ctx.input(|i| {
            let v = i.viewport();
            (v.minimized.unwrap_or(false), v.maximized.unwrap_or(false), v.outer_rect, v.inner_rect)
        });
        if minimized {
            return;
        }
        if maximized {
            if let Some(w) = self.window.as_mut() {
                w.maximized = true;
            }
            return;
        }
        let (Some(outer), Some(inner)) = (outer, inner) else { return };
        let geom = crate::state::WindowGeom {
            x: outer.min.x,
            y: outer.min.y,
            width: inner.width(),
            height: inner.height(),
            maximized: false,
        };
        if geom.is_sane() {
            self.window = Some(geom);
        }
    }

    /// A janela vira o mini player: sem bordas, sempre no topo, no canto
    /// onde ele ficou da última vez (ou no pé direito da janela atual).
    fn enter_mini(&mut self, ctx: &egui::Context) {
        if self.mini.is_some() {
            return;
        }
        let (maximized, outer) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().outer_rect));
        self.track_window(ctx);
        self.mini = Some(Mini { lyrics: false, was_maximized: maximized });
        let scale = self.look.mini_layout.scale();
        self.expanded = false;
        let anchor = self.mini_anchor.unwrap_or_else(|| match outer {
            Some(r) => [r.right() - mini::width(scale) - 24.0, r.bottom() - 24.0],
            None => [100.0, 700.0],
        });
        self.mini_anchor = Some(anchor);
        if maximized {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
        }
        ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(mini::width(scale), mini::row_height(scale))));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        self.place_mini(ctx);
        self.round_corners = Some(true);
    }

    /// Tamanho e lugar do mini player, com o pé em `mini_anchor`.
    fn place_mini(&self, ctx: &egui::Context) {
        let (Some(mini), Some([x, bottom])) = (&self.mini, self.mini_anchor) else { return };
        let scale = self.look.mini_layout.scale();
        let h = mini::height(mini.lyrics, scale);
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(mini::width(scale), h)));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, bottom - h)));
    }

    /// Volta à janela inteira, no tamanho e lugar de antes.
    fn leave_mini(&mut self, ctx: &egui::Context) {
        let Some(mini) = self.mini.take() else { return };
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(egui::Vec2::from(crate::state::WindowGeom::MIN_SIZE)));
        match self.window {
            Some(g) => {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(g.width, g.height)));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(g.x, g.y)));
            }
            None => ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1240.0, 800.0))),
        }
        if mini.was_maximized {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(true));
        }
        self.round_corners = Some(false);
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// O quadro do mini player: só ele, sem a página por trás.
    fn mini_ui(&mut self, ui: &mut Ui, mut actions: Vec<Action>, position: f64) {
        let ctx = ui.ctx().clone();
        let mini_latency = self.player.latency();
        let Some(lyrics_open) = self.mini.as_ref().map(|m| m.lyrics) else { return };
        // Guarda onde a pessoa o arrastou (quando a janela já tem o tamanho
        // pedido: no meio de uma troca, o retângulo ainda é o de antes).
        if let Some(r) = ctx.input(|i| i.viewport().outer_rect)
            && (r.height() - mini::height(lyrics_open, self.look.mini_layout.scale())).abs() < 2.0
            && !ctx.input(|i| i.viewport().minimized.unwrap_or(false))
        {
            self.mini_anchor = Some([r.left(), r.bottom()]);
        }
        let current = self.snap.current().map(|t| t.video_id.clone());
        let playing = self.snap.status == Status::Playing;
        let lyrics = lyrics_open.then(|| current.as_ref().and_then(|id| self.lyrics.get(id)));
        let mut out = mini::Out::default();
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency: mini_latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
            let skin = &self.look.mini;
            let backdrop = if *skin == crate::look::Skin::default() {
                None
            } else {
                let cover = self.snap.current().filter(|_| skin.wants_cover()).and_then(|t| cx.covers.color(&t.thumbs));
                Some((cover, skin.viz.map(|_| (&self.analyzer.bands, &self.analyzer.wave))))
            };
            let images = &self.backdrops;
            let paint = |ui: &Ui, rect: egui::Rect| {
                if let Some((cover, sound)) = backdrop {
                    let surface = crate::look::Surface::Mini;
                    crate::look::paint(ui, rect, skin, images.get(surface), &crate::look::Now { cover, sound }, surface.base());
                }
            };
            out = mini::show(ui, &mut cx, &self.snap, position, lyrics, backdrop.is_some().then_some(&paint as &dyn Fn(&Ui, egui::Rect)), &self.look.mini_layout);
        });
        if out.toggle_lyrics
            && let Some(m) = self.mini.as_mut()
        {
            m.lyrics = !m.lyrics;
            self.place_mini(&ctx);
        }
        for action in actions {
            self.apply(&ctx, action);
        }
        if playing || self.snap.status == Status::Loading {
            // O fio do progresso anda.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn autosave(&mut self) {
        // Antes do primeiro `Snapshot`, a fila ainda é a restaurada.
        if self.snap_rev == u64::MAX || self.saved_at.elapsed() < Duration::from_secs(15) {
            return;
        }
        self.saved_at = Instant::now();
        let session = self.light_session();
        if session != self.saved || !Arc::ptr_eq(&self.saved_queue, &self.snap.queue) {
            self.saved_queue = self.snap.queue.clone();
            session.clone().save_in_background(self.snap.queue.clone(), &self.paths.state);
            self.saved = session;
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let typing = ctx.memory(|m| m.focused().is_some());
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND, Key::F) || (!typing && i.consume_key(Modifiers::NONE, Key::Slash)) {
                self.search.focus = true;
            }
            if i.consume_key(Modifiers::ALT, Key::ArrowLeft) || i.pointer.button_pressed(PointerButton::Extra1) {
                actions.push(Action::Back);
            }
            if i.consume_key(Modifiers::ALT, Key::ArrowRight) || i.pointer.button_pressed(PointerButton::Extra2) {
                actions.push(Action::Forward);
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::M) {
                actions.push(Action::Mini(self.mini.is_none()));
            }
            if typing {
                return;
            }
            if i.consume_key(Modifiers::NONE, Key::Space) {
                actions.push(Action::Cmd(Command::Toggle));
            }
            if i.consume_key(Modifiers::COMMAND, Key::ArrowRight) {
                actions.push(Action::Cmd(Command::Next));
            }
            if i.consume_key(Modifiers::COMMAND, Key::ArrowLeft) {
                actions.push(Action::Cmd(Command::Previous));
            }
            if i.consume_key(Modifiers::SHIFT, Key::ArrowRight) {
                actions.push(Action::Cmd(Command::SeekBy(10.0)));
            }
            if i.consume_key(Modifiers::SHIFT, Key::ArrowLeft) {
                actions.push(Action::Cmd(Command::SeekBy(-10.0)));
            }
            let volume = self.snap.volume;
            if i.consume_key(Modifiers::COMMAND, Key::ArrowUp) {
                actions.push(Action::Cmd(Command::SetVolume((volume + 0.05).min(1.0))));
            }
            if i.consume_key(Modifiers::COMMAND, Key::ArrowDown) {
                actions.push(Action::Cmd(Command::SetVolume((volume - 0.05).max(0.0))));
            }
        });
    }

    fn paint_toast(&mut self, ctx: &egui::Context) {
        let Some((text, until)) = &self.toast else { return };
        let now = ctx.input(|i| i.time);
        if now > *until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_secs_f64((until - now).max(0.05)));
        let screen = ctx.content_rect();
        let pos = egui::pos2(screen.center().x, screen.bottom() - bar::HEIGHT - 24.0);
        egui::Area::new(egui::Id::new("aviso"))
            .order(egui::Order::Tooltip)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(pos)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::TEXT)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(16, 10))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(text.as_str()).color(egui::Color32::BLACK).font(theme::medium(14.0)));
                    });
            });
    }
}

/// A biblioteca sem conta: o convite para entrar.
fn sign_in_prompt(ui: &mut Ui, cx: &mut Cx) {
    ui.add_space(80.0);
    ui.vertical_centered(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
        crate::ui::icons::paint(ui.painter(), r, crate::ui::icons::LIBRARY, theme::TEXT_DIM);
        ui.add_space(12.0);
        ui.label(egui::RichText::new(t().sign_in_prompt_title).font(theme::bold(22.0)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(t().sign_in_prompt_body).color(theme::TEXT_DIM));
        ui.add_space(16.0);
        let label = if cx.me.signing_in { t().signing_in } else { t().sign_in };
        if kit::pill(ui, Some(crate::ui::icons::LOG_IN), label, true).clicked() && !cx.me.signing_in {
            cx.act(Action::SignIn);
        }
    });
}

/// O degradê do fundo, como o do YouTube Music: azul-violeta escuro no alto
/// à esquerda, vinho à direita, quase preto embaixo. Uma malha de 3×3
/// vértices que a GPU interpola; custa 8 triângulos por quadro.
/// `dark` (0..=1) leva tudo para o preto do tema (o player aberto).
fn background(ctx: &egui::Context, dark: f32) {
    use egui::{Color32, Mesh, pos2};
    let r = ctx.content_rect();
    let grid: [[Color32; 3]; 3] = [
        [Color32::from_rgb(18, 13, 34), Color32::from_rgb(14, 8, 24), Color32::from_rgb(38, 7, 24)],
        [Color32::from_rgb(9, 7, 16), Color32::from_rgb(7, 5, 10), Color32::from_rgb(20, 5, 13)],
        [Color32::from_rgb(4, 4, 6), Color32::from_rgb(3, 3, 4), Color32::from_rgb(6, 3, 5)],
    ];
    let mut mesh = Mesh::default();
    for (row, colors) in grid.iter().enumerate() {
        for (col, color) in colors.iter().enumerate() {
            let p = pos2(r.left() + r.width() * col as f32 / 2.0, r.top() + r.height() * row as f32 / 2.0);
            mesh.colored_vertex(p, color.lerp_to_gamma(theme::BG, dark));
        }
    }
    for row in 0..2u32 {
        for col in 0..2u32 {
            let i = row * 3 + col;
            mesh.add_triangle(i, i + 1, i + 3);
            mesh.add_triangle(i + 1, i + 4, i + 3);
        }
    }
    ctx.layer_painter(egui::LayerId::background()).add(mesh);
}

/// O HWND da janela, no Windows (o arrasto de arquivo para fora precisa).
fn window_handle(frame: &eframe::Frame) -> Option<isize> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = frame.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = frame;
        None
    }
}

/// Cantos redondos (o mini player) ou os do sistema, pelo DWM do Windows 11.
/// Noutros sistemas, e no Windows 10, não faz nada.
fn round_corners(frame: &eframe::Frame, round: bool) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DEFAULT, DWMWCP_ROUND, DwmSetWindowAttribute};
        let Ok(handle) = frame.window_handle() else { return };
        let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
        let preference: i32 = if round { DWMWCP_ROUND } else { DWMWCP_DEFAULT };
        // SAFETY: o HWND é o da janela viva deste quadro, e o atributo é um
        // inteiro de 4 bytes, como o DWM pede.
        unsafe {
            DwmSetWindowAttribute(
                h.hwnd.get() as _,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&preference as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            );
        }
    }
    #[cfg(not(windows))]
    let _ = (frame, round);
}

/// O aviso de versão nova na barra de cima, à esquerda da foto da conta:
/// "Reiniciar para atualizar" quando já baixou, "Baixar versão nova" quando
/// esta cópia não se troca sozinha, e o progresso enquanto baixa.
fn update_badge(ui: &mut Ui, cx: &mut Cx, top: egui::Rect, state: &crate::update::State) {
    use crate::update::State;
    use egui::{CursorIcon, Id, Rect, pos2};
    let (label, tip) = match state {
        State::Idle => return,
        State::Downloading { version, progress } => (t().downloading_percent(*progress), t().downloading_version(version)),
        State::Ready { version } => (t().restart_to_update.to_owned(), t().version_ready(version)),
        State::Manual { version, reason, .. } => (t().new_version.to_owned(), t().version_out(version, reason)),
    };
    let font = theme::semibold(13.0);
    let width = ui.fonts_mut(|f| f.layout_no_wrap(label.clone(), font.clone(), theme::TEXT).size().x) + 28.0;
    let quick = nav::quick_accounts_width(cx.me, top.width());
    let rect = Rect::from_center_size(pos2(top.right() - 76.0 - quick - width / 2.0, top.center().y), vec2(width, 32.0));
    let clickable = !matches!(state, State::Downloading { .. });
    let resp = ui.interact(rect, Id::new("atualizacao"), if clickable { Sense::click() } else { Sense::hover() });
    let fill = match (state, resp.hovered()) {
        (State::Downloading { .. }, _) => theme::GLASS_HOVER,
        (_, true) => egui::Color32::from_rgb(62, 166, 255),
        _ => egui::Color32::from_rgb(40, 140, 235),
    };
    ui.painter().rect_filled(rect, CornerRadius::same(16), fill);
    if let State::Downloading { progress, .. } = state {
        let done = Rect::from_min_size(rect.min, vec2(rect.width() * progress.clamp(0.0, 1.0), rect.height()));
        ui.painter().with_clip_rect(done).rect_filled(rect, CornerRadius::same(16), egui::Color32::from_rgba_unmultiplied(40, 140, 235, 160));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, theme::TEXT);
    let resp = resp.on_hover_text(tip);
    if clickable && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        match state {
            State::Ready { .. } => cx.act(Action::RestartToUpdate),
            State::Manual { url, .. } => kit::open_in_browser(url),
            _ => {}
        }
    }
}

/// Traz a janela para a frente (outra execução, ou os controles de mídia).
fn raise(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
}

/// As capas guardadas para os controles são uma por música; ficam as 20
/// mais novas. Roda na thread do download (`Jobs::art`).
pub fn clean_art(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= 20 {
        return;
    }
    files.sort_unstable_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in files.into_iter().skip(20) {
        let _ = std::fs::remove_file(path);
    }
}

impl eframe::App for MusicApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_wake(ctx);
        self.handle_replies(ctx);
        if self.me.signed_in && self.account_checked.elapsed() > Duration::from_secs(6 * 3600) {
            self.account_checked = Instant::now();
            self.jobs.account();
        }
        self.refresh_snapshot(ctx);
        self.media_controls(ctx);
        self.discord_presence();
        self.track_window(ctx);
        self.track_wanted();
        self.autosave();
        // Compilações de desenvolvimento não perguntam: não vieram da página
        // de releases e não teriam o que fazer com a resposta.
        if !cfg!(debug_assertions) && Instant::now() >= self.next_update_check {
            self.next_update_check = Instant::now() + fastframe_update::CHECK_INTERVAL;
            self.updates.check();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.first_frame_logged {
            // O instrumento do boot (DOCUMENTACAO_PERFORMANCE.md): o que a
            // pessoa espera entre abrir e ver a janela desenhada.
            self.first_frame_logged = true;
            if let Some(started) = crate::STARTED.get() {
                log::info!("primeiro quadro em {} ms desde o início do processo", started.elapsed().as_millis());
            }
            // A versão nova abriu: o ajudante para de esperar e apaga o backup.
            if let Some(receipt) = self.update_receipt.take() {
                let _ = std::thread::Builder::new().name("recibo-atualizacao".into()).spawn(move || {
                    match receipt.acknowledge() {
                        Ok(()) => log::info!("atualizado para {}", env!("CARGO_PKG_VERSION")),
                        Err(error) => log::warn!("recibo da atualização: {error:#}"),
                    }
                });
                self.say(&ctx, &t().updated_to(env!("CARGO_PKG_VERSION")));
            }
            if let Some(error) = self.toast_on_start.take() {
                log::warn!("atualização desfeita: {error}");
                self.say(&ctx, t().update_rolled_back);
            }
        }
        if let Some(error) = self.backdrops.poll(&ctx) {
            self.say(&ctx, &t().image_load_failed(&error));
        }
        if let Some(picked) = self.picking_folder.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.picking_folder = None;
            if let Some(dir) = picked {
                self.set_downloads_dir(&ctx, Some(dir));
            }
        }
        if let Some((surface, picked)) = self.picking.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.picking = None;
            if let Some(path) = picked {
                self.adopt_backdrop(&ctx, surface, &path);
            }
        }
        // Arrastar uma imagem para a janela com o "Personalizar" aberto.
        if let Some(Dialog::Look { surface, .. }) = &self.dialog {
            let surface = *surface;
            let dropped = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).find(|p| !p.as_os_str().is_empty()));
            if let Some(path) = dropped {
                self.adopt_backdrop(&ctx, surface, &path);
            }
        }
        // O visualizador: o som de agora, uma vez por quadro, só se usado.
        let viz_on = match self.mini {
            Some(_) => self.look.mini.viz.is_some(),
            None => {
                self.look.player.viz.is_some() && self.expanded
                    || self.look.bar.viz.is_some()
                    || self.look.home.viz.is_some() && !self.expanded
                    || matches!(self.dialog, Some(Dialog::Look { .. }))
            }
        };
        if viz_on {
            self.analyzer.update(&self.player, ctx.input(|i| i.time));
            if self.snap.status == Status::Playing || self.analyzer.moving() {
                ctx.request_repaint_after(crate::viz::FRAME);
            }
        }
        crate::look::ensure_lyrics_fonts(&ctx, &self.look.lyrics, &mut self.lyrics_fonts);
        crate::fonts::drop_previews_when_closed(&ctx);
        self.scrolling.apply(&ctx);
        self.covers.begin_frame(&ctx);
        let mut actions = Vec::new();
        self.shortcuts(&ctx, &mut actions);
        let position = self.player.position();
        let latency = self.player.latency();
        if let Some(round) = self.round_corners.take() {
            round_corners(_frame, round);
        }
        if self.window_handle.is_none() {
            self.window_handle = window_handle(_frame);
        }
        let revision = self.offline.revision();
        if self.download_tracks.0 != revision {
            self.download_tracks = (revision, Arc::new(self.offline.tracks()));
        }
        if self.mini.is_some() {
            self.mini_ui(ui, actions, position);
            return;
        }
        let mut suggest = None;
        let view = self.view().clone();
        let current = self.snap.current().map(|t| t.video_id.clone());
        let playing = self.snap.status == Status::Playing;
        let (can_back, can_forward) = (self.at > 0, self.at + 1 < self.history.len());
        if self.expanded && self.snap.current().is_none() {
            self.expanded = false;
        }
        let expanded = self.expanded;
        // O fundo do site: um degradê escuro atrás da janela inteira, e a
        // coluna, a barra de cima e a página transparentes por cima dele.
        // Com o player aberto o degradê some para o preto, como no site.
        let dark = ctx.animate_bool_with_time_and_easing(egui::Id::new("player-aberto"), expanded, 0.3, egui::emath::easing::cubic_out);
        // A tela principal personalizada no lugar do degradê do site.
        let home = &self.look.home;
        let panels = self.shell == crate::ui::Shell::Panels;
        if panels {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(ctx.content_rect(), 0, egui::Color32::BLACK);
        } else if *home == crate::look::Skin::default() {
            background(&ctx, dark);
        } else {
            let cover = self.snap.current().filter(|_| home.wants_cover()).and_then(|t| self.covers.color(&t.thumbs));
            let now = crate::look::Now { cover, sound: home.viz.map(|_| (&self.analyzer.bands, &self.analyzer.wave)) };
            let surface = crate::look::Surface::Home;
            let painter = ctx.layer_painter(egui::LayerId::background());
            crate::look::paint_on(&ctx, &painter, ctx.content_rect(), home, self.backdrops.get(surface), &now, surface.base());
        }

        // Nos painéis a barra de cima é a largura toda da janela.
        if panels {
            egui::Panel::top("topo-paineis").exact_size(crate::ui::panels::TOP).resizable(false).show_separator_line(false).frame(Frame::NONE).show(ui, |ui| {
                let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
                let top = ui.max_rect();
                crate::ui::panels::top(ui, &mut cx, &view, &mut self.search, can_back, can_forward, &mut suggest);
                update_badge(ui, &mut cx, top, &self.updates.state());
            });
        }
        let (bar_height, bar_frame) = if panels { (bar::PANELS_HEIGHT, Frame::NONE) } else { (bar::HEIGHT, Frame::new().fill(bar::BG)) };
        egui::Panel::bottom("barra")
            .exact_size(bar_height).show_separator_line(!panels)
            .resizable(false)
            .frame(bar_frame)
            .show(ui, |ui| {
                let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
                let skin = &self.look.bar;
                if *skin != crate::look::Skin::default() {
                    let cover = self.snap.current().filter(|_| skin.wants_cover()).and_then(|t| cx.covers.color(&t.thumbs));
                    let sound = skin.viz.map(|_| (&self.analyzer.bands, &self.analyzer.wave));
                    let now = crate::look::Now { cover, sound };
                    let surface = crate::look::Surface::Bar;
                    crate::look::paint(ui, ui.max_rect(), skin, self.backdrops.get(surface), &now, surface.base());
                }
                if panels {
                    bar::show_panels(ui, &mut cx, &self.snap, position, self.side, expanded, &mut self.bar);
                } else {
                    bar::show(ui, &mut cx, &self.snap, position, self.side, expanded, &mut self.bar);
                }
            });
        // Aberta, a coluna se ajusta arrastando a borda direita; recolhida, é
        // fixa. Ids diferentes para a largura da recolhida não vazar para a
        // aberta na memória do painel.
        let mini = self.sidebar_mini;
        let gap = crate::ui::panels::GAP;
        let panel = if panels && mini {
            egui::Panel::left("biblioteca-mini").exact_size(crate::ui::panels::LIBRARY_MINI).resizable(false).show_separator_line(false)
        } else if panels {
            egui::Panel::left("biblioteca").default_size(crate::ui::panels::LIBRARY).size_range(crate::ui::panels::LIBRARY_RANGE).resizable(true).show_separator_line(false)
        } else if mini {
            egui::Panel::left("navegacao-mini").exact_size(nav::SIDEBAR_MINI).resizable(false)
        } else {
            egui::Panel::left("navegacao")
                .default_size(self.sidebar_width)
                .size_range(nav::SIDEBAR_RANGE)
                .resizable(true)
        };
        let panel_frame = if panels {
            crate::ui::panels::frame(egui::Margin { left: gap, right: gap / 2, top: 0, bottom: 0 })
        } else {
            Frame::new().fill(egui::Color32::from_black_alpha(70))
        };
        let shown = panel.frame(panel_frame).show(ui, |ui| {
            let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
            if panels {
                let tab = self.library.tab();
                let shelf = self.pages.get(&View::Library(tab)).and_then(|s| s.page.as_deref());
                crate::ui::panels::library(ui, &mut cx, &view, mini, &mut self.library, shelf);
            } else {
                nav::sidebar(ui, &mut cx, &view, mini);
            }
        });
        if !mini && !panels {
            self.sidebar_width = nav::sidebar_width(shown.response.rect.width());
        }
        // Com o player aberto, a fila e a letra já estão nas abas dele.
        if let Some(which) = self.side.filter(|_| !expanded) {
            egui::Panel::right("lado")
                .default_size(360.0)
                .size_range(280.0..=520.0)
                .show_separator_line(!panels)
                .resizable(true)
                .frame(if panels {
                    crate::ui::panels::frame(egui::Margin { left: gap / 2, right: gap, top: 0, bottom: 0 }).inner_margin(egui::Margin::symmetric(14, 0))
                } else {
                    Frame::new().fill(egui::Color32::from_black_alpha(110)).inner_margin(egui::Margin::symmetric(14, 0))
                })
                .show(ui, |ui| {
                    let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
                    let card = ui.max_rect().expand2(vec2(14.0, 0.0));
                    if panels && let Some(tint) = self.snap.current().and_then(|t| cx.covers.color(&t.thumbs)) {
                        crate::ui::panels::top_glow(ui, card, tint, 300.0, 0.5);
                    }
                    match which {
                        Side::Queue => side::queue(ui, &mut cx, &self.snap),
                        Side::Lyrics => {
                            let state = self.snap.current().and_then(|t| self.lyrics.get(&t.video_id));
                            side::lyrics(ui, &mut cx, &self.snap, state, position);
                        }
                    }
                    if panels {
                        crate::ui::panels::round_corners(ui, card);
                    }
                });
        }
        let side_shown = self.side.is_some() && !expanded;
        let central = if panels {
            crate::ui::panels::frame(egui::Margin { left: gap / 2, right: if side_shown { gap / 2 } else { gap }, top: 0, bottom: 0 })
        } else {
            Frame::NONE
        };
        egui::CentralPanel::default().frame(central).show(ui, |ui| {
            let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
            let panel_rect = ui.max_rect();
            if panels {
                // A tela principal personalizada fica dentro do painel.
                let home = &self.look.home;
                if *home != crate::look::Skin::default() {
                    let cover = self.snap.current().filter(|_| home.wants_cover()).and_then(|t| cx.covers.color(&t.thumbs));
                    let now = crate::look::Now { cover, sound: home.viz.map(|_| (&self.analyzer.bands, &self.analyzer.wave)) };
                    let surface = crate::look::Surface::Home;
                    crate::look::paint(ui, panel_rect, home, self.backdrops.get(surface), &now, surface.base());
                } else if view != View::Downloads && self.pages.get(&view).and_then(|s| s.page.as_ref()).is_none_or(|p| p.header.is_none()) {
                    // As páginas com capa têm a cor dela no próprio cabeçalho.
                    let tint = self.snap.current().and_then(|t| cx.covers.color(&t.thumbs));
                    crate::ui::panels::page_glow(ui, panel_rect, tint);
                }
            } else {
                let (top, _) = ui.allocate_exact_size(vec2(ui.available_width(), nav::TOP), Sense::hover());
                let mut top_ui = ui.new_child(egui::UiBuilder::new().max_rect(top));
                nav::top(&mut top_ui, &mut cx, &mut self.search, can_back, can_forward, &mut suggest);
                update_badge(&mut top_ui, &mut cx, top, &self.updates.state());
            }
            // O player sobe de baixo por cima da página ao abrir, e desce ao
            // fechar; parado aberto, a página nem é desenhada.
            let shown = ctx.animate_bool_with_time_and_easing(egui::Id::new("player-aberto"), expanded, 0.3, egui::emath::easing::cubic_out);
            let rest = egui::Rect::from_min_max(egui::pos2(ui.max_rect().left(), ui.cursor().top()), ui.max_rect().max);
            if shown < 1.0 {
                let mut page_ui = ui.new_child(egui::UiBuilder::new().max_rect(rest));
                let ui = &mut page_ui;
                if panels && matches!(view, View::Search { .. } | View::Library(_)) {
                    ui.add_space(12.0);
                }
                if let View::Search { query, filter } = &view {
                    ui.add_space(4.0);
                    nav::filters(ui, &mut cx, query, *filter);
                    ui.add_space(4.0);
                }
                if let View::Library(tab) = &view {
                    ui.add_space(4.0);
                    nav::library_tabs(ui, &mut cx, *tab);
                    ui.add_space(4.0);
                }
                if view == View::Downloads {
                    let dir = self.offline.dir();
                    crate::ui::downloads::show(ui, &mut cx, &self.download_tracks.1, &dir, self.offline.has_chosen_dir(), &mut self.downloads_page);
                } else if view.needs_account() && !self.me.signed_in {
                    sign_in_prompt(ui, &mut cx);
                } else {
                    let empty = PageState::default();
                    let state = self.pages.get(&view).unwrap_or(&empty);
                    page::show(ui, &mut cx, &view, state);
                }
            }
            if shown > 0.0 {
                let sheet = rest.translate(vec2(0.0, (1.0 - shown) * rest.height()));
                let skin = &self.look.player;
                let cover = self.snap.current().filter(|_| skin.wants_cover()).and_then(|t| cx.covers.color(&t.thumbs));
                let sound = skin.viz.map(|_| (&self.analyzer.bands, &self.analyzer.wave));
                let now = crate::look::Now { cover, sound };
                let surface = crate::look::Surface::Player;
                crate::look::paint(ui, sheet.intersect(rest), skin, self.backdrops.get(surface), &now, surface.base());
                let mut sheet_ui = ui.new_child(egui::UiBuilder::new().max_rect(sheet));
                sheet_ui.set_clip_rect(rest.intersect(ui.clip_rect()));
                let id = current.clone().unwrap_or_default();
                player::show(&mut sheet_ui, &mut cx, &self.snap, position, &mut self.player_tab, self.lyrics.get(&id), self.related.get(&id));
            }
            if panels {
                crate::ui::panels::round_corners(ui, panel_rect);
            }
        });

        if let Some(open) = self.dialog.as_mut() {
            let mut cx = Cx { covers: &mut self.covers, actions: &mut actions, me: &self.me, language: self.language, latency, translate_lyrics: self.translate_lyrics, lyrics_style: &self.look.lyrics, current: current.clone(), playing, offline: Some(&*self.offline) };
            // As miniaturas mostram a capa e o som de agora.
            let cover = self.snap.current().and_then(|t| cx.covers.color(&t.thumbs));
            let now = crate::look::Now { cover, sound: Some((&self.analyzer.bands, &self.analyzer.wave)) };
            if dialog::show(&ctx, open, &mut cx, &self.backdrops, &now) {
                self.dialog = None;
            }
        } else if expanded && ctx.input(|i| i.key_pressed(Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
            actions.push(Action::Expand(false));
        }

        self.paint_toast(&ctx);
        if let Some(input) = suggest {
            self.jobs.suggestions(input);
        }
        for action in actions {
            self.apply(&ctx, action);
        }
        if playing || self.snap.status == Status::Loading {
            // O tempo anda: um quadro a cada quarto de segundo basta.
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        // O arrasto do arquivo terminou dentro do laço do Windows, que ficou
        // com o soltar do botão: sem este aviso o egui seguiria arrastando.
        if std::mem::take(&mut self.release_pointer) {
            let pos = ctx.input(|i| i.pointer.latest_pos()).unwrap_or_default();
            raw_input.events.push(egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.snap_rev != u64::MAX {
            self.session().save_now(&self.paths.state);
        }
        self.controls = None;
        self.player.shutdown();
    }
}
