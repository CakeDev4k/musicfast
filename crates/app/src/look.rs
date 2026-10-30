//! A aparência do player, escolhida em "Personalizar o player", para duas
//! superfícies com ajustes próprios: a tela "Tocando agora" e a barra de
//! baixo. Cada uma tem um fundo (o original, a cor da capa, uma cor fixa ou
//! uma imagem/GIF do computador ou de um link) e, por cima de qualquer um
//! deles, um visualizador que se mexe com o som (`viz`).
//!
//! A imagem é lida e decodificada numa thread; o GIF vira uma textura por
//! quadro, reduzidas até caberem em `FRAMES_BUDGET`, e o quadro da vez sai
//! do relógio da janela.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use egui::{Color32, ColorImage, CornerRadius, Mesh, Painter, Rect, TextureHandle, TextureOptions, pos2, vec2};
use serde::{Deserialize, Serialize};

use crate::ui::{icons, theme};
use crate::viz::{BANDS, Viz, VizCfg};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backdrop {
    #[default]
    Plain,
    Cover,
    Color,
    Image,
}

impl Backdrop {
    pub const ALL: [Self; 4] = [Self::Plain, Self::Cover, Self::Color, Self::Image];

    pub fn label(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Plain => t.look_default,
            Self::Cover => t.look_cover,
            Self::Color => t.look_color,
            Self::Image => t.look_image,
        }
    }

    pub fn icon(self) -> icons::Icon {
        match self {
            Self::Plain => icons::MOON,
            Self::Cover => icons::PIPETTE,
            Self::Color => icons::PAINT_BUCKET,
            Self::Image => icons::IMAGE,
        }
    }

    /// A frase que explica a escolha, embaixo dos cartões.
    pub fn hint(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Plain => t.look_hint_plain,
            Self::Cover => t.look_hint_cover,
            Self::Color => t.look_hint_color,
            Self::Image => t.image_formats,
        }
    }
}

/// Onde a aparência vale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Surface {
    /// A tela "Tocando agora".
    #[default]
    Player,
    /// A barra de baixo.
    Bar,
    /// O mini player sempre no topo.
    Mini,
    /// A tela principal: o fundo atrás do início, da busca e da coluna.
    Home,
}

impl Surface {
    pub const ALL: [Self; 4] = [Self::Home, Self::Player, Self::Bar, Self::Mini];

    pub fn label(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Player => t.surface_player,
            Self::Bar => t.surface_bar,
            Self::Mini => t.surface_mini,
            Self::Home => t.surface_home,
        }
    }

    pub fn icon(self) -> icons::Icon {
        match self {
            Self::Home => icons::HOUSE,
            Self::Player => icons::MONITOR_PLAY,
            Self::Bar => icons::PANEL_BOTTOM,
            Self::Mini => icons::PICTURE_IN_PICTURE_2,
        }
    }

    /// A cor por baixo de tudo (a do fundo original de cada uma).
    pub fn base(self) -> Color32 {
        match self {
            Self::Player => theme::BG,
            Self::Bar => crate::ui::bar::BG,
            Self::Mini => crate::ui::mini::FILL,
            Self::Home => theme::BG,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Player => 0,
            Self::Bar => 1,
            Self::Mini => 2,
            Self::Home => 3,
        }
    }
}

/// A aparência de uma superfície.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Skin {
    pub backdrop: Backdrop,
    /// O visualizador por cima do fundo, se ligado.
    pub viz: Option<Viz>,
    /// Cores, quantidade, sensibilidade, opacidade, espessura e brilho dele.
    pub viz_cfg: VizCfg,
    pub color: [u8; 3],
    /// Um link (https://…) ou um arquivo copiado para a pasta do app.
    pub image_url: String,
    /// Quanto o fundo escurece (0 a 0,9), para o conteúdo se ler por cima.
    pub dim: f32,
    /// Quanto a imagem de fundo desfoca (0 a 1).
    pub blur: f32,
}

