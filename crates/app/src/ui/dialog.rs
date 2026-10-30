//! As janelas por cima de tudo: nova playlist, salvar numa playlist e o
//! login colando os cookies do navegador (para quando a janela do Google
//! recusar o navegador embutido).

use egui::ecolor::Hsva;
use egui::{Color32, CornerRadius, CursorIcon, Id, Mesh, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use musicfast_i18n::t;
use musicfast_ytm::{Privacy, Track};

use super::icons;
use super::kit;
use super::theme;
use super::{Action, Cx};
use crate::look::{Backdrop, Image, Images, Look, Now, PRESETS, Skin, Surface};
use crate::viz::{Viz, VizCfg, VizColor};

pub enum Dialog {
    NewPlaylist { title: String, privacy: Privacy, track: Option<Track> },
    SaveTo { track: Track },
    Cookies { text: String, error: Option<String> },
    /// "Personalizar o player": a escolha em edição, a superfície ajustada,
    /// o link digitado, a cor do seletor (em HSV, para o matiz não sumir no
    /// cinza) e se ele está aberto.
    Look { look: Look, surface: Surface, url: String, hsv: [f32; 3], custom: bool },
    /// "Armazenamento e cache": o que o cache ocupa (`None` enquanto mede)
    /// e se a limpeza está rodando.
    Storage { usage: Option<crate::storage::Usage>, busy: bool },
}

/// Desenha o diálogo aberto; `true` quando ele deve fechar.
/// `images` e `now`: o que as miniaturas do "Personalizar" mostram.
pub fn show(ctx: &egui::Context, dialog: &mut Dialog, cx: &mut Cx, images: &Images, now: &Now) -> bool {
    let mut close = false;
    let modal = egui::Modal::new(Id::new("dialogo"))
        .frame(
            egui::Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(if matches!(dialog, Dialog::Look { .. }) { 540.0 } else { 420.0 });
            match dialog {
                Dialog::NewPlaylist { title, privacy, track } => close = new_playlist(ui, cx, title, privacy, track),
                Dialog::SaveTo { track } => close = save_to(ui, cx, track),
                Dialog::Cookies { text, error } => close = cookies(ui, cx, text, error),
                Dialog::Storage { usage, busy } => close = storage(ui, cx, *usage, busy),
                Dialog::Look { look, surface, url, hsv, custom } => {
                    close = personalize(ui, cx, look, surface, url, hsv, custom, images, now);
                }
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
    heading(ui, t().new_playlist);
    let first = ui.memory(|m| m.focused().is_none());
    let r = field(ui, title, t().title_hint, first);
    ui.add_space(12.0);
    ui.label(egui::RichText::new(t().privacy).color(theme::TEXT_DIM));
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
        if (kit::pill(ui, None, t().create, true).clicked() || enter) && ready {
            cx.act(Action::CreatePlaylist {
                title: title.trim().to_owned(),
                privacy: *privacy,
                video_id: track.as_ref().map(|t| t.video_id.clone()),
            });
            close = true;
        }
        if kit::pill(ui, None, t().cancel, false).clicked() {
            close = true;
        }
    });
    close
}

fn save_to(ui: &mut Ui, cx: &mut Cx, track: &Track) -> bool {
    heading(ui, t().save_to_playlist);
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
        ui.painter().text(pos2(icon.right() + 14.0, rect.center().y), egui::Align2::LEFT_CENTER, t().new_playlist, theme::medium(14.5), theme::TEXT);
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            cx.act(Action::Dialog(Dialog::NewPlaylist { title: String::new(), privacy: Privacy::Private, track: Some(track.clone()) }));
        }
        if playlists.is_empty() {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(t().no_playlists_yet).color(theme::TEXT_DIM));
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
    if kit::pill(ui, None, t().close, false).clicked() {
        close = true;
    }
    close
}

fn storage(ui: &mut Ui, cx: &mut Cx, usage: Option<crate::storage::Usage>, busy: &mut bool) -> bool {
    use crate::storage::human;
    heading(ui, t().storage);
    ui.label(egui::RichText::new(t().storage_hint).color(theme::TEXT_DIM));
    ui.add_space(14.0);
    let rows = [
        (t().storage_lyrics, usage.map(|u| u.lyrics)),
        (t().storage_covers, usage.map(|u| u.covers)),
        (t().storage_browser, usage.map(|u| u.browser)),
    ];
    let line = |ui: &mut Ui, label: &str, value: Option<u64>, strong: bool| {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        let font = if strong { theme::semibold(15.0) } else { theme::regular(14.5) };
        let color = if strong { theme::TEXT } else { theme::TEXT_DIM };
        ui.painter().text(pos2(rect.left(), rect.center().y), egui::Align2::LEFT_CENTER, label, font.clone(), color);
        let value = value.map_or_else(|| t().storage_measuring.to_owned(), human);
        ui.painter().text(pos2(rect.right(), rect.center().y), egui::Align2::RIGHT_CENTER, value, font, theme::TEXT);
    };
    for (label, value) in rows {
        line(ui, label, value, false);
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, theme::BORDER));
    line(ui, t().storage_total, usage.map(|u| u.total()), true);
    ui.add_space(14.0);
    ui.label(egui::RichText::new(t().clear_cache_keeps).color(theme::TEXT_FAINT).size(13.0));
    ui.add_space(12.0);
    let mut close = false;
    ui.horizontal(|ui| {
        let ready = usage.is_some_and(|u| u.total() > 0) && !*busy;
        let r = ui.add_enabled_ui(ready, |ui| kit::pill(ui, Some(icons::TRASH_2), t().clear_cache, true)).inner;
        if r.clicked() && ready {
            *busy = true;
            cx.act(Action::ClearCache);
        }
        if *busy {
            kit::spinner(ui, 18.0);
        }
        if kit::pill(ui, None, t().close, false).clicked() {
            close = true;
        }
    });
    close
}

fn cookies(ui: &mut Ui, cx: &mut Cx, text: &mut String, error: &mut Option<String>) -> bool {
    heading(ui, t().sign_in_with_cookies);
    for step in t().cookie_steps {
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
        if kit::pill(ui, None, t().enter, true).clicked() {
            if musicfast_ytm::Auth::from_cookie_header(text).is_some() {
                cx.act(Action::SubmitCookies(text.trim().to_owned()));
                close = true;
            } else {
                *error = Some(t().cookies_without_session.into());
            }
        }
        if kit::pill(ui, None, t().cancel, false).clicked() {
            close = true;
        }
    });
    close
}

