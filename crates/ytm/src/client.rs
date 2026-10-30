//! As chamadas ao `music.youtube.com/youtubei/v1`.
//!
//! Bloqueante de propósito: quem chama já está numa thread de trabalho (a
//! interface nunca espera rede), e o `reqwest::blocking` reaproveita a
//! conexão HTTP/2 entre chamadas — a segunda chamada ao mesmo host custa só a
//! ida e volta (medido: 1ª ~480 ms com TLS, depois 80–380 ms).

use std::sync::RwLock;
use std::time::Duration;

use serde_json::{Value, json};

use crate::json::str_at;
use crate::model::{LikeStatus, Lyrics, Page, Privacy, Session, Watch};
use crate::parse;

const BASE: &str = "https://music.youtube.com/youtubei/v1/";
const ORIGIN: &str = "https://music.youtube.com";
/// A versão do WEB_REMIX lida do site em 28/10/2026. O InnerTube aceita
/// versões antigas por meses; `refresh_version` troca pela atual, lida da
/// página inicial, sem segurar ninguém.
pub const DEFAULT_VERSION: &str = "1.20261027.10.00";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SearchFilter {
    #[default]
    All,
    Songs,
    Videos,
    Albums,
    Artists,
    Playlists,
}

impl SearchFilter {
    pub const ALL: [Self; 6] = [Self::All, Self::Songs, Self::Videos, Self::Albums, Self::Artists, Self::Playlists];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Tudo",
            Self::Songs => "Músicas",
            Self::Videos => "Vídeos",
            Self::Albums => "Álbuns",
            Self::Artists => "Artistas",
            Self::Playlists => "Playlists",
        }
    }

    /// Os `params` dos botões de filtro do site (lidos em 28/10/2026).
    fn params(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Songs => Some("EgWKAQIIAWoSEAUQAxAJEAoQEBAEEA4QFRAR"),
            Self::Videos => Some("EgWKAQIQAWoSEAUQAxAJEAoQEBAEEA4QFRAR"),
            Self::Albums => Some("EgWKAQIYAWoSEAUQAxAJEAoQEBAEEA4QFRAR"),
            Self::Artists => Some("EgWKAQIgAWoSEAUQAxAJEAoQEBAEEA4QFRAR"),
            Self::Playlists => Some("EgeKAQQoADgBahIQBRADEAkQChAQEAQQDhAVEBE%3D"),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Http(String),
    Status(u16),
    Json(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "sem conexão com o YouTube Music ({e})"),
            Self::Status(s) => write!(f, "o YouTube Music respondeu {s}"),
            Self::Json(e) => write!(f, "resposta inesperada do YouTube Music ({e})"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A sessão de uma conta Google: os cookies do site, como o navegador os
/// manda. O `SAPISID` assina cada chamada (`SAPISIDHASH`), igual ao site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auth {
    cookie: String,
    sapisid: String,
}

impl Auth {
    /// `"SID=...; HSID=...; SAPISID=..."`. Sem `SAPISID` (ou a variante
    /// `__Secure-3PAPISID`), não é uma sessão logada.
    pub fn from_cookie_header(cookie: &str) -> Option<Self> {
        let cookie = cookie.trim().trim_start_matches("Cookie:").trim().to_owned();
        let value = |name: &str| {
            cookie.split(';').find_map(|pair| {
                let (k, v) = pair.trim().split_once('=')?;
                (k.trim() == name).then(|| v.trim().to_owned())
            })
        };
        let sapisid = value("SAPISID").or_else(|| value("__Secure-3PAPISID")).filter(|v| !v.is_empty())?;
        Some(Self { cookie, sapisid })
    }

    pub fn cookie_header(&self) -> &str {
        &self.cookie
    }

    fn authorization(&self) -> String {
        use sha1::{Digest, Sha1};
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let digest = Sha1::digest(format!("{ts} {} {ORIGIN}", self.sapisid).as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("SAPISIDHASH {ts}_{hex}")
    }
}

pub struct Client {
    http: reqwest::blocking::Client,
    version: RwLock<String>,
    visitor: RwLock<Option<String>>,
    auth: RwLock<Option<Auth>>,
    hl: String,
    gl: String,
}

impl Client {
    /// `hl`/`gl`: idioma e país dos títulos ("pt", "BR").
    pub fn new(hl: &str, gl: &str) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(6))
            .pool_idle_timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(Self {
            http,
            version: RwLock::new(DEFAULT_VERSION.to_owned()),
            visitor: RwLock::new(None),
            auth: RwLock::new(None),
            hl: hl.to_owned(),
            gl: gl.to_owned(),
        })
    }

    /// O `visitorData` que o YouTube devolve: mandado de volta, ele mantém as
    /// recomendações do início coerentes entre chamadas e reinícios.
    pub fn set_visitor(&self, visitor: Option<String>) {
        *self.visitor.write().unwrap_or_else(|e| e.into_inner()) = visitor;
    }

    pub fn visitor(&self) -> Option<String> {
        self.visitor.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Entra (ou sai, com `None`) numa conta. As chamadas seguintes já vêm
    /// personalizadas: início, biblioteca, curtidas.
    pub fn set_auth(&self, auth: Option<Auth>) {
        *self.auth.write().unwrap_or_else(|e| e.into_inner()) = auth;
    }

    pub fn signed_in(&self) -> bool {
        self.auth.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    fn auth(&self) -> Option<Auth> {
        self.auth.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn with_auth(&self, mut req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        if let Some(auth) = self.auth() {
            req = req
                .header("Cookie", auth.cookie_header())
                .header("Authorization", auth.authorization())
                .header("X-Goog-AuthUser", "0")
                .header("X-Origin", ORIGIN);
        }
        req
    }

    pub fn set_version(&self, version: &str) {
        *self.version.write().unwrap_or_else(|e| e.into_inner()) = version.to_owned();
    }

    /// Lê a versão atual do cliente na página do site (~340 kB). Rode fora
    /// de qualquer caminho de espera, e só de vez em quando.
    pub fn refresh_version(&self) -> Result<String> {
        let page = self
            .http
            .get("https://music.youtube.com/")
            .header("Accept-Language", &self.hl)
            .send()
            .and_then(|r| r.text())
            .map_err(|e| Error::Http(e.to_string()))?;
        let key = "\"INNERTUBE_CLIENT_VERSION\":\"";
        let start = page.find(key).ok_or_else(|| Error::Json("sem versão na página".into()))? + key.len();
        let end = page[start..].find('"').ok_or_else(|| Error::Json("versão cortada".into()))?;
        let version = page[start..start + end].to_owned();
        self.set_version(&version);
        Ok(version)
    }

    fn post(&self, endpoint: &str, query: &str, mut body: Value) -> Result<Value> {
        let version = self.version.read().unwrap_or_else(|e| e.into_inner()).clone();
        body["context"] = json!({
            "client": { "clientName": "WEB_REMIX", "clientVersion": version, "hl": self.hl, "gl": self.gl }
        });
        let mut req = self
            .http
            .post(format!("{BASE}{endpoint}?prettyPrint=false{query}"))
            .header("Origin", "https://music.youtube.com")
            .header("Referer", "https://music.youtube.com/")
            .header("X-Youtube-Client-Name", "67")
            .header("X-Youtube-Client-Version", &version)
            .json(&body);
        if let Some(visitor) = self.visitor() {
            req = req.header("X-Goog-Visitor-Id", visitor);
        }
        let req = self.with_auth(req);
        let resp = req.send().map_err(|e| Error::Http(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(Error::Status(status.as_u16()));
        }
        let value: Value = resp.json().map_err(|e| Error::Json(e.to_string()))?;
        if self.visitor().is_none() {
            if let Some(v) = str_at(&value, "responseContext.visitorData") {
                self.set_visitor(Some(v.to_owned()));
            }
        }
        Ok(value)
    }

    pub fn search(&self, query: &str, filter: SearchFilter) -> Result<Page> {
        let mut body = json!({ "query": query });
        if let Some(params) = filter.params() {
            body["params"] = json!(params);
        }
        Ok(parse::page(&self.post("search", "", body)?))
    }

    pub fn suggestions(&self, input: &str) -> Result<Vec<String>> {
        let v = self.post("music/get_search_suggestions", "", json!({ "input": input }))?;
        Ok(parse::suggestions(&v))
    }

    pub fn browse(&self, browse_id: &str, params: Option<&str>) -> Result<Page> {
        let mut body = json!({ "browseId": browse_id });
        if let Some(p) = params {
            body["params"] = json!(p);
        }
        let mut page = parse::page(&self.post("browse", "", body)?);
        if let Some(h) = page.header.as_mut() {
            if h.kind == crate::Kind::Other {
                h.kind = parse::kind_of_id(browse_id);
            }
        }
        Ok(page)
    }

    pub fn home(&self) -> Result<Page> {
        self.browse("FEmusic_home", None)
    }

    /// Mais prateleiras de uma página (o token vai na URL, como o site faz).
    pub fn more_sections(&self, token: &str) -> Result<Page> {
        let query = format!("&ctoken={token}&continuation={token}&type=next");
        Ok(parse::continuation(&self.post("browse", &query, json!({}))?))
    }

    /// Mais itens de uma prateleira (o token vai no corpo).
    pub fn more_items(&self, token: &str) -> Result<Page> {
        Ok(parse::continuation(&self.post("browse", "", json!({ "continuation": token }))?))
    }

    /// A fila que o site monta ao tocar: a música e o rádio dela, ou uma
    /// playlist/álbum inteiro.
    pub fn watch(&self, video_id: Option<&str>, playlist_id: Option<&str>, params: Option<&str>) -> Result<Watch> {
        let mut body = json!({
            "enablePersistentPlaylistPanel": true,
            "isAudioOnly": true,
            "tunerSettingValue": "AUTOMIX_SETTING_NORMAL",
        });
        if let Some(v) = video_id {
            body["videoId"] = json!(v);
        }
        match (playlist_id, video_id) {
            (Some(p), _) => body["playlistId"] = json!(p.strip_prefix("VL").unwrap_or(p)),
            // Sem playlist, o rádio da música (o que o "Iniciar rádio" faz).
            (None, Some(v)) => body["playlistId"] = json!(format!("RDAMVM{v}")),
            (None, None) => {}
        }
        if let Some(p) = params {
            body["params"] = json!(p);
        }
        Ok(parse::watch(&self.post("next", "", body)?))
    }

    pub fn watch_more(&self, token: &str) -> Result<Watch> {
        let body = json!({ "continuation": token, "enablePersistentPlaylistPanel": true, "isAudioOnly": true });
        Ok(parse::watch_continuation(&self.post("next", "", body)?))
    }

    pub fn lyrics(&self, browse_id: &str) -> Result<Option<Lyrics>> {
        Ok(parse::lyrics(&self.post("browse", "", json!({ "browseId": browse_id }))?))
    }

    /// A conta logada, ou se a sessão venceu (os cookies não valem mais).
    /// 401/403 também é sessão vencida; o resto dos erros é rede.
    pub fn account(&self) -> Result<Session> {
        match self.post("account/account_menu", "", json!({})) {
            Ok(v) => Ok(parse::session(&v)),
            Err(Error::Status(401 | 403)) => Ok(Session::SignedOut),
            Err(e) => Err(e),
        }
    }

    pub fn rate_song(&self, video_id: &str, status: LikeStatus) -> Result<()> {
        let endpoint = match status {
            LikeStatus::Like => "like/like",
            LikeStatus::Dislike => "like/dislike",
            LikeStatus::Indifferent => "like/removelike",
        };
        self.post(endpoint, "", json!({ "target": { "videoId": video_id } }))?;
        Ok(())
    }

    /// Salvar (ou tirar) um álbum ou playlist da biblioteca.
    pub fn rate_playlist(&self, playlist_id: &str, saved: bool) -> Result<()> {
        let endpoint = if saved { "like/like" } else { "like/removelike" };
        let id = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
        self.post(endpoint, "", json!({ "target": { "playlistId": id } }))?;
        Ok(())
    }

    /// Cria a playlist e devolve o id dela.
    pub fn create_playlist(&self, title: &str, privacy: Privacy, video_ids: &[String]) -> Result<String> {
        let mut body = json!({ "title": title, "description": "", "privacyStatus": privacy.api() });
        if !video_ids.is_empty() {
            body["videoIds"] = json!(video_ids);
        }
        let v = self.post("playlist/create", "", body)?;
        str_at(&v, "playlistId").map(Into::into).ok_or_else(|| Error::Json("playlist sem id".into()))
    }

    pub fn add_to_playlist(&self, playlist_id: &str, video_id: &str) -> Result<()> {
        let id = playlist_id.strip_prefix("VL").unwrap_or(playlist_id);
        let v = self.post(
            "browse/edit_playlist",
            "",
            json!({
                "playlistId": id,
                "actions": [{ "action": "ACTION_ADD_VIDEO", "addedVideoId": video_id, "dedupeOption": "DEDUPE_OPTION_CHECK" }],
            }),
        )?;
        match str_at(&v, "status") {
            Some("STATUS_SUCCEEDED") | None => Ok(()),
            Some(other) => Err(Error::Json(format!("a playlist respondeu {other}"))),
        }
    }

    pub fn subscribe(&self, channel_id: &str, on: bool) -> Result<()> {
        let endpoint = if on { "subscription/subscribe" } else { "subscription/unsubscribe" };
        self.post(endpoint, "", json!({ "channelIds": [channel_id] }))?;
        Ok(())
    }

    /// Conta a reprodução no histórico da conta, como o player do site faz
    /// (o áudio daqui vem pelo yt-dlp, que não avisa ninguém).
    pub fn add_history(&self, video_id: &str) -> Result<()> {
        let v = self.post("player", "", json!({ "videoId": video_id }))?;
        let base = str_at(&v, "playbackTracking.videostatsPlaybackUrl.baseUrl")
            .ok_or_else(|| Error::Json("sem endereço de histórico".into()))?;
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64 | 1)
            .unwrap_or(7);
        let cpn: String = (0..16)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                CHARS[(seed % 64) as usize] as char
            })
            .collect();
        let sep = if base.contains('?') { '&' } else { '?' };
        let req = self.http.get(format!("{base}{sep}ver=2&c=WEB_REMIX&cpn={cpn}"));
        let resp = self.with_auth(req).send().map_err(|e| Error::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        Ok(())
    }
}
