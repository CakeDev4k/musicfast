//! Letras sincronizadas, como as da extensão Better Lyrics: o LRCLIB
//! (lrclib.net, aberto e sem chave) e o NetEase Cloud Music dão a letra com
//! o tempo de cada linha; a do YouTube Music (sem tempo) fica de reserva.
//! Uma versão "slowed" ou "sped up" sem letra própria usa a da original,
//! com os tempos esticados.
//!
//! Sem o tempo de cada palavra no LRC, ele é estimado: a linha é cantada no
//! começo do seu trecho, cada palavra com tempo proporcional às letras.

use std::time::Duration;

use musicfast_ytm::{Client, Track};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A letra pronta para a tela (e para o cache em disco).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Song {
    /// Com tempo; vazia quando só há a letra corrida.
    pub lines: Vec<Line>,
    /// A letra sem tempo (a de reserva).
    pub plain: String,
    /// De onde veio, para o rodapé.
    pub source: String,
    /// O tempo de cada palavra é o de verdade (o karaokê do NetEase), não
    /// estimado pelas letras.
    pub words_timed: bool,
    /// Há indício de que existe outra letra (duas fontes discordam, a busca
    /// foi solta, a letra foi esticada): só então a tela oferece "Outra letra".
    pub doubtful: bool,
    /// O idioma da letra, como o tradutor o detectou ("en", "ja").
    pub lang: Option<String>,
    /// Para qual idioma e por qual serviço as linhas têm `translation`
    /// ("pt:google", `LyricsStyle::translation_key`); `None` antes de passar
    /// pelo tradutor.
    pub translated_to: Option<String>,
    /// O serviço que traduziu de fato (`TranslateService::key`): pode não
    /// ser o escolhido, quando ele falhou e o outro entrou no lugar.
    pub translated_by: Option<String>,
}

impl Song {
    /// A letra está noutro idioma que `target` (e a tradução faz sentido).
    pub fn foreign_to(&self, target: &str) -> bool {
        self.lang.as_deref().is_some_and(|l| base_lang(l) != base_lang(target))
    }
}

/// "pt-BR" → "pt"; "zh-CN" fica "zh".
fn base_lang(code: &str) -> &str {
    code.split(['-', '_']).next().unwrap_or(code)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    /// Vazio num trecho instrumental (o "♪" do LRC).
    pub text: String,
    pub words: Vec<Word>,
    /// A pronúncia em letras latinas (o romaji do japonês), quando há.
    pub romaji: Option<String>,
    /// A linha no idioma do app (`Song::translated_to`).
    pub translation: Option<String>,
}

impl Line {
    pub fn is_gap(&self) -> bool {
        self.text.is_empty()
    }
}

/// Uma palavra da linha, com o espaço que vem depois dela incluído no
/// `len` (contado em caracteres, não bytes).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    /// Onde começa na linha e quantos caracteres tem, em `char`.
    pub from: usize,
    pub len: usize,
}

const LRCLIB: &str = "https://lrclib.net/api";
const NETEASE: &str = "https://music.163.com/api";
const AGENT: &str = concat!("MusicFast/", env!("CARGO_PKG_VERSION"), " (cliente nativo do YouTube Music)");

/// O que se procura: o título limpo, os artistas, o álbum e a duração, e,
/// numa versão acelerada ou desacelerada ("slowed", "sped up", "nightcore"),
/// que ela é uma (a letra da original serve, esticada).
struct Query {
    title: String,
    artists: Vec<String>,
    album: Option<String>,
    duration: Option<f64>,
    altered: bool,
}

impl Query {
    fn new(track: &Track) -> Self {
        let lower = track.title.to_lowercase();
        let altered = ["slowed", "sped up", "speed up", "spedup", "nightcore", "daycore", "reverb"].iter().any(|w| lower.contains(w));
        Self {
            title: clean_title(&track.title),
            artists: track.artists.iter().map(|a| a.name.clone()).filter(|a| !a.is_empty()).collect(),
            album: track.album.as_ref().map(|a| a.name.clone()),
            duration: track.duration_secs.map(f64::from),
            altered,
        }
    }

    fn artist(&self) -> &str {
        self.artists.first().map_or("", String::as_str)
    }

    /// A duração de um resultado bate com a desta música.
    fn close(&self, got: Option<f64>, slack: f64) -> bool {
        match (self.duration, got) {
            (Some(want), Some(got)) => (want - got).abs() <= slack,
            _ => true,
        }
    }

    /// O título de um resultado é o desta música (sem pontuação, maiúsculas
    /// nem o que vem entre parênteses).
    fn same_title(&self, other: &str) -> bool {
        let (a, b) = (normalize(&self.title), normalize(&clean_title(other)));
        let (short, long) = (a.chars().count().min(b.chars().count()), a.chars().count().max(b.chars().count()));
        // Um dentro do outro só vale se forem de tamanho parecido: "Feel" não
        // é "Feel Good Inc.".
        !a.is_empty() && (a == b || (short >= 4 && short * 10 >= long * 6 && (a.contains(&b) || b.contains(&a))))
    }

    /// Algum artista desta música aparece no resultado.
    fn same_artist(&self, other: &str) -> bool {
        let other = normalize(other);
        self.artists.iter().map(|a| normalize(a)).any(|a| !a.is_empty() && (other.contains(&a) || a.contains(&other)))
    }
}

/// "(Instrumental)", "- Karaoke Version", "(Inst.)", "Off Vocal".
fn is_instrumental_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    ["instrumental", "karaoke", "(inst", "[inst", "- inst", "off vocal", "backing track"].iter().any(|w| lower.contains(w))
}