/// "Personalizar o player". Cada mudança vale na hora (a superfície editada
/// fica à vista atrás); o link só ao apertar "Aplicar" ou Enter, para não
/// baixar a cada letra. Fundo e visualizador em abas, para o painel caber
/// sem rolar.
#[allow(clippy::too_many_arguments)]
fn personalize(
    ui: &mut Ui,
    cx: &mut Cx,
    look: &mut Look,
    surface: &mut Surface,
    url: &mut String,
    hsv: &mut [f32; 3],
    custom: &mut bool,
    images: &Images,
    now: &Now,
) -> bool {
    heading(ui, t().customize_player);
    let width = ui.available_width();
    // A seção: o fundo, o visualizador, a letra ou a tradução.
    let section_id = Id::new("personalizar-secao");
    let mut section = ui.data(|d| d.get_temp::<usize>(section_id)).unwrap_or(0);
    let tabs = [
        (icons::IMAGE, t().look_section_background),
        (icons::AUDIO_WAVEFORM, t().viz_title),
        (icons::MIC_VOCAL, t().look_section_lyrics),
        (icons::GLOBE, t().lyrics_translation),
    ];
    if let Some(i) = segmented(ui, "secao", &tabs, section, width, 40.0) {
        section = i;
        ui.data_mut(|d| d.insert_temp(section_id, section));
    }
    ui.add_space(12.0);
    // Em qual tela: só no fundo e no visualizador (a letra vale para todas).
    let items: Vec<(icons::Icon, &str)> = Surface::ALL.iter().map(|s| (s.icon(), s.label())).collect();
    let current = Surface::ALL.iter().position(|s| s == surface).unwrap_or(0);
    let picked = if section < 2 { segmented(ui, "superficie", &items, current, width, 34.0) } else { None };
    if section < 2 {
        ui.add_space(14.0);
    }
    if let Some(i) = picked {
        let s = Surface::ALL[i];
        *surface = s;
        // A tela do player aberta atrás só quando é ela que se edita: a tela
        // principal ficaria escondida por baixo.
        cx.act(Action::Expand(s == Surface::Player));
        let skin = look.skin(s);
        *url = if skin.image_url.starts_with("http") { skin.image_url.clone() } else { String::new() };
        *hsv = to_hsv(skin.color);
        *custom = !PRESETS.contains(&skin.color);
    }

    let image = images.get(*surface);
    let surface = *surface;
    let mut changed = false;
    // Só para janelas muito baixas; no tamanho normal tudo cabe.
    let max_h = (ui.ctx().content_rect().height() - 210.0).max(200.0);
    egui::ScrollArea::vertical().id_salt("personalizar").max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
        // A letra e a tradução valem para todas as telas.
        if section == 2 {
            changed |= lyrics_section(ui, &mut look.lyrics, cx.translate_lyrics, width);
            return;
        }
        if section == 3 {
            changed |= translation_section(ui, cx, &mut look.lyrics, width);
            return;
        }
        let skin = look.skin_mut(surface);
        if section == 0 {
            // Os cartões, um por fundo, cada um com a miniatura de como fica.
            const GAP: f32 = 10.0;
            let card_w = ((width - GAP * 3.0) / 4.0).floor();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for b in Backdrop::ALL {
                    let sample = Skin { backdrop: b, ..skin.clone() };
                    if look_card(ui, &sample, skin.backdrop == b, card_w, image, now, surface) && skin.backdrop != b {
                        skin.backdrop = b;
                        changed = true;
                    }
                }
            });
            ui.add_space(8.0);
            ui.label(egui::RichText::new(skin.backdrop.hint()).color(theme::TEXT_DIM).font(theme::regular(13.0)));
            ui.add_space(10.0);
            match skin.backdrop {
                Backdrop::Color => changed |= color_choice(ui, skin, hsv, custom),
                Backdrop::Image => changed |= image_choice(ui, cx, skin, url, image, surface),
                _ => {}
            }
            if skin.backdrop != Backdrop::Plain {
                ui.add_space(12.0);
                egui::Frame::new()
                    .fill(PANEL)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(PANEL_PAD as i8))
                    .show(ui, |ui| {
                        let inner = width - PANEL_PAD * 2.0;
                        ui.set_width(inner);
                        let col = ((inner - 20.0) / 2.0).floor();
                        let percent: fn(f32) -> String = |v| format!("{:.0}%", v * 100.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 20.0;
                            changed |= slider(ui, t().look_dim, &mut skin.dim, 0.0..=0.9, percent, col);
                            if skin.backdrop == Backdrop::Image {
                                changed |= slider(ui, t().look_blur, &mut skin.blur, 0.0..=1.0, percent, col);
                            }
                        });
                    });
            }
        } else {
            ui.label(egui::RichText::new(t().viz_hint).color(theme::TEXT_DIM).font(theme::regular(13.0)));
            ui.add_space(10.0);
            const GAP: f32 = 8.0;
            let choices: Vec<Option<Viz>> = std::iter::once(None).chain(Viz::ALL.map(Some)).collect();
            let viz_w = ((width - GAP * 5.0) / 6.0).floor();
            for row in choices.chunks(6) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for &choice in row {
                        let sample = Skin { viz: choice, ..skin.clone() };
                        if viz_card(ui, &sample, skin.viz == choice, viz_w, image, now, surface) && skin.viz != choice {
                            skin.viz = choice;
                            changed = true;
                        }
                    }
                });
                ui.add_space(6.0);
            }
            if let Some(style) = skin.viz {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(PANEL)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(PANEL_PAD as i8))
                    .show(ui, |ui| {
                        let inner = width - PANEL_PAD * 2.0;
                        ui.set_width(inner);
                        changed |= viz_settings(ui, &mut skin.viz_cfg, style, inner);
                    });
            }
        }
    });

    if changed {
        cx.act(Action::SetLook(look.clone()));
    }
    ui.add_space(16.0);
    let mut close = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        close = kit::pill(ui, None, t().close, true).clicked();
    });
    close
}

