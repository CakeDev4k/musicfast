//! Atualização automática pelas releases do GitHub (`fastframe-update`).
//!
//! Uma vez por dia (e uns segundos depois de abrir) o app pergunta ao GitHub
//! se saiu versão nova. No Windows e no Linux, com a cópia baixada da página
//! de releases (a que tem o `musicfast-portable.txt` ao lado), ele baixa em
//! segundo plano, confere a assinatura Ed25519 do `checksums.txt` contra a
//! chave pública embutida (`update-public-key.hex`) e mostra "Reiniciar para
//! atualizar". Um ajudante troca o executável, reabre o app e volta à versão
//! anterior se a nova não abrir.
//!
//! Onde ele não pode se trocar sozinho (o app do macOS sem assinatura da
//! Apple, uma cópia de gerenciador de pacotes, uma compilação local), o botão
//! abre a página da release para baixar à mão.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use fastframe_update::{MacConfig, Prepared, Release, Request, Response, Transport, Unsupported, UpdateConfig, Updater};

pub const REPOSITORY: &str = "CakeDev4k/musicfast";

pub const CONFIG: UpdateConfig = UpdateConfig {
    macos: MacConfig { bundle_ids: &["io.github.cakedev4k.musicfast"], executable_names: &[], legacy_bundle_names: &[] },
    publisher_key: Some(include_str!("../update-public-key.hex")),
    ..UpdateConfig::new(REPOSITORY, "MusicFast", "musicfast", env!("CARGO_PKG_VERSION"))
};

/// O que o app mostra sobre a atualização.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// Nada novo (ou ainda não perguntou).
    Idle,
    /// Baixando a versão nova, com o progresso de 0 a 1.
    Downloading { version: String, progress: f32 },
    /// Baixada e conferida: falta reiniciar.
    Ready { version: String },
    /// Saiu versão nova, mas esta cópia não se troca sozinha: o botão abre
    /// a página da release.
    Manual { version: String, url: String, reason: String },
}

/// O cliente HTTP do atualizador: o mesmo reqwest do app, mas sem seguir
/// redirecionamentos (o `fastframe-update` os segue só para os hosts do
/// GitHub) e com tempo para baixar o pacote inteiro.
struct Http(reqwest::blocking::Client);

impl Transport for Http {
    fn get(&self, request: &Request<'_>) -> anyhow::Result<Response> {
        let response = self
            .0
            .get(request.url)
            .header(reqwest::header::ACCEPT, request.accept)
            .header(reqwest::header::USER_AGENT, request.user_agent)
            .send()?;
        let location = response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).map(str::to_owned);
        Ok(Response { status: response.status().as_u16(), location, body: Box::new(response) })
    }
}

/// O atualizador e o que ele já sabe. Vive no app; o trabalho de rede roda
/// em threads e conversa por aqui.
pub struct Updates {
    updater: Option<Arc<Updater>>,
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
}

#[derive(Default)]
struct Shared {
    state: Option<State>,
    prepared: Option<Prepared>,
    busy: bool,
}

impl Updates {
    pub fn new(ctx: &egui::Context) -> Self {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(15 * 60))
            .build();
        let updater = match client {
            Ok(client) => Some(Arc::new(Updater::new(CONFIG, Http(client)))),
            Err(error) => {
                log::warn!("atualizador sem cliente HTTP: {error}");
                None
            }
        };
        Self { updater, shared: Arc::default(), ctx: ctx.clone() }
    }

    pub fn state(&self) -> State {
        lock(&self.shared).state.clone().unwrap_or(State::Idle)
    }

    /// Pergunta ao GitHub, numa thread. Uma pergunta por vez; depois de ter
    /// a versão nova pronta, não pergunta de novo.
    pub fn check(&self) {
        let Some(updater) = self.updater.clone() else { return };
        {
            let mut shared = lock(&self.shared);
            if shared.busy || shared.prepared.is_some() {
                return;
            }
            shared.busy = true;
        }
        let shared = self.shared.clone();
        let ctx = self.ctx.clone();
        let spawned = std::thread::Builder::new().name("atualizacao".into()).spawn(move || {
            let state = run(&updater, &shared, &ctx);
            let mut guard = lock(&shared);
            guard.busy = false;
            if let Some(state) = state {
                guard.state = Some(state);
            }
            drop(guard);
            ctx.request_repaint();
        });
        if let Err(error) = spawned {
            log::warn!("atualização: {error}");
            lock(&self.shared).busy = false;
        }
    }

    /// Entrega a versão baixada ao ajudante. `Ok(true)`: o app deve fechar
    /// agora (o ajudante espera ele sair para trocar e reabrir).
    pub fn restart(&self) -> Result<bool, String> {
        let Some(updater) = &self.updater else { return Ok(false) };
        let Some(prepared) = lock(&self.shared).prepared.take() else { return Ok(false) };
        match updater.handoff(prepared, Vec::new()) {
            Ok(()) => Ok(true),
            Err(error) => {
                lock(&self.shared).state = Some(State::Idle);
                Err(format!("{error:#}"))
            }
        }
    }
}

fn run(updater: &Updater, shared: &Mutex<Shared>, ctx: &egui::Context) -> Option<State> {
    let release = match updater.check() {
        Ok(Some(release)) => release,
        Ok(None) => {
            log::info!("atualização: já é a versão mais nova");
            return None;
        }
        Err(error) => {
            log::info!("atualização: não deu para perguntar ao GitHub ({error:#})");
            return None;
        }
    };
    log::info!("atualização: saiu a {}", release.version);
    if let Err(reason) = updater.installation() {
        log::info!("atualização: esta cópia não se troca sozinha ({reason})");
        return Some(manual(&release, &reason));
    }
    let version = release.version.clone();
    lock(shared).state = Some(State::Downloading { version: version.clone(), progress: 0.0 });
    ctx.request_repaint();
    let mut last = 0.0f32;
    let downloaded = updater.download(&release, |received, total| {
        let progress = if total > 0 { received as f32 / total as f32 } else { 0.0 };
        // Um quadro a cada 2%, não a cada pedaço da rede.
        if progress - last >= 0.02 {
            last = progress;
            lock(shared).state = Some(State::Downloading { version: version.clone(), progress });
            ctx.request_repaint();
        }
    });
    match downloaded {
        Ok(prepared) => {
            log::info!("atualização: {version} baixada e conferida");
            lock(shared).prepared = Some(prepared);
            Some(State::Ready { version })
        }
        Err(error) => {
            log::warn!("atualização: o download de {version} falhou ({error:#})");
            Some(State::Manual { version, url: release.url.clone(), reason: "o download automático falhou".to_owned() })
        }
    }
}

fn manual(release: &Release, reason: &Unsupported) -> State {
    let reason = match reason {
        Unsupported::MoveToApplications => "mova o MusicFast para a pasta Aplicativos".to_owned(),
        Unsupported::Homebrew => "atualize pelo Homebrew".to_owned(),
        Unsupported::Flatpak | Unsupported::Snap | Unsupported::SystemPackage(_) | Unsupported::Nix => {
            "atualize pelo gerenciador de pacotes".to_owned()
        }
        Unsupported::Cargo | Unsupported::NotPortable => "esta cópia não veio da página de releases".to_owned(),
        _ => "baixe a versão nova na página de releases".to_owned(),
    };
    State::Manual { version: release.version.clone(), url: release.url.clone(), reason }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_configuration_is_valid() {
        CONFIG.validate().expect("UpdateConfig válida");
        assert_eq!(CONFIG.current_version, env!("CARGO_PKG_VERSION"));
    }
}