/// Só letras e números, em minúsculas: "Feel Good Inc." e "feel good inc" batem.
fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Procura a letra, a sincronizada antes de tudo: o LRCLIB e o NetEase ao
/// mesmo tempo; numa versão acelerada ou desacelerada sem letra própria, a
/// da original no tempo desta; senão a do YouTube Music (sem tempo), senão
/// qualquer texto achado. `Ok(None)`: nenhuma fonte tem. Um erro só volta
/// quando nenhuma fonte respondeu.
pub fn fetch(client: &Client, http: &reqwest::blocking::Client, track: &Track, lyrics_id: Option<&str>) -> Result<Option<Song>, String> {
    // A versão instrumental (ou karaokê) não tem letra: buscar acharia a da
    // versão cantada, de mesma duração.
    if is_instrumental_title(&track.title) {
        let t = musicfast_i18n::t();
        return Ok(Some(Song { plain: t.instrumental.to_owned(), ..Song::default() }));
    }
    let q = Query::new(track);
    // As três fontes ao mesmo tempo.
    let (lrclib, netease, ytm) = std::thread::scope(|s| {
        let other = s.spawn(|| netease(http, &q));
        let youtube = s.spawn(|| lyrics_id.map(|id| client.lyrics(id).map_err(|e| e.to_string())));
        let lrclib = lrclib(http, &q);
        (
            lrclib,
            other.join().unwrap_or_else(|_| Err("NetEase: falha interna".to_owned())),
            youtube.join().unwrap_or(None),
        )
    });
    let ytm: Option<Result<Option<Song>, String>> = ytm.map(|r| r.map(|found| found.and_then(|l| from_ytm(l, q.duration))));

    // A ordem das sincronizadas: o karaokê de verdade do NetEase (mesma
    // duração), a do YouTube Music (da mesma gravação), a do LRCLIB e a do
    // NetEase linha a linha.
    let synced = |r: Option<&Result<Option<Song>, String>>| r.and_then(|r| r.as_ref().ok()).and_then(|s| s.as_ref()).filter(|s| !s.lines.is_empty()).cloned();
    let netease_song = synced(Some(&netease));
    let ytm_song = synced(ytm.as_ref());
    let lrclib_song = synced(Some(&lrclib));
    let mut order: Vec<Song> = Vec::new();
    if let Some(s) = netease_song.clone().filter(|s| s.words_timed) {
        order.push(s);
    }
    order.extend(ytm_song);
    order.extend(lrclib_song);
    order.extend(netease_song.filter(|s| !s.words_timed));
    if let Some(mut chosen) = order.first().cloned() {
        // Outra fonte com letra bem diferente: há outra para oferecer.
        if order.iter().skip(1).any(|other| !similar(&chosen, other)) {
            chosen.doubtful = true;
        }
        if let Ok(Some(n)) = &netease {
            attach_romaji(&mut chosen, n);
        }
        return Ok(Some(chosen));
    }
    if q.altered
        && let Some(mut song) = stretched(http, &q)
    {
        if let Ok(Some(n)) = &netease {
            attach_romaji(&mut song, n);
        }
        song.doubtful = true;
        return Ok(Some(song));
    }

    // Só texto: a do YouTube Music, senão qualquer uma achada.
    let mut errors: Vec<String> = [Some(&lrclib), Some(&netease), ytm.as_ref()].into_iter().flatten().filter_map(|r| r.as_ref().err().cloned()).collect();
    for found in [ytm, Some(lrclib), Some(netease)].into_iter().flatten() {
        if let Ok(Some(song)) = found {
            return Ok(Some(song));
        }
    }
    // Um erro só conta quando nenhuma fonte disse "não tem".
    if errors.len() >= 2 + usize::from(lyrics_id.is_some()) {
        return Err(errors.remove(0));
    }
    Ok(None)
}

/// A letra do YouTube Music como `Song`: com tempo, as linhas viram um LRC
/// (com os trechos instrumentais longos marcados) e passam pelo mesmo
/// leitor; sem tempo, o texto.
fn from_ytm(l: musicfast_ytm::Lyrics, duration: Option<f64>) -> Option<Song> {
    if l.text.trim().is_empty() {
        return None;
    }
    if l.lines.is_empty() {
        return Some(Song { plain: l.text, source: l.source, ..Song::default() });
    }
    let stamp = |t: f64| format!("[{:02}:{:05.2}]", (t / 60.0) as u32, t % 60.0);
    let mut lrc = String::new();
    for (i, line) in l.lines.iter().enumerate() {
        lrc.push_str(&format!("{}{}\n", stamp(line.start), line.text));
        // Os pontinhos só num buraco entre duas linhas, não depois da última.
        if let Some(next) = l.lines.get(i + 1)
            && next.start - line.end > 4.0
        {
            lrc.push_str(&format!("{}\n", stamp(line.end)));
        }
    }
    Some(Song { lines: parse_lrc(&lrc, duration), plain: l.text, source: l.source, ..Song::default() })
}

/// Duas letras são a mesma (a mesma música com pontuação ou uma palavra
/// diferente): metade ou mais das palavras do começo em comum.
fn similar(a: &Song, b: &Song) -> bool {
    let words = |s: &Song| -> std::collections::HashSet<String> {
        s.lines.iter().flat_map(|l| l.text.split_whitespace()).map(normalize).filter(|w| !w.is_empty()).take(60).collect()
    };
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return true;
    }
    let common = wa.intersection(&wb).count();
    common * 2 >= wa.len().min(wb.len())
}

/// Põe nas linhas de `song` o romaji das linhas do NetEase: a de mesmo
/// texto, senão a de tempo mais perto.
fn attach_romaji(song: &mut Song, netease: &Song) {
    let with: Vec<&Line> = netease.lines.iter().filter(|l| l.romaji.is_some()).collect();
    if with.is_empty() || std::ptr::eq(song, netease) {
        return;
    }
    for line in song.lines.iter_mut().filter(|l| !l.is_gap() && l.romaji.is_none()) {
        let key = normalize(&line.text);
        let same = with.iter().filter(|n| normalize(&n.text) == key).min_by(|a, b| (a.start - line.start).abs().total_cmp(&(b.start - line.start).abs()));
        let near = || with.iter().filter(|n| (n.start - line.start).abs() < 0.8).min_by(|a, b| (a.start - line.start).abs().total_cmp(&(b.start - line.start).abs()));
        if let Some(n) = same.or_else(near) {
            line.romaji = n.romaji.clone();
        }
    }
}