/// Um cartão: a miniatura, com contorno e visto quando escolhido, e embaixo
/// o ícone e o nome. `true` no clique.
fn look_card(ui: &mut Ui, sample: &Skin, selected: bool, w: f32, image: &Image, now: &Now, surface: Surface) -> bool {
    let thumb_h = (w * 0.62).round();
    let (rect, response) = ui.allocate_exact_size(vec2(w, thumb_h + 30.0), Sense::click());
    let hovered = response.hovered();
    let thumb = Rect::from_min_size(rect.min, vec2(w, thumb_h));
    crate::look::preview(ui, thumb, sample, image, now, surface);
    round_off(ui, thumb, 8);
    let stroke = if selected {
        Stroke::new(2.0, theme::TEXT)
    } else if hovered {
        Stroke::new(1.0, Color32::from_white_alpha(90))
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    ui.painter().rect_stroke(thumb, CornerRadius::same(8), stroke, StrokeKind::Inside);
    if selected {
        let badge = Rect::from_center_size(thumb.right_top() + vec2(-13.0, 13.0), vec2(18.0, 18.0));
        ui.painter().circle_filled(badge.center(), 9.0, theme::TEXT);
        icons::paint(ui.painter(), badge.shrink(4.5), icons::CHECK, Color32::BLACK);
    }
    let color = if selected || hovered { theme::TEXT } else { theme::TEXT_DIM };
    let icon = Rect::from_center_size(pos2(rect.left() + 8.0, thumb.bottom() + 16.0), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon, sample.backdrop.icon(), color);
    let font = if selected { theme::semibold(13.0) } else { theme::medium(13.0) };
    let label = kit::one_line(ui, sample.backdrop.label(), font, color, w - 24.0);
    ui.painter().galley(pos2(icon.right() + 7.0, icon.center().y - label.size().y / 2.0), label, color);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Arredonda os cantos do que foi pintado reto em `rect` (degradê, imagem),
/// cobrindo-os com a cor do diálogo.
fn round_off(ui: &Ui, rect: Rect, radius: u8) {
    round_off_on(ui, rect, radius, theme::SURFACE);
}

/// Como `round_off`, sobre um fundo de outra cor.
fn round_off_on(ui: &Ui, rect: Rect, radius: u8, under: Color32) {
    ui.painter().rect_stroke(rect, CornerRadius::same(radius), Stroke::new(8.0, under), StrokeKind::Outside);
}

/// A cor do quadro dos ajustes do visualizador (o diálogo com um véu branco).
const PANEL: Color32 = Color32::from_rgb(36, 36, 36);
/// A margem de dentro desse quadro.
const PANEL_PAD: f32 = 14.0;

/// Um cartão do visualizador: a miniatura com o estilo se mexendo e o nome
/// embaixo. `true` no clique.
fn viz_card(ui: &mut Ui, sample: &Skin, selected: bool, w: f32, image: &Image, now: &Now, surface: Surface) -> bool {
    let thumb_h = (w * 0.66).round();
    let (rect, response) = ui.allocate_exact_size(vec2(w, thumb_h + 24.0), Sense::click());
    let hovered = response.hovered();
    let thumb = Rect::from_min_size(rect.min, vec2(w, thumb_h));
    crate::look::preview(ui, thumb, sample, image, now, surface);
    round_off(ui, thumb, 7);
    let stroke = if selected {
        Stroke::new(2.0, theme::TEXT)
    } else if hovered {
        Stroke::new(1.0, Color32::from_white_alpha(90))
    } else {
        Stroke::new(1.0, theme::BORDER)
    };
    ui.painter().rect_stroke(thumb, CornerRadius::same(7), stroke, StrokeKind::Inside);
    let color = if selected || hovered { theme::TEXT } else { theme::TEXT_DIM };
    let name = sample.viz.map_or(t().viz_off, Viz::label);
    let font = if selected { theme::semibold(12.5) } else { theme::medium(12.5) };
    let label = kit::one_line(ui, name, font, color, w);
    ui.painter().galley(pos2(rect.center().x - label.size().x / 2.0, thumb.bottom() + 6.0), label, color);
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// A "Cor fixa": as cores prontas em bolinhas e, no "+", o seletor pequeno.
fn color_choice(ui: &mut Ui, skin: &mut Skin, hsv: &mut [f32; 3], custom: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for preset in PRESETS {
            let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
            let c = rect.center();
            ui.painter().circle_filled(c, 12.0, Color32::from_rgb(preset[0], preset[1], preset[2]));
            if skin.color == preset && !*custom {
                ui.painter().circle_stroke(c, 13.5, Stroke::new(2.0, theme::TEXT));
            } else if response.hovered() {
                ui.painter().circle_stroke(c, 13.5, Stroke::new(1.0, Color32::from_white_alpha(90)));
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                skin.color = preset;
                *hsv = to_hsv(preset);
                *custom = false;
                changed = true;
            }
        }
        // O "+": abre e fecha o seletor; aberto, mostra a cor feita nele.
        let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
        let c = rect.center();
        if *custom {
            ui.painter().circle_filled(c, 12.0, Color32::from_rgb(skin.color[0], skin.color[1], skin.color[2]));
            ui.painter().circle_stroke(c, 13.5, Stroke::new(2.0, theme::TEXT));
        } else {
            ui.painter().circle_filled(c, 12.0, if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS });
            icons::paint(ui.painter(), rect.shrink(8.0), icons::PLUS, theme::TEXT);
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t().custom_color).clicked() {
            *custom = !*custom;
            if *custom {
                *hsv = to_hsv(skin.color);
            }
        }
    });
    if *custom {
        ui.add_space(12.0);
        if hsv_picker(ui, hsv) {
            let c = Color32::from(Hsva::new(hsv[0], hsv[1], hsv[2], 1.0));
            skin.color = [c.r(), c.g(), c.b()];
            changed = true;
        }
    }
    changed
}

pub fn to_hsv(rgb: [u8; 3]) -> [f32; 3] {
    let h = Hsva::from(Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
    [h.h, h.s, h.v]
}

/// O seletor pequeno: o quadro de saturação e brilho, e numa linha a
/// amostra, o código e a faixa do matiz. `true` quando a cor mudou.
fn hsv_picker(ui: &mut Ui, hsv: &mut [f32; 3]) -> bool {
    hsv_picker_on(ui, hsv, theme::SURFACE)
}

/// Como `hsv_picker`, sobre um fundo de outra cor.
fn hsv_picker_on(ui: &mut Ui, hsv: &mut [f32; 3], under: Color32) -> bool {
    let before = *hsv;
    let color_at = |h: f32, s: f32, v: f32| Color32::from(Hsva::new(h, s, v, 1.0));

    // Saturação para a direita, brilho para cima.
    let (area, response) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::click_and_drag());
    const COLS: u32 = 12;
    const ROWS: u32 = 6;
    let mut mesh = Mesh::default();
    for row in 0..=ROWS {
        for col in 0..=COLS {
            let (s, v) = (col as f32 / COLS as f32, 1.0 - row as f32 / ROWS as f32);
            let p = pos2(area.left() + area.width() * s, area.top() + area.height() * (1.0 - v));
            mesh.colored_vertex(p, color_at(hsv[0], s, v));
        }
    }
    for row in 0..ROWS {
        for col in 0..COLS {
            let i = row * (COLS + 1) + col;
            mesh.add_triangle(i, i + 1, i + COLS + 1);
            mesh.add_triangle(i + 1, i + COLS + 2, i + COLS + 1);
        }
    }
    ui.painter().add(mesh);
    round_off_on(ui, area, 8, under);
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
    {
        hsv[1] = ((p.x - area.left()) / area.width()).clamp(0.0, 1.0);
        hsv[2] = (1.0 - (p.y - area.top()) / area.height()).clamp(0.0, 1.0);
    }
    let knob = pos2(area.left() + area.width() * hsv[1], area.top() + area.height() * (1.0 - hsv[2]));
    ui.painter().circle_filled(knob, 7.0, color_at(hsv[0], hsv[1], hsv[2]));
    ui.painter().circle_stroke(knob, 7.0, Stroke::new(2.0, Color32::WHITE));
    response.on_hover_cursor(CursorIcon::Crosshair);
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        let c = color_at(hsv[0], hsv[1], hsv[2]);
        let (sample, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        ui.painter().rect_filled(sample, CornerRadius::same(6), c);
        let (code, _) = ui.allocate_exact_size(vec2(72.0, 24.0), Sense::hover());
        let hex = format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b());
        ui.painter().text(code.left_center() + vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, hex, theme::medium(13.0), theme::TEXT_DIM);
        let (strip, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
        let bar = Rect::from_center_size(strip.center(), vec2(strip.width() - 16.0, 10.0));
        const STEPS: u32 = 24;
        let mut mesh = Mesh::default();
        for i in 0..=STEPS {
            let h = i as f32 / STEPS as f32;
            let x = bar.left() + bar.width() * h;
            mesh.colored_vertex(pos2(x, bar.top()), color_at(h, 1.0, 1.0));
            mesh.colored_vertex(pos2(x, bar.bottom()), color_at(h, 1.0, 1.0));
        }
        for i in 0..STEPS {
            let k = i * 2;
            mesh.add_triangle(k, k + 1, k + 2);
            mesh.add_triangle(k + 1, k + 3, k + 2);
        }
        ui.painter().add(mesh);
        round_off_on(ui, bar, 5, under);
        if let Some(p) = response.interact_pointer_pos()
            && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
        {
            hsv[0] = ((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        }
        let knob = pos2(bar.left() + bar.width() * hsv[0], bar.center().y);
        ui.painter().circle_filled(knob, 8.0, color_at(hsv[0], 1.0, 1.0));
        ui.painter().circle_stroke(knob, 8.0, Stroke::new(2.0, Color32::WHITE));
        response.on_hover_cursor(CursorIcon::PointingHand);
    });
    *hsv != before
}

/// "Imagem ou GIF": do computador (botão, ou arrastando para a janela) ou de
/// um link, e como está o carregamento.
fn image_choice(ui: &mut Ui, cx: &mut Cx, skin: &mut Skin, url: &mut String, image: &Image, surface: Surface) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if kit::pill(ui, Some(icons::IMAGE), t().pick_from_computer, true).clicked() {
            cx.act(Action::PickBackdropFile(surface));
        }
        ui.add_space(6.0);
        ui.label(egui::RichText::new(t().drop_hint).color(theme::TEXT_FAINT).font(theme::regular(12.5)));
    });
    ui.add_space(12.0);
    ui.label(egui::RichText::new(t().or_paste_link).color(theme::TEXT_DIM).font(theme::medium(13.0)));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let apply = 100.0;
        let r = ui.add(
            egui::TextEdit::singleline(url)
                .hint_text(egui::RichText::new(t().image_url_hint).color(theme::TEXT_FAINT))
                .font(theme::regular(14.0))
                .margin(egui::Margin::symmetric(12, 9))
                .desired_width(ui.available_width() - apply - 8.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (kit::pill(ui, Some(icons::LINK), t().apply, false).clicked() || enter) && !url.trim().is_empty() && url.trim() != skin.image_url {
            skin.image_url = url.trim().to_owned();
            changed = true;
        }
    });
    ui.add_space(8.0);
    let (text, color) = if image.loading() {
        (t().loading_image.to_owned(), theme::TEXT_DIM)
    } else if let Some(error) = image.error() {
        (t().image_load_failed(error), theme::ACCENT)
    } else if image.ready() && !skin.image_url.is_empty() {
        let name = skin.image_url.rsplit(['/', '\\']).next().unwrap_or_default();
        (format!("{} {name}", t().image_applied), theme::TEXT_DIM)
    } else {
        (t().image_formats.to_owned(), theme::TEXT_FAINT)
    };
    ui.add(egui::Label::new(egui::RichText::new(text).color(color).font(theme::regular(12.5))).truncate());
    changed
}

