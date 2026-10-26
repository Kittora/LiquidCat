#!/usr/bin/env python3
"""Genera src/i18n_data.rs y ui/i18n.slint a partir de tools/i18n/<idioma>.json.

Cada JSON tiene: "name" (nombre nativo para el selector), "ui" (textos de la
interfaz, global T de Slint) y "backend" (mensajes de registro/progreso que
usa Rust con lang::tr). El espanol es la referencia: define las claves.
Si a un idioma le falta una clave se usa la inglesa (y se avisa aqui).

Uso:  python tools/gen_i18n.py
Para agregar un idioma: crea tools/i18n/<codigo>.json, anade el codigo a
ORDER y vuelve a ejecutar este script.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "tools" / "i18n"
# Orden del selector de idioma en Ajustes.
ORDER = ["es", "en", "pt", "fr", "de", "it"]


def rs_str(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def main():
    data = {code: json.loads((SRC / f"{code}.json").read_text(encoding="utf-8")) for code in ORDER}
    ref = data["es"]
    for section in ("ui", "backend"):
        keys = list(ref[section])
        for code in ORDER:
            d = data[code][section]
            missing = [k for k in keys if k not in d]
            extra = [k for k in d if k not in ref[section]]
            if missing:
                print(f"[aviso] {code}.{section}: faltan {len(missing)} claves (se usara ingles): {', '.join(missing)}")
            if extra:
                print(f"[aviso] {code}.{section}: claves que no existen en es: {', '.join(extra)}")
    ui_keys = list(ref["ui"])

    sl = [
        "// GENERADO por tools/gen_i18n.py a partir de tools/i18n/*.json. No editar a mano.",
        "// Textos de la interfaz: se rellenan desde Rust al cambiar de idioma.",
        "export global T {",
    ]
    sl += [f"    in property <string> {k.replace('_', '-')};" for k in ui_keys]
    sl.append("}")
    (ROOT / "ui" / "i18n.slint").write_text("\n".join(sl) + "\n", encoding="utf-8", newline="\r\n")

    rs = [
        "// GENERADO por tools/gen_i18n.py a partir de tools/i18n/*.json. No editar a mano.",
        "use crate::{App, T};",
        "use slint::ComponentHandle;",
        "",
        "/// Idiomas disponibles (codigo, nombre nativo), en el orden del selector.",
        "pub static LANGS: &[(&str, &str)] = &[",
    ]
    rs += [f"    ({rs_str(c)}, {rs_str(data[c]['name'])})," for c in ORDER]
    rs += ["];", ""]
    for section, prefix in (("ui", "UI"), ("backend", "BE")):
        for code in ORDER:
            rs.append(f"pub static {prefix}_{code.upper()}: &[(&str, &str)] = &[")
            rs += [f"    ({rs_str(k)}, {rs_str(v)})," for k, v in data[code][section].items() if k in ref[section]]
            rs += ["];", ""]
    for section, prefix, doc in (("ui", "UI", "interfaz"), ("backend", "BE", "backend")):
        fn = "ui_table" if section == "ui" else "backend_table"
        rs.append(f"/// Tabla de textos de la {doc} para un idioma (ingles si no existe).")
        rs.append(f"pub fn {fn}(lang: &str) -> &'static [(&'static str, &'static str)] {{")
        rs.append("    match lang {")
        rs += [f"        {rs_str(c)} => {prefix}_{c.upper()}," for c in ORDER if c != "en"]
        rs += [f"        _ => {prefix}_EN,", "    }", "}", ""]
    rs += [
        "/// Rellena el global `T` de la interfaz: primero en ingles (por si a una",
        "/// traduccion le falta algun texto) y encima el idioma pedido.",
        "pub fn apply(app: &App, lang: &str) {",
        "    let t = app.global::<T>();",
        "    for table in [UI_EN, ui_table(lang)] {",
        "        for (k, v) in table.iter() {",
        "            match *k {",
    ]
    rs += [f"                {rs_str(k)} => t.set_{k}((*v).into())," for k in ui_keys]
    rs += ["                _ => {}", "            }", "        }", "    }", "}"]
    (ROOT / "src" / "i18n_data.rs").write_text("\n".join(rs) + "\n", encoding="utf-8", newline="\r\n")
    print(f"i18n: {len(ORDER)} idiomas, {len(ui_keys)} textos de interfaz, {len(ref['backend'])} del backend")


if __name__ == "__main__":
    main()