/// O LRCLIB: o pedido exato (título, artista, álbum, duração), a busca por
/// título e artista e, por fim, só pelo título (o artista pode estar escrito
/// de outro jeito lá: "Mao Sasagawa" e "笹川真生"). A primeira sincronizada
/// de duração parecida ganha; a corrida fica de reserva.
fn lrclib(http: &reqwest::blocking::Client, q: &Query) -> Result<Option<Song>, String> {
    let mut plain: Option<Song> = None;
    let mut failures = 0;
    let mut tries = 0;
    if !q.artist().is_empty() {
        tries += 1;
        let mut params = vec![("track_name", q.title.clone()), ("artist_name", q.artist().to_owned())];
        if let Some(album) = &q.album {
            params.push(("album_name", album.clone()));
        }
        if let Some(d) = q.duration {
            params.push(("duration", d.round().to_string()));
        }
        match get(http, "get", &params) {
            Ok(Some(hit)) => {
                if let Some(song) = song_from(&hit, q.duration) {
                    if !song.lines.is_empty() {
                        return Ok(Some(song));
                    }
                    plain.get_or_insert(song);
                }
            }
            Ok(None) => {}
            Err(_) => failures += 1,
        }
    }
    let mut searches: Vec<Vec<(&str, String)>> = Vec::new();
    if !q.artist().is_empty() {
        searches.push(vec![("track_name", q.title.clone()), ("artist_name", q.artist().to_owned())]);
    }
    searches.push(vec![("q", q.title.clone())]);
    for (i, params) in searches.iter().enumerate() {
        tries += 1;
        let hits = match get(http, "search", params) {
            Ok(Some(Value::Array(hits))) => hits,
            Ok(_) => continue,
            Err(_) => {
                failures += 1;
                continue;
            }
        };
        // Só pelo título, o resultado precisa do mesmo título e de artista
        // ou duração batendo de perto.
        let title_only = i == searches.len() - 1;
        let fits = |v: &&Value| {
            let duration = v["duration"].as_f64();
            if title_only {
                q.same_title(v["trackName"].as_str().unwrap_or_default())
                    && (q.same_artist(v["artistName"].as_str().unwrap_or_default()) && q.close(duration, 4.0) || q.close(duration, 2.0) && q.duration.is_some())
            } else {
                q.close(duration, 4.0)
            }
        };
        let synced = hits.iter().filter(fits).find(|v| v["syncedLyrics"].as_str().is_some_and(|s| !s.trim().is_empty()));
        if let Some(v) = synced
            && let Some(mut song) = song_from(v, q.duration)
        {
            // Só pelo título, ou de duração não tão perto: pode haver outra.
            let off = !q.close(v["duration"].as_f64(), 2.0);
            song.doubtful = title_only || off;
            return Ok(Some(song));
        }
        if plain.is_none() {
            plain = hits.iter().filter(fits).find_map(|v| song_from(v, q.duration));
        }
    }
    if plain.is_none() && failures == tries {
        return Err(musicfast_i18n::t().lrclib_busy(503));
    }
    Ok(plain)
}

/// O NetEase Cloud Music: a busca por "título artista" e só pelo título, os
/// de título igual e duração parecida, e a letra do primeiro que tiver.
/// Cobre bem o que o LRCLIB não tem (música japonesa, chinesa, coreana).
fn netease(http: &reqwest::blocking::Client, q: &Query) -> Result<Option<Song>, String> {
    let mut queries = vec![q.title.clone()];
    if !q.artist().is_empty() {
        queries.insert(0, format!("{} {}", q.title, q.artist()));
    }
    let mut plain = None;
    let mut answered = false;
    let mut last_error = None;
    for query in queries {
        let songs = match netease_search(http, &query) {
            Ok(songs) => songs,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };
        answered = true;
        for (name, artists, duration, id) in songs {
            let fits = q.same_title(&name) && (q.close(Some(duration), 3.0) && q.duration.is_some() || q.same_artist(&artists) && q.close(Some(duration), 4.0));
            if !fits {
                continue;
            }
            match netease_lyric(http, id, q.duration) {
                Ok(Some(song)) if !song.lines.is_empty() => return Ok(Some(song)),
                Ok(Some(song)) => {
                    plain.get_or_insert(song);
                }
                _ => {}
            }
        }
    }
    match (answered, last_error) {
        (false, Some(e)) => Err(e),
        _ => Ok(plain),
    }
}

/// Os resultados da busca do NetEase: (título, artistas, duração em s, id).
fn netease_search(http: &reqwest::blocking::Client, query: &str) -> Result<Vec<(String, String, f64, u64)>, String> {
    let url = reqwest::Url::parse_with_params(&format!("{NETEASE}/search/get"), &[("s", query), ("type", "1"), ("limit", "10")])
        .map_err(|e| e.to_string())?;
    let v: Value = http
        .get(url)
        .header("Referer", "https://music.163.com/")
        .header("User-Agent", AGENT)
        .timeout(Duration::from_secs(8))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("NetEase: {e}"))?;
    let songs = v["result"]["songs"].as_array().cloned().unwrap_or_default();
    Ok(songs
        .iter()
        .filter_map(|s| {
            let artists: Vec<&str> = s["artists"].as_array()?.iter().filter_map(|a| a["name"].as_str()).collect();
            Some((s["name"].as_str()?.to_owned(), artists.join(" "), s["duration"].as_f64()? / 1000.0, s["id"].as_u64()?))
        })
        .collect())
}

/// A letra de uma música do NetEase, sem os créditos que ele põe no começo.
fn netease_lyric(http: &reqwest::blocking::Client, id: u64, duration: Option<f64>) -> Result<Option<Song>, String> {
    // A v1 manda de uma vez a letra, o karaokê (`yrc`) e o romaji.
    let url = format!("{NETEASE}/song/lyric/v1?id={id}&lv=1&yv=1&rv=-1&tv=-1&kv=-1");
    let v: Value = http
        .get(url)
        .header("Referer", "https://music.163.com/")
        .header("User-Agent", AGENT)
        .timeout(Duration::from_secs(8))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("NetEase: {e}"))?;
    let t = musicfast_i18n::t();
    if v["nolyric"].as_bool() == Some(true) || v["pureMusic"].as_bool() == Some(true) {
        return Ok(Some(Song { plain: t.instrumental.to_owned(), source: t.lyrics_by_netease.to_owned(), ..Song::default() }));
    }
    let lrc = v["lrc"]["lyric"].as_str().unwrap_or_default();
    let cleaned: String = lrc.lines().filter(|l| !is_credit(l)).collect::<Vec<_>>().join("\n");
    let lrc_lines = parse_lrc(&cleaned, duration);
    let plain: String = lrc_lines.iter().map(|l| l.text.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n");
    if plain.trim().is_empty() {
        return Ok(None);
    }
    // O karaokê, quando há, no lugar das linhas do LRC.
    let yrc = parse_yrc(v["yrc"]["lyric"].as_str().unwrap_or_default());
    let words_timed = yrc.len() >= 3;
    let mut lines = if words_timed { yrc } else { lrc_lines };
    // Sem tempo de verdade (tudo em 00:00), é só texto.
    if lines.iter().filter(|l| l.start > 0.0).count() < 2 {
        lines.clear();
    }
    let mut song = Song { lines, plain, source: t.lyrics_by_netease.to_owned(), words_timed, ..Song::default() };
    // O romaji (das letras japonesa e coreana), linha a linha pelo tempo.
    let roma = parse_lrc(v["romalrc"]["lyric"].as_str().unwrap_or_default(), duration);
    for line in song.lines.iter_mut().filter(|l| !l.is_gap()) {
        line.romaji = roma
            .iter()
            .filter(|r| !r.is_gap() && (r.start - line.start).abs() < 0.8)
            .min_by(|a, b| (a.start - line.start).abs().total_cmp(&(b.start - line.start).abs()))
            .map(|r| r.text.clone());
    }
    Ok(Some(song))
}

