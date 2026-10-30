//! Os textos da interface e o idioma em que ela fala.
//!
//! Cada idioma é um `Strings` inteiro (`pt.rs`, `en.rs`, `es.rs`): esquecer
//! um texto num deles não compila. O idioma vale para o processo todo
//! (`set`) e `t()` devolve os textos dele; a interface chama `t()` a cada
//! quadro, então trocar o idioma vale já no quadro seguinte.
//!
//! O idioma automático é o de exibição do sistema (`system`): no Windows o
//! que está em Configurações → Hora e idioma, no Linux o `LANG`, no macOS o
//! primeiro de Idiomas Preferidos. Fora dos três, inglês.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod en;
mod es;
mod pt;
mod strings;

pub use strings::Strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Pt,
    En,
    Es,
}

impl Lang {
    pub const ALL: [Self; 3] = [Self::Pt, Self::En, Self::Es];

    /// O código guardado no estado e mandado ao YouTube (`hl`).
    pub fn code(self) -> &'static str {
        match self {
            Self::Pt => "pt",
            Self::En => "en",
            Self::Es => "es",
        }
    }

    /// O nome no próprio idioma, como nos seletores de idioma.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::Pt => "Português",
            Self::En => "English",
            Self::Es => "Español",
        }
    }

    /// O idioma de uma etiqueta qualquer: "pt", "pt-BR", "pt_BR.UTF-8",
    /// "es-419", "en_US". `None` para os que o app não fala.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "pt" => Some(Self::Pt),
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            _ => None,
        }
    }

    /// O país das listas do YouTube quando o sistema não diz o seu.
    fn default_region(self) -> &'static str {
        match self {
            Self::Pt => "BR",
            Self::En => "US",
            Self::Es => "ES",
        }
    }

    fn strings(self) -> &'static Strings {
        match self {
            Self::Pt => &pt::STRINGS,
            Self::En => &en::STRINGS,
            Self::Es => &es::STRINGS,
        }
    }

    fn index(self) -> u8 {
        match self {
            Self::Pt => 0,
            Self::En => 1,
            Self::Es => 2,
        }
    }
}

/// Português até alguém dizer outro: os testes do catálogo esperam os
/// títulos de reserva nele.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Es,
        _ => Lang::Pt,
    }
}

/// Os textos do idioma atual.
pub fn t() -> &'static Strings {
    current().strings()
}

/// O idioma e o país das listas do YouTube (`hl` e `gl`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    pub lang: Lang,
    /// Duas letras maiúsculas ("BR").
    pub region: String,
}

impl Locale {
    /// A etiqueta para páginas web ("pt-BR", "en-US").
    pub fn tag(&self) -> String {
        format!("{}-{}", self.lang.code(), self.region)
    }
}

/// A variável que fixa o idioma por fora (e o que a janela de login, um
/// processo à parte, recebe do app).
pub const ENV: &str = "MUSICFAST_LANG";

/// O idioma do sistema, lido uma vez por processo.
pub fn system() -> &'static Locale {
    static SYSTEM: OnceLock<Locale> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let (tags, region) = os::preferred();
        let forced = std::env::var(ENV).ok().and_then(|v| Lang::from_tag(&v));
        let lang = forced.or_else(|| tags.iter().find_map(|t| Lang::from_tag(t))).unwrap_or(Lang::En);
        let region = region
            .or_else(|| tags.iter().find(|t| Lang::from_tag(t) == Some(lang)).and_then(|t| region_of(t)))
            .unwrap_or_else(|| lang.default_region().to_owned());
        Locale { lang, region }
    })
}

/// O idioma escolhido (`None`: o do sistema), já em vigor. O país continua
/// o do sistema: quem mora no Brasil e escolhe inglês segue vendo as
/// paradas do Brasil.
pub fn apply(choice: Option<Lang>) -> Locale {
    let system = system();
    let locale = match choice {
        Some(lang) => Locale { lang, region: system.region.clone() },
        None => system.clone(),
    };
    set(locale.lang);
    locale
}

