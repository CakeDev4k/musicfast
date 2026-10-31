//! A música no perfil do Discord (Rich Presence), pelo canal local que o
//! próprio Discord abre no computador: nada passa por servidor nosso e não
//! precisa de login. Uma thread cuida do canal: a escrita lá bloqueia se o
//! Discord travar, e a janela não pode esperar por isso.
//!
//! Precisa do id de um aplicativo criado em discord.com/developers: é o nome
//! dele que aparece em "Ouvindo ...".

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// O aplicativo do MusicFast no Discord.
pub const APP_ID: &str = "1565910137305043025";
/// Para onde o botão de divulgação leva.
pub const SITE_URL: &str = "https://www.threshold-editor.com/#musicfast";

const NAME: &str = "MusicFast";
/// O Discord aceita poucas trocas por minuto; pular músicas seguidas manda
/// só a última.
const MIN_INTERVAL: Duration = Duration::from_secs(4);
const RETRY: Duration = Duration::from_secs(15);
const KEEPALIVE: Duration = Duration::from_secs(30);
const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
const OP_PING: u32 = 3;
const OP_PONG: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Mostrar a música no perfil. Desligado de fábrica: é a pessoa quem
    /// decide publicar o que ouve.
    pub on: bool,
    /// O botão que leva ao site para baixar o app, junto da música.
    pub promote: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { on: false, promote: true }
    }
}

/// A música como o Discord a mostra.
#[derive(Clone, Debug, PartialEq)]
pub struct Song {
    pub video_id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub cover: Option<String>,
    /// Quando a música começou e quando acaba (Unix, ms).
    pub start_ms: u64,
    pub end_ms: Option<u64>,
}

/// O limite do Discord é de 128 caracteres, e título ou artista com menos de
/// 2 fazem ele recusar a atividade inteira.
fn field(text: &str) -> String {
    let mut text: String = text.trim().chars().take(128).collect();
    while text.chars().count() < 2 {
        text.push('\u{2800}');
    }
    text
}

fn label(text: &str) -> String {
    text.trim().chars().take(32).collect()
}

/// A atividade "Ouvindo": a música, a capa e os botões embaixo. `site_label`
/// é o texto do botão de divulgação, que vem primeiro (`None`: sem ele).
pub fn activity(song: &Song, site_label: Option<&str>, listen_label: &str) -> Value {
    let track_url = format!("https://music.youtube.com/watch?v={}", song.video_id);
    let mut buttons = Vec::new();
    if let Some(site) = site_label {
        buttons.push(json!({ "label": label(site), "url": SITE_URL }));
    }
    buttons.push(json!({ "label": label(listen_label), "url": track_url }));

    let mut activity = json!({
        "name": NAME,
        "type": 2,
        // "Ouvindo <música>" na lista de membros, em vez do nome do app.
        "status_display_type": 2,
        "details": field(&song.title),
        "details_url": track_url,
        "state": field(&song.artist),
        "timestamps": { "start": song.start_ms },
        "buttons": buttons,
    });
    if let Some(end) = song.end_ms {
        activity["timestamps"]["end"] = json!(end);
    }
    // A capa é um endereço público; acima de 256 o Discord recusa.
    if let Some(cover) = song.cover.as_ref().filter(|c| c.len() <= 256) {
        let mut assets = json!({ "large_image": cover });
        if let Some(album) = song.album.as_deref().filter(|a| a.trim().chars().count() >= 2) {
            assets["large_text"] = json!(field(album));
        }
        if site_label.is_some() {
            assets["large_url"] = json!(SITE_URL);
        }
        activity["assets"] = assets;
    }
    activity
}

fn write_frame(stream: &mut impl Write, op: u32, body: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(body)?;
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    stream.write_all(&frame)?;
    stream.flush()
}

