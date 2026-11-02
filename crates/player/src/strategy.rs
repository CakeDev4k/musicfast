//! Qual jeito de pedir o áudio ao YouTube tentar primeiro.
//!
//! Cada jeito (`Way`) é um cliente do yt-dlp, com ou sem a conta, por IPv4
//! ou IPv6. Antes a ordem era fixa no código: quando o YouTube mudava um
//! cliente (ele muda a cada poucas semanas) ou passava a pedir o "não é um
//! robô" a uma rede, toda música pagava as tentativas que iam falhar antes
//! da que funcionava, e só uma versão nova do app mudava isso.
//!
//! Aqui a ordem se ajusta sozinha, pelo que aconteceu nesta máquina:
//! - o jeito que funcionou por último vai na frente;
//! - o que falhou fica de lado por um tempo que cresce a cada falha seguida
//!   (`BENCH_STEP`), e volta a ser tentado depois, sem ninguém mexer;
//! - a conta só vai na frente quando entrega o áudio do Premium (o cliente
//!   de TV leva 6 s para resolver e o endereço dele só vale 4 a 6 s depois):
//!   sem Premium ela é a reserva, para o "não é um robô" e as músicas com
//!   restrição de idade.
//!
//! O que se aprendeu fica num JSON pequeno, para valer na próxima abertura.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Quanto tempo o vencedor da última música fica na frente.
const WINNER_TTL: u64 = 6 * 3600;
/// De lado por isto vezes as falhas seguidas (até `MAX_BENCH_STEPS`).
const BENCH_STEP: u64 = 15 * 60;
const MAX_BENCH_STEPS: u64 = 8;
/// De quanto em quanto tempo a conta sem Premium é testada de novo na
/// frente (a pessoa pode ter assinado).
const ACCOUNT_PROBE: u64 = 7 * 24 * 3600;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Way {
    /// `visionos`: 2,6–3,0 s, o endereço vale na hora.
    Fast,
    /// O padrão do yt-dlp: os mantenedores dele o trocam quando o YouTube
    /// muda, então é a reserva que se conserta sozinha com a atualização.
    Default,
    /// `web_embedded`: sem PO token e sem cookies; passa por muitos "não é
    /// um robô". Só vídeos que aceitam ser embutidos.
    Embed,
    /// `visionos` por IPv6: quando o IPv4 da casa é dividido com a
    /// vizinhança (CGNAT) e o YouTube marcou o endereço inteiro.
    Ipv6,
    /// Com a conta, pelo cliente de TV (o Premium dá o AAC de 256 kbps).
    Account,
    /// Com a conta, pelos clientes que o yt-dlp escolhe para cookies: a
    /// reserva se o de TV mudar.
    AccountDefault,
}

impl Way {
    /// A ordem de partida, antes de aprender qualquer coisa.
    pub const ALL: [Way; 6] = [Way::Fast, Way::Default, Way::Embed, Way::Ipv6, Way::Account, Way::AccountDefault];

    pub fn uses_account(self) -> bool {
        matches!(self, Way::Account | Way::AccountDefault)
    }

    /// A chave no JSON (não mudar: é o que já está gravado nas máquinas).
    pub fn key(self) -> &'static str {
        match self {
            Way::Fast => "visionos",
            Way::Default => "padrao",
            Way::Embed => "embutido",
            Way::Ipv6 => "ipv6",
            Way::Account => "conta",
            Way::AccountDefault => "conta-padrao",
        }
    }

    fn from_key(key: &str) -> Option<Way> {
        Way::ALL.into_iter().find(|w| w.key() == key)
    }
}

/// O que se sabe de um jeito. Segundos Unix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub ok_at: u64,
    #[serde(default)]
    pub fail_at: u64,
    /// Falhas seguidas (zera no primeiro acerto).
    #[serde(default)]
    pub fails: u32,
    /// Média móvel do tempo de resolução, em ms.
    #[serde(default)]
    pub ms: u32,
    /// Com a conta: o último acerto veio no formato do Premium (141).
    #[serde(default)]
    pub premium: bool,
}

