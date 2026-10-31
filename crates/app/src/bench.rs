//! `musicfast --medir`: o instrumento das passadas de desempenho
//! (`DOCUMENTACAO_PERFORMANCE.md`).
//!
//! Roda as telas de verdade (`page::show`, `side::queue_list`) num contexto
//! egui sem janela, com catálogos sintéticos e sem rede, rolando com a roda
//! do mouse, e mede por quadro o que é CPU do quadro: a construção (`run_ui`)
//! e a tesselação. **Não mede** a GPU, a pintura do driver, nem o tempo de
//! espera pelo vsync; as capas são texturas já carregadas (o download e a
//! decodificação não entram).
//!
//! Imprime uma tabela em texto na saída padrão (rode com a saída redirecionada:
//! o executável de release não tem console).

use std::sync::Arc;
use std::time::Instant;

use egui::{ColorImage, Event, MouseWheelUnit, Pos2, RawInput, Rect, TouchPhase, vec2};
use musicfast_player::Snapshot;
use musicfast_ytm::{Card, Header, Item, Kind, Link, Page, Shelf, ShelfStyle, Thumb, Thumbs, Track};

use crate::covers::Covers;
use crate::ui::{Cx, Me, PageState, View, page, side, theme};

const WARMUP: usize = 10;
const FRAMES: usize = 150;
const SCREEN: [f32; 2] = [1020.0, 650.0];
const COVERS: usize = 64;

pub fn run() -> ! {
    let started = Instant::now();
    let ctx = egui::Context::default();
    // Sem os fallbacks primeiro: eles são lidos uma vez por processo, e a
    // ordem inversa mediria o cache.
    let t = Instant::now();
    std::hint::black_box(fastframe_fonts::FontSetup::default().system_fallbacks(false).definitions());
    let bare_ms = ms(t);
    let t = Instant::now();
    let rendering = fastframe_text::detect();
    let mut fonts = fastframe_fonts::FontSetup::default().definitions();
    rendering.apply_to(&mut fonts);
    let fonts_ms = ms(t);
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        theme::apply(style);
        rendering.apply_to_visuals(&mut style.visuals);
    });

    let http = Arc::new(reqwest::blocking::Client::new());
    let mut covers = Covers::new(&ctx, http);
    for i in 0..COVERS {
        let shade = (40 + i * 3) as u8;
        covers.preload(&ctx, &thumbs(i), ColorImage::filled([226, 226], egui::Color32::from_gray(shade)));
    }
    let me = Me::default();

    println!("MusicFast --medir  ({} quadros por cenário, {} de aquecimento, tela {}x{})", FRAMES, WARMUP, SCREEN[0], SCREEN[1]);
    println!("fontes só com a Inter: {bare_ms:.1} ms; com os fallbacks do sistema (CJK, árabe…): {fonts_ms:.1} ms");
    let t = Instant::now();
    let _ = frame(&ctx, Vec::new(), 0.0, |_| {});
    println!("primeiro quadro (atlas de fontes): {:.1} ms", ms(t));
    println!();
    println!("{:<28} {:>9} {:>9} {:>9}   {:>9} {:>9} {:>9}", "cenário", "p50 ms", "p95 ms", "máx ms", "ui p95", "tess p95", "vértices");

    let scenarios: Vec<(&str, View, Page)> = vec![
        ("playlist 2.000 músicas", View::browse("VLbench", None), playlist(2000)),
        ("playlist 200 músicas", View::browse("VLbench200", None), playlist(200)),
        ("início (10 carrosséis)", View::Home, home()),
        ("biblioteca (grade 300)", View::Library(crate::ui::LibraryTab::Playlists), grid(300)),
    ];
    for (name, view, page_data) in scenarios {
        let state = PageState { page: Some(Arc::new(page_data)), ..PageState::default() };
        let stats = scroll(&ctx, |ui| {
            let mut actions = Vec::new();
            let mut cx = Cx { covers: &mut covers, actions: &mut actions, me: &me, language: None, latency: 0.0, translate_lyrics: false, lyrics_style: &crate::look::LyricsStyle::default(), current: Some("v7".into()), playing: true, offline: None };
            page::show(ui, &mut cx, &view, &state);
        });
        stats.print(name);
    }

    let snap = Snapshot {
        queue: Arc::new((0..2000).map(track).collect()),
        index: Some(10),
        ..Snapshot::default()
    };
    let stats = scroll(&ctx, |ui| {
        let mut actions = Vec::new();
        let mut cx = Cx { covers: &mut covers, actions: &mut actions, me: &me, language: None, latency: 0.0, translate_lyrics: false, lyrics_style: &crate::look::LyricsStyle::default(), current: Some("v10".into()), playing: true, offline: None };
        side::queue_list(ui, &mut cx, &snap, false);
    });
    stats.print("fila 2.000 músicas");

    // Copiar a fila, comparar e serializar: o que o `autosave` fazia na
    // thread da janela a cada 15 s tocando. Desde a passada 1 (§1.5) isso
    // roda na thread `estado`; na janela fica só a comparação sem a fila.
    let mut times = Vec::new();
    let mut bytes = 0;
    for _ in 0..20 {
        let t = Instant::now();
        let saved = crate::state::Saved { queue: snap.queue.as_ref().clone(), index: snap.index, ..Default::default() };
        let previous = crate::state::Saved { position: 1.0, ..saved.clone() };
        let changed = saved != previous;
        let json = serde_json::to_vec(&saved).unwrap_or_default();
        bytes = json.len();
        std::hint::black_box((changed, json));
        times.push(ms(t));
    }
    println!();
    println!("gravar estado (fila 2.000, fora da janela): p50 {:.2} ms, máx {:.2} ms, {} kB", pct(&times, 0.5), pct(&times, 1.0), bytes / 1024);
    println!();
    println!("total: {:.1} s", started.elapsed().as_secs_f64());
    std::process::exit(0)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn cover_url(i: usize) -> String {
    format!("bench://capa/{}", i % COVERS)
}

