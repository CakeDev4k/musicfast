//! O que o app ocupa no disco, e a faxina do cache.
//!
//! O áudio nunca vai para o disco (toca da memória, até 48 MB). No disco
//! ficam só coisas pequenas que se refazem sozinhas: as letras, as capas
//! dos controles de mídia e o cache do navegador da janela de login. A
//! conta, a fila, as preferências e os fundos escolhidos não são cache e
//! nunca saem daqui.

use std::path::Path;

use crate::paths::Paths;

/// As pastas de cache do navegador embutido (WebView2, WebKitGTK). Apagar
/// não tira a sessão: os cookies ficam em outros arquivos do perfil.
const BROWSER_CACHES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "GrShaderCache",
    "ShaderCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "GPUPersistentCache",
    "component_crx_cache",
    "Crashpad",
    "WebKitCache",
];

/// Acima disso o cache do navegador é limpo sozinho antes de abrir o login.
const BROWSER_LIMIT: u64 = 40 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub lyrics: u64,
    pub covers: u64,
    pub browser: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.lyrics + self.covers + self.browser
    }
}

/// Mede o cache. Lê só metadados; roda numa thread.
pub fn measure(paths: &Paths) -> Usage {
    Usage { lyrics: dir_size(&paths.lyrics), covers: dir_size(&paths.art), browser: browser_size(&paths.webview) }
}

/// Apaga o cache e diz quanto liberou. `browser`: também o do navegador do
/// login (só com a janela de login fechada, para não mexer em arquivo
/// aberto).
pub fn clear(paths: &Paths, browser: bool) -> u64 {
    let mut freed = clear_files(&paths.lyrics) + clear_files(&paths.art);
    if browser {
        freed += clear_browser(&paths.webview);
    }
    log::info!("cache limpo: {} liberados", human(freed));
    freed
}

/// Antes de abrir o login: se o cache do navegador passou do teto, limpa.
/// Assim ele nunca cresce sem fim, e o login continua rápido.
pub fn trim_browser(profile: &Path) {
    if browser_size(profile) > BROWSER_LIMIT {
        let freed = clear_browser(profile);
        log::info!("cache do login passou do teto: {} liberados", human(freed));
    }
}

/// "1,5 MB", "820 KB".
pub fn human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    let text = if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    };
    if musicfast_i18n::current().code().starts_with("en") { text } else { text.replace('.', ",") }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        })
        .sum()
}

/// Apaga os arquivos soltos da pasta (não as subpastas).
fn clear_files(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut freed = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() && std::fs::remove_file(entry.path()).is_ok() {
            freed += meta.len();
        }
    }
    freed
}

/// As pastas de cache dentro do perfil do navegador, em qualquer nível.
fn browser_caches(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = entry.path();
        if entry.file_name().to_str().is_some_and(|n| BROWSER_CACHES.contains(&n)) {
            out.push(path);
        } else {
            browser_caches(&path, out);
        }
    }
}

fn browser_size(profile: &Path) -> u64 {
    let mut dirs = Vec::new();
    browser_caches(profile, &mut dirs);
    dirs.iter().map(|d| dir_size(d)).sum()
}

fn clear_browser(profile: &Path) -> u64 {
    let mut dirs = Vec::new();
    browser_caches(profile, &mut dirs);
    let mut freed = 0;
    for dir in dirs {
        let size = dir_size(&dir);
        if std::fs::remove_dir_all(&dir).is_ok() {
            freed += size;
        } else {
            // Algum arquivo preso: conta só o que de fato saiu.
            freed += size.saturating_sub(dir_size(&dir));
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clears_only_cache_folders_of_the_browser() {
        let root = std::env::temp_dir().join(format!("musicfast-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("EBWebView").join("Default").join("Cache");
        let cookies = root.join("EBWebView").join("Default").join("Network");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&cookies).unwrap();
        std::fs::write(cache.join("data"), vec![0u8; 1000]).unwrap();
        std::fs::write(cookies.join("Cookies"), vec![0u8; 50]).unwrap();

        assert_eq!(browser_size(&root), 1000);
        assert_eq!(clear_browser(&root), 1000);
        assert!(!cache.exists());
        assert!(cookies.join("Cookies").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