/// O karaokê do NetEase: `[início,duração](início,duração,0)palavra(…)…`, em
/// ms. As linhas de crédito vêm em JSON (`{"t":…}`) e ficam de fora.
fn parse_yrc(text: &str) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for raw in text.lines() {
        let Some(rest) = raw.trim().strip_prefix('[') else { continue };
        let Some((head, mut body)) = rest.split_once(']') else { continue };
        let Some((start, dur)) = head.split_once(',') else { continue };
        let (Ok(start), Ok(dur)) = (start.trim().parse::<f64>(), dur.trim().parse::<f64>()) else { continue };
        let mut words = Vec::new();
        let mut line_text = String::new();
        while let Some(open) = body.find('(') {
            let Some(close) = body[open..].find(')') else { break };
            let tag = &body[open + 1..open + close];
            let after = &body[open + close + 1..];
            let piece_end = after.find('(').unwrap_or(after.len());
            let piece = &after[..piece_end];
            let mut nums = tag.split(',').map(|n| n.trim().parse::<f64>());
            if let (Some(Ok(ws)), Some(Ok(wd))) = (nums.next(), nums.next()) {
                let from = line_text.chars().count();
                line_text.push_str(piece);
                words.push(Word { start: ws / 1000.0, end: (ws + wd) / 1000.0, from, len: piece.chars().count() });
            } else {
                line_text.push_str(piece);
            }
            body = &after[piece_end..];
        }
        if line_text.trim().is_empty() || is_credit(&line_text) {
            continue;
        }
        let start = start / 1000.0;
        let end = start + dur / 1000.0;
        // Um trecho sem voz longo entre duas linhas ganha os pontinhos.
        if let Some(prev) = lines.last()
            && start - prev.end > 5.0
        {
            lines.push(Line { start: prev.end, end: start, ..Line::default() });
        }
        lines.push(Line { start, end, text: line_text.trim_end().to_owned(), words, ..Line::default() });
    }
    if let Some(first) = lines.first()
        && first.start > 3.0
    {
        let end = first.start;
        lines.insert(0, Line { start: 0.0, end, ..Line::default() });
    }
    lines
}

/// As linhas de crédito do NetEase ("作词 : …", "Composed by: …") e o
/// "纯音乐，请欣赏" (só instrumental).
fn is_credit(line: &str) -> bool {
    let text = line.rsplit(']').next().unwrap_or(line).trim();
    const KEYS: [&str; 16] = [
        "作词", "作曲", "编曲", "制作人", "制作", "混音", "母带", "和声", "录音", "监制", "出品", "发行", "纯音乐", "Lyrics by", "Composed by", "Produced by",
    ];
    KEYS.iter().any(|k| text.starts_with(k) && (text.contains(':') || text.contains('：') || *k == "纯音乐"))
}

/// Numa versão acelerada ou desacelerada sem letra própria: a sincronizada
/// da original (qualquer duração razoável), com os tempos esticados na
/// razão entre as durações. "Otoshi (Slowed)" usa a de "Otoshi".
fn stretched(http: &reqwest::blocking::Client, q: &Query) -> Option<Song> {
    let want = q.duration?;
    let ratio_ok = |got: f64| {
        let r = want / got;
        (0.6..=1.6).contains(&r) && (want - got).abs() > 2.0
    };
    let mut found: Option<(Song, f64)> = None;
    if let Ok(Some(Value::Array(hits))) = get(http, "search", &[("track_name", q.title.clone()), ("artist_name", q.artist().to_owned())]) {
        found = hits.iter().find_map(|v| {
            let got = v["duration"].as_f64()?;
            let synced = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
            let lower = v["trackName"].as_str().unwrap_or_default().to_lowercase();
            // A original: sem "slowed" no nome, de duração em proporção.
            if lower.contains("slowed") || lower.contains("sped") || !ratio_ok(got) {
                return None;
            }
            Some((Song { lines: parse_lrc(synced, Some(got)), plain: v["plainLyrics"].as_str().unwrap_or_default().to_owned(), source: musicfast_i18n::t().lyrics_by_lrclib.to_owned(), ..Song::default() }, got))
        });
    }
    if found.is_none()
        && let Ok(songs) = netease_search(http, &format!("{} {}", q.title, q.artist()))
    {
        found = songs.into_iter().find_map(|(name, artists, got, id)| {
            let lower = name.to_lowercase();
            if !q.same_title(&name) || !q.same_artist(&artists) || lower.contains("slowed") || lower.contains("sped") || !ratio_ok(got) {
                return None;
            }
            let song = netease_lyric(http, id, Some(got)).ok()??;
            (!song.lines.is_empty()).then_some((song, got))
        });
    }
    let (mut song, got) = found?;
    let r = want / got;
    for line in &mut song.lines {
        line.start *= r;
        line.end *= r;
        for w in &mut line.words {
            w.start *= r;
            w.end *= r;
        }
    }
    song.source = format!("{} · {}", song.source, musicfast_i18n::t().lyrics_stretched);
    Some(song)
}

