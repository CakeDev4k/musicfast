//! O ícone da janela (o do .exe vem do mesmo desenho, pelo `build.rs`).

mod art {
    include!("icon_art.rs");
}

pub fn window_icon() -> egui::IconData {
    const SIDE: u32 = 64;
    egui::IconData { rgba: art::icon_rgba(SIDE), width: SIDE, height: SIDE }
}

/// `musicfast --exportar-icone <pasta>`: grava o ícone em PNG nos tamanhos
/// que o empacotamento usa (o `.icns` do macOS e o ícone do Linux). Só para
/// o CI das releases.
pub fn export(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for side in [16u32, 32, 64, 128, 256, 512, 1024] {
        let path = dir.join(format!("musicfast-{side}.png"));
        image::save_buffer(&path, &art::icon_rgba(side), side, side, image::ExtendedColorType::Rgba8).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}
