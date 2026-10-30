//! Onde o app guarda o que é dele: `%LOCALAPPDATA%\MusicFast` no Windows,
//! `~/Library/Application Support/MusicFast` no macOS e
//! `$XDG_DATA_HOME/musicfast` (`~/.local/share/musicfast`) no Linux.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    /// O yt-dlp gerenciado.
    pub bin: PathBuf,
    /// Os endereços de áudio resolvidos (`musicfast-player`).
    pub resolved: PathBuf,
    /// Volume, fila e posição da sessão anterior.
    pub state: PathBuf,
    /// A capa da música atual, em disco, para os controles de mídia do
    /// Windows (eles só leem arquivo).
    pub art: PathBuf,
    /// Os cookies da conta Google, cifrados (`vault`).
    pub session: PathBuf,
    /// As imagens de fundo escolhidas no computador (`look::adopt`).
    pub backdrops: PathBuf,
    /// O perfil do WebView2 da janela de login.
    pub webview: PathBuf,
    pub log: PathBuf,
    pub panic_log: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let root = data_dir();
        Self {
            bin: root.join("bin"),
            resolved: root.join("enderecos.json"),
            state: root.join("estado.json"),
            art: root.join("capas"),
            session: root.join("conta.bin"),
            webview: root.join("webview"),
            backdrops: root.join("fundos"),
            log: root.join("musicfast.log"),
            panic_log: root.join("panicos.log"),
            root,
        }
    }

    pub fn create(&self) {
        // Só o dono lê: a pasta guarda a sessão da conta e o perfil do login.
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.root);
            let _ = std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700));
        }
        for dir in [&self.root, &self.bin, &self.art] {
            if let Err(error) = std::fs::create_dir_all(dir) {
                log::warn!("não criei {}: {error}", dir.display());
            }
        }
    }
}

#[cfg(windows)]
fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("MusicFast")
}

#[cfg(target_os = "macos")]
fn data_dir() -> PathBuf {
    home().join("Library").join("Application Support").join("MusicFast")
}

#[cfg(not(any(windows, target_os = "macos")))]
fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("musicfast")
}

#[cfg(not(windows))]
fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}
