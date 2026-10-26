#!/usr/bin/env python3
"""Genera los assets derivados (fuentes, banderas, iconos Slint, textos i18n).
Los textos se editan en tools/i18n/*.json (ver tools/gen_i18n.py).

Uso:  python tools/gen.py <carpeta del proyecto original LiquidCat>
Requiere: fonttools, brotli, cairosvg  (pip install fonttools brotli cairosvg) y Node.js.
Los resultados YA estan incluidos en el repositorio; este script solo hace falta
si quieres regenerarlos (p. ej. al agregar textos o iconos nuevos).
"""
import json, re, subprocess, sys, xml.etree.ElementTree as ET
from pathlib import Path

SRC = Path(sys.argv[1])
ROOT = Path(__file__).resolve().parent.parent
(ROOT / "assets" / "fonts").mkdir(parents=True, exist_ok=True)
(ROOT / "ui").mkdir(exist_ok=True)
(ROOT / "src").mkdir(exist_ok=True)

def make_fonts():
    from fontTools.ttLib import TTFont
    from fontTools.ttLib.woff2 import decompress
    from fontTools.varLib import instancer
    from fontTools import subset
    tmp = ROOT / "assets" / "fonts" / "_inter_var.ttf"
    decompress(str(SRC / "assets/fonts/InterVariable.woff2"), str(tmp))
    unicodes = list(range(0x20, 0x7F)) + list(range(0xA0, 0x180)) + \
        [0x2013, 0x2014, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2026, 0x00B7,
         0x2190, 0x2191, 0x2192, 0x2193, 0x2713, 0x2715, 0x00AB, 0x00BB, 0x2212]
    for name, wght in (("Regular", 400), ("Bold", 700)):
        f = TTFont(tmp)
        axes = [a.axisTag for a in f["fvar"].axes]
        f = instancer.instantiateVariableFont(f, {"wght": wght, "opsz": 14} if "opsz" in axes else {"wght": wght})
        opts = subset.Options(); opts.layout_features = ["kern", "liga", "calt"]
        opts.notdef_outline = True; opts.name_IDs = ["*"]
        s = subset.Subsetter(opts); s.populate(unicodes=unicodes); s.subset(f)
        out = ROOT / "assets" / "fonts" / f"Inter-{name}.ttf"
        f.save(out); print("font", out.name, out.stat().st_size // 1024, "KB")
    tmp.unlink()

def make_flags():
    import cairosvg
    for code in ("mx", "us"):
        cairosvg.svg2png(url=str(SRC / f"assets/flag-{code}.svg"),
                         write_to=str(ROOT / "assets" / f"flag-{code}.png"),
                         output_width=66, output_height=45)

NS = "{http://www.w3.org/2000/svg}"
def fnum(x): return ("%.3f" % float(x)).rstrip("0").rstrip(".")
def circle(cx, cy, r):
    cx, cy, r = float(cx), float(cy), float(r)
    return f"M{fnum(cx-r)} {fnum(cy)}a{fnum(r)} {fnum(r)} 0 1 0 {fnum(2*r)} 0a{fnum(r)} {fnum(r)} 0 1 0 {fnum(-2*r)} 0z"
def rect(x, y, w, h, rx=0):
    x, y, w, h, rx = (float(v) for v in (x, y, w, h, rx))
    if rx <= 0: return f"M{fnum(x)} {fnum(y)}h{fnum(w)}v{fnum(h)}h{fnum(-w)}z"
    return (f"M{fnum(x+rx)} {fnum(y)}h{fnum(w-2*rx)}a{fnum(rx)} {fnum(rx)} 0 0 1 {fnum(rx)} {fnum(rx)}"
            f"v{fnum(h-2*rx)}a{fnum(rx)} {fnum(rx)} 0 0 1 {fnum(-rx)} {fnum(rx)}h{fnum(-(w-2*rx))}"
            f"a{fnum(rx)} {fnum(rx)} 0 0 1 {fnum(-rx)} {fnum(-rx)}v{fnum(-(h-2*rx))}"
            f"a{fnum(rx)} {fnum(rx)} 0 0 1 {fnum(rx)} {fnum(-rx)}z")
def abs_start(d):
    """Un 'm' inicial es relativo a (0,0) en el SVG original; al concatenar trazos hay que
    volverlo absoluto ('M') y convertir los pares siguientes en 'l' (lineto relativo)."""
    m = re.match(r"^\s*m\s*(-?[\d.]+)[ ,]*(-?[\d.]+)(.*)$", d, re.S)
    if not m: return d
    x, y, rest = m.groups(); rest = rest.strip()
    if rest and (rest[0].isdigit() or rest[0] in "-."): rest = "l" + rest
    return f"M{x} {y}{rest}"
def make_icons():
    root = ET.fromstring((SRC / "assets/icons.svg").read_text(encoding="utf-8"))
    cases = []
    for sym in root:
        parts = []
        for el in sym:
            t = el.tag.replace(NS, "")
            if t == "path": parts.append(abs_start(el.get("d")))
            elif t == "circle": parts.append(circle(el.get("cx"), el.get("cy"), el.get("r")))
            elif t == "rect": parts.append(rect(el.get("x", 0), el.get("y", 0), el.get("width"), el.get("height"), el.get("rx", 0)))
        cases.append((sym.get("id"), "".join(parts)))
    lines = ["// GENERADO por tools/gen.py a partir de assets/icons.svg (estilo Lucide, 24x24).",
             "export global IconData {", "    pure public function d(name: string) -> string {"]
    for n, d in cases: lines.append(f'        if (name == "{n}") {{ return "{d}"; }}')
    lines += ['        return "";', "    }", "}"]
    (ROOT / "ui" / "icon_data.slint").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("icons", len(cases))

def plain(s): return re.sub(r"<[^>]+>", "", s)
def rs_str(s): return json.dumps(s, ensure_ascii=False)
def make_i18n():
    # Los textos ya no salen del script.js original: viven en tools/i18n/*.json
    # (un archivo por idioma) y los genera tools/gen_i18n.py.
    subprocess.run([sys.executable, str(ROOT / "tools/gen_i18n.py")], check=True)

if __name__ == "__main__":
    make_fonts(); make_flags(); make_icons(); make_i18n()
    import shutil
    shutil.copy(SRC / "assets/logo.png", ROOT / "assets/logo.png")
    shutil.copy(SRC / "icon.ico", ROOT / "assets/icon.ico")
