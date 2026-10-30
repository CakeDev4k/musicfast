//! A letra sincronizada, como a da extensão Better Lyrics: a linha cantada
//! acende palavra por palavra e fica no terço de cima; as outras ficam
//! apagadas. A roda do mouse solta a letra por uns segundos (ou até o botão
//! "Sincronizar"), e clicar numa linha pula a música para ela.

use std::sync::Arc;

use egui::text::CCursor;
use egui::{Align2, Color32, CornerRadius, CursorIcon, Galley, Id, Rect, Sense, Ui, pos2, vec2};
use musicfast_player::Command;

use super::theme;
use super::{Action, Cx};
use crate::lyrics::{Line, Song};

/// A linha acende um pouco antes da voz, como na extensão: o olho chega
/// antes do ouvido.
const LEAD: f64 = 0.15;
/// Quanto tempo a letra fica onde a roda do mouse a deixou.
const MANUAL_FOR: f64 = 3.5;

#[derive(Clone, Copy, Default)]
struct Scroll {
    offset: f32,
    /// Rolada à mão: até quando ela fica parada ali.
    manual_until: Option<f64>,
    started: bool,
}

pub fn show(ui: &mut Ui, cx: &mut Cx, song: &Song, key: &str, position: f64, big: bool) {
    let (area, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let painter = ui.painter_at(area);
    let id = Id::new(("letra-sinc", key, big));
    let now = ui.input(|i| i.time);
    let t = position + LEAD;

    let font = if big { theme::bold(28.0) } else { theme::bold(20.0) };
    let gap_h = if big { 44.0 } else { 34.0 };
    let spacing = if big { 18.0 } else { 12.0 };
    let pad = 10.0;
    let wrap = (area.width() - pad * 2.0).max(60.0);

    // As linhas com o tempo, uma galley cada (o egui guarda o layout entre
    // quadros, então isto é barato).
    let mut layout: Vec<(f32, f32, Option<Arc<Galley>>)> = Vec::with_capacity(song.lines.len());
    let mut y = 0.0;
    for line in &song.lines {
        let (h, galley) = if line.is_gap() {
            (gap_h, None)
        } else {
            let g = painter.layout(line.text.clone(), font.clone(), Color32::WHITE, wrap);
            (g.size().y, Some(g))
        };
        layout.push((y, h, galley));
        y += h + spacing;
    }
    let source_y = y + 12.0;
    let total = source_y + 30.0;

    let active = song.lines.iter().rposition(|l| l.start <= t);

    // Para onde rolar: a linha cantada no terço de cima.
    let anchor = area.height() * 0.32;
    let auto_target = match active {
        Some(i) => layout[i].0 + layout[i].1 / 2.0 - anchor,
        None => -anchor + layout.first().map_or(0.0, |l| l.1 / 2.0),
    };
    let mut st: Scroll = ui.ctx().memory(|m| m.data.get_temp(id).unwrap_or_default());
    if !st.started {
        st = Scroll { offset: auto_target, manual_until: None, started: true };
    }
    let wheel = if response.hovered() { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
    if wheel != 0.0 {
        ui.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
        st.offset = (st.offset - wheel).clamp(-area.height() * 0.5, (total - area.height() * 0.5).max(0.0));
        st.manual_until = Some(now + MANUAL_FOR);
    }
    if st.manual_until.is_some_and(|until| now > until) {
        st.manual_until = None;
    }
    let mut animating = false;
    if st.manual_until.is_none() {
        // Aproxima com amortecimento: suave e sem depender de quando começou.
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1);
        let k = 1.0 - (-dt * 7.0).exp();
        st.offset += (auto_target - st.offset) * k;
        if (auto_target - st.offset).abs() < 0.5 {
            st.offset = auto_target;
        } else {
            animating = true;
        }
    }

    let top = area.top() - st.offset;
    let mut seek = None;
    for (i, line) in song.lines.iter().enumerate() {
        let (ly, h, galley) = &layout[i];
        let rect = Rect::from_min_size(pos2(area.left(), top + ly), vec2(area.width(), *h));
        if !area.intersects(rect.expand(spacing)) {
            continue;
        }
        // Some nas bordas, em vez de cortar seco.
        let fade = edge_fade(rect.center().y, area);
        let hit = ui.interact(rect.intersect(area), id.with(i), Sense::click());
        let hovered = hit.hovered();
        if hovered {
            painter.rect_filled(rect.expand2(vec2(-2.0, spacing * 0.35)), CornerRadius::same(theme::RADIUS), Color32::from_white_alpha(8));
        }
        if hit.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            seek = Some(line.start);
        }
        let is_active = active == Some(i) && t < line.end + 0.5;
        match galley {
            None => dots(&painter, rect, line, t, is_active, fade, now),
            Some(g) => {
                let pos = pos2(area.left() + pad, rect.top());
                let rest_alpha = if hovered { 0.75 } else { 0.32 };
                if is_active {
                    painter.galley_with_override_text_color(pos, g.clone(), white(0.42 * fade));
                    paint_sung(&painter, g, pos, line, t, area, white(fade));
                } else {
                    painter.galley_with_override_text_color(pos, g.clone(), white(rest_alpha * fade));
                }
            }
        }
    }
    if !song.source.is_empty() {
        painter.text(pos2(area.left() + pad, top + source_y), Align2::LEFT_TOP, &song.source, theme::regular(12.0), theme::TEXT_FAINT);
    }

    // Rolada à mão: o botão que volta para a linha cantada.
    if st.manual_until.is_some() {
        let r = Rect::from_center_size(pos2(area.center().x, area.bottom() - 30.0), vec2(124.0, 34.0));
        let b = ui.interact(r, id.with("sincronizar"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let fill = if b.hovered() { Color32::from_gray(225) } else { theme::TEXT };
        painter.rect_filled(r, CornerRadius::same(17), fill);
        painter.text(r.center(), Align2::CENTER_CENTER, musicfast_i18n::t().sync, theme::medium(14.0), Color32::BLACK);
        if b.clicked() {
            st.manual_until = None;
        }
        animating = true;
    }
    if let Some(at) = seek {
        st.manual_until = None;
        cx.act(Action::Cmd(Command::Seek(at)));
    }
    ui.ctx().memory_mut(|m| m.data.insert_temp(id, st));

    // As palavras acendem com o tempo: só pede quadros enquanto toca (ou
    // enquanto a rolagem anda), e não mais que ~60 por segundo.
    if animating {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    } else if cx.playing {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
}

fn white(alpha: f32) -> Color32 {
    Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

fn edge_fade(y: f32, area: Rect) -> f32 {
    let band = (area.height() * 0.12).max(24.0);
    let top = ((y - area.top()) / band).clamp(0.0, 1.0);
    let bottom = ((area.bottom() - y) / band).clamp(0.0, 1.0);
    top.min(bottom)
}

/// A parte já cantada da linha em branco por cima da apagada: as linhas
/// visuais de cima inteiras, e a atual até onde a voz chegou.
fn paint_sung(painter: &egui::Painter, g: &Arc<Galley>, pos: egui::Pos2, line: &Line, t: f64, area: Rect, color: Color32) {
    let chars = line.text.chars().count();
    let mut filled = 0.0f64;
    for w in &line.words {
        if t >= w.end {
            filled = (w.from + w.len) as f64;
        } else {
            if t > w.start {
                let span = (w.end - w.start).max(0.01);
                filled = w.from as f64 + w.len as f64 * ((t - w.start) / span);
            }
            break;
        }
    }
    if filled <= 0.0 {
        return;
    }
    if filled >= chars as f64 {
        painter.galley_with_override_text_color(pos, g.clone(), color);
        return;
    }
    let at = filled.floor() as usize;
    let frac = (filled - at as f64) as f32;
    let a = g.pos_from_cursor(CCursor::new(at));
    let b = g.pos_from_cursor(CCursor::new(at + 1));
    let x = if (a.min.y - b.min.y).abs() < 1.0 { egui::lerp(a.min.x..=b.min.x, frac) } else { a.min.x };
    let above = Rect::from_min_max(pos2(area.left(), pos.y - 4.0), pos2(area.right(), pos.y + a.min.y));
    let row = Rect::from_min_max(pos2(area.left(), pos.y + a.min.y), pos2(pos.x + x, pos.y + a.max.y + 4.0));
    for clip in [above, row] {
        let clip = clip.intersect(area);
        if clip.is_positive() {
            painter.with_clip_rect(clip).galley_with_override_text_color(pos, g.clone(), color);
        }
    }
}

/// O trecho instrumental: três pontos que acendem um a um ao longo dele e
/// respiram de leve.
fn dots(painter: &egui::Painter, rect: Rect, line: &Line, t: f64, active: bool, fade: f32, now: f64) {
    let r = rect.height() * 0.13;
    let progress = if active { ((t - line.start) / (line.end - line.start).max(0.1)).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let breathe = if active { 1.0 + 0.12 * (now as f32 * 3.0).sin() } else { 1.0 };
    let x0 = rect.left() + 10.0 + r;
    for i in 0..3 {
        let lit = (progress * 3.0 - i as f32).clamp(0.0, 1.0);
        let alpha = if active { 0.35 + 0.65 * lit } else { 0.22 };
        let c = pos2(x0 + i as f32 * r * 3.2, rect.center().y);
        painter.circle_filled(c, r * breathe, white(alpha * fade));
    }
}
