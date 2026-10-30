//! O endereço do áudio de uma música, pelo yt-dlp.
//!
//! O InnerTube devolve o catálogo sem esforço, mas o endereço do áudio exige
//! resolver o desafio de JavaScript do player do YouTube (o yt-dlp usa o
//! Deno quando existe). Reescrever isso seria ter duas coisas quebrando na
//! mesma noite quando o YouTube mudar (decisão do PrimeDrop, memória
//! youtube-via-ytdlp). O yt-dlp é a fonte única.
//!
//! **Medido em 28/10/2026, nesta máquina:** ~1 s só de abrir o yt-dlp (a
//! versão em pasta, `yt-dlp_win.zip`; o executável de arquivo único levou
//! 3,9 s para `--version`, porque se descompacta a cada chamada) e ~2,4 s de
//! rede por música. Juntar várias num processo só poupa a abertura. Pular a
//! página (`player_skip=webpage`) faz o YouTube pedir login.
//!
//! **O cliente importa (29/10/2026).** O padrão consulta `tv_samsung` e
//! `visionos` (~4 s) e entrega o endereço da TV, que leva 403 até o
//! `available_at` — 4 a 6 s no futuro (o yt-dlp dorme esse tempo antes de
//! baixar). Só `visionos`: 2,6–3,0 s e o endereço vale na hora. Por isso ele
//! vem primeiro, e o padrão é a reserva; o download respeita o
//! `available_at` nos dois casos.
//!
//! O resto da velocidade vem de resolver ANTES de precisar (a próxima da
//! fila, o que está sob o mouse) e de guardar o resultado até ele vencer
//! (~6 h, o `expire` do endereço).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPO: &str = "yt-dlp/yt-dlp";
/// A versão em pasta (abre rápido) de cada sistema, e o executável dentro
/// dela. `PATH_EXE` é o nome de um yt-dlp instalado pelo sistema.
#[cfg(windows)]
const ZIP: &str = "yt-dlp_win.zip";
#[cfg(windows)]
const EXE: &str = "yt-dlp.exe";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const ZIP: &str = "yt-dlp_linux_aarch64.zip";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const EXE: &str = "yt-dlp_linux_aarch64";
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const ZIP: &str = "yt-dlp_linux.zip";
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const EXE: &str = "yt-dlp_linux";
#[cfg(target_os = "macos")]
const ZIP: &str = "yt-dlp_macos.zip";
#[cfg(target_os = "macos")]
const EXE: &str = "yt-dlp_macos";
#[cfg(windows)]
const PATH_EXE: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const PATH_EXE: &str = "yt-dlp";
/// Dentro da pasta do yt-dlp: a versão (tag da release) instalada, e o
/// arquivo cuja data marca a última consulta ao GitHub.
const VERSION_FILE: &str = "versao.txt";
const CHECKED_FILE: &str = "verificado";
/// O YouTube muda o site sem aviso e o yt-dlp corrige em dias: uma consulta
/// por dia pega a correção antes de a pessoa notar.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);

/// O Deno resolve o desafio de JavaScript do player do YouTube para o
/// yt-dlp. Sem ele, só os clientes que dispensam o desafio funcionam (o
/// `visionos` de hoje); com ele, todos. Fica em `<bin>/deno` quando a
/// máquina não tem um no PATH.
const DENO_REPO: &str = "denoland/deno";
#[cfg(windows)]
const DENO_ZIP: &str = "deno-x86_64-pc-windows-msvc.zip";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const DENO_ZIP: &str = "deno-aarch64-unknown-linux-gnu.zip";
#[cfg(all(target_os = "linux", not(target_arch = "aarch64")))]
const DENO_ZIP: &str = "deno-x86_64-unknown-linux-gnu.zip";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const DENO_ZIP: &str = "deno-aarch64-apple-darwin.zip";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const DENO_ZIP: &str = "deno-x86_64-apple-darwin.zip";
#[cfg(windows)]
const DENO_EXE: &str = "deno.exe";
#[cfg(not(windows))]
const DENO_EXE: &str = "deno";

