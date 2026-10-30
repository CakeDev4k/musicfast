//! Tradução no próprio computador, sem serviço de fora: pelo Ollama (um
//! modelo de IA local, que traduz letra bem e de qualquer idioma) ou pelo
//! LibreTranslate, o que estiver aberto. O Ollama tem a preferência.
//!
//! A tela de ajustes lê o que foi encontrado em `status`, que procura de
//! novo de tempos em tempos numa thread, e pode baixar o modelo sugerido
//! pelo próprio Ollama (`pull`).

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// O modelo sugerido: leve (uns 3 GB), roda em CPU e traduz bem entre os
/// idiomas das letras.
pub const RECOMMENDED: &str = "gemma3:4b";

/// Os modelos que traduzem bem, na ordem de preferência da escolha
/// automática (o começo do nome).
const PREFERRED: [&str; 8] = ["gemma3", "qwen3", "qwen2.5", "gemma2", "aya", "llama3", "mistral", "phi4"];

/// Quanto tempo o que foi encontrado vale antes de procurar de novo.
const RECHECK: Duration = Duration::from_secs(4);

/// O que tem de tradutor local agora.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Procurando (a primeira vez; nas outras, mostra o anterior).
    pub checking: bool,
    /// O Ollama aberto, com os modelos instalados (sem os de embedding).
    pub ollama: Option<Vec<String>>,
    /// O LibreTranslate aberto.
    pub libre: bool,
    /// O download de um modelo pelo Ollama.
    pub pull: Option<Pull>,
    checked_at: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct Pull {
    pub model: String,
    /// 0 a 1; `None` antes de saber o tamanho.
    pub progress: Option<f32>,
    pub error: Option<String>,
}

fn state() -> &'static Mutex<Status> {
    static STATE: OnceLock<Mutex<Status>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Status::default()))
}

fn http() -> &'static reqwest::blocking::Client {
    static HTTP: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    HTTP.get_or_init(reqwest::blocking::Client::new)
}

/// O endereço do Ollama: o de `OLLAMA_HOST`, se tiver, ou o de sempre.
pub fn ollama_base() -> String {
    let host = std::env::var("OLLAMA_HOST").ok().filter(|h| !h.trim().is_empty());
    let Some(host) = host else { return "http://127.0.0.1:11434".to_owned() };
    let host = host.trim().trim_end_matches('/').replace("0.0.0.0", "127.0.0.1");
    let host = if host.contains("://") { host } else { format!("http://{host}") };
    // Só o host, sem porta: a do Ollama.
    if host.rsplit_once(':').is_some_and(|(_, port)| port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty()) { host } else { format!("{host}:11434") }
}

/// O endereço do LibreTranslate: o de `LIBRETRANSLATE_URL`, ou o de sempre.
pub fn libre_base() -> String {
    std::env::var("LIBRETRANSLATE_URL")
        .ok()
        .map(|u| u.trim().trim_end_matches('/').to_owned())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:5000".to_owned())
}

/// O que foi encontrado; procura de novo numa thread quando o anterior
/// passou do tempo. Chama `ctx.request_repaint` quando muda.
pub fn status(ctx: &egui::Context) -> Status {
    use std::sync::atomic::{AtomicBool, Ordering};
    static PROBING: AtomicBool = AtomicBool::new(false);
    let s = state().lock().unwrap_or_else(|e| e.into_inner());
    let stale = s.checked_at.is_none_or(|at| at.elapsed() > RECHECK);
    if stale && !PROBING.swap(true, Ordering::SeqCst) {
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("tradutor-local".into()).spawn(move || {
            let (ollama, libre) = (ollama_models(http()).ok(), libre_up(http()));
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            let changed = s.ollama != ollama || s.libre != libre || s.checked_at.is_none();
            s.ollama = ollama;
            s.libre = libre;
            s.checked_at = Some(Instant::now());
            drop(s);
            PROBING.store(false, Ordering::SeqCst);
            if changed {
                ctx.request_repaint();
            }
        });
        if spawned.is_err() {
            PROBING.store(false, Ordering::SeqCst);
        }
    }
    // Enquanto procura pela primeira vez, a tela mostra "procurando"; nas
    // outras, o que já se sabia.
    let mut out = s.clone();
    out.checking = s.checked_at.is_none();
    drop(s);
    // Procura de novo enquanto a tela está aberta (a pessoa pode abrir o
    // Ollama agora).
    ctx.request_repaint_after(RECHECK);
    out
}

