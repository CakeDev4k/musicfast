//! A thread de controle: a fila, o rádio, o yt-dlp, os downloads e o
//! callback de áudio. A interface só manda `Command` e lê `Snapshot`.
//!
//! **Parado, não acorda.** Sem música, a thread dorme no `recv` sem prazo e
//! o dispositivo de áudio nem foi aberto (abre no primeiro tocar). Pausado,
//! o dispositivo fica sem callbacks (fastframe-audio) e esta thread acorda a
//! cada 10 s só até o fastframe soltar o dispositivo (5 min).
//!
//! **Rápido é resolver antes.** Resolver custa ~3 s (ver `ytdlp`); baixar,
//! ~0,2 s. Então a próxima da fila é resolvida, baixada e entregue ao
//! callback já decodificável enquanto a atual toca (troca sem intervalo), a
//! linha sob o mouse é resolvida antes do clique (`Warm`), e cada endereço
//! fica guardado em disco até vencer.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fastframe_audio::{Maintained, Output, OutputOptions};
use musicfast_ytm::{Client, Track, Watch};
use serde::{Deserialize, Serialize};

use crate::render::{Event, Loaded, RCmd, Renderer, Shared};
use crate::source::{Audio, Decoder, FetchError, fetch, http_client};
use crate::ytdlp::{self, Resolved, YtDlp, now};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Debug)]
pub enum Command {
    /// Tocar uma lista que a interface já tem (as faixas do álbum aberto).
    PlayTracks { tracks: Vec<Track>, index: usize },
    /// Tocar a música e montar o rádio dela, como o site faz.
    PlayRadio { seed: Track },
    /// Tocar uma playlist/álbum pela fila do YouTube (`next`).
    PlayList { seed: Option<Track>, playlist_id: String, params: Option<String> },
    /// A fila da sessão anterior, pausada na posição em que ficou.
    Restore { tracks: Vec<Track>, index: usize, position: f64 },
    Toggle,
    Pause,
    Resume,
    Next,
    Previous,
    Seek(f64),
    SeekBy(f64),
    SetVolume(f32),
    Jump(usize),
    Remove(usize),
    PlayNext(Track),
    Enqueue(Track),
    SetShuffle(bool),
    SetRepeat(Repeat),
    /// Resolver antes do clique (o mouse parou sobre a linha).
    Warm(String),
    /// Os cookies da conta (ou `None` ao sair), para o yt-dlp pedir o áudio
    /// como ela: 256 kbps no Premium, e as músicas com restrição de idade.
    SetAccount(Option<String>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub revision: u64,
    pub queue: Arc<Vec<Track>>,
    pub index: Option<usize>,
    pub status: Status,
    /// O que está demorando, em palavras ("Buscando o áudio").
    pub step: Option<&'static str>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub duration: Option<f64>,
    /// Um aviso para a tela, com id para não repetir.
    pub notice: Option<(u64, String)>,
}

impl Snapshot {
    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.queue.get(i))
    }
}

pub struct Config {
    /// Onde mora o yt-dlp gerenciado.
    pub bin_dir: PathBuf,
    /// Os endereços resolvidos (JSON pequeno), para reabrir e tocar sem
    /// esperar o yt-dlp.
    pub cache_file: PathBuf,
    pub client: Arc<Client>,
    pub volume: f32,
}

pub(crate) enum Msg {
    Cmd(Command),
    Audio(Event),
    Resolved { video_id: String, result: Result<Resolved, (String, bool)> },
    Fetched { video_id: String, result: Result<Audio, FetchError> },
    Queue { req: u64, result: Result<Watch, String> },
    /// A continuação do rádio, ou o autoplay do fim da fila. `req` é a fila
    /// de quando foi pedida: se outra a trocou, a resposta não serve mais.
    More { req: u64, result: Result<Watch, String> },
    Installed(Result<YtDlp, String>),
    /// A consulta diária: `Ok(None)` é já estar na última versão.
    Updated(Result<Option<YtDlp>, String>),
    Quit,
}

