//! Letras sincronizadas, como as da extensão Better Lyrics: o LRCLIB
//! (lrclib.net, aberto e sem chave) dá a letra com o tempo de cada linha; a
//! do YouTube Music (sem tempo) fica de reserva.
//!
//! Sem o tempo de cada palavra no LRC, ele é estimado: a linha é cantada no
//! começo do seu trecho, cada palavra com tempo proporcional às letras.

use std::time::Duration;

use musicfast_ytm::{Client, Track};
use serde_json::Value;

/// A letra pronta para a tela.
#[derive(Clone, Debug, Default)]
pub struct Song {
    /// Com tempo; vazia quando só há a letra corrida.
    pub lines: Vec<Line>,
    /// A letra sem tempo (a de reserva).
    pub plain: String,
    /// De onde veio, para o rodapé.
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    /// Vazio num trecho instrumental (o "♪" do LRC).
    pub text: String,
    pub words: Vec<Word>,
}

impl Line {
    pub fn is_gap(&self) -> bool {
        self.text.is_empty()
    }
}

/// Uma palavra da linha, com o espaço que vem depois dela incluído no
/// `len` (contado em caracteres, não bytes).
#[derive(Clone, Copy, Debug)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    /// Onde começa na linha e quantos caracteres tem, em `char`.
    pub from: usize,
    pub len: usize,
}

const LRCLIB: &str = "https://lrclib.net/api";
const AGENT: &str = concat!("MusicFast/", env!("CARGO_PKG_VERSION"), " (cliente nativo do YouTube Music)");

/// Procura a letra: a sincronizada do LRCLIB, senão a do YouTube Music,
/// senão a corrida do LRCLIB. `Ok(None)`: nenhuma das fontes tem.
pub fn fetch(client: &Client, http: &reqwest::blocking::Client, track: &Track, lyrics_id: Option<&str>) -> Result<Option<Song>, String> {
    let lrclib = lrclib(http, track);
    if let Ok(Some(found)) = &lrclib
        && !found.lines.is_empty()
    {
        return Ok(Some(found.clone()));
    }
    let mut ytm_error = None;
    if let Some(id) = lyrics_id {
        match client.lyrics(id) {
            Ok(Some(l)) if !l.text.trim().is_empty() => {
                return Ok(Some(Song { lines: Vec::new(), plain: l.text, source: l.source }));
            }
            Ok(_) => {}
            Err(e) => ytm_error = Some(e.to_string()),
        }
    }
    match lrclib {
        Ok(Some(found)) => Ok(Some(found)),
        Ok(None) => ytm_error.map_or(Ok(None), Err),
        Err(e) => Err(ytm_error.unwrap_or(e)),
    }
}

/// O LRCLIB: primeiro o pedido exato (título, artista, álbum, duração),
/// depois a busca, escolhendo a de duração mais perto.
fn lrclib(http: &reqwest::blocking::Client, track: &Track) -> Result<Option<Song>, String> {
    let artist = track.artists.first().map(|a| a.name.as_str()).unwrap_or_default();
    let title = clean_title(&track.title);
    let duration = track.duration_secs.map(f64::from);
    let mut params = vec![("track_name", title.clone()), ("artist_name", artist.to_owned())];
    if let Some(album) = &track.album {
        params.push(("album_name", album.name.clone()));
    }
    if let Some(d) = track.duration_secs {
        params.push(("duration", d.to_string()));
    }
    if let Some(hit) = get(http, "get", &params)?
        && let Some(song) = song_from(&hit, duration)
    {
        return Ok(Some(song));
    }
    let Some(Value::Array(hits)) = get(http, "search", &[("track_name", title), ("artist_name", artist.to_owned())])? else {
        return Ok(None);
    };
    // Só as de duração parecida: a mesma música noutra versão tem outro tempo.
    let close = |v: &&Value| match (duration, v["duration"].as_f64()) {
        (Some(want), Some(got)) => (want - got).abs() <= 4.0,
        _ => true,
    };
    let synced = hits.iter().filter(close).find(|v| v["syncedLyrics"].as_str().is_some_and(|s| !s.trim().is_empty()));
    let any = synced.or_else(|| hits.iter().filter(close).find(|v| v["plainLyrics"].as_str().is_some_and(|s| !s.trim().is_empty())));
    Ok(any.and_then(|v| song_from(v, duration)))
}

