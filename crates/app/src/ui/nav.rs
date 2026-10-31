//! A navegação: a coluna da esquerda (Início, Explorar) e a barra de cima
//! (voltar, avançar, a busca com sugestões e os filtros dela).

use egui::{Align2, CornerRadius, CursorIcon, Frame, Id, Key, Rect, Sense, Ui, pos2, vec2};
use musicfast_i18n::{Lang, t};
use musicfast_ytm::SearchFilter;

use super::icons;
use super::kit;
use super::theme;
use super::{Action, Cx, LibraryTab, Shell, View};

pub const SIDEBAR: f32 = 220.0;
/// Até onde a coluna aberta estica ou encolhe ao arrastar a borda.
pub const SIDEBAR_RANGE: std::ops::RangeInclusive<f32> = 180.0..=420.0;

/// Uma largura lida (do arquivo ou do painel) dentro do intervalo.
pub fn sidebar_width(width: f32) -> f32 {
    if width.is_finite() { width.clamp(*SIDEBAR_RANGE.start(), *SIDEBAR_RANGE.end()) } else { SIDEBAR }
}
pub const TOP: f32 = 64.0;

#[derive(Default)]
pub struct SearchBox {
    pub text: String,
    pub suggestions: Vec<String>,
    /// Para qual texto as sugestões vieram, e o último pedido.
    pub suggested_for: String,
    pub requested_for: String,
    pub edited_at: f64,
    pub focus: bool,
    pub selected: Option<usize>,
}

/// A coluna recolhida: só os ícones, com o nome embaixo, como no site.
pub const SIDEBAR_MINI: f32 = 72.0;

/// O botão de menu, que abre e recolhe a coluna.
fn menu_button(ui: &mut Ui, cx: &mut Cx, mini: bool) {
    let (r, response) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    kit::paint_icon_button(ui, r, &response, icons::MENU, theme::TEXT);
    if response.on_hover_text(if mini { t().expand_menu } else { t().collapse_menu }).clicked() {
        cx.act(Action::ToggleSidebar);
    }
}

