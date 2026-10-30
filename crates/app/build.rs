//! O ícone do .exe: o mesmo desenho do ícone da janela (`src/icon_art.rs`),
//! gravado como .ico no `OUT_DIR` e embutido pelo `winresource`.

use std::path::Path;

include!("src/icon_art.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let ico = Path::new(&out).join("musicfast.ico");
    std::fs::write(&ico, ico_bytes(&[16, 24, 32, 48, 64, 256])).expect("gravar o .ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().expect("caminho do .ico"));
    res.set("ProductName", "MusicFast");
    res.set("FileDescription", "MusicFast");
    // Sem o compilador de recursos do Windows SDK o app ainda compila, só
    // sem ícone no Explorer.
    if let Err(error) = res.compile() {
        println!("cargo:warning=sem ícone no .exe: {error}");
    }
}

/// Um .ico com uma imagem BMP de 32 bits por tamanho (o formato que todo
/// Windows lê, sem precisar de um codificador PNG aqui).
fn ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| dib(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&side, image) in sizes.iter().zip(&images) {
        // 256 se escreve 0 no diretório.
        let byte = if side >= 256 { 0 } else { side as u8 };
        out.extend_from_slice(&[byte, byte, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        out.extend_from_slice(&image);
    }
    out
}

fn dib(side: u32) -> Vec<u8> {
    let rgba = icon_rgba(side);
    let mask_row = side.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(side as i32).to_le_bytes());
    // A altura conta a imagem e a máscara.
    out.extend_from_slice(&(side as i32 * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    // De baixo para cima, em BGRA.
    for y in (0..side).rev() {
        for x in 0..side {
            let i = ((y * side + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    // A máscara AND fica zerada: o alfa já recorta.
    out.extend(std::iter::repeat_n(0u8, (mask_row * side) as usize));
    out
}
