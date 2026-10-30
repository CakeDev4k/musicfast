//! O visualizador: o som que está tocando agora, desenhado por cima do fundo
//! do player e da barra. O callback de áudio guarda as últimas amostras
//! (`Player::scope`); a cada quadro elas passam por uma FFT e viram faixas de
//! frequência em escala logarítmica, que sobem rápido e caem devagar, como
//! num equalizador de aparelho de som.
//!
//! O estilo (`Viz`) e os ajustes (`VizCfg`: cores, quantas barras,
//! sensibilidade, opacidade, espessura e brilho) são de cada superfície.

use std::f32::consts::PI;

use egui::ecolor::Hsva;
use egui::{Color32, CornerRadius, Mesh, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Viz {
    /// Barras subindo do pé.
    Bars,
    /// Barras espelhadas em volta de uma linha no meio.
    Mirror,
    /// A onda do som, como num osciloscópio.
    Line,
    /// Barras em volta de um círculo.
    Circle,
    /// O espectro como um relevo preenchido.
    Mountain,
    /// Barras em quadradinhos, como o painel de um aparelho de som.
    Led,
    /// Colunas de pontos.
    Dots,
    /// Uma curva lisa pelo topo das faixas, espelhada.
    Curve,
    /// Fitas de onda em camadas, ondulando devagar.
    Silk,
    /// Um anel que respira com o grave e se deforma com o resto.
    Pulse,
    /// Cortinas de luz subindo do pé com a forma do som e sumindo para cima.
    Aurora,
}

impl Viz {
    pub const ALL: [Self; 11] = [
        Self::Bars,
        Self::Mirror,
        Self::Led,
        Self::Dots,
        Self::Mountain,
        Self::Curve,
        Self::Line,
        Self::Silk,
        Self::Circle,
        Self::Pulse,
        Self::Aurora,
    ];

    pub fn label(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Bars => t.viz_bars,
            Self::Mirror => t.viz_mirror,
            Self::Line => t.viz_line,
            Self::Circle => t.viz_circle,
            Self::Mountain => t.viz_mountain,
            Self::Led => t.viz_led,
            Self::Dots => t.viz_dots,
            Self::Curve => t.viz_curve,
            Self::Silk => t.viz_silk,
            Self::Pulse => t.viz_pulse,
            Self::Aurora => t.viz_aurora,
        }
    }

    /// Os estilos que desenham faixas separadas (o "Quantidade" muda algo).
    pub fn has_count(self) -> bool {
        !matches!(self, Self::Line | Self::Silk)
    }
}

/// De onde vem a cor do visualizador.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VizColor {
    /// A cor da capa (branco sem capa).
    #[default]
    Cover,
    /// O arco-íris de ponta a ponta.
    Rainbow,
    /// De `color` até `color2`.
    Gradient,
    /// Só `color`.
    Solid,
}

impl VizColor {
    pub const ALL: [Self; 4] = [Self::Cover, Self::Rainbow, Self::Gradient, Self::Solid];

    pub fn label(self) -> &'static str {
        let t = musicfast_i18n::t();
        match self {
            Self::Cover => t.viz_color_cover,
            Self::Rainbow => t.viz_color_rainbow,
            Self::Gradient => t.viz_color_gradient,
            Self::Solid => t.viz_color_solid,
        }
    }
}

/// Os ajustes do visualizador de uma superfície.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VizCfg {
    pub color_mode: VizColor,
    pub color: [u8; 3],
    pub color2: [u8; 3],
    /// Quantas barras (ou pontos, ou raios).
    pub count: u32,
    /// Quanto o som de cada faixa estica (1 = como vem).
    pub sensitivity: f32,
    pub opacity: f32,
    /// De 0,2 (fino, com espaço entre as barras) a 1 (encostadas, grossas).
    pub thickness: f32,
    /// Um halo mais largo e transparente por trás.
    pub glow: bool,
}

impl Default for VizCfg {
    fn default() -> Self {
        Self {
            color_mode: VizColor::Cover,
            color: [120, 200, 255],
            color2: [255, 90, 170],
            count: 48,
            sensitivity: 1.0,
            opacity: 0.7,
            thickness: 0.6,
            glow: false,
        }
    }
}