/// Baixa `model` pelo Ollama numa thread, com o progresso em `status`.
pub fn pull(ctx: &egui::Context, model: &str) {
    {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        if s.pull.as_ref().is_some_and(|p| p.error.is_none()) {
            return;
        }
        s.pull = Some(Pull { model: model.to_owned(), progress: None, error: None });
    }
    let (ctx, model) = (ctx.clone(), model.to_owned());
    let _ = std::thread::Builder::new().name("ollama-pull".into()).spawn(move || {
        let result = pull_blocking(&model, |progress| {
            if let Some(p) = state().lock().unwrap_or_else(|e| e.into_inner()).pull.as_mut() {
                p.progress = Some(progress);
            }
            ctx.request_repaint();
        });
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        match result {
            Ok(()) => {
                s.pull = None;
                // O modelo novo entra na lista já.
                if let Ok(models) = ollama_models(http()) {
                    s.ollama = Some(models);
                }
            }
            Err(error) => {
                log::warn!("ollama pull {model}: {error}");
                if let Some(p) = s.pull.as_mut() {
                    p.error = Some(error);
                }
            }
        }
        drop(s);
        ctx.request_repaint();
    });
}

fn pull_blocking(model: &str, mut progress: impl FnMut(f32)) -> Result<(), String> {
    use std::io::BufRead;
    let response = http()
        .post(format!("{}/api/pull", ollama_base()))
        .json(&json!({ "model": model, "stream": true }))
        .timeout(Duration::from_secs(6 * 3600))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Ollama: {e}"))?;
    for line in std::io::BufReader::new(response).lines() {
        let line = line.map_err(|e| format!("Ollama: {e}"))?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(error) = v["error"].as_str() {
            return Err(error.to_owned());
        }
        if let (Some(done), Some(total)) = (v["completed"].as_f64(), v["total"].as_f64())
            && total > 0.0
        {
            progress((done / total) as f32);
        }
        if v["status"].as_str() == Some("success") {
            return Ok(());
        }
    }
    Err("Ollama: o download parou no meio".to_owned())
}

/// Os modelos de texto instalados no Ollama; `Err` se ele não está aberto.
fn ollama_models(http: &reqwest::blocking::Client) -> Result<Vec<String>, String> {
    let v: Value = http
        .get(format!("{}/api/tags", ollama_base()))
        .timeout(Duration::from_millis(1500))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("Ollama: {e}"))?;
    let mut models: Vec<String> = v["models"]
        .as_array()
        .map(|list| list.iter().filter_map(|m| m["name"].as_str().or_else(|| m["model"].as_str())).map(str::to_owned).collect())
        .unwrap_or_default();
    models.retain(|m| !m.contains("embed") && !m.contains("bge-") && !m.contains("minilm"));
    models.sort();
    Ok(models)
}

fn libre_up(http: &reqwest::blocking::Client) -> bool {
    http.get(format!("{}/languages", libre_base()))
        .timeout(Duration::from_millis(1500))
        .send()
        .is_ok_and(|r| r.status().is_success())
}

/// O modelo que vai traduzir: o escolhido, se está instalado, ou o melhor
/// dos instalados.
pub fn pick_model(models: &[String], wanted: Option<&str>) -> Option<String> {
    if let Some(w) = wanted
        && let Some(m) = models.iter().find(|m| *m == w || m.strip_suffix(":latest") == Some(w))
    {
        return Some(m.clone());
    }
    PREFERRED
        .iter()
        .find_map(|p| models.iter().find(|m| m.starts_with(p)))
        .or_else(|| models.first())
        .cloned()
}

/// Quem traduz, decidido uma vez por letra.
pub enum Backend {
    Ollama { model: String },
    Libre,
}

impl Backend {
    /// O Ollama com um modelo, se tiver; senão o LibreTranslate.
    pub fn detect(http: &reqwest::blocking::Client, wanted: Option<&str>) -> Result<Self, String> {
        match ollama_models(http) {
            Ok(models) => match pick_model(&models, wanted) {
                Some(model) => return Ok(Self::Ollama { model }),
                None if !libre_up(http) => return Err(musicfast_i18n::t().local_no_model.to_owned()),
                None => {}
            },
            Err(_) if !libre_up(http) => return Err(musicfast_i18n::t().local_not_found.to_owned()),
            Err(_) => {}
        }
        Ok(Self::Libre)
    }

    /// Caracteres por pedido.
    pub fn chunk_size(&self) -> usize {
        match self {
            // Pedaços menores: o modelo erra menos a conta das linhas.
            Self::Ollama { .. } => 1200,
            Self::Libre => 3000,
        }
    }

    /// As linhas traduzidas para `target`, na mesma ordem, e o idioma
    /// delas.
    pub fn translate(&self, http: &reqwest::blocking::Client, lines: &[&str], target: &str) -> Result<(String, Vec<String>), String> {
        match self {
            Self::Ollama { model } => ollama_lines(http, model, lines, target),
            Self::Libre => libre_lines(http, lines, target),
        }
    }
}