/// As cores prontas do visualizador: vivas, para aparecer sobre o escuro.
const VIZ_PRESETS: [[u8; 3]; 10] = [
    [255, 255, 255],
    [170, 200, 255],
    [120, 200, 255],
    [70, 110, 255],
    [160, 100, 255],
    [255, 90, 170],
    [255, 70, 70],
    [255, 160, 50],
    [255, 225, 80],
    [90, 230, 140],
];

/// Os ajustes do visualizador ligado, em `width`: a cor (de onde vem, e as
/// cores), os controles deslizantes em duas colunas e o brilho. `true`
/// quando algo mudou.
fn viz_settings(ui: &mut Ui, cfg: &mut VizCfg, style: Viz, width: f32) -> bool {
    let mut changed = false;
    // A cor: da capa, arco-íris, degradê ou uma só; o restaurar na ponta.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.set_min_height(32.0);
        let (r, _) = ui.allocate_exact_size(vec2(44.0, 30.0), Sense::hover());
        let g = kit::one_line(ui, t().viz_colors, theme::regular(13.0), theme::TEXT_DIM, 44.0);
        ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, theme::TEXT_DIM);
        for mode in VizColor::ALL {
            if kit::chip(ui, mode.label(), cfg.color_mode == mode).clicked() && cfg.color_mode != mode {
                cfg.color_mode = mode;
                changed = true;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (r, reset) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
            let fill = if reset.hovered() { theme::GLASS_HOVER } else { theme::GLASS };
            ui.painter().circle_filled(r.center(), 15.0, fill);
            icons::paint(ui.painter(), r.shrink(8.0), icons::REFRESH_CW, theme::TEXT_DIM);
            if reset.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t().viz_reset).clicked() && *cfg != VizCfg::default() {
                *cfg = VizCfg::default();
                changed = true;
            }
        });
    });
    match cfg.color_mode {
        VizColor::Solid => {
            ui.add_space(8.0);
            ui.horizontal(|ui| changed |= swatches(ui, &mut cfg.color));
        }
        VizColor::Gradient => {
            ui.add_space(10.0);
            changed |= gradient_editor(ui, cfg);
        }
        _ => {}
    }
    ui.add_space(12.0);

    // Os controles deslizantes, dois por linha.
    let mut count = cfg.count as f32;
    let count_range = *VizCfg::COUNT.start() as f32..=*VizCfg::COUNT.end() as f32;
    let col = ((width - 20.0) / 2.0).floor();
    let mut cells: Vec<(&str, &mut f32, std::ops::RangeInclusive<f32>, fn(f32) -> String)> = Vec::new();
    if style.has_count() {
        cells.push((t().viz_count, &mut count, count_range, |v| format!("{v:.0}")));
    }
    cells.push((t().viz_sensitivity, &mut cfg.sensitivity, VizCfg::SENSITIVITY, |v| format!("{v:.1}×")));
    cells.push((t().viz_opacity, &mut cfg.opacity, VizCfg::OPACITY, |v| format!("{:.0}%", v * 100.0)));
    cells.push((t().viz_thickness, &mut cfg.thickness, VizCfg::THICKNESS, |v| format!("{:.0}%", v * 100.0)));
    for pair in cells.chunks_mut(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for (label, value, range, show) in pair.iter_mut() {
                changed |= slider(ui, label, value, range.clone(), *show, col);
            }
        });
        ui.add_space(6.0);
    }
    if style.has_count() && count.round() as u32 != cfg.count {
        cfg.count = count.round() as u32;
        changed = true;
    }
    if toggle_row(ui, t().viz_glow, t().viz_glow_hint, cfg.glow, width) {
        cfg.glow = !cfg.glow;
        changed = true;
    }
    changed
}




/// As bolinhas das cores do visualizador. `true` quando mudou.
fn swatches(ui: &mut Ui, color: &mut [u8; 3]) -> bool {
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = 8.0;
    for preset in VIZ_PRESETS {
        let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
        let c = rect.center();
        ui.painter().circle_filled(c, 10.0, Color32::from_rgb(preset[0], preset[1], preset[2]));
        if *color == preset {
            ui.painter().circle_stroke(c, 11.5, Stroke::new(2.0, theme::TEXT));
        } else if response.hovered() {
            ui.painter().circle_stroke(c, 11.5, Stroke::new(1.0, Color32::from_white_alpha(90)));
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && *color != preset {
            *color = preset;
            changed = true;
        }
    }
    changed
}

/// O controle deslizante do app, numa coluna de `width`: o nome e o valor
/// em cima, a trilha fina com a bolinha embaixo. `true` quando mudou.
fn slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, show: fn(f32) -> String, width: f32) -> bool {
    let before = *value;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    let name = kit::one_line(ui, label, theme::regular(12.5), theme::TEXT_DIM, width - 50.0);
    ui.painter().galley(rect.left_top(), name, theme::TEXT_DIM);
    ui.painter().text(rect.right_top(), egui::Align2::RIGHT_TOP, show(*value), theme::medium(12.5), theme::TEXT);
    let hit = Rect::from_min_max(pos2(rect.left(), rect.top() + 18.0), rect.max);
    let response = ui.interact(hit, Id::new(("deslizante", label)), Sense::click_and_drag());
    let track = Rect::from_center_size(hit.center(), vec2(hit.width() - 14.0, 4.0));
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
    {
        *value = lo + (hi - lo) * ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
    }
    let f = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let x = track.left() + track.width() * f;
    ui.painter().rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(40));
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), CornerRadius::same(2), theme::TEXT);
    let active = response.hovered() || response.dragged();
    ui.painter().circle_filled(pos2(x, track.center().y), if active { 7.5 } else { 6.5 }, theme::TEXT);
    response.on_hover_cursor(CursorIcon::PointingHand);
    *value != before
}

