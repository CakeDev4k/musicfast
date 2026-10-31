//! O arquivo `.m4a` das músicas baixadas para ouvir sem internet.
//!
//! O YouTube entrega o AAC num MP4 fragmentado de DASH (marca `dash`, um
//! `moof` por pedaço). O Symphonia toca, mas o Premiere, o WhatsApp e boa
//! parte dos programas de edição só aceitam o MP4 comum. Sem ffmpeg, o
//! jeito é reembalar: os pacotes de AAC saem do Symphonia exatamente como
//! vieram (sem decodificar nem perder qualidade) e entram num MP4 normal,
//! com o `moov` na frente, o título, o artista, o álbum e a capa (`ilst`, as
//! etiquetas que o iTunes, o Explorer e o Premiere leem).

use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// O que vai nas etiquetas do arquivo.
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// A capa em JPEG ou PNG (outro formato fica de fora: o `covr` não tem
    /// como dizer que é WebP).
    pub cover: Option<Vec<u8>>,
}

/// Os pacotes de AAC de um arquivo, e o que o MP4 precisa saber deles.
struct Stream {
    rate: u32,
    channels: u16,
    /// O `AudioSpecificConfig` do AAC (vai inteiro no `esds`).
    config: Vec<u8>,
    /// Tamanho e duração (em amostras) de cada pacote.
    samples: Vec<(u32, u32)>,
    data: Vec<u8>,
}

/// Lê o MP4 do YouTube (fragmentado ou não) e devolve um `.m4a` comum com as
/// etiquetas.
pub fn remux(input: Vec<u8>, tags: &Tags) -> Result<Vec<u8>, String> {
    let stream = demux(input)?;
    Ok(write(&stream, tags))
}

fn demux(input: Vec<u8>) -> Result<Stream, String> {
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(input)), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension("m4a");
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| format!("formato não reconhecido: {e}"))?;
    let track = format.default_track(TrackType::Audio).ok_or("sem faixa de áudio")?;
    let track_id = track.id;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or("faixa sem parâmetros de áudio")?;
    if params.codec != symphonia::core::codecs::audio::well_known::CODEC_ID_AAC {
        return Err("o áudio não é AAC".into());
    }
    let rate = params.sample_rate.ok_or("taxa de amostragem desconhecida")?;
    let channels = params.channels.as_ref().map(|c| c.count() as u16).unwrap_or(2).max(1);
    let config = params.extra_data.as_deref().map(<[u8]>::to_vec).ok_or("sem a configuração do AAC")?;
    // A duração dos pacotes vem na base de tempo da faixa; o MP4 novo conta
    // em amostras (a base é a própria taxa).
    let (numer, denom) = track.time_base.map(|tb| (tb.numer.get() as u64, tb.denom.get() as u64)).unwrap_or((1, rate as u64));
    let mut samples = Vec::new();
    let mut data = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(format!("arquivo corrompido: {e}")),
        };
        if packet.track_id != track_id || packet.data.is_empty() {
            continue;
        }
        // O pacote inteiro: o que o Symphonia corta do começo e do fim
        // (atraso do codificador) ainda está dentro dele.
        let units = packet.dur.get() + packet.trim_start.get() + packet.trim_end.get();
        let frames = (units * numer * rate as u64 + denom / 2) / denom;
        let frames = if frames == 0 { 1024 } else { frames.min(u32::MAX as u64) as u32 };
        samples.push((packet.data.len() as u32, frames));
        data.extend_from_slice(&packet.data);
    }
    if samples.is_empty() {
        return Err("arquivo sem áudio".into());
    }
    Ok(Stream { rate, channels, config, samples, data })
}

/// Amostras por pedaço do `mdat` (~1 s de AAC a 44,1 kHz).
const PER_CHUNK: usize = 43;

