//! Os bytes de uma música e o decodificador sobre eles.
//!
//! **A música inteira vai para a memória antes de tocar.** Medido em
//! 28/10/2026: 3,4 MB (3:34 em AAC 128 kbps) descem em 0,18 s com a conexão
//! aberta, e o Symphonia decodifica a música toda em ~140 ms. Contra os ~3 s
//! da resolução, esperar o download é invisível, e em troca o decodificador
//! lê de um `Cursor` que nunca bloqueia: pode rodar no callback de áudio, a
//! busca de posição é instantânea e não há fila de PCM para esvaziar.
//! Custo: ~1 MB por minuto de música em RAM, para a atual e a próxima.

use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

use crate::ytdlp::Resolved;

/// Abaixo de 10 MB o YouTube entrega cada pedaço sem limitar a velocidade
/// (o mesmo que o yt-dlp faz com `http_chunk_size`).
const CHUNK: u64 = 8 * 1024 * 1024;
/// Um episódio de podcast de 4 h em 128 kbps tem ~230 MB.
const MAX_BYTES: usize = 300 * 1024 * 1024;

pub fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        // O endereço é do IP que o yt-dlp usou (`--force-ipv4`).
        .local_address(Some(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)))
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(90))
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .expect("cliente HTTP")
}

pub enum FetchError {
    /// 403/410: o endereço venceu ou é de outro IP. Resolver de novo.
    Expired,
    Other(String),
}

pub fn fetch(http: &reqwest::blocking::Client, r: &Resolved) -> Result<Arc<[u8]>, FetchError> {
    let started = std::time::Instant::now();
    // O endereço do cliente de TV leva 403 até o `available_at` (o yt-dlp
    // dorme o mesmo tempo antes de baixar). Teto de 10 s: além disso, a
    // espera é o defeito, e o 403 manda resolver de novo.
    if let Some(at) = r.available_at {
        let now = crate::ytdlp::now();
        if at > now {
            std::thread::sleep(Duration::from_secs((at - now).min(10)));
        }
    }
    let mut data: Vec<u8> = Vec::with_capacity(r.size.unwrap_or(4 << 20).min(MAX_BYTES as u64) as usize);
    loop {
        let start = data.len() as u64;
        let end = match r.size {
            Some(total) if start >= total => break,
            Some(total) => (start + CHUNK - 1).min(total - 1),
            None => start + CHUNK - 1,
        };
        let mut req = http.get(format!("{}&range={start}-{end}", r.url));
        if let Some(ua) = &r.user_agent {
            req = req.header("User-Agent", ua);
        }
        let t = musicfast_i18n::t();
        let resp = req.send().map_err(|e| FetchError::Other(t.download_failed(&e.to_string())))?;
        match resp.status().as_u16() {
            200 | 206 => {}
            403 | 410 => return Err(FetchError::Expired),
            s => return Err(FetchError::Other(t.audio_server_answered(s))),
        }
        let before = data.len();
        resp.take(CHUNK + 1)
            .read_to_end(&mut data)
            .map_err(|e| FetchError::Other(t.download_interrupted(&e.to_string())))?;
        let got = (data.len() - before) as u64;
        if data.len() > MAX_BYTES {
            return Err(FetchError::Other(t.audio_too_long.into()));
        }
        if got == 0 || (r.size.is_none() && got < CHUNK) {
            break;
        }
    }
    if data.is_empty() {
        return Err(FetchError::Other(musicfast_i18n::t().audio_empty.into()));
    }
    log::info!("{}: {} kB em {} ms", r.video_id, data.len() / 1024, started.elapsed().as_millis());
    Ok(data.into())
}

pub struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    pub rate: u32,
    pub duration: Option<f64>,
    /// Quadros a descartar depois de uma busca exata.
    skip: u64,
    packet: Vec<f32>,
    channels: usize,
}