/// O nome do idioma em inglês, para o pedido ao modelo.
fn lang_name(code: &str) -> &str {
    match code.split(['-', '_']).next().unwrap_or(code) {
        "pt" => "Brazilian Portuguese",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Simplified Chinese",
        _ => code,
    }
}

/// Pelo Ollama. Se o modelo devolve outro número de linhas, tenta de novo
/// em duas metades (até uma linha só), para a linha a linha bater.
fn ollama_lines(http: &reqwest::blocking::Client, model: &str, lines: &[&str], target: &str) -> Result<(String, Vec<String>), String> {
    let (lang, out) = ollama_request(http, model, lines, target)?;
    if out.len() == lines.len() {
        return Ok((lang, out));
    }
    if lines.len() == 1 {
        // Uma linha que virou várias: junta.
        return Ok((lang, vec![out.join(" ")]));
    }
    log::debug!("ollama: {} linhas para {}; em metades", out.len(), lines.len());
    let (a, b) = lines.split_at(lines.len() / 2);
    let (lang, mut first) = ollama_lines(http, model, a, target)?;
    let (_, second) = ollama_lines(http, model, b, target)?;
    first.extend(second);
    Ok((lang, first))
}

fn ollama_request(http: &reqwest::blocking::Client, model: &str, lines: &[&str], target: &str) -> Result<(String, Vec<String>), String> {
    let language = lang_name(target);
    let system = format!(
        "You translate song lyrics into {language}. You get a JSON array of lyric lines. \
         Return \"lines\": an array with exactly the same number of items, in the same order, \
         each item the {language} translation of the matching line. Translate the meaning naturally, \
         like good subtitles, keeping the tone; never merge, split, skip or explain lines. \
         Keep names and sounds (\"oh\", \"la la\") as they are. A line already in {language} is copied. \
         Return \"lang\": the ISO 639-1 code of the original lyrics' main language."
    );
    let schema = json!({
        "type": "object",
        "properties": {
            "lang": { "type": "string" },
            "lines": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["lang", "lines"]
    });
    let body = json!({
        "model": model,
        "stream": false,
        "format": schema,
        "keep_alive": "10m",
        "options": { "temperature": 0.2 },
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": serde_json::to_string(lines).unwrap_or_default() }
        ]
    });
    let response = http
        .post(format!("{}/api/chat", ollama_base()))
        .json(&body)
        // Em CPU, o primeiro pedido ainda carrega o modelo.
        .timeout(Duration::from_secs(240))
        .send()
        .map_err(|e| format!("Ollama: {e}"))?;
    let status = response.status();
    let v: Value = response.json().map_err(|e| format!("Ollama: {e}"))?;
    if !status.is_success() {
        return Err(format!("Ollama: {}", v["error"].as_str().unwrap_or(status.as_str())));
    }
    parse_ollama(v["message"]["content"].as_str().unwrap_or_default())
}

/// A resposta do modelo: o JSON com o idioma e as linhas (às vezes vem
/// dentro de um bloco de código).
fn parse_ollama(content: &str) -> Result<(String, Vec<String>), String> {
    let start = content.find('{').ok_or("Ollama: resposta sem JSON")?;
    let end = content.rfind('}').ok_or("Ollama: resposta sem JSON")?;
    let v: Value = serde_json::from_str(&content[start..=end]).map_err(|e| format!("Ollama: {e}"))?;
    let lines: Vec<String> = v["lines"]
        .as_array()
        .ok_or("Ollama: resposta sem linhas")?
        .iter()
        .map(|l| l.as_str().unwrap_or_default().trim().to_owned())
        .collect();
    let lang = v["lang"].as_str().unwrap_or_default().trim().to_lowercase();
    Ok((lang, lines))
}