pub struct Player {
    tx: SyncSender<Msg>,
    snapshot: Arc<Mutex<Snapshot>>,
    revision: Arc<AtomicU64>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// `notify` é chamado a cada mudança do `Snapshot` (pedir um quadro).
    pub fn start(config: Config, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = sync_channel(256);
        let snapshot = Arc::new(Mutex::new(Snapshot { volume: config.volume, ..Snapshot::default() }));
        let revision = Arc::new(AtomicU64::new(0));
        let shared = Arc::new(Shared::default());
        shared.gain.store(curve(config.volume).to_bits(), Ordering::Relaxed);
        let engine = Engine::new(config, rx, tx.clone(), snapshot.clone(), revision.clone(), shared.clone(), notify);
        let thread = std::thread::Builder::new()
            .name("player".into())
            .spawn(move || engine.run())
            .expect("thread do player");
        Self { tx, snapshot, revision, shared, thread: Some(thread) }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Msg::Cmd(cmd));
    }

    /// Muda a cada `Snapshot` novo: a interface clona só quando muda.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Segundos tocados da música atual (lido do callback, sem trava).
    pub fn position(&self) -> f64 {
        self.shared.position_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }

    /// Quanto o som entregue demora a sair no dispositivo, em segundos (0
    /// antes de abrir a saída, ou onde o sistema não diz).
    pub fn latency(&self) -> f64 {
        let clock = self.shared.clock.lock().unwrap_or_else(|e| e.into_inner());
        clock.as_ref().map_or(0.0, |c| c.latency().as_secs_f64().min(1.0))
    }

    /// Onde está o que a pessoa ouve agora: a posição menos a latência.
    /// É a que a letra deve seguir.
    pub fn heard_position(&self) -> f64 {
        (self.position() - self.latency()).max(0.0)
    }

    /// As últimas amostras tocadas (para o visualizador) e a taxa delas.
    pub fn scope(&self, out: &mut [f32]) -> u32 {
        self.shared.scope.read(out);
        self.shared.out_rate.load(Ordering::Relaxed)
    }

    /// Para o player e grava o cache de endereços (no máximo ~1 s).
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(Msg::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// O volume do controle (0–1) em amplitude: o ouvido é logarítmico, e a
/// curva quadrática deixa a metade do controle soar como metade.
pub fn curve(volume: f32) -> f32 {
    let v = volume.clamp(0.0, 1.0);
    v * v
}

struct Want {
    video_id: String,
    token: u64,
    start_at: f64,
    autoplay: bool,
}

const MAX_RESOLVING: usize = 3;
const BYTES_KEPT: usize = 4;
/// O teto, em bytes, das músicas guardadas (`keep_bytes`). Quatro músicas
/// comuns somam ~16 MB (32 MB a 256 kbps) e cabem; quatro mixes de 1 h
/// passavam de 230 MB. A atual e a próxima nunca saem por tamanho.
const BYTES_BUDGET: usize = 48 * 1024 * 1024;
/// Quantos pânicos o laço do player aguenta (`Engine::turn`) antes de
/// desistir: um defeito que quebra toda volta pararia aqui, não num laço.
const MAX_PANICS: u32 = 20;

struct Engine {
    rx: Receiver<Msg>,
    tx: SyncSender<Msg>,
    snapshot: Arc<Mutex<Snapshot>>,
    revision: Arc<AtomicU64>,
    shared: Arc<Shared>,
    notify: Arc<dyn Fn() + Send + Sync>,

    output: Option<Output<Renderer>>,
    output_paused: bool,
    rcmds: Option<rtrb::Producer<RCmd>>,
    retired: Option<rtrb::Consumer<Box<Loaded>>>,

    client: Arc<Client>,
    http: Arc<reqwest::blocking::Client>,
    bin_dir: PathBuf,
    cache_file: PathBuf,
    /// A conta para o yt-dlp, e onde o arquivo de cookies dele é escrito.
    account: Option<ytdlp::Account>,
    cookie_dir: PathBuf,
    ytdlp: Option<Arc<YtDlp>>,
    installing: bool,
    tried_update: bool,

    resolved: HashMap<String, Resolved>,
    resolving: HashSet<String>,
    retried: HashSet<String>,
    bytes: VecDeque<(String, Audio)>,
    fetching: HashSet<String>,

    /// A fila, dividida com o `Snapshot` (e com a tela): muda por
    /// `Arc::make_mut`, que só copia quando a tela ainda segura a versão
    /// publicada. Antes cada publicação copiava a fila inteira, e o app
    /// carregava duas cópias dela o tempo todo.
    queue: Arc<Vec<Track>>,
    index: Option<usize>,
    want: Option<Want>,
    playing: Option<(String, u64)>,
    armed: Option<(String, u64)>,
    next_token: u64,
    status: Status,
    step: Option<&'static str>,
    volume: f32,
    shuffle: bool,
    repeat: Repeat,
    duration: Option<f64>,
    radio: Option<String>,
    more_inflight: bool,
    /// A última música da qual o autoplay já buscou o rádio, para não
    /// pedir de novo a cada quadro se o YouTube não devolveu nada.
    autoplay_seed: Option<String>,
    /// A fila acabou enquanto o autoplay ainda buscava: toca a primeira
    /// que chegar.
    resume_on_more: bool,
    queue_req: u64,
    failures: u32,
    notice: Option<(u64, String)>,
    notice_id: u64,

    /// Onde retomar a música pausada que ainda não está no callback.
    restore_at: Option<f64>,
    fade_deadline: Option<Instant>,
    last_active: Instant,
    cache_dirty: bool,
    cache_saved: Instant,
    rng: u64,
}

impl Engine {
    fn new(
        config: Config,
        rx: Receiver<Msg>,
        tx: SyncSender<Msg>,
        snapshot: Arc<Mutex<Snapshot>>,
        revision: Arc<AtomicU64>,
        shared: Arc<Shared>,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let resolved = load_cache(&config.cache_file);
        // A mesma fila (vazia) do `Snapshot`: `publish` compara por ponteiro.
        let queue = snapshot.lock().unwrap_or_else(|e| e.into_inner()).queue.clone();
        let ytdlp = YtDlp::locate(&config.bin_dir).map(Arc::new);
        let cookie_dir = config.cache_file.parent().map(|d| d.join("tmp")).unwrap_or_else(|| config.bin_dir.join("tmp"));
        ytdlp::clear_cookie_files(&cookie_dir);
        Self {
            rx,
            tx,
            snapshot,
            revision,
            shared,
            notify,
            output: None,
            output_paused: true,
            rcmds: None,
            retired: None,
            client: config.client,
            http: Arc::new(http_client()),
            bin_dir: config.bin_dir,
            account: None,
            cookie_dir,
            cache_file: config.cache_file,
            ytdlp,
            installing: false,
            tried_update: false,
            resolved,
            resolving: HashSet::new(),
            retried: HashSet::new(),
            bytes: VecDeque::new(),
            fetching: HashSet::new(),
            queue,
            index: None,
            want: None,
            playing: None,
            armed: None,
            next_token: 0,
            status: Status::Idle,
            step: None,
            volume: config.volume,
            shuffle: false,
            repeat: Repeat::Off,
            duration: None,
            radio: None,
            more_inflight: false,
            autoplay_seed: None,
            resume_on_more: false,
            queue_req: 0,
            failures: 0,
            notice: None,
            notice_id: 0,
            restore_at: None,
            fade_deadline: None,
            last_active: Instant::now(),
            cache_dirty: false,
            cache_saved: Instant::now(),
            rng: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64 | 1)
                .unwrap_or(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn run(mut self) {
        // A versão em pasta do yt-dlp abre em ~1 s; a de arquivo único do
        // PATH (winget), em ~3,9 s. Se só existe a segunda, a primeira vem em
        // segundo plano já na primeira música.
        self.spawn_maintenance();
        let mut panics = 0;
        loop {
            let msg = match self.timeout() {
                Some(t) => match self.rx.recv_timeout(t) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match self.rx.recv() {
                    Ok(m) => Some(m),
                    Err(_) => break,
                },
            };
            if matches!(msg, Some(Msg::Quit)) {
                break;
            }
            if !self.turn(msg) {
                panics += 1;
                // Um defeito que quebra toda volta não vira um laço de pânicos.
                if panics >= MAX_PANICS {
                    log::error!("o player entrou em pânico {panics} vezes; parando");
                    break;
                }
            }
        }
        self.save_cache();
        self.rcmd(RCmd::Stop);
        if let Some(out) = self.output.as_mut() {
            out.pause();
        }
    }

    /// Uma volta do laço: a mensagem (se veio) e a manutenção. Um pânico no
    /// meio não derruba o player: antes a thread morria calada, a tela
    /// seguia, nada mais tocava e os comandos caíam no vazio até reabrir o
    /// app. `false` se entrou em pânico.
    fn turn(&mut self, msg: Option<Msg>) -> bool {
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(msg) = msg {
                self.handle(msg);
            }
            self.housekeeping();
        }))
        .is_ok();
        if !ok {
            self.recover();
        }
        ok
    }

    /// Depois de um pânico no meio de uma mensagem. O que carregava para (a
    /// música pedida podia ficar sem quem a levasse adiante), os pedidos em
    /// andamento podem ser refeitos, e a tela volta a ver o estado. A música
    /// que já toca segue: ela está no callback de áudio.
    fn recover(&mut self) {
        // O endereço do pânico já foi para o panicos.log.
        log::error!("o player entrou em pânico; o que carregava parou");
        self.want = None;
        self.resolving.clear();
        self.fetching.clear();
        self.more_inflight = false;
        self.resume_on_more = false;
        if self.playing.is_none() && self.status == Status::Loading {
            self.status = if self.index.is_some() { Status::Paused } else { Status::Idle };
            self.step = None;
        }
        // O `publish` pode ter sido justamente onde quebrou.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.publish()));
    }

    fn timeout(&self) -> Option<Duration> {
        if let Some(deadline) = self.fade_deadline {
            return Some(deadline.saturating_duration_since(Instant::now()));
        }
        if self.output.is_some() && !self.output_paused {
            return Some(Duration::from_secs(2));
        }
        if self.output.is_some() && self.last_active.elapsed() < Duration::from_secs(6 * 60) {
            return Some(Duration::from_secs(10));
        }
        if self.cache_dirty {
            return Some(Duration::from_secs(30));
        }
        None
    }

    fn housekeeping(&mut self) {
        if let Some(deadline) = self.fade_deadline {
            if Instant::now() >= deadline {
                // O dispositivo não mandou o FadedOut a tempo: pausa assim.
                self.fade_deadline = None;
                self.pause_output();
            }
        }
        if let Some(out) = self.output.as_mut() {
            match out.maintain() {
                Maintained::Reopened { device, sample_rate, .. } => {
                    log::info!("saída de áudio reaberta: {device} a {sample_rate} Hz");
                }
                Maintained::Failed(error) => log::warn!("saída de áudio: {error}"),
                _ => {}
            }
            for error in out.take_errors() {
                log::warn!("áudio: {error:?}");
            }
            if !self.output_paused {
                self.last_active = Instant::now();
            }
        }
        if let Some(retired) = self.retired.as_mut() {
            while retired.pop().is_ok() {}
        }
        if self.cache_dirty && self.cache_saved.elapsed() > Duration::from_secs(30) {
            self.save_cache();
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Cmd(cmd) => self.command(cmd),
            Msg::Audio(event) => self.audio(event),
            Msg::Resolved { video_id, result } => self.on_resolved(video_id, result),
            Msg::Fetched { video_id, result } => self.on_fetched(video_id, result),
            Msg::Queue { req, result } => self.on_queue(req, result),
            Msg::More { req, result } => self.on_more(req, result),
            Msg::Installed(result) => self.on_installed(result),
            Msg::Updated(result) => {
                self.installing = false;
                match result {
                    Ok(Some(y)) => self.ytdlp = Some(Arc::new(y)),
                    Ok(None) => {}
                    Err(error) => log::info!("consulta diária do yt-dlp: {error}"),
                }
            }
            Msg::Quit => {}
        }
        self.publish();
    }

    fn publish(&mut self) {
        let mut s = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        // Toda mudança da fila troca o ponteiro (`make_mut` copia, porque o
        // `Snapshot` segura a versão publicada; ou uma fila nova): comparar
        // ponteiros basta, sem percorrer 2.000 músicas a cada mensagem. E
        // pega também a mesma música com dados novos (os artistas que a fila
        // do YouTube traz para a semente do rádio), que a comparação por id
        // deixava passar.
        let queue_changed = !Arc::ptr_eq(&s.queue, &self.queue);
        let changed = queue_changed
            || s.index != self.index
            || s.status != self.status
            || s.step != self.step
            || s.volume != self.volume
            || s.shuffle != self.shuffle
            || s.repeat != self.repeat
            || s.duration != self.duration
            || s.notice != self.notice;
        if !changed {
            return;
        }
        if queue_changed {
            s.queue = self.queue.clone();
        }
        s.index = self.index;
        s.status = self.status;
        s.step = self.step;
        s.volume = self.volume;
        s.shuffle = self.shuffle;
        s.repeat = self.repeat;
        s.duration = self.duration;
        s.notice = self.notice.clone();
        s.revision += 1;
        self.revision.store(s.revision, Ordering::Release);
        drop(s);
        (self.notify)();
    }

    fn say(&mut self, text: String) {
        log::warn!("{text}");
        self.notice_id += 1;
        self.notice = Some((self.notice_id, text));
    }

    // ── saída de áudio ─────────────────────────────────────────────────

    fn ensure_output(&mut self) -> bool {
        if self.output.is_some() {
            return true;
        }
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(64);
        let (ret_tx, ret_rx) = rtrb::RingBuffer::new(16);
        let renderer = Renderer::new(cmd_rx, ret_tx, self.tx.clone(), self.shared.clone());
        // 44,1 kHz é a taxa do AAC do YouTube; o WASAPI converte para a do
        // dispositivo (AUTOCONVERTPCM) e o reamostrador próprio fica parado.
        let options = OutputOptions { sample_rate: Some(44_100), ..OutputOptions::default() };
        match Output::open(options, renderer) {
            Ok(mut out) => {
                out.pause();
                log::info!("saída de áudio: {} a {} Hz, {} canais", out.device_name(), out.sample_rate(), out.channels());
                *self.shared.clock.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clock());
                self.output = Some(out);
                self.output_paused = true;
                self.rcmds = Some(cmd_tx);
                self.retired = Some(ret_rx);
                true
            }
            Err(error) => {
                self.say(musicfast_i18n::t().no_audio_output(&error.to_string()));
                false
            }
        }
    }

    fn rcmd(&mut self, cmd: RCmd) {
        if let Some(p) = self.rcmds.as_mut() {
            if p.push(cmd).is_err() {
                log::warn!("fila do callback cheia; ordem perdida");
            }
        }
    }

    fn play_output(&mut self) {
        self.fade_deadline = None;
        if let Some(out) = self.output.as_mut() {
            if self.output_paused {
                out.resume();
                self.output_paused = false;
            }
        }
        self.last_active = Instant::now();
    }

    fn pause_output(&mut self) {
        if let Some(out) = self.output.as_mut() {
            if !self.output_paused {
                out.pause();
                self.output_paused = true;
            }
        }
        self.last_active = Instant::now();
    }

    // ── a fila ─────────────────────────────────────────────────────────

    fn command(&mut self, cmd: Command) {
        match cmd {
            Command::PlayTracks { tracks, index } => {
                self.queue = Arc::new(tracks);
                self.radio = None;
                self.queue_req += 1;
                self.reshuffle();
                self.start(index.min(self.queue.len().saturating_sub(1)), 0.0, true);
            }
            Command::PlayRadio { seed } => {
                let video_id = seed.video_id.clone();
                self.queue = Arc::new(vec![seed]);
                self.radio = None;
                self.start(0, 0.0, true);
                self.fetch_queue(Some(video_id), None, None);
            }
            Command::PlayList { seed, playlist_id, params } => {
                let video_id = seed.as_ref().map(|s| s.video_id.clone());
                self.radio = None;
                match seed {
                    Some(seed) => {
                        self.queue = Arc::new(vec![seed]);
                        self.start(0, 0.0, true);
                    }
                    None => {
                        self.queue = Arc::default();
                        self.index = None;
                        self.status = Status::Loading;
                        self.step = Some(musicfast_i18n::t().building_queue);
                        self.rcmd(RCmd::Stop);
                    }
                }
                self.fetch_queue(video_id, Some(playlist_id), params);
            }
            Command::Restore { tracks, index, position } => {
                if tracks.is_empty() {
                    return;
                }
                self.queue = Arc::new(tracks);
                let index = index.min(self.queue.len() - 1);
                self.index = Some(index);
                self.duration = self.queue[index].duration_secs.map(f64::from);
                self.status = Status::Paused;
                self.shared.position_ms.store((position * 1000.0) as u64, Ordering::Relaxed);
                self.restore_at = Some(position);
                // Resolve e baixa a música da sessão anterior em segundo
                // plano: o primeiro play depois de abrir sai na hora.
                let video_id = self.queue[index].video_id.clone();
                self.need(&video_id);
            }
            Command::Toggle => match self.status {
                Status::Playing => self.pause(),
                Status::Loading => {
                    if let Some(w) = self.want.as_mut() {
                        w.autoplay = !w.autoplay;
                    }
                }
                _ => self.resume(),
            },
            Command::Pause => self.pause(),
            Command::Resume => self.resume(),
            Command::Next => {
                if let Some(i) = self.index {
                    let next = if i + 1 < self.queue.len() {
                        Some(i + 1)
                    } else if self.repeat == Repeat::All && !self.queue.is_empty() {
                        Some(0)
                    } else {
                        None
                    };
                    if let Some(n) = next {
                        self.start(n, 0.0, true);
                    }
                }
            }
            Command::Previous => {
                let position = self.shared.position_ms.load(Ordering::Relaxed) as f64 / 1000.0;
                match self.index {
                    Some(_) if position > 3.0 && self.playing.is_some() => self.seek(0.0),
                    Some(i) if i > 0 => self.start(i - 1, 0.0, true),
                    Some(_) => self.seek(0.0),
                    None => {}
                }
            }
            Command::Seek(secs) => self.seek(secs),
            Command::SeekBy(delta) => {
                let position = self.shared.position_ms.load(Ordering::Relaxed) as f64 / 1000.0;
                self.seek((position + delta).max(0.0));
            }
            Command::SetVolume(v) => {
                self.volume = v.clamp(0.0, 1.0);
                self.shared.gain.store(curve(self.volume).to_bits(), Ordering::Relaxed);
            }
            Command::Jump(i) => {
                if i < self.queue.len() {
                    self.start(i, 0.0, true);
                }
            }
            Command::Remove(i) => {
                if i >= self.queue.len() {
                    return;
                }
                Arc::make_mut(&mut self.queue).remove(i);
                match self.index {
                    Some(cur) if i < cur => self.index = Some(cur - 1),
                    Some(cur) if i == cur => {
                        if self.queue.is_empty() {
                            self.stop_all();
                        } else {
                            self.start(cur.min(self.queue.len() - 1), 0.0, self.status == Status::Playing);
                        }
                        return;
                    }
                    _ => {}
                }
                self.prepare_next();
            }
            Command::PlayNext(track) => {
                match self.index {
                    Some(i) => Arc::make_mut(&mut self.queue).insert(i + 1, track),
                    None => {
                        self.queue = Arc::new(vec![track]);
                        self.start(0, 0.0, true);
                        return;
                    }
                }
                self.prepare_next();
            }
            Command::Enqueue(track) => {
                Arc::make_mut(&mut self.queue).push(track);
                if self.index.is_none() {
                    self.start(self.queue.len() - 1, 0.0, true);
                } else {
                    self.prepare_next();
                }
            }
            Command::SetShuffle(on) => {
                self.shuffle = on;
                self.reshuffle();
                self.prepare_next();
            }
            Command::SetRepeat(r) => {
                self.repeat = r;
                self.prepare_next();
            }
            Command::SetAccount(cookie) => {
                self.account = cookie.map(|cookie| ytdlp::Account { cookie: cookie.into(), dir: self.cookie_dir.clone() });
            }
            Command::Warm(video_id) => {
                let fresh = self.resolved.get(&video_id).is_some_and(|r| r.fresh(now()));
                if !fresh && !self.resolving.contains(&video_id) && self.resolving.len() + 1 < MAX_RESOLVING {
                    self.resolve(&video_id);
                }
            }
        }
    }

    /// Embaralha o que vem depois da atual (como o site: a lista muda, e
    /// desligar não desfaz).
    fn reshuffle(&mut self) {
        if !self.shuffle {
            return;
        }
        let from = self.index.map(|i| i + 1).unwrap_or(0);
        let n = self.queue.len();
        if from + 1 >= n {
            return;
        }
        let queue = Arc::make_mut(&mut self.queue);
        for i in (from + 1..n).rev() {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            let j = from + (self.rng % (i - from + 1) as u64) as usize;
            queue.swap(i, j);
        }
    }

    fn stop_all(&mut self) {
        self.rcmd(RCmd::Stop);
        self.want = None;
        self.playing = None;
        self.armed = None;
        self.index = None;
        self.status = Status::Idle;
        self.step = None;
        self.duration = None;
        self.pause_output();
    }

    fn start(&mut self, index: usize, start_at: f64, autoplay: bool) {
        let Some(track) = self.queue.get(index) else { return };
        let video_id = track.video_id.clone();
        self.index = Some(index);
        self.duration = track.duration_secs.map(f64::from);
        self.next_token += 1;
        self.want = Some(Want { video_id, token: self.next_token, start_at, autoplay });
        self.restore_at = None;
        self.playing = None;
        self.armed = None;
        // A anterior para na hora: o clique tem resposta mesmo com a nova
        // ainda a 3 s de distância.
        self.rcmd(RCmd::Stop);
        self.shared.position_ms.store((start_at * 1000.0) as u64, Ordering::Relaxed);
        self.status = Status::Loading;
        self.advance_want();
    }

    fn pause(&mut self) {
        if self.status == Status::Playing {
            self.rcmd(RCmd::Fade(0.0));
            self.fade_deadline = Some(Instant::now() + Duration::from_millis(150));
            self.status = Status::Paused;
        } else if self.status == Status::Loading {
            if let Some(w) = self.want.as_mut() {
                w.autoplay = false;
            }
        }
    }

    fn resume(&mut self) {
        match self.status {
            Status::Paused if self.playing.is_some() => {
                self.rcmd(RCmd::Fade(1.0));
                self.play_output();
                self.status = Status::Playing;
            }
            Status::Paused | Status::Idle => {
                if let Some(i) = self.index.or((!self.queue.is_empty()).then_some(0)) {
                    let at = if self.status == Status::Paused { self.restore_at.take().unwrap_or(0.0) } else { 0.0 };
                    self.start(i, at, true);
                }
            }
            Status::Loading => {
                if let Some(w) = self.want.as_mut() {
                    w.autoplay = true;
                }
            }
            Status::Playing => {}
        }
    }

    fn seek(&mut self, secs: f64) {
        let secs = match self.duration {
            Some(d) => secs.min((d - 0.5).max(0.0)),
            None => secs,
        };
        if let Some(w) = self.want.as_mut() {
            w.start_at = secs;
        } else if self.playing.is_some() {
            self.rcmd(RCmd::Seek(secs));
        } else if self.status == Status::Paused {
            self.restore_at = Some(secs);
        }
        self.shared.position_ms.store((secs * 1000.0) as u64, Ordering::Relaxed);
    }

    fn next_index(&self) -> Option<usize> {
        let i = self.index?;
        match self.repeat {
            Repeat::One => Some(i),
            _ if i + 1 < self.queue.len() => Some(i + 1),
            Repeat::All if !self.queue.is_empty() => Some(0),
            _ => None,
        }
    }

    fn bytes_for(&mut self, video_id: &str) -> Option<Audio> {
        let pos = self.bytes.iter().position(|(id, _)| id == video_id)?;
        let entry = self.bytes.remove(pos)?;
        let bytes = entry.1.clone();
        self.bytes.push_back(entry);
        Some(bytes)
    }

    fn keep_bytes(&mut self, video_id: String, bytes: Audio) {
        self.bytes.retain(|(id, _)| *id != video_id);
        self.bytes.push_back((video_id, bytes));
        trim_kept(&mut self.bytes);
    }

    /// Leva a música pedida adiante: bytes → decodificador → callback.
    fn advance_want(&mut self) {
        let Some(video_id) = self.want.as_ref().map(|w| w.video_id.clone()) else { return };
        let Some(bytes) = self.bytes_for(&video_id) else {
            self.step = Some(if self.resolved.get(&video_id).is_some_and(|r| r.fresh(now())) {
                musicfast_i18n::t().downloading
            } else if self.ytdlp.is_none() {
                musicfast_i18n::t().preparing_ytdlp
            } else {
                musicfast_i18n::t().fetching_audio
            });
            self.need(&video_id);
            return;
        };
        let decoder = match Decoder::open(bytes) {
            Ok(d) => d,
            Err(error) => return self.fail_current(error),
        };
        if !self.ensure_output() {
            self.want = None;
            self.status = Status::Idle;
            self.step = None;
            return;
        }
        let want = self.want.take().expect("want");
        if let Some(d) = decoder.duration {
            self.duration = Some(d);
        }
        self.failures = 0;
        self.step = None;
        self.rcmd(RCmd::Fade(1.0));
        self.rcmd(RCmd::Play(Box::new(Loaded { token: want.token, decoder, start_at: want.start_at })));
        self.playing = Some((video_id, want.token));
        if want.autoplay {
            self.play_output();
            self.status = Status::Playing;
        } else {
            self.status = Status::Paused;
        }
        self.prepare_next();
        self.extend_radio();
    }

    fn fail_current(&mut self, error: String) {
        let title = self.index.and_then(|i| self.queue.get(i)).map(|t| t.title.clone()).unwrap_or_default();
        self.want = None;
        self.failures += 1;
        self.say(if title.is_empty() { error } else { format!("{title}: {error}") });
        // Três seguidas é rede ou YouTube, não a música: para de pular.
        match self.next_index().filter(|_| self.failures < 3 && self.repeat != Repeat::One) {
            Some(n) if Some(n) != self.index => self.start(n, 0.0, true),
            _ => {
                self.status = Status::Idle;
                self.step = None;
                self.pause_output();
            }
        }
    }

    /// Garante que os bytes de `video_id` estão vindo.
    fn need(&mut self, video_id: &str) {
        if self.bytes.iter().any(|(id, _)| id == video_id)
            || self.fetching.contains(video_id)
            || self.resolving.contains(video_id)
        {
            return;
        }
        match self.resolved.get(video_id).filter(|r| r.fresh(now())).cloned() {
            Some(r) => self.spawn_fetch(r),
            None => self.resolve(video_id),
        }
    }

    fn resolve(&mut self, video_id: &str) {
        let Some(ytdlp) = self.ytdlp_or_install() else { return };
        self.resolving.insert(video_id.to_owned());
        let video_id = video_id.to_owned();
        let account = self.account.clone();
        let failed = Msg::Resolved { video_id: video_id.clone(), result: Err(("erro interno ao ler a resposta do yt-dlp".into(), false)) };
        answering("resolver", self.tx.clone(), Some(failed), move || {
            let result = ytdlp.resolve(&video_id, account.as_ref());
            Some(Msg::Resolved { video_id, result })
        });
    }

    fn ytdlp_or_install(&mut self) -> Option<Arc<YtDlp>> {
        let managed = self.ytdlp.as_ref().is_some_and(|y| y.is_managed(&self.bin_dir));
        if !managed && !self.installing && !self.tried_update {
            self.spawn_install();
        }
        self.ytdlp.clone()
    }

    /// Instala o yt-dlp gerenciado; se já existe, só troca quando saiu
    /// versão nova (não adianta baixar a mesma de novo).
    fn spawn_install(&mut self) {
        self.installing = true;
        let dir = self.bin_dir.clone();
        let managed = self.ytdlp.as_ref().is_some_and(|y| y.is_managed(&self.bin_dir));
        // Sem resposta, `installing` ficaria ligado e nada mais instalaria.
        let failed = Msg::Installed(Err("erro interno ao instalar o yt-dlp".into()));
        answering("instalar-yt-dlp", self.tx.clone(), Some(failed), move || {
            let _ = std::fs::create_dir_all(&dir);
            let result = if managed {
                ytdlp::update(&dir).and_then(|fresh| fresh.ok_or_else(|| musicfast_i18n::t().ytdlp_already_latest.to_owned()))
            } else {
                ytdlp::install(&dir)
            };
            Some(Msg::Installed(result))
        });
    }

    /// Na abertura, fora do caminho de tocar: o Deno (uma vez) e a consulta
    /// diária do yt-dlp. O app fica aberto dias, mas a consulta da abertura
    /// basta: o erro de "yt-dlp velho" também dispara uma.
    fn spawn_maintenance(&mut self) {
        let due = ytdlp::update_due(&self.bin_dir);
        if due {
            self.installing = true;
        }
        let dir = self.bin_dir.clone();
        // Só a consulta diária tem quem espere por ela (`installing`).
        let failed = due.then(|| Msg::Updated(Err("erro interno na consulta do yt-dlp".into())));
        answering("manutencao-yt-dlp", self.tx.clone(), failed, move || {
            let _ = std::fs::create_dir_all(&dir);
            if let Err(error) = ytdlp::ensure_deno(&dir) {
                log::warn!("Deno não instalado: {error}");
            }
            due.then(|| Msg::Updated(ytdlp::update(&dir)))
        });
    }

    fn on_installed(&mut self, result: Result<YtDlp, String>) {
        self.installing = false;
        match result {
            Ok(y) => self.ytdlp = Some(Arc::new(y)),
            Err(error) => {
                // Não tentar de novo a cada música: na próxima abertura.
                self.tried_update = true;
                if self.ytdlp.is_none() {
                    self.say(musicfast_i18n::t().ytdlp_install_failed(&error.to_string()));
                    if self.want.is_some() {
                        self.want = None;
                        self.status = Status::Idle;
                        self.step = None;
                    }
                    return;
                }
                log::warn!("yt-dlp gerenciado não instalou ({error}); seguindo com o do PATH");
            }
        }
        // O que esperava o yt-dlp segue agora.
        if let Some(w) = self.want.as_ref().map(|w| w.video_id.clone()) {
            self.need(&w);
        }
        self.prepare_next();
    }

    fn spawn_fetch(&mut self, r: Resolved) {
        self.fetching.insert(r.video_id.clone());
        let http = self.http.clone();
        let failed = Msg::Fetched { video_id: r.video_id.clone(), result: Err(FetchError::Other("erro interno no download".into())) };
        answering("download", self.tx.clone(), Some(failed), move || {
            let result = fetch(&http, &r);
            Some(Msg::Fetched { video_id: r.video_id, result })
        });
    }

    fn is_wanted(&self, video_id: &str) -> bool {
        self.want.as_ref().is_some_and(|w| w.video_id == video_id)
    }

    fn is_next(&self, video_id: &str) -> bool {
        self.next_index().and_then(|i| self.queue.get(i)).is_some_and(|t| t.video_id == video_id)
    }

    fn is_restore(&self, video_id: &str) -> bool {
        self.restore_at.is_some() && self.index.and_then(|i| self.queue.get(i)).is_some_and(|t| t.video_id == video_id)
    }

    fn on_resolved(&mut self, video_id: String, result: Result<Resolved, (String, bool)>) {
        self.resolving.remove(&video_id);
        match result {
            Ok(r) => {
                self.resolved.insert(video_id.clone(), r.clone());
                self.cache_dirty = true;
                if self.is_wanted(&video_id) || self.is_next(&video_id) || self.is_restore(&video_id) {
                    if self.is_wanted(&video_id) {
                        self.step = Some("Baixando");
                    }
                    self.spawn_fetch(r);
                }
            }
            Err((error, outdated)) => {
                if outdated && !self.tried_update && !self.installing {
                    // O YouTube mudou e o yt-dlp envelheceu: atualiza e tenta
                    // de novo uma vez (como o Download de Mídias faz).
                    self.tried_update = true;
                    self.spawn_install();
                    return;
                }
                if self.is_wanted(&video_id) {
                    self.fail_current(error);
                } else {
                    log::info!("pré-resolução de {video_id} falhou: {error}");
                }
            }
        }
    }

    fn on_fetched(&mut self, video_id: String, result: Result<Audio, FetchError>) {
        self.fetching.remove(&video_id);
        match result {
            Ok(bytes) => {
                self.keep_bytes(video_id.clone(), bytes);
                if self.is_wanted(&video_id) {
                    self.advance_want();
                } else if self.is_next(&video_id) {
                    self.prepare_next();
                }
            }
            Err(FetchError::Expired) => {
                self.resolved.remove(&video_id);
                self.cache_dirty = true;
                let relevant = self.is_wanted(&video_id) || self.is_next(&video_id);
                if relevant && self.retried.insert(video_id.clone()) {
                    self.resolve(&video_id);
                } else if self.is_wanted(&video_id) {
                    self.fail_current(musicfast_i18n::t().audio_url_refused.into());
                }
            }
            Err(FetchError::Other(error)) => {
                if self.is_wanted(&video_id) {
                    self.fail_current(error);
                }
            }
        }
    }

    /// Entrega ao callback a próxima música já decodificável.
    fn prepare_next(&mut self) {
        if self.playing.is_none() {
            return;
        }
        let Some(ni) = self.next_index() else {
            if self.armed.take().is_some() {
                self.rcmd(RCmd::Arm(None));
            }
            return;
        };
        let video_id = self.queue[ni].video_id.clone();
        if self.armed.as_ref().is_some_and(|(v, _)| *v == video_id) {
            return;
        }
        if self.armed.take().is_some() {
            self.rcmd(RCmd::Arm(None));
        }
        let Some(bytes) = self.bytes_for(&video_id) else {
            self.need(&video_id);
            return;
        };
        match Decoder::open(bytes) {
            Ok(decoder) => {
                self.next_token += 1;
                let token = self.next_token;
                self.armed = Some((video_id, token));
                self.rcmd(RCmd::Arm(Some(Box::new(Loaded { token, decoder, start_at: 0.0 }))));
            }
            Err(error) => log::warn!("a próxima ({video_id}) não abre: {error}"),
        }
    }

    fn audio(&mut self, event: Event) {
        match event {
            Event::Started { token } => {
                if self.armed.as_ref().is_some_and(|(_, t)| *t == token) {
                    // A próxima entrou sozinha, sem intervalo.
                    let (video_id, token) = self.armed.take().expect("armed");
                    self.index = self.next_index();
                    self.playing = Some((video_id, token));
                    self.duration = self.index.and_then(|i| self.queue.get(i)).and_then(|t| t.duration_secs).map(f64::from);
                    if self.status != Status::Paused {
                        self.status = Status::Playing;
                    }
                    self.prepare_next();
                    self.extend_radio();
                }
            }
            Event::Ended { token } => {
                let was_playing = self.playing.as_ref().is_some_and(|(_, t)| *t == token);
                if was_playing && self.armed.is_none() {
                    self.playing = None;
                    match self.next_index() {
                        Some(n) => self.start(n, 0.0, true),
                        None => {
                            self.extend_radio();
                            if self.more_inflight {
                                // O autoplay ainda não respondeu: segue assim que chegar.
                                self.resume_on_more = true;
                                self.status = Status::Loading;
                                self.step = Some(musicfast_i18n::t().finding_similar);
                            } else {
                                self.status = Status::Paused;
                                self.restore_at = Some(0.0);
                                self.shared.position_ms.store(0, Ordering::Relaxed);
                                self.pause_output();
                            }
                        }
                    }
                }
            }
            Event::Failed { token, error } => {
                if self.playing.as_ref().is_some_and(|(_, t)| *t == token) {
                    self.playing = None;
                    self.fail_current(error.to_owned());
                }
            }
            Event::FadedOut => {
                if self.fade_deadline.take().is_some() && self.status == Status::Paused {
                    self.pause_output();
                }
            }
        }
    }

    // ── fila do YouTube (rádio, playlist) ──────────────────────────────

    fn fetch_queue(&mut self, video_id: Option<String>, playlist_id: Option<String>, params: Option<String>) {
        self.queue_req += 1;
        let req = self.queue_req;
        let client = self.client.clone();
        let failed = Msg::Queue { req, result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        answering("fila", self.tx.clone(), Some(failed), move || {
            let result = client
                .watch(video_id.as_deref(), playlist_id.as_deref(), params.as_deref())
                .map_err(|e| e.to_string());
            Some(Msg::Queue { req, result })
        });
    }

    fn on_queue(&mut self, req: u64, result: Result<Watch, String>) {
        // Só a última importa: um clique novo já trocou a fila.
        if req != self.queue_req {
            return;
        }
        let watch = match result {
            Ok(w) if !w.tracks.is_empty() => w,
            Ok(_) => return self.say(musicfast_i18n::t().queue_not_built.into()),
            Err(e) => return self.say(musicfast_i18n::t().queue_error(&e.to_string())),
        };
        self.radio = watch.continuation;
        let current = self.index.and_then(|i| self.queue.get(i)).cloned();
        match current {
            Some(cur) => {
                let mut tracks = watch.tracks;
                let pos = match tracks.iter().position(|t| t.video_id == cur.video_id) {
                    Some(p) => p,
                    None => {
                        tracks.insert(0, cur);
                        0
                    }
                };
                // A atual guarda os dados que já tinha (capa, duração).
                self.queue = Arc::new(tracks);
                self.index = Some(pos);
                self.reshuffle();
                self.prepare_next();
            }
            None => {
                self.queue = Arc::new(watch.tracks);
                self.reshuffle();
                self.start(0, 0.0, true);
            }
        }
    }

    /// Mantém a fila andando: com rádio, pede a continuação perto do fim;
    /// sem rádio (um álbum, uma playlist, músicas escolhidas), ao chegar na
    /// última música busca o rádio dela e acrescenta as parecidas no fim,
    /// como o autoplay do YouTube Music. Com repetir ligado a fila não acaba,
    /// então não há autoplay.
    fn extend_radio(&mut self) {
        let Some(i) = self.index else { return };
        if self.more_inflight {
            return;
        }
        let req = self.queue_req;
        let client = self.client.clone();
        let tx = self.tx.clone();
        // Sem resposta, `more_inflight` ficaria ligado e a fila não cresceria
        // mais até reabrir o app.
        let failed = Msg::More { req, result: Err(musicfast_i18n::t().ytm_internal_error.to_owned()) };
        match self.radio.clone() {
            Some(token) => {
                if i + 5 < self.queue.len() {
                    return;
                }
                self.more_inflight = true;
                answering("radio", tx, Some(failed), move || {
                    let result = client.watch_more(&token).map_err(|e| e.to_string());
                    Some(Msg::More { req, result })
                });
            }
            None => {
                if self.repeat != Repeat::Off || i + 1 < self.queue.len() {
                    return;
                }
                let Some(last) = self.queue.last().map(|t| t.video_id.clone()) else { return };
                if self.autoplay_seed.as_deref() == Some(last.as_str()) {
                    return;
                }
                self.autoplay_seed = Some(last.clone());
                self.more_inflight = true;
                answering("autoplay", tx, Some(failed), move || {
                    let result = client.watch(Some(&last), None, None).map_err(|e| e.to_string());
                    Some(Msg::More { req, result })
                });
            }
        }
    }

    fn on_more(&mut self, req: u64, result: Result<Watch, String>) {
        self.more_inflight = false;
        let resume = std::mem::take(&mut self.resume_on_more);
        if req != self.queue_req {
            return;
        }
        let before = self.queue.len();
        match result {
            Ok(w) => {
                let known: HashSet<&str> = self.queue.iter().map(|t| t.video_id.as_str()).collect();
                let fresh: Vec<Track> = w.tracks.into_iter().filter(|t| !known.contains(t.video_id.as_str())).collect();
                // Sem nada novo, a fila (e a cópia que a mudança custaria) fica.
                if !fresh.is_empty() {
                    Arc::make_mut(&mut self.queue).extend(fresh);
                }
                self.radio = w.continuation;
                self.prepare_next();
            }
            Err(e) => log::warn!("mais rádio: {e}"),
        }
        if resume && self.playing.is_none() && self.want.is_none() {
            if self.queue.len() > before {
                self.start(before, 0.0, true);
            } else {
                self.status = Status::Paused;
                self.step = None;
                self.restore_at = Some(0.0);
                self.shared.position_ms.store(0, Ordering::Relaxed);
                self.pause_output();
            }
        }
    }

    // ── cache de endereços ─────────────────────────────────────────────

    fn save_cache(&mut self) {
        let now = now();
        self.resolved.retain(|_, r| r.fresh(now));
        let entries: Vec<&Resolved> = self.resolved.values().collect();
        if let Ok(text) = serde_json::to_string(&entries) {
            if let Some(dir) = self.cache_file.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = self.cache_file.with_extension("tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, &self.cache_file);
            }
        }
        self.cache_dirty = false;
        self.cache_saved = Instant::now();
    }
}