/// Uma linha baixa em `width`: o nome, a dica ao lado e o interruptor na
/// ponta. `true` no clique.
fn toggle_row(ui: &mut Ui, label: &str, hint: &str, on: bool, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let title = kit::one_line(ui, label, theme::regular(13.0), theme::TEXT_DIM, 120.0);
    let title_w = title.size().x;
    ui.painter().galley(pos2(rect.left(), rect.center().y - title.size().y / 2.0), title, theme::TEXT_DIM);
    let sub = kit::one_line(ui, hint, theme::regular(12.0), theme::TEXT_FAINT, (width - title_w - 70.0).max(0.0));
    ui.painter().galley(pos2(rect.left() + title_w + 10.0, rect.center().y - sub.size().y / 2.0), sub, theme::TEXT_FAINT);
    // O interruptor, deslizando.
    let how = ui.ctx().animate_bool_with_time(Id::new(("interruptor", label)), on, 0.15);
    let track = Rect::from_center_size(pos2(rect.right() - 19.0, rect.center().y), vec2(36.0, 20.0));
    ui.painter().rect_filled(track, CornerRadius::same(10), theme::GLASS_HOVER.lerp_to_gamma(theme::TEXT, how));
    let knob = pos2(track.left() + 10.0 + 16.0 * how, track.center().y);
    ui.painter().circle_filled(knob, 7.5, Color32::WHITE.lerp_to_gamma(Color32::from_gray(20), how));
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Um seletor segmentado centralizado, de `width` por `height`: partes
/// iguais com ícone e nome, e o destaque branco deslizando até a escolhida.
/// A nova escolha, se houve clique.
fn segmented(ui: &mut Ui, id: &str, items: &[(icons::Icon, &str)], current: usize, width: f32, height: f32) -> Option<usize> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let rect = Rect::from_center_size(row.center(), vec2(width.min(row.width()), height));
    let round = (height / 2.0) as u8;
    ui.painter().rect_filled(rect, CornerRadius::same(round), Color32::from_white_alpha(10));
    let n = items.len().max(1) as f32;
    let seg = (rect.width() - 6.0) / n;
    let at = ui.ctx().animate_value_with_time(Id::new((id, "destaque")), current as f32, 0.18);
    let slot = |i: f32| Rect::from_min_size(pos2(rect.left() + 3.0 + seg * i, rect.top() + 3.0), vec2(seg, rect.height() - 6.0));
    ui.painter().rect_filled(slot(at), CornerRadius::same(round.saturating_sub(3)), theme::TEXT);
    let font_size = if height < 36.0 { 12.5 } else { 13.5 };
    let mut picked = None;
    for (i, (icon, label)) in items.iter().enumerate() {
        let r = slot(i as f32);
        let response = ui.interact(r, Id::new((id, i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        // A cor do texto acompanha o destaque enquanto ele passa.
        let cover = (1.0 - (at - i as f32).abs()).clamp(0.0, 1.0);
        let idle = if response.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        let color = idle.lerp_to_gamma(Color32::BLACK, cover);
        let font = if i == current { theme::semibold(font_size) } else { theme::medium(font_size) };
        let text = kit::one_line(ui, label, font, color, seg - 34.0);
        let icon_side = font_size + 2.0;
        let content_w = icon_side + 7.0 + text.size().x;
        let x = r.center().x - content_w / 2.0;
        let ir = Rect::from_center_size(pos2(x + icon_side / 2.0, r.center().y), vec2(icon_side, icon_side));
        icons::paint(ui.painter(), ir, icon, color);
        ui.painter().galley(pos2(ir.right() + 7.0, r.center().y - text.size().y / 2.0), text, color);
        if response.clicked() && i != current {
            picked = Some(i);
        }
    }
    picked
}

/// Degradês prontos para o visualizador.
const GRADIENTS: [([u8; 3], [u8; 3]); 6] = [
    ([255, 90, 170], [120, 200, 255]),
    ([255, 160, 50], [255, 70, 70]),
    ([90, 230, 140], [70, 110, 255]),
    ([160, 100, 255], [255, 90, 170]),
    ([255, 225, 80], [90, 230, 140]),
    ([120, 200, 255], [160, 100, 255]),
];

/// Pinta um degradê horizontal de `a` até `b` em `rect`.
fn gradient_rect(ui: &Ui, rect: Rect, a: Color32, b: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), a);
    mesh.colored_vertex(rect.left_bottom(), a);
    mesh.colored_vertex(rect.right_top(), b);
    mesh.colored_vertex(rect.right_bottom(), b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(mesh);
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// O editor do degradê: os prontos, a barra com as duas pontas (clicar numa
/// escolhe qual editar), o botão de inverter, e as cores da ponta escolhida
/// (as prontas e o seletor). `true` quando mudou.
fn gradient_editor(ui: &mut Ui, cfg: &mut VizCfg) -> bool {
    let mut changed = false;
    // Os prontos, em pílulas com o próprio degradê.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        for (a, b) in GRADIENTS {
            let (rect, response) = ui.allocate_exact_size(vec2(52.0, 24.0), Sense::click());
            gradient_rect(ui, rect, rgb(a), rgb(b));
            round_off_on(ui, rect, 12, PANEL);
            let on = cfg.color == a && cfg.color2 == b;
            if on {
                ui.painter().rect_stroke(rect.expand(3.0), CornerRadius::same(15), Stroke::new(2.0, theme::TEXT), StrokeKind::Inside);
            } else if response.hovered() {
                ui.painter().rect_stroke(rect.expand(3.0), CornerRadius::same(15), Stroke::new(1.0, Color32::from_white_alpha(90)), StrokeKind::Inside);
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).clicked() && !on {
                (cfg.color, cfg.color2) = (a, b);
                changed = true;
            }
        }
    });
    ui.add_space(12.0);

    // A barra com as duas pontas e o inverter.
    let end_id = Id::new("degrade-ponta");
    let mut end = ui.data(|d| d.get_temp::<bool>(end_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width() - 44.0, 34.0), Sense::click());
        gradient_rect(ui, rect, rgb(cfg.color), rgb(cfg.color2));
        round_off_on(ui, rect, 17, PANEL);
        for (i, color) in [cfg.color, cfg.color2].into_iter().enumerate() {
            let c = if i == 0 { pos2(rect.left() + 17.0, rect.center().y) } else { pos2(rect.right() - 17.0, rect.center().y) };
            let selected = end == (i == 1);
            ui.painter().circle_filled(c, 11.0, rgb(color));
            ui.painter().circle_stroke(c, 11.0, Stroke::new(if selected { 3.0 } else { 1.5 }, if selected { Color32::WHITE } else { Color32::from_white_alpha(120) }));
            let label = if i == 0 { t().viz_from } else { t().viz_to };
            let g = kit::one_line(ui, label, theme::medium(11.5), Color32::from_black_alpha(200), 60.0);
            let x = if i == 0 { c.x + 18.0 } else { c.x - 18.0 - g.size().x };
            ui.painter().galley(pos2(x, c.y - g.size().y / 2.0), g, Color32::BLACK);
        }
        if let Some(p) = response.interact_pointer_pos()
            && response.clicked()
        {
            end = p.x > rect.center().x;
            ui.data_mut(|d| d.insert_temp(end_id, end));
        }
        response.on_hover_cursor(CursorIcon::PointingHand);
        ui.add_space(8.0);
        let (r, swap) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
        ui.painter().circle_filled(r.center(), 17.0, if swap.hovered() { theme::GLASS_HOVER } else { theme::GLASS });
        icons::paint(ui.painter(), r.shrink(9.0), icons::REPEAT, theme::TEXT);
        if swap.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t().viz_swap).clicked() {
            std::mem::swap(&mut cfg.color, &mut cfg.color2);
            changed = true;
        }
    });
    ui.add_space(10.0);

    // As cores da ponta escolhida, e o "+" com o seletor dela.
    let open_id = Id::new("degrade-seletor");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let target = if end { &mut cfg.color2 } else { &mut cfg.color };
    ui.horizontal(|ui| {
        changed |= swatches(ui, target);
        let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
        let c = rect.center();
        if open {
            ui.painter().circle_filled(c, 10.0, rgb(*target));
            ui.painter().circle_stroke(c, 11.5, Stroke::new(2.0, theme::TEXT));
        } else {
            ui.painter().circle_filled(c, 10.0, if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS });
            icons::paint(ui.painter(), rect.shrink(7.0), icons::PLUS, theme::TEXT);
        }
        if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t().custom_color).clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(open_id, open));
        }
    });
    if open {
        ui.add_space(10.0);
        // O matiz vive à parte: do RGB ele se perde no cinza.
        let hsv_id = Id::new(("degrade-hsv", end));
        let mut hsv = ui.data(|d| d.get_temp::<[f32; 3]>(hsv_id)).unwrap_or_else(|| to_hsv(*target));
        if rgb(*target) != Color32::from(Hsva::new(hsv[0], hsv[1], hsv[2], 1.0)) {
            hsv = to_hsv(*target);
        }
        if hsv_picker_on(ui, &mut hsv, PANEL) {
            let c = Color32::from(Hsva::new(hsv[0], hsv[1], hsv[2], 1.0));
            *target = [c.r(), c.g(), c.b()];
            changed = true;
        }
        ui.data_mut(|d| d.insert_temp(hsv_id, hsv));
    }
    changed
}