impl Default for Skin {
    fn default() -> Self {
        Self { backdrop: Backdrop::Plain, viz: None, viz_cfg: VizCfg::default(), color: PRESETS[0], image_url: String::new(), dim: 0.45, blur: 0.0 }
    }
}

impl Skin {
    /// Precisa da cor da capa (o fundo dela, ou o visualizador tingido por ela).
    pub fn wants_cover(&self) -> bool {
        self.backdrop == Backdrop::Cover || self.viz.is_some()
    }
}

/// O que a pessoa escolheu; vai no `estado.json`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub player: Skin,
    pub bar: Skin,
    pub mini: Skin,
    pub home: Skin,
}

impl Look {
    pub fn skin(&self, surface: Surface) -> &Skin {
        match surface {
            Surface::Player => &self.player,
            Surface::Bar => &self.bar,
            Surface::Mini => &self.mini,
            Surface::Home => &self.home,
        }
    }

    pub fn skin_mut(&mut self, surface: Surface) -> &mut Skin {
        match surface {
            Surface::Player => &mut self.player,
            Surface::Bar => &mut self.bar,
            Surface::Mini => &mut self.mini,
            Surface::Home => &mut self.home,
        }
    }
}

/// As cores prontas da "Cor fixa".
pub const PRESETS: [[u8; 3]; 8] = [
    [92, 40, 140],
    [30, 64, 175],
    [8, 120, 120],
    [22, 120, 60],
    [190, 120, 20],
    [190, 50, 40],
    [170, 30, 100],
    [60, 60, 70],
];

/// O maior arquivo aceito, os quadros de um GIF e a memória que eles podem
/// ocupar juntos depois de reduzidos.
const MAX_BYTES: usize = 40 * 1024 * 1024;
const MAX_FRAMES: usize = 300;
const FRAMES_BUDGET: usize = 160 * 1024 * 1024;
/// O lado maior de uma imagem parada: o fundo fica escurecido, não precisa de mais.
const MAX_SIDE: u32 = 1280;

type Frames = Vec<(ColorImage, Duration)>;

enum Load {
    Idle,
    Loading(Receiver<Result<Frames, String>>),
    Ready { frames: Vec<(TextureHandle, f64)>, total: f64 },
    Failed(String),
}

/// A imagem de fundo de uma superfície: o endereço pedido e o que já chegou.
pub struct Image {
    url: String,
    /// O desfoque com que a imagem foi pedida (mudar pede de novo).
    blur: f32,
    load: Load,
    http: Arc<reqwest::blocking::Client>,
    /// Para as texturas das duas superfícies não terem o mesmo nome.
    name: &'static str,
}

impl Image {
    fn new(http: Arc<reqwest::blocking::Client>, name: &'static str) -> Self {
        Self { url: String::new(), blur: 0.0, load: Load::Idle, http, name }
    }