pub fn sidebar(ui: &mut Ui, cx: &mut Cx, view: &View, mini: bool) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(if mini { 16.0 } else { 12.0 });
        menu_button(ui, cx, mini);
        if !mini {
            ui.add_space(4.0);
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 14.0, theme::accent());
            icons::paint_filled(ui.painter(), r.shrink(8.0).translate(vec2(1.0, 0.0)), icons::PLAY, theme::TEXT);
            ui.label(egui::RichText::new("MusicFast").font(theme::bold(19.0)));
        }
    });
    ui.add_space(if mini { 12.0 } else { 16.0 });
    let entries: [(icons::Icon, &str, View); 4] = [
        (icons::HOUSE, t().home, View::Home),
        (icons::COMPASS, t().explore, View::Explore),
        (icons::LIBRARY, t().library, View::Library(LibraryTab::Playlists)),
        // Sem conta e sem internet também: as músicas estão no disco.
        (icons::DOWNLOAD, t().downloads, View::Downloads),
    ];
    for (icon, label, target) in entries {
        let selected = match (&target, view) {
            (View::Library(_), View::Library(_)) => true,
            (t, v) => t == v,
        };
        let height = if mini { 64.0 } else { 46.0 };
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
        let item = if mini { rect.shrink2(vec2(6.0, 2.0)) } else { rect.shrink2(vec2(10.0, 2.0)) };
        if selected {
            ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS_HOVER);
        } else if response.hovered() {
            ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS);
        }
        if mini {
            let icon_rect = Rect::from_center_size(pos2(item.center().x, item.top() + 22.0), vec2(22.0, 22.0));
            icons::paint(ui.painter(), icon_rect, icon, theme::TEXT);
            let font = if selected { theme::semibold(10.5) } else { theme::regular(10.5) };
            ui.painter().text(pos2(item.center().x, item.bottom() - 14.0), Align2::CENTER_CENTER, label, font, theme::TEXT);
        } else {
            let icon_rect = Rect::from_center_size(pos2(item.left() + 26.0, item.center().y), vec2(22.0, 22.0));
            icons::paint(ui.painter(), icon_rect, icon, theme::TEXT);
            let font = if selected { theme::semibold(15.0) } else { theme::medium(15.0) };
            ui.painter().text(pos2(item.left() + 52.0, item.center().y), Align2::LEFT_CENTER, label, font, theme::TEXT);
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && *view != target {
            cx.act(Action::Open(target));
        }
    }
    if mini {
        // Recolhida, a coluna fica só com o essencial.
        if cx.me.signed_in {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space((SIDEBAR_MINI - 40.0) / 2.0);
                let (r, response) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                let response = response.on_hover_cursor(CursorIcon::PointingHand);
                let fill = if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS };
                ui.painter().circle_filled(r.center(), 20.0, fill);
                icons::paint(ui.painter(), r.shrink(10.0), icons::PLUS, theme::TEXT);
                if response.on_hover_text(t().new_playlist).clicked() {
                    cx.act(Action::Dialog(super::Dialog::NewPlaylist {
                        title: String::new(),
                        privacy: musicfast_ytm::Privacy::Private,
                        track: None,
                    }));
                }
            });
        }
        return;
    }
    ui.add_space(10.0);
    let line = ui.available_rect_before_wrap().shrink2(vec2(20.0, 0.0));
    ui.painter().hline(line.x_range(), line.top(), egui::Stroke::new(1.0, theme::BORDER));
    ui.add_space(14.0);

    if !cx.me.signed_in {
        // Sem conta, o convite do site no lugar das playlists.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            sign_in_invite(ui, cx);
        });
        return;
    }

    ui.horizontal(|ui| {
        ui.add_space(14.0);
        if kit::pill(ui, Some(icons::PLUS), t().new_playlist, false).clicked() {
            cx.act(Action::Dialog(super::Dialog::NewPlaylist {
                title: String::new(),
                privacy: musicfast_ytm::Privacy::Private,
                track: None,
            }));
        }
    });
    ui.add_space(10.0);
    // Só as linhas à vista são montadas, e sem copiar os nomes: antes, a
    // cada quadro, cada playlist da conta virava três `String` novas e uma
    // linha desenhada fora da tela.
    let me = cx.me;
    let playlists: Vec<&musicfast_ytm::Card> = me.playlists.iter().filter(|c| c.browse_id != "VLLM").collect();
    egui::ScrollArea::vertical().id_salt("playlists-lado").auto_shrink([false, false]).show_rows(ui, 48.0, playlists.len() + 1, |ui, range| {
        for i in range {
            let (id, title, subtitle) = match i.checked_sub(1) {
                None => ("VLLM", t().liked_songs, t().auto_playlist),
                Some(p) => (playlists[p].browse_id.as_str(), playlists[p].title.as_str(), playlists[p].subtitle.as_str()),
            };
            let selected = matches!(view, View::Browse { id: open, params: None } if open == id);
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
            let item = rect.shrink2(vec2(10.0, 1.0));
            if selected {
                ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS_HOVER);
            } else if response.hovered() {
                ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS);
            }
            let w = item.width() - 24.0;
            let title = kit::one_line(ui, title, theme::medium(14.0), theme::TEXT, w);
            ui.painter().galley(pos2(item.left() + 12.0, item.center().y - 17.0), title, theme::TEXT);
            let sub = kit::one_line(ui, subtitle, theme::regular(12.0), theme::TEXT_DIM, w);
            ui.painter().galley(pos2(item.left() + 12.0, item.center().y + 2.0), sub, theme::TEXT_DIM);
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !selected {
                cx.act(Action::Open(View::browse(id, None)));
            }
        }
    });
}

