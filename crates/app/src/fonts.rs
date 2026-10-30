//! As fontes instaladas no computador, para a letra poder usar qualquer
//! uma. Lê de cada arquivo só o índice e as tabelas `name` (o nome da
//! família) e `OS/2` (o peso e se é itálica): alguns KB por arquivo, não o
//! arquivo inteiro (as fontes chinesas passam de 20 MB). A lista é montada
//! uma vez, numa thread, na primeira vez que o seletor de fonte abre.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Uma face de uma família: o arquivo, o índice dentro dele (nas coleções
/// `.ttc`), o peso (400 normal, 700 negrito) e se é itálica.
#[derive(Clone, Debug)]
pub struct Face {
    pub path: PathBuf,
    pub index: u32,
    pub weight: u16,
    pub italic: bool,
}

#[derive(Clone, Debug)]
pub struct Family {
    pub name: String,
    pub faces: Vec<Face>,
}

impl Family {
    /// A face não itálica de peso mais perto de `weight` (a itálica só se
    /// a família não tem outra).
    pub fn best(&self, weight: u16) -> Option<&Face> {
        let pick = |italic: bool| self.faces.iter().filter(|f| f.italic == italic).min_by_key(|f| (f.weight as i32 - weight as i32).abs());
        pick(false).or_else(|| pick(true))
    }
}

static FAMILIES: OnceLock<Vec<Family>> = OnceLock::new();
static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// As famílias instaladas, em ordem alfabética; `None` enquanto a lista é
/// montada (a primeira chamada a começa numa thread e pede um quadro ao
/// terminar).
pub fn families(ctx: &egui::Context) -> Option<&'static [Family]> {
    if let Some(list) = FAMILIES.get() {
        return Some(list);
    }
    if !SCANNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        let ctx = ctx.clone();
        let _ = std::thread::Builder::new().name("fontes-instaladas".into()).spawn(move || {
            let started = std::time::Instant::now();
            let list = scan();
            log::info!("{} famílias de fonte em {} ms", list.len(), started.elapsed().as_millis());
            let _ = FAMILIES.set(list);
            ctx.request_repaint();
        });
    }
    None
}

/// A família `name`, se a lista já está pronta. Sem ela, lê a lista na hora
/// (ao abrir o app com uma fonte escolhida antes).
pub fn family(name: &str) -> Option<&'static Family> {
    let list = match FAMILIES.get() {
        Some(list) => list,
        None => {
            let list = scan();
            let _ = FAMILIES.set(list);
            FAMILIES.get()?
        }
    };
    list.iter().find(|f| f.name == name)
}

/// O prefixo das famílias de prévia (o nome de cada fonte escrito nela, no
/// seletor).
const PREVIEW: &str = "previa:";
/// Acima disso (as fontes chinesas e japonesas de 20 MB), a prévia fica na
/// fonte do app: carregar custaria memória demais para um nome.
const PREVIEW_MAX_BYTES: u64 = 6 * 1024 * 1024;
/// Quantas prévias entram por quadro, numa só troca das fontes do egui.
const PREVIEW_PER_FRAME: usize = 8;
/// Há prévias nas fontes do egui: sem elas (quase sempre), a faxina de cada
/// quadro (`drop_previews_when_closed`) sai sem olhar as famílias.
static PREVIEWS_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn preview_id() -> egui::Id {
    egui::Id::new("previa-fontes-quadro")
}

/// A família da prévia da fonte `name`, se já carregada.
pub fn preview(ctx: &egui::Context, name: &str, size: f32) -> Option<egui::FontId> {
    let family = egui::FontFamily::Name(format!("{PREVIEW}{name}").into());
    ctx.fonts(|f| f.definitions().families.contains_key(&family)).then(|| egui::FontId::new(size, family))
}

/// As fontes que não dá para mostrar nelas mesmas (grandes demais, ilegíveis).
fn no_preview() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    static SET: OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    SET.get_or_init(Default::default)
}

