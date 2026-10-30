//! A navegação: a coluna da esquerda (Início, Explorar) e a barra de cima
//! (voltar, avançar, a busca com sugestões e os filtros dela).

use egui::{Align2, CornerRadius, CursorIcon, Frame, Id, Key, Rect, Sense, Ui, pos2, vec2};
use musicfast_ytm::SearchFilter;

use super::icons;
use super::kit;
use super::theme;
use super::{Action, Cx, LibraryTab, View};

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
    if response.on_hover_text(if mini { "Expandir o menu" } else { "Recolher o menu" }).clicked() {
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
            ui.painter().circle_filled(r.center(), 14.0, theme::ACCENT);
            icons::paint_filled(ui.painter(), r.shrink(8.0).translate(vec2(1.0, 0.0)), icons::PLAY, theme::TEXT);
            ui.label(egui::RichText::new("MusicFast").font(theme::bold(19.0)));
        }
    });
    ui.add_space(if mini { 12.0 } else { 16.0 });
    let entries: [(icons::Icon, &str, View); 3] = [
        (icons::HOUSE, "Início", View::Home),
        (icons::COMPASS, "Explorar", View::Explore),
        (icons::LIBRARY, "Biblioteca", View::Library(LibraryTab::Playlists)),
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
                if response.on_hover_text("Nova playlist").clicked() {
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
            ui.vertical(|ui| {
                ui.set_max_width(ui.available_width() - 20.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new("Faça login para criar e compartilhar playlists, receber recomendações personalizadas e muito mais.")
                            .color(theme::TEXT_DIM)
                            .font(theme::regular(13.0)),
                    )
                    .wrap(),
                );
                ui.add_space(10.0);
                let label = if cx.me.signing_in { "Entrando…" } else { "Fazer login" };
                if kit::pill(ui, Some(icons::LOG_IN), label, false).clicked() && !cx.me.signing_in {
                    cx.act(Action::SignIn);
                }
                ui.add_space(6.0);
                // Para quando o Google recusar a janela embutida.
                let alt = ui
                    .add(
                        egui::Label::new(egui::RichText::new("Entrar com os cookies do navegador").color(theme::TEXT_FAINT).font(theme::regular(12.0)))
                            .sense(Sense::click()),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                if alt.clicked() {
                    cx.act(Action::Dialog(super::Dialog::Cookies { text: String::new(), error: None }));
                }
            });
        });
        return;
    }

    ui.horizontal(|ui| {
        ui.add_space(14.0);
        if kit::pill(ui, Some(icons::PLUS), "Nova playlist", false).clicked() {
            cx.act(Action::Dialog(super::Dialog::NewPlaylist {
                title: String::new(),
                privacy: musicfast_ytm::Privacy::Private,
                track: None,
            }));
        }
    });
    ui.add_space(10.0);
    egui::ScrollArea::vertical().id_salt("playlists-lado").auto_shrink([false, false]).show(ui, |ui| {
        let liked = ("VLLM".to_owned(), "Músicas curtidas".to_owned(), "Playlist automática".to_owned());
        let mut rows = vec![liked];
        rows.extend(
            cx.me
                .playlists
                .iter()
                .filter(|c| c.browse_id != "VLLM")
                .map(|c| (c.browse_id.clone(), c.title.clone(), c.subtitle.clone())),
        );
        for (id, title, subtitle) in rows {
            let target = View::browse(&id, None);
            let selected = *view == target;
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
            let item = rect.shrink2(vec2(10.0, 1.0));
            if selected {
                ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS_HOVER);
            } else if response.hovered() {
                ui.painter().rect_filled(item, CornerRadius::same(theme::RADIUS), theme::GLASS);
            }
            let w = item.width() - 24.0;
            let t = kit::one_line(ui, &title, theme::medium(14.0), theme::TEXT, w);
            ui.painter().galley(pos2(item.left() + 12.0, item.center().y - 17.0), t, theme::TEXT);
            let s = kit::one_line(ui, &subtitle, theme::regular(12.0), theme::TEXT_DIM, w);
            ui.painter().galley(pos2(item.left() + 12.0, item.center().y + 2.0), s, theme::TEXT_DIM);
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !selected {
                cx.act(Action::Open(target));
            }
        }
    });
}