/// O convite para entrar, com o idioma e a interface (sem conta não há o
/// menu da foto).
pub(super) fn sign_in_invite(ui: &mut Ui, cx: &mut Cx) {
    ui.vertical(|ui| {
        ui.set_max_width(ui.available_width() - 20.0);
        ui.add(egui::Label::new(egui::RichText::new(t().sign_in_invite).color(theme::TEXT_DIM).font(theme::regular(13.0))).wrap());
        ui.add_space(10.0);
        let label = if cx.me.signing_in { t().signing_in } else { t().sign_in };
        if kit::pill(ui, Some(icons::LOG_IN), label, false).clicked() && !cx.me.signing_in {
            cx.act(Action::SignIn);
        }
        ui.add_space(6.0);
        // Para quando o Google recusar a janela embutida.
        let alt = ui
            .add(egui::Label::new(egui::RichText::new(t().sign_in_with_cookies).color(theme::TEXT_FAINT).font(theme::regular(12.0))).sense(Sense::click()))
            .on_hover_cursor(CursorIcon::PointingHand);
        if alt.clicked() {
            cx.act(Action::Dialog(super::Dialog::Cookies { text: String::new(), error: None }));
        }
        ui.add_space(14.0);
        let text = format!("{}: {}", t().language, language_label(cx.language));
        let r = ui
            .horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint(ui.painter(), r, icons::GLOBE, theme::TEXT_FAINT);
                ui.add(egui::Label::new(egui::RichText::new(text).color(theme::TEXT_FAINT).font(theme::regular(12.0))).sense(Sense::click()))
            })
            .inner
            .on_hover_cursor(CursorIcon::PointingHand);
        egui::Popup::menu(&r).width(LANGUAGE_W).show(|ui| {
            ui.set_width(LANGUAGE_W);
            for choice in LANGUAGES {
                if language_choice(ui, cx, choice, LANGUAGE_W) {
                    ui.close();
                }
            }
        });
        let shell = ui
            .horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint(ui.painter(), r, icons::PALETTE, theme::TEXT_FAINT);
                ui.add(egui::Label::new(egui::RichText::new(shell_label()).color(theme::TEXT_FAINT).font(theme::regular(12.0))).sense(Sense::click()))
            })
            .inner
            .on_hover_cursor(CursorIcon::PointingHand);
        if shell.clicked() {
            cx.act(Action::SetShell(other_shell()));
        }
    });
}

/// "Interface: Painéis".
fn shell_label() -> String {
    let name = if theme::panels() { t().interface_panels } else { t().interface_classic };
    format!("{}: {name}", t().interface)
}

fn other_shell() -> Shell {
    if theme::panels() { Shell::Classic } else { Shell::Panels }
}