impl VizCfg {
    pub const COUNT: std::ops::RangeInclusive<u32> = 12..=96;
    pub const SENSITIVITY: std::ops::RangeInclusive<f32> = 0.4..=2.5;
    pub const OPACITY: std::ops::RangeInclusive<f32> = 0.1..=1.0;
    pub const THICKNESS: std::ops::RangeInclusive<f32> = 0.2..=1.0;

    /// A cor num ponto `t` (0 a 1: da esquerda à direita, ou a volta do
    /// círculo), com `level` (0 a 1) clareando as faixas mais altas.
    fn color(&self, t: f32, level: f32, cover: Option<Color32>) -> Color32 {
        let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
        let base = match self.color_mode {
            VizColor::Cover => cover.map_or(Color32::WHITE, |c| c.lerp_to_gamma(Color32::WHITE, 0.45)),
            VizColor::Rainbow => Color32::from(Hsva::new(t.clamp(0.0, 1.0) * 0.83, 0.75, 1.0, 1.0)),
            VizColor::Gradient => rgb(self.color).lerp_to_gamma(rgb(self.color2), t.clamp(0.0, 1.0)),
            VizColor::Solid => rgb(self.color),
        };
        base.gamma_multiply(self.opacity * (0.55 + 0.45 * level.clamp(0.0, 1.0)))
    }
}

const FFT: usize = 2048;
/// As faixas que o analisador calcula; `VizCfg::count` junta de várias em uma.
pub const BANDS: usize = 96;
/// As amostras da onda da "Linha".
pub const WAVE_LEN: usize = 512;
const WAVE: usize = WAVE_LEN;

/// As faixas de agora, já suavizadas.
pub struct Analyzer {
    samples: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    window: Vec<f32>,
    pub bands: [f32; BANDS],
    pub wave: [f32; WAVE],
    last: f64,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self {
            samples: vec![0.0; FFT],
            re: vec![0.0; FFT],
            im: vec![0.0; FFT],
            window: (0..FFT).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / (FFT - 1) as f32).cos()).collect(),
            bands: [0.0; BANDS],
            wave: [0.0; WAVE],
            last: 0.0,
        }
    }
}