fn thumbs(i: usize) -> Thumbs {
    Thumbs(vec![Thumb { url: cover_url(i), width: 544 }])
}

fn track(i: usize) -> Track {
    Track {
        video_id: format!("v{i}"),
        title: format!("Música de número {i} com um título comprido para cortar"),
        artists: vec![
            Link { name: format!("Artista {}", i % 37), id: Some(format!("UC{}", i % 37)) },
            Link { name: "Participação".into(), id: None },
        ],
        album: Some(Link { name: format!("Álbum {}", i % 91), id: Some(format!("MPRE{}", i % 91)) }),
        duration_secs: Some(150 + (i as u32 % 120)),
        thumbs: thumbs(i),
        explicit: i.is_multiple_of(9),
    }
}

fn card(i: usize) -> Card {
    Card {
        kind: if i.is_multiple_of(5) { Kind::Artist } else { Kind::Playlist },
        browse_id: format!("VLPL{i}"),
        params: None,
        title: format!("Playlist {i} para ouvir"),
        subtitle: "Playlist • YouTube Music".into(),
        thumbs: thumbs(i),
        play: None,
    }
}

fn playlist(n: usize) -> Page {
    Page {
        header: Some(Header {
            kind: Kind::Playlist,
            title: "Playlist de medida".into(),
            subtitle: "Playlist • 2026".into(),
            detail: format!("{n} músicas"),
            thumbs: thumbs(0),
            ..Header::default()
        }),
        shelves: vec![Shelf { items: (0..n).map(|i| Item::Track(track(i))).collect(), ..Shelf::default() }],
        ..Page::default()
    }
}

fn home() -> Page {
    let mut shelves = vec![Shelf {
        title: "Escolha a dedo".into(),
        items: (0..20).map(|i| Item::Track(track(i))).collect(),
        ..Shelf::default()
    }];
    for s in 0..10 {
        shelves.push(Shelf {
            title: format!("Prateleira {s}"),
            style: ShelfStyle::Carousel,
            items: (0..20).map(|i| Item::Card(card(s * 20 + i))).collect(),
            ..Shelf::default()
        });
    }
    Page { shelves, ..Page::default() }
}

fn grid(n: usize) -> Page {
    Page {
        shelves: vec![Shelf { style: ShelfStyle::Grid, items: (0..n).map(|i| Item::Card(card(i))).collect(), ..Shelf::default() }],
        ..Page::default()
    }
}