/// O canto direito da barra de cima: a foto da conta (com o menu) ou o
/// "Fazer login".
pub(super) fn account(ui: &mut Ui, cx: &mut Cx, rect: Rect) {
    let cy = rect.center().y;
    if !cx.me.signed_in {
        let w = 128.0;
        let area = Rect::from_center_size(pos2(rect.right() - 24.0 - w / 2.0, cy), vec2(w, 36.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::right_to_left(egui::Align::Center)));
        let label = if cx.me.signing_in { t().signing_in } else { t().sign_in };
        let r = kit::pill(&mut child, None, label, true);
        if cx.me.accounts.is_empty() {
            if r.clicked() && !cx.me.signing_in {
                cx.act(Action::SignIn);
            }
        } else {
            // Já há contas guardadas: entrar é escolher uma delas.
            egui::Popup::menu(&r).align(egui::RectAlign::BOTTOM_END).gap(8.0).width(MENU_W).show(|ui| {
                ui.set_width(MENU_W);
                ui.spacing_mut().item_spacing.y = 0.0;
                account_rows(ui, cx);
            });
        }
        // Plano B, pelo botão direito: colar os cookies do navegador.
        r.context_menu(|ui| {
            if ui.button(t().sign_in_pasting_cookies).clicked() {
                cx.act(Action::Dialog(super::Dialog::Cookies { text: String::new(), error: None }));
            }
            if ui.button(t().storage).clicked() {
                cx.act(Action::Storage);
            }
        });
        return;
    }
    let avatar = Rect::from_center_size(pos2(rect.right() - 40.0, cy), vec2(36.0, 36.0));
    let response = ui.interact(avatar, Id::new("conta"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    match &cx.me.account {
        Some(account) if !account.photo.is_empty() => cx.covers.paint(ui, avatar, &account.photo, 18),
        other => {
            ui.painter().circle_filled(avatar.center(), 18.0, theme::accent());
            let initial = other.as_ref().and_then(|a| a.name.chars().next()).unwrap_or('?').to_uppercase().to_string();
            ui.painter().text(avatar.center(), Align2::CENTER_CENTER, initial, theme::semibold(16.0), theme::TEXT);
        }
    }
    // As outras contas ao lado da foto: um clique troca.
    let me = cx.me;
    for (i, profile) in me.other_accounts().take(quick_accounts(me, rect.width())).enumerate() {
        let center = avatar.center() - vec2(44.0 + QUICK_STEP * i as f32, 0.0);
        let r = Rect::from_center_size(center, vec2(QUICK_SIDE, QUICK_SIDE));
        let quick = ui.interact(r, Id::new(("conta-rapida", &profile.id)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        profile_photo(ui, cx, r, profile);
        if quick.hovered() {
            ui.painter().circle_stroke(center, QUICK_SIDE / 2.0 + 1.5, egui::Stroke::new(2.0, theme::TEXT));
        }
        if quick.on_hover_text(t().switch_to(profile_name(profile))).clicked() {
            cx.act(Action::SwitchAccount(profile.id.clone()));
        }
    }
    // O menu da conta, como o do site: largura fixa, preso à direita da
    // foto, cabeçalho com a foto grande e as ações em linhas largas.
    let frame = Frame::new()
        .fill(theme::MENU)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(CornerRadius::same(12))
        .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: egui::Color32::from_black_alpha(140) })
        .inner_margin(egui::Margin::symmetric(0, 8));
    egui::Popup::menu(&response)
        .align(egui::RectAlign::BOTTOM_END)
        .gap(8.0)
        .width(MENU_W)
        .frame(frame)
        .show(|ui| {
            ui.set_width(MENU_W);
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(account) = &cx.me.account {
                account_header(ui, cx, account);
                menu_divider(ui);
            }
            account_rows(ui, cx);
            menu_divider(ui);
            for (icon, label, target) in [
                (icons::LIBRARY, t().your_library, View::Library(LibraryTab::Playlists)),
                (icons::THUMBS_UP, t().liked_songs, View::browse("VLLM", None)),
                (icons::HISTORY, t().history, View::Library(LibraryTab::History)),
            ] {
                if menu_row(ui, icon, label) {
                    cx.act(Action::Open(target));
                }
            }
            menu_divider(ui);
            language_rows(ui, cx);
            if menu_row(ui, icons::PALETTE, &shell_label()) {
                cx.act(Action::SetShell(other_shell()));
            }
            if menu_row(ui, icons::PANEL_BOTTOM, t().bar_buttons) {
                cx.act(Action::CustomizeBar);
            }
            if menu_row(ui, icons::TRASH_2, t().storage) {
                cx.act(Action::Storage);
            }
            menu_divider(ui);
            if menu_row(ui, icons::LOG_OUT, t().sign_out) {
                cx.act(Action::SignOut);
            }
        });
}

const MENU_W: f32 = 300.0;
/// As fotinhas das outras contas na barra de cima.
const QUICK_MAX: usize = 3;
const QUICK_SIDE: f32 = 28.0;
const QUICK_STEP: f32 = 34.0;

/// Quantas fotinhas cabem sem encostar na busca.
fn quick_accounts(me: &super::Me, width: f32) -> usize {
    if width < 1000.0 { 0 } else { me.other_accounts().count().min(QUICK_MAX) }
}

/// A largura que as fotinhas ocupam, para o aviso de atualização não ir para
/// cima delas.
pub fn quick_accounts_width(me: &super::Me, width: f32) -> f32 {
    quick_accounts(me, width) as f32 * QUICK_STEP
}

fn profile_name(profile: &crate::accounts::Profile) -> &str {
    if profile.name.is_empty() { &profile.handle } else { &profile.name }
}

/// A foto da conta, ou a inicial sobre a cor de destaque.
fn profile_photo(ui: &Ui, cx: &mut Cx, rect: Rect, profile: &crate::accounts::Profile) {
    let radius = rect.width() / 2.0;
    if profile.photo.is_empty() {
        ui.painter().circle_filled(rect.center(), radius, theme::accent());
        let initial = profile_name(profile).chars().next().unwrap_or('?').to_uppercase().to_string();
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, initial, theme::semibold(radius), theme::TEXT);
    } else {
        cx.covers.paint(ui, rect, &profile.photo, radius as u8);
    }
}

/// As outras contas, cada linha um clique, e "Adicionar outra conta".
fn account_rows(ui: &mut Ui, cx: &mut Cx) {
    let me = cx.me;
    for profile in me.other_accounts() {
        let (rect, response) = ui.allocate_exact_size(vec2(MENU_W, 52.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::MENU_HOVER);
        }
        let photo = Rect::from_center_size(pos2(rect.left() + 34.0, rect.center().y), vec2(36.0, 36.0));
        profile_photo(ui, cx, photo, profile);
        let x = photo.right() + 14.0;
        let w = rect.right() - x - 16.0;
        let name = kit::one_line(ui, profile_name(profile), theme::medium(14.5), theme::TEXT, w);
        let name_y = if profile.handle.is_empty() { rect.center().y - name.size().y / 2.0 } else { rect.center().y - 19.0 };
        ui.painter().galley(pos2(x, name_y), name, theme::TEXT);
        if !profile.handle.is_empty() {
            let handle = kit::one_line(ui, &profile.handle, theme::regular(12.5), theme::TEXT_DIM, w);
            ui.painter().galley(pos2(x, rect.center().y + 2.0), handle, theme::TEXT_DIM);
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            cx.act(Action::SwitchAccount(profile.id.clone()));
            ui.close();
        }
    }
    if menu_row(ui, icons::PLUS, t().add_account) && !cx.me.signing_in {
        cx.act(Action::AddAccount);
    }
}

/// A foto, o nome, o @ e o atalho para a conta Google.
fn account_header(ui: &mut Ui, cx: &mut Cx, account: &musicfast_ytm::Account) {
    let (rect, _) = ui.allocate_exact_size(vec2(MENU_W, 76.0), Sense::hover());
    let avatar = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 8.0), vec2(44.0, 44.0));
    if account.photo.is_empty() {
        ui.painter().circle_filled(avatar.center(), 22.0, theme::accent());
        let initial = account.name.chars().next().unwrap_or('?').to_uppercase().to_string();
        ui.painter().text(avatar.center(), Align2::CENTER_CENTER, initial, theme::semibold(18.0), theme::TEXT);
    } else {
        cx.covers.paint(ui, avatar, &account.photo, 22);
    }
    let x = avatar.right() + 14.0;
    let w = rect.right() - x - 16.0;
    let name = kit::one_line(ui, &account.name, theme::semibold(16.0), theme::TEXT, w);
    ui.painter().galley(pos2(x, avatar.top() + 1.0), name, theme::TEXT);
    if !account.handle.is_empty() {
        let handle = kit::one_line(ui, &account.handle, theme::regular(13.5), theme::TEXT_DIM, w);
        ui.painter().galley(pos2(x, avatar.top() + 23.0), handle, theme::TEXT_DIM);
    }
    let link = kit::one_line(ui, t().manage_google_account, theme::medium(13.5), theme::LINK, w);
    if let Some(r) = kit::text_link(ui, pos2(x, avatar.bottom() + 6.0), link, theme::LINK, Id::new("conta-google"), true)
        && r.clicked()
    {
        kit::open_in_browser("https://myaccount.google.com/");
        ui.close();
    }
}