/// Todas as letras que dá para oferecer em "Procurar outra letra", a melhor
/// antes: as sincronizadas de duração mais perto, depois as só de texto,
/// sem repetir a mesma letra. Os critérios são mais largos que os de
/// `fetch` (outra versão, ±10 s), porque quem pede outra já viu a primeira.
pub fn alternatives(client: &Client, http: &reqwest::blocking::Client, track: &Track, lyrics_id: Option<&str>) -> Result<Vec<Song>, String> {
    let q = Query::new(track);
    // (diferença de duração, letra)
    let mut found: Vec<(f64, Song)> = Vec::new();
    let mut answered = false;
    let mut last_error = None;
    let gap = |got: Option<f64>| match (q.duration, got) {
        (Some(want), Some(got)) => (want - got).abs(),
        _ => 5.0,
    };

    // O LRCLIB: título e artista, título sozinho, e "título artista" livre.
    let mut searches = vec![vec![("q", q.title.clone())]];
    if !q.artist().is_empty() {
        searches.insert(0, vec![("track_name", q.title.clone()), ("artist_name", q.artist().to_owned())]);
        searches.push(vec![("q", format!("{} {}", q.title, q.artist()))]);
    }
    for params in &searches {
        match get(http, "search", params) {
            Ok(Some(Value::Array(hits))) => {
                answered = true;
                for v in &hits {
                    let duration = v["duration"].as_f64();
                    if !q.same_title(v["trackName"].as_str().unwrap_or_default()) || gap(duration) > 10.0 {
                        continue;
                    }
                    if let Some(mut song) = song_from(v, duration) {
                        song.source = with_length(&song.source, duration);
                        found.push((gap(duration), song));
                    }
                }
            }
            Ok(_) => answered = true,
            Err(e) => last_error = Some(e),
        }
    }

    // O NetEase: as quatro primeiras de mesmo título.
    for query in [format!("{} {}", q.title, q.artist()), q.title.clone()] {
        match netease_search(http, query.trim()) {
            Ok(songs) => {
                answered = true;
                for (_, _, duration, id) in songs.into_iter().filter(|s| q.same_title(&s.0)).take(4) {
                    if gap(Some(duration)) > 10.0 {
                        continue;
                    }
                    if let Ok(Some(mut song)) = netease_lyric(http, id, Some(duration)) {
                        song.source = with_length(&song.source, Some(duration));
                        found.push((gap(Some(duration)), song));
                    }
                }
            }
            Err(e) => last_error = Some(e),
        }
    }

    if q.altered
        && let Some(song) = stretched(http, &q)
    {
        found.push((0.5, song));
    }
    if let Some(id) = lyrics_id
        && let Ok(Some(l)) = client.lyrics(id)
        && !l.text.trim().is_empty()
    {
        found.push((99.0, Song { lines: Vec::new(), plain: l.text, source: l.source, ..Song::default() }));
    }
    if found.is_empty() && !answered {
        return Err(last_error.unwrap_or_default());
    }

    // As sincronizadas antes; entre elas, a de duração mais perto.
    found.sort_by(|a, b| a.1.lines.is_empty().cmp(&b.1.lines.is_empty()).then(a.0.total_cmp(&b.0)));
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (_, song) in found {
        let key = fingerprint(&song);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(song);
    }
    Ok(out)
}

/// O rodapé com a duração da versão de onde a letra veio: "Letra: LRCLIB ·
/// 3:41", para quem troca saber qual está vendo.
fn with_length(source: &str, duration: Option<f64>) -> String {
    match duration {
        Some(d) if d > 0.0 => format!("{source} · {}:{:02}", (d / 60.0) as u32, (d % 60.0) as u32),
        _ => source.to_owned(),
    }
}

/// O que identifica uma letra para não oferecer a mesma duas vezes: o
/// começo do texto, sem pontuação, e se tem tempo (a mesma letra com e sem
/// tempo são duas escolhas).
pub fn fingerprint(song: &Song) -> String {
    let text: String = if song.lines.is_empty() {
        song.plain.clone()
    } else {
        song.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ")
    };
    let mut key: String = normalize(&text).chars().take(80).collect();
    if !key.is_empty() && !song.lines.is_empty() {
        key.push('#');
    }
    key
}

/// O cache das letras em disco: uma por música, para abrir na hora e não
/// depender do LRCLIB estar de pé. "Sem letra" também fica, mas por menos
/// tempo (uma fonte pode ganhar a letra depois).
#[derive(Serialize, Deserialize)]
struct Cached {
    saved: u64,
    song: Option<Song>,
}

const KEEP_SONG: u64 = 30 * 24 * 3600;
const KEEP_NONE: u64 = 3 * 24 * 3600;
/// As letras guardadas no máximo; as mais velhas saem. Uma letra com
/// tempo e tradução fica em ~20 KB: o teto dá ~12 MB.
const CACHE_FILES: usize = 600;

fn cache_path(dir: &std::path::Path, video_id: &str) -> std::path::PathBuf {
    let name: String = video_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    dir.join(format!("{name}.json"))
}

/// A letra guardada da música: `Some(None)` é "não tem", `None` é "não sei"
/// (nunca buscada, ou o guardado venceu).
pub fn cached(dir: &std::path::Path, video_id: &str) -> Option<Option<Song>> {
    let bytes = std::fs::read(cache_path(dir, video_id)).ok()?;
    let entry: Cached = serde_json::from_slice(&bytes).ok()?;
    let keep = if entry.song.is_some() { KEEP_SONG } else { KEEP_NONE };
    (crate::state::unix_now().saturating_sub(entry.saved) < keep).then_some(entry.song)
}

/// Guarda a letra (ou o "não tem") da música.
pub fn store(dir: &std::path::Path, video_id: &str, song: Option<&Song>) {
    let entry = Cached { saved: crate::state::unix_now(), song: song.cloned() };
    let Ok(json) = serde_json::to_vec(&entry) else { return };
    if std::fs::create_dir_all(dir).is_ok()
        && let Err(error) = std::fs::write(cache_path(dir, video_id), json)
    {
        log::debug!("letra não guardada: {error}");
    }
}

