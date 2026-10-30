//! As capas: baixadas, decodificadas e reduzidas por quatro threads, viram
//! textura no quadro seguinte. O último pedido sai primeiro (o que acabou de
//! aparecer na tela importa mais do que o que já passou rolando), e as
//! texturas sem uso há mais tempo saem quando passam do limite.
//!
//! A textura tem o tamanho em que é desenhada (o degrau de `BUCKETS`), nunca
//! o do arquivo: a miniatura de vídeo do `i.ytimg.com` chega com até
//! 1280×720 e virava uma textura de 3,6 MB para um quadradinho de 40 px.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, CornerRadius, Rect, TextureHandle, TextureOptions, Ui};
use musicfast_ytm::Thumbs;

use crate::ui::{icons, theme};

const WORKERS: usize = 4;
const MAX_PENDING: usize = 160;
const MAX_TEXTURES: usize = 500;
/// Uma capa que falhou (sem rede, por exemplo) é pedida de novo depois disto,
/// não a cada quadro.
const RETRY: Duration = Duration::from_secs(30);

/// Os tamanhos pedidos ao servidor: poucos, para que a mesma capa em lugares
/// parecidos caia na mesma textura.
const BUCKETS: [u32; 5] = [60, 120, 226, 360, 544];

struct Entry {
    /// Vazia enquanto baixa, ou depois de falhar.
    texture: Option<TextureHandle>,
    /// A cor da capa (média puxada para as cores vivas), para o fundo do player.
    color: Option<Color32>,
    used: u64,
    /// Quando pedir de novo a capa que falhou.
    retry_at: Option<Instant>,
}

/// Um pedido: o endereço e o lado, em pixels, da textura.
struct Request {
    key: String,
    url: String,
    px: u32,
}

#[derive(Default)]
struct Queue {
    pending: Mutex<Vec<Request>>,
    ready: Condvar,
}

pub struct Covers {
    entries: HashMap<String, Entry>,
    queue: Arc<Queue>,
    rx: Receiver<(String, Option<(ColorImage, Color32)>)>,
    frame: u64,
}

impl Covers {
    pub fn new(ctx: &egui::Context, http: Arc<reqwest::blocking::Client>) -> Self {
        let queue = Arc::new(Queue::default());
        let (tx, rx) = channel();
        for n in 0..WORKERS {
            let (queue, tx, http, ctx) = (queue.clone(), tx.clone(), http.clone(), ctx.clone());
            let _ = std::thread::Builder::new()
                .name(format!("capas-{n}"))
                .spawn(move || worker(&queue, &tx, &http, &ctx));
        }
        Self { entries: HashMap::new(), queue, rx, frame: 0 }
    }

    /// No começo do quadro: sobe as capas que chegaram e solta as velhas.
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        while let Ok((key, image)) = self.rx.try_recv() {
            let Some(entry) = self.entries.get_mut(&key) else { continue };
            match image {
                Some((image, color)) => {
                    entry.texture = Some(ctx.load_texture(&key, image, TextureOptions::LINEAR));
                    entry.color = Some(color);
                }
                None => entry.retry_at = Some(Instant::now() + RETRY),
            }
        }
        if self.entries.len() > MAX_TEXTURES {
            let mut ages: Vec<(u64, String)> = self.entries.iter().map(|(k, e)| (e.used, k.clone())).collect();
            ages.sort_unstable();
            for (_, url) in ages.into_iter().take(self.entries.len() - MAX_TEXTURES * 4 / 5) {
                self.entries.remove(&url);
            }
        }
    }

    /// Uma capa já pronta, sem rede (o `--medir`).
    pub fn preload(&mut self, ctx: &egui::Context, url: String, image: ColorImage) {
        let texture = ctx.load_texture(&url, image, TextureOptions::LINEAR);
        self.entries.insert(url, Entry { texture: Some(texture), color: None, used: self.frame, retry_at: None });
    }

    /// Desenha a capa em `rect` (ou o lugar dela, enquanto baixa).
    pub fn paint(&mut self, ui: &Ui, rect: Rect, thumbs: &Thumbs, radius: u8) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let px = (rect.width().max(rect.height()) * ui.ctx().pixels_per_point()).ceil() as u32;
        let bucket = BUCKETS.iter().copied().find(|b| *b >= px).unwrap_or(BUCKETS[BUCKETS.len() - 1]);
        let texture = thumbs.url_for(bucket).and_then(|url| self.texture(url, bucket));
        let corner = CornerRadius::same(radius);
        match texture {
            Some(texture) => {
                egui::Image::new((texture.id(), rect.size())).corner_radius(corner).paint_at(ui, rect);
            }
            None => {
                ui.painter().rect_filled(rect, corner, theme::PLACEHOLDER);
                let side = (rect.width().min(rect.height()) * 0.36).max(12.0);
                let icon = Rect::from_center_size(rect.center(), egui::vec2(side, side));
                icons::paint(ui.painter(), icon, icons::MUSIC_2, Color32::from_gray(90));
            }
        }
    }

    /// A cor da capa (pedindo a menor, se ainda não veio).
    pub fn color(&mut self, thumbs: &Thumbs) -> Option<Color32> {
        let px = BUCKETS[0];
        let url = thumbs.url_for(px)?;
        let key = if url.contains("=w") { url.clone() } else { format!("{url}#{px}") };
        let _loaded = self.texture(url, px)?;
        self.entries.get(&key).and_then(|e| e.color)
    }

    fn texture(&mut self, url: String, px: u32) -> Option<TextureHandle> {
        let frame = self.frame;
        // O endereço das miniaturas de vídeo não muda com o tamanho: a chave
        // leva o degrau, para a capa grande não reaproveitar a de 60 px.
        let key = if url.contains("=w") { url.clone() } else { format!("{url}#{px}") };
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = frame;
            match entry.retry_at {
                Some(at) if Instant::now() >= at => entry.retry_at = None,
                _ => return entry.texture.clone(),
            }
        }
        let mut pending = self.queue.pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending.len() >= MAX_PENDING {
            // Os mais antigos já saíram da tela: esquecê-los deixa que voltem
            // a ser pedidos se reaparecerem.
            let dropped: Vec<Request> = pending.drain(..MAX_PENDING / 2).collect();
            for request in dropped {
                self.entries.remove(&request.key);
            }
        }
        pending.push(Request { key: key.clone(), url, px });
        drop(pending);
        self.queue.ready.notify_one();
        self.entries.insert(key, Entry { texture: None, color: None, used: frame, retry_at: None });
        None
    }
}

