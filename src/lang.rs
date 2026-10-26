//! Textos traducidos. Las tablas (interfaz y backend, un juego por idioma) se
//! generan en i18n_data.rs con tools/gen_i18n.py a partir de tools/i18n/*.json.
use crate::i18n_data::{backend_table, ui_table, BE_EN, LANGS, UI_EN};

/// Idioma por defecto cuando el guardado no existe o no es valido.
pub const DEFAULT: &str = "es";

pub fn supported(lang: &str) -> bool {
    LANGS.iter().any(|(code, _)| *code == lang)
}

/// Busca en el idioma pedido, despues en ingles; si no existe, devuelve la clave.
fn lookup(table: &[(&str, &str)], fallback: &[(&str, &str)], key: &str) -> String {
    table
        .iter()
        .chain(fallback.iter())
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn fill(mut s: String, vars: &[(&str, &str)]) -> String {
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Mensaje del backend (registro/progreso).
pub fn tr(lang: &str, key: &str) -> String {
    lookup(backend_table(lang), BE_EN, key)
}

pub fn trf(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    fill(tr(lang, key), vars)
}

/// Texto de la interfaz para usarlo desde el backend.
pub fn ui(lang: &str, key: &str) -> String {
    lookup(ui_table(lang), UI_EN, key)
}

pub fn uif(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    fill(ui(lang, key), vars)
}
