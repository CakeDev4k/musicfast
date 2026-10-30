//! O callback de áudio: decodifica a música atual direto da memória.
//!
//! Regras do callback (README do fastframe-audio): não espera ninguém, não
//! aloca no caminho comum, não grava log. Ordens chegam por uma fila sem
//! trava (`rtrb`); o decodificador que sai volta para a thread de controle
//! ser liberada lá (liberar 4 MB é uma chamada ao sistema); eventos saem por
//! um canal com capacidade fixa (`try_send` não bloqueia).
//!
//! **Sem intervalo entre músicas:** a próxima chega decodificador pronto
//! (`Arm`) e entra no mesmo callback em que a atual termina.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;

use crate::engine::Msg;
use crate::source::{Decoder, Resampler};

pub struct Loaded {
    pub token: u64,
    pub decoder: Decoder,
    pub start_at: f64,
}

pub enum RCmd {
    Play(Box<Loaded>),
    Arm(Option<Box<Loaded>>),
    Seek(f64),
    Stop,
    /// 1 = som, 0 = silêncio. Pausar desce suave antes de parar o dispositivo.
    Fade(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Started { token: u64 },
    Ended { token: u64 },
    Failed { token: u64, error: &'static str },
    FadedOut,
}

/// As últimas amostras tocadas (mono, antes do volume), para o visualizador
/// da interface. O callback escreve sem trava; quem lê pode pegar uma
/// amostra no meio de uma escrita, o que num desenho não se vê.
pub struct Scope {
    samples: Box<[AtomicU32]>,
    at: AtomicUsize,
}

impl Scope {
    pub const LEN: usize = 4096;

    fn push(&self, value: f32) {
        let i = self.at.load(Ordering::Relaxed);
        self.samples[i % Self::LEN].store(value.to_bits(), Ordering::Relaxed);
        self.at.store(i.wrapping_add(1), Ordering::Relaxed);
    }

    /// Copia as `out.len()` amostras mais novas, da mais velha à mais nova.
    pub fn read(&self, out: &mut [f32]) {
        let end = self.at.load(Ordering::Relaxed);
        let n = out.len().min(Self::LEN);
        for (k, slot) in out.iter_mut().take(n).enumerate() {
            let i = end.wrapping_sub(n).wrapping_add(k) % Self::LEN;
            *slot = f32::from_bits(self.samples[i].load(Ordering::Relaxed));
        }
    }
}

impl Default for Scope {
    fn default() -> Self {
        Self { samples: (0..Self::LEN).map(|_| AtomicU32::new(0)).collect(), at: AtomicUsize::new(0) }
    }
}

/// O que a interface lê sem perguntar a ninguém.
#[derive(Default)]
pub struct Shared {
    pub token: AtomicU64,
    pub position_ms: AtomicU64,
    /// O volume pedido, já na curva (amplitude), como bits de `f32`.
    pub gain: AtomicU32,
    pub out_rate: AtomicU32,
    pub scope: Scope,
    /// O relógio da saída aberta: diz a latência do dispositivo (o tempo
    /// entre entregar uma amostra e ela sair no fone; ~200 ms no Bluetooth).
    pub clock: std::sync::Mutex<Option<fastframe_audio::Clock>>,
}

struct Playing {
    loaded: Box<Loaded>,
    buf: Vec<f32>,
    pos: usize,
    frames_out: u64,
    started_at: f64,
    resampler: Resampler,
    raw: Vec<f32>,
}

impl Playing {
    fn new(mut loaded: Box<Loaded>, out_rate: u32) -> Self {
        let started_at = if loaded.start_at > 0.5 { loaded.decoder.seek(loaded.start_at) } else { 0.0 };
        let resampler = Resampler::new(loaded.decoder.rate, out_rate);
        Self {
            loaded,
            buf: Vec::with_capacity(8192),
            pos: 0,
            frames_out: 0,
            started_at,
            resampler,
            raw: Vec::with_capacity(8192),
        }
    }

    /// Enche `buf` com o próximo pacote. `Ok(false)` no fim.
    fn refill(&mut self) -> Result<bool, &'static str> {
        self.buf.clear();
        self.pos = 0;
        while self.buf.is_empty() {
            if self.resampler.is_passthrough() {
                if !self.loaded.decoder.decode_into(&mut self.buf)? {
                    return Ok(false);
                }
            } else {
                self.raw.clear();
                if !self.loaded.decoder.decode_into(&mut self.raw)? {
                    return Ok(false);
                }
                self.resampler.process(&self.raw, &mut self.buf);
            }
        }
        Ok(true)
    }
}

pub struct Renderer {
    cmds: rtrb::Consumer<RCmd>,
    retired: rtrb::Producer<Box<Loaded>>,
    events: SyncSender<Msg>,
    shared: Arc<Shared>,
    current: Option<Playing>,
    next: Option<Box<Loaded>>,
    rate: u32,
    channels: usize,
    gain: f32,
    fade: f32,
    /// Coeficiente do filtro de um polo que suaviza volume e fade (~8 ms).
    k: f32,
    faded_out_sent: bool,
}

impl Renderer {
    pub fn new(
        cmds: rtrb::Consumer<RCmd>,
        retired: rtrb::Producer<Box<Loaded>>,
        events: SyncSender<Msg>,
        shared: Arc<Shared>,
    ) -> Self {
        Self {
            cmds,
            retired,
            events,
            shared,
            current: None,
            next: None,
            rate: 44_100,
            channels: 2,
            gain: 0.0,
            fade: 1.0,
            k: 0.003,
            faded_out_sent: true,
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.try_send(Msg::Audio(event));
    }

    fn retire(&mut self, loaded: Box<Loaded>) {
        // Cheia (nunca deveria): libera aqui mesmo, raro e só custa tempo.
        let _ = self.retired.push(loaded);
    }

    fn apply(&mut self, cmd: RCmd) {
        match cmd {
            RCmd::Play(loaded) => {
                if let Some(old) = self.current.take() {
                    self.retire(old.loaded);
                }
                let token = loaded.token;
                self.current = Some(Playing::new(loaded, self.rate));
                self.shared.token.store(token, Ordering::Release);
                self.emit(Event::Started { token });
            }
            RCmd::Arm(next) => {
                if let Some(old) = std::mem::replace(&mut self.next, next) {
                    self.retire(old);
                }
            }
            RCmd::Seek(secs) => {
                if let Some(cur) = self.current.as_mut() {
                    cur.started_at = cur.loaded.decoder.seek(secs);
                    cur.frames_out = 0;
                    cur.buf.clear();
                    cur.pos = 0;
                    cur.resampler = Resampler::new(cur.loaded.decoder.rate, self.rate);
                }
            }
            RCmd::Stop => {
                if let Some(old) = self.current.take() {
                    self.retire(old.loaded);
                }
                if let Some(old) = self.next.take() {
                    self.retire(old);
                }
                self.shared.token.store(0, Ordering::Release);
            }
            RCmd::Fade(to) => {
                self.fade = to;
                self.faded_out_sent = to > 0.0;
            }
        }
    }

    /// O próximo quadro estéreo, passando para a próxima música no fim.
    fn frame(&mut self) -> (f32, f32) {
        loop {
            let Some(cur) = self.current.as_mut() else { return (0.0, 0.0) };
            if cur.pos + 1 < cur.buf.len() {
                let f = (cur.buf[cur.pos], cur.buf[cur.pos + 1]);
                cur.pos += 2;
                cur.frames_out += 1;
                return f;
            }
            match cur.refill() {
                Ok(true) => continue,
                Ok(false) => {
                    let token = cur.loaded.token;
                    let old = self.current.take().expect("atual");
                    self.retire(old.loaded);
                    self.emit(Event::Ended { token });
                    match self.next.take() {
                        Some(next) => {
                            let token = next.token;
                            self.current = Some(Playing::new(next, self.rate));
                            self.shared.token.store(token, Ordering::Release);
                            self.emit(Event::Started { token });
                        }
                        None => {
                            self.shared.token.store(0, Ordering::Release);
                            return (0.0, 0.0);
                        }
                    }
                }
                Err(error) => {
                    let token = cur.loaded.token;
                    let old = self.current.take().expect("atual");
                    self.retire(old.loaded);
                    self.emit(Event::Failed { token, error });
                    return (0.0, 0.0);
                }
            }
        }
    }
}

impl fastframe_audio::Render for Renderer {
    fn configure(&mut self, sample_rate: u32, channels: u16) {
        self.rate = sample_rate.max(8_000);
        self.channels = channels.max(1) as usize;
        self.k = 1.0 - (-1.0 / (0.008 * self.rate as f32)).exp();
        self.shared.out_rate.store(self.rate, Ordering::Relaxed);
        if let Some(cur) = self.current.as_mut() {
            cur.resampler = Resampler::new(cur.loaded.decoder.rate, self.rate);
        }
    }

    fn render(&mut self, out: &mut [f32]) {
        while let Ok(cmd) = self.cmds.pop() {
            self.apply(cmd);
        }
        let target = f32::from_bits(self.shared.gain.load(Ordering::Relaxed)) * self.fade;
        let ch = self.channels;
        for frame in out.chunks_exact_mut(ch) {
            let (l, r) = self.frame();
            self.shared.scope.push((l + r) * 0.5);
            self.gain += (target - self.gain) * self.k;
            if ch == 1 {
                frame[0] = (l + r) * 0.5 * self.gain;
            } else {
                frame[0] = l * self.gain;
                frame[1] = r * self.gain;
                for extra in &mut frame[2..] {
                    *extra = 0.0;
                }
            }
        }
        if let Some(cur) = &self.current {
            let secs = cur.started_at + cur.frames_out as f64 / self.rate as f64;
            self.shared.position_ms.store((secs * 1000.0) as u64, Ordering::Relaxed);
        }
        if !self.faded_out_sent && self.fade == 0.0 && self.gain < 1e-3 {
            self.faded_out_sent = true;
            self.emit(Event::FadedOut);
        }
    }
}