/// A largura da coluna dos nomes nas abas da letra e da tradução.
const LABEL: f32 = 104.0;

/// Um cartão com título: o fundo um tom acima do diálogo, na largura toda.
fn card(ui: &mut Ui, title: &str, width: f32, add: impl FnOnce(&mut Ui, f32)) {
    if !title.is_empty() {
        ui.label(egui::RichText::new(title).color(theme::TEXT_DIM).font(theme::semibold(12.5)));
        ui.add_space(6.0);
    }
    egui::Frame::new().fill(PANEL).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(PANEL_PAD as i8, 8)).show(ui, |ui| {
        let inner = width - PANEL_PAD * 2.0;
        ui.set_width(inner);
        add(ui, inner);
    });
}

/// Um texto de ajuda embaixo do controle, alinhado com ele.
fn faint_hint(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(LABEL);
        ui.add(egui::Label::new(egui::RichText::new(text).color(theme::TEXT_FAINT).font(theme::regular(12.0))).wrap());
    });
}

/// A tradução "No meu PC": o que foi encontrado no computador (Ollama ou
/// LibreTranslate), o modelo do Ollama, e o caminho quando falta algo
/// (baixar o Ollama, baixar o modelo sugerido). `true` quando o estilo mudou.
fn local_translator(ui: &mut Ui, style: &mut crate::look::LyricsStyle) -> bool {
    use crate::local_tr;
    let status = local_tr::status(ui.ctx());
    let mut changed = false;
    let indented = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            ui.add_space(LABEL);
            add(ui);
        });
    };
    if status.checking {
        indented(ui, &mut |ui| {
            kit::spinner(ui, 12.0);
            ui.label(egui::RichText::new(t().local_checking).color(theme::TEXT_FAINT).font(theme::regular(12.0)));
        });
        return false;
    }
    let pulling = status.pull.as_ref().filter(|p| p.error.is_none());
    match &status.ollama {
        Some(models) if !models.is_empty() => {
            row(ui, t().local_model, |ui, w| {
                let auto = local_tr::pick_model(models, None).unwrap_or_default();
                let auto_label = format!("{} ({auto})", t().local_auto);
                let mut options: Vec<(Option<String>, String)> = vec![(None, auto_label)];
                options.extend(models.iter().map(|m| (Some(m.clone()), m.clone())));
                if let Some(picked) = dropdown(ui, &options, &style.local_model, w) {
                    style.local_model = picked;
                    changed = true;
                }
            });
            faint_hint(ui, t().local_using_ollama);
        }
        Some(_) if pulling.is_none() => {
            faint_hint(ui, t().local_ollama_empty);
            indented(ui, &mut |ui| {
                let label = format!("{} ({}, ~3 GB)", t().local_download, local_tr::RECOMMENDED);
                if kit::pill(ui, None, &label, true).clicked() {
                    local_tr::pull(ui.ctx(), local_tr::RECOMMENDED);
                }
            });
        }
        Some(_) => {}
        None if status.libre => faint_hint(ui, t().local_using_libre),
        None => {
            faint_hint(ui, t().local_none);
            indented(ui, &mut |ui| {
                if kit::pill(ui, Some(icons::LINK), t().local_get_ollama, true).clicked() {
                    kit::open_in_browser("https://ollama.com/download");
                }
            });
        }
    }
    if let Some(pull) = &status.pull {
        match &pull.error {
            None => indented(ui, &mut |ui| {
                kit::spinner(ui, 12.0);
                let pct = pull.progress.map(|p| format!(" {:.0}%", p * 100.0)).unwrap_or_default();
                ui.label(egui::RichText::new(format!("{} {}{pct}", t().local_downloading, pull.model)).color(theme::TEXT_DIM).font(theme::regular(12.0)));
            }),
            Some(error) => {
                faint_hint(ui, &format!("{} {error}", t().local_download_failed));
                indented(ui, &mut |ui| {
                    if kit::pill(ui, None, t().local_retry, false).clicked() {
                        local_tr::pull(ui.ctx(), &pull.model);
                    }
                });
            }
        }
    }
    changed
}

/// Uma lista que abre embaixo, como a do idioma: a nova escolha, se houve.
fn dropdown(ui: &mut Ui, options: &[(Option<String>, String)], current: &Option<String>, w: f32) -> Option<Option<String>> {
    let shown = options.iter().find(|(v, _)| v == current).or(options.first()).map_or("", |o| o.1.as_str());
    let (rect, response) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let fill = if response.hovered() { theme::GLASS_HOVER } else { Color32::from_white_alpha(10) };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let g = kit::one_line(ui, shown, theme::medium(13.5), theme::TEXT, w - 40.0);
    ui.painter().galley(pos2(rect.left() + 12.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), icons::CHEVRON_DOWN, theme::TEXT_DIM);
    let mut picked = None;
    egui::Popup::menu(&response).width(w).show(|ui| {
        ui.set_width(w);
        for (value, name) in options {
            let selected = value == current;
            let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
            if resp.hovered() || selected {
                ui.painter().rect_filled(r, CornerRadius::same(6), if selected { theme::GLASS_HOVER } else { theme::HOVER });
            }
            if selected {
                icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(14.0, 14.0)), icons::CHECK, theme::TEXT);
            }
            let g = kit::one_line(ui, name, theme::regular(13.5), theme::TEXT, w - 40.0);
            ui.painter().galley(pos2(r.left() + 30.0, r.center().y - g.size().y / 2.0), g, theme::TEXT);
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                if !selected {
                    picked = Some(value.clone());
                }
                ui.close();
            }
        }
    });
    picked
}

/// Uma linha de ajuste: o nome na coluna da esquerda, o controle no resto.
fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, f32)) {
    ui.horizontal(|ui| {
        ui.set_min_height(36.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(LABEL, 30.0), Sense::hover());
        let g = kit::one_line(ui, label, theme::regular(13.0), theme::TEXT_DIM, LABEL - 8.0);
        ui.painter().galley(pos2(r.left(), r.center().y - g.size().y / 2.0), g, theme::TEXT_DIM);
        let rest = ui.available_width();
        ui.spacing_mut().item_spacing.x = 6.0;
        add(ui, rest);
    });
}