/// Tira do cache as letras vencidas (que `cached` já não usaria) e deixa só
/// as `CACHE_FILES` mais novas. Roda numa thread.
pub fn clean_cache(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let expired = std::time::SystemTime::now() - std::time::Duration::from_secs(KEEP_SONG);
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for (modified, path) in entries.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))) {
        if modified < expired {
            let _ = std::fs::remove_file(path);
        } else {
            files.push((modified, path));
        }
    }
    if files.len() <= CACHE_FILES {
        return;
    }
    files.sort_unstable_by_key(|f| std::cmp::Reverse(f.0));
    for (_, path) in files.into_iter().skip(CACHE_FILES) {
        let _ = std::fs::remove_file(path);
    }
}

/// O tradutor do Google (o mesmo endpoint aberto que a extensão Better
/// Lyrics usa): detecta o idioma sozinho.
const TRANSLATE: &str = "https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&dt=t";
const MYMEMORY: &str = "https://api.mymemory.translated.net/get";

/// A letra traduzida para `target` ("pt"): pelo `service` escolhido e, se
/// ele falhar (fora do ar, bloqueado, sem cota, nada instalado), pelos
/// outros. A marca `translated_to` fica a do escolhido, para não pedir de
/// novo a cada quadro; `translated_by` diz qual traduziu. `local_model` é o
/// modelo do Ollama na tradução local (`None`, o melhor instalado).
pub fn translate(http: &reqwest::blocking::Client, song: &Song, target: &str, service: crate::look::TranslateService, local_model: Option<&str>) -> Result<Song, String> {
    use crate::look::TranslateService;
    let order = std::iter::once(service).chain(TranslateService::ALL.into_iter().filter(|s| *s != service));
    let mut errors: Vec<String> = Vec::new();
    for (i, each) in order.enumerate() {
        match translate_with(http, song, target, each, local_model) {
            Ok(mut out) => {
                out.translated_to = Some(crate::look::translation_key(target, service, local_model));
                return Ok(out);
            }
            Err(error) => {
                if i == 0 {
                    log::info!("tradução por {}: {error}; tentando os outros", each.key());
                }
                errors.push(error);
            }
        }
    }
    Err(errors.join("; "))
}

/// A letra com cada linha traduzida para `target` pelo `service`, e o idioma
/// dela. Na letra que já está em `target`, só marca o idioma (nada a
/// mostrar). Cada texto diferente é traduzido uma vez; a letra inteira vai
/// em poucos pedidos, uma linha por linha.
fn translate_with(http: &reqwest::blocking::Client, song: &Song, target: &str, service: crate::look::TranslateService, local_model: Option<&str>) -> Result<Song, String> {
    use crate::look::TranslateService;
    let mut texts: Vec<&str> = Vec::new();
    for line in song.lines.iter().filter(|l| !l.is_gap()) {
        if !texts.contains(&line.text.as_str()) {
            texts.push(&line.text);
        }
    }
    let mut out = song.clone();
    out.translated_to = Some(format!("{target}:{}", service.key()));
    out.translated_by = Some(service.key().to_owned());
    if texts.is_empty() {
        return Ok(out);
    }
    // O tradutor local: decidido uma vez (Ollama ou LibreTranslate).
    let local = match service {
        TranslateService::Local => Some(crate::local_tr::Backend::detect(http, local_model)?),
        _ => None,
    };
    // Caracteres por pedido, com folga do limite de cada serviço.
    let chunk_size = match (service, &local) {
        (_, Some(backend)) => backend.chunk_size(),
        (TranslateService::MyMemory, _) => 450,
        _ => 3500,
    };
    // Em blocos de linhas inteiras.
    let mut chunks: Vec<Vec<&str>> = vec![Vec::new()];
    let mut size = 0;
    for t in &texts {
        if size + t.len() > chunk_size && !chunks.last().is_some_and(Vec::is_empty) {
            chunks.push(Vec::new());
            size = 0;
        }
        size += t.len() + 1;
        if let Some(last) = chunks.last_mut() {
            last.push(t);
        }
    }
    let mut translated: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for chunk in chunks {
        let (lang, lines) = match (&local, service) {
            (Some(backend), _) => backend.translate(http, &chunk, target)?,
            (None, TranslateService::MyMemory) => mymemory_chunk(http, &chunk.join("\n"), target)?,
            (None, _) => translate_chunk(http, &chunk.join("\n"), target)?,
        };
        if out.lang.is_none() && !lang.is_empty() {
            out.lang = Some(lang.clone());
        }
        // Já no idioma do app: nada a traduzir.
        if base_lang(&lang) == base_lang(target) {
            return Ok(out);
        }
        // O tradutor guarda as quebras de linha; se não guardou, a linha a
        // linha não bate e é melhor não mostrar.
        if lines.len() != chunk.len() {
            return Err(format!("tradução com {} linhas para {}", lines.len(), chunk.len()));
        }
        for (original, line) in chunk.iter().zip(lines) {
            translated.insert(original, line);
        }
    }
    for line in out.lines.iter_mut().filter(|l| !l.is_gap()) {
        line.translation = translated.get(line.text.as_str()).filter(|t| normalize(t) != normalize(&line.text)).cloned();
    }
    Ok(out)
}

