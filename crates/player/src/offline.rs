//! As músicas baixadas para ouvir sem internet.
//!
//! Cada uma vira um `.m4a` comum (`m4a::remux`, com capa e etiquetas) na
//! pasta de baixadas (a padrão do app, ou a que a pessoa escolheu), com o
//! nome "Artista - Título.m4a": é esse arquivo que a pessoa arrasta para o
//! Premiere, o Explorer ou um chat. O índice (`baixadas.json`, na pasta do
//! app) guarda a música do YouTube e a pasta de cada arquivo: trocar a
//! pasta vale para os próximos downloads, e os de antes seguem onde estão.
//! Ele serve à página "Baixadas" e ao player tocar do disco
//! (`Engine::need` pergunta aqui antes de chamar o yt-dlp).
//!
//! Os downloads correm em até `WORKERS` threads, fora do player: resolver
//! (yt-dlp, ~3 s), baixar (com progresso), reembalar e gravar.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use musicfast_ytm::{Thumb, Thumbs, Track};
use serde::{Deserialize, Serialize};

use crate::m4a::{self, Tags};
use crate::source::{FetchError, fetch_with, http_client};
use crate::ytdlp::{self, Account, YtDlp};

const INDEX: &str = "baixadas.json";
const WORKERS: usize = 2;
/// A origem das capas guardadas em disco (`Thumb::url`): quem lê as capas
/// (`covers`, `jobs::art`) tira o prefixo e abre o arquivo.
pub const LOCAL_PREFIX: &str = "file:";

/// Onde um download está.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Queued,
    Resolving,
    /// A fração baixada, quando o tamanho é conhecido.
    Downloading(Option<f32>),
    Saving,
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    None,
    Working(Step),
    Failed(String),
    Done(PathBuf),
}

/// Uma música baixada.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    /// A música como veio do YouTube (as capas originais, os links).
    pub track: Track,
    /// O nome do arquivo na pasta de baixadas.
    pub file: String,
    pub bytes: u64,
    /// Segundos Unix.
    pub added: u64,
    /// O nome do arquivo da capa na pasta de capas, se veio.
    #[serde(default)]
    pub cover: Option<String>,
    /// A pasta do arquivo (`None`: a pasta padrão).
    #[serde(default)]
    pub folder: Option<PathBuf>,
}

#[derive(Default)]
struct Inner {
    entries: Arc<Vec<Entry>>,
    jobs: HashMap<String, Step>,
    failed: HashMap<String, String>,
    pending: VecDeque<Track>,
    workers: usize,
    account: Option<Arc<str>>,
    /// A pasta escolhida pela pessoa (`None`: a padrão).
    chosen: Option<PathBuf>,
    revision: u64,
}

/// Onde as baixadas moram.
pub struct Folders {
    /// O índice (`baixadas.json`).
    pub index: PathBuf,
    /// A pasta padrão dos `.m4a`.
    pub default_dir: PathBuf,
    /// As capas (fora da pasta que a pessoa vê).
    pub covers: PathBuf,
    /// O yt-dlp gerenciado.
    pub bin: PathBuf,
    /// Onde o arquivo temporário de cookies da conta é escrito.
    pub cookies: PathBuf,
}

