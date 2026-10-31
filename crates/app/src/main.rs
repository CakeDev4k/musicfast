//! MusicFast: YouTube Music nativo e rápido, em egui.
//!
//! O catálogo vem do `musicfast-ytm` e o tocar do `musicfast-player`; este
//! crate é só a janela: navegação, capas, a barra do player, fila e letra,
//! os controles de mídia do Windows e a sessão que sobrevive ao fechar.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod app;
mod bar_layout;
mod bench;
mod covers;
mod discord;
mod dragout;
mod fonts;
mod icon;
mod jobs;
mod local_tr;
mod login;
mod look;
mod lyrics;
mod viz;
mod paths;
mod state;
mod storage;
mod ui;
mod update;
mod vault;

use std::sync::{Arc, Mutex, OnceLock};

use fastframe_instance::{Claim, Slot};

use crate::paths::Paths;

/// Também o prefixo das mensagens entre a cópia que roda e uma segunda
/// execução: uma cópia de outro app nunca obedece.
const APP_ID: &str = "com.threshold.musicfast";

/// Pedidos de outra execução (`musicfast --tocar-pausar`, ou só abrir de
/// novo), entregues ao quadro seguinte.
#[derive(Default)]
pub struct Wake {
    ctx: OnceLock<egui::Context>,
    requests: Mutex<Vec<String>>,
}

impl Wake {
    fn push(&self, request: &str) {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).push(request.to_owned());
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }

    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.requests.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Os verbos que a cópia que roda aceita. Os de mídia servem para atalhos
/// globais de outros programas.
const VERBS: [(&str, &str); 4] = [
    ("--tocar-pausar", "toggle"),
    ("--proxima", "next"),
    ("--anterior", "previous"),
    ("--sair", "quit"),
];

/// Quando o processo começou: o log registra o primeiro quadro a partir daqui.
pub static STARTED: OnceLock<std::time::Instant> = OnceLock::new();

fn main() -> eframe::Result<()> {
    let _ = STARTED.set(std::time::Instant::now());
    // A janela de login é um processo à parte, aberto pela cópia que roda:
    // não disputa a vaga de instância única nem o log.
    match std::env::args().nth(1).as_deref() {
        // O idioma vem do app que abriu a janela (`musicfast_i18n::ENV`).
        Some("--entrar") => login::window(login::profile_from_env(Paths::new().webview), std::env::args().nth(2).as_deref() == Some(login::SILENT)),
        Some("--medir") => bench::run(),
        Some("--exportar-icone") => {
            let dir = std::env::args().nth(2).unwrap_or_else(|| ".".to_owned());
            if let Err(error) = icon::export(std::path::Path::new(&dir)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            return Ok(());
        }
        // O atualizador confere a versão baixada perguntando a ela.
        Some("--version") => {
            println!("musicfast {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }
    // O ajudante da atualização (`--apply-update`) roda aqui e não volta; as
    // marcas que ele passa ao app novo saem dos argumentos.
    let launch = fastframe_update::intercept(&update::CONFIG);
    let mut request = "show";
    let mut verbose = false;
    for arg in launch.arguments.iter().skip(1) {
        let arg = arg.to_string_lossy();
        match VERBS.iter().find(|(flag, _)| *flag == arg) {
            Some((_, verb)) => request = verb,
            None if arg == "--verbose" => verbose = true,
            None => eprintln!("argumento desconhecido: {arg}"),
        }
    }

    let wake = Arc::new(Wake::default());
    let requests = wake.clone();
    let instance = match Slot::new(APP_ID).claim(request, move |request| match request {
        "show" | "toggle" | "next" | "previous" | "quit" => {
            requests.push(request);
            Some("ok".to_owned())
        }
        _ => None,
    }) {
        Claim::First(guard) => guard,
        // A cópia que roda cuidou do pedido. Sair antes de ligar o log: o
        // arquivo é recriado a cada execução, e esta apagaria o da outra.
        Claim::Running(_) | Claim::Declined | Claim::Unanswered => return Ok(()),
    };
    if request != "show" {
        // Um verbo de mídia sem cópia rodando não tem o que fazer.
        return Ok(());
    }

    let paths = Paths::new();
    paths.create();
    let filter = if verbose {
        "info,musicfast=debug,musicfast_player=debug,musicfast_ytm=debug"
    } else {
        "warn,musicfast=info,musicfast_player=info,musicfast_ytm=info"
    };
    if let Err(error) = fastframe_log::Logging::new("musicfast", env!("CARGO_PKG_VERSION"))
        .filter(filter)
        .file(&paths.log)
        .panic_log(&paths.panic_log)
        .init()
    {
        eprintln!("sem log: {error}");
    }

    let saved = state::Saved::load(&paths.state);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MusicFast")
        .with_app_id(APP_ID)
        .with_inner_size([1240.0, 800.0])
        .with_min_inner_size(state::WindowGeom::MIN_SIZE)
        .with_icon(Arc::new(icon::window_icon()));
    match saved.window {
        Some(g) if g.is_sane() => {
            viewport = viewport
                .with_inner_size([g.width, g.height])
                .with_position([g.x, g.y])
                .with_maximized(g.maximized);
        }
        Some(g) => log::warn!("janela gravada inválida, aberta no tamanho padrão: {g:?}"),
        None => {}
    }
    let options = eframe::NativeOptions {
        viewport,
        // A janela é guardada pelo app (`state::WindowGeom`), não pelo eframe.
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "MusicFast",
        options,
        Box::new(move |cc| {
            let rendering = fastframe_text::detect();
            // A janela pinta com a Inter (0 ms); as fontes do sistema para
            // CJK, árabe e afins (~105 ms de leitura, títulos como "蕾に雷")
            // chegam numa thread e valem a partir do quadro seguinte.
            // Antes seguravam o primeiro quadro (DOCUMENTACAO_PERFORMANCE.md §1.6).
            let mut fonts = fastframe_fonts::FontSetup::default().system_fallbacks(false).definitions();
            rendering.apply_to(&mut fonts);
            cc.egui_ctx.set_fonts(fonts);
            let ctx = cc.egui_ctx.clone();
            let _ = std::thread::Builder::new().name("fontes".into()).spawn(move || {
                let mut fonts = fastframe_fonts::FontSetup::default().definitions();
                fastframe_text::detect().apply_to(&mut fonts);
                ctx.set_fonts(fonts);
                ctx.request_repaint();
            });
            // Escuro sempre, como o site: o egui seguiria o tema do Windows.
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            cc.egui_ctx.all_styles_mut(|style| {
                ui::theme::apply(style);
                rendering.apply_to_visuals(&mut style.visuals);
            });
            let _ = wake.ctx.set(cc.egui_ctx.clone());
            Ok(Box::new(app::MusicApp::new(
                &cc.egui_ctx,
                app::Startup { paths, saved, wake, instance, update_receipt: launch.receipt, update_error: launch.error },
            )))
        }),
    )
}
