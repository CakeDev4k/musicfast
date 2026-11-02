//! Um cache em disco com teto de tamanho: o que foi usado há mais tempo sai
//! primeiro. Serve às músicas tocadas (uma música repetida começa em ~10 ms,
//! sem yt-dlp e sem pedir nada ao YouTube) e às capas (o app abre com as
//! capas da última vez, sem baixá-las de novo).
//!
//! "Usado" é a data de modificação do arquivo: cada leitura a renova, e a
//! faxina (`trim`) apaga pela mais velha. Sem índice para corromper: o
//! diretório é a verdade, e um arquivo apagado por fora só vira uma falta.
//! A escrita vai por um arquivo temporário e uma troca de nome, então uma
//! queda no meio não deixa arquivo pela metade com o nome de verdade.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

/// A faxina corre na primeira escrita e depois a cada tantas: listar a
/// pasta a cada capa seria desperdício.
const TRIM_EVERY: u32 = 16;
/// Um temporário mais velho que isso sobrou de uma queda.
const STALE_TMP: Duration = Duration::from_secs(3600);

pub struct DiskCache {
    dir: PathBuf,
    budget: u64,
    ext: &'static str,
    puts: AtomicU32,
}

impl DiskCache {
    /// `budget` em bytes; `ext` sem o ponto.
    pub fn new(dir: PathBuf, budget: u64, ext: &'static str) -> Self {
        Self { dir, budget, ext, puts: AtomicU32::new(0) }
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Um nome de arquivo para uma chave qualquer (um endereço de capa).
    pub fn hashed(key: &str) -> String {
        Sha256::digest(key.as_bytes()).iter().take(16).map(|b| format!("{b:02x}")).collect()
    }

    /// Só letras, números, `-` e `_` (o id de uma música, um hash): nada que
    /// saia da pasta ou mude de sentido no Windows.
    fn path(&self, name: &str) -> Option<PathBuf> {
        let safe = !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        safe.then(|| self.dir.join(format!("{name}.{}", self.ext)))
    }

    /// O arquivo de `name`, se existe, marcado como usado agora.
    pub fn get(&self, name: &str) -> Option<PathBuf> {
        let path = self.path(name)?;
        if !path.is_file() {
            return None;
        }
        if let Ok(file) = std::fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(path)
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.get(name)?).ok()
    }

    /// Guarda `bytes` como `name`. Erros só vão ao log: cache que não grava
    /// é cache que falta, não defeito.
    pub fn put(&self, name: &str, bytes: &[u8]) {
        let Some(path) = self.path(name) else { return };
        // Um só arquivo que come um quarto do teto (um mix de horas) tiraria
        // dezenas de músicas comuns: fica de fora.
        if bytes.is_empty() || bytes.len() as u64 > self.budget / 4 {
            return;
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let tmp = self.dir.join(format!("{name}.{}.{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let written = std::fs::create_dir_all(&self.dir)
            .and_then(|_| std::fs::write(&tmp, bytes))
            .and_then(|_| std::fs::rename(&tmp, &path));
        if let Err(error) = written {
            let _ = std::fs::remove_file(&tmp);
            log::debug!("cache em {}: {error}", self.dir.display());
            return;
        }
        if self.puts.fetch_add(1, Ordering::Relaxed).is_multiple_of(TRIM_EVERY) {
            self.trim();
        }
    }

    /// Tira `name` do cache (o arquivo não decodificou). `true` se havia.
    pub fn forget(&self, name: &str) -> bool {
        self.path(name).is_some_and(|p| std::fs::remove_file(p).is_ok())
    }

    /// Acima do teto, apaga pelos usados há mais tempo até 90 % dele (folga
    /// para não faxinar a cada escrita). Leva junto os temporários velhos.
    pub fn trim(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        let suffix = format!(".{}", self.ext);
        let now = SystemTime::now();
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if name.ends_with(".tmp") {
                if now.duration_since(modified).is_ok_and(|age| age > STALE_TMP) {
                    let _ = std::fs::remove_file(entry.path());
                }
            } else if name.ends_with(&suffix) {
                files.push((modified, meta.len(), entry.path()));
            }
        }
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        if total <= self.budget {
            return;
        }
        files.sort_by_key(|(modified, _, _)| *modified);
        let target = self.budget / 10 * 9;
        for (_, len, path) in files {
            if total <= target {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("musicfast-teste-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn what_goes_in_comes_back_and_unsafe_names_are_refused() {
        let dir = scratch("ida");
        let cache = DiskCache::new(dir.clone(), 1 << 20, "m4a");
        cache.put("djV11Xbc914", b"audio");
        assert_eq!(cache.read("djV11Xbc914").as_deref(), Some(&b"audio"[..]));
        assert!(cache.read("outra").is_none());
        cache.put("../fora", b"x");
        cache.put("a:b", b"x");
        assert!(!dir.parent().unwrap().join("fora.m4a").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(cache.forget("djV11Xbc914"));
        assert!(!cache.forget("djV11Xbc914"));
        assert_eq!(DiskCache::hashed("https://x/a=w120").len(), 32);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn over_the_budget_the_least_recently_used_go_first() {
        let dir = scratch("teto");
        let cache = DiskCache::new(dir.clone(), 1000, "m4a");
        std::fs::create_dir_all(&dir).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        for (i, name) in ["a", "b", "c", "d"].iter().enumerate() {
            let path = dir.join(format!("{name}.m4a"));
            std::fs::write(&path, vec![0u8; 240]).unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(old + Duration::from_secs(i as u64)).unwrap();
        }
        // "a" é a mais velha, mas acabou de ser lida: quem sai é "b".
        assert!(cache.get("a").is_some());
        // 960 + 240 = 1.200 > 1.000: a faxina desce a 900 ou menos.
        std::fs::write(dir.join("e.m4a"), vec![0u8; 240]).unwrap();
        cache.trim();
        let left: Vec<String> = ["a", "b", "c", "d", "e"].iter().filter(|n| dir.join(format!("{n}.m4a")).exists()).map(|n| n.to_string()).collect();
        assert_eq!(left, ["a", "d", "e"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_bigger_than_a_quarter_of_the_budget_is_not_kept() {
        let dir = scratch("grande");
        let cache = DiskCache::new(dir.clone(), 1000, "m4a");
        cache.put("mix", &vec![0u8; 251]);
        assert!(cache.get("mix").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
