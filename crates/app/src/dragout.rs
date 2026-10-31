//! Arrastar uma música baixada para fora do app: soltar no Premiere importa,
//! no Explorer copia, no WhatsApp ou no Discord envia.
//!
//! No Windows é o arrasto de arquivo do sistema (OLE): o objeto de dados
//! vem pronto do próprio Explorer (`SHCreateDataObject` sobre o item do
//! arquivo), com todos os formatos que ele mesmo oferece ao arrastar um
//! arquivo (`CF_HDROP`, a lista de itens do shell, o nome), e
//! `SHDoDragDrop` desenha o ícone do arquivo sob o mouse. Daqui só sai a
//! fonte do arrasto (`IDropSource`), que diz quando soltar e quando desistir.
//!
//! `SHDoDragDrop` roda o próprio laço de mensagens até soltar: a janela não
//! desenha nesse meio tempo (o som continua, na thread do áudio). O laço
//! consome o soltar do botão, então o app avisa o egui depois
//! (`MusicApp::raw_input_hook`), ou ele acharia que o botão segue apertado.

use std::path::Path;

/// Como terminou o arrasto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Dropped,
    Cancelled,
    /// Este sistema não arrasta arquivo para fora (por enquanto, só o
    /// Windows): quem chama mostra o arquivo na pasta.
    #[cfg_attr(windows, allow(dead_code))]
    Unsupported,
}

/// Começa o arrasto de `path` e só volta quando ele termina. Chamar com o
/// botão esquerdo ainda apertado (no `drag_started` do egui).
pub fn start(window: Option<isize>, path: &Path) -> Result<Outcome, String> {
    #[cfg(windows)]
    {
        win::drag(window.unwrap_or(0), path)
    }
    #[cfg(not(windows))]
    {
        let _ = (window, path);
        Ok(Outcome::Unsupported)
    }
}

/// Abre a pasta com o arquivo selecionado.
pub fn reveal(path: &Path) {
    #[cfg(windows)]
    {
        if win::reveal(path).is_ok() {
            return;
        }
        let _ = std::process::Command::new("explorer").arg(path.parent().unwrap_or(path)).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
    }
}