/// Um pedido ao MyMemory, com detecção do idioma: o idioma e as linhas.
fn mymemory_chunk(http: &reqwest::blocking::Client, text: &str, target: &str) -> Result<(String, Vec<String>), String> {
    // "pt" lá é o de Portugal; o app é do Brasil.
    let target = if target == "pt" { "pt-BR" } else { target };
    let url = reqwest::Url::parse_with_params(MYMEMORY, &[("q", text), ("langpair", &format!("autodetect|{target}"))]).map_err(|e| e.to_string())?;
    let v: Value = http
        .get(url)
        .header("User-Agent", AGENT)
        .timeout(Duration::from_secs(12))
        .send()
        .and_then(|r| r.json())
        .map_err(|e| format!("MyMemory: {e}"))?;
    let status = v["responseStatus"].as_u64().or_else(|| v["responseStatus"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
    let translated = v["responseData"]["translatedText"].as_str().unwrap_or_default();
    // Sem cota, ele responde com o aviso no lugar da tradução.
    if status != 200 || translated.starts_with("MYMEMORY WARNING") {
        return Err(format!("MyMemory: {}", v["responseDetails"].as_str().unwrap_or("limite diário atingido")));
    }
    let lang = v["responseData"]["detectedLanguage"].as_str().unwrap_or_default().to_owned();
    Ok((lang, translated.split('\n').map(|l| l.trim().to_owned()).collect()))
}

/// Um pedido ao tradutor: o idioma detectado e as linhas traduzidas.
fn translate_chunk(http: &reqwest::blocking::Client, text: &str, target: &str) -> Result<(String, Vec<String>), String> {
    // O texto vai no corpo, como formulário (na URL, uma letra longa passaria
    // do limite).
    let form = reqwest::Url::parse_with_params("http://x/", &[("q", text)]).map_err(|e| e.to_string())?;
    let body = form.query().unwrap_or_default().to_owned();
    let v: Value = http
        .post(format!("{TRANSLATE}&tl={target}"))
        .header("Content-Type", "application/x-www-form-urlencoded;charset=utf-8")
        .header("User-Agent", AGENT)
        .timeout(Duration::from_secs(10))
        .body(body)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| format!("tradutor: {e}"))?;
    let joined: String = v[0].as_array().map(|segs| segs.iter().filter_map(|seg| seg[0].as_str()).collect()).unwrap_or_default();
    let lang = v[2].as_str().unwrap_or_default().to_owned();
    Ok((lang, joined.split('\n').map(|l| l.trim().to_owned()).collect()))
}

/// Um GET no LRCLIB. 404 é "não tem" (`None`); "servidor ocupado" (503,
/// comum lá) tenta de novo, esperando cada vez mais.
fn get(http: &reqwest::blocking::Client, path: &str, params: &[(&str, String)]) -> Result<Option<Value>, String> {
    let url = reqwest::Url::parse_with_params(&format!("{LRCLIB}/{path}"), params).map_err(|e| e.to_string())?;
    let mut last = String::new();
    for attempt in 0..4u64 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(900 * attempt));
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
            code @ (429 | 500..=599) => last = musicfast_i18n::t().lrclib_busy(code),
            code => return Err(format!("LRCLIB: HTTP {code}")),
        }
    }
    Err(last)
}

