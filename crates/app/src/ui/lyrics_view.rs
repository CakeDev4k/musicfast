//! A letra sincronizada, como a da extensão Better Lyrics: a linha cantada
//! acende palavra por palavra e fica no terço de cima; as outras ficam
//! apagadas. A roda do mouse solta a letra por uns segundos (ou até o botão
//! "Sincronizar"), e clicar numa linha pula a música para ela. Debaixo da
//! linha, o romaji quando há; no canto, com o mouse em cima, "Outra letra"
//! (só quando há indício de que exista outra).

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

/// Uma linha posta na tela: onde começa, a altura, o texto, e embaixo dele
/// o romaji e a tradução.
type Laid = (f32, f32, Option<Arc<Galley>>, Option<Arc<Galley>>, Option<Arc<Galley>>);

/// O texto quebrado em `wrap`, encostado no lado `align` do x onde a galley
/// é desenhada (à direita dele, em volta dele, ou à esquerda dele).
fn lay(painter: &egui::Painter, text: &str, font: egui::FontId, wrap: f32, align: crate::look::TextAlign) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), font, Color32::WHITE, wrap);
    job.halign = align.egui();
    painter.layout_job(job)
}

/// O x de onde as galleys saem em `rect`, conforme o alinhamento.
fn anchor_x(rect: Rect, pad: f32, align: crate::look::TextAlign) -> f32 {
    match align {
        crate::look::TextAlign::Left => rect.left() + pad,
        crate::look::TextAlign::Center => rect.center().x,
        crate::look::TextAlign::Right => rect.right() - pad,
    }
}