impl Analyzer {
    /// Lê o som do player e anda as faixas. `now` em segundos (o relógio da
    /// janela), para a queda não depender da taxa de quadros. Uma vez por
    /// quadro basta: chamadas no mesmo instante não refazem a conta.
    pub fn update(&mut self, player: &musicfast_player::Player, now: f64) {
        if (now - self.last).abs() < 1e-6 {
            return;
        }
        let dt = (now - self.last).clamp(0.0, 0.1) as f32;
        self.last = now;
        let rate = player.scope(&mut self.samples).max(8000) as f32;
        self.wave.copy_from_slice(&self.samples[FFT - WAVE..]);
        for i in 0..FFT {
            self.re[i] = self.samples[i] * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im);
        // De 40 Hz a 16 kHz, cada faixa um pedaço igual em escala de oitavas.
        let (lo, hi) = (40.0f32, 16_000.0f32.min(rate / 2.0));
        let bin_hz = rate / FFT as f32;
        let rise = 1.0 - (-dt / 0.03).exp();
        let fall = 1.0 - (-dt / 0.25).exp();
        for (b, band) in self.bands.iter_mut().enumerate() {
            let f0 = lo * (hi / lo).powf(b as f32 / BANDS as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / BANDS as f32);
            let (k0, k1) = ((f0 / bin_hz) as usize, ((f1 / bin_hz) as usize).max((f0 / bin_hz) as usize + 1));
            let mut peak = 0.0f32;
            for k in k0.max(1)..k1.min(FFT / 2) {
                peak = peak.max((self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt());
            }
            // Em decibéis, com um ganho que sobe com a frequência (a música
            // tem bem menos energia nos agudos e eles sumiriam).
            let db = 20.0 * (peak * 4.0 / FFT as f32 + 1e-9).log10() + 3.0 * (b as f32 / BANDS as f32) * 8.0;
            let level = ((db + 60.0) / 52.0).clamp(0.0, 1.0);
            let k = if level > *band { rise } else { fall };
            *band += (level - *band) * k;
        }
    }

    /// Ainda há movimento para desenhar (as faixas caindo depois da pausa).
    pub fn moving(&self) -> bool {
        self.bands.iter().any(|b| *b > 0.01)
    }
}

/// FFT de raiz 2, no lugar (`re.len()` potência de 2).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f32;
        let (wr, wi) = (angle.cos(), angle.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
}

/// Faixas de enfeite para as miniaturas quando nada toca: mexem com o
/// relógio, para a miniatura mostrar o estilo em movimento.
pub fn demo(now: f64) -> ([f32; BANDS], [f32; WAVE]) {
    let t = now as f32;
    let mut bands = [0.0; BANDS];
    for (i, b) in bands.iter_mut().enumerate() {
        let x = i as f32 / BANDS as f32;
        let k = i as f32 * 0.5;
        *b = (0.55 - 0.35 * x + 0.3 * ((t * 3.1 + k * 0.45).sin() * (t * 1.7 + k * 0.21).cos())).clamp(0.05, 1.0);
    }
    let mut wave = [0.0; WAVE];
    for (i, w) in wave.iter_mut().enumerate() {
        let x = i as f32 / WAVE as f32;
        *w = 0.35 * (x * 18.0 + t * 6.0).sin() * (x * 3.0 + t).sin() + 0.12 * (x * 61.0 - t * 9.0).sin();
    }
    (bands, wave)
}

/// As faixas juntadas em `n`, com a sensibilidade aplicada.
fn levels(bands: &[f32; BANDS], n: usize, sensitivity: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let (a, b) = (i * BANDS / n, ((i + 1) * BANDS / n).max(i * BANDS / n + 1));
            let avg = bands[a..b.min(BANDS)].iter().sum::<f32>() / (b - a) as f32;
            // A curva deixa a sensibilidade alta acender o fraco sem estourar o forte.
            (1.0 - (1.0 - avg.clamp(0.0, 1.0)).powf(sensitivity)).clamp(0.0, 1.0)
        })
        .collect()
}

/// Uma curva lisa (Catmull-Rom) passando pelos pontos.
fn smooth(points: &[Pos2], steps: usize) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * steps);
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(points.len() - 1)];
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3);
            out.push(pos2(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
        }
    }
    out.extend(points.last());
    out
}

/// Um traço, com o halo por trás quando o brilho está ligado.
fn stroke_line(painter: &Painter, points: Vec<Pos2>, width: f32, color: Color32, glow: bool) {
    if glow {
        painter.add(Shape::line(points.clone(), Stroke::new(width * 4.0, color.gamma_multiply(0.18))));
    }
    painter.add(Shape::line(points, Stroke::new(width, color)));
}

fn bar(painter: &Painter, r: Rect, color: Color32, glow: bool) {
    let radius = CornerRadius::same((r.width() / 2.0).min(3.0) as u8);
    if glow {
        painter.rect_filled(r.expand(r.width().clamp(2.0, 6.0)), CornerRadius::same(6), color.gamma_multiply(0.16));
    }
    painter.rect_filled(r, radius, color);
}