/// Um controle deslizante numa linha: trilha e valor, na largura `width`.
fn slider_inline(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, show: fn(f32) -> String, width: f32) -> bool {
    let before = *value;
    let value_w = 48.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width - value_w, 30.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 14.0, 4.0));
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
    {
        *value = lo + (hi - lo) * ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
    }
    let f = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let x = track.left() + track.width() * f;
    ui.painter().rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(40));
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), CornerRadius::same(2), theme::TEXT);
    let active = response.hovered() || response.dragged();
    ui.painter().circle_filled(pos2(x, track.center().y), if active { 7.5 } else { 6.5 }, theme::TEXT);
    response.on_hover_cursor(CursorIcon::PointingHand);
    let (v, _) = ui.allocate_exact_size(vec2(value_w, 30.0), Sense::hover());
    ui.painter().text(v.right_center(), egui::Align2::RIGHT_CENTER, show(*value), theme::medium(12.5), theme::TEXT);
    *value != before
}

/// Escolha entre poucas opções de texto, como o seletor de seção: partes
/// iguais e o destaque branco. A nova escolha, se houve clique.
fn choice(ui: &mut Ui, id: &str, labels: &[&str], current: usize, width: f32) -> Option<usize> {
    let h = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(15), Color32::from_white_alpha(10));
    let seg = (rect.width() - 6.0) / labels.len().max(1) as f32;
    let at = ui.ctx().animate_value_with_time(Id::new((id, "destaque")), current as f32, 0.18);
    let slot = |i: f32| Rect::from_min_size(pos2(rect.left() + 3.0 + seg * i, rect.top() + 3.0), vec2(seg, h - 6.0));
    ui.painter().rect_filled(slot(at), CornerRadius::same(12), theme::TEXT);
    let mut picked = None;
    for (i, label) in labels.iter().enumerate() {
        let r = slot(i as f32);
        let resp = ui.interact(r, Id::new((id, i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let cover = (1.0 - (at - i as f32).abs()).clamp(0.0, 1.0);
        let idle = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
        let color = idle.lerp_to_gamma(Color32::BLACK, cover);
        let font = if i == current { theme::semibold(12.5) } else { theme::medium(12.5) };
        let g = kit::one_line(ui, label, font, color, seg - 10.0);
        ui.painter().galley(r.center() - g.size() / 2.0, g, color);
        if resp.clicked() && i != current {
            picked = Some(i);
        }
    }
    picked
}

/// As cores prontas e o "+", que abre embaixo o seletor de qualquer cor (o
/// quadro de tom e brilho, a faixa do matiz e o código). `true` quando mudou.
fn color_row(ui: &mut Ui, id: &str, color: &mut [u8; 3]) -> bool {
    let open_id = Id::new((id, "aberto"));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let custom = !VIZ_PRESETS.contains(color);
    let mut changed = false;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            changed |= swatches(ui, color);
            let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
            let c = rect.center();
            if custom || open {
                ui.painter().circle_filled(c, 10.0, Color32::from_rgb(color[0], color[1], color[2]));
                if custom {
                    ui.painter().circle_stroke(c, 11.5, Stroke::new(2.0, theme::TEXT));
                }
                icons::paint(ui.painter(), rect.shrink(7.0), icons::PLUS, Color32::from_white_alpha(200));
            } else {
                ui.painter().circle_filled(c, 10.0, if response.hovered() { theme::GLASS_HOVER } else { theme::GLASS });
                icons::paint(ui.painter(), rect.shrink(7.0), icons::PLUS, theme::TEXT);
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t().custom_color).clicked() {
                open = !open;
                ui.data_mut(|d| d.insert_temp(open_id, open));
            }
        });
        if open {
            ui.add_space(8.0);
            let hsv_id = Id::new((id, "hsv"));
            let mut hsv = ui.data(|d| d.get_temp::<[f32; 3]>(hsv_id)).unwrap_or_else(|| to_hsv(*color));
            if Color32::from_rgb(color[0], color[1], color[2]) != Color32::from(Hsva::new(hsv[0], hsv[1], hsv[2], 1.0)) {
                hsv = to_hsv(*color);
            }
            if hsv_picker_on(ui, &mut hsv, PANEL) {
                let c = Color32::from(Hsva::new(hsv[0], hsv[1], hsv[2], 1.0));
                *color = [c.r(), c.g(), c.b()];
                changed = true;
            }
            ui.data_mut(|d| d.insert_temp(hsv_id, hsv));
        }
    });
    changed
}

/// O seletor de fonte: o botão com o nome da escolhida e, ao clicar, a lista
/// das fontes instaladas com busca. `default`: o nome da primeira opção
/// (`None` na fonte). `true` quando mudou.
fn font_picker(ui: &mut Ui, id: &str, font: &mut Option<String>, default: &str, width: f32) -> bool {
    let mut changed = false;
    let label = font.clone().unwrap_or_else(|| default.to_owned());
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let fill = if response.hovered() { theme::GLASS_HOVER } else { Color32::from_white_alpha(10) };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let g = kit::one_line(ui, &label, theme::medium(13.5), theme::TEXT, width - 40.0);
    ui.painter().galley(pos2(rect.left() + 12.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT);
    let chevron = Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0));
    icons::paint(ui.painter(), chevron, icons::CHEVRON_DOWN, theme::TEXT_DIM);
    let search_id = Id::new((id, "busca"));
    // A lista abre maior que o botão: cada fonte escrita nela mesma.
    let menu_w = width.max(420.0);
    egui::Popup::menu(&response).width(menu_w).show(|ui| {
        ui.set_width(menu_w);
        let Some(families) = crate::fonts::families(ui.ctx()) else {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                kit::spinner(ui, 16.0);
                ui.label(egui::RichText::new(t().fonts_loading).color(theme::TEXT_DIM));
            });
            ui.add_space(10.0);
            return;
        };
        // A busca, com a lupa, e quantas fontes há.
        let mut query = ui.data(|d| d.get_temp::<String>(search_id)).unwrap_or_default();
        let (field, _) = ui.allocate_exact_size(vec2(menu_w, 38.0), Sense::hover());
        ui.painter().rect_filled(field, CornerRadius::same(8), Color32::from_white_alpha(12));
        icons::paint(ui.painter(), Rect::from_center_size(pos2(field.left() + 18.0, field.center().y), vec2(16.0, 16.0)), icons::SEARCH, theme::TEXT_DIM);
        let mut edit_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(field.left() + 34.0, field.top()), pos2(field.right() - 8.0, field.bottom()))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let edit = edit_ui.add(
            egui::TextEdit::singleline(&mut query)
                .frame(egui::Frame::NONE)
                .hint_text(egui::RichText::new(t().font_search).color(theme::TEXT_FAINT))
                .font(theme::regular(14.5))
                .desired_width(f32::INFINITY),
        );
        edit.request_focus();
        ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
        ui.add_space(8.0);

        let needle = query.to_lowercase();
        let mut options: Vec<(&str, Option<&str>)> = Vec::new();
        if needle.is_empty() {
            options.push((default, None));
        }
        options.extend(families.iter().filter(|f| needle.is_empty() || f.name.to_lowercase().contains(&needle)).map(|f| (f.name.as_str(), Some(f.name.as_str()))));
        const ROW: f32 = 46.0;
        egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, true]).show_rows(ui, ROW, options.len(), |ui, range| {
            // Só as visíveis ganham a prévia (algumas por quadro).
            let visible: Vec<&str> = options[range.clone()].iter().filter_map(|o| o.1).collect();
            crate::fonts::want_previews(ui.ctx(), &visible);
            for (label, value) in &options[range] {
                let selected = font.as_deref() == *value;
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                if resp.hovered() || selected {
                    ui.painter().rect_filled(r.shrink2(vec2(0.0, 2.0)), CornerRadius::same(8), if selected { theme::GLASS_HOVER } else { theme::HOVER });
                }
                if selected {
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(15.0, 15.0)), icons::CHECK, theme::TEXT);
                }
                // O nome na própria fonte; sem prévia (ainda chegando, ou a
                // fonte é grande demais), na fonte do app, apagado.
                let (font_id, color) = match value.and_then(|v| crate::fonts::preview(ui.ctx(), v, 19.0)) {
                    Some(f) => (f, theme::TEXT),
                    None if value.is_none() => (theme::medium(16.0), theme::TEXT),
                    None => (theme::regular(15.0), theme::TEXT_DIM),
                };
                let g = kit::one_line(ui, label, font_id, color, r.width() - 50.0);
                ui.painter().galley(pos2(r.left() + 38.0, r.center().y - g.size().y / 2.0), g, color);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    let value = value.map(str::to_owned);
                    if *font != value {
                        *font = value;
                        changed = true;
                    }
                    ui.close();
                }
            }
        });
        ui.add_space(4.0);
        ui.label(egui::RichText::new(t().fonts_count(families.len())).color(theme::TEXT_FAINT).font(theme::regular(12.0)));
    });
    changed
}

