//! "Tocando agora": a tela que a barra abre, como a do site. A capa grande à
//! esquerda e, à direita, as abas A seguir, Letra e Relacionados.

use egui::{Align2, CornerRadius, CursorIcon, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use musicfast_player::Snapshot;

use super::side::{self, LyricsState};
use super::theme;
use super::{Cx, PageState, View, page};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    UpNext,
    Lyrics,
    Related,
}

impl Tab {
    const ALL: [Self; 3] = [Self::UpNext, Self::Lyrics, Self::Related];

    fn label(self) -> &'static str {
        match self {
            Self::UpNext => "A seguir",
            Self::Lyrics => "Letra",
            Self::Related => "Relacionados",
        }
    }
}

pub fn show(ui: &mut Ui, cx: &mut Cx, snap: &Snapshot, position: f64, tab: &mut Tab, lyrics: Option<&LyricsState>, related: Option<&PageState>) {
    // Só o que sobra abaixo da barra de busca.
    let max = ui.max_rect();
    let full = Rect::from_min_max(pos2(max.left(), ui.cursor().top()), max.max);
    let Some(track) = snap.current() else { return };
    // Como no site: a capa à esquerda e as abas numa coluna à direita; na
    // janela estreita, a capa em cima e as abas embaixo.
    let wide = full.width() >= 860.0;
    let pad = if wide { 32.0 } else { 24.0 };
    let area = full.shrink2(vec2(pad, 0.0));
    let (art_box, right) = if wide {
        let right_w = (area.width() * 0.42).clamp(380.0, 640.0);
        let right = Rect::from_min_max(pos2(area.right() - right_w, area.top() + 8.0), pos2(area.right(), area.bottom() - 16.0));
        let left = Rect::from_min_max(pos2(area.left(), area.top() + 16.0), pos2(right.left() - pad * 2.0, area.bottom() - 32.0));
        (Some(left), right)
    } else if area.height() >= 520.0 {
        let art_h = (area.height() * 0.38).min(320.0);
        let top = Rect::from_min_size(pos2(area.left(), area.top() + 8.0), vec2(area.width(), art_h));
        let right_w = area.width().min(640.0);
        let right = Rect::from_min_max(pos2(area.center().x - right_w / 2.0, top.bottom() + 16.0), pos2(area.center().x + right_w / 2.0, area.bottom() - 8.0));
        (Some(top), right)
    } else {
        let right_w = area.width().min(640.0);
        (None, Rect::from_min_max(pos2(area.center().x - right_w / 2.0, area.top() + 8.0), pos2(area.center().x + right_w / 2.0, area.bottom() - 8.0)))
    };
    if let Some(left) = art_box {
        // A capa quadrada no meio do espaço, com folga em volta como no site
        // (não a maior que couber), e uma sombra que a destaca do preto.
        let side_len = (left.width().min(left.height()) * if wide { 0.72 } else { 1.0 }).clamp(80.0, 640.0);
        let art = Rect::from_center_size(left.center(), vec2(side_len, side_len));
        let shadow = egui::Shadow { offset: [0, 12], blur: 48, spread: 0, color: egui::Color32::from_black_alpha(200) };
        ui.painter().add(shadow.as_shape(art, CornerRadius::same(theme::RADIUS)));
        cx.covers.paint(ui, art, &track.thumbs, theme::RADIUS);
    }

    let mut inner = ui.new_child(UiBuilder::new().max_rect(right).layout(egui::Layout::top_down(egui::Align::Min)));
    let ui = &mut inner;
    // As abas, com o traço embaixo da escolhida.
    let (bar, _) = ui.allocate_exact_size(vec2(right.width(), 48.0), Sense::hover());
    let w = bar.width() / Tab::ALL.len() as f32;
    for (i, t) in Tab::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(bar.left() + w * i as f32, bar.top()), vec2(w, bar.height()));
        let resp = ui.interact(r, Id::new(("aba-player", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let selected = *tab == t;
        let color = if selected || resp.hovered() { theme::TEXT } else { theme::TEXT_FAINT };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, t.label().to_uppercase(), theme::semibold(14.0), color);
        if selected {
            let line = Rect::from_min_max(pos2(r.left(), r.bottom() - 2.0), r.max);
            ui.painter().rect_filled(line, CornerRadius::ZERO, theme::TEXT);
        }
        if resp.clicked() {
            *tab = t;
        }
    }
    ui.painter().hline(bar.x_range(), bar.bottom(), egui::Stroke::new(1.0, theme::BORDER));
    ui.add_space(4.0);
    match tab {
        Tab::UpNext => {
            // O cabeçalho do site. De onde a fila veio o app não guarda; só
            // quantas músicas ela tem.
            ui.add_space(10.0);
            let n = snap.queue.len();
            let count = format!("{n} {}", if n == 1 { "música na fila" } else { "músicas na fila" });
            ui.label(egui::RichText::new("Tocando neste dispositivo").font(theme::regular(12.5)).color(theme::TEXT_DIM));
            ui.label(egui::RichText::new(count).font(theme::semibold(14.0)));
            ui.add_space(12.0);
            side::queue_list(ui, cx, snap, true)
        }
        Tab::Lyrics => side::lyrics_body(ui, cx, snap, lyrics, position, true),
        Tab::Related => match related {
            Some(state) => page::show(ui, cx, &View::browse("relacionados", None), state),
            None => {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Nada relacionado a esta música.").color(theme::TEXT_DIM));
            }
        },
    }
}