fn menu_divider(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(MENU_W, 17.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, egui::Stroke::new(1.0, theme::BORDER));
}

/// O idioma do sistema e os que o app fala, na ordem do seletor.
const LANGUAGES: [Option<Lang>; 4] = [None, Some(Lang::Pt), Some(Lang::En), Some(Lang::Es)];
const LANGUAGE_W: f32 = 240.0;

/// "Português", ou "Automático (Português)" quando segue o sistema.
fn language_label(choice: Option<Lang>) -> String {
    match choice {
        Some(lang) => lang.native_name().to_owned(),
        None => t().language_auto(musicfast_i18n::system().lang.native_name()),
    }
}

/// No menu da conta: "Idioma: Português", que abre as escolhas logo abaixo
/// (um submenu dentro do menu da foto fecharia os dois).
fn language_rows(ui: &mut Ui, cx: &mut Cx) {
    let open_id = Id::new("menu-idioma-aberto");
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let (rect, response) = ui.allocate_exact_size(vec2(MENU_W, 42.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::MENU_HOVER);
    }
    let r = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(20.0, 20.0));
    icons::paint(ui.painter(), r, icons::GLOBE, theme::TEXT);
    let text = format!("{}: {}", t().language, language_label(cx.language));
    let galley = kit::one_line(ui, &text, theme::regular(14.5), theme::TEXT, MENU_W - 56.0 - 44.0);
    ui.painter().galley(pos2(rect.left() + 56.0, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
    let chevron = Rect::from_center_size(pos2(rect.right() - 24.0, rect.center().y), vec2(18.0, 18.0));
    icons::paint(ui.painter(), chevron, if open { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN }, theme::TEXT_DIM);
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        ui.data_mut(|d| d.insert_temp(open_id, !open));
    }
    if open {
        for choice in LANGUAGES {
            if language_choice(ui, cx, choice, MENU_W) {
                ui.data_mut(|d| d.insert_temp(open_id, false));
                ui.close();
            }
        }
    }
}