/// Carrega as prévias das famílias visíveis no seletor (algumas por quadro)
/// e marca que o seletor está aberto (`drop_previews_when_closed`).
pub fn want_previews(ctx: &egui::Context, names: &[&str]) {
    // Fora do `data_mut`: ler o contexto dentro dele trava a thread (o
    // lock do egui não é reentrante).
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(preview_id(), pass));
    let missing: Vec<&str> = {
        let skip = no_preview().lock().unwrap_or_else(|e| e.into_inner());
        names.iter().copied().filter(|n| !skip.contains(*n) && preview(ctx, n, 10.0).is_none()).take(PREVIEW_PER_FRAME).collect()
    };
    if missing.is_empty() {
        return;
    }
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    let fallback = crate::ui::theme::regular(10.0).family;
    let chain = defs.families.get(&fallback).cloned().unwrap_or_default();
    let mut loaded = 0;
    for name in missing {
        let face = family(name).and_then(|f| f.best(400)).cloned();
        let bytes = face.as_ref().filter(|f| std::fs::metadata(&f.path).is_ok_and(|m| m.len() <= PREVIEW_MAX_BYTES)).and_then(|f| std::fs::read(&f.path).ok());
        let (Some(face), Some(bytes)) = (face, bytes) else {
            no_preview().lock().unwrap_or_else(|e| e.into_inner()).insert(name.to_owned());
            continue;
        };
        let key = format!("{PREVIEW}{name}");
        let mut data = egui::FontData::from_owned(bytes);
        data.index = face.index;
        defs.font_data.insert(key.clone(), std::sync::Arc::new(data));
        let mut family_chain = vec![key.clone()];
        family_chain.extend(chain.iter().cloned());
        defs.families.insert(egui::FontFamily::Name(key.into()), family_chain);
        loaded += 1;
    }
    if loaded > 0 {
        PREVIEWS_LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
        ctx.set_fonts(defs);
        ctx.request_repaint();
    }
}

/// Com o seletor fechado há uns quadros, tira as prévias das fontes do egui
/// (podem ser dezenas de MB). Barato quando não há nenhuma.
pub fn drop_previews_when_closed(ctx: &egui::Context) {
    use std::sync::atomic::Ordering;
    if !PREVIEWS_LOADED.load(Ordering::Relaxed) {
        return;
    }
    let last = ctx.data(|d| d.get_temp::<u64>(preview_id())).unwrap_or(0);
    if ctx.cumulative_pass_nr() < last + 3 {
        return;
    }
    PREVIEWS_LOADED.store(false, Ordering::Relaxed);
    let any = ctx.fonts(|f| f.definitions().families.keys().any(|k| matches!(k, egui::FontFamily::Name(n) if n.starts_with(PREVIEW))));
    if !any {
        return;
    }
    let mut defs = ctx.fonts(|f| f.definitions().clone());
    defs.font_data.retain(|k, _| !k.starts_with(PREVIEW));
    defs.families.retain(|k, _| !matches!(k, egui::FontFamily::Name(n) if n.starts_with(PREVIEW)));
    ctx.set_fonts(defs);
}

fn scan() -> Vec<Family> {
    let mut by_name: BTreeMap<String, Vec<Face>> = BTreeMap::new();
    let mut files = Vec::new();
    for dir in font_dirs() {
        collect(&dir, &mut files, 0);
    }
    for path in files {
        for (name, face) in read_faces(&path).unwrap_or_default() {
            by_name.entry(name).or_default().push(face);
        }
    }
    let mut list: Vec<Family> = by_name.into_iter().map(|(name, faces)| Family { name, faces }).collect();
    list.sort_by_key(|f| f.name.to_lowercase());
    list
}

