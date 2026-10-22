#!/usr/bin/env python3
"""Genera src/themes_data.rs leyendo los 14 temas del style.css original.
Uso: python tools/gen_themes.py <carpeta del proyecto original>"""
import re, sys
from pathlib import Path
SRC = Path(sys.argv[1]); ROOT = Path(__file__).resolve().parent.parent
css = (SRC / "assets/style.css").read_text(encoding="utf-8")
html = (SRC / "index.html").read_text(encoding="utf-8")

order = re.findall(r'class="theme-card" data-theme="([a-z-]+)" type="button"><span class="swatch (\w+)"></span>([^<]+)<', html)
sw = {m[0]: m[1:] for m in re.findall(r'\.swatch\.(\w+)\s*\{\s*background:\s*linear-gradient\(90deg,\s*(#\w+) 0 40%,\s*(#\w+) 40% 70%,\s*(#\w+) 70%\)', css)}

def block(theme):
    out = {}
    for m in re.finditer(r'body\[data-theme="%s"\]\s*\{([^}]*)\}' % re.escape(theme), css):
        out.update({k: v.strip() for k, v in re.findall(r'--([a-z0-9-]+)\s*:\s*([^;]+);', m.group(1))})
    return out

def norm(c):
    c = c.strip()
    if re.fullmatch(r'#[0-9a-fA-F]{3}', c): c = '#' + ''.join(ch * 2 for ch in c[1:])
    return c.lower()

rows = []
for tid, swk, name in order:
    v = block(tid)
    ctp = {k: x for k, x in v.items() if k.startswith('ctp-')}
    def res(x):
        m = re.fullmatch(r'var\(--(ctp-[a-z0-9]+)\)', x)
        return norm(ctp[m.group(1)]) if m else norm(x)
    g = lambda k, d=None: res(v[k]) if k in v else d
    brutal = tid.startswith('neo-brutalist')
    t = dict(
        bg=g('bg'), bg_accent=g('bg-accent'), surface=g('surface'), panel=g('panel'), panel_strong=g('panel-strong'),
        input=g('input'), line=g('line'), line_strong=g('line-strong'), text=g('text'), muted=g('muted'),
        accent=g('accent'), accent2=g('accent-2'), success=g('success'), danger=g('danger'), warning=g('warning'),
        on_accent=g('on-accent'),
        log_bg=g('log-bg'), log_text=g('log-text'), log_border=g('log-border'), log_cmd=g('log-cmd'),
        log_info=g('log-info'), log_ok=g('log-ok'), log_err=g('log-err'),
    )
    keys7 = ('c_search', 'c_preview', 'c_layers', 'c_sliders', 'c_folder', 'c_file', 'c_terminal')
    if tid.startswith('catppuccin'):
        t.update(c_search=norm(ctp['ctp-sapphire']), c_preview=norm(ctp['ctp-pink']), c_layers=norm(ctp['ctp-blue']),
                 c_sliders=norm(ctp['ctp-peach']), c_folder=norm(ctp['ctp-yellow']), c_file=norm(ctp['ctp-teal']),
                 c_terminal=norm(ctp['ctp-green']))
    elif tid == 'light':
        for k in keys7: t[k] = t['text']
    else:
        for k in keys7: t[k] = t['accent']
    t['brutal'] = brutal
    if brutal:
        t['hard_panel'] = norm(re.search(r'(#\w+)\s*$', v['panel-shadow']).group(1))
        t['hard_ctrl'] = norm(re.search(r'(#\w+)\s*$', v['control-shadow']).group(1))
    else:
        t['hard_panel'] = '#171717'; t['hard_ctrl'] = '#171717'
    miss = [k for k, x in t.items() if x is None]
    assert not miss, (tid, miss)
    rows.append((tid, name.strip(), [norm(x) for x in sw[swk]], t))

def c(x): return "0x" + x.lstrip('#').upper()
out = ["// GENERADO por tools/gen_themes.py a partir de style.css. No editar a mano.", "",
       "pub struct ThemeDef {", "    pub id: &'static str,", "    pub name: &'static str,", "    pub sw: [u32; 3],"]
keys = ["bg","bg_accent","surface","panel","panel_strong","input","line","line_strong","text","muted","accent","accent2",
        "success","danger","warning","on_accent","log_bg","log_text","log_border","log_cmd","log_info","log_ok","log_err",
        "c_search","c_preview","c_layers","c_sliders","c_folder","c_file","c_terminal","hard_panel","hard_ctrl"]
for k in keys: out.append(f"    pub {k}: u32,")
out += ["    pub brutal: bool,", "}", "", "pub static THEMES: &[ThemeDef] = &["]
for tid, name, s, t in rows:
    out.append("    ThemeDef {")
    out.append(f'        id: "{tid}", name: "{name}", sw: [{c(s[0])}, {c(s[1])}, {c(s[2])}],')
    for k in keys: out.append(f"        {k}: {c(t[k])},")
    out.append(f"        brutal: {'true' if t['brutal'] else 'false'},")
    out.append("    },")
out.append("];")
(ROOT / "src/themes_data.rs").write_text("\n".join(out) + "\n", encoding="utf-8")
print(len(rows), "temas")