    /// Pede o endereço com o desfoque (só lê de novo se um dos dois mudou,
    /// ou se falhou). O desfoque vem em degraus de 0,1: arrastar o controle
    /// não decodifica a imagem a cada quadro.
    pub fn want(&mut self, ctx: &egui::Context, url: &str, blur: f32) {
        let url = url.trim();
        let blur = (blur.clamp(0.0, 1.0) * 10.0).round() / 10.0;
        if url == self.url && blur == self.blur && !matches!(self.load, Load::Failed(_)) {
            return;
        }
        self.url = url.to_owned();
        self.blur = blur;
        if url.is_empty() {
            self.load = Load::Idle;
            return;
        }
        let (tx, rx) = channel();
        let (http, url, ctx) = (self.http.clone(), url.to_owned(), ctx.clone());
        let _ = std::thread::Builder::new().name("fundo".into()).spawn(move || {
            // Um arquivo malformado não derruba o app.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fetch(&http, &url, blur)))
                .unwrap_or_else(|_| Err("decodificador falhou".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.load = Load::Loading(rx);
    }

    /// Sobe os quadros que chegaram. `Some(erro)` uma vez, se falhou.
    fn poll(&mut self, ctx: &egui::Context) -> Option<String> {
        let Load::Loading(rx) = &self.load else { return None };
        let result = rx.try_recv().ok()?;
        match result {
            Ok(frames) => {
                let mut total = 0.0;
                let frames = frames
                    .into_iter()
                    .enumerate()
                    .map(|(i, (image, delay))| {
                        total += delay.as_secs_f64();
                        (ctx.load_texture(format!("{}-{i}", self.name), image, TextureOptions::LINEAR), total)
                    })
                    .collect();
                self.load = Load::Ready { frames, total };
                None
            }
            Err(error) => {
                log::warn!("fundo do player: {error}");
                self.load = Load::Failed(error.clone());
                Some(error)
            }
        }
    }

    pub fn loading(&self) -> bool {
        matches!(self.load, Load::Loading(_))
    }

    pub fn ready(&self) -> bool {
        matches!(self.load, Load::Ready { .. })
    }

    pub fn error(&self) -> Option<&str> {
        match &self.load {
            Load::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// O quadro da vez, e quanto falta para o próximo.
    fn frame(&self, now: f64) -> Option<(&TextureHandle, Option<f64>)> {
        let Load::Ready { frames, total } = &self.load else { return None };
        if frames.len() <= 1 || *total <= 0.0 {
            return frames.first().map(|(t, _)| (t, None));
        }
        let at = now % total;
        let i = frames.iter().position(|(_, end)| at < *end).unwrap_or(frames.len() - 1);
        Some((&frames[i].0, Some(frames[i].1 - at)))
    }
}

/// As imagens das duas superfícies.
pub struct Images([Image; 4]);

impl Images {
    pub fn new(ctx: &egui::Context, http: Arc<reqwest::blocking::Client>, look: &Look) -> Self {
        let mut images = Self([Image::new(http.clone(), "fundo-player"), Image::new(http.clone(), "fundo-barra"), Image::new(http.clone(), "fundo-mini"), Image::new(http, "fundo-inicio")]);
        images.sync(ctx, look);
        images
    }

    pub fn get(&self, surface: Surface) -> &Image {
        &self.0[surface.index()]
    }

    /// Pede o que cada superfície com imagem precisa.
    pub fn sync(&mut self, ctx: &egui::Context, look: &Look) {
        for surface in Surface::ALL {
            let skin = look.skin(surface);
            if skin.backdrop == Backdrop::Image {
                self.0[surface.index()].want(ctx, &skin.image_url, skin.blur);
            }
        }
    }

    /// A cada quadro: sobe o que chegou; devolve o primeiro erro novo.
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<String> {
        let mut error = None;
        for image in &mut self.0 {
            if let Some(e) = image.poll(ctx) {
                error = Some(e);
            }
        }
        error
    }
}

/// Lê e decodifica a imagem (ou os quadros do GIF), já desfocada.
fn fetch(http: &reqwest::blocking::Client, url: &str, blur: f32) -> Result<Frames, String> {
    let bytes: Vec<u8> = if url.starts_with("https://") || url.starts_with("http://") {
        let resp = http.get(url).send().and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
        if resp.content_length().is_some_and(|n| n as usize > MAX_BYTES) {
            return Err("arquivo grande demais".into());
        }
        resp.bytes().map_err(|e| e.to_string())?.to_vec()
    } else {
        // Um arquivo do computador (já copiado para a pasta do app).
        let path = std::path::Path::new(url);
        if !path.is_absolute() {
            return Err("o link precisa começar com https://".into());
        }
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() as usize > MAX_BYTES {
            return Err("arquivo grande demais".into());
        }
        std::fs::read(path).map_err(|e| e.to_string())?
    };
    if bytes.len() > MAX_BYTES {
        return Err("arquivo grande demais".into());
    }
    let format = image::guess_format(&bytes).map_err(|e| e.to_string())?;
    if format == image::ImageFormat::Gif {
        return gif_frames(&bytes, blur);
    }
    let img = image::load_from_memory_with_format(&bytes, format).map_err(|e| e.to_string())?;
    Ok(vec![(to_color_image(soften(shrink(img, MAX_SIDE), blur)), Duration::ZERO)])
}

/// Copia a imagem escolhida no computador para a pasta do app (o fundo
/// continua lá mesmo se o original for movido) e devolve o caminho novo.
/// Uma cópia por superfície; a anterior dela sai.
pub fn adopt(dir: &std::path::Path, surface: Surface, source: &std::path::Path) -> Result<String, String> {
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("img").to_ascii_lowercase();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let prefix = match surface {
        Surface::Player => "player",
        Surface::Bar => "barra",
        Surface::Mini => "mini",
        Surface::Home => "inicio",
    };
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    // Um nome novo a cada escolha: o mesmo nome não seria lido de novo.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let target = dir.join(format!("{prefix}-{stamp}.{ext}"));
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

/// O desfoque proporcional ao tamanho: o mesmo `blur` borra igual uma
/// imagem pequena e uma grande.
fn soften(img: image::DynamicImage, blur: f32) -> image::DynamicImage {
    if blur <= 0.0 {
        return img;
    }
    let sigma = blur * img.width().max(img.height()) as f32 / 40.0;
    img.fast_blur(sigma)
}

fn gif_frames(bytes: &[u8], blur: f32) -> Result<Frames, String> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut used = 0usize;
    for frame in decoder.into_frames().take(MAX_FRAMES) {
        let frame = frame.map_err(|e| e.to_string())?;
        let (num, den) = frame.delay().numer_denom_ms();
        // O navegador trata atraso 0 ou 1 como 100 ms; aqui igual.
        let ms = if den == 0 { 100.0 } else { num as f64 / den as f64 };
        let delay = Duration::from_secs_f64(if ms < 20.0 { 0.1 } else { ms / 1000.0 });
        let img = soften(shrink(image::DynamicImage::ImageRgba8(frame.into_buffer()), 720), blur);
        used += img.width() as usize * img.height() as usize * 4;
        if used > FRAMES_BUDGET && !frames.is_empty() {
            break;
        }
        frames.push((to_color_image(img), delay));
    }
    if frames.is_empty() {
        return Err("GIF sem quadros".into());
    }
    Ok(frames)
}

fn shrink(img: image::DynamicImage, side: u32) -> image::DynamicImage {
    if img.width().max(img.height()) > side {
        img.resize(side, side, image::imageops::FilterType::Triangle)
    } else {
        img
    }
}

fn to_color_image(img: image::DynamicImage) -> ColorImage {
    let rgba = img.to_rgba8();
    ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
}

/// O que o fundo precisa saber da música atual.
pub struct Now<'a> {
    pub cover: Option<Color32>,
    /// As faixas e a onda do som de agora (`viz::Analyzer`); sem elas, nada
    /// toca ainda.
    pub sound: Option<(&'a [f32; BANDS], &'a [f32; crate::viz::WAVE_LEN])>,
}

/// Pinta a aparência `skin` em `rect`, sobre `base`.
pub fn paint(ui: &egui::Ui, rect: Rect, skin: &Skin, image: &Image, now: &Now, base: Color32) {
    paint_on(ui.ctx(), &ui.painter_at(rect), rect, skin, image, now, base);
}

/// Como `paint`, num pintor dado (a camada de fundo da janela, para a tela
/// principal).
pub fn paint_on(ctx: &egui::Context, painter: &Painter, rect: Rect, skin: &Skin, image: &Image, now: &Now, base: Color32) {
    let painter = painter.with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, base);
    match skin.backdrop {
        Backdrop::Plain => {}
        Backdrop::Cover => {
            // A cor anda até a da capa nova, em vez de pular.
            let target = now.cover.unwrap_or(base);
            let c = |i: &str, v: u8| ctx.animate_value_with_time(egui::Id::new(("fundo-capa", i)), v as f32, 0.6) as u8;
            let color = Color32::from_rgb(c("r", target.r()), c("g", target.g()), c("b", target.b()));
            glow(&painter, rect, color, base);
            darken(&painter, rect, skin.dim);
        }
        Backdrop::Color => {
            let [r, g, b] = skin.color;
            glow(&painter, rect, Color32::from_rgb(r, g, b), base);
            darken(&painter, rect, skin.dim);
        }
        Backdrop::Image => {
            if let Some((texture, next)) = image.frame(ctx.input(|i| i.time)) {
                cover_fill(&painter, rect, texture);
                // Escurecido para o texto e os botões continuarem legíveis.
                darken(&painter, rect, skin.dim);
                if let Some(next) = next {
                    ctx.request_repaint_after(Duration::from_secs_f64(next.max(0.016)));
                }
            }
        }
    }
    if let (Some(style), Some((bands, wave))) = (skin.viz, now.sound) {
        let time = ctx.input(|i| i.time);
        crate::viz::paint(&painter, viz_area(rect, style), style, &skin.viz_cfg, bands, wave, now.cover, time);
    }
}

/// Onde o visualizador fica: na barra, nela inteira; na tela, o círculo no
/// meio e os outros no pé, longe das letras e da fila.
fn viz_area(rect: Rect, style: Viz) -> Rect {
    if rect.width() > rect.height() * 4.0 {
        return rect.shrink2(vec2(0.0, rect.height() * 0.08));
    }
    match style {
        Viz::Circle | Viz::Pulse => rect,
        Viz::Line | Viz::Silk | Viz::Curve | Viz::Aurora => Rect::from_min_max(pos2(rect.left(), rect.bottom() - rect.height() * 0.4), pos2(rect.right(), rect.bottom() - rect.height() * 0.08)),
        _ => Rect::from_min_max(pos2(rect.left(), rect.bottom() - rect.height() * 0.38), rect.max),
    }
}

/// A miniatura de uma escolha no "Personalizar o player": a superfície como
/// ela fica, com um player de mentira por cima para mostrar como o conteúdo
/// se lê sobre o fundo. A barra aparece como uma faixa no pé da miniatura.
pub fn preview(ui: &egui::Ui, rect: Rect, skin: &Skin, image: &Image, now: &Now, surface: Surface) {
    // Sem som de verdade (nada tocando), o de enfeite, que mexe sozinho;
    // sem capa, uma cor de exemplo.
    let demo = crate::viz::demo(ui.input(|i| i.time));
    let mut shown = Now { cover: now.cover, sound: now.sound };
    if skin.viz.is_some() && shown.sound.is_none_or(|(b, _)| b.iter().all(|v| *v < 0.01)) {
        shown.sound = Some((&demo.0, &demo.1));
        ui.ctx().request_repaint();
    }
    if skin.backdrop == Backdrop::Cover && shown.cover.is_none() {
        shown.cover = Some(Color32::from_rgb(150, 60, 110));
    }
    let missing = skin.backdrop == Backdrop::Image && image.frame(ui.input(|i| i.time)).is_none();
    let s = rect.height() / 86.0;
    let painter = ui.painter_at(rect);
    match surface {
        Surface::Player => {
            paint(ui, rect, skin, image, &shown, surface.base());
            if missing {
                placeholder(&painter, rect.shrink2(vec2(0.0, rect.height() * 0.15)));
                return;
            }
            let art = Rect::from_min_size(rect.left_top() + vec2(12.0 * s, 14.0 * s), vec2(34.0 * s, 34.0 * s));
            painter.rect_filled(art, CornerRadius::same(3), Color32::from_white_alpha(70));
            fake_lines(&painter, pos2(art.right() + 8.0 * s, art.top() + 8.0 * s), s);
        }
        Surface::Home => {
            // A página de mentira: a coluna, a busca e as capas.
            paint(ui, rect, skin, image, &shown, surface.base());
            if missing {
                placeholder(&painter, rect.shrink2(vec2(0.0, rect.height() * 0.15)));
                return;
            }
            let side = Rect::from_min_max(rect.min, pos2(rect.left() + 16.0 * s, rect.bottom()));
            painter.rect_filled(side, CornerRadius::ZERO, Color32::from_black_alpha(60));
            let search = Rect::from_min_size(pos2(rect.left() + 34.0 * s, rect.top() + 8.0 * s), vec2(rect.width() * 0.5, 9.0 * s));
            painter.rect_filled(search, CornerRadius::same(3), Color32::from_white_alpha(28));
            for row in 0..2 {
                for col in 0..4 {
                    let tile = 22.0 * s;
                    let at = pos2(rect.left() + (24.0 + col as f32 * 27.0) * s, rect.top() + (24.0 + row as f32 * 30.0) * s);
                    painter.rect_filled(Rect::from_min_size(at, vec2(tile, tile)), CornerRadius::same(2), Color32::from_white_alpha(55));
                }
            }
        }
        Surface::Mini => {
            // A janelinha flutuando sobre uma área de trabalho de mentira.
            painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(28, 34, 48));
            let w = rect.width() * 0.86;
            let mini = Rect::from_center_size(rect.center(), vec2(w, w * crate::ui::mini::ROW / crate::ui::mini::WIDTH));
            painter.rect_filled(mini.expand(3.0), CornerRadius::same(6), Color32::from_black_alpha(90));
            paint(ui, mini, skin, image, &shown, surface.base());
            if missing {
                placeholder(&painter, mini);
                return;
            }
            let cy = mini.center().y;
            let k = mini.height() / 68.0;
            let art = Rect::from_center_size(pos2(mini.left() + 34.0 * k, cy), vec2(48.0 * k, 48.0 * k));
            painter.rect_filled(art, CornerRadius::same(2), Color32::from_white_alpha(70));
            fake_lines(&painter, pos2(art.right() + 10.0 * k, art.top() + 12.0 * k), s * 0.7);
            let play = Rect::from_center_size(pos2(mini.right() - 60.0 * k, cy), vec2(22.0 * k, 22.0 * k));
            icons::paint_filled(&painter, play, icons::PLAY, Color32::from_white_alpha(200));
        }
        Surface::Bar => {
            // A página por cima, a barra embaixo.
            painter.rect_filled(rect, CornerRadius::ZERO, theme::BG);
            let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 30.0 * s), rect.max);
            paint(ui, bar, skin, image, &shown, surface.base());
            for i in 0..3 {
                let r = Rect::from_min_size(rect.left_top() + vec2((10.0 + 34.0 * i as f32) * s, 10.0 * s), vec2(26.0 * s, 26.0 * s));
                painter.rect_filled(r, CornerRadius::same(3), Color32::from_white_alpha(18));
            }
            if missing {
                placeholder(&painter, bar);
                return;
            }
            let cy = bar.center().y;
            let play = Rect::from_center_size(pos2(bar.left() + 14.0 * s, cy), vec2(12.0 * s, 12.0 * s));
            icons::paint_filled(&painter, play, icons::PLAY, Color32::from_white_alpha(200));
            let art = Rect::from_center_size(pos2(bar.center().x - 22.0 * s, cy), vec2(18.0 * s, 18.0 * s));
            painter.rect_filled(art, CornerRadius::same(2), Color32::from_white_alpha(70));
            fake_lines(&painter, pos2(art.right() + 6.0 * s, art.top() + 2.0 * s), s * 0.7);
        }
    }
}

fn placeholder(painter: &Painter, rect: Rect) {
    let side = rect.height() * 0.5;
    let r = Rect::from_center_size(rect.center(), vec2(side, side));
    icons::paint(painter, r, icons::IMAGE, Color32::from_gray(100));
}

/// O título e o artista de mentira das miniaturas.
fn fake_lines(painter: &Painter, at: egui::Pos2, s: f32) {
    let line = |dy: f32, w: f32, a: u8| {
        painter.rect_filled(Rect::from_min_size(at + vec2(0.0, dy * s), vec2(w * s, 5.0 * s)), CornerRadius::same(2), Color32::from_white_alpha(a));
    };
    line(0.0, 58.0, 200);
    line(12.0, 38.0, 90);
}

/// O véu preto do "Escurecer".
fn darken(painter: &Painter, rect: Rect, dim: f32) {
    let alpha = (dim.clamp(0.0, 0.9) * 255.0) as u8;
    if alpha > 0 {
        painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_black_alpha(alpha));
    }
}