fn write(s: &Stream, tags: &Tags) -> Vec<u8> {
    let ftyp = atom(b"ftyp", &cat(&[b"M4A ", &0u32.to_be_bytes(), b"M4A ", b"mp42", b"isom"]));
    // O `moov` tem o mesmo tamanho qualquer que seja o deslocamento do áudio:
    // monta uma vez para medir e outra com o endereço certo.
    let probe = moov(s, tags, 0);
    let base = (ftyp.len() + probe.len() + 8) as u64;
    let moov = moov(s, tags, base);
    let mut out = Vec::with_capacity(base as usize + s.data.len());
    out.extend_from_slice(&ftyp);
    out.extend_from_slice(&moov);
    out.extend_from_slice(&((s.data.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(b"mdat");
    out.extend_from_slice(&s.data);
    out
}

fn atom(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// Um átomo "completo" (versão 0 e as bandeiras).
fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b[0] = 0;
    b.extend_from_slice(body);
    atom(kind, &b)
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

const MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

fn matrix() -> Vec<u8> {
    MATRIX.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn moov(s: &Stream, tags: &Tags, base: u64) -> Vec<u8> {
    let duration: u64 = s.samples.iter().map(|&(_, d)| d as u64).sum();
    let duration = duration.min(u32::MAX as u64) as u32;

    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&0u32.to_be_bytes()); // criação
    mvhd.extend_from_slice(&0u32.to_be_bytes()); // modificação
    mvhd.extend_from_slice(&s.rate.to_be_bytes());
    mvhd.extend_from_slice(&duration.to_be_bytes());
    mvhd.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // velocidade 1,0
    mvhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1,0
    mvhd.extend_from_slice(&[0; 10]);
    mvhd.extend_from_slice(&matrix());
    mvhd.extend_from_slice(&[0; 24]);
    mvhd.extend_from_slice(&2u32.to_be_bytes()); // próxima faixa
    let mvhd = full(b"mvhd", 0, &mvhd);

    let mut tkhd = Vec::new();
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&1u32.to_be_bytes()); // faixa 1
    tkhd.extend_from_slice(&0u32.to_be_bytes());
    tkhd.extend_from_slice(&duration.to_be_bytes());
    tkhd.extend_from_slice(&[0; 8]);
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // camada
    tkhd.extend_from_slice(&0u16.to_be_bytes()); // grupo
    tkhd.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
    tkhd.extend_from_slice(&0u16.to_be_bytes());
    tkhd.extend_from_slice(&matrix());
    tkhd.extend_from_slice(&0u32.to_be_bytes()); // largura
    tkhd.extend_from_slice(&0u32.to_be_bytes()); // altura
    let tkhd = full(b"tkhd", 0x3, &tkhd); // ativa e no filme

    let mut mdhd = Vec::new();
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&0u32.to_be_bytes());
    mdhd.extend_from_slice(&s.rate.to_be_bytes());
    mdhd.extend_from_slice(&duration.to_be_bytes());
    mdhd.extend_from_slice(&0x55C4u16.to_be_bytes()); // "und"
    mdhd.extend_from_slice(&0u16.to_be_bytes());
    let mdhd = full(b"mdhd", 0, &mdhd);

    let hdlr = full(b"hdlr", 0, &cat(&[&[0; 4], b"soun", &[0; 12], b"SoundHandler\0"]));
    let smhd = full(b"smhd", 0, &[0; 4]);
    let dref = full(b"dref", 0, &cat(&[&1u32.to_be_bytes(), &full(b"url ", 1, &[])]));
    let dinf = atom(b"dinf", &dref);
    let stbl = atom(b"stbl", &cat(&[&stsd(s), &stts(s), &stsc(s), &stsz(s), &stco(s, base)]));
    let minf = atom(b"minf", &cat(&[&smhd, &dinf, &stbl]));
    let mdia = atom(b"mdia", &cat(&[&mdhd, &hdlr, &minf]));
    let trak = atom(b"trak", &cat(&[&tkhd, &mdia]));
    atom(b"moov", &cat(&[&mvhd, &trak, &udta(tags)]))
}

/// Um descritor do MPEG-4 (o tamanho em um byte basta para o `esds`).
fn descriptor(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    // Tamanho em até 4 bytes de 7 bits.
    let len = body.len() as u32;
    let mut groups = vec![(len & 0x7F) as u8];
    let mut rest = len >> 7;
    while rest > 0 {
        groups.push((rest & 0x7F) as u8 | 0x80);
        rest >>= 7;
    }
    groups.reverse();
    out.extend_from_slice(&groups);
    out.extend_from_slice(body);
    out
}

fn stsd(s: &Stream) -> Vec<u8> {
    let total_secs = (s.samples.iter().map(|&(_, d)| d as u64).sum::<u64>() as f64 / s.rate as f64).max(0.001);
    let avg = (s.data.len() as f64 * 8.0 / total_secs) as u32;
    let biggest = s.samples.iter().map(|&(n, _)| n).max().unwrap_or(0);
    let mut dec = vec![0x40, 0x15]; // AAC (MPEG-4 Áudio), fluxo de áudio
    dec.extend_from_slice(&biggest.to_be_bytes()[1..]); // bufferSizeDB, 24 bits
    dec.extend_from_slice(&avg.max(1).to_be_bytes()); // máximo (o médio basta)
    dec.extend_from_slice(&avg.to_be_bytes());
    dec.extend_from_slice(&descriptor(0x05, &s.config));
    let mut es = vec![0, 1, 0]; // ES_ID 1, sem bandeiras
    es.extend_from_slice(&descriptor(0x04, &dec));
    es.extend_from_slice(&descriptor(0x06, &[0x02]));
    let esds = full(b"esds", 0, &descriptor(0x03, &es));

    let mut mp4a = Vec::new();
    mp4a.extend_from_slice(&[0; 6]);
    mp4a.extend_from_slice(&1u16.to_be_bytes()); // referência de dados
    mp4a.extend_from_slice(&[0; 8]);
    mp4a.extend_from_slice(&s.channels.to_be_bytes());
    mp4a.extend_from_slice(&16u16.to_be_bytes());
    mp4a.extend_from_slice(&[0; 4]);
    let rate = if s.rate > 0xFFFF { 0 } else { s.rate << 16 };
    mp4a.extend_from_slice(&rate.to_be_bytes());
    mp4a.extend_from_slice(&esds);
    full(b"stsd", 0, &cat(&[&1u32.to_be_bytes(), &atom(b"mp4a", &mp4a)]))
}

fn stts(s: &Stream) -> Vec<u8> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &(_, d) in &s.samples {
        match runs.last_mut() {
            Some((count, delta)) if *delta == d => *count += 1,
            _ => runs.push((1, d)),
        }
    }
    let mut b = (runs.len() as u32).to_be_bytes().to_vec();
    for (count, delta) in runs {
        b.extend_from_slice(&count.to_be_bytes());
        b.extend_from_slice(&delta.to_be_bytes());
    }
    full(b"stts", 0, &b)
}

fn stsc(s: &Stream) -> Vec<u8> {
    let chunks = s.samples.len().div_ceil(PER_CHUNK);
    let last = s.samples.len() - (chunks - 1) * PER_CHUNK;
    let mut entries = vec![(1u32, PER_CHUNK.min(s.samples.len()) as u32)];
    if chunks > 1 && last != PER_CHUNK {
        entries.push((chunks as u32, last as u32));
    }
    let mut b = (entries.len() as u32).to_be_bytes().to_vec();
    for (first, count) in entries {
        b.extend_from_slice(&first.to_be_bytes());
        b.extend_from_slice(&count.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
    }
    full(b"stsc", 0, &b)
}

fn stsz(s: &Stream) -> Vec<u8> {
    let mut b = 0u32.to_be_bytes().to_vec();
    b.extend_from_slice(&(s.samples.len() as u32).to_be_bytes());
    for &(n, _) in &s.samples {
        b.extend_from_slice(&n.to_be_bytes());
    }
    full(b"stsz", 0, &b)
}

fn stco(s: &Stream, base: u64) -> Vec<u8> {
    let mut offsets = Vec::new();
    let mut at = base;
    for (i, &(n, _)) in s.samples.iter().enumerate() {
        if i % PER_CHUNK == 0 {
            offsets.push(at);
        }
        at += n as u64;
    }
    let mut b = (offsets.len() as u32).to_be_bytes().to_vec();
    for o in offsets {
        b.extend_from_slice(&(o as u32).to_be_bytes());
    }
    full(b"stco", 0, &b)
}

/// As etiquetas no formato do iTunes (`moov/udta/meta/ilst`).
fn udta(tags: &Tags) -> Vec<u8> {
    let mut ilst = Vec::new();
    let text = |kind: &[u8; 4], value: &str| -> Vec<u8> {
        let data = atom(b"data", &cat(&[&1u32.to_be_bytes(), &[0; 4], value.as_bytes()]));
        atom(kind, &data)
    };
    for (kind, value) in [(b"\xA9nam", &tags.title), (b"\xA9ART", &tags.artist), (b"aART", &tags.artist), (b"\xA9alb", &tags.album)] {
        if !value.is_empty() {
            ilst.extend_from_slice(&text(kind, value));
        }
    }
    ilst.extend_from_slice(&text(b"\xA9too", "MusicFast"));
    if let Some(cover) = &tags.cover {
        let kind: u32 = if cover.starts_with(&[0x89, b'P', b'N', b'G']) { 14 } else { 13 };
        let data = atom(b"data", &cat(&[&kind.to_be_bytes(), &[0; 4], cover]));
        ilst.extend_from_slice(&atom(b"covr", &data));
    }
    let hdlr = full(b"hdlr", 0, &cat(&[&[0; 4], b"mdir", b"appl", &[0; 8], &[0]]));
    let meta = full(b"meta", 0, &cat(&[&hdlr, &atom(b"ilst", &ilst)]));
    atom(b"udta", &meta)
}

/// A capa serve para o `covr`? Só JPEG e PNG.
pub fn is_cover_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um fluxo de mentira (AAC-LC, 44,1 kHz, estéreo): o que importa aqui
    /// são as caixas e os endereços, não o som.
    fn stream(n: usize) -> Stream {
        let samples: Vec<(u32, u32)> = (0..n).map(|i| (100 + (i as u32 % 7), 1024)).collect();
        let data: Vec<u8> = samples.iter().flat_map(|&(len, _)| std::iter::repeat_n(0xAB, len as usize)).collect();
        Stream { rate: 44_100, channels: 2, config: vec![0x12, 0x10], samples, data }
    }

    fn find(hay: &[u8], kind: &[u8; 4]) -> Option<usize> {
        hay.windows(4).position(|w| w == kind)
    }

    #[test]
    fn chunk_offsets_point_into_mdat() {
        let s = stream(100);
        let out = write(&s, &Tags { title: "Música".into(), artist: "Fulano".into(), album: String::new(), cover: Some(vec![0xFF, 0xD8, 0xFF, 0xE0]) });
        let mdat = find(&out, b"mdat").unwrap() + 4;
        assert_eq!(out.len(), mdat + s.data.len());
        let stco = find(&out, b"stco").unwrap() + 4;
        let count = u32::from_be_bytes(out[stco + 4..stco + 8].try_into().unwrap()) as usize;
        assert_eq!(count, 100usize.div_ceil(PER_CHUNK));
        let first = u32::from_be_bytes(out[stco + 8..stco + 12].try_into().unwrap()) as usize;
        assert_eq!(first, mdat);
        let second = u32::from_be_bytes(out[stco + 12..stco + 16].try_into().unwrap()) as usize;
        let skipped: usize = s.samples[..PER_CHUNK].iter().map(|&(n, _)| n as usize).sum();
        assert_eq!(second, mdat + skipped);
        // O `moov` vem antes do áudio (abre sem ler o arquivo todo).
        assert!(find(&out, b"moov").unwrap() < mdat);
        assert!(find(&out, b"covr").is_some());
        assert!(out.windows("Música".len()).any(|w| w == "Música".as_bytes()));
    }

    #[test]
    fn sizes_of_every_atom_add_up() {
        let out = write(&stream(50), &Tags::default());
        // As caixas de cima cobrem o arquivo inteiro, sem sobra nem falta.
        let mut at = 0;
        let mut kinds = Vec::new();
        while at < out.len() {
            let size = u32::from_be_bytes(out[at..at + 4].try_into().unwrap()) as usize;
            kinds.push(out[at + 4..at + 8].to_vec());
            assert!(size >= 8);
            at += size;
        }
        assert_eq!(at, out.len());
        assert_eq!(kinds, vec![b"ftyp".to_vec(), b"moov".to_vec(), b"mdat".to_vec()]);
    }

    fn decoded_frames(bytes: Vec<u8>) -> usize {
        let mut d = crate::source::Decoder::open(bytes.into()).expect("decodificador");
        let mut out = Vec::new();
        let mut frames = 0;
        while d.decode_into(&mut out).expect("decodificar") {
            frames += out.len() / 2;
            out.clear();
        }
        frames
    }

    /// Uma música de verdade: o `.m4a` reembalado toca igual ao original
    /// (mesmo número de quadros). `MUSICFAST_BIN=<pasta do yt-dlp>` e
    /// `MUSICFAST_OUT=<arquivo>` para olhar o resultado no ffprobe.
    #[test]
    #[ignore]
    fn remux_live() {
        let dir = std::env::var_os("MUSICFAST_BIN").map(std::path::PathBuf::from).unwrap_or_default();
        let y = crate::ytdlp::YtDlp::locate(&dir).expect("nenhum yt-dlp");
        let r = y.resolve("djV11Xbc914", None).unwrap_or_else(|e| panic!("{}", e.0));
        let Ok(original) = crate::source::fetch(&crate::source::http_client(), &r) else { panic!("download") };
        let original = original.into_vec();
        let tags = Tags { title: "Take On Me".into(), artist: "a-ha".into(), album: "Hunting High and Low".into(), cover: None };
        let out = remux(original.clone(), &tags).expect("remux");
        if let Some(path) = std::env::var_os("MUSICFAST_OUT") {
            std::fs::write(path, &out).unwrap();
        }
        assert_eq!(decoded_frames(out), decoded_frames(original));
    }

    #[test]
    fn long_descriptors_use_several_length_bytes() {
        assert_eq!(descriptor(5, &[0; 3]), vec![5, 3, 0, 0, 0]);
        let long = descriptor(5, &[0; 200]);
        assert_eq!(&long[..3], &[5, 0x81, 0x48]);
    }

    #[test]
    fn stsc_has_the_short_last_chunk() {
        let b = stsc(&stream(100));
        // 2 entradas: 43 por pedaço e, no pedaço 3, as 14 que sobram.
        assert_eq!(u32::from_be_bytes(b[12..16].try_into().unwrap()), 2);
        assert_eq!(u32::from_be_bytes(b[28..32].try_into().unwrap()), 3);
        assert_eq!(u32::from_be_bytes(b[32..36].try_into().unwrap()), 14);
    }
}