/// Uma escolha de idioma, com o visto na que vale. `true` no clique.
fn language_choice(ui: &mut Ui, cx: &mut Cx, choice: Option<Lang>, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::MENU_HOVER);
    }
    if cx.language == choice {
        let r = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(16.0, 16.0));
        icons::paint(ui.painter(), r, icons::CHECK, theme::TEXT);
    }
    let galley = kit::one_line(ui, &language_label(choice), theme::regular(14.0), theme::TEXT, width - 56.0 - 12.0);
    ui.painter().galley(pos2(rect.left() + 56.0, rect.center().y - galley.size().y / 2.0), galley, theme::TEXT);
    let clicked = response.on_hover_cursor(CursorIcon::PointingHand).clicked();
    if clicked && cx.language != choice {
        cx.act(Action::SetLanguage(choice));
    }
    clicked
}

fn menu_row(ui: &mut Ui, icon: icons::Icon, text: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(MENU_W, 42.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::MENU_HOVER);
    }
    let r = Rect::from_center_size(pos2(rect.left() + 28.0, rect.center().y), vec2(20.0, 20.0));
    icons::paint(ui.painter(), r, icon, theme::TEXT);
    ui.painter().text(pos2(rect.left() + 56.0, rect.center().y), Align2::LEFT_CENTER, text, theme::regular(14.5), theme::TEXT);
    let clicked = response.on_hover_cursor(CursorIcon::PointingHand).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// As abas da biblioteca, acima da página.
pub fn library_tabs(ui: &mut Ui, cx: &mut Cx, current: LibraryTab) {
    ui.horizontal(|ui| {
        ui.add_space(32.0);
        for tab in LibraryTab::ALL {
            if kit::chip(ui, tab.label(), tab == current).clicked() && tab != current {
                cx.act(Action::Open(View::Library(tab)));
            }
        }
    });
}

pub fn top(ui: &mut Ui, cx: &mut Cx, search: &mut SearchBox, can_back: bool, can_forward: bool, suggest: &mut Option<String>) {
    let rect = ui.max_rect();
    let cy = rect.center().y;
    let mut x = rect.left() + 24.0;
    for (icon, enabled, action, tip) in [
        (icons::ARROW_LEFT, can_back, Action::Back, t().back_tip),
        (icons::CHEVRON_RIGHT, can_forward, Action::Forward, t().forward_tip),
    ] {
        let r = Rect::from_center_size(pos2(x, cy), vec2(36.0, 36.0));
        let resp = ui.interact(r, Id::new(("nav", tip)), if enabled { Sense::click() } else { Sense::hover() });
        let color = if enabled { theme::TEXT } else { theme::TEXT_FAINT };
        kit::paint_icon_button(ui, r, &resp, icon, color);
        if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tip).clicked() {
            cx.act(action);
        }
        x += 42.0;
    }

    account(ui, cx, rect);

    // A caixa de busca, centrada no que sobra.
    let width = (rect.width() - 320.0).clamp(220.0, 560.0);
    let field = Rect::from_center_size(pos2((x + rect.right()) / 2.0 - 20.0, cy), vec2(width, 42.0));
    let field = field.translate(vec2((x + 12.0 - field.left()).max(0.0), 0.0));
    search_field(ui, cx, search, field, theme::RADIUS, 40.0, suggest);
}