/// O país de uma etiqueta ("pt-BR" → "BR", "pt_BR.UTF-8" → "BR"); os
/// códigos numéricos ("es-419") não servem ao YouTube.
fn region_of(tag: &str) -> Option<String> {
    let tag = tag.split(['.', '@']).next()?;
    tag.split(['-', '_']).skip(1).find(|p| is_region(p)).map(str::to_ascii_uppercase)
}

fn is_region(s: &str) -> bool {
    s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic())
}

#[cfg(windows)]
mod os {
    use windows_sys::Win32::Globalization::{GetUserDefaultGeoName, GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};

    /// Os idiomas de exibição do Windows, na ordem de preferência, e o país
    /// de Configurações → Hora e idioma → Região.
    pub fn preferred() -> (Vec<String>, Option<String>) {
        (ui_languages(), geo_name())
    }

    fn ui_languages() -> Vec<String> {
        let (mut count, mut len) = (0u32, 0u32);
        // SAFETY: a primeira chamada só mede (buffer nulo); a segunda escreve
        // no máximo `len` caracteres no buffer desse tamanho.
        let buf = unsafe {
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut len) == 0 || len == 0 {
                return Vec::new();
            }
            let mut buf = vec![0u16; len as usize];
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buf.as_mut_ptr(), &mut len) == 0 {
                return Vec::new();
            }
            buf.truncate(len as usize);
            buf
        };
        // Vem "pt-BR\0en-US\0\0".
        buf.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
    }

    fn geo_name() -> Option<String> {
        let mut buf = [0u16; 16];
        // SAFETY: o tamanho passado é o do buffer.
        let n = unsafe { GetUserDefaultGeoName(buf.as_mut_ptr(), buf.len() as i32) };
        let name = String::from_utf16_lossy(buf.get(..(n.max(1) as usize - 1))?);
        super::is_region(&name).then(|| name.to_ascii_uppercase())
    }
}

#[cfg(not(windows))]
mod os {
    /// `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` e `LANG`, nessa ordem (a do
    /// gettext). No macOS, aberto pelo Finder sem nenhum deles, os Idiomas
    /// Preferidos do sistema.
    pub fn preferred() -> (Vec<String>, Option<String>) {
        let mut tags: Vec<String> = Vec::new();
        if let Ok(list) = std::env::var("LANGUAGE") {
            tags.extend(list.split(':').filter(|s| !s.is_empty()).map(str::to_owned));
        }
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(v) = std::env::var(var)
                && !v.is_empty()
                && v != "C"
                && v != "POSIX"
                && !v.starts_with("C.")
            {
                tags.push(v);
            }
        }
        #[cfg(target_os = "macos")]
        if tags.is_empty() {
            tags = apple_languages();
        }
        (tags, None)
    }

    /// `defaults read -g AppleLanguages` responde `( "pt-BR", "en-US" )`.
    #[cfg(target_os = "macos")]
    fn apple_languages() -> Vec<String> {
        let Ok(out) = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&out.stdout)
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_read_like_the_systems_write_them() {
        assert_eq!(Lang::from_tag("pt-BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_tag("pt_PT.UTF-8"), Some(Lang::Pt));
        assert_eq!(Lang::from_tag("es-419"), Some(Lang::Es));
        assert_eq!(Lang::from_tag("EN"), Some(Lang::En));
        assert_eq!(Lang::from_tag("fr-FR"), None);
        assert_eq!(region_of("pt_BR.UTF-8"), Some("BR".to_owned()));
        assert_eq!(region_of("es-419"), None);
        assert_eq!(region_of("zh-Hant-TW"), Some("TW".to_owned()));
        assert_eq!(region_of("en"), None);
    }

    #[test]
    fn the_system_always_gives_a_region() {
        let locale = system();
        eprintln!("idioma do sistema: {locale:?}");
        assert!(is_region(&locale.region), "{locale:?}");
    }

    #[test]
    fn every_language_formats_its_texts() {
        for lang in Lang::ALL {
            let s = lang.strings();
            assert!(s.queue_count(1).contains('1'));
            assert!(s.saved_in("X").contains('X'));
            assert!(!s.cookie_steps.iter().any(|step| step.is_empty()));
        }
    }
}
