// O ícone do app, desenhado em código: um disco vermelho com o triângulo do
// tocar. Incluído com `include!` pelo `build.rs` (o .ico do executável) e por
// `icon.rs` (o ícone da janela), então os dois nunca divergem e não há
// arquivo binário no repositório.

/// RGBA de `side`×`side`, com antisserrilhado por 4×4 amostras por pixel.
pub fn icon_rgba(side: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let s = side as f32;
    let center = s / 2.0;
    let radius = s * 0.47;
    // O triângulo, um pouco à direita do centro para parecer centrado.
    let tri = [
        (s * 0.40, s * 0.29),
        (s * 0.40, s * 0.71),
        (s * 0.73, s * 0.50),
    ];
    let inside_tri = |x: f32, y: f32| {
        let sign = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| (x - bx) * (ay - by) - (ax - bx) * (y - by);
        let d1 = sign(tri[0], tri[1]);
        let d2 = sign(tri[1], tri[2]);
        let d3 = sign(tri[2], tri[0]);
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    for py in 0..side {
        for px in 0..side {
            let (mut disc, mut white) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = px as f32 + (sx as f32 + 0.5) / SS as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
                    let (dx, dy) = (x - center, y - center);
                    if dx * dx + dy * dy <= radius * radius {
                        disc += 1;
                        if inside_tri(x, y) {
                            white += 1;
                        }
                    }
                }
            }
            let n = (SS * SS) as f32;
            let alpha = disc as f32 / n;
            if disc == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // Mistura do vermelho com o branco dentro do disco.
            let w = white as f32 / disc as f32;
            let mix = |red: f32, white: f32| (red * (1.0 - w) + white * w).round() as u8;
            out.extend_from_slice(&[
                mix(255.0, 255.0),
                mix(0.0, 255.0),
                mix(51.0, 255.0),
                (alpha * 255.0).round() as u8,
            ]);
        }
    }
    out
}