/// A cor apagando até a base: de cima para baixo na tela, da esquerda para
/// a direita na barra (comprida e baixa, um degradê vertical nem se veria).
fn glow(painter: &Painter, rect: Rect, color: Color32, base: Color32) {
    let strong = color.gamma_multiply(0.75);
    let mid = color.lerp_to_gamma(base, 0.7);
    let horizontal = rect.width() > rect.height() * 4.0;
    let mut mesh = Mesh::default();
    for (t, c) in [0.0, 0.55, 1.0].into_iter().zip([strong, mid, base]) {
        if horizontal {
            let x = rect.left() + rect.width() * t;
            mesh.colored_vertex(pos2(x, rect.top()), c);
            mesh.colored_vertex(pos2(x, rect.bottom()), c);
        } else {
            let y = rect.top() + rect.height() * t;
            mesh.colored_vertex(pos2(rect.left(), y), c);
            mesh.colored_vertex(pos2(rect.right(), y), c);
        }
    }
    for k in [0u32, 2] {
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    painter.add(mesh);
}

/// A imagem cobrindo `rect` inteiro, cortada no centro (o `cover` do CSS).
fn cover_fill(painter: &Painter, rect: Rect, texture: &TextureHandle) {
    let [w, h] = texture.size().map(|v| v as f32);
    let scale = (rect.width() / w).max(rect.height() / h);
    let (sw, sh) = (rect.width() / (w * scale), rect.height() / (h * scale));
    let uv = Rect::from_center_size(pos2(0.5, 0.5), vec2(sw, sh));
    painter.image(texture.id(), rect, uv, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_keeps_every_frame_and_its_delay() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame, Rgba, RgbaImage};
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
                let frame = Frame::from_parts(RgbaImage::from_pixel(8, 8, Rgba(color)), 0, 0, Delay::from_numer_denom_ms(250, 1));
                encoder.encode_frame(frame).unwrap();
            }
        }
        let frames = gif_frames(&bytes, 0.3).unwrap();
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().all(|(_, d)| (d.as_secs_f64() - 0.25).abs() < 0.02), "{:?}", frames.iter().map(|f| f.1).collect::<Vec<_>>());
    }

    #[test]
    fn only_web_links_and_absolute_files_are_read() {
        let http = reqwest::blocking::Client::new();
        assert!(fetch(&http, "file:///C:/Windows/win.ini", 0.0).is_err());
        assert!(fetch(&http, "relativo.png", 0.0).is_err());
    }

    #[test]
    fn a_picked_file_is_copied_and_replaces_the_old_copy() {
        let dir = std::env::temp_dir().join(format!("musicfast-teste-fundo-{}", std::process::id()));
        let source = std::env::temp_dir().join(format!("musicfast-teste-origem-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255])).save(&source).unwrap();
        let first = adopt(&dir, Surface::Bar, &source).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        let second = adopt(&dir, Surface::Bar, &source).unwrap();
        assert_ne!(first, second);
        assert!(!std::path::Path::new(&first).exists());
        let http = reqwest::blocking::Client::new();
        assert_eq!(fetch(&http, &second, 0.5).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&source);
    }
}