fn load_cache(path: &std::path::Path) -> HashMap<String, Resolved> {
    let now = now();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Resolved>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.fresh(now))
        .map(|r| (r.video_id.clone(), r))
        .collect()
}

/// O teto das músicas guardadas: a atual, a próxima e as duas últimas (o
/// "voltar" é na hora), até `BYTES_BUDGET` juntas. As duas do fim (as
/// últimas usadas: a atual e a próxima) ficam mesmo acima dele. O que sai
/// daqui e ainda toca continua vivo no decodificador.
fn trim_kept(kept: &mut VecDeque<(String, Audio)>) {
    let mut total: usize = kept.iter().map(|(_, b)| b.len()).sum();
    while kept.len() > BYTES_KEPT || (kept.len() > 2 && total > BYTES_BUDGET) {
        if let Some((_, old)) = kept.pop_front() {
            total -= old.len();
        }
    }
}

/// Uma thread de trabalho do player que sempre responde ao motor: se `work`
/// entrar em pânico (a saída do yt-dlp ou o JSON do YouTube numa forma
/// inesperada), a resposta é `on_panic`. Sem ela, o motor esperava para
/// sempre: a música parada em "Buscando o áudio" (marcada como resolvendo,
/// nunca pedida de novo), o rádio sem crescer até reabrir o app. As threads
/// da janela já faziam o mesmo (`jobs::spawn_or`, DOCUMENTACAO_PERFORMANCE.md
/// §1.2).
fn answering(name: &str, tx: SyncSender<Msg>, on_panic: Option<Msg>, work: impl FnOnce() -> Option<Msg> + Send + 'static) {
    let thread = name.to_owned();
    let spawned = std::thread::Builder::new().name(name.into()).spawn(move || {
        let msg = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
            Ok(msg) => msg,
            Err(_) => {
                // O endereço do pânico já foi para o panicos.log.
                log::error!("thread {thread} do player entrou em pânico");
                on_panic
            }
        };
        if let Some(msg) = msg {
            let _ = tx.send(msg);
        }
    });
    if let Err(error) = spawned {
        log::warn!("thread {name}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_that_panics_still_answers() {
        let (tx, rx) = sync_channel(4);
        let failed = Msg::Queue { req: 7, result: Err("falhou".into()) };
        answering("teste", tx, Some(failed), || panic!("JSON numa forma inesperada"));
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Msg::Queue { req: 7, result: Err(error) }) => assert_eq!(error, "falhou"),
            Ok(_) => panic!("outra resposta"),
            Err(error) => panic!("sem resposta: {error}"),
        }
    }

    /// Um motor sem rodar o laço (`run` instalaria o Deno): só o estado.
    fn engine(name: &str) -> Engine {
        let dir = std::env::temp_dir().join(format!("musicfast-teste-{name}-{}", std::process::id()));
        let (tx, rx) = sync_channel(16);
        let config = Config {
            bin_dir: dir.join("bin"),
            cache_file: dir.join("enderecos.json"),
            client: Arc::new(Client::new("pt", "BR").unwrap()),
            volume: 1.0,
        };
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        Engine::new(config, rx, tx, snapshot, Arc::new(AtomicU64::new(0)), Arc::new(Shared::default()), Arc::new(|| {}))
    }

    #[test]
    fn a_panic_in_a_message_leaves_the_player_answering() {
        let mut e = engine("panico");
        // Incoerente de propósito: com o índice fora da fila, `prepare_next`
        // lê além do fim e entra em pânico.
        e.index = Some(5);
        e.playing = Some(("x".into(), 1));
        e.want = Some(Want { video_id: "y".into(), token: 2, start_at: 0.0, autoplay: true });
        e.resolving.insert("y".into());
        e.more_inflight = true;
        assert!(!e.turn(Some(Msg::Cmd(Command::SetRepeat(Repeat::One)))), "o pânico não aconteceu");
        assert!(e.want.is_none() && e.resolving.is_empty() && !e.more_inflight);
        // A mensagem seguinte é atendida e chega à tela.
        assert!(e.turn(Some(Msg::Cmd(Command::SetVolume(0.5)))));
        assert_eq!(e.snapshot.lock().unwrap().volume, 0.5);
    }

    #[test]
    fn what_was_loading_stops_after_a_panic() {
        let mut e = engine("carregando");
        e.queue = Arc::new(vec![Track { video_id: "a".into(), ..Track::default() }]);
        e.index = Some(0);
        e.status = Status::Loading;
        e.step = Some("Buscando o áudio");
        e.recover();
        assert_eq!(e.status, Status::Paused);
        assert_eq!(e.snapshot.lock().unwrap().status, Status::Paused);
    }

    fn song(id: &str, mb: usize) -> (String, Audio) {
        (id.to_owned(), Audio::from(vec![0u8; mb << 20]))
    }

    #[test]
    fn common_songs_keep_four_and_long_mixes_keep_two() {
        let mut kept: VecDeque<_> = ["a", "b", "c", "d", "e"].map(|id| song(id, 4)).into();
        trim_kept(&mut kept);
        assert_eq!(kept.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["b", "c", "d", "e"]);

        // Mixes de 1 h (~58 MB): só a atual e a próxima, mesmo acima do teto.
        let mut kept: VecDeque<_> = ["a", "b", "c"].map(|id| song(id, 58)).into();
        trim_kept(&mut kept);
        assert_eq!(kept.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["b", "c"]);
    }
}