fn font_dirs() -> Vec<PathBuf> {
    let home = |rest: &str| std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|h| PathBuf::from(h).join(rest));
    let mut dirs: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        dirs.push(std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from).join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        dirs.extend(home("Library/Fonts"));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        dirs.extend(home(".local/share/fonts"));
        dirs.extend(home(".fonts"));
    }
    let _ = home;
    dirs
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                collect(&path, out, depth + 1);
            }
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            out.push(path);
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn read_range(file: &mut std::fs::File, offset: u64, len: usize) -> Option<Vec<u8>> {
    // Uma tabela de nomes passa de alguns KB só em fontes estranhas.
    let len = len.min(512 * 1024);
    let mut buf = vec![0; len];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Os nomes e as faces de um arquivo (várias numa coleção `.ttc`).
fn read_faces(path: &Path) -> Option<Vec<(String, Face)>> {
    let mut file = std::fs::File::open(path).ok()?;
    let head = read_range(&mut file, 0, 12)?;
    let offsets: Vec<u32> = if &head[0..4] == b"ttcf" {
        let count = u32_at(&head, 8)?.min(64) as usize;
        let list = read_range(&mut file, 12, count * 4)?;
        (0..count).filter_map(|i| u32_at(&list, i * 4)).collect()
    } else {
        vec![0]
    };
    let mut out = Vec::new();
    for (index, offset) in offsets.into_iter().enumerate() {
        if let Some((name, weight, italic)) = read_face(&mut file, offset as u64) {
            out.push((name, Face { path: path.to_owned(), index: index as u32, weight, italic }));
        }
    }
    Some(out)
}

/// Nome da família, peso e itálico de uma face, pelo índice das tabelas.
fn read_face(file: &mut std::fs::File, offset: u64) -> Option<(String, u16, bool)> {
    let dir = read_range(file, offset, 12)?;
    let tables = u16_at(&dir, 4)?.min(64) as usize;
    let records = read_range(file, offset + 12, tables * 16)?;
    let (mut name_table, mut os2) = (None, None);
    for i in 0..tables {
        let r = i * 16;
        let tag = records.get(r..r + 4)?;
        let (at, len) = (u32_at(&records, r + 8)? as u64, u32_at(&records, r + 12)? as usize);
        match tag {
            b"name" => name_table = Some((at, len)),
            b"OS/2" => os2 = Some((at, len)),
            _ => {}
        }
    }
    let (at, len) = name_table?;
    let names = read_range(file, at, len)?;
    let name = family_name(&names)?;
    // O peso (usWeightClass) e o itálico (bit 0 do fsSelection).
    let (weight, italic) = match os2.and_then(|(at, _)| read_range(file, at, 64)) {
        Some(t) => (u16_at(&t, 4).unwrap_or(400), u16_at(&t, 62).is_some_and(|s| s & 1 == 1)),
        None => (400, false),
    };
    Some((name, weight, italic))
}

/// O nome da família: o tipográfico (id 16) antes do comum (id 1), em
/// inglês do Windows antes de qualquer outro.
fn family_name(t: &[u8]) -> Option<String> {
    let count = u16_at(t, 2)? as usize;
    let strings = u16_at(t, 4)? as usize;
    let mut best: Option<(u32, String)> = None;
    for i in 0..count {
        let r = 6 + i * 12;
        let (platform, encoding, language, id) = (u16_at(t, r)?, u16_at(t, r + 2)?, u16_at(t, r + 4)?, u16_at(t, r + 6)?);
        let (len, off) = (u16_at(t, r + 8)? as usize, u16_at(t, r + 10)? as usize);
        if id != 1 && id != 16 {
            continue;
        }
        let raw = t.get(strings + off..strings + off + len)?;
        let text = match (platform, encoding) {
            (0, _) | (3, 1) | (3, 10) => {
                let units: Vec<u16> = raw.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                String::from_utf16_lossy(&units)
            }
            (1, 0) => raw.iter().map(|&b| b as char).collect(),
            _ => continue,
        };
        let text = text.trim().to_owned();
        if text.is_empty() || text.starts_with('.') {
            continue;
        }
        // Quanto menor, melhor: o tipográfico, o do Windows, o inglês.
        let rank = u32::from(id != 16) * 4 + u32::from(platform != 3) * 2 + u32::from(platform == 3 && language != 0x0409);
        if best.as_ref().is_none_or(|(b, _)| rank < *b) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_as_fontes_do_sistema() {
        let list = scan();
        println!("{} famílias", list.len());
        // Todo sistema com interface tem alguma fonte.
        assert!(!list.is_empty());
        assert!(list.iter().all(|f| !f.name.is_empty() && !f.faces.is_empty()));
        #[cfg(windows)]
        {
            let arial = list.iter().find(|f| f.name == "Arial").expect("Arial no Windows");
            let bold = arial.best(700).unwrap();
            assert!(bold.weight >= 600, "{bold:?}");
        }
    }
}