/// A aba "Letra": a amostra, o texto (fonte, tamanho, alinhamento) e a cor e
/// os efeitos (cor, desfoque, as outras linhas, romaji). `true` quando mudou.
fn lyrics_section(ui: &mut Ui, style: &mut crate::look::LyricsStyle, translate: bool, width: f32) -> bool {
    use crate::look::{LyricsStyle, TextAlign};
    let mut changed = false;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 118.0 * style.size.max(1.0)), Sense::hover());
    super::lyrics_view::preview(ui, rect, style, translate);
    ui.add_space(4.0);
    ui.label(egui::RichText::new(t().lyrics_style_hint).color(theme::TEXT_FAINT).font(theme::regular(12.0)));
    ui.add_space(12.0);
    let percent: fn(f32) -> String = |v| format!("{:.0}%", v * 100.0);
    card(ui, t().lyrics_text_group, width, |ui, _| {
        row(ui, t().lyrics_font, |ui, w| changed |= font_picker(ui, "fonte-letra", &mut style.font_family, t().font_default, w));
        row(ui, t().lyrics_size, |ui, w| changed |= slider_inline(ui, &mut style.size, LyricsStyle::SIZE, percent, w));
        row(ui, t().lyrics_align, |ui, w| {
            let labels: Vec<&str> = TextAlign::ALL.iter().map(|a| a.label()).collect();
            let current = TextAlign::ALL.iter().position(|a| *a == style.align).unwrap_or(0);
            if let Some(i) = choice(ui, "alinhamento", &labels, current, w) {
                style.align = TextAlign::ALL[i];
                changed = true;
            }
        });
    });
    ui.add_space(12.0);
    card(ui, t().lyrics_effects_group, width, |ui, inner| {
        row(ui, t().lyrics_color, |ui, _| changed |= color_row(ui, "cor-letra", &mut style.color));
        row(ui, t().lyrics_blur, |ui, w| changed |= slider_inline(ui, &mut style.blur, 0.0..=1.0, percent, w));
        row(ui, t().lyrics_dim, |ui, w| changed |= slider_inline(ui, &mut style.dim, LyricsStyle::DIM, percent, w));
        if toggle_row(ui, t().lyrics_romaji, t().lyrics_romaji_hint, style.romaji, inner) {
            style.romaji = !style.romaji;
            changed = true;
        }
    });
    changed
}

/// A aba "Tradução": ligar e desligar, a amostra, e a forma, o idioma, o
/// serviço, a fonte e a cor. `true` quando o estilo mudou (ligar e
/// desligar vai por ação).
fn translation_section(ui: &mut Ui, cx: &mut Cx, style: &mut crate::look::LyricsStyle, width: f32) -> bool {
    use crate::look::{TRANSLATION_LANGS, TranslateService, TranslationMode};
    let mut changed = false;
    card(ui, "", width, |ui, inner| {
        if toggle_row(ui, t().lyrics_translation, t().translation_hint, cx.translate_lyrics, inner) {
            cx.act(Action::ToggleTranslation);
        }
    });
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 118.0 * style.size.max(1.0)), Sense::hover());
    super::lyrics_view::preview(ui, rect, style, cx.translate_lyrics);
    ui.add_space(12.0);
    card(ui, "", width, |ui, _| {
        row(ui, t().translation_shape, |ui, w| {
            let modes = [TranslationMode::Below, TranslationMode::Above, TranslationMode::Replace];
            let current = modes.iter().position(|m| *m == style.translation).unwrap_or(0);
            if let Some(i) = choice(ui, "forma", &[t().translation_below, t().translation_above, t().translation_replace], current, w) {
                style.translation = modes[i];
                changed = true;
            }
        });
        row(ui, t().translation_language, |ui, w| {
            // Uma lista, como a da fonte: o idioma do app e os outros.
            let mut options: Vec<(Option<String>, &str)> = vec![(None, t().translation_app_language)];
            options.extend(TRANSLATION_LANGS.iter().map(|(code, name)| (Some((*code).to_owned()), *name)));
            let current = options.iter().find(|(code, _)| *code == style.translation_lang).map_or(t().translation_app_language, |o| o.1);
            let (rect, response) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
            let response = response.on_hover_cursor(CursorIcon::PointingHand);
            let fill = if response.hovered() { theme::GLASS_HOVER } else { Color32::from_white_alpha(10) };
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
            let g = kit::one_line(ui, current, theme::medium(13.5), theme::TEXT, w - 40.0);
            ui.painter().galley(pos2(rect.left() + 12.0, rect.center().y - g.size().y / 2.0), g, theme::TEXT);
            icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(16.0, 16.0)), icons::CHEVRON_DOWN, theme::TEXT_DIM);
            egui::Popup::menu(&response).width(w).show(|ui| {
                ui.set_width(w);
                for (code, name) in options {
                    let selected = style.translation_lang == code;
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                    if resp.hovered() || selected {
                        ui.painter().rect_filled(r, CornerRadius::same(6), if selected { theme::GLASS_HOVER } else { theme::HOVER });
                    }
                    if selected {
                        icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(14.0, 14.0)), icons::CHECK, theme::TEXT);
                    }
                    ui.painter().text(pos2(r.left() + 30.0, r.center().y), egui::Align2::LEFT_CENTER, name, theme::regular(13.5), theme::TEXT);
                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        if !selected {
                            style.translation_lang = code;
                            changed = true;
                        }
                        ui.close();
                    }
                }
            });
        });
        row(ui, t().translation_method, |ui, w| {
            let labels: Vec<&str> = TranslateService::ALL.iter().map(|s| s.label()).collect();
            let current = TranslateService::ALL.iter().position(|s| *s == style.service).unwrap_or(0);
            if let Some(i) = choice(ui, "metodo", &labels, current, w) {
                style.service = TranslateService::ALL[i];
                changed = true;
            }
        });
        if style.service == TranslateService::Local {
            changed |= local_translator(ui, style);
        } else {
            faint_hint(ui, style.service.hint());
        }
        ui.add_space(4.0);
        row(ui, t().translation_font, |ui, w| changed |= font_picker(ui, "fonte-traducao", &mut style.translation_font, t().translation_same_font, w));
        row(ui, t().translation_color, |ui, _| changed |= color_row(ui, "cor-traducao", &mut style.translation_color));
    });
    changed
}
