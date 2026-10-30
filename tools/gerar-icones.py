"""Gera crates/app/src/ui/icon_data.rs a partir do lucide-react (trazido do Produtividade Prime).

Os ícones do overlay antigo são do Lucide (licença ISC, aviso em
THIRD_PARTY_NOTICES.md). Em vez de carregar uma fonte de ícones ou um leitor
de SVG no app, os traçados são achatados aqui em polilinhas na grade 24x24 do
Lucide, e o app só escala e risca. Rode de novo para trocar a lista:

    python tools/gerar-icones.py "<overlay-app>/node_modules/lucide-react"
"""

import math
import pathlib
import re
import sys

# O nome exportado pelo lucide-react (o que o overlay antigo importa) e o nome
# da constante no Rust.
ICONS = """
Play Pause SkipBack SkipForward Shuffle Repeat Repeat1 Volume2 Volume1 VolumeX Search
House Compass ListMusic MicVocal ArrowLeft Ellipsis X Radio ListPlus ListEnd Disc3 User
Loader2 ChevronRight ChevronLeft Music2 Trash2 RefreshCw Info
ThumbsUp ThumbsDown Plus Library LogIn LogOut History ChevronDown ChevronUp Check BookmarkPlus BookmarkCheck Menu
EllipsisVertical Minimize2 Maximize2 Pin
""".split()

CURVE_STEPS = 12
ARC_STEP_DEGREES = 12


def const_name(export):
    name = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", export)
    return re.sub(r"(?<=[A-Za-z])(?=\d)", "_", name).upper()


def export_files(root):
    text = (root / "dist/esm/lucide-react.js").read_text(encoding="utf-8")
    files = {}
    for line in text.splitlines():
        match = re.search(r"from '\./icons/([^']+)\.js'", line)
        if not match:
            continue
        for name in re.findall(r"default as (\w+)", line):
            files[name] = match.group(1)
    return files


def icon_nodes(root, file):
    text = (root / f"dist/esm/icons/{file}.js").read_text(encoding="utf-8")
    body = text.split("const __iconNode = [", 1)[1].split("];", 1)[0]
    nodes = []
    for tag, attrs in re.findall(r'\[\s*"(\w+)",\s*\{([^}]*)\}\s*\]', body):
        nodes.append((tag, dict(re.findall(r'(\w+):\s*"([^"]*)"', attrs))))
    return nodes


NUMBER = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


def arc_points(x1, y1, rx, ry, phi, large, sweep, x2, y2):
    if rx == 0 or ry == 0:
        return [(x2, y2)]
    phi = math.radians(phi)
    cos_p, sin_p = math.cos(phi), math.sin(phi)
    dx, dy = (x1 - x2) / 2, (y1 - y2) / 2
    x1p = cos_p * dx + sin_p * dy
    y1p = -sin_p * dx + cos_p * dy
    rx, ry = abs(rx), abs(ry)
    lam = (x1p ** 2) / (rx ** 2) + (y1p ** 2) / (ry ** 2)
    if lam > 1:
        rx *= math.sqrt(lam)
        ry *= math.sqrt(lam)
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    coef = math.sqrt(max(0.0, num / den)) if den else 0.0
    if large == sweep:
        coef = -coef
    cxp = coef * rx * y1p / ry
    cyp = -coef * ry * x1p / rx
    cx = cos_p * cxp - sin_p * cyp + (x1 + x2) / 2
    cy = sin_p * cxp + cos_p * cyp + (y1 + y2) / 2

    def angle(ux, uy, vx, vy):
        a = math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        return a

    theta1 = angle(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry)
    delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry)
    if not sweep and delta > 0:
        delta -= 2 * math.pi
    elif sweep and delta < 0:
        delta += 2 * math.pi
    steps = max(2, int(abs(math.degrees(delta)) / ARC_STEP_DEGREES) + 1)
    points = []
    for i in range(1, steps + 1):
        t = theta1 + delta * i / steps
        px = cx + rx * math.cos(t) * cos_p - ry * math.sin(t) * sin_p
        py = cy + rx * math.cos(t) * sin_p + ry * math.sin(t) * cos_p
        points.append((px, py))
    points[-1] = (x2, y2)
    return points


