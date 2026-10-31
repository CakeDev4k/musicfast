//! O painel da direita: a fila (com a atual em destaque) ou a letra.

use egui::{Align2, Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Ui, pos2, vec2};
use musicfast_i18n::t;
use musicfast_player::{Command, Snapshot};

use super::icons;
use super::kit;
use super::page::track_menu;
use super::theme;
use super::{Action, Cx};

pub enum LyricsState {
    Loading,
    Ready(Option<crate::lyrics::Song>),
    Failed(String),
}

fn title(ui: &mut Ui, text: &str) {
    ui.add_space(16.0);
    ui.label(egui::RichText::new(text).font(theme::bold(18.0)));
    ui.add_space(8.0);
}

pub fn queue(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot) {
    title(ui, t().queue);
    queue_list(ui, cx, snap, false);
}

/// A fila sem o título (também a aba "A seguir" do player).
///
/// `big` é a do player aberto, como a do site: linhas mais altas separadas
/// por um traço, a atual com fundo cinza e o play sobre a capa.
pub fn queue_list(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, big: bool) {
    let row_h = if big { 58.0 } else { 56.0 };
    if snap.queue.is_empty() {
        ui.label(egui::RichText::new(t().queue_empty).color(theme::TEXT_DIM));
        return;
    }
    let current = snap.index;
    let mut scroll = egui::ScrollArea::vertical().id_salt("fila").auto_shrink([false, false]);
    // Ao trocar de música, a atual aparece no topo.
    let changed = ui.ctx().memory_mut(|m| {
        let last = m.data.get_temp_mut_or_default::<Option<usize>>(Id::new("fila-atual"));
        let changed = *last != current;
        *last = current;
        changed
    });
    if changed
        && let Some(i) = current
    {
        scroll = scroll.vertical_scroll_offset((i as f32 * row_h - row_h).max(0.0));
    }
    scroll.show_rows(ui, row_h, snap.queue.len(), |ui, range| {
        if big {
            ui.spacing_mut().item_spacing.y = 0.0;
        }
        for i in range {
            let track = &snap.queue[i];
            let width = ui.available_width();
            let (rect, response) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
            let is_current = Some(i) == current;
            let hovered = response.hovered() || response.context_menu_opened();
            if big {
                let fill = if is_current {
                    theme::HOVER
                } else if hovered {
                    theme::SURFACE
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
                if i + 1 < snap.queue.len() && !is_current && current != Some(i + 1) {
                    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, theme::BORDER));
                }
            } else if is_current {
                ui.painter().rect_filled(rect, CornerRadius::same(6), theme::HOVER);
            } else if hovered {
                ui.painter().rect_filled(rect, CornerRadius::same(6), theme::SURFACE);
            }
            if hovered {
                cx.act(Action::Warm(track.video_id.clone()));
            }
            let art = Rect::from_min_size(pos2(rect.left() + if big { 8.0 } else { 6.0 }, rect.center().y - 20.0), vec2(40.0, 40.0));
            cx.covers.paint(ui, art, &track.thumbs, if big { 2 } else { 4 });
            if big && is_current {
                ui.painter().rect_filled(art, CornerRadius::same(2), Color32::from_black_alpha(150));
                icons::paint_filled(ui.painter(), art.shrink(11.0).translate(vec2(1.0, 0.0)), icons::PLAY, theme::TEXT);
            }
            // As que já tocaram ficam apagadas.
            let past = current.is_some_and(|c| i < c);
            let color = if big && is_current {
                theme::TEXT
            } else if is_current {
                theme::accent()
            } else if past {
                theme::TEXT_FAINT
            } else {
                theme::TEXT
            };
            let x = art.right() + 12.0;
            let w = rect.right() - x - 52.0;
            let (title_font, sub_font, sub_color) = if big {
                (theme::medium(15.0), theme::regular(14.0), theme::TEXT_SUB)
            } else {
                (theme::medium(14.0), theme::regular(12.5), theme::TEXT_DIM)
            };
            let t = kit::one_line(ui, &track.title, title_font, color, w);
            ui.painter().galley(pos2(x, rect.center().y - if big { 20.0 } else { 18.0 }), t, color);
            let a = kit::one_line(ui, &track.artist_names(), sub_font, sub_color, w);
            ui.painter().galley(pos2(x, rect.center().y + 2.0), a, sub_color);

            let remove = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(30.0, 30.0));
            let mut removed = false;
            if hovered && !is_current {
                let r = ui.interact(remove, Id::new(("tirar", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                kit::paint_icon_button(ui, remove, &r, icons::X, theme::TEXT_DIM);
                if r.on_hover_text(musicfast_i18n::t().remove_from_queue).clicked() {
                    cx.act(Action::Cmd(Command::Remove(i)));
                    removed = true;
                }
            } else if let Some(d) = track.duration_secs {
                ui.painter().text(
                    pos2(rect.right() - 10.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    kit::time(d as f64),
                    if big { theme::regular(14.0) } else { theme::regular(12.0) },
                    if big { theme::TEXT_SUB } else { theme::TEXT_FAINT },
                );
            }
            if !removed && response.clicked() && !is_current {
                cx.act(Action::Cmd(Command::Jump(i)));
            }
            response.on_hover_cursor(CursorIcon::PointingHand).context_menu(|ui| track_menu(ui, cx, track));
        }
    });
}

pub fn lyrics(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, state: Option<&LyricsState>, position: f64) {
    title(ui, t().lyrics);
    lyrics_body(ui, cx, snap, state, position, false);
}

/// A letra: sincronizada quando a fonte tem o tempo das linhas, senão a
/// corrida. `big` é a da tela do player.
pub fn lyrics_body(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, state: Option<&LyricsState>, position: f64, big: bool) {
    let Some(track) = snap.current() else {
        ui.label(egui::RichText::new(t().play_to_see_lyrics).color(theme::TEXT_DIM));
        return;
    };
    match state {
        None | Some(LyricsState::Loading) => kit::spinner(ui, 20.0),
        Some(LyricsState::Failed(error)) => {
            ui.label(egui::RichText::new(error).color(theme::TEXT_DIM));
        }
        Some(LyricsState::Ready(None)) => {
            ui.label(egui::RichText::new(t().no_lyrics).color(theme::TEXT_DIM));
        }
        Some(LyricsState::Ready(Some(lyrics))) if !lyrics.lines.is_empty() => {
            ui.add_space(8.0);
            super::lyrics_view::show(ui, cx, lyrics, &track.video_id, position, big);
        }
        Some(LyricsState::Ready(Some(lyrics))) => {
            let font = if big { theme::semibold(20.0) } else { theme::medium(16.0) };
            // A letra corrida; "Outra letra" no canto só com indício de outra.
            let area = ui.available_rect_before_wrap();
            egui::ScrollArea::vertical().id_salt("letra").auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(t().lyrics_not_synced).font(theme::regular(12.5)).color(theme::TEXT_FAINT));
                ui.add_space(10.0);
                ui.add(egui::Label::new(egui::RichText::new(&lyrics.plain).font(font).color(Color32::from_gray(230))).wrap());
                if !lyrics.source.is_empty() {
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(&lyrics.source).font(theme::regular(12.0)).color(theme::TEXT_FAINT));
                }
                ui.add_space(16.0);
            });
            if lyrics.doubtful && ui.rect_contains_pointer(area) {
                super::lyrics_view::other_button(ui, cx, area, &track.video_id);
            }
        }
    }
}