pub struct Offline {
    index: PathBuf,
    default_dir: PathBuf,
    covers: PathBuf,
    bin_dir: PathBuf,
    cookie_dir: PathBuf,
    inner: Mutex<Inner>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl Offline {
    /// `chosen`: a pasta que a pessoa escolheu para os downloads novos.
    pub fn open(folders: Folders, chosen: Option<PathBuf>, notify: Arc<dyn Fn() + Send + Sync>) -> Arc<Self> {
        let read = |path: &Path| -> Option<Vec<Entry>> { serde_json::from_slice(&std::fs::read(path).ok()?).ok() };
        // As primeiras versões guardavam o índice dentro da pasta padrão.
        let mut entries = read(&folders.index).or_else(|| read(&folders.default_dir.join(INDEX))).unwrap_or_default();
        let Folders { index, default_dir, covers, bin, cookies } = folders;
        let me = Arc::new(Self { index, default_dir, covers, bin_dir: bin, cookie_dir: cookies, inner: Mutex::new(Inner::default()), notify });
        // O arquivo apagado pelo Explorer sai da lista.
        let before = entries.len();
        entries.retain(|e| me.file_of(e).is_file());
        let dropped = entries.len() != before || !me.index.is_file();
        {
            let mut inner = me.lock();
            inner.entries = Arc::new(entries);
            inner.chosen = chosen;
        }
        if dropped {
            me.save();
        }
        me
    }

    /// O caminho do arquivo de uma baixada.
    fn file_of(&self, e: &Entry) -> PathBuf {
        e.folder.as_deref().unwrap_or(&self.default_dir).join(&e.file)
    }

    /// A pasta padrão (a do app).
    pub fn default_dir(&self) -> &Path {
        &self.default_dir
    }

    /// Para onde vão os próximos downloads (`None`: a pasta padrão). Os já
    /// baixados ficam onde estão.
    pub fn set_dir(&self, chosen: Option<PathBuf>) {
        let mut inner = self.lock();
        inner.chosen = chosen;
        inner.revision += 1;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A pasta dos próximos downloads.
    pub fn dir(&self) -> PathBuf {
        self.lock().chosen.clone().unwrap_or_else(|| self.default_dir.clone())
    }

    /// A pessoa escolheu uma pasta (não é a padrão).
    pub fn has_chosen_dir(&self) -> bool {
        self.lock().chosen.is_some()
    }

    /// Muda a cada download, falha ou exclusão.
    pub fn revision(&self) -> u64 {
        self.lock().revision
    }

    /// As baixadas, da mais nova para a mais antiga.
    pub fn entries(&self) -> Arc<Vec<Entry>> {
        self.lock().entries.clone()
    }

    /// As músicas para tocar e desenhar, com a capa do disco no lugar da
    /// da internet (sem rede, a capa ainda aparece).
    pub fn tracks(&self) -> Vec<Track> {
        self.entries().iter().map(|e| self.local_track(e)).collect()
    }

    fn local_track(&self, e: &Entry) -> Track {
        let mut track = e.track.clone();
        if let Some(cover) = &e.cover {
            let path = self.covers.join(cover);
            track.thumbs = Thumbs(vec![Thumb { url: format!("{LOCAL_PREFIX}{}", path.display()), width: 544 }]);
        }
        track
    }

    pub fn state(&self, video_id: &str) -> State {
        let inner = self.lock();
        if let Some(step) = inner.jobs.get(video_id) {
            return State::Working(*step);
        }
        if let Some(e) = inner.entries.iter().find(|e| e.track.video_id == video_id) {
            return State::Done(self.file_of(e));
        }
        match inner.failed.get(video_id) {
            Some(error) => State::Failed(error.clone()),
            None => State::None,
        }
    }

    /// O arquivo baixado da música, se ainda está no disco (o player toca
    /// dele em vez de ir à internet).
    pub fn path_of(&self, video_id: &str) -> Option<PathBuf> {
        let path = self.lock().entries.iter().find(|e| e.track.video_id == video_id).map(|e| self.file_of(e))?;
        path.is_file().then_some(path)
    }

    /// Os cookies da conta (`None` ao sair): o download sai na qualidade
    /// dela (256 kbps no Premium), como o player.
    pub fn set_account(&self, cookie: Option<String>) {
        self.lock().account = cookie.map(Into::into);
    }

    /// Põe a música na fila de downloads (nada, se já está baixada ou indo).
    pub fn download(self: &Arc<Self>, track: Track) {
        let spawn = {
            let mut inner = self.lock();
            let id = &track.video_id;
            if id.is_empty() || inner.jobs.contains_key(id) || inner.entries.iter().any(|e| e.track.video_id == *id) {
                return;
            }
            inner.failed.remove(id);
            inner.jobs.insert(id.clone(), Step::Queued);
            inner.pending.push_back(track);
            inner.revision += 1;
            let spawn = inner.workers < WORKERS;
            if spawn {
                inner.workers += 1;
            }
            spawn
        };
        (self.notify)();
        if spawn {
            let me = self.clone();
            let started = std::thread::Builder::new().name("baixar".into()).spawn(move || me.worker());
            if let Err(error) = started {
                log::warn!("thread de download: {error}");
                self.lock().workers -= 1;
            }
        }
    }

    /// Apaga o arquivo e a capa. O arquivo aberto em outro programa (o
    /// Premiere segura o que importou) não sai, e a música fica na lista.
    pub fn delete(&self, video_id: &str) -> Result<(), String> {
        let entry = self.lock().entries.iter().find(|e| e.track.video_id == video_id).cloned();
        let Some(entry) = entry else { return Ok(()) };
        match std::fs::remove_file(self.file_of(&entry)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        if let Some(cover) = &entry.cover {
            let _ = std::fs::remove_file(self.covers.join(cover));
        }
        {
            let mut inner = self.lock();
            Arc::make_mut(&mut inner.entries).retain(|e| e.track.video_id != video_id);
            inner.revision += 1;
        }
        self.save();
        (self.notify)();
        Ok(())
    }

    /// Esquece o erro de um download que falhou.
    pub fn dismiss(&self, video_id: &str) {
        let mut inner = self.lock();
        if inner.failed.remove(video_id).is_some() {
            inner.revision += 1;
        }
    }

    fn worker(self: Arc<Self>) {
        loop {
            let track = {
                let mut inner = self.lock();
                match inner.pending.pop_front() {
                    Some(t) => t,
                    None => {
                        inner.workers -= 1;
                        return;
                    }
                }
            };
            let id = track.video_id.clone();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.job(&track)))
                .unwrap_or_else(|_| Err("erro interno no download".into()));
            {
                let mut inner = self.lock();
                inner.jobs.remove(&id);
                inner.revision += 1;
                match result {
                    Ok(entry) => {
                        log::info!("baixada: {} ({} kB)", entry.file, entry.bytes / 1024);
                        let entries = Arc::make_mut(&mut inner.entries);
                        entries.retain(|e| e.track.video_id != id);
                        entries.insert(0, entry);
                    }
                    Err(error) => {
                        log::warn!("download de {id} falhou: {error}");
                        inner.failed.insert(id, error);
                    }
                }
            }
            self.save();
            (self.notify)();
        }
    }

    fn set(&self, video_id: &str, step: Step) {
        let changed = {
            let mut inner = self.lock();
            let changed = inner.jobs.get(video_id).is_some_and(|s| *s != step);
            if changed {
                inner.jobs.insert(video_id.to_owned(), step);
                inner.revision += 1;
            }
            changed
        };
        if changed {
            (self.notify)();
        }
    }

    fn job(&self, track: &Track) -> Result<Entry, String> {
        let t = musicfast_i18n::t();
        let id = track.video_id.as_str();
        self.set(id, Step::Resolving);
        let ytdlp = match YtDlp::locate(&self.bin_dir) {
            Some(y) => y,
            None => {
                let _ = std::fs::create_dir_all(&self.bin_dir);
                ytdlp::install(&self.bin_dir).map_err(|e| t.ytdlp_install_failed(&e))?
            }
        };
        let account = self.lock().account.clone().map(|cookie| Account { cookie, dir: self.cookie_dir.clone() });
        let http = http_client();
        let mut retried = false;
        let audio = loop {
            let resolved = ytdlp.resolve(id, account.as_ref()).map_err(|(error, _)| error)?;
            self.set(id, Step::Downloading(None));
            let mut progress = |got: u64, total: Option<u64>| {
                let fraction = total.filter(|&n| n > 0).map(|n| (got as f64 / n as f64).min(1.0) as f32);
                // Em degraus de 2%: não acorda a tela a cada pedaço.
                let fraction = fraction.map(|f| (f * 50.0).floor() / 50.0);
                self.set(id, Step::Downloading(fraction));
            };
            match fetch_with(&http, &resolved, &mut progress) {
                Ok(audio) => break audio,
                Err(FetchError::Expired) if !retried => {
                    retried = true;
                    self.set(id, Step::Resolving);
                }
                Err(FetchError::Expired) => return Err(t.audio_url_refused.into()),
                Err(FetchError::Other(error)) => return Err(error),
            }
        };
        self.set(id, Step::Saving);
        let cover = track
            .thumbs
            .url_for(544)
            .and_then(|url| http.get(url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).ok())
            .map(|b| b.to_vec())
            .filter(|b| m4a::is_cover_image(b));
        let album = track.album.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let tags = Tags { title: track.title.clone(), artist: track.artist_names(), album, cover: cover.clone() };
        let bytes = m4a::remux(audio.into_vec(), &tags)?;

        let dir = self.dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = self.free_name(&dir, &file_name(track));
        let path = dir.join(&file);
        let part = dir.join(format!("{file}.part"));
        std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&part, &path).map_err(|e| {
            let _ = std::fs::remove_file(&part);
            e.to_string()
        })?;
        let cover = cover.and_then(|img| {
            let ext = if img.starts_with(&[0x89, b'P']) { "png" } else { "jpg" };
            let name = format!("{id}.{ext}");
            let _ = std::fs::create_dir_all(&self.covers);
            std::fs::write(self.covers.join(&name), img).ok().map(|_| name)
        });
        let mut track = track.clone();
        // Sem a capa local no índice: ela é posta na leitura (`local_track`).
        track.thumbs.compact();
        let folder = (dir != self.default_dir).then_some(dir);
        Ok(Entry { track, file, bytes: bytes.len() as u64, added: ytdlp::now(), cover, folder })
    }