/// Pelo LibreTranslate: uma lista de textos, uma tradução por texto.
fn libre_lines(http: &reqwest::blocking::Client, lines: &[&str], target: &str) -> Result<(String, Vec<String>), String> {
    let target = target.split(['-', '_']).next().unwrap_or(target);
    let target = if target == "zh" { "zh-Hans" } else { target };
    let mut body = json!({ "q": lines, "source": "auto", "target": target, "format": "text" });
    if let Ok(key) = std::env::var("LIBRETRANSLATE_API_KEY") {
        body["api_key"] = Value::String(key);
    }
    let response = http
        .post(format!("{}/translate", libre_base()))
        .json(&body)
        .timeout(Duration::from_secs(120))
        .send()
        .map_err(|e| format!("LibreTranslate: {e}"))?;
    let status = response.status();
    let v: Value = response.json().map_err(|e| format!("LibreTranslate: {e}"))?;
    if !status.is_success() {
        return Err(format!("LibreTranslate: {}", v["error"].as_str().unwrap_or(status.as_str())));
    }
    let out: Vec<String> = match &v["translatedText"] {
        Value::Array(list) => list.iter().map(|l| l.as_str().unwrap_or_default().trim().to_owned()).collect(),
        Value::String(one) => vec![one.trim().to_owned()],
        _ => return Err("LibreTranslate: resposta sem tradução".to_owned()),
    };
    // O idioma da maioria das linhas.
    let detected: Vec<&str> = match &v["detectedLanguage"] {
        Value::Array(list) => list.iter().filter_map(|d| d["language"].as_str()).collect(),
        d => d["language"].as_str().into_iter().collect(),
    };
    let lang = detected.iter().max_by_key(|l| detected.iter().filter(|o| o == l).count()).copied().unwrap_or_default().to_owned();
    Ok((lang, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escolhe_o_modelo() {
        let models = vec!["llama3.2:3b".to_owned(), "gemma3:4b".to_owned(), "zzz:1b".to_owned()];
        assert_eq!(pick_model(&models, None).as_deref(), Some("gemma3:4b"));
        assert_eq!(pick_model(&models, Some("llama3.2:3b")).as_deref(), Some("llama3.2:3b"));
        // O escolhido saiu: volta ao automático.
        assert_eq!(pick_model(&models, Some("sumiu:7b")).as_deref(), Some("gemma3:4b"));
        assert_eq!(pick_model(&["x:latest".to_owned()], Some("x")).as_deref(), Some("x:latest"));
        assert_eq!(pick_model(&[], None), None);
    }

    #[test]
    fn le_a_resposta_do_modelo() {
        let (lang, lines) = parse_ollama("```json\n{\"lang\": \"EN\", \"lines\": [\" Olá \", \"mundo\"]}\n```").unwrap();
        assert_eq!(lang, "en");
        assert_eq!(lines, ["Olá", "mundo"]);
        assert!(parse_ollama("desculpe").is_err());
    }

    /// Um Ollama de mentira numa porta livre: lista um modelo e "traduz"
    /// cada linha pondo "PT " na frente; na primeira vez, erra a conta das
    /// linhas, para testar a volta em metades.
    fn fake_ollama() -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let mut calls = 0;
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Lê os cabeçalhos e o corpo (Content-Length).
                let body = loop {
                    let n = stream.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len: usize = text.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break (text[..end].to_owned(), String::from_utf8_lossy(&buf[end + 4..end + 4 + len]).to_string());
                        }
                    }
                    if n == 0 {
                        break (text, String::new());
                    }
                };
                let reply = if body.0.starts_with("GET /api/tags") {
                    json!({ "models": [{ "name": "nomic-embed-text:latest" }, { "name": "gemma3:4b" }] })
                } else {
                    calls += 1;
                    let req: Value = serde_json::from_str(&body.1).unwrap();
                    assert_eq!(req["model"], "gemma3:4b");
                    let lines: Vec<String> = serde_json::from_str(req["messages"][1]["content"].as_str().unwrap()).unwrap();
                    let mut out: Vec<String> = lines.iter().map(|l| format!("PT {l}")).collect();
                    if calls == 1 {
                        out.pop();
                    }
                    let content = json!({ "lang": "en", "lines": out }).to_string();
                    json!({ "message": { "role": "assistant", "content": content } })
                };
                let reply = reply.to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
            }
        });
        addr
    }

    #[test]
    fn traduz_pelo_ollama_falso() {
        // SAFETY: só este teste mexe no OLLAMA_HOST.
        unsafe { std::env::set_var("OLLAMA_HOST", fake_ollama()) };
        let http = reqwest::blocking::Client::new();
        let backend = Backend::detect(&http, None).unwrap();
        assert!(matches!(&backend, Backend::Ollama { model } if model == "gemma3:4b"));
        let (lang, lines) = backend.translate(&http, &["one", "two", "three"], "pt").unwrap();
        assert_eq!(lang, "en");
        assert_eq!(lines, ["PT one", "PT two", "PT three"]);
    }

    /// Pelo Ollama de verdade. Rode com `--ignored`, com o Ollama aberto.
    #[test]
    #[ignore]
    fn traduz_pelo_ollama() {
        let http = reqwest::blocking::Client::new();
        let backend = Backend::detect(&http, None).unwrap();
        let (lang, lines) = backend.translate(&http, &["正午の陽射しで生まれたの", "I just want to hold your hand"], "pt").unwrap();
        println!("{lang}: {lines:?}");
        assert_eq!(lines.len(), 2);
    }
}
