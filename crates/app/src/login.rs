//! "Entrar com o Google": a página de login de verdade do Google numa janela
//! com o navegador embutido do sistema (WebView2 no Windows, WKWebView no
//! macOS, WebKitGTK no Linux), num processo à parte (`musicfast --entrar`), para que o laço de
//! eventos dela não divida a thread com o da janela principal.
//!
//! O app nunca vê a senha: quem fala com o Google é o WebView2. Quando a
//! página chega ao music.youtube.com com a sessão aberta, este processo lê os
//! cookies do site, escreve-os numa linha na saída padrão e fecha. O perfil
//! do navegador embutido fica na pasta do app (`webview`; no macOS o
//! WKWebView usa o armazenamento do próprio app), então entrar de novo
//! depois de sair costuma ser só escolher a conta.
//!
//! **Renovação silenciosa** (`--entrar --silencioso`): quando a sessão
//! guardada vence, a mesma janela abre invisível direto no
//! music.youtube.com. Se o Google ainda reconhece o perfil, o site devolve
//! cookies novos e ninguém precisa digitar nada; se não, ela desiste em
//! poucos segundos e o app pede o login de verdade.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A mesma entrada que o botão "Fazer login" do site usa, no idioma do app
/// (`hl`, "pt-BR").
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn login_url(hl: &str) -> String {
    format!(
        "https://accounts.google.com/ServiceLogin?service=youtube&uilel=3&passive=true&hl={hl}&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26app%3Ddesktop%26hl%3D{hl}%26next%3Dhttps%253A%252F%252Fmusic.youtube.com%252F"
    )
}

/// O prefixo da linha que leva os cookies de volta.
const PREFIX: &str = "musicfast-cookies\t";

pub const SILENT: &str = "--silencioso";
/// A renovação silenciosa desiste depois disso.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const SILENT_LIMIT: std::time::Duration = std::time::Duration::from_secs(25);

/// No processo principal: abre a janela de login e espera. `Ok(None)` é a
/// pessoa ter fechado a janela (ou, `silent`, o perfil não ter mais sessão).
pub fn sign_in(silent: bool) -> Result<Option<String>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("--entrar");
    // A janela é outro processo: recebe o idioma que o app está usando.
    cmd.env(musicfast_i18n::ENV, musicfast_i18n::current().code());
    if silent {
        cmd.arg(SILENT);
    }
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| musicfast_i18n::t().login_spawn_failed(&e.to_string()))?;
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    let _ = child.wait();
    Ok(out.lines().find_map(|l| l.strip_prefix(PREFIX)).map(str::to_owned))
}

/// Apaga o perfil do WebView2 (sair de verdade: a próxima entrada pede a
/// conta de novo).
pub fn forget(profile: &Path) {
    if profile.exists()
        && let Err(error) = std::fs::remove_dir_all(profile)
    {
        log::warn!("perfil do login não apagado: {error}");
    }
}

/// No processo `--entrar`: a janela em si. Não volta.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
pub fn window(profile: PathBuf, silent: bool) -> ! {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::time::{Duration, Instant};

    use tao::dpi::LogicalSize;
    use tao::event::{Event, StartCause, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::{WebContext, WebViewBuilder};

    // O `MUSICFAST_LANG` que `sign_in` passou manda no idioma.
    musicfast_i18n::set(musicfast_i18n::system().lang);
    let fail = |what: &str, error: &dyn std::fmt::Display| -> ! {
        eprintln!("{what}: {error}");
        std::process::exit(2)
    };
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(musicfast_i18n::t().login_window_title)
        .with_inner_size(LogicalSize::new(500.0, 720.0))
        .with_window_icon(tao::window::Icon::from_rgba(crate::icon::window_icon().rgba, 64, 64).ok())
        .with_visible(!silent)
        .build(&event_loop)
        .unwrap_or_else(|e| fail("janela", &e));
    let mut context = WebContext::new(Some(profile));
    let builder = WebViewBuilder::new_with_web_context(&mut context).with_url(if silent { "https://music.youtube.com/".to_owned() } else { login_url(&musicfast_i18n::system().tag()) });
    // No Linux o WebKitGTK entra na caixa GTK que o tao põe na janela.
    #[cfg(target_os = "linux")]
    let built = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        match window.default_vbox() {
            Some(vbox) => builder.build_gtk(vbox),
            None => fail("janela", &"sem a caixa GTK padrão"),
        }
    };
    #[cfg(not(target_os = "linux"))]
    let built = builder.build(&window);
    let webview = built.unwrap_or_else(|e| fail("navegador embutido", &e));

    let started = Instant::now();
    // O que o próprio site diz (`ytcfg.get('LOGGED_IN')`): 0 sem resposta
    // ainda, 1 logado, 2 não. Só a renovação silenciosa precisa perguntar:
    // os cookies velhos ainda têm `SAPISID` mesmo com a sessão morta.
    let logged = Arc::new(AtomicU8::new(0));
    event_loop.run(move |event, _, flow| {
        // O contexto vive enquanto o WebView vive.
        let _ = &context;
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(700));
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => std::process::exit(1),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if silent && started.elapsed() > SILENT_LIMIT {
                    std::process::exit(1);
                }
                let at_music = webview.url().is_ok_and(|u| u.starts_with("https://music.youtube.com"));
                if !at_music {
                    // Mandado para a página de login: o perfil não tem sessão.
                    if silent && webview.url().is_ok_and(|u| u.starts_with("https://accounts.google.com")) {
                        std::process::exit(1);
                    }
                    return;
                }
                if silent {
                    match logged.load(Ordering::Relaxed) {
                        1 => {}
                        2 => std::process::exit(1),
                        _ => {
                            let flag = logged.clone();
                            let _ = webview.evaluate_script_with_callback("window.ytcfg ? ytcfg.get('LOGGED_IN') : null", move |answer| {
                                match answer.trim() {
                                    "true" => flag.store(1, Ordering::Relaxed),
                                    "false" => flag.store(2, Ordering::Relaxed),
                                    _ => {}
                                }
                            });
                            return;
                        }
                    }
                }
                let Ok(cookies) = webview.cookies_for_url("https://music.youtube.com") else { return };
                let header = cookies
                    .iter()
                    .map(|c| format!("{}={}", c.name(), c.value()))
                    .collect::<Vec<_>>()
                    .join("; ");
                if musicfast_ytm::Auth::from_cookie_header(&header).is_some() {
                    println!("{PREFIX}{header}");
                    std::process::exit(0);
                }
            }
            _ => {}
        }
    })
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn window(_profile: PathBuf, _silent: bool) -> ! {
    eprintln!("o login pelo navegador embutido só existe no Windows, no macOS e no Linux");
    std::process::exit(2)
}