/// O AAC de 128 kbps (formato 140) é o que o Symphonia decodifica; o Opus
/// (251) teria qualidade parecida e exigiria uma biblioteca em C.
const FORMAT: &str = "140/bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]";
const FAST_CLIENT: &str = "visionos";
/// Com a conta: o AAC de 256 kbps (141), que o YouTube só entrega a quem tem
/// Premium; sem Premium ele não vem e fica o de 128 kbps de sempre.
const ACCOUNT_FORMAT: &str = "141/140/bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]";
/// O `visionos` ignora cookies (medido em 29/10/2026: um cookie falso não
/// muda nada nele); o `tv` os usa.
const ACCOUNT_CLIENT: &str = "tv";

/// A conta para o yt-dlp: o cabeçalho de cookies e a pasta (do usuário,
/// fora do log) onde o arquivo temporário de cookies é escrito.
#[derive(Clone)]
pub struct Account {
    pub cookie: Arc<str>,
    pub dir: PathBuf,
}

/// O arquivo de cookies no formato Netscape que o `--cookies` lê. Vive só
/// durante uma chamada do yt-dlp e é apagado ao sair do escopo, até em erro
/// (o yt-dlp ainda regrava o arquivo ao terminar, então ele não pode ser
/// reaproveitado nem ficar para trás).
struct CookieFile(PathBuf);

