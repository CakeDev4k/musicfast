//! Quais botões a barra do player mostra, e onde. Só os dados: quem desenha é
//! `ui::bar`, quem edita é o diálogo. Cada organização da janela guarda o seu
//! arranjo, porque as duas barras têm colunas diferentes.

use serde::{Deserialize, Serialize};

use crate::ui::Shell;

/// Um botão da barra (ou o tempo, que ocupa lugar como um).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Item {
    Previous,
    PlayPause,
    Next,
    Shuffle,
    Repeat,
    Time,
    Like,
    Dislike,
    Menu,
    Volume,
    Queue,
    Lyrics,
    Mini,
    Expand,
    Discord,
    Download,
    Save,
    Radio,
    CopyLink,
    Album,
    Artist,
    Personalize,
    SeekBack,
    SeekForward,
}

impl Item {
    pub const ALL: [Self; 24] = [
        Self::Previous,
        Self::PlayPause,
        Self::Next,
        Self::Shuffle,
        Self::Repeat,
        Self::Time,
        Self::Like,
        Self::Dislike,
        Self::Menu,
        Self::Volume,
        Self::Queue,
        Self::Lyrics,
        Self::Mini,
        Self::Expand,
        Self::Discord,
        Self::Download,
        Self::Save,
        Self::Radio,
        Self::CopyLink,
        Self::Album,
        Self::Artist,
        Self::Personalize,
        Self::SeekBack,
        Self::SeekForward,
    ];

    /// Largura na barra, com a coluna fixa do tempo e a barrinha do volume.
    pub fn width(self, panels: bool) -> f32 {
        match (self, panels) {
            (Self::PlayPause, false) => 52.0,
            (Self::PlayPause, true) => 36.0,
            (Self::Time, _) => 110.0,
            (Self::Volume, true) => FIXED_VOLUME_W,
            (_, false) => 40.0,
            (_, true) => 32.0,
        }
    }

    /// Curtir, não curtir e salvar em playlist só existem com conta.
    pub fn needs_account(self) -> bool {
        matches!(self, Self::Like | Self::Dislike | Self::Save)
    }
}

/// O volume com a barrinha sempre à vista.
pub const FIXED_VOLUME_W: f32 = 138.0;
/// O que a barrinha aberta sob o mouse ocupa à esquerda do ícone.
pub const HOVER_SLIDER_W: f32 = 118.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Zone {
    Left,
    Center,
    Right,
}

impl Zone {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub left: Vec<Item>,
    pub center: Vec<Item>,
    pub right: Vec<Item>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::classic()
    }
}

impl Layout {
    /// O arranjo do YouTube Music.
    pub fn classic() -> Self {
        use Item::*;
        Self { left: vec![Previous, PlayPause, Next, Time], center: vec![Like, Dislike, Menu], right: vec![Volume, Repeat, Shuffle, Expand] }
    }

    /// O arranjo dos painéis.
    pub fn panels() -> Self {
        use Item::*;
        Self { left: vec![Like], center: vec![Shuffle, Previous, PlayPause, Next, Repeat], right: vec![Lyrics, Queue, Volume, Mini, Expand] }
    }

    pub fn of(shell: Shell) -> Self {
        match shell {
            Shell::Classic => Self::classic(),
            Shell::Panels => Self::panels(),
        }
    }

    pub fn zone(&self, zone: Zone) -> &[Item] {
        match zone {
            Zone::Left => &self.left,
            Zone::Center => &self.center,
            Zone::Right => &self.right,
        }
    }

    fn zone_mut(&mut self, zone: Zone) -> &mut Vec<Item> {
        match zone {
            Zone::Left => &mut self.left,
            Zone::Center => &mut self.center,
            Zone::Right => &mut self.right,
        }
    }

    pub fn contains(&self, item: Item) -> bool {
        Zone::ALL.iter().any(|z| self.zone(*z).contains(&item))
    }

    pub fn remove(&mut self, item: Item) {
        for zone in Zone::ALL {
            self.zone_mut(zone).retain(|i| *i != item);
        }
    }

    /// Põe `item` em `zone`, no lugar `index` da coluna já sem ele.
    pub fn place(&mut self, item: Item, zone: Zone, index: usize) {
        self.remove(item);
        let list = self.zone_mut(zone);
        list.insert(index.min(list.len()), item);
    }