impl Record {
    fn benched(&self, now: u64) -> bool {
        self.fails > 0 && now < self.fail_at + BENCH_STEP * u64::from(self.fails).min(MAX_BENCH_STEPS)
    }
}

/// A ordem de tentar, pelo que se sabe. Pura: os testes a exercitam.
pub fn order(records: &HashMap<Way, Record>, account: bool, now: u64) -> Vec<Way> {
    let rec = |w: Way| records.get(&w).copied().unwrap_or_default();
    // O vencedor é só entre os anônimos: a conta sem Premium que acertou
    // porque os outros falharam sobe sozinha quando eles ficam de lado.
    let winner = Way::ALL
        .into_iter()
        .filter(|w| !w.uses_account())
        .map(|w| (w, rec(w)))
        .filter(|(_, r)| r.ok_at > 0 && now < r.ok_at + WINNER_TTL && !r.benched(now))
        .max_by_key(|(_, r)| r.ok_at)
        .map(|(w, _)| w);
    let account_first = account && {
        let accounts = [rec(Way::Account), rec(Way::AccountDefault)];
        let premium = accounts.iter().any(|r| r.premium && now < r.ok_at + ACCOUNT_PROBE);
        let last_try = accounts.iter().map(|r| r.ok_at.max(r.fail_at)).max().unwrap_or(0);
        premium || last_try == 0 || now >= last_try + ACCOUNT_PROBE
    };
    let mut ways: Vec<Way> = Way::ALL.into_iter().filter(|w| account || !w.uses_account()).collect();
    ways.sort_by_key(|&w| {
        let r = rec(w);
        let benched = r.benched(now);
        let pinned = account_first && w.uses_account() && !benched;
        let base = Way::ALL.iter().position(|x| *x == w).unwrap_or(usize::MAX);
        (!pinned, benched, Some(w) != winner, base)
    });
    ways
}

/// A memória do processo inteiro (o player e os downloads a dividem).
pub struct Memory {
    records: HashMap<Way, Record>,
    file: Option<PathBuf>,
}

fn memory() -> &'static Mutex<Memory> {
    static MEMORY: OnceLock<Mutex<Memory>> = OnceLock::new();
    MEMORY.get_or_init(|| Mutex::new(Memory { records: HashMap::new(), file: None }))
}

fn lock() -> std::sync::MutexGuard<'static, Memory> {
    memory().lock().unwrap_or_else(|e| e.into_inner())
}

/// Lê o que já se aprendeu e passa a gravar em `file`. Chamado ao abrir o
/// player; sem isto a memória vale só até fechar o app.
pub fn remember_in(file: PathBuf) {
    let records: HashMap<Way, Record> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|text| serde_json::from_str::<HashMap<String, Record>>(&text).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, r)| Way::from_key(&k).map(|w| (w, r)))
        .collect();
    let mut m = lock();
    m.records = records;
    m.file = Some(file);
}

pub fn current_order(account: bool, now: u64) -> Vec<Way> {
    order(&lock().records, account, now)
}

pub fn success(way: Way, ms: u64, premium: bool, now: u64) {
    let mut m = lock();
    let r = m.records.entry(way).or_default();
    let ms = ms.min(u64::from(u32::MAX)) as u32;
    r.ms = if r.ms == 0 { ms } else { (r.ms * 3 + ms) / 4 };
    r.ok_at = now;
    r.fails = 0;
    if way.uses_account() {
        r.premium = premium;
    }
    m.save();
}

pub fn failure(way: Way, now: u64) {
    let mut m = lock();
    let r = m.records.entry(way).or_default();
    r.fails = r.fails.saturating_add(1);
    r.fail_at = now;
    m.save();
}