impl CookieFile {
    fn write(account: &Account) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(&account.dir).map_err(|e| e.to_string())?;
        let name = format!("cookies-{}-{}.txt", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let path = account.dir.join(name);
        std::fs::write(&path, netscape(&account.cookie)).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for CookieFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Apaga arquivos de cookies que uma execução interrompida (queda, energia)
/// deixou para trás. Chamado ao abrir o player.
pub fn clear_cookie_files(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("cookies-") && name.ends_with(".txt") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `a=1; b=2` no formato Netscape, para o domínio do YouTube. Os
/// `__Host-` não servem a outro host e ficam de fora.
fn netscape(header: &str) -> String {
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    let expires = now() + 365 * 24 * 3600;
    for pair in header.split(';') {
        let Some((name, value)) = pair.trim().split_once('=') else { continue };
        let (name, value) = (name.trim(), value.trim());
        if name.is_empty() || name.starts_with("__Host-") || name.contains(['\t', '\n']) || value.contains(['\t', '\n']) {
            continue;
        }
        out.push_str(&format!(".youtube.com\tTRUE\t/\tTRUE\t{expires}\t{name}\t{value}\n"));
    }
    out
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Resolved {
    pub video_id: String,
    pub url: String,
    pub size: Option<u64>,
    pub duration: Option<f64>,
    pub user_agent: Option<String>,
    /// Segundos Unix em que o endereço vence (o `expire` dele).
    pub expires_at: u64,
    /// Antes disso o servidor responde 403 (endereço do cliente de TV).
    #[serde(default)]
    pub available_at: Option<u64>,
    /// O formato que veio (141 é o de 256 kbps do Premium).
    #[serde(default)]
    pub format_id: Option<String>,
}

impl Resolved {
    /// Ainda serve? Com folga para tocar a música inteira.
    pub fn fresh(&self, now: u64) -> bool {
        self.expires_at > now + 15 * 60
    }
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then_some(v)
    })
}

/// O que o `--print` devolve.
#[derive(Deserialize)]
struct Printed {
    id: String,
    url: String,
    filesize: Option<u64>,
    duration: Option<f64>,
    #[serde(default)]
    http_headers: HashMap<String, String>,
    available_at: Option<u64>,
    format_id: Option<String>,
}

pub fn parse_printed(line: &str) -> Result<Resolved, String> {
    let p: Printed = serde_json::from_str(line.trim()).map_err(|e| format!("saída do yt-dlp ilegível: {e}"))?;
    let expires_at = query_param(&p.url, "expire")
        .and_then(|e| e.parse().ok())
        .unwrap_or_else(|| now() + 3600);
    let size = p.filesize.or_else(|| query_param(&p.url, "clen").and_then(|c| c.parse().ok()));
    Ok(Resolved {
        video_id: p.id,
        url: p.url,
        size,
        duration: p.duration,
        user_agent: p.http_headers.get("User-Agent").cloned(),
        expires_at,
        available_at: p.available_at,
        format_id: p.format_id,
    })
}

/// A mensagem do yt-dlp em português, para a tela. A original vai ao log.
pub fn explain(stderr: &str) -> String {
    let last = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with("ERROR:"))
        .or_else(|| stderr.lines().rev().map(str::trim).find(|l| !l.is_empty()))
        .unwrap_or("");
    let lower = last.to_lowercase();
    if lower.contains("sign in to confirm") || lower.contains("not a bot") {
        "o YouTube pediu para confirmar que não é um robô".into()
    } else if lower.contains("video unavailable") || lower.contains("private video") || lower.contains("removed") {
        "música indisponível no YouTube".into()
    } else if lower.contains("requested format is not available") {
        "o YouTube não ofereceu áudio AAC para esta música".into()
    } else if lower.contains("age") && lower.contains("confirm") {
        "música com restrição de idade (precisa de login)".into()
    } else if lower.contains("unable to download") || lower.contains("timed out") || lower.contains("getaddrinfo") {
        "sem conexão com o YouTube".into()
    } else if last.is_empty() {
        "o yt-dlp falhou sem dizer por quê".into()
    } else {
        last.trim_start_matches("ERROR:").trim().chars().take(160).collect()
    }
}

/// O erro sugere que o yt-dlp envelheceu (o YouTube mudou o site)?
pub fn looks_outdated(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    // O "não é um robô" e o "formato indisponível" também aparecem quando o
    // YouTube muda o desafio; a atualização só baixa se houver versão nova.
    [
        "unable to extract",
        "please report this issue",
        "update to the latest",
        "nsig extraction failed",
        "signature extraction failed",
        "requested format is not available",
        "not a bot",
        "challenge",
    ]
        .iter()
        .any(|k| lower.contains(k))
}

pub struct YtDlp {
    pub exe: PathBuf,
    /// A pasta do Deno gerenciado (pode ainda não existir).
    deno_dir: PathBuf,
}

impl YtDlp {
    /// A cópia gerenciada (pasta, abre rápido) antes do PATH.
    pub fn locate(managed: &Path) -> Option<Self> {
        let deno_dir = managed.join("deno");
        let own = managed.join("yt-dlp").join(EXE);
        if own.is_file() {
            return Some(Self { exe: own, deno_dir });
        }
        on_path(PATH_EXE).map(|exe| Self { exe, deno_dir })
    }

    pub fn is_managed(&self, managed: &Path) -> bool {
        self.exe.starts_with(managed)
    }

    /// O cliente `visionos` primeiro (rápido, sem espera); se ele falhar, o
    /// padrão do yt-dlp. A reserva é um caminho testado, não um palpite:
    /// `ytdlp_live` no fim deste arquivo roda os dois.
    ///
    /// Com a conta, ela vai na frente: dá o áudio de 256 kbps a quem tem
    /// Premium e abre as músicas com restrição de idade. Se a conta falhar
    /// (sessão vencida, o cliente `tv` mudou), segue o caminho anônimo.
    pub fn resolve(&self, video_id: &str, account: Option<&Account>) -> Result<Resolved, (String, bool)> {
        if let Some(account) = account {
            match CookieFile::write(account) {
                Ok(file) => match self.run(video_id, Some(ACCOUNT_CLIENT), ACCOUNT_FORMAT, Some(&file.0)) {
                    Ok(r) => return Ok(r),
                    Err((error, outdated)) if error.contains("indisponível") => return Err((error, outdated)),
                    Err((error, _)) => log::info!("{video_id}: com a conta falhou ({error}); tentando sem"),
                },
                Err(error) => log::warn!("arquivo de cookies do yt-dlp: {error}"),
            }
        }
        match self.resolve_with(video_id, Some(FAST_CLIENT)) {
            Ok(r) => Ok(r),
            Err((error, outdated)) if error.contains("indisponível") => Err((error, outdated)),
            Err((error, _)) => {
                log::info!("{video_id}: o cliente {FAST_CLIENT} falhou ({error}); tentando o padrão");
                self.resolve_with(video_id, None)
            }
        }
    }