    /// Onde um botão entra quando a pessoa só o liga: no lugar de fábrica,
    /// depois dos vizinhos que ainda estão lá, ou no fim da direita.
    pub fn home(&self, item: Item, shell: Shell) -> (Zone, usize) {
        let factory = Self::of(shell);
        let Some(zone) = Zone::ALL.into_iter().find(|z| factory.zone(*z).contains(&item)) else {
            return (Zone::Right, self.right.len());
        };
        let order = factory.zone(zone);
        let before = &order[..order.iter().position(|i| *i == item).unwrap_or(0)];
        let index = self.zone(zone).iter().rposition(|i| before.contains(i)).map_or(0, |at| at + 1);
        (zone, index)
    }

    /// O arranjo lido do arquivo: sem repetidos, que dariam dois botões com
    /// o mesmo id.
    pub fn sane(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        for zone in Zone::ALL {
            self.zone_mut(zone).retain(|i| seen.insert(*i));
        }
        self
    }

    /// O arranjo que cabe em `width`: a música fica com `track_min`, e o que
    /// não cabe sai a partir do fim da direita. Tocar/pausar nunca sai.
    pub fn fitted(&self, panels: bool, signed_in: bool, width: f32, track_min: f32) -> Self {
        let mut shown = self.clone();
        if !signed_in {
            for zone in Zone::ALL {
                shown.zone_mut(zone).retain(|i| !i.needs_account());
            }
        }
        while shown.needed(panels) + track_min > width {
            let dropped = [Zone::Right, Zone::Center, Zone::Left].into_iter().any(|zone| {
                let list = shown.zone_mut(zone);
                let at = list.iter().rposition(|i| *i != Item::PlayPause);
                at.map(|at| list.remove(at)).is_some()
            });
            if !dropped {
                break;
            }
        }
        shown
    }

    /// Volume sem barrinha fixa: só no começo da direita a barrinha abre sob
    /// o mouse sem cobrir outro botão.
    pub fn hover_volume(&self, panels: bool) -> bool {
        !panels && self.right.first() == Some(&Item::Volume)
    }

    /// A largura de uma coluna, com a barrinha do volume.
    pub fn zone_width(&self, zone: Zone, panels: bool) -> f32 {
        let items = self.zone(zone);
        let gap = gap(zone, panels);
        let mut width: f32 = items.iter().map(|i| self.item_width(*i, panels) + gap).sum();
        width -= if items.is_empty() { 0.0 } else { gap };
        if zone == Zone::Right && self.hover_volume(panels) {
            width += HOVER_SLIDER_W;
        }
        width
    }

    /// `Volume` muda de largura conforme o lugar: fora do começo da direita a
    /// barrinha fica sempre aberta.
    pub fn item_width(&self, item: Item, panels: bool) -> f32 {
        if item == Item::Volume && !self.hover_volume(panels) { FIXED_VOLUME_W } else { item.width(panels) }
    }

    /// O espaço que os botões pedem. A barrinha do volume sob o mouse passa
    /// por cima da música e não conta.
    fn needed(&self, panels: bool) -> f32 {
        let slider = if self.hover_volume(panels) { HOVER_SLIDER_W } else { 0.0 };
        Zone::ALL.iter().map(|z| self.zone_width(*z, panels)).sum::<f32>() - slider
    }
}

/// O espaço entre os botões de uma coluna.
pub fn gap(zone: Zone, panels: bool) -> f32 {
    match (zone, panels) {
        (Zone::Left, false) => 8.0,
        (Zone::Center, false) => 0.0,
        (Zone::Center, true) => 11.0,
        _ => 4.0,
    }
}

/// O arranjo de cada organização da janela.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layouts {
    pub classic: Layout,
    pub panels: Layout,
}

impl Default for Layouts {
    fn default() -> Self {
        Self { classic: Layout::classic(), panels: Layout::panels() }
    }
}

impl Layouts {
    pub fn of_mut(&mut self, shell: Shell) -> &mut Layout {
        match shell {
            Shell::Classic => &mut self.classic,
            Shell::Panels => &mut self.panels,
        }
    }