    /// `nome.m4a`, ou `nome (2).m4a` se já existe outro arquivo com ele.
    fn free_name(&self, dir: &Path, wanted: &str) -> String {
        let stem = wanted.strip_suffix(".m4a").unwrap_or(wanted);
        let taken = |name: &str| {
            let path = dir.join(name);
            path.exists() || self.lock().entries.iter().any(|e| self.file_of(e).to_string_lossy().eq_ignore_ascii_case(&path.to_string_lossy()))
        };
        if !taken(wanted) {
            return wanted.to_owned();
        }
        (2..).map(|n| format!("{stem} ({n}).m4a")).find(|name| !taken(name)).expect("nome livre")
    }

    fn save(&self) {
        let entries = self.entries();
        let Ok(json) = serde_json::to_vec_pretty(&*entries) else { return };
        if let Some(parent) = self.index.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let path = &self.index;
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, path)).is_err() {
            log::warn!("índice das baixadas não gravou em {}", path.display());
        }
    }
}

/// "Artista - Título.m4a", sem o que o Windows não aceita num nome.
pub fn file_name(track: &Track) -> String {
    let artist = track.artist_names();
    let title = if track.title.trim().is_empty() { track.video_id.as_str() } else { track.title.as_str() };
    let raw = if artist.trim().is_empty() { title.to_owned() } else { format!("{artist} - {title}") };
    let mut name: String = raw
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c })
        .collect();
    // Até 120 caracteres (o caminho inteiro do Windows tem limite).
    if let Some((cut, _)) = name.char_indices().nth(120) {
        name.truncate(cut);
    }
    let name = name.trim().trim_end_matches(['.', ' ']).to_owned();
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"];
    let name = if name.is_empty() || reserved.iter().any(|r| name.eq_ignore_ascii_case(r)) { format!("{name}_{}", track.video_id) } else { name };
    format!("{name}.m4a")
}