fn read_frame(stream: &mut impl Read) -> io::Result<(u32, Value)> {
    let mut head = [0u8; 8];
    stream.read_exact(&mut head)?;
    let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    // Nenhuma resposta do Discord chega perto disto: é canal corrompido.
    if len > 1 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "quadro grande demais"));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok((op, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

/// Por que a conversa com o Discord acabou.
#[derive(Debug)]
enum Failure {
    /// O canal caiu (o Discord fechou): vale tentar de novo.
    Link(io::Error),
    /// O Discord recusou: tentar de novo com o mesmo pedido não adianta.
    Refused(String),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Link(error)
    }
}

/// A próxima resposta de verdade, respondendo aos pings no caminho.
fn answer(stream: &mut (impl Read + Write)) -> Result<Value, Failure> {
    loop {
        let (op, body) = read_frame(stream)?;
        match op {
            OP_PING => write_frame(stream, OP_PONG, &body)?,
            OP_PONG => return Ok(body),
            OP_CLOSE => return Err(Failure::Refused(body["message"].as_str().unwrap_or("fechado").to_owned())),
            _ => return Ok(body),
        }
    }
}

fn handshake(stream: &mut (impl Read + Write), app_id: &str) -> Result<(), Failure> {
    write_frame(stream, OP_HANDSHAKE, &json!({ "v": 1, "client_id": app_id }))?;
    let reply = answer(stream)?;
    if reply["evt"] == "ERROR" {
        return Err(Failure::Refused(reply["data"]["message"].as_str().unwrap_or("erro").to_owned()));
    }
    Ok(())
}

fn set_activity(stream: &mut (impl Read + Write), activity: Option<&Value>, nonce: u64) -> Result<(), Failure> {
    let body = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": nonce.to_string(),
    });
    write_frame(stream, OP_FRAME, &body)?;
    let reply = answer(stream)?;
    if reply["evt"] == "ERROR" {
        return Err(Failure::Refused(reply["data"]["message"].as_str().unwrap_or("erro").to_owned()));
    }
    // Os botões não aparecem para a própria conta: a resposta é o jeito de
    // saber se eles chegaram.
    if activity.is_some() {
        log::info!("discord: atividade aceita, botões: {}", reply["data"]["buttons"]);
    }
    Ok(())
}

#[cfg(windows)]
type Pipe = std::fs::File;
#[cfg(unix)]
type Pipe = std::os::unix::net::UnixStream;

/// O Discord abre o primeiro canal livre de 0 a 9 (mais de uma cópia aberta,
/// como o Canary, usa os seguintes).
fn open_pipe() -> io::Result<Pipe> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "Discord fechado");
    for n in 0..10 {
        #[cfg(windows)]
        let opened = std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{n}"));
        #[cfg(unix)]
        let opened = {
            let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"].iter().find_map(std::env::var_os).unwrap_or_else(|| "/tmp".into());
            Pipe::connect(std::path::Path::new(&dir).join(format!("discord-ipc-{n}")))
        };
        match opened {
            Ok(pipe) => return Ok(pipe),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// Em que pé a ligação está, para o botão e o painel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Off,
    /// O Discord não está aberto (ou ainda não respondeu).
    Waiting,
    Connected,
    /// O Discord recusou o aplicativo.
    Refused,
}

enum Message {
    Set { app_id: String, activity: Option<Value> },
    Stop,
}

#[derive(Clone)]
pub struct Discord {
    tx: Sender<Message>,
    link: Arc<AtomicU8>,
}

impl Discord {
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let link = Arc::new(AtomicU8::new(0));
        let shared = link.clone();
        let _ = std::thread::Builder::new().name("discord".into()).spawn(move || run(&rx, &shared, &wake));
        Self { tx, link }
    }

    pub fn link(&self) -> Link {
        match self.link.load(Ordering::Relaxed) {
            1 => Link::Waiting,
            2 => Link::Connected,
            3 => Link::Refused,
            _ => Link::Off,
        }
    }

    /// `None` tira a música do perfil, mantendo o canal aberto.
    pub fn set(&self, app_id: &str, activity: Option<Value>) {
        let _ = self.tx.send(Message::Set { app_id: app_id.to_owned(), activity });
    }

    /// Fecha o canal: o Discord tira a atividade na hora.
    pub fn stop(&self) {
        let _ = self.tx.send(Message::Stop);
    }
}