    pub fn sane(self) -> Self {
        Self { classic: self.classic.sane(), panels: self.panels.sane() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Item::*;

    #[test]
    fn o_lugar_conta_a_coluna_ja_sem_o_botao() {
        let mut layout = Layout { left: vec![Previous, PlayPause, Next], center: vec![], right: vec![] };
        layout.place(Previous, Zone::Left, 1);
        assert_eq!(layout.left, [PlayPause, Previous, Next]);
        layout.place(Next, Zone::Left, 0);
        assert_eq!(layout.left, [Next, PlayPause, Previous]);
    }

    #[test]
    fn mover_entre_colunas_nao_duplica() {
        let mut layout = Layout::classic();
        layout.place(Shuffle, Zone::Left, 0);
        assert_eq!(layout.left[0], Shuffle);
        assert!(!layout.right.contains(&Shuffle));
    }

    #[test]
    fn tirar_e_por_um_botao() {
        let mut layout = Layout::classic();
        assert!(!layout.contains(Discord));
        layout.remove(Menu);
        assert!(!layout.contains(Menu));
        layout.place(Discord, Zone::Right, 99);
        assert_eq!(layout.right.last(), Some(&Discord));
    }

    #[test]
    fn ligar_um_botao_devolve_ele_ao_lugar_de_fabrica() {
        let mut layout = Layout::classic();
        layout.remove(Next);
        let (zone, index) = layout.home(Next, Shell::Classic);
        assert_eq!((zone, index), (Zone::Left, 2));
        layout.place(Next, zone, index);
        assert_eq!(layout, Layout::classic());
        // Sem os vizinhos, abre a coluna; sem lugar de fábrica, fecha a direita.
        let empty = Layout { left: vec![], center: vec![], right: vec![Menu] };
        assert_eq!(empty.home(Next, Shell::Classic), (Zone::Left, 0));
        assert_eq!(empty.home(Discord, Shell::Classic), (Zone::Right, 1));
    }

    #[test]
    fn arquivo_com_repetidos_nao_passa() {
        let layout = Layout { left: vec![Next, Next], center: vec![Next], right: vec![Menu] }.sane();
        assert_eq!(layout.left, [Next]);
        assert!(layout.center.is_empty());
    }

    #[test]
    fn o_que_nao_cabe_sai_pela_direita_e_tocar_fica() {
        let layout = Layout { left: vec![PlayPause, Next], center: vec![], right: vec![Queue, Lyrics] };
        let fitted = layout.fitted(false, true, 200.0, 100.0);
        assert_eq!(fitted.right, [] as [Item; 0]);
        assert_eq!(fitted.left, [PlayPause, Next]);
        let tiny = layout.fitted(false, true, 10.0, 100.0);
        assert_eq!(tiny.left, [PlayPause]);
    }

    #[test]
    fn sem_conta_curtir_nao_aparece() {
        let fitted = Layout::classic().fitted(false, false, 2000.0, 100.0);
        assert_eq!(fitted.center, [Menu]);
    }

    #[test]
    fn os_botoes_novos_comecam_fora_e_entram_no_fim_da_direita() {
        let layout = Layout::classic();
        assert!(!layout.contains(Download));
        assert_eq!(layout.home(Download, Shell::Classic), (Zone::Right, layout.right.len()));
        let fitted = Layout { left: vec![Save, Radio], center: vec![], right: vec![] }.fitted(false, false, 2000.0, 100.0);
        assert_eq!(fitted.left, [Radio]);
    }

    #[test]
    fn nenhum_botao_se_repete_na_lista() {
        let unique: std::collections::HashSet<_> = Item::ALL.into_iter().collect();
        assert_eq!(unique.len(), Item::ALL.len());
    }

    #[test]
    fn o_volume_so_abre_sob_o_mouse_no_comeco_da_direita() {
        let mut layout = Layout::classic();
        assert!(layout.hover_volume(false));
        layout.place(Volume, Zone::Left, 0);
        assert!(!layout.hover_volume(false));
        assert_eq!(layout.item_width(Volume, false), FIXED_VOLUME_W);
    }

    #[test]
    fn os_arranjos_padrao_cabem_na_menor_janela() {
        assert_eq!(Layout::classic().fitted(false, true, 756.0, 190.0), Layout::classic());
        assert_eq!(Layout::panels().fitted(true, true, 756.0, 190.0), Layout::panels());
    }
}