impl Decoder {
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, String> {
        let t = musicfast_i18n::t();
        let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes)), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        hint.with_extension("m4a");
        let format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| t.audio_format_unknown(&e.to_string()))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or(t.audio_no_track)?;
        let track_id = track.id;
        let time_base = track.time_base;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or(t.audio_no_params)?
            .clone();
        let rate = params.sample_rate.ok_or(t.audio_no_rate)?;
        let duration = match (track.num_frames, time_base) {
            (Some(frames), Some(tb)) => tb
                .calc_time(symphonia::core::units::Timestamp::new(frames as i64))
                .map(|t| t.as_secs_f64()),
            (Some(frames), None) => Some(frames as f64 / rate as f64),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| t.audio_no_decoder(&e.to_string()))?;
        Ok(Self {
            format,
            decoder,
            track_id,
            time_base,
            rate,
            duration,
            skip: 0,
            packet: Vec::with_capacity(8192),
            channels: 2,
        })
    }

    /// Decodifica o próximo pacote e acrescenta a `out` em estéreo
    /// intercalado. `Ok(false)` no fim da música.
    pub fn decode_into(&mut self, out: &mut Vec<f32>) -> Result<bool, &'static str> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymphoniaError::IoError(_)) => return Ok(false),
                Err(SymphoniaError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(_) => return Err(musicfast_i18n::t().audio_corrupted),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                // Um pacote ruim vira um silêncio de 23 ms, não o fim da música.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(_) => return Err(musicfast_i18n::t().audio_decoder_failed),
            };
            self.channels = decoded.spec().channels().count().max(1);
            decoded.copy_to_vec_interleaved(&mut self.packet);
            let ch = self.channels;
            let frames = self.packet.len() / ch;
            let first = (self.skip as usize).min(frames);
            self.skip -= first as u64;
            out.reserve((frames - first) * 2);
            for f in first..frames {
                let l = self.packet[f * ch];
                let r = if ch > 1 { self.packet[f * ch + 1] } else { l };
                out.push(l);
                out.push(r);
            }
            return Ok(true);
        }
    }

    /// Vai para `secs`; devolve onde ficou.
    pub fn seek(&mut self, secs: f64) -> f64 {
        let Some(time) = Time::try_from_secs_f64(secs.max(0.0)) else { return 0.0 };
        match self.format.seek(SeekMode::Accurate, SeekTo::Time { time, track_id: Some(self.track_id) }) {
            Ok(seeked) => {
                self.decoder.reset();
                let delta = seeked.required_ts.get() - seeked.actual_ts.get();
                // O contêiner volta ao começo do pacote; o resto até o ponto
                // pedido é descartado na decodificação.
                self.skip = match self.time_base {
                    Some(tb) => tb
                        .calc_time(symphonia::core::units::Timestamp::new(delta.max(0)))
                        .map(|t| (t.as_secs_f64() * self.rate as f64) as u64)
                        .unwrap_or(0),
                    None => delta.max(0) as u64,
                };
                secs.max(0.0)
            }
            Err(_) => 0.0,
        }
    }
}

/// Interpolação linear entre taxas, só para quando a placa recusa 44,1 kHz
/// (o WASAPI aceita com `AUTOCONVERTPCM`, que o cpal liga; aqui é reserva).
#[derive(Clone, Debug)]
pub struct Resampler {
    step: f64,
    t: f64,
    prev: [f32; 2],
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, t: 1.0, prev: [0.0; 2] }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < 1e-9
    }

    /// `input` em estéreo intercalado na taxa de origem; `out` recebe na de
    /// destino. A posição continua entre chamadas.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len() / 2;
        if n == 0 {
            return;
        }
        // O fluxo virtual é `prev, input[0], input[1]...`: o índice 0 é o
        // último quadro da chamada anterior.
        let at = |i: usize, c: usize, prev: &[f32; 2]| if i == 0 { prev[c] } else { input[(i - 1) * 2 + c] };
        while self.t < n as f64 {
            let i = self.t.floor() as usize;
            let frac = (self.t - i as f64) as f32;
            for c in 0..2 {
                let a = at(i, c, &self.prev);
                let b = at(i + 1, c, &self.prev);
                out.push(a + (b - a) * frac);
            }
            self.t += self.step;
        }
        self.t -= n as f64;
        self.prev = [input[(n - 1) * 2], input[(n - 1) * 2 + 1]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_keeps_the_length_ratio_and_a_constant_signal() {
        let mut r = Resampler::new(44_100, 48_000);
        let input = vec![0.5f32; 44_100 * 2];
        let mut out = Vec::new();
        for chunk in input.chunks(2048) {
            r.process(chunk, &mut out);
        }
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 2, "{frames}");
        // Depois do primeiro quadro (que interpola do silêncio inicial).
        assert!(out[4..].iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn same_rate_is_passthrough() {
        assert!(Resampler::new(44_100, 44_100).is_passthrough());
    }
}