struct Stats {
    total: Vec<f64>,
    ui: Vec<f64>,
    tess: Vec<f64>,
    /// Vértices do último quadro: prova de que a tela desenhou alguma coisa.
    vertices: usize,
}

impl Stats {
    fn print(&self, name: &str) {
        // Rápido porque não desenhou nada não é medida (OTIMIZACAO.md §1.3).
        assert!(self.vertices > 2_000, "{name}: o quadro quase não desenhou ({} vértices)", self.vertices);
        println!(
            "{:<28} {:>9.2} {:>9.2} {:>9.2}   {:>9.2} {:>9.2} {:>9}",
            name,
            pct(&self.total, 0.50),
            pct(&self.total, 0.95),
            pct(&self.total, 1.0),
            pct(&self.ui, 0.95),
            pct(&self.tess, 0.95),
            self.vertices
        );
    }
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let i = ((s.len() as f64 - 1.0) * p).round() as usize;
    s[i.min(s.len() - 1)]
}

/// Aquece e depois mede `FRAMES` quadros, cada um com um giro da roda.
fn scroll(ctx: &egui::Context, mut draw: impl FnMut(&mut egui::Ui)) -> Stats {
    let mut stats = Stats { total: Vec::new(), ui: Vec::new(), tess: Vec::new(), vertices: 0 };
    // Volta ao topo: cada cenário tem sua própria área de rolagem.
    for i in 0..(WARMUP + FRAMES) {
        let wheel = Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(0.0, -110.0),
            phase: TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        let events = vec![Event::PointerMoved(Pos2::new(SCREEN[0] / 2.0, SCREEN[1] / 2.0)), wheel];
        let (ui_ms, tess_ms, vertices) = frame(ctx, events, i as f64 / 60.0, &mut draw);
        stats.vertices = vertices;
        if i >= WARMUP {
            stats.total.push(ui_ms + tess_ms);
            stats.ui.push(ui_ms);
            stats.tess.push(tess_ms);
        }
    }
    stats
}

fn frame(ctx: &egui::Context, events: Vec<Event>, time: f64, mut draw: impl FnMut(&mut egui::Ui)) -> (f64, f64, usize) {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN.into())),
        time: Some(time),
        events,
        ..RawInput::default()
    };
    let t = Instant::now();
    let mut output = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG)).show(ui, |ui| draw(ui));
    });
    let ui_ms = ms(t);
    // Sem GPU, as texturas não sobem para lugar nenhum.
    output.textures_delta.clear();
    let t = Instant::now();
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let tess_ms = ms(t);
    let vertices = primitives
        .iter()
        .map(|p| match &p.primitive {
            egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
            egui::epaint::Primitive::Callback(_) => 0,
        })
        .sum();
    (ui_ms, tess_ms, vertices)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A playlist grande não pode voltar a copiar a lista por linha
    /// (DOCUMENTACAO_PERFORMANCE.md §1.1): o quadro custava 1,6 s no release.
    /// O limite é folgado de propósito (build de teste, máquina ocupada).
    #[test]
    fn big_playlist_frame_is_not_quadratic() {
        let ctx = egui::Context::default();
        ctx.set_fonts(fastframe_fonts::FontSetup::default().system_fallbacks(false).definitions());
        let mut covers = Covers::new(&ctx, Arc::new(reqwest::blocking::Client::new()));
        let me = Me::default();
        let view = View::browse("VLteste", None);
        let state = PageState { page: Some(Arc::new(playlist(2000))), ..PageState::default() };
        let mut worst = 0.0f64;
        for i in 0..6 {
            let (ui_ms, _, vertices) = frame(&ctx, Vec::new(), i as f64 / 60.0, |ui| {
                let mut actions = Vec::new();
                let mut cx = Cx { covers: &mut covers, actions: &mut actions, me: &me, language: None, latency: 0.0, translate_lyrics: false, lyrics_style: &crate::look::LyricsStyle::default(), current: None, playing: false, offline: None };
                page::show(ui, &mut cx, &view, &state);
            });
            assert!(vertices > 0);
            if i >= 2 {
                worst = worst.max(ui_ms);
            }
        }
        assert!(worst < 500.0, "quadro da playlist de 2.000: {worst:.0} ms");
    }
}