/// Desenha o estilo em `rect`. `cover`: a cor da capa, se houver; `now`: o
/// relógio (para o que ondula sozinho).
#[allow(clippy::too_many_arguments)]
pub fn paint(painter: &Painter, rect: Rect, style: Viz, cfg: &VizCfg, bands: &[f32; BANDS], wave: &[f32; WAVE], cover: Option<Color32>, now: f64) {
    // A barra (comprida e baixa) não tem altura para o que é redondo.
    let flat = rect.width() > rect.height() * 4.0;
    let style = match style {
        Viz::Circle if flat => Viz::Mirror,
        Viz::Pulse if flat => Viz::Curve,
        s => s,
    };
    let n = (cfg.count as usize).clamp(4, BANDS);
    let lv = levels(bands, n, cfg.sensitivity);
    let step = rect.width() / n as f32;
    let w = (step * cfg.thickness).max(1.0);
    let x_of = |i: usize| rect.left() + step * i as f32 + (step - w) / 2.0;
    let t_of = |i: usize| i as f32 / (n - 1).max(1) as f32;
    let line_w = 1.0 + 3.0 * cfg.thickness;
    match style {
        Viz::Bars => {
            for (i, l) in lv.iter().enumerate() {
                let h = (rect.height() * 0.92 * l).max(1.5);
                bar(painter, Rect::from_min_max(pos2(x_of(i), rect.bottom() - h), pos2(x_of(i) + w, rect.bottom())), cfg.color(t_of(i), *l, cover), cfg.glow);
            }
        }
        Viz::Mirror => {
            for (i, l) in lv.iter().enumerate() {
                let h = (rect.height() * 0.46 * l).max(1.0);
                let cy = rect.center().y;
                bar(painter, Rect::from_min_max(pos2(x_of(i), cy - h), pos2(x_of(i) + w, cy + h)), cfg.color(t_of(i), *l, cover), cfg.glow);
            }
        }
        Viz::Led => {
            // Quadradinhos de altura fixa; o de cima mais claro.
            let seg = (rect.height() / 14.0).clamp(3.0, 9.0);
            let gap = (seg * 0.3).max(1.0);
            let total = ((rect.height() * 0.95) / seg) as usize;
            for (i, l) in lv.iter().enumerate() {
                let lit = (l * total as f32).round() as usize;
                for k in 0..lit {
                    let y = rect.bottom() - (k + 1) as f32 * seg;
                    let r = Rect::from_min_max(pos2(x_of(i), y + gap), pos2(x_of(i) + w, y + seg));
                    let up = k as f32 / total.max(1) as f32;
                    let c = cfg.color(t_of(i), up, cover);
                    let c = if k + 1 == lit { c.lerp_to_gamma(Color32::WHITE, 0.35) } else { c };
                    if cfg.glow {
                        painter.rect_filled(r.expand(2.0), CornerRadius::same(2), c.gamma_multiply(0.15));
                    }
                    painter.rect_filled(r, CornerRadius::same(1), c);
                }
            }
        }
        Viz::Dots => {
            let r = (w / 2.0).clamp(1.0, 6.0);
            let pitch = r * 2.0 + r.max(2.0);
            let total = ((rect.height() * 0.95) / pitch) as usize;
            for (i, l) in lv.iter().enumerate() {
                let cx = x_of(i) + w / 2.0;
                let lit = (l * total as f32).round() as usize;
                for k in 0..lit {
                    let c = rect.bottom() - pitch * (k as f32 + 0.5);
                    // O de cima é o ponto cheio; os de baixo vão apagando.
                    let fade = if k + 1 == lit { 1.0 } else { 0.35 + 0.4 * k as f32 / lit as f32 };
                    let col = cfg.color(t_of(i), *l, cover).gamma_multiply(fade);
                    if cfg.glow && k + 1 == lit {
                        painter.circle_filled(pos2(cx, c), r * 2.6, col.gamma_multiply(0.2));
                    }
                    painter.circle_filled(pos2(cx, c), r, col);
                }
            }
        }
        Viz::Mountain => {
            // O relevo: um polígono por fatia entre duas faixas (o egui só
            // preenche convexos), com a crista contornada.
            let step = rect.width() / (n - 1).max(1) as f32;
            let top = |i: usize| pos2(rect.left() + step * i as f32, rect.bottom() - rect.height() * 0.9 * lv[i]);
            for i in 0..n - 1 {
                let (a, b) = (top(i), top(i + 1));
                let fill = cfg.color(t_of(i), lv[i], cover).gamma_multiply(0.55);
                painter.add(Shape::convex_polygon(vec![a, b, pos2(b.x, rect.bottom()), pos2(a.x, rect.bottom())], fill, Stroke::NONE));
            }
            let crest: Vec<Pos2> = smooth(&(0..n).map(top).collect::<Vec<_>>(), 4);
            stroke_line(painter, crest, line_w, cfg.color(0.5, 1.0, cover), cfg.glow);
        }
        Viz::Curve => {
            // A curva por cima e o reflexo por baixo, a partir do meio.
            let step = rect.width() / (n - 1).max(1) as f32;
            let cy = rect.center().y;
            for (sign, alpha) in [(-1.0, 1.0), (1.0, 0.45)] {
                let pts: Vec<Pos2> = (0..n).map(|i| pos2(rect.left() + step * i as f32, cy + sign * rect.height() * 0.46 * lv[i])).collect();
                let pts = smooth(&pts, 6);
                // O traço em pedaços, para a cor andar junto (arco-íris, degradê).
                let len = pts.len();
                let mut k = 0;
                while k + 1 < len {
                    let end = (k + 4).min(len);
                    let color = cfg.color(k as f32 / len as f32, 0.9, cover).gamma_multiply(alpha);
                    stroke_line(painter, pts[k..end].to_vec(), line_w, color, cfg.glow);
                    k += 3;
                }
            }
        }
        Viz::Line => {
            let amp = rect.height() * 0.42 * cfg.sensitivity.min(1.8);
            let points: Vec<Pos2> = wave
                .iter()
                .enumerate()
                .step_by(2)
                .map(|(i, s)| pos2(rect.left() + rect.width() * i as f32 / (WAVE - 1) as f32, rect.center().y - (s * 2.5).clamp(-1.0, 1.0) * amp))
                .collect();
            stroke_line(painter, points, line_w, cfg.color(0.5, 1.0, cover), cfg.glow);
        }
        Viz::Silk => {
            // Fitas: cada uma uma senoide cuja altura em cada ponto vem do
            // som daquela faixa, com fase própria andando com o relógio.
            let t = now as f32;
            let fine = levels(bands, 48, cfg.sensitivity);
            let cy = rect.center().y;
            for layer in 0..4 {
                let lf = layer as f32;
                let pts: Vec<Pos2> = (0..=80)
                    .map(|k| {
                        let x = k as f32 / 80.0;
                        let l = fine[((x * 47.0) as usize).min(47)];
                        let y = (x * (6.0 + lf * 2.0) * PI + t * (0.8 + lf * 0.35) + lf * 1.3).sin();
                        pos2(rect.left() + rect.width() * x, cy + y * rect.height() * 0.42 * (0.15 + 0.85 * l))
                    })
                    .collect();
                let c = cfg.color(lf / 3.0, 0.8, cover).gamma_multiply(0.85 - lf * 0.15);
                stroke_line(painter, pts, line_w * (1.0 - lf * 0.15), c, cfg.glow);
            }
        }
        Viz::Circle => {
            let c = rect.center();
            let r0 = rect.width().min(rect.height()) * 0.22;
            let len = rect.width().min(rect.height()) * 0.22;
            // Metade de cada lado, espelhadas, para o círculo fechar.
            let total = n * 2;
            let width = (r0 * 2.0 * PI / total as f32 * cfg.thickness).max(1.0);
            for i in 0..total {
                let l = lv[if i < n { i } else { total - 1 - i }];
                let a = i as f32 / total as f32 * 2.0 * PI - PI / 2.0;
                let dir = vec2(a.cos(), a.sin());
                let col = cfg.color(i as f32 / total as f32, l, cover);
                let seg = [c + dir * r0, c + dir * (r0 + 2.0 + len * l)];
                if cfg.glow {
                    painter.line_segment(seg, Stroke::new(width * 3.0, col.gamma_multiply(0.15)));
                }
                painter.line_segment(seg, Stroke::new(width, col));
            }
            painter.circle_stroke(c, r0 - 3.0, Stroke::new(1.5, cfg.color(0.0, 0.3, cover)));
        }
        Viz::Aurora => {
            // Três cortinas, cada uma uma faixa de malha da crista (cor
            // cheia) até bem acima dela (transparente), ondulando devagar
            // com o relógio por cima da forma do som.
            let t = now as f32;
            let fine = levels(bands, 64, cfg.sensitivity);
            for layer in 0..3 {
                let lf = layer as f32;
                let mut mesh = Mesh::default();
                let cols = 64;
                for k in 0..=cols {
                    let x = k as f32 / cols as f32;
                    let l = fine[((x * 63.0) as usize).min(63)];
                    let sway = 0.5 + 0.5 * (x * (5.0 + lf * 2.0) * PI + t * (0.6 + lf * 0.3) + lf * 2.1).sin();
                    let h = rect.height() * (0.18 + 0.72 * l) * (0.65 + 0.35 * sway) * (1.0 - lf * 0.18);
                    let px = rect.left() + rect.width() * x;
                    let foot = rect.bottom() - rect.height() * 0.04 * lf;
                    let color = cfg.color((x + lf * 0.2) % 1.0, l, cover).gamma_multiply(0.6 - lf * 0.12);
                    mesh.colored_vertex(pos2(px, foot), color.gamma_multiply(0.35));
                    mesh.colored_vertex(pos2(px, foot - h * 0.55), color);
                    mesh.colored_vertex(pos2(px, foot - h), Color32::TRANSPARENT);
                }
                for k in 0..cols as u32 {
                    let (a, b) = (k * 3, (k + 1) * 3);
                    for row in 0..2 {
                        mesh.add_triangle(a + row, b + row, a + row + 1);
                        mesh.add_triangle(b + row, b + row + 1, a + row + 1);
                    }
                }
                if cfg.glow {
                    // O brilho: a mesma cortina mais larga e mais fraca por trás.
                    let mut halo = mesh.clone();
                    for v in &mut halo.vertices {
                        v.color = v.color.gamma_multiply(0.5);
                        v.pos.y -= rect.height() * 0.06;
                    }
                    painter.add(halo);
                }
                painter.add(mesh);
            }
        }
        Viz::Pulse => {
            // O raio respira com o grave; o contorno se deforma com as faixas.
            let c = rect.center();
            let side = rect.width().min(rect.height());
            let bass = lv.iter().take((n / 6).max(1)).sum::<f32>() / (n / 6).max(1) as f32;
            let r0 = side * (0.16 + 0.08 * bass);
            let spin = now as f32 * 0.25;
            let ring: Vec<Pos2> = (0..n * 2)
                .map(|i| {
                    let l = lv[if i < n { i } else { n * 2 - 1 - i }];
                    let a = i as f32 / (n * 2) as f32 * 2.0 * PI + spin;
                    c + vec2(a.cos(), a.sin()) * (r0 + side * 0.14 * l)
                })
                .collect();
            let mut closed = smooth(&[ring.as_slice(), &ring[..3]].concat(), 4);
            closed.truncate(closed.len().saturating_sub(4 * 2));
            // O miolo preenchido por um leque a partir do centro (o contorno
            // não é convexo).
            let mut mesh = Mesh::default();
            mesh.colored_vertex(c, cfg.color(0.5, bass, cover).gamma_multiply(0.35));
            for (k, p) in closed.iter().enumerate() {
                mesh.colored_vertex(*p, cfg.color(k as f32 / closed.len() as f32, 0.5, cover).gamma_multiply(0.15));
            }
            let m = closed.len() as u32;
            for k in 0..m {
                mesh.add_triangle(0, 1 + k, 1 + (k + 1) % m);
            }
            painter.add(mesh);
            let mut outline = closed.clone();
            if let Some(first) = closed.first() {
                outline.push(*first);
            }
            stroke_line(painter, outline, line_w, cfg.color(0.3, 1.0, cover), cfg.glow);
            painter.circle_stroke(c, r0 * 0.6, Stroke::new(1.0, cfg.color(0.7, 0.4, cover)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pure_tone_lands_in_one_bin() {
        let n = 256;
        let mut re: Vec<f32> = (0..n).map(|i| (2.0 * PI * 8.0 * i as f32 / n as f32).sin()).collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..n / 2).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        let top = mag.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(top, 8);
        assert!(mag[3] < 1e-2 && mag[20] < 1e-2);
    }

    #[test]
    fn sensitivity_lifts_quiet_bands_without_going_over() {
        let mut bands = [0.2; BANDS];
        bands[0] = 1.0;
        let soft = levels(&bands, 12, 1.0);
        let loud = levels(&bands, 12, 2.5);
        assert!(loud[5] > soft[5]);
        assert!(loud.iter().all(|l| *l <= 1.0));
    }
}