/// Um GET no LRCLIB. 404 é "não tem" (`None`); "servidor ocupado" (503,
/// comum lá) tenta de novo duas vezes.
fn get(http: &reqwest::blocking::Client, path: &str, params: &[(&str, String)]) -> Result<Option<Value>, String> {
    let url = reqwest::Url::parse_with_params(&format!("{LRCLIB}/{path}"), params).map_err(|e| e.to_string())?;
    let mut last = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(700 * attempt));
        }
        let response = match http.get(url.clone()).header("User-Agent", AGENT).timeout(Duration::from_secs(10)).send() {
            Ok(r) => r,
            Err(e) => {
                last = format!("LRCLIB: {e}");
                continue;
            }
        };
        match response.status().as_u16() {
            404 => return Ok(None),
            200 => return response.json::<Value>().map(Some).map_err(|e| format!("LRCLIB: {e}")),
            code @ (429 | 500..=599) => last = format!("LRCLIB ocupado (HTTP {code})"),
            code => return Err(format!("LRCLIB: HTTP {code}")),
        }
    }
    Err(last)
}

fn song_from(v: &Value, duration: Option<f64>) -> Option<Song> {
    if v["instrumental"].as_bool() == Some(true) {
        return Some(Song { lines: Vec::new(), plain: "♪ Música instrumental".to_owned(), source: "Letra: LRCLIB".to_owned() });
    }
    let lines = v["syncedLyrics"].as_str().map(|s| parse_lrc(s, duration.or(v["duration"].as_f64()))).unwrap_or_default();
    let plain = v["plainLyrics"].as_str().unwrap_or_default().to_owned();
    if lines.is_empty() && plain.trim().is_empty() {
        return None;
    }
    Some(Song { lines, plain, source: "Letra: LRCLIB".to_owned() })
}

/// Tira o que o YouTube põe no título e o LRCLIB não tem: "(Official
/// Video)", "[Lyrics]", "(feat. …)".
fn clean_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut depth = 0i32;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.split(" - Official").next().unwrap_or(&out).trim().to_owned();
    if out.is_empty() { title.to_owned() } else { out }
}

/// `mm:ss.xx` (ou `mm:ss`, ou `mm:ss:xx`) em segundos.
fn timestamp(s: &str) -> Option<f64> {
    let (min, rest) = s.split_once(':')?;
    let rest = rest.replacen(':', ".", 1);
    let min: f64 = min.trim().parse().ok()?;
    let sec: f64 = rest.trim().parse().ok()?;
    (min >= 0.0 && sec >= 0.0).then_some(min * 60.0 + sec)
}

/// Lê um LRC: `[mm:ss.xx]texto`, várias marcas na mesma linha, `[offset:]`,
/// e o LRC "enhanced" com `<mm:ss.xx>` antes de cada palavra.
pub fn parse_lrc(text: &str, duration: Option<f64>) -> Vec<Line> {
    let mut offset = 0.0;
    // (início, texto, palavras com tempo do LRC enhanced)
    let mut raw: Vec<(f64, String, Vec<(f64, String)>)> = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim();
        let mut starts = Vec::new();
        while let Some(tail) = rest.strip_prefix('[') {
            let Some(close) = tail.find(']') else { break };
            let tag = &tail[..close];
            if let Some(t) = timestamp(tag) {
                starts.push(t);
            } else if let Some(ms) = tag.strip_prefix("offset:") {
                // Positivo adianta a letra.
                offset = ms.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
            }
            rest = &tail[close + 1..];
        }
        if starts.is_empty() {
            continue;
        }
        let (plain, words) = enhanced(rest.trim());
        for s in starts {
            raw.push((s, plain.clone(), words.clone()));
        }
    }
    raw.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut lines: Vec<Line> = Vec::with_capacity(raw.len() + 1);
    for (i, (start, text, timed)) in raw.iter().enumerate() {
        let start = (start - offset).max(0.0);
        let end = raw.get(i + 1).map(|n| (n.0 - offset).max(start)).unwrap_or_else(|| duration.unwrap_or(start + 6.0).max(start + 2.0));
        let text = if text.chars().all(|c| matches!(c, '♪' | '♫' | '♬' | '*' | ' ' | '-' | '.')) { String::new() } else { text.clone() };
        // Duas linhas vazias seguidas viram um trecho só.
        if text.is_empty()
            && let Some(prev) = lines.last_mut()
            && prev.is_gap()
        {
            prev.end = end;
            continue;
        }
        let words = if text.is_empty() {
            Vec::new()
        } else if !timed.is_empty() {
            timed_words(timed, end)
        } else {
            estimate_words(&text, start, end)
        };
        lines.push(Line { start, end, text, words });
    }
    // Um começo instrumental longo também ganha os pontinhos.
    if let Some(first) = lines.first()
        && first.start > 3.0
        && !first.is_gap()
    {
        let end = first.start;
        lines.insert(0, Line { start: 0.0, end, text: String::new(), words: Vec::new() });
    }
    lines
}