def parse_path(d):
    """Comandos SVG completos, com os flags de arco colados que o Lucide usa."""
    subpaths = []
    current = []
    x = y = sx = sy = 0.0
    last_ctrl = None
    last_cmd = ""
    i = 0
    n = len(d)

    def skip():
        nonlocal i
        while i < n and d[i] in " ,\t\n\r":
            i += 1

    def number():
        nonlocal i
        skip()
        match = NUMBER.match(d, i)
        if not match:
            raise ValueError(f"número esperado em {d[i:i+12]!r}")
        i = match.end()
        return float(match.group(0))

    def flag():
        nonlocal i
        skip()
        value = d[i]
        if value not in "01":
            raise ValueError(f"flag esperado em {d[i:i+12]!r}")
        i += 1
        return int(value)

    def has_number():
        skip()
        return i < n and (d[i].isdigit() or d[i] in "-+.")

    cmd = None
    while True:
        skip()
        if i >= n:
            break
        if d[i].isalpha():
            cmd = d[i]
            i += 1
        elif cmd is None:
            raise ValueError("caminho sem comando")
        rel = cmd.islower()
        c = cmd.upper()
        if c == "Z":
            if current:
                subpaths.append((True, current))
            current = []
            x, y = sx, sy
            last_ctrl = None
            last_cmd = "Z"
            continue
        if c == "M":
            if current:
                subpaths.append((False, current))
            nx, ny = number(), number()
            if rel:
                nx, ny = x + nx, y + ny
            x, y, sx, sy = nx, ny, nx, ny
            current = [(x, y)]
            cmd = "l" if rel else "L"
            last_ctrl = None
            last_cmd = "M"
            continue
        if c == "L":
            nx, ny = number(), number()
            if rel:
                nx, ny = x + nx, y + ny
            x, y = nx, ny
            current.append((x, y))
            last_ctrl = None
        elif c == "H":
            nx = number()
            x = x + nx if rel else nx
            current.append((x, y))
            last_ctrl = None
        elif c == "V":
            ny = number()
            y = y + ny if rel else ny
            current.append((x, y))
            last_ctrl = None
        elif c in "CS":
            if c == "C":
                x1, y1 = number(), number()
                if rel:
                    x1, y1 = x + x1, y + y1
            else:
                if last_ctrl and last_cmd in "CS":
                    x1, y1 = 2 * x - last_ctrl[0], 2 * y - last_ctrl[1]
                else:
                    x1, y1 = x, y
            x2, y2, ex, ey = number(), number(), number(), number()
            if rel:
                x2, y2, ex, ey = x + x2, y + y2, x + ex, y + ey
            for k in range(1, CURVE_STEPS + 1):
                t = k / CURVE_STEPS
                mt = 1 - t
                px = mt ** 3 * x + 3 * mt * mt * t * x1 + 3 * mt * t * t * x2 + t ** 3 * ex
                py = mt ** 3 * y + 3 * mt * mt * t * y1 + 3 * mt * t * t * y2 + t ** 3 * ey
                current.append((px, py))
            last_ctrl = (x2, y2)
            x, y = ex, ey
        elif c in "QT":
            if c == "Q":
                x1, y1 = number(), number()
                if rel:
                    x1, y1 = x + x1, y + y1
            else:
                if last_ctrl and last_cmd in "QT":
                    x1, y1 = 2 * x - last_ctrl[0], 2 * y - last_ctrl[1]
                else:
                    x1, y1 = x, y
            ex, ey = number(), number()
            if rel:
                ex, ey = x + ex, y + ey
            for k in range(1, CURVE_STEPS + 1):
                t = k / CURVE_STEPS
                mt = 1 - t
                px = mt * mt * x + 2 * mt * t * x1 + t * t * ex
                py = mt * mt * y + 2 * mt * t * y1 + t * t * ey
                current.append((px, py))
            last_ctrl = (x1, y1)
            x, y = ex, ey
        elif c == "A":
            rx, ry, phi = number(), number(), number()
            large, sweep = flag(), flag()
            ex, ey = number(), number()
            if rel:
                ex, ey = x + ex, y + ey
            current.extend(arc_points(x, y, rx, ry, phi, large, sweep, ex, ey))
            x, y = ex, ey
            last_ctrl = None
        else:
            raise ValueError(f"comando {cmd}")
        last_cmd = c
    if current:
        subpaths.append((False, current))
    return subpaths