impl Memory {
    /// Grava na hora (é um JSON de poucas linhas), pelo arquivo temporário:
    /// uma queda no meio não deixa o arquivo pela metade.
    fn save(&self) {
        let Some(file) = &self.file else { return };
        let map: HashMap<&str, &Record> = self.records.iter().map(|(w, r)| (w.key(), r)).collect();
        let Ok(text) = serde_json::to_string(&map) else { return };
        let tmp = file.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn ok(at: u64) -> Record {
        Record { ok_at: at, ..Record::default() }
    }

    fn failed(fails: u32, at: u64) -> Record {
        Record { fails, fail_at: at, ..Record::default() }
    }

    #[test]
    fn nothing_known_keeps_the_starting_order_and_probes_the_account() {
        let none = HashMap::new();
        assert_eq!(order(&none, false, NOW), [Way::Fast, Way::Default, Way::Embed, Way::Ipv6]);
        // A conta nunca testada vai na frente uma vez, para saber se é Premium.
        assert_eq!(order(&none, true, NOW)[..2], [Way::Account, Way::AccountDefault]);
    }

    #[test]
    fn the_last_winner_goes_first_and_failures_are_benched_for_a_while() {
        let mut r = HashMap::new();
        r.insert(Way::Fast, failed(1, NOW - 60));
        r.insert(Way::Embed, ok(NOW - 30));
        assert_eq!(order(&r, false, NOW), [Way::Embed, Way::Default, Way::Ipv6, Way::Fast]);
        // Passado o castigo, o rápido volta a ser tentado (depois do vencedor).
        assert_eq!(order(&r, false, NOW + BENCH_STEP)[..2], [Way::Embed, Way::Fast]);
        // O vencedor vence de validade.
        assert_eq!(order(&r, false, NOW + WINNER_TTL + BENCH_STEP)[0], Way::Fast);
    }

    #[test]
    fn repeated_failures_stay_benched_longer() {
        let mut r = HashMap::new();
        r.insert(Way::Fast, failed(4, NOW));
        assert_eq!(*order(&r, false, NOW + 3 * BENCH_STEP).last().unwrap(), Way::Fast);
        assert_eq!(order(&r, false, NOW + 4 * BENCH_STEP)[0], Way::Fast);
        // O castigo tem teto: um jeito que falhou mil vezes ainda volta.
        r.insert(Way::Fast, failed(1000, NOW));
        assert_eq!(order(&r, false, NOW + MAX_BENCH_STEPS * BENCH_STEP)[0], Way::Fast);
    }

    #[test]
    fn the_account_leads_only_with_premium() {
        let mut r = HashMap::new();
        r.insert(Way::Account, Record { ok_at: NOW - 60, premium: true, ..Record::default() });
        assert_eq!(order(&r, true, NOW)[0], Way::Account);
        // Sem Premium: reserva, atrás dos anônimos.
        r.insert(Way::Account, Record { ok_at: NOW - 60, premium: false, ..Record::default() });
        assert_eq!(order(&r, true, NOW), [Way::Fast, Way::Default, Way::Embed, Way::Ipv6, Way::Account, Way::AccountDefault]);
        // Uma semana depois, testa de novo na frente.
        assert_eq!(order(&r, true, NOW + ACCOUNT_PROBE)[0], Way::Account);
    }

    #[test]
    fn with_every_anonymous_way_benched_the_account_rises() {
        let mut r = HashMap::new();
        for w in [Way::Fast, Way::Default, Way::Embed, Way::Ipv6] {
            r.insert(w, failed(1, NOW - 10));
        }
        r.insert(Way::Account, ok(NOW - 5));
        assert_eq!(order(&r, true, NOW)[0], Way::Account);
    }

    #[test]
    fn records_survive_a_round_trip_through_the_file() {
        let dir = std::env::temp_dir().join(format!("musicfast-teste-estrategias-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("estrategias.json");
        std::fs::write(&file, r#"{"embutido":{"ok_at":5,"ms":4700},"desconhecido":{"ok_at":9}}"#).unwrap();
        remember_in(file.clone());
        assert_eq!(lock().records.get(&Way::Embed).map(|r| r.ms), Some(4700));
        success(Way::Fast, 3000, false, NOW);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"visionos\"") && text.contains("\"embutido\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