    pub fn resolve_with(&self, video_id: &str, client: Option<&str>) -> Result<Resolved, (String, bool)> {
        self.run(video_id, client, FORMAT, None)
    }

    fn run(&self, video_id: &str, client: Option<&str>, format: &str, cookies: Option<&Path>) -> Result<Resolved, (String, bool)> {
        let started = std::time::Instant::now();
        let mut cmd = Command::new(&self.exe);
        // O yt-dlp procura o Deno no PATH: o gerenciado vai na frente.
        if self.deno_dir.join(DENO_EXE).is_file() {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let dirs = std::iter::once(self.deno_dir.clone()).chain(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(dirs) {
                cmd.env("PATH", joined);
            }
        }
        if let Some(client) = client {
            cmd.args(["--extractor-args", &format!("youtube:player_client={client}")]);
        }
        if let Some(file) = cookies {
            cmd.arg("--cookies").arg(file);
        }
        cmd.args([
            "--no-warnings",
            "--no-playlist",
            "--skip-download",
            "--no-progress",
            // O endereço vale só para o IP que o pediu; o download também
            // sai por IPv4 (nesta máquina o IPv6 nem conecta).
            "--force-ipv4",
            "-f",
            format,
            "--print",
            "%(.{id,url,filesize,duration,http_headers,available_at,format_id})j",
            "--",
        ]);
        cmd.arg(format!("https://music.youtube.com/watch?v={video_id}"));
        let out = crate::job::output(cmd).map_err(|e| (format!("o yt-dlp não abriu: {e}"), false))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let line = stdout.lines().rev().find(|l| l.trim_start().starts_with('{'));
        match (out.status.success(), line) {
            (true, Some(line)) => {
                let resolved = parse_printed(line).map_err(|e| (e, false))?;
                log::info!(
                    "resolvido {video_id} ({}{}, formato {}) em {} ms",
                    client.unwrap_or("padrão"),
                    if cookies.is_some() { " com a conta" } else { "" },
                    resolved.format_id.as_deref().unwrap_or("?"),
                    started.elapsed().as_millis()
                );
                Ok(resolved)
            }
            _ => {
                log::warn!("yt-dlp falhou em {video_id}: {}", stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | "));
                Err((explain(&stderr), looks_outdated(&stderr)))
            }
        }
    }
}

fn http() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("musicfast")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// O hash do arquivo no `SHA2-256SUMS` da release.
pub fn checksum_for<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == file && hash.len() == 64).then_some(hash)
    })
}

/// O hash no `.sha256sum` do Deno, que é a saída do `Get-FileHash` do
/// PowerShell ("Hash : CBA8…", em maiúsculas): o único token de 64 dígitos
/// hexadecimais do arquivo.
pub fn single_checksum(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_hexdigit())
        .find(|t| t.len() == 64)
        .map(str::to_ascii_lowercase)
}

fn on_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(exe)).find(|p| p.is_file())
}

/// A tag da última release (`2026.10.26`, `v2.9.8`): o GitHub redireciona
/// `/releases/latest` para `/releases/tag/<tag>`, sem gastar a cota da API.
pub fn latest_tag(http: &reqwest::blocking::Client, repo: &str) -> Result<String, String> {
    let resp = http
        .head(format!("https://github.com/{repo}/releases/latest"))
        .send()
        .map_err(|e| format!("sem conexão com o GitHub: {e}"))?;
    tag_of(resp.url().as_str()).ok_or_else(|| format!("o GitHub não disse a última versão de {repo} ({})", resp.url()))
}

