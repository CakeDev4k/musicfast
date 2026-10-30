//! Os ícones Lucide, riscados pelo egui (trazido do Produtividade Prime).
//!
//! Os traçados vêm achatados de `tools/gerar-icones.py` (`icon_data.rs`): o
//! app não carrega fonte de ícones nem lê SVG. Aqui eles só são escalados da
//! grade 24x24 para o tamanho pedido e riscados com 2 unidades de espessura,
//! pontas e cantos redondos, como o `stroke-linecap/linejoin="round"` do
//! Lucide.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};

pub use super::icon_data::*;

/// Um traço de um ícone, na grade 24x24.
pub struct Line {
    pub closed: bool,
    pub points: &'static [[f32; 2]],
}

pub type Icon = &'static [Line];

fn mapped(line: &Line, origin: Pos2, scale: f32) -> Vec<Pos2> {
    line.points
        .iter()
        .map(|[x, y]| pos2(origin.x + x * scale, origin.y + y * scale))
        .collect()
}

/// Vértice com curva fechada ganha um disco: sem ele, a junção em ângulo
/// agudo do egui vira uma ponta.
fn round_joins(painter: &Painter, points: &[Pos2], closed: bool, radius: f32, color: Color32) {
    let n = points.len();
    if n < 3 {
        return;
    }
    let range = if closed { 0..n } else { 1..n - 1 };
    for i in range {
        let prev = points[(i + n - 1) % n];
        let here = points[i];
        let next = points[(i + 1) % n];
        let a = (here - prev).normalized();
        let b = (next - here).normalized();
        // Só nos cantos de verdade (as curvas achatadas já são suaves).
        if a.dot(b) < 0.7 {
            painter.circle_filled(here, radius, color);
        }
    }
}

/// Risca o ícone centrado em `rect`, no tamanho do lado menor.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    paint_with(painter, rect, icon, color, 2.0);
}

/// Como `paint`, com a espessura em unidades da grade (o Lucide usa 2).
pub fn paint_with(painter: &Painter, rect: Rect, icon: Icon, color: Color32, width_units: f32) {
    let side = rect.width().min(rect.height());
    let scale = side / 24.0;
    let origin = rect.center() - vec2(12.0, 12.0) * scale;
    let width = (width_units * scale).max(1.0);
    let stroke = Stroke::new(width, color);
    for line in icon {
        let points = mapped(line, origin, scale);
        if line.closed {
            round_joins(painter, &points, true, width / 2.0, color);
            painter.add(Shape::closed_line(points, stroke));
        } else {
            if let (Some(first), Some(last)) = (points.first(), points.last()) {
                painter.circle_filled(*first, width / 2.0, color);
                painter.circle_filled(*last, width / 2.0, color);
            }
            round_joins(painter, &points, false, width / 2.0, color);
            painter.add(Shape::line(points, stroke));
        }
    }
}

/// Risca e preenche os traços fechados (o `fill="currentColor"` que o player
/// antigo usava no tocar/pausar).
pub fn paint_filled(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let scale = side / 24.0;
    let origin = rect.center() - vec2(12.0, 12.0) * scale;
    for line in icon.iter().filter(|l| l.closed) {
        painter.add(Shape::convex_polygon(
            mapped(line, origin, scale),
            color,
            Stroke::NONE,
        ));
    }
    paint(painter, rect, icon, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Todos os ícones ficam dentro da grade: um traçado mal lido sairia dela.
    #[test]
    fn every_icon_stays_inside_the_grid() {
        let all: &[(&str, Icon)] = &[
            ("PLAY", PLAY),
            ("MINIMIZE_2", MINIMIZE_2),
            ("PIN", PIN),
            ("MAXIMIZE_2", MAXIMIZE_2),
            ("PAUSE", PAUSE),
            ("SKIP_BACK", SKIP_BACK),
            ("SKIP_FORWARD", SKIP_FORWARD),
            ("SHUFFLE", SHUFFLE),
            ("REPEAT", REPEAT),
            ("REPEAT_1", REPEAT_1),
            ("VOLUME_2", VOLUME_2),
            ("VOLUME_1", VOLUME_1),
            ("VOLUME_X", VOLUME_X),
            ("SEARCH", SEARCH),
            ("HOUSE", HOUSE),
            ("COMPASS", COMPASS),
            ("LIST_MUSIC", LIST_MUSIC),
            ("MIC_VOCAL", MIC_VOCAL),
            ("ARROW_LEFT", ARROW_LEFT),
            ("ELLIPSIS", ELLIPSIS),
            ("X", X),
            ("RADIO", RADIO),
            ("LIST_PLUS", LIST_PLUS),
            ("LIST_END", LIST_END),
            ("DISC_3", DISC_3),
            ("USER", USER),
            ("LOADER_2", LOADER_2),
            ("CHEVRON_RIGHT", CHEVRON_RIGHT),
            ("CHEVRON_LEFT", CHEVRON_LEFT),
            ("MUSIC_2", MUSIC_2),
            ("TRASH_2", TRASH_2),
            ("REFRESH_CW", REFRESH_CW),
            ("INFO", INFO),
        ];
        for (name, icon) in all {
            assert!(!icon.is_empty(), "{name} vazio");
            for line in icon.iter() {
                assert!(
                    line.points.len() >= 2 || line.closed,
                    "{name}: traço sem pontos"
                );
                for [x, y] in line.points {
                    assert!(
                        (-0.5..=24.5).contains(x) && (-0.5..=24.5).contains(y),
                        "{name} fora da grade: {x},{y}"
                    );
                }
            }
        }
    }
}
