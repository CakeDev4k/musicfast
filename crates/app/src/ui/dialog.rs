//! As janelas por cima de tudo: nova playlist, salvar numa playlist e o
//! login colando os cookies do navegador (para quando a janela do Google
//! recusar o navegador embutido).

use egui::{CornerRadius, CursorIcon, Id, Rect, Sense, Ui, pos2, vec2};
use musicfast_ytm::{Privacy, Track};

use super::icons;
use super::kit;
use super::theme;
use super::{Action, Cx};

pub enum Dialog {
    NewPlaylist { title: String, privacy: Privacy, track: Option<Track> },
    SaveTo { track: Track },
    Cookies { text: String, error: Option<String> },
}

/// Desenha o diálogo aberto; `true` quando ele deve fechar.
pub fn show(ctx: &egui::Context, dialog: &mut Dialog, cx: &mut Cx) -> bool {
    let mut close = false;
    let modal = egui::Modal::new(Id::new("dialogo"))
        .frame(
            egui::Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(420.0);
            match dialog {
                Dialog::NewPlaylist { title, privacy, track } => close = new_playlist(ui, cx, title, privacy, track),
                Dialog::SaveTo { track } => close = save_to(ui, cx, track),
                Dialog::Cookies { text, error } => close = cookies(ui, cx, text, error),
            }
        });
    close || modal.should_close()
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(theme::bold(20.0)));
    ui.add_space(14.0);
}

fn field(ui: &mut Ui, text: &mut String, hint: &str, focus: bool) -> egui::Response {
    let r = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(egui::RichText::new(hint).color(theme::TEXT_FAINT))
            .font(theme::regular(15.0))
            .margin(egui::Margin::symmetric(12, 10))
            .desired_width(f32::INFINITY),
    );
    if focus {
        r.request_focus();
    }
    r
}

fn new_playlist(ui: &mut Ui, cx: &mut Cx, title: &mut String, privacy: &mut Privacy, track: &Option<Track>) -> bool {
    heading(ui, "Nova playlist");
    let first = ui.memory(|m| m.focused().is_none());
    let r = field(ui, title, "Título", first);
    ui.add_space(12.0);
    ui.label(egui::RichText::new("Privacidade").color(theme::TEXT_DIM));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for p in Privacy::ALL {
            if kit::chip(ui, p.label(), *privacy == p).clicked() {
                *privacy = p;
            }
        }
    });
    ui.add_space(20.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let ready = !title.trim().is_empty();
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (kit::pill(ui, None, "Criar", true).clicked() || enter) && ready {
            cx.act(Action::CreatePlaylist {
                title: title.trim().to_owned(),
                privacy: *privacy,
                video_id: track.as_ref().map(|t| t.video_id.clone()),
            });
            close = true;
        }
        if kit::pill(ui, None, "Cancelar", false).clicked() {
            close = true;
        }
    });
    close
}

fn save_to(ui: &mut Ui, cx: &mut Cx, track: &Track) -> bool {
    heading(ui, "Salvar em uma playlist");
    ui.label(egui::RichText::new(&track.title).color(theme::TEXT_DIM));
    ui.add_space(10.0);
    let mut close = false;
    let playlists: Vec<_> = cx.me.editable_playlists().cloned().collect();
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(6), theme::HOVER);
        }
        let icon = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(40.0, 40.0));
        ui.painter().rect_filled(icon, CornerRadius::same(4), theme::ACTIVE);
        icons::paint(ui.painter(), icon.shrink(10.0), icons::PLUS, theme::TEXT);
        ui.painter().text(pos2(icon.right() + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, "Nova playlist", theme::medium(14.5), theme::TEXT);
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            cx.act(Action::Dialog(Dialog::NewPlaylist { title: String::new(), privacy: Privacy::Private, track: Some(track.clone()) }));
        }
        if playlists.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Nenhuma playlist sua ainda.").color(theme::TEXT_DIM));
        }
        for card in playlists {
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, CornerRadius::same(6), theme::HOVER);
            }
            let art = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(40.0, 40.0));
            cx.covers.paint(ui, art, &card.thumbs, 4);
            let w = rect.right() - art.right() - 24.0;
            let t = kit::one_line(ui, &card.title, theme::medium(14.5), theme::TEXT, w);
            ui.painter().galley(pos2(art.right() + 14.0, rect.center().y - 18.0), t, theme::TEXT);
            let s = kit::one_line(ui, &card.subtitle, theme::regular(12.5), theme::TEXT_DIM, w);
            ui.painter().galley(pos2(art.right() + 14.0, rect.center().y + 2.0), s, theme::TEXT_DIM);
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                cx.act(Action::AddToPlaylist {
                    playlist_id: card.browse_id.clone(),
                    title: card.title.clone(),
                    video_id: track.video_id.clone(),
                });
                close = true;
            }
        }
    });
    ui.add_space(14.0);
    if kit::pill(ui, None, "Fechar", false).clicked() {
        close = true;
    }
    close
}

fn cookies(ui: &mut Ui, cx: &mut Cx, text: &mut String, error: &mut Option<String>) -> bool {
    heading(ui, "Entrar com os cookies do navegador");
    for step in [
        "1. Abra music.youtube.com no seu navegador, já logado.",
        "2. Aperte F12, vá em Rede (Network) e recarregue a página.",
        "3. Clique num pedido para \"browse\" ou \"next\" e copie o valor do cabeçalho \"cookie\".",
        "4. Cole aqui. Ele fica cifrado neste computador e só este app o usa.",
    ] {
        ui.label(egui::RichText::new(step).color(theme::TEXT_DIM));
    }
    ui.add_space(10.0);
    ui.add(
        egui::TextEdit::multiline(text)
            .hint_text("SID=...; HSID=...; SAPISID=...")
            .font(egui::TextStyle::Monospace)
            .desired_rows(5)
            .desired_width(f32::INFINITY),
    );
    if let Some(e) = error {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(e.as_str()).color(theme::ACCENT));
    }
    ui.add_space(16.0);
    let mut close = false;
    ui.horizontal(|ui| {
        if kit::pill(ui, None, "Entrar", true).clicked() {
            if musicfast_ytm::Auth::from_cookie_header(text).is_some() {
                cx.act(Action::SubmitCookies(text.trim().to_owned()));
                close = true;
            } else {
                *error = Some("Esses cookies não têm a sessão (falta o SAPISID).".into());
            }
        }
        if kit::pill(ui, None, "Cancelar", false).clicked() {
            close = true;
        }
    });
    close
}