pub fn tag_of(url: &str) -> Option<String> {
    let (_, tag) = url.split_once("/releases/tag/")?;
    let tag = tag.split(['?', '#', '/']).next()?;
    (!tag.is_empty()).then(|| tag.to_owned())
}

fn get(http: &reqwest::blocking::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = http.get(url).send().map_err(|e| format!("sem conexão com o GitHub: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("o GitHub respondeu {} para {url}", resp.status()));
    }
    let mut buf = Vec::new();
    resp.take(200 * 1024 * 1024).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Extrai o zip numa pasta nova e troca a pasta `name` inteira por ela. Se
/// a antiga estiver em uso (um yt-dlp rodando), nada muda.
fn swap_in(managed: &Path, name: &str, zip: &[u8], exe: &str) -> Result<PathBuf, String> {
    let target = managed.join(name);
    let fresh = managed.join(format!("{name}.novo"));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).map_err(|e| e.to_string())?;
    extract(zip, &fresh)?;
    if !fresh.join(exe).is_file() {
        let _ = std::fs::remove_dir_all(&fresh);
        return Err(format!("o zip não tem o {exe}"));
    }
    let old = managed.join(format!("{name}.velho"));
    let _ = std::fs::remove_dir_all(&old);
    if target.exists() {
        std::fs::rename(&target, &old).map_err(|e| format!("a pasta {name} está em uso: {e}"))?;
    }
    std::fs::rename(&fresh, &target).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(target)
}

/// Baixa o `yt-dlp_win.zip` da última release, confere o SHA-256 contra o
/// `SHA2-256SUMS` dela e troca a pasta inteira. A versão em pasta abre em
/// ~1 s; a de arquivo único se descompacta a cada chamada (3,9 s medidos).
pub fn install(managed: &Path) -> Result<YtDlp, String> {
    let http = http()?;
    // Pela tag, não por `latest/download`: o zip e a versão gravada são
    // garantidamente da mesma release.
    let tag = latest_tag(&http, REPO)?;
    let base = format!("https://github.com/{REPO}/releases/download/{tag}");
    let sums = String::from_utf8(get(&http, &format!("{base}/SHA2-256SUMS"))?).map_err(|_| "SHA2-256SUMS ilegível".to_string())?;
    let expected = checksum_for(&sums, ZIP).ok_or("o SHA2-256SUMS não lista o zip")?.to_owned();
    let zip = get(&http, &format!("{base}/{ZIP}"))?;
    let actual = sha256_hex(&zip);
    if actual != expected {
        return Err(format!("o yt-dlp baixado não confere (esperado {expected}, veio {actual})"));
    }
    let target = swap_in(managed, "yt-dlp", &zip, EXE)?;
    let _ = std::fs::write(target.join(VERSION_FILE), &tag);
    let _ = std::fs::write(target.join(CHECKED_FILE), "");
    log::info!("yt-dlp {tag} instalado em {}", target.display());
    Ok(YtDlp { exe: target.join(EXE), deno_dir: managed.join("deno") })
}

/// A versão instalada é a última? Se não, instala a nova. `Ok(None)`: já
/// era a última. Só vale para o yt-dlp gerenciado.
pub fn update(managed: &Path) -> Result<Option<YtDlp>, String> {
    let dir = managed.join("yt-dlp");
    let installed = std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap_or_default();
    let latest = latest_tag(&http()?, REPO)?;
    let _ = std::fs::write(dir.join(CHECKED_FILE), "");
    if installed.trim() == latest {
        log::info!("yt-dlp {latest} já é o mais novo");
        return Ok(None);
    }
    log::info!("yt-dlp: instalado {:?}, saiu {latest}", installed.trim());
    install(managed).map(Some)
}

/// A consulta diária já passou do prazo? (Sem a marca, também.)
pub fn update_due(managed: &Path) -> bool {
    let dir = managed.join("yt-dlp");
    if !dir.join(EXE).is_file() {
        return false;
    }
    std::fs::metadata(dir.join(CHECKED_FILE))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_none_or(|age| age > CHECK_EVERY)
}

/// Garante um Deno para o yt-dlp: o do PATH serve; senão, baixa o zip
/// oficial (~43 MB, uma vez) e confere o SHA-256 publicado junto.
pub fn ensure_deno(managed: &Path) -> Result<(), String> {
    if managed.join("deno").join(DENO_EXE).is_file() || on_path(DENO_EXE).is_some() {
        return Ok(());
    }
    let http = http()?;
    let tag = latest_tag(&http, DENO_REPO)?;
    let base = format!("https://github.com/{DENO_REPO}/releases/download/{tag}");
    let sum = String::from_utf8(get(&http, &format!("{base}/{DENO_ZIP}.sha256sum"))?).map_err(|_| "sha256sum do Deno ilegível".to_string())?;
    let expected = single_checksum(&sum).ok_or("o sha256sum do Deno não tem o hash")?;
    let zip = get(&http, &format!("{base}/{DENO_ZIP}"))?;
    let actual = sha256_hex(&zip);
    if actual != expected {
        return Err(format!("o Deno baixado não confere (esperado {expected}, veio {actual})"));
    }
    let target = swap_in(managed, "deno", &zip, DENO_EXE)?;
    log::info!("Deno {tag} instalado em {}", target.display());
    Ok(())
}

/// Extrai recusando o zip inteiro se um nome sair da pasta.
fn extract(zip: &[u8], into: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let name = archive.by_index(i).map_err(|e| e.to_string())?.enclosed_name();
        if name.is_none() {
            return Err("o zip do yt-dlp tem um caminho que sai da pasta".into());
        }
    }
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = file.enclosed_name().ok_or("caminho inválido")?;
        let path = into.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut out).map_err(|e| e.to_string())?;
        // No Linux e no macOS o executável e as bibliotecas precisam do bit de
        // execução que o zip guarda (o do Deno nem sempre guarda: ele ganha).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let is_exe = rel.file_name().is_some_and(|n| n == EXE || n == DENO_EXE);
            let mode = file.unix_mode().unwrap_or(0o644) | if is_exe { 0o755 } else { 0 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_becomes_netscape_without_host_only_cookies() {
        let text = netscape("SAPISID=abc/def; __Host-3PLSID=x; LOGIN_INFO=a=b=c;  ; semvalor");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "# Netscape HTTP Cookie File");
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[1].starts_with(".youtube.com\tTRUE\t/\tTRUE\t"));
        assert!(lines[1].ends_with("\tSAPISID\tabc/def"));
        assert!(lines[2].ends_with("\tLOGIN_INFO\ta=b=c"));
    }

    #[test]
    fn cookie_file_is_deleted_when_dropped() {
        let dir = std::env::temp_dir().join(format!("musicfast-teste-cookies-{}", std::process::id()));
        let account = Account { cookie: "SAPISID=abc".into(), dir: dir.clone() };
        let path = {
            let file = CookieFile::write(&account).unwrap();
            assert!(file.0.is_file());
            file.0.clone()
        };
        assert!(!path.exists());
        std::fs::write(dir.join("cookies-velho.txt"), "x").unwrap();
        clear_cookie_files(&dir);
        assert!(!dir.join("cookies-velho.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn printed_line_becomes_resolved_with_expiry_and_size() {
        let line = r#"{"id": "abc", "url": "https://rr1.googlevideo.com/videoplayback?expire=1793260861&itag=140&clen=3459077&x=1", "duration": 214, "http_headers": {"User-Agent": "Mozilla/5.0 X"}}"#;
        let r = parse_printed(line).unwrap();
        assert_eq!(r.video_id, "abc");
        assert_eq!(r.expires_at, 1_793_260_861);
        assert_eq!(r.size, Some(3_459_077));
        assert_eq!(r.user_agent.as_deref(), Some("Mozilla/5.0 X"));
        assert!(r.fresh(1_793_260_861 - 3600));
        assert!(!r.fresh(1_793_260_861 - 60));
    }

    #[test]
    fn errors_are_explained_in_portuguese() {
        let bot = "WARNING: x\nERROR: [youtube] abc: Sign in to confirm you're not a bot. Use --cookies";
        assert_eq!(explain(bot), "o YouTube pediu para confirmar que não é um robô");
        assert!(explain("ERROR: [youtube] abc: Video unavailable").contains("indisponível"));
        assert!(looks_outdated("ERROR: Unable to extract nsig function; please report this issue"));
        assert!(looks_outdated(bot));
        assert!(!looks_outdated("ERROR: [youtube] abc: Video unavailable"));
    }

    /// Contra o YouTube: os dois caminhos resolvem e o endereço baixa.
    /// `cargo test -p musicfast-player -- --ignored` (usa o yt-dlp do PATH
    /// ou o de `MUSICFAST_BIN`).
    #[test]
    #[ignore]
    fn ytdlp_live() {
        let dir = std::env::var_os("MUSICFAST_BIN").map(PathBuf::from).unwrap_or_default();
        let y = YtDlp::locate(&dir).expect("nenhum yt-dlp");
        let http = crate::source::http_client();
        for client in [Some(FAST_CLIENT), None] {
            let r = y.resolve_with("djV11Xbc914", client).unwrap_or_else(|e| panic!("{client:?}: {}", e.0));
            let bytes = crate::source::fetch(&http, &r).unwrap_or_else(|_| panic!("{client:?}: download"));
            assert!(bytes.len() as u64 == r.size.unwrap(), "{client:?}");
            crate::source::Decoder::open(bytes).expect("decodificador");
        }
    }

    /// Contra o GitHub: instala o yt-dlp e o Deno numa pasta vazia, a
    /// consulta de versão diz que já é a última, e o cliente padrão (o que
    /// precisa do Deno) resolve. `MUSICFAST_TMP=<pasta vazia> cargo test -p
    /// musicfast-player -- --ignored manutencao`.
    #[test]
    #[ignore]
    fn manutencao_live() {
        let dir = PathBuf::from(std::env::var_os("MUSICFAST_TMP").expect("MUSICFAST_TMP"));
        let y = install(&dir).expect("instalar");
        assert!(update(&dir).expect("consultar").is_none());
        assert!(!update_due(&dir));
        // Sem Deno no PATH, para baixar o gerenciado.
        // SAFETY: este teste roda sozinho (`--ignored manutencao`).
        unsafe { std::env::set_var("PATH", std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32")).unwrap_or_default()) };
        ensure_deno(&dir).expect("Deno");
        assert!(dir.join("deno").join(DENO_EXE).is_file());
        y.resolve_with("djV11Xbc914", None).unwrap_or_else(|e| panic!("cliente padrão: {}", e.0));
    }

    #[test]
    fn release_tags_and_deno_checksums_are_read() {
        assert_eq!(tag_of("https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26").as_deref(), Some("2026.10.26"));
        assert_eq!(tag_of("https://github.com/denoland/deno/releases/tag/v2.9.8").as_deref(), Some("v2.9.8"));
        assert_eq!(tag_of("https://github.com/denoland/deno/releases"), None);
        let ps = "\r\nAlgorithm : SHA256\r\nHash      : CBA84B26E0DF5AE7F74FC66B86A95B866872A86292EBD84FA1E959927EDD93BF\r\nPath      : C:\\a\\deno.zip\r\n";
        assert_eq!(single_checksum(ps).as_deref(), Some("cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf"));
    }

    #[test]
    fn checksum_lines_of_the_release_are_read() {
        let sums = "fd51b212cf8380c7a52ee08c0d6be7f3c59b2f2c832eaf7b45acfae6eab57419  yt-dlp_win.zip\n0000000000000000000000000000000000000000000000000000000000000000  yt-dlp.exe\n";
        assert_eq!(checksum_for(sums, "yt-dlp_win.zip"), Some("fd51b212cf8380c7a52ee08c0d6be7f3c59b2f2c832eaf7b45acfae6eab57419"));
        assert_eq!(checksum_for(sums, "nada.zip"), None);
    }
}