fn song_from(v: &Value, duration: Option<f64>) -> Option<Song> {
    if v["instrumental"].as_bool() == Some(true) {
        let t = musicfast_i18n::t();
        return Some(Song { lines: Vec::new(), plain: t.instrumental.to_owned(), source: t.lyrics_by_lrclib.to_owned(), ..Song::default() });
    }
    let lines = v["syncedLyrics"].as_str().map(|s| parse_lrc(s, duration.or(v["duration"].as_f64()))).unwrap_or_default();
    let plain = v["plainLyrics"].as_str().unwrap_or_default().to_owned();
    if lines.is_empty() && plain.trim().is_empty() {
        return None;
    }
    Some(Song { lines, plain, source: musicfast_i18n::t().lyrics_by_lrclib.to_owned(), ..Song::default() })
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
        lines.push(Line { start, end, text, words, ..Line::default() });
    }
    // Um começo instrumental longo também ganha os pontinhos.
    if let Some(first) = lines.first()
        && first.start > 3.0
        && !first.is_gap()
    {
        let end = first.start;
        lines.insert(0, Line { start: 0.0, end, ..Line::default() });
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

    /// Busca músicas de verdade e mostra o que a letra acha. Rede: rode com
    /// `cargo test -p musicfast diagnostico_letras -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn diagnostico_letras() {
        let client = Client::new("pt", "BR").unwrap();
        let http = reqwest::blocking::Client::new();
        let queries = std::env::var("LETRAS").unwrap_or_default();
        for q in queries.split(';').filter(|q| !q.trim().is_empty()) {
            let page = client.search(q, musicfast_ytm::SearchFilter::Songs).unwrap();
            let Some(found) = page.tracks().next().cloned() else {
                println!("{q:<45} | sem resultado no YouTube Music");
                continue;
            };
            let watch = client.watch(Some(&found.video_id), None, None).unwrap();
            let track = watch.tracks.first().cloned().unwrap_or(found);
            let result = fetch(&client, &http, &track, watch.lyrics_id.as_deref());
            let what = match &result {
                Ok(Some(s)) if !s.lines.is_empty() => format!(
                    "SINCRONIZADA ({} linhas) {}{}{}{}",
                    s.lines.len(),
                    s.source,
                    if s.words_timed { " [palavra por palavra]" } else { "" },
                    if s.lines.iter().any(|l| l.romaji.is_some()) { " [romaji]" } else { "" },
                    if s.doubtful { " [outra letra?]" } else { "" }
                ),
                Ok(Some(s)) => format!("só texto: {}", s.source),
                Ok(None) => "NADA".to_owned(),
                Err(e) => format!("erro: {e}"),
            };
            println!("{q:<45} | {} — {} [{:?}s] | {what}", track.title, track.artist_names(), track.duration_secs);
        }
    }

    /// As candidatas do "Outra letra". Rede: `LETRAS=... --ignored`.
    #[test]
    #[ignore]
    fn diagnostico_outras_letras() {
        let client = Client::new("pt", "BR").unwrap();
        let http = reqwest::blocking::Client::new();
        let queries = std::env::var("LETRAS").unwrap_or_default();
        for q in queries.split(';').filter(|q| !q.trim().is_empty()) {
            let page = client.search(q, musicfast_ytm::SearchFilter::Songs).unwrap();
            let Some(track) = page.tracks().next().cloned() else { continue };
            let alts = alternatives(&client, &http, &track, None).unwrap();
            println!("{} — {}: {} candidatas", track.title, track.artist_names(), alts.len());
            for a in &alts {
                let kind = if a.lines.is_empty() { "texto".to_owned() } else { format!("{} linhas", a.lines.len()) };
                println!("    {kind:<10} {}", a.source);
            }
        }
    }

    /// O ajuste de velocidade: uma "slowed" sem letra própria usa a da
    /// original esticada. Rede: rode com `--ignored`.
    #[test]
    #[ignore]
    fn letra_esticada_da_original() {
        let http = reqwest::blocking::Client::new();
        let artist = musicfast_ytm::Link { name: "Dominic Fike".into(), id: None };
        let track = Track { title: "Babydoll (Slowed)".into(), artists: vec![artist], duration_secs: Some(120), ..Track::default() };
        let song = stretched(&http, &Query::new(&track)).expect("a original tem letra sincronizada");
        let last = song.lines.iter().rev().find(|l| !l.is_gap()).unwrap();
        println!("{} linhas, última em {:.1}s, fonte: {}", song.lines.len(), last.start, song.source);
        // A original tem 98 s: a última linha cantada passa do fim dela.
        assert!(last.start > 80.0 && song.source.contains('·'));
    }

    #[test]
    fn idioma_estrangeiro() {
        let song = |lang: Option<&str>| Song { lang: lang.map(str::to_owned), ..Song::default() };
        assert!(song(Some("en")).foreign_to("pt"));
        assert!(!song(Some("pt")).foreign_to("pt-BR"));
        assert!(!song(None).foreign_to("pt"));
    }

    /// A tradução de verdade. Rede: rode com `--ignored`.
    #[test]
    #[ignore]
    fn traduz_linha_a_linha() {
        let http = reqwest::blocking::Client::new();
        let song = Song { lines: parse_lrc("[00:01.00]I've been tryna call\n[00:03.00]♪\n[00:05.00]I've been tryna call\n[00:07.00]Maybe you can show me how to love", None), ..Song::default() };
        let out = translate(&http, &song, "pt", crate::look::TranslateService::Google, None).unwrap();
        for l in &out.lines {
            println!("{:<40} → {:?}", l.text, l.translation);
        }
        assert_eq!(out.lang.as_deref(), Some("en"));
        assert!(out.lines.iter().filter(|l| !l.is_gap()).all(|l| l.translation.is_some()));
        assert_eq!(out.translated_to.as_deref(), Some("pt:google"));
        assert!(out.translated_by.is_some());
    }

    /// A tradução pelo MyMemory. Rede: rode com `--ignored`.
    #[test]
    #[ignore]
    fn traduz_pelo_mymemory() {
        let http = reqwest::blocking::Client::new();
        let song = Song { lines: parse_lrc("[00:01.00]正午の陽射しで生まれたの
[00:03.00]名前を入力して", None), ..Song::default() };
        let out = translate(&http, &song, "pt", crate::look::TranslateService::MyMemory, None).unwrap();
        for l in &out.lines {
            println!("{:<30} → {:?}", l.text, l.translation);
        }
        assert_eq!(out.lang.as_deref(), Some("ja"));
        assert_eq!(out.translated_to.as_deref(), Some("pt:mymemory"));
        assert!(out.lines.iter().all(|l| l.translation.is_some()));
    }

    #[test]
    fn le_o_karaoke_do_netease() {
        let yrc = "{\"t\":0,\"c\":[{\"tx\":\"作词: \"}]}
[9030,3510](9030,630,0)We (9660,780,0)hug (10440,120,0)and
[20000,1000](20000,500,0)Oi (20500,500,0)la";
        let lines = parse_yrc(yrc);
        // A primeira linha começa em 9 s: ganha os pontinhos antes; o
        // silêncio de 7,5 s até a outra também.
        assert!(lines[0].is_gap());
        assert_eq!(lines[1].text, "We hug and");
        assert_eq!(lines[1].words.len(), 3);
        assert!((lines[1].words[1].start - 9.66).abs() < 1e-9 && (lines[1].words[1].end - 10.44).abs() < 1e-9);
        assert_eq!(lines[1].words[1].from, 3);
        assert!(lines[2].is_gap());
        assert_eq!(lines[3].text, "Oi la");
    }

    #[test]
    fn letras_parecidas_e_diferentes() {
        let song = |s: &str| Song { lines: parse_lrc(s, None), ..Song::default() };
        let a = song("[00:01.00]I've been tryna call
[00:03.00]I've been on my own for long enough");
        let b = song("[00:01.20]I have been trying to call
[00:03.10]I've been on my own for long enough");
        let c = song("[00:01.00]Completely different words here
[00:03.00]nothing in common at all");
        assert!(similar(&a, &b));
        assert!(!similar(&a, &c));
    }

    #[test]
    fn letra_do_youtube_com_tempo() {
        let line = |start: f64, end: f64, text: &str| musicfast_ytm::TimedLine { start, end, text: text.into() };
        let l = musicfast_ytm::Lyrics {
            text: "Yeah
I've been tryna call".into(),
            source: "Fonte: Musixmatch".into(),
            lines: vec![line(13.1, 16.5, "Yeah"), line(27.1, 30.0, "I've been tryna call")],
        };
        let song = from_ytm(l, Some(40.0)).unwrap();
        let texts: Vec<&str> = song.lines.iter().map(|l| l.text.as_str()).collect();
        // Os pontinhos no começo e no buraco de 10 s entre as duas.
        assert_eq!(texts, ["", "Yeah", "", "I've been tryna call"]);
        assert!((song.lines[1].start - 13.1).abs() < 0.01);
    }

    #[test]
    fn reconhece_versao_instrumental() {
        assert!(is_instrumental_title("One Of The Girls (Instrumental)"));
        assert!(is_instrumental_title("Idol (Off Vocal)"));
        assert!(is_instrumental_title("Song - Karaoke Version"));
        assert!(!is_instrumental_title("Instant Crush"));
        assert!(!is_instrumental_title("Feel Good Inc."));
    }

    #[test]
    fn creditos_do_netease_saem() {
        assert!(is_credit("[00:00.00] 作词 : 笹川真生"));
        assert!(is_credit("[00:01.00]作曲：someone"));
        assert!(is_credit("[00:02.00] Composed by: X"));
        assert!(!is_credit("[00:18.61]正午の陽射しで生まれたの"));
    }

    #[test]
    fn titulo_e_artista_batem_sem_pontuacao() {
        let track = Track { title: "Feel Good Inc. (Official Video)".into(), artists: vec![musicfast_ytm::Link { name: "Gorillaz".into(), id: None }], duration_secs: Some(223), ..Track::default() };
        let q = Query::new(&track);
        assert!(q.same_title("Feel Good Inc"));
        assert!(!q.same_title("Feel"));
        assert!(q.same_artist("Gorillaz & De La Soul"));
        assert!(q.close(Some(225.0), 4.0) && !q.close(Some(230.0), 4.0));
        let slowed = Track { title: "Otoshi (Slowed)".into(), ..Track::default() };
        assert!(Query::new(&slowed).altered);
    }

    #[test]
    fn limpa_titulo() {
        assert_eq!(clean_title("Song (Official Video) [4K]"), "Song");
        assert_eq!(clean_title("(Wed)"), "(Wed)");
    }
}