/// A caixa de busca com as sugestões, em `field`. `trailing`: o que fica
/// livre à direita do texto (o "limpar", e o que quem chama puser ali).
pub(super) fn search_field(ui: &mut Ui, cx: &mut Cx, search: &mut SearchBox, field: Rect, radius: u8, trailing: f32, suggest: &mut Option<String>) {
    let cy = field.center().y;
    ui.painter().rect(field, CornerRadius::same(radius), theme::GLASS, egui::Stroke::new(1.0, theme::GLASS_HOVER), egui::StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(pos2(field.left() + 22.0, cy), vec2(20.0, 20.0));
    icons::paint(ui.painter(), icon_rect, icons::SEARCH, theme::TEXT_DIM);
    let edit_rect = Rect::from_min_max(pos2(field.left() + 42.0, field.top()), pos2(field.right() - trailing, field.bottom()));
    let edit_id = Id::new("busca");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let had_focus = child.memory(|m| m.has_focus(edit_id));
    // As setas escolhem a sugestão antes que o campo as use.
    let (up, down, escape) = child.input_mut(|i| {
        if !had_focus {
            return (false, false, false);
        }
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.key_pressed(Key::Escape),
        )
    });
    let response = child.add(
        egui::TextEdit::singleline(&mut search.text)
            .id(edit_id)
            .frame(egui::Frame::NONE)
            .hint_text(egui::RichText::new(t().search_hint).color(theme::TEXT_FAINT))
            .font(theme::regular(15.0))
            .desired_width(edit_rect.width()),
    );
    if search.focus {
        response.request_focus();
        search.focus = false;
    }
    let now = ui.ctx().input(|i| i.time);
    if response.changed() {
        search.edited_at = now;
        search.selected = None;
    }
    let focused = response.has_focus();
    let query = search.text.trim().to_owned();
    // Sugestões depois de 180 ms sem digitar.
    if focused && !query.is_empty() && query != search.requested_for {
        if now - search.edited_at > 0.18 {
            search.requested_for = query.clone();
            *suggest = Some(query.clone());
        } else {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
    let shown: &[String] = if focused && !query.is_empty() && search.suggested_for == query { &search.suggestions } else { &[] };
    if !shown.is_empty() {
        let n = shown.len();
        if down {
            search.selected = Some(search.selected.map_or(0, |s| (s + 1) % n));
        }
        if up {
            search.selected = Some(search.selected.map_or(n - 1, |s| (s + n - 1) % n));
        }
    }
    if escape {
        response.surrender_focus();
    }
    let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    let mut chosen: Option<String> = None;
    if submitted {
        chosen = match search.selected.and_then(|s| shown.get(s)) {
            Some(s) => Some(s.clone()),
            None if !query.is_empty() => Some(query.clone()),
            None => None,
        };
    }
    if !search.text.is_empty() {
        let clear = Rect::from_center_size(pos2(field.right() - trailing + 18.0, cy), vec2(30.0, 30.0));
        let r = ui.interact(clear, Id::new("limpar-busca"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        kit::paint_icon_button(ui, clear, &r, icons::X, theme::TEXT_DIM);
        if r.clicked() {
            search.text.clear();
            search.focus = true;
        }
    }

    if !shown.is_empty() {
        let list_rect = Rect::from_min_size(field.left_bottom() + vec2(0.0, 4.0), vec2(field.width(), shown.len() as f32 * 40.0 + 12.0));
        let mut picked = None;
        egui::Area::new(Id::new("sugestoes"))
            .order(egui::Order::Foreground)
            .fixed_pos(list_rect.min)
            .show(ui.ctx(), |ui| {
                ui.painter().rect_filled(list_rect, CornerRadius::same(theme::RADIUS), theme::SURFACE);
                ui.allocate_exact_size(vec2(field.width(), 6.0), Sense::hover());
                for (i, s) in shown.iter().enumerate() {
                    let (row, resp) = ui.allocate_exact_size(vec2(field.width(), 40.0), Sense::click());
                    if resp.hovered() || search.selected == Some(i) {
                        ui.painter().rect_filled(row.shrink2(vec2(6.0, 0.0)), CornerRadius::same(6), theme::HOVER);
                    }
                    let ir = Rect::from_center_size(pos2(row.left() + 22.0, row.center().y), vec2(18.0, 18.0));
                    icons::paint(ui.painter(), ir, icons::SEARCH, theme::TEXT_FAINT);
                    let g = kit::one_line(ui, s, theme::regular(14.5), theme::TEXT, row.width() - 60.0);
                    ui.painter().galley(pos2(row.left() + 44.0, row.center().y - g.size().y / 2.0), g, theme::TEXT);
                    // Clicar tira o foco do campo antes do clique: vale o
                    // aperto, não a soltura.
                    if resp.is_pointer_button_down_on() || resp.clicked() {
                        picked = Some(s.clone());
                    }
                }
            });
        if picked.is_some() {
            chosen = picked;
        }
    }
    if let Some(query) = chosen {
        search.text = query.clone();
        search.requested_for = query.clone();
        search.selected = None;
        response.surrender_focus();
        cx.act(Action::Open(View::Search { query, filter: SearchFilter::All }));
    }
}

/// Os filtros da busca, acima dos resultados.
pub fn filters(ui: &mut Ui, cx: &mut Cx, query: &str, current: SearchFilter) {
    ui.horizontal(|ui| {
        ui.add_space(32.0);
        for filter in SearchFilter::ALL {
            if kit::chip(ui, filter.label(), filter == current).clicked() && filter != current {
                cx.act(Action::Open(View::Search { query: query.to_owned(), filter }));
            }
        }
    });
}