/// O canto direito da barra de cima: a foto da conta (com o menu) ou o
/// "Fazer login".
fn account(ui: &mut Ui, cx: &mut Cx, rect: Rect) {
    let cy = rect.center().y;
    if !cx.me.signed_in {
        let w = 128.0;
        let area = Rect::from_center_size(pos2(rect.right() - 24.0 - w / 2.0, cy), vec2(w, 36.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::right_to_left(egui::Align::Center)));
        let label = if cx.me.signing_in { "Entrando…" } else { "Fazer login" };
        let r = kit::pill(&mut child, None, label, true);
        if r.clicked() && !cx.me.signing_in {
            cx.act(Action::SignIn);
        }
        // Plano B, pelo botão direito: colar os cookies do navegador.
        r.context_menu(|ui| {
            if ui.button("Entrar colando os cookies do navegador…").clicked() {
                cx.act(Action::Dialog(super::Dialog::Cookies { text: String::new(), error: None }));
            }
        });
        return;
    }
    let avatar = Rect::from_center_size(pos2(rect.right() - 40.0, cy), vec2(36.0, 36.0));
    let response = ui.interact(avatar, Id::new("conta"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    match &cx.me.account {
        Some(account) if !account.photo.is_empty() => cx.covers.paint(ui, avatar, &account.photo, 18),
        other => {
            ui.painter().circle_filled(avatar.center(), 18.0, theme::ACCENT);
            let initial = other.as_ref().and_then(|a| a.name.chars().next()).unwrap_or('?').to_uppercase().to_string();
            ui.painter().text(avatar.center(), Align2::CENTER_CENTER, initial, theme::semibold(16.0), theme::TEXT);
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
            for (icon, label, target) in [
                (icons::LIBRARY, "Sua biblioteca", View::Library(LibraryTab::Playlists)),
                (icons::THUMBS_UP, "Músicas curtidas", View::browse("VLLM", None)),
                (icons::HISTORY, "Histórico", View::Library(LibraryTab::History)),
            ] {
                if menu_row(ui, icon, label) {
                    cx.act(Action::Open(target));
                }
            }
            menu_divider(ui);
            if menu_row(ui, icons::LOG_OUT, "Sair") {
                cx.act(Action::SignOut);
            }
        });
}

const MENU_W: f32 = 300.0;

/// A foto, o nome, o @ e o atalho para a conta Google.
fn account_header(ui: &mut Ui, cx: &mut Cx, account: &musicfast_ytm::Account) {
    let (rect, _) = ui.allocate_exact_size(vec2(MENU_W, 76.0), Sense::hover());
    let avatar = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top() + 8.0), vec2(44.0, 44.0));
    if account.photo.is_empty() {
        ui.painter().circle_filled(avatar.center(), 22.0, theme::ACCENT);
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
    let link = kit::one_line(ui, "Gerenciar sua Conta do Google", theme::medium(13.5), theme::LINK, w);
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
        (icons::ARROW_LEFT, can_back, Action::Back, "Voltar (Alt+←)"),
        (icons::CHEVRON_RIGHT, can_forward, Action::Forward, "Avançar (Alt+→)"),
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
    ui.painter().rect(field, CornerRadius::same(theme::RADIUS), theme::GLASS, egui::Stroke::new(1.0, theme::GLASS_HOVER), egui::StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(pos2(field.left() + 22.0, cy), vec2(20.0, 20.0));
    icons::paint(ui.painter(), icon_rect, icons::SEARCH, theme::TEXT_DIM);
    let edit_rect = Rect::from_min_max(pos2(field.left() + 42.0, field.top()), pos2(field.right() - 40.0, field.bottom()));
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
            .hint_text(egui::RichText::new("Buscar músicas, álbuns, artistas, podcasts").color(theme::TEXT_FAINT))
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
        let clear = Rect::from_center_size(pos2(field.right() - 22.0, cy), vec2(30.0, 30.0));
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
