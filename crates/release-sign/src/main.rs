//! A assinatura das releases do MusicFast.
//!
//! ```text
//! release-sign keygen <pasta-da-chave-privada> <arquivo-da-chave-pública>
//! release-sign sign <checksums.txt>          # chave em MUSICFAST_SIGNING_KEY
//! release-sign verify <checksums.txt> <arquivo-da-chave-pública>
//! ```
//!
//! A chave privada é o PKCS#8 em hexadecimal. Ela fica fora do repositório
//! (no segredo `MUSICFAST_SIGNING_KEY` do GitHub e num backup seu); a
//! pública vai embutida no app (`crates/app/update-public-key.hex`). O
//! `sign` escreve `<arquivo>.sig`: os 64 bytes crus da assinatura, o formato
//! que o `fastframe-update` lê.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ring::rand::SystemRandom;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

const ENV: &str = "MUSICFAST_SIGNING_KEY";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["keygen", private_dir, public_file] => keygen(Path::new(private_dir), Path::new(public_file)),
        ["sign", file] => sign(Path::new(file)),
        ["verify", file, public_file] => verify(Path::new(file), Path::new(public_file)),
        _ => Err("uso: release-sign keygen <pasta> <pública> | sign <arquivo> | verify <arquivo> <pública>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("release-sign: {error}");
            ExitCode::FAILURE
        }
    }
}

fn keygen(private_dir: &Path, public_file: &Path) -> Result<(), String> {
    let private_file: PathBuf = private_dir.join("musicfast-update-signing-key.hex");
    if private_file.exists() {
        return Err(format!("{} já existe; não sobrescrevo uma chave", private_file.display()));
    }
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|_| "o gerador de chaves falhou")?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "a chave gerada não abriu")?;
    std::fs::create_dir_all(private_dir).map_err(|e| e.to_string())?;
    std::fs::write(&private_file, hex(pkcs8.as_ref())).map_err(|e| e.to_string())?;
    std::fs::write(public_file, hex(pair.public_key().as_ref())).map_err(|e| e.to_string())?;
    eprintln!("chave privada: {} (guarde um backup; nunca a publique)", private_file.display());
    eprintln!("chave pública: {}", public_file.display());
    Ok(())
}

fn sign(file: &Path) -> Result<(), String> {
    let key = std::env::var(ENV).map_err(|_| format!("defina {ENV} com a chave privada (PKCS#8 em hexadecimal)"))?;
    let pkcs8 = unhex(key.trim())?;
    let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| format!("{ENV} não é uma chave Ed25519 PKCS#8"))?;
    let data = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let signature = pair.sign(&data);
    let out = signature_path(file);
    std::fs::write(&out, signature.as_ref()).map_err(|e| e.to_string())?;
    eprintln!("assinado: {}", out.display());
    Ok(())
}

fn verify(file: &Path, public_file: &Path) -> Result<(), String> {
    let public = unhex(std::fs::read_to_string(public_file).map_err(|e| e.to_string())?.trim())?;
    let data = std::fs::read(file).map_err(|e| e.to_string())?;
    let signature = std::fs::read(signature_path(file)).map_err(|e| e.to_string())?;
    UnparsedPublicKey::new(&ED25519, public).verify(&data, &signature).map_err(|_| "a assinatura NÃO confere".to_owned())?;
    eprintln!("a assinatura confere");
    Ok(())
}

fn signature_path(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".sig");
    PathBuf::from(name)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 || !text.is_ascii() {
        return Err("hexadecimal inválido".to_owned());
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "hexadecimal inválido".to_owned())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_verifies_round_trip() {
        let dir = std::env::temp_dir().join(format!("release-sign-teste-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let public = dir.join("pub.hex");
        std::fs::create_dir_all(&dir).unwrap();
        keygen(&dir, &public).unwrap();
        let key = std::fs::read_to_string(dir.join("musicfast-update-signing-key.hex")).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(&unhex(&key).unwrap()).unwrap();
        let file = dir.join("checksums.txt");
        std::fs::write(&file, "abc  musicfast.zip\n").unwrap();
        std::fs::write(signature_path(&file), pair.sign(b"abc  musicfast.zip\n").as_ref()).unwrap();
        verify(&file, &public).unwrap();
        std::fs::write(&file, "mudado\n").unwrap();
        assert!(verify(&file, &public).is_err());
        assert_eq!(std::fs::read_to_string(&public).unwrap().len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
