//! A sessão que sobrevive ao fechar: volume, fila, posição e o `visitorData`
//! (que mantém o início coerente entre execuções).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Duas gravações ao mesmo tempo disputariam o mesmo `.tmp`.
static WRITING: Mutex<()> = Mutex::new(());

use musicfast_player::Repeat;
use musicfast_ytm::Track;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub queue: Vec<Track>,
    pub index: Option<usize>,
    pub position: f64,
    pub visitor: Option<String>,
    /// A versão do cliente WEB_REMIX lida do site, e quando (Unix, s).
    pub client_version: Option<String>,
    pub version_checked: u64,
    pub side: Option<String>,
    /// A coluna da esquerda recolhida.
    pub sidebar_mini: bool,
    /// A largura da coluna da esquerda aberta, ajustada arrastando a borda.
    pub sidebar_width: f32,
    /// Onde o mini player ficou: o canto de baixo à esquerda, em pontos da
    /// tela (a letra abre para cima, então o pé é o que fica parado).
    pub mini_anchor: Option<[f32; 2]>,
    /// Onde a janela estava da última vez que esteve normal.
    pub window: Option<WindowGeom>,
    /// O idioma escolhido ("pt", "en", "es"); sem ele, o do sistema.
    pub language: Option<String>,
    /// O fundo do player ("Personalizar o player").
    pub look: crate::look::Look,
    /// A tradução da letra embaixo de cada linha (quando ela é de outro idioma).
    pub translate_lyrics: bool,
}

/// Posição e tamanho da janela, em pontos.
///
/// O app guarda isto em vez de deixar o eframe guardar: o eframe gravava a
/// janela como estava ao fechar, e fechada minimizada (pelo `--sair`, ou pelo
/// Windows ao desligar) ela está em -32000,-32000 com 0×0 — o app reabria
/// numa janelinha do tamanho do título. Aqui só se grava a janela normal, e
/// a lida é conferida antes de valer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeom {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl WindowGeom {
    pub const MIN_SIZE: [f32; 2] = [780.0, 540.0];

    /// Nem a posição de janela minimizada (-32000), nem tamanho abaixo do
    /// mínimo ou absurdo.
    pub fn is_sane(&self) -> bool {
        let finite = [self.x, self.y, self.width, self.height].iter().all(|v| v.is_finite());
        finite
            && (-10_000.0..30_000.0).contains(&self.x)
            && (-10_000.0..30_000.0).contains(&self.y)
            && (Self::MIN_SIZE[0]..20_000.0).contains(&self.width)
            && (Self::MIN_SIZE[1]..20_000.0).contains(&self.height)
    }
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            volume: 0.7,
            shuffle: false,
            repeat: Repeat::Off,
            queue: Vec::new(),
            index: None,
            position: 0.0,
            visitor: None,
            client_version: None,
            version_checked: 0,
            side: None,
            sidebar_mini: false,
            sidebar_width: crate::ui::nav::SIDEBAR,
            mini_anchor: None,
            window: None,
            language: None,
            look: crate::look::Look::default(),
            translate_lyrics: true,
        }
    }
}

impl Saved {
    pub fn load(path: &Path) -> Self {
        let mut saved: Self = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                log::warn!("estado ilegível, começando do zero: {error}");
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        // A fila gravada antes de `Thumbs::compact` traz todas as miniaturas
        // de cada música; sem isto, seria regravada assim para sempre.
        for track in &mut saved.queue {
            track.thumbs.compact();
        }
        saved
    }

    /// Grava numa thread, com a fila posta lá: copiar e serializar uma fila
    /// de 2.000 músicas custava 2–5 ms na thread da janela a cada 15 s
    /// (DOCUMENTACAO_PERFORMANCE.md §1.5). Escreve ao lado e renomeia, para
    /// nunca deixar meio arquivo, e uma gravação por vez.
    pub fn save_in_background(mut self, queue: Arc<Vec<Track>>, path: &Path) {
        let path: PathBuf = path.to_owned();
        let write = move || {
            self.queue = queue.as_ref().clone();
            let Ok(json) = serde_json::to_vec(&self) else { return };
            let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
            let tmp = path.with_extension("json.tmp");
            if let Err(error) = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, &path)) {
                log::warn!("não gravei o estado: {error}");
            }
        };
        let _ = std::thread::Builder::new().name("estado".into()).spawn(write);
    }

    /// Como `save_in_background`, esperando (ao fechar).
    pub fn save_now(&self, path: &Path) {
        let Ok(json) = serde_json::to_vec(self) else { return };
        let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = path.with_extension("json.tmp");
        if let Err(error) = std::fs::write(&tmp, &json).and_then(|()| std::fs::rename(&tmp, path)) {
            log::warn!("não gravei o estado: {error}");
        }
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(x: f32, y: f32, width: f32, height: f32) -> WindowGeom {
        WindowGeom { x, y, width, height, maximized: false }
    }

    #[test]
    fn minimized_window_is_never_restored() {
        // O que o eframe tinha gravado ao fechar minimizado.
        assert!(!geom(-32000.0, -32000.0, 0.0, 0.0).is_sane());
        assert!(!geom(100.0, 100.0, 160.0, 28.0).is_sane());
        assert!(!geom(f32::NAN, 0.0, 1240.0, 800.0).is_sane());
    }

    #[test]
    fn normal_window_is_restored() {
        assert!(geom(120.0, 80.0, 1240.0, 800.0).is_sane());
        // Monitor à esquerda do principal.
        assert!(geom(-1800.0, 50.0, 1240.0, 800.0).is_sane());
    }
}