#[cfg(test)]
mod tests {
    use super::*;
    use musicfast_ytm::Link;

    fn track(title: &str, artist: &str) -> Track {
        Track {
            video_id: "abc123".into(),
            title: title.into(),
            artists: vec![Link { name: artist.into(), id: None }],
            ..Track::default()
        }
    }

    #[test]
    fn file_names_are_safe_for_windows() {
        assert_eq!(file_name(&track("Garota de Ipanema", "Tom Jobim")), "Tom Jobim - Garota de Ipanema.m4a");
        assert_eq!(file_name(&track("AC/DC: Live?", "")), "AC_DC_ Live_.m4a");
        assert_eq!(file_name(&track("con", "")), "con_abc123.m4a");
        assert_eq!(file_name(&track("Fim...", "")), "Fim.m4a");
        let long = "a".repeat(300);
        assert!(file_name(&track(&long, "")).chars().count() <= 124);
    }

    #[test]
    fn index_survives_reopen_and_drops_missing_files() {
        let dir = std::env::temp_dir().join(format!("musicfast-baixadas-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("A - B.m4a"), b"x").unwrap();
        let entries = vec![
            Entry { track: track("B", "A"), file: "A - B.m4a".into(), bytes: 1, added: 1, cover: None, folder: None },
            Entry { track: Track { video_id: "sumiu".into(), ..Track::default() }, file: "sumiu.m4a".into(), bytes: 1, added: 1, cover: None, folder: None },
        ];
        // O índice antigo, dentro da pasta padrão: é lido e passa para o novo lugar.
        std::fs::write(dir.join(INDEX), serde_json::to_vec(&entries).unwrap()).unwrap();
        let folders = || Folders { index: dir.join("indice.json"), default_dir: dir.clone(), covers: dir.join("capas"), bin: dir.join("bin"), cookies: dir.join("tmp") };
        let offline = Offline::open(folders(), None, Arc::new(|| {}));
        assert!(dir.join("indice.json").is_file());
        assert_eq!(offline.entries().len(), 1);
        assert_eq!(offline.path_of("abc123"), Some(dir.join("A - B.m4a")));
        assert!(offline.path_of("sumiu").is_none());
        assert_eq!(offline.free_name(&dir, "A - B.m4a"), "A - B (2).m4a");
        // Outra pasta: os próximos vão para ela, a antiga segue valendo.
        let other = dir.join("outra");
        offline.set_dir(Some(other.clone()));
        assert_eq!(offline.dir(), other);
        assert_eq!(offline.free_name(&other, "A - B.m4a"), "A - B.m4a");
        assert_eq!(offline.path_of("abc123"), Some(dir.join("A - B.m4a")));
        offline.delete("abc123").unwrap();
        assert!(!dir.join("A - B.m4a").exists());
        assert_eq!(offline.state("abc123"), State::None);
        let reopened = Offline::open(folders(), None, Arc::new(|| {}));
        assert!(reopened.entries().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Um download de verdade, do clique ao arquivo com capa:
/// `MUSICFAST_BIN=<pasta do yt-dlp> MUSICFAST_OUT=<pasta vazia> cargo test
/// -p musicfast-player -- --ignored baixar_live`.
#[cfg(test)]
#[test]
#[ignore]
fn baixar_live() {
    let bin = PathBuf::from(std::env::var_os("MUSICFAST_BIN").expect("MUSICFAST_BIN"));
    let out = PathBuf::from(std::env::var_os("MUSICFAST_OUT").expect("MUSICFAST_OUT"));
    let (tx, rx) = std::sync::mpsc::channel();
    let tx = Mutex::new(tx);
    let folders = Folders { index: out.join("baixadas.json"), default_dir: out.join("padrao"), covers: out.join("capas"), bin, cookies: out.join("tmp") };
    let offline = Offline::open(folders, Some(out.join("baixadas")), Arc::new(move || {
        let _ = tx.lock().unwrap().send(());
    }));
    let track = Track {
        video_id: "djV11Xbc914".into(),
        title: "Take On Me".into(),
        artists: vec![musicfast_ytm::Link { name: "a-ha".into(), id: None }],
        thumbs: Thumbs(vec![Thumb { url: "https://i.ytimg.com/vi/djV11Xbc914/hqdefault.jpg".into(), width: 480 }]),
        ..Track::default()
    };
    offline.download(track);
    let mut steps = Vec::new();
    let path = loop {
        rx.recv_timeout(std::time::Duration::from_secs(120)).expect("sem notícia do download");
        match offline.state("djV11Xbc914") {
            State::Working(step) => steps.push(step),
            State::Done(path) => break path,
            State::Failed(error) => panic!("{error}"),
            State::None => {}
        }
    };
    assert!(steps.iter().any(|s| matches!(s, Step::Downloading(Some(f)) if *f > 0.0 && *f < 1.0)), "{steps:?}");
    assert_eq!(path, out.join("baixadas").join("a-ha - Take On Me.m4a"));
    assert!(offline.entries()[0].cover.is_some());
    assert!(offline.tracks()[0].thumbs.0[0].url.starts_with(LOCAL_PREFIX));
}