def rounded_rect(x, y, w, h, rx, ry):
    rx = min(rx, w / 2)
    ry = min(ry, h / 2)
    if rx <= 0 or ry <= 0:
        return [(True, [(x, y), (x + w, y), (x + w, y + h), (x, y + h)])]
    pts = []
    corners = [
        (x + w - rx, y + ry, -90),
        (x + w - rx, y + h - ry, 0),
        (x + rx, y + h - ry, 90),
        (x + rx, y + ry, 180),
    ]
    for cx, cy, start in corners:
        for k in range(0, 7):
            a = math.radians(start + 90 * k / 6)
            pts.append((cx + rx * math.cos(a), cy + ry * math.sin(a)))
    return [(True, pts)]


def ellipse(cx, cy, rx, ry):
    steps = max(16, int(2 * math.pi * max(rx, ry) / 1.0))
    steps = min(steps, 48)
    return [(True, [(cx + rx * math.cos(2 * math.pi * k / steps), cy + ry * math.sin(2 * math.pi * k / steps)) for k in range(steps)])]


def points_attr(text):
    nums = [float(v) for v in NUMBER.findall(text)]
    return list(zip(nums[0::2], nums[1::2]))


def element(tag, a):
    f = lambda k, default=0.0: float(a.get(k, default))
    if tag == "path":
        return parse_path(a["d"])
    if tag == "rect":
        rx = f("rx", a.get("ry", 0))
        ry = f("ry", a.get("rx", 0))
        return rounded_rect(f("x"), f("y"), f("width"), f("height"), rx, ry)
    if tag == "circle":
        return ellipse(f("cx"), f("cy"), f("r"), f("r"))
    if tag == "ellipse":
        return ellipse(f("cx"), f("cy"), f("rx"), f("ry"))
    if tag == "line":
        return [(False, [(f("x1"), f("y1")), (f("x2"), f("y2"))])]
    if tag == "polyline":
        return [(False, points_attr(a["points"]))]
    if tag == "polygon":
        return [(True, points_attr(a["points"]))]
    raise ValueError(f"elemento {tag}")


def dedupe(points):
    out = []
    for p in points:
        if not out or abs(out[-1][0] - p[0]) > 1e-4 or abs(out[-1][1] - p[1]) > 1e-4:
            out.append(p)
    return out


def main():
    root = pathlib.Path(sys.argv[1])
    version = re.search(r'"version":\s*"([^"]+)"', (root / "package.json").read_text(encoding="utf-8")).group(1)
    files = export_files(root)
    out = [
        f"//! Gerado por `tools/gerar-icones.py` a partir do lucide-react {version}",
        "//! (licença ISC; aviso em `THIRD_PARTY_NOTICES.md`). Não edite à mão: rode o",
        "//! gerador. Cada ícone é uma lista de traços na grade 24x24 do Lucide.",
        "//! O conjunto inteiro que o app antigo usa fica aqui, mesmo o que uma",
        "//! fase ainda não desenha.",
        "",
        "#![allow(dead_code)]",
        "",
        "use super::icons::{Icon, Line};",
        "",
    ]
    for export in ICONS:
        file = files.get(export)
        if not file:
            raise SystemExit(f"ícone desconhecido no lucide-react {version}: {export}")
        lines = []
        for tag, attrs in icon_nodes(root, file):
            for closed, points in element(tag, attrs):
                points = dedupe(points)
                if closed and len(points) > 2 and abs(points[0][0] - points[-1][0]) < 1e-4 and abs(points[0][1] - points[-1][1]) < 1e-4:
                    points = points[:-1]
                if len(points) < 2 and not closed:
                    if len(points) == 1:
                        # Um "ponto" do Lucide (M x y h.01): risco mínimo.
                        points = [points[0], (points[0][0] + 0.01, points[0][1])]
                    else:
                        continue
                coords = ", ".join(f"[{px:.3f}, {py:.3f}]".replace(".000", ".0") for px, py in points)
                lines.append(f"    Line {{ closed: {'true' if closed else 'false'}, points: &[{coords}] }},")
        out.append(f"/// `{export}` (`{file}`)")
        out.append(f"pub const {const_name(export)}: Icon = &[")
        out.extend(lines)
        out.append("];")
        out.append("")
    target = pathlib.Path(__file__).resolve().parent.parent / "crates/app/src/ui/icon_data.rs"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("\n".join(out), encoding="utf-8", newline="\n")
    print(f"{len(ICONS)} ícones em {target}")


if __name__ == "__main__":
    main()
