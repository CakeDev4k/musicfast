//! Os cookies da conta. São o equivalente a estar logado no navegador,
//! então nunca vão para o log nem ficam em texto puro quando o sistema tem
//! onde guardá-los:
//!
//! - **Windows:** o arquivo `conta.bin`, cifrado pelo DPAPI (só este usuário,
//!   nesta máquina, consegue ler).
//! - **macOS:** o Chaveiro (Keychain).
//! - **Linux:** o Secret Service (GNOME Keyring, KWallet). Sem ele (um
//!   ambiente mínimo), o arquivo `conta.bin` com permissão só do dono, numa
//!   pasta também só do dono, como os navegadores fazem nesse caso.

use std::path::Path;

#[cfg(windows)]
pub fn save(path: &Path, cookie: &str) {
    match protect(cookie.as_bytes()) {
        Some(bytes) => {
            if let Err(error) = std::fs::write(path, bytes) {
                log::warn!("sessão não gravada: {error}");
            }
        }
        None => log::warn!("sessão não cifrada; não gravei"),
    }
}

#[cfg(windows)]
pub fn load(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let plain = unprotect(&bytes)?;
    String::from_utf8(plain).ok()
}

#[cfg(windows)]
pub fn clear(path: &Path) {
    remove(path);
}

fn remove(path: &Path) {
    if path.exists()
        && let Err(error) = std::fs::remove_file(path)
    {
        log::warn!("sessão não apagada: {error}");
    }
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Option<Vec<u8>> {
    crypt(data, true)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    crypt(data, false)
}

#[cfg(windows)]
fn crypt(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::{HLOCAL, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    // SAFETY: as estruturas apontam para memória válida durante a chamada; a
    // saída é alocada pelo Windows e liberada com LocalFree logo abaixo.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 || output.pbData.is_null() {
        return None;
    }
    // SAFETY: o Windows devolveu `cbData` bytes válidos em `pbData`.
    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(output.pbData as HLOCAL) };
    Some(bytes)
}

/// A entrada do chaveiro do sistema.
#[cfg(not(windows))]
fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new("MusicFast", "sessao-google").map_err(|e| log::warn!("chaveiro: {e}")).ok()
}

#[cfg(not(windows))]
pub fn save(path: &Path, cookie: &str) {
    match entry().map(|e| e.set_password(cookie)) {
        Some(Ok(())) => {
            // Uma cópia em arquivo de antes do chaveiro não fica para trás.
            remove(path);
            return;
        }
        Some(Err(error)) => log::warn!("chaveiro indisponível ({error}); a sessão vai para um arquivo só do dono"),
        None => {}
    }
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(cookie.as_bytes()));
    if let Err(error) = written {
        log::warn!("sessão não gravada: {error}");
    }
}

#[cfg(not(windows))]
pub fn load(path: &Path) -> Option<String> {
    if let Some(entry) = entry() {
        match entry.get_password() {
            Ok(cookie) => return Some(cookie),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => log::warn!("chaveiro: {error}"),
        }
    }
    std::fs::read_to_string(path).ok().filter(|s| !s.trim().is_empty())
}

#[cfg(not(windows))]
pub fn clear(path: &Path) {
    if let Some(entry) = entry() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(error) => log::warn!("chaveiro: {error}"),
        }
    }
    remove(path);
}