fn run(rx: &Receiver<Message>, link: &AtomicU8, wake: &impl Fn()) {
    let set_link = |value: u8| {
        if link.swap(value, Ordering::Relaxed) != value {
            wake();
        }
    };
    let mut pipe: Option<Pipe> = None;
    let mut wanted: Option<(String, Option<Value>)> = None;
    let mut dirty = false;
    let mut refused_id: Option<String> = None;
    let mut last_send = Instant::now().checked_sub(MIN_INTERVAL).unwrap_or_else(Instant::now);
    let mut last_try: Option<Instant> = None;
    let mut last_alive = Instant::now();
    let mut nonce = 0u64;

    loop {
        // Espera o próximo pedido só até o que já está pendente vencer.
        let wait = if !dirty {
            KEEPALIVE
        } else if pipe.is_some() {
            MIN_INTERVAL.saturating_sub(last_send.elapsed())
        } else {
            RETRY.saturating_sub(last_try.map_or(RETRY, |t| t.elapsed()))
        };
        match rx.recv_timeout(wait) {
            Ok(Message::Set { app_id, activity }) => {
                if wanted.as_ref().is_some_and(|(id, _)| *id != app_id) {
                    pipe = None;
                    last_try = None;
                }
                if refused_id.as_deref() != Some(&app_id) {
                    refused_id = None;
                }
                wanted = Some((app_id, activity));
                dirty = true;
                continue_if_more(rx, &mut wanted, &mut dirty, &mut pipe, &set_link);
            }
            Ok(Message::Stop) => {
                pipe = None;
                wanted = None;
                dirty = false;
                refused_id = None;
                set_link(0);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }

        let Some((app_id, activity)) = wanted.clone() else { continue };
        if refused_id.as_deref() == Some(app_id.as_str()) {
            dirty = false;
            continue;
        }
        if pipe.is_none() {
            if last_try.is_some_and(|t| t.elapsed() < RETRY) {
                continue;
            }
            last_try = Some(Instant::now());
            match open_pipe().map_err(Failure::Link).and_then(|mut p| handshake(&mut p, &app_id).map(|()| p)) {
                Ok(p) => {
                    log::info!("discord: ligado");
                    pipe = Some(p);
                    dirty = true;
                    last_send = Instant::now().checked_sub(MIN_INTERVAL).unwrap_or_else(Instant::now);
                    last_alive = Instant::now();
                    set_link(2);
                }
                Err(Failure::Refused(why)) => {
                    log::warn!("discord recusou o aplicativo: {why}");
                    refused_id = Some(app_id);
                    dirty = false;
                    set_link(3);
                    continue;
                }
                Err(Failure::Link(_)) => {
                    dirty = true;
                    set_link(1);
                    continue;
                }
            }
        }
        let Some(stream) = pipe.as_mut() else { continue };
        let result = if dirty {
            if last_send.elapsed() < MIN_INTERVAL {
                continue;
            }
            nonce += 1;
            last_send = Instant::now();
            last_alive = Instant::now();
            dirty = false;
            set_activity(stream, activity.as_ref(), nonce)
        } else if last_alive.elapsed() >= KEEPALIVE {
            // Com o perfil parado, é o ping que descobre que o Discord fechou.
            last_alive = Instant::now();
            write_frame(stream, OP_PING, &json!({ "nonce": "vivo" })).map_err(Failure::Link).and_then(|()| answer(stream).map(|_| ()))
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {}
            Err(Failure::Refused(why)) => {
                // Um campo recusado perde só esta atualização.
                log::warn!("discord recusou a atividade: {why}");
            }
            Err(Failure::Link(error)) => {
                log::info!("discord: canal caiu ({error})");
                pipe = None;
                dirty = true;
                last_try = None;
                set_link(1);
            }
        }
    }
}

/// Junta os pedidos já na fila: só o último importa.
fn continue_if_more(rx: &Receiver<Message>, wanted: &mut Option<(String, Option<Value>)>, dirty: &mut bool, pipe: &mut Option<Pipe>, set_link: &impl Fn(u8)) {
    while let Ok(message) = rx.try_recv() {
        match message {
            Message::Set { app_id, activity } => {
                if wanted.as_ref().is_some_and(|(id, _)| *id != app_id) {
                    *pipe = None;
                }
                *wanted = Some((app_id, activity));
                *dirty = true;
            }
            Message::Stop => {
                *pipe = None;
                *wanted = None;
                *dirty = false;
                set_link(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Um Discord de mentira: entrega o que tem na fila e guarda o que ouve.
    #[derive(Default)]
    struct Fake {
        replies: VecDeque<u8>,
        heard: Vec<u8>,
    }

    impl Fake {
        fn reply(&mut self, op: u32, body: &Value) {
            let mut bytes = Vec::new();
            write_frame(&mut bytes, op, body).unwrap();
            self.replies.extend(bytes);
        }

        fn frames(&self) -> Vec<(u32, Value)> {
            let mut reader = self.heard.as_slice();
            let mut frames = Vec::new();
            while !reader.is_empty() {
                frames.push(read_frame(&mut reader).unwrap());
            }
            frames
        }
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.replies.read(buf)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.heard.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn song() -> Song {
        Song {
            video_id: "abc".into(),
            title: "Plastic Beach".into(),
            artist: "Gorillaz".into(),
            album: Some("Plastic Beach".into()),
            cover: Some("https://lh3.googleusercontent.com/x=w544-h544-l90-rj".into()),
            start_ms: 1_000,
            end_ms: Some(228_000),
        }
    }

    #[test]
    fn o_quadro_tem_op_e_tamanho_em_little_endian() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, OP_FRAME, &json!({ "a": 1 })).unwrap();
        assert_eq!(&bytes[..8], &[1, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(&bytes[8..], br#"{"a":1}"#);
    }

    #[test]
    fn a_saudacao_manda_o_id_e_aceita_o_ready() {
        let mut fake = Fake::default();
        fake.reply(OP_FRAME, &json!({ "cmd": "DISPATCH", "evt": "READY" }));
        handshake(&mut fake, "123").unwrap();
        let (op, body) = &fake.frames()[0];
        assert_eq!(*op, OP_HANDSHAKE);
        assert_eq!(body["client_id"], "123");
        assert_eq!(body["v"], 1);
    }

    #[test]
    fn id_invalido_e_recusa_nao_queda() {
        let mut fake = Fake::default();
        fake.reply(OP_CLOSE, &json!({ "code": 4000, "message": "Invalid Client ID" }));
        assert!(matches!(handshake(&mut fake, "1"), Err(Failure::Refused(_))));
    }

    #[test]
    fn ping_e_respondido_antes_da_resposta() {
        let mut fake = Fake::default();
        fake.reply(OP_PING, &json!({ "n": 1 }));
        fake.reply(OP_FRAME, &json!({ "cmd": "SET_ACTIVITY" }));
        set_activity(&mut fake, None, 1).unwrap();
        let frames = fake.frames();
        assert_eq!(frames[0].0, OP_FRAME);
        assert_eq!(frames[0].1["args"]["activity"], Value::Null);
        assert_eq!(frames[1].0, OP_PONG);
    }

    #[test]
    fn canal_fechado_e_queda() {
        let mut fake = Fake::default();
        assert!(matches!(set_activity(&mut fake, None, 1), Err(Failure::Link(_))));
    }

    #[test]
    fn a_atividade_traz_a_musica_e_a_divulgacao_primeiro() {
        let value = activity(&song(), Some("Baixar o MusicFast"), "Ouvir no YouTube Music");
        assert_eq!(value["type"], 2);
        assert_eq!(value["details"], "Plastic Beach");
        assert_eq!(value["state"], "Gorillaz");
        assert_eq!(value["timestamps"]["end"], 228_000);
        assert_eq!(value["buttons"][0]["label"], "Baixar o MusicFast");
        assert_eq!(value["buttons"][0]["url"], SITE_URL);
        assert_eq!(value["buttons"][1]["url"], "https://music.youtube.com/watch?v=abc");
        assert_eq!(value["assets"]["large_url"], SITE_URL);
    }

    #[test]
    fn sem_divulgacao_o_site_some_de_tudo() {
        let value = activity(&song(), None, "Ouvir");
        assert_eq!(value["buttons"].as_array().unwrap().len(), 1);
        assert_eq!(value["buttons"][0]["url"], "https://music.youtube.com/watch?v=abc");
        assert!(value["assets"].get("large_url").is_none());
    }

    #[test]
    fn os_limites_do_discord_sao_respeitados() {
        let mut short = song();
        short.title = "5".into();
        short.artist = "x".repeat(300);
        short.cover = Some("x".repeat(300));
        let value = activity(&short, Some(&"y".repeat(50)), "Ouvir");
        assert_eq!(value["details"].as_str().unwrap().chars().count(), 2);
        assert_eq!(value["state"].as_str().unwrap().chars().count(), 128);
        assert!(value.get("assets").is_none());
        assert_eq!(value["buttons"][0]["label"].as_str().unwrap().chars().count(), 32);
    }

    /// Fala com o Discord aberto no computador, sem publicar nada: um id que
    /// não existe só pode ser recusado na saudação.
    #[test]
    #[ignore = "precisa do Discord aberto"]
    fn o_discord_de_verdade_responde_a_saudacao() {
        let mut pipe = open_pipe().expect("Discord aberto");
        match handshake(&mut pipe, "1") {
            Err(Failure::Refused(why)) => println!("recusado: {why}"),
            other => panic!("esperava recusa: {other:?}"),
        }
    }

    /// O id do app vale de verdade. Só a saudação: nada vai para o perfil.
    #[test]
    #[ignore = "precisa do Discord aberto"]
    fn o_discord_de_verdade_aceita_o_id_do_app() {
        let mut pipe = open_pipe().expect("Discord aberto");
        handshake(&mut pipe, APP_ID).expect("id aceito");
    }

    #[test]
    fn a_divulgacao_vem_ligada_mesmo_em_sessao_antiga() {
        let old: Settings = serde_json::from_str(r#"{"on":true,"app_id":"1","site_label":"x","site_url":"y"}"#).unwrap();
        assert_eq!(old, Settings { on: true, promote: true });
        assert_eq!(Settings::default(), Settings { on: false, promote: true });
    }
}