fn worker(queue: &Queue, tx: &Sender<(String, Option<(ColorImage, Color32)>)>, http: &reqwest::blocking::Client, ctx: &egui::Context) {
    loop {
        let request = {
            let mut pending = queue.pending.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(request) = pending.pop() {
                    break request;
                }
                pending = queue.ready.wait(pending).unwrap_or_else(|e| e.into_inner());
            }
        };
        // Uma capa que derruba o decodificador não leva a thread junto.
        let fetched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fetch(http, &request)));
        let image = match fetched {
            Ok(Ok(image)) => Some(image),
            Ok(Err(error)) => {
                log::debug!("capa não carregou: {error}");
                None
            }
            Err(_) => {
                log::warn!("capa derrubou o decodificador");
                None
            }
        };
        if tx.send((request.key, image)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

fn fetch(http: &reqwest::blocking::Client, request: &Request) -> Result<(ColorImage, Color32), String> {
    let bytes = http
        .get(&request.url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.bytes())
        .map_err(|e| e.to_string())?;
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = fit_square(img, request.px).to_rgba8();
    let color = vivid_average(rgba.as_raw());
    Ok((ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()), color))
}

/// A média das cores com peso para as saturadas: a média pura de uma capa
/// colorida sobre fundo cinza dava um cinza lamacento.
pub fn vivid_average(rgba: &[u8]) -> Color32 {
    let (mut r, mut g, mut b, mut total) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for px in rgba.chunks_exact(4).step_by(3) {
        let (pr, pg, pb) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let max = pr.max(pg).max(pb);
        let min = pr.min(pg).min(pb);
        // Saturação vezes um pouco de brilho: o preto e o branco quase não contam.
        let weight = 0.05 + (max - min) / 255.0 * (max / 255.0) * 4.0;
        r += pr * weight;
        g += pg * weight;
        b += pb * weight;
        total += weight;
    }
    if total <= 0.0 {
        return Color32::from_gray(40);
    }
    Color32::from_rgb((r / total) as u8, (g / total) as u8, (b / total) as u8)
}

/// Todo lugar de capa é quadrado: corta o centro e reduz para `px`.
///
/// A miniatura de vídeo 4:3 (`hqdefault`, 480×360) traz o vídeo 16:9 com
/// faixas pretas em cima e embaixo; elas saem antes do corte, senão o
/// quadrado ficava com duas tarjas.
fn fit_square(img: image::DynamicImage, px: u32) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());
    let img = if w * 3 == h * 4 && w >= 16 {
        let band = w * 9 / 16;
        img.crop_imm(0, (h - band) / 2, w, band)
    } else {
        img
    };
    let side = img.width().min(img.height());
    let img = if img.width() != img.height() {
        img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side)
    } else {
        img
    };
    if side > px {
        img.resize_exact(px, px, image::imageops::FilterType::Triangle)
    } else {
        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_red_cover_on_grey_comes_out_red() {
        // Três quartos cinza, um quarto vermelho vivo.
        let mut rgba = Vec::new();
        for i in 0..400 {
            rgba.extend_from_slice(if i % 4 == 0 { &[220, 20, 30, 255] } else { &[90, 90, 90, 255] });
        }
        let c = vivid_average(&rgba);
        assert!(c.r() > c.g() + 60 && c.r() > c.b() + 60, "{c:?}");
    }

    fn solid(w: u32, h: u32) -> image::DynamicImage {
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(w, h, image::Rgba([200, 10, 10, 255])))
    }

    #[test]
    fn video_thumbnail_becomes_a_small_square() {
        let out = fit_square(solid(1280, 720), 60);
        assert_eq!((out.width(), out.height()), (60, 60));
    }

    #[test]
    fn letterboxed_thumbnail_loses_the_bars() {
        // 480×360 com tarjas pretas de 45 px em cima e embaixo.
        let mut img = image::RgbaImage::from_pixel(480, 360, image::Rgba([200, 10, 10, 255]));
        for y in (0..45).chain(315..360) {
            for x in 0..480 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        let out = fit_square(image::DynamicImage::ImageRgba8(img), 270).to_rgba8();
        assert_eq!(out.dimensions(), (270, 270));
        assert_ne!(out.get_pixel(135, 1).0[0], 0, "a tarja de cima ficou");
        assert_ne!(out.get_pixel(135, 268).0[0], 0, "a tarja de baixo ficou");
    }

    #[test]
    fn small_cover_is_not_enlarged() {
        let out = fit_square(solid(120, 120), 544);
        assert_eq!((out.width(), out.height()), (120, 120));
    }
}