pub fn show(ui: &mut Ui, cx: &mut Cx, song: &Song, key: &str, position: f64, big: bool) {
    let (area, response) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
    let painter = ui.painter_at(area);
    let id = Id::new(("letra-sinc", key, big));
    let now = ui.input(|i| i.time);
    // O que se ouve agora (a posição menos a latência do fone), um pouco
    // adiantado para a linha acender quando o cantor começa.
    let t = (position - cx.latency).max(0.0) + LEAD;
    let style = cx.lyrics_style;

    let scale = style.size.clamp(*crate::look::LyricsStyle::SIZE.start(), *crate::look::LyricsStyle::SIZE.end());
    let font = crate::look::lyrics_font(ui.ctx(), style, if big { 28.0 } else { 20.0 } * scale);
    let gap_h = if big { 44.0 } else { 34.0 };
    let spacing = if big { 18.0 } else { 12.0 } * scale.max(1.0);
    let pad = 10.0;
    let wrap = (area.width() - pad * 2.0).max(60.0);
    // Onde as galleys são desenhadas: na margem, ou no meio (centradas).
    let x0 = anchor_x(area, pad, style.align);
    let replace = cx.translate_lyrics && style.translation == crate::look::TranslationMode::Replace;
    let above = style.translation == crate::look::TranslationMode::Above;
    let [r, g, b] = style.translation_color;
    let trans_color = Color32::from_rgb(r, g, b);
    let [r, g, b] = style.color;
    let sung_color = Color32::from_rgb(r, g, b);

    // As linhas com o tempo, uma galley cada (o egui guarda o layout entre
    // quadros, então isto é barato). "Só a tradução" põe a tradução no lugar
    // da linha.
    let roma_font = if big { theme::medium(17.0) } else { theme::medium(13.5) };
    let trans_font = crate::look::translation_font(ui.ctx(), style, if big { 17.0 } else { 13.5 } * scale);
    let mut layout: Vec<Laid> = Vec::with_capacity(song.lines.len());
    let mut y = 0.0;
    for line in &song.lines {
        let (h, galley, roma, trans) = if line.is_gap() {
            (gap_h, None, None, None)
        } else {
            let translation = line.translation.as_ref().filter(|tr| cx.translate_lyrics && !tr.trim().is_empty());
            let main = match translation {
                Some(tr) if replace => tr.as_str(),
                _ => line.text.as_str(),
            };
            let g = lay(&painter, main, font.clone(), wrap, style.align);
            let r = line
                .romaji
                .as_ref()
                .filter(|r| style.romaji && !replace && !r.trim().is_empty())
                .map(|r| lay(&painter, r, roma_font.clone(), wrap, style.align));
            let tr = translation.filter(|_| !replace).map(|tr| lay(&painter, tr, trans_font.clone(), wrap, style.align));
            let h = g.size().y + r.as_ref().map_or(0.0, |r| r.size().y + 3.0) + tr.as_ref().map_or(0.0, |tr| tr.size().y + 3.0);
            (h, Some(g), r, tr)
        };
        layout.push((y, h, galley, roma, trans));
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
        let (ly, h, galley, roma, trans) = &layout[i];
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
        // O desfoque cresce com a distância da linha cantada; a sob o mouse
        // fica nítida para dar para ler.
        let distance = active.map_or(3.0, |a| (i as f32 - a as f32).abs().min(4.0));
        let blur = if is_active || hovered { 0.0 } else { style.blur * (1.0 + 1.1 * distance) * if big { 1.3 } else { 1.0 } };
        match galley {
            None => dots(&painter, rect, line, t, is_active, fade, now, x0, style.align),
            Some(g) => {
                let rest_alpha = if hovered { 0.75 } else { style.dim };
                let trans_alpha = if is_active { 0.85 } else { rest_alpha * 0.9 };
                // "Em cima": a tradução primeiro, a original logo abaixo.
                let mut pos = pos2(x0, rect.top());
                if let Some(tr) = trans.as_ref().filter(|_| above) {
                    soft(&painter, pos, tr, trans_color.gamma_multiply(trans_alpha * fade), blur);
                    pos.y += tr.size().y + 3.0;
                }
                if is_active && !replace {
                    painter.galley_with_override_text_color(pos, g.clone(), white(0.42 * fade));
                    paint_sung(&painter, g, pos, line, t, area, sung_color.gamma_multiply(fade));
                } else if is_active {
                    // A tradução no lugar não tem o tempo das palavras: acende inteira.
                    painter.galley_with_override_text_color(pos, g.clone(), sung_color.gamma_multiply(fade));
                } else {
                    soft(&painter, pos, g, white(rest_alpha * fade), blur);
                }
                let mut below = g.size().y + 3.0;
                if let Some(r) = roma {
                    let alpha = if is_active { 0.7 } else { rest_alpha * 0.8 };
                    soft(&painter, pos + vec2(0.0, below), r, white(alpha * fade), blur);
                    below += r.size().y + 3.0;
                }
                if let Some(tr) = trans.as_ref().filter(|_| !above) {
                    soft(&painter, pos + vec2(0.0, below), tr, trans_color.gamma_multiply(trans_alpha * fade), blur);
                }
            }
        }
    }
    let translated = cx.translate_lyrics && song.lines.iter().any(|l| l.translation.is_some());
    if !song.source.is_empty() || translated {
        let mut footer = song.source.clone();
        if translated {
            if !footer.is_empty() {
                footer.push_str(" · ");
            }
            // O serviço que traduziu de fato (o outro, quando o escolhido falhou).
            let by = crate::look::TranslateService::ALL.into_iter().find(|s| song.translated_by.as_deref() == Some(s.key())).unwrap_or(style.service);
            footer.push_str(&format!("{}: {}", musicfast_i18n::t().lyrics_translation, by.label()));
        }
        let align = match style.align {
            crate::look::TextAlign::Left => Align2::LEFT_TOP,
            crate::look::TextAlign::Center => Align2::CENTER_TOP,
            crate::look::TextAlign::Right => Align2::RIGHT_TOP,
        };
        painter.text(pos2(x0, top + source_y), align, footer, theme::regular(12.0), theme::TEXT_FAINT);
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
    if ui.rect_contains_pointer(area) {
        corner(ui, cx, area, key, song);
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

/// Os botões do canto de cima, com o mouse em cima: "Tradução" (na letra de
/// outro idioma, ou ainda não detectado com a tradução desligada) e "Outra
/// letra" (só com indício de que existe outra).
fn corner(ui: &mut Ui, cx: &mut Cx, area: Rect, video_id: &str, song: &Song) {
    let t = musicfast_i18n::t();
    let target = cx.lyrics_style.target();
    let target = target.as_str();
    let mut right = area.right() - 8.0;
    if song.doubtful {
        let (left, clicked) = corner_pill(ui, area, right, video_id, "outra", super::icons::REFRESH_CW, t.lyrics_other, false, t.lyrics_other);
        if clicked {
            cx.act(Action::OtherLyrics(video_id.to_owned()));
        }
        right = left - 6.0;
    }
    // O estilo da letra (abre o "Personalizar" na aba da letra).
    let (left, clicked) = corner_pill(ui, area, right, video_id, "estilo", super::icons::PALETTE, "", false, t.lyrics_style_button);
    if clicked {
        cx.act(Action::PersonalizeLyrics);
    }
    right = left - 6.0;
    let offer = song.foreign_to(target) || (!cx.translate_lyrics && song.lang.is_none());
    if offer && !song.lines.is_empty() {
        let tip = if cx.translate_lyrics { t.lyrics_translation_hide } else { t.lyrics_translation_show };
        let (_, clicked) = corner_pill(ui, area, right, video_id, "traducao", super::icons::GLOBE, t.lyrics_translation, cx.translate_lyrics, tip);
        if clicked {
            cx.act(Action::ToggleTranslation);
        }
    }
}

/// "Outra letra", no canto de cima: só quando a letra tem indício de que
/// existe outra (`Song::doubtful`). Para a letra corrida.
pub fn other_button(ui: &mut Ui, cx: &mut Cx, area: Rect, video_id: &str) {
    let t = musicfast_i18n::t();
    let (_, clicked) = corner_pill(ui, area, area.right() - 8.0, video_id, "outra", super::icons::REFRESH_CW, t.lyrics_other, false, t.lyrics_other);
    if clicked {
        cx.act(Action::OtherLyrics(video_id.to_owned()));
    }
}

/// Uma pílula do canto, com a borda direita em `right`; acesa (`on`) como um
/// botão ligado. Na coluna estreita, só o ícone. Devolve a borda esquerda e
/// se foi clicada.
#[allow(clippy::too_many_arguments)]
fn corner_pill(ui: &mut Ui, area: Rect, right: f32, video_id: &str, key: &str, icon: super::icons::Icon, label: &str, on: bool, tip: &str) -> (f32, bool) {
    let h = 30.0;
    let narrow = area.width() < 320.0 || label.is_empty();
    let w = if narrow { 34.0 } else { 34.0 + ui.painter().layout_no_wrap(label.to_owned(), theme::medium(12.5), theme::TEXT).size().x + 4.0 };
    let r = Rect::from_min_size(pos2(right - w, area.top() + 8.0), vec2(w, h));
    let resp = ui.interact(r, Id::new((key, video_id)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let (fill, fg) = match (on, resp.hovered()) {
        (true, true) => (Color32::from_gray(225), Color32::BLACK),
        (true, false) => (theme::TEXT, Color32::BLACK),
        (false, true) => (Color32::from_rgba_unmultiplied(60, 60, 60, 220), theme::TEXT),
        (false, false) => (Color32::from_black_alpha(170), theme::TEXT),
    };
    ui.painter().rect_filled(r, CornerRadius::same(15), fill);
    if !on {
        ui.painter().rect_stroke(r, CornerRadius::same(15), egui::Stroke::new(1.0, Color32::from_white_alpha(22)), egui::StrokeKind::Inside);
    }
    let ir = Rect::from_center_size(pos2(r.left() + 17.0, r.center().y), vec2(14.0, 14.0));
    super::icons::paint(ui.painter(), ir, icon, fg);
    if !narrow {
        ui.painter().text(pos2(ir.right() + 7.0, r.center().y), Align2::LEFT_CENTER, label, theme::medium(12.5), fg);
    }
    (r.left(), resp.on_hover_text(tip).clicked())
}

/// A amostra do estilo no "Personalizar": a linha anterior e a próxima
/// apagadas e desfocadas, e a do meio cantada pela metade na cor escolhida,
/// com a tradução embaixo (ou no lugar) quando ela está ligada.
pub fn preview(ui: &Ui, rect: Rect, style: &crate::look::LyricsStyle, translate: bool) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), theme::BG);
    let scale = style.size.clamp(*crate::look::LyricsStyle::SIZE.start(), *crate::look::LyricsStyle::SIZE.end());
    let font = crate::look::lyrics_font(ui.ctx(), style, 17.0 * scale);
    let wrap = rect.width() - 28.0;
    let x = anchor_x(rect, 14.0, style.align);
    let replace = translate && style.translation == crate::look::TranslationMode::Replace;
    let above = translate && style.translation == crate::look::TranslationMode::Above;
    let t = musicfast_i18n::t();
    let [r, g, b] = style.color;
    let sung = Color32::from_rgb(r, g, b);
    let [r, g, b] = style.translation_color;
    let trans = Color32::from_rgb(r, g, b);
    let middle = if replace { t.lyrics_sample_translation } else { "Maybe you can show me how to love" };
    let lines = [("I've been on my own for long enough", 1.0), (middle, 0.0), ("I'm going through withdrawals", 1.0)];
    let mut y = rect.top() + 12.0;
    for (text, distance) in lines {
        if distance == 0.0 && above {
            let tg = lay(&painter, t.lyrics_sample_translation, crate::look::translation_font(ui.ctx(), style, 12.5 * scale), wrap, style.align);
            painter.galley_with_override_text_color(pos2(x, y), tg.clone(), trans.gamma_multiply(0.85));
            y += tg.size().y + 3.0;
        }
        let g = lay(&painter, text, font.clone(), wrap, style.align);
        let pos = pos2(x, y);
        if distance == 0.0 {
            painter.galley_with_override_text_color(pos, g.clone(), white(0.42));
            // A primeira metade já cantada.
            let row = g.rect.translate(pos.to_vec2());
            let half = Rect::from_min_max(pos2(rect.left(), y - 4.0), pos2(row.center().x, y + g.size().y + 4.0));
            painter.with_clip_rect(half.intersect(rect)).galley_with_override_text_color(pos, g.clone(), sung);
            y += g.size().y + 3.0;
            if translate && !replace && !above {
                let tg = lay(&painter, t.lyrics_sample_translation, crate::look::translation_font(ui.ctx(), style, 12.5 * scale), wrap, style.align);
                painter.galley_with_override_text_color(pos2(x, y), tg.clone(), trans.gamma_multiply(0.85));
                y += tg.size().y;
            }
        } else {
            soft(&painter, pos, &g, white(style.dim), style.blur * 2.1);
            y += g.size().y;
        }
        y += 10.0 * scale.max(1.0);
    }
}

/// O texto desfocado: com `radius` (pontos), oito cópias em volta e uma no
/// meio, transparentes, no lugar de uma nítida. O egui não desfoca texto; é
/// o mesmo truque de sombra suave dos sites.
fn soft(painter: &egui::Painter, pos: egui::Pos2, g: &Arc<Galley>, color: Color32, radius: f32) {
    if radius < 0.3 {
        painter.galley_with_override_text_color(pos, g.clone(), color);
        return;
    }
    painter.galley_with_override_text_color(pos, g.clone(), color.gamma_multiply(0.3));
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let offset = vec2(a.cos(), a.sin()) * radius;
        painter.galley_with_override_text_color(pos + offset, g.clone(), color.gamma_multiply(0.14));
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
#[allow(clippy::too_many_arguments)]
fn dots(painter: &egui::Painter, rect: Rect, line: &Line, t: f64, active: bool, fade: f32, now: f64, x: f32, align: crate::look::TextAlign) {
    let r = rect.height() * 0.13;
    let progress = if active { ((t - line.start) / (line.end - line.start).max(0.1)).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let breathe = if active { 1.0 + 0.12 * (now as f32 * 3.0).sin() } else { 1.0 };
    // Os três, encostados no lado do alinhamento.
    let x0 = match align {
        crate::look::TextAlign::Left => x + r,
        crate::look::TextAlign::Center => x - r * 3.2,
        crate::look::TextAlign::Right => x - r * 7.4,
    };
    for i in 0..3 {
        let lit = (progress * 3.0 - i as f32).clamp(0.0, 1.0);
        let alpha = if active { 0.35 + 0.65 * lit } else { 0.22 };
        let c = pos2(x0 + i as f32 * r * 3.2, rect.center().y);
        painter.circle_filled(c, r * breathe, white(alpha * fade));
    }
}