/// Abre uma pasta no gerenciador de arquivos.
pub fn open_folder(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    let _ = std::process::Command::new(program).arg(dir).spawn();
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows_sys::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_NOINTERFACE, S_OK};
    use windows_sys::Win32::System::Ole::{DROPEFFECT_COPY, OleInitialize};
    use windows_sys::Win32::System::SystemServices::MK_LBUTTON;
    use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows_sys::Win32::UI::Shell::{ILClone, ILCreateFromPathW, ILFindLastID, ILFree, ILRemoveLastID, SHCreateDataObject, SHDoDragDrop, SHOpenFolderAndSelectItems};
    use windows_sys::core::{BOOL, GUID, HRESULT, IID_IUnknown};

    use super::Outcome;

    const IID_IDROPSOURCE: GUID = GUID::from_u128(0x00000121_0000_0000_c000_000000000046);
    const IID_IDATAOBJECT: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);

    /// O `IDropSource`: a tabela de métodos do COM e a contagem de
    /// referências, na ordem que o COM espera.
    #[repr(C)]
    struct DropSource {
        vtbl: *const Vtbl,
        refs: AtomicU32,
    }

    #[repr(C)]
    struct Vtbl {
        query_interface: unsafe extern "system" fn(*mut DropSource, *const GUID, *mut *mut c_void) -> HRESULT,
        add_ref: unsafe extern "system" fn(*mut DropSource) -> u32,
        release: unsafe extern "system" fn(*mut DropSource) -> u32,
        query_continue_drag: unsafe extern "system" fn(*mut DropSource, BOOL, u32) -> HRESULT,
        give_feedback: unsafe extern "system" fn(*mut DropSource, u32) -> HRESULT,
    }

    static VTBL: Vtbl = Vtbl { query_interface, add_ref, release, query_continue_drag, give_feedback };

    unsafe extern "system" fn query_interface(this: *mut DropSource, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
        // SAFETY: o COM passa ponteiros válidos (ou nulos, conferidos).
        unsafe {
            if out.is_null() || iid.is_null() {
                return E_NOINTERFACE;
            }
            let iid = &*iid;
            if same(iid, &IID_IUnknown) || same(iid, &IID_IDROPSOURCE) {
                add_ref(this);
                *out = this.cast();
                S_OK
            } else {
                *out = std::ptr::null_mut();
                E_NOINTERFACE
            }
        }
    }

    unsafe extern "system" fn add_ref(this: *mut DropSource) -> u32 {
        // SAFETY: `this` é o objeto vivo (o COM só chama com referência).
        unsafe { (*this).refs.fetch_add(1, Ordering::AcqRel) + 1 }
    }

    unsafe extern "system" fn release(this: *mut DropSource) -> u32 {
        // SAFETY: idem; na última referência o objeto volta ao `Box`.
        unsafe {
            let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
            if left == 0 {
                drop(Box::from_raw(this));
            }
            left
        }
    }

    unsafe extern "system" fn query_continue_drag(_: *mut DropSource, escape: BOOL, keys: u32) -> HRESULT {
        if escape != 0 {
            DRAGDROP_S_CANCEL
        } else if keys & MK_LBUTTON == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    unsafe extern "system" fn give_feedback(_: *mut DropSource, _: u32) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }

    fn same(a: &GUID, b: &GUID) -> bool {
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    }

    /// O `Release` de um objeto COM qualquer (o terceiro método de todos).
    unsafe fn release_com(object: *mut c_void) {
        // SAFETY: `object` é um ponteiro de interface COM válido.
        unsafe {
            let vtbl = *(object as *const *const [usize; 3]);
            let release: unsafe extern "system" fn(*mut c_void) -> u32 = std::mem::transmute((*vtbl)[2]);
            release(object);
        }
    }

    /// A lista de itens do shell do arquivo, liberada ao sair do escopo.
    struct Pidl(*mut ITEMIDLIST);

    impl Pidl {
        fn of(path: &Path) -> Result<Self, String> {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: `wide` termina em zero e vive durante a chamada.
            let pidl = unsafe { ILCreateFromPathW(wide.as_ptr()) };
            if pidl.is_null() { Err(format!("o Windows não achou {}", path.display())) } else { Ok(Self(pidl)) }
        }
    }

    impl Drop for Pidl {
        fn drop(&mut self) {
            // SAFETY: veio do shell (`ILCreateFromPathW`/`ILClone`).
            unsafe { ILFree(self.0) };
        }
    }

    fn ole() {
        // A janela do winit já inicia o OLE (o soltar arquivos nela); de
        // novo só soma uma referência. SAFETY: sem argumentos.
        unsafe { OleInitialize(std::ptr::null()) };
    }

    /// O `IDataObject` do Explorer para o arquivo (quem chama dá o
    /// `Release`).
    fn data_object(path: &Path) -> Result<*mut c_void, String> {
        ole();
        let folder = Pidl::of(path)?;
        // SAFETY: os ponteiros são do shell e vivem até o fim da função; o
        // filho é copiado antes de cortar o último item da pasta.
        unsafe {
            let child = Pidl(ILClone(ILFindLastID(folder.0)));
            if child.0.is_null() {
                return Err("sem memória para o arrasto".into());
            }
            ILRemoveLastID(folder.0);
            let mut data: *mut c_void = std::ptr::null_mut();
            let items = [child.0 as *const ITEMIDLIST];
            let hr = SHCreateDataObject(folder.0, 1, items.as_ptr(), std::ptr::null_mut(), &IID_IDATAOBJECT, &mut data);
            if hr < 0 || data.is_null() {
                return Err(format!("SHCreateDataObject: 0x{hr:08X}"));
            }
            Ok(data)
        }
    }

    pub fn drag(hwnd: isize, path: &Path) -> Result<Outcome, String> {
        let data = data_object(path)?;
        // SAFETY: `data` é um `IDataObject` vivo; a fonte é nossa.
        unsafe {
            let source = Box::into_raw(Box::new(DropSource { vtbl: &VTBL, refs: AtomicU32::new(1) }));
            let mut effect = 0;
            let hr = SHDoDragDrop(hwnd as _, data, source.cast(), DROPEFFECT_COPY, &mut effect);
            release(source);
            release_com(data);
            match hr {
                DRAGDROP_S_DROP => Ok(Outcome::Dropped),
                DRAGDROP_S_CANCEL => Ok(Outcome::Cancelled),
                hr if hr >= 0 => Ok(Outcome::Cancelled),
                hr => Err(format!("SHDoDragDrop: 0x{hr:08X}")),
            }
        }
    }

    pub fn reveal(path: &Path) -> Result<(), String> {
        ole();
        let item = Pidl::of(path)?;
        // SAFETY: a lista é válida; sem filhos, o próprio item é selecionado.
        let hr = unsafe { SHOpenFolderAndSelectItems(item.0, 0, std::ptr::null(), 0) };
        if hr < 0 { Err(format!("0x{hr:08X}")) } else { Ok(()) }
    }

    #[cfg(test)]
    mod tests {
        use windows_sys::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
        use windows_sys::Win32::System::Ole::{CF_HDROP, ReleaseStgMedium};
        use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

        use super::*;

        /// O que o Premiere, o Explorer e os chats leem ao soltar: o
        /// `CF_HDROP` com o caminho do arquivo.
        #[test]
        fn the_drag_carries_the_file_as_hdrop() {
            let dir = std::env::temp_dir().join(format!("musicfast-arrasto-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("Artista - Música (2).m4a");
            std::fs::write(&path, b"teste").unwrap();
            let data = data_object(&path).expect("objeto de dados");
            // SAFETY: `GetData` é o quarto método do `IDataObject`.
            let dropped = unsafe {
                let vtbl = *(data as *const *const [usize; 4]);
                let get_data: unsafe extern "system" fn(*mut c_void, *const FORMATETC, *mut STGMEDIUM) -> HRESULT = std::mem::transmute((*vtbl)[3]);
                let format = FORMATETC { cfFormat: CF_HDROP, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT, lindex: -1, tymed: TYMED_HGLOBAL as u32 };
                let mut medium: STGMEDIUM = std::mem::zeroed();
                let hr = get_data(data, &format, &mut medium);
                assert!(hr >= 0, "GetData: 0x{hr:08X}");
                let hdrop = medium.u.hGlobal as HDROP;
                assert_eq!(DragQueryFileW(hdrop, u32::MAX, std::ptr::null_mut(), 0), 1);
                let mut buf = [0u16; 1024];
                let n = DragQueryFileW(hdrop, 0, buf.as_mut_ptr(), buf.len() as u32) as usize;
                ReleaseStgMedium(&mut medium);
                release_com(data);
                String::from_utf16_lossy(&buf[..n])
            };
            // A pasta temporária pode vir no nome curto (`RUNNER~1`) e o
            // arrasto, no longo: vale ser o mesmo arquivo.
            assert_eq!(std::fs::canonicalize(dropped).unwrap(), std::fs::canonicalize(&path).unwrap());
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
