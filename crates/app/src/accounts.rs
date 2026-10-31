//! As contas guardadas neste computador. Cada uma tem um lugar (`Profile::id`)
//! com a própria sessão cifrada e o próprio perfil do login (`Paths::session_of`,
//! `Paths::webview_of`): dividir o perfil faria "adicionar conta" voltar para
//! a conta já aberta, e a renovação silenciosa trazer a conta errada. O id
//! vazio é o lugar de antes das várias contas (`conta.bin`, `webview`), que a
//! conta já logada continua usando.

use musicfast_ytm::{Account, Thumbs};
use serde::{Deserialize, Serialize};

/// O que se mostra de uma conta sem precisar da rede. Nada aqui é segredo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub photo: Thumbs,
}

impl Profile {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_owned(), ..Self::default() }
    }

    pub fn update(&mut self, account: &Account) {
        self.name.clone_from(&account.name);
        self.handle.clone_from(&account.handle);
        self.photo = account.photo.clone();
    }

    /// A conta como a tela a mostra, antes de a conferência responder.
    pub fn account(&self) -> Option<Account> {
        (!self.name.is_empty()).then(|| Account { name: self.name.clone(), handle: self.handle.clone(), photo: self.photo.clone() })
    }

    /// A mesma pessoa entrou de novo em outro lugar. Só pelo @: dois nomes
    /// iguais podem ser contas diferentes.
    pub fn same_as(&self, account: &Account) -> bool {
        !account.handle.is_empty() && self.handle == account.handle
    }
}

/// Vira nome de arquivo e de pasta, e vem do `estado.json`: só letras e
/// números.
pub fn valid_id(id: &str) -> bool {
    id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Um lugar novo.
pub fn new_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}")
}

/// Onde a próxima conta entra: o lugar antigo, se ninguém o usa.
pub fn free_id(accounts: &[Profile]) -> String {
    if accounts.iter().any(|p| p.id.is_empty()) { new_id() } else { String::new() }
}

/// As contas lidas do arquivo, sem as que não servem nem repetidas.
pub fn sane(mut accounts: Vec<Profile>) -> Vec<Profile> {
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|p| valid_id(&p.id) && seen.insert(p.id.clone()));
    accounts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(name: &str, handle: &str) -> Account {
        Account { name: name.to_owned(), handle: handle.to_owned(), photo: Thumbs::default() }
    }

    #[test]
    fn a_primeira_conta_usa_o_lugar_antigo_e_as_outras_um_novo() {
        assert_eq!(free_id(&[]), "");
        let id = free_id(&[Profile::new("")]);
        assert!(!id.is_empty() && valid_id(&id));
    }

    #[test]
    fn id_que_escaparia_da_pasta_nao_passa() {
        let list = vec![Profile::new("../x"), Profile::new("a1"), Profile::new("a1"), Profile::new("")];
        let ids: Vec<String> = sane(list).into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["a1", ""]);
    }

    #[test]
    fn a_mesma_pessoa_e_reconhecida_pelo_arroba() {
        let mut profile = Profile::new("a1");
        profile.update(&account("Ana", "@ana"));
        assert!(profile.same_as(&account("Ana Maria", "@ana")));
        assert!(!profile.same_as(&account("Ana", "@outra")));
        let mut sem_arroba = Profile::new("b");
        sem_arroba.update(&account("Ana", ""));
        assert!(!sem_arroba.same_as(&account("Ana", "")));
    }

    #[test]
    fn a_conta_sem_nome_ainda_nao_aparece() {
        assert_eq!(Profile::new("a").account(), None);
    }
}