/// Separa as marcas `<mm:ss.xx>` do texto: o texto limpo e cada pedaço com
/// o tempo em que começa.
fn enhanced(s: &str) -> (String, Vec<(f64, String)>) {
    if !s.contains('<') {
        return (s.to_owned(), Vec::new());
    }
    let mut words = Vec::new();
    let mut plain = String::new();
    let mut rest = s;
    let mut current: Option<f64> = None;
    loop {
        let (before, after) = match rest.find('<') {
            Some(i) => (&rest[..i], Some(&rest[i + 1..])),
            None => (rest, None),
        };
        if !before.is_empty() {
            plain.push_str(before);
            if let Some(t) = current {
                words.push((t, before.to_owned()));
            }
        }
        let Some(after) = after else { break };
        let Some(close) = after.find('>') else {
            plain.push_str(after);
            break;
        };
        current = timestamp(&after[..close]).or(current);
        rest = &after[close + 1..];
    }
    // O texto pode ter espaços sobrando entre as marcas.
    let trimmed = plain.trim_start();
    let cut = plain.chars().count() - trimmed.chars().count();
    let plain = trimmed.trim_end().to_owned();
    let mut from = 0usize;
    let mut out = Vec::new();
    for (t, w) in words {
        let n = w.chars().count();
        if from + n > cut {
            out.push((t, w));
        }
        from += n;
    }
    (plain, out)
}

fn timed_words(timed: &[(f64, String)], line_end: f64) -> Vec<Word> {
    let mut words = Vec::with_capacity(timed.len());
    let mut from = 0usize;
    for (i, (start, text)) in timed.iter().enumerate() {
        // O espaço antes do primeiro pedaço foi cortado do texto da linha.
        let text = if i == 0 { text.trim_start() } else { text.as_str() };
        let len = text.chars().count();
        let end = timed.get(i + 1).map_or(line_end, |n| n.0).max(*start);
        words.push(Word { start: *start, end, from, len });
        from += len;
    }
    words
}

/// Sem o tempo das palavras: a linha é cantada em ~0,1 s por letra (no
/// máximo o trecho dela), e cada palavra leva o tempo das suas letras.
fn estimate_words(text: &str, start: f64, end: f64) -> Vec<Word> {
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len().max(1) as f64;
    let sung = (end - start).min((total * 0.1 + 0.4).max(0.8)).max(0.05);
    let mut words = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let from = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        // Japonês e chinês não têm espaço: cada caractere vira uma "palavra",
        // senão a linha inteira acenderia de uma vez.
        if i - from > 1 && chars[from..i].iter().all(|c| is_cjk(*c) || c.is_whitespace()) {
            for k in from..i {
                words.push((k, 1));
            }
        } else {
            words.push((from, i - from));
        }
    }
    words
        .into_iter()
        .map(|(from, len)| Word {
            start: start + sung * from as f64 / total,
            end: start + sung * (from + len) as f64 / total,
            from,
            len,
        })
        .collect()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF66..=0xFF9F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_lrc_simples() {
        let lines = parse_lrc("[ar:x]\n[00:13.13] Yeah\n[00:16.56] ♪\n[00:27.16] I've been tryna call\n", Some(40.0));
        assert_eq!(lines.len(), 4);
        assert!(lines[0].is_gap());
        assert_eq!(lines[1].text, "Yeah");
        assert!((lines[1].end - 16.56).abs() < 1e-9);
        assert!(lines[2].is_gap());
        assert_eq!(lines[3].words.len(), 4);
        assert!((lines[3].end - 40.0).abs() < 1e-9);
    }

    #[test]
    fn le_lrc_enhanced() {
        let lines = parse_lrc("[00:01.00] <00:01.00>Hello <00:01.50>world\n[00:03.00]fim", None);
        assert_eq!(lines[0].text, "Hello world");
        assert_eq!(lines[0].words.len(), 2);
        assert!((lines[0].words[1].start - 1.5).abs() < 1e-9);
        assert_eq!(lines[0].words[1].from, 6);
    }

    #[test]
    fn limpa_titulo() {
        assert_eq!(clean_title("Song (Official Video) [4K]"), "Song");
        assert_eq!(clean_title("(Wed)"), "(Wed)");
    }
}
