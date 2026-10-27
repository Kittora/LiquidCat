//! Configuracion persistente. Usa el MISMO config.json que la version en Python
//! (%APPDATA%\LiquidCat\config.json), asi se conservan preferencias, presets e historial.
use crate::util;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub settings: Value,
}

/// Un preset a partir de sus ajustes. Solo se guardan las claves que fija:
/// al aplicarlo, lo demas (p. ej. la GPU detectada) se queda como esta.
fn preset(name: &str, settings: Value) -> Preset {
    Preset { name: name.into(), settings }
}

/// Ajustes de fabrica del descargador (los de una instalacion nueva). Se
/// usan al elegir "— Aplicar preset —"; "auto" = la GPU detectada.
pub fn default_settings() -> Value {
    json!({
        "mode": "video", "quality": "", "direct": "0",
        "audio_codec": "mp3", "audio_bitrate": "",
        "vendor": "auto", "codec": "h264", "cq": "23",
        "audio_norm": false, "subs": false, "subs_embed": false,
        "embed_thumb": true, "embed_meta": true, "cookies": ""
    })
}

/// Presets que acompanan a una instalacion: cubren los usos tipicos sin
/// tener que tocar cada ajuste.
pub fn default_presets() -> Vec<Preset> {
    vec![
        // Lo mejor disponible reencodeado con la GPU del equipo ("auto"):
        // HEVC con calidad muy alta. Si la GPU no puede, se usa la CPU.
        preset("Max quality · GPU", json!({
            "mode": "video", "quality": "", "direct": "0",
            "vendor": "auto", "codec": "hevc", "cq": "18",
            "audio_codec": "aac", "audio_bitrate": "320",
            "subs": false, "subs_embed": false, "embed_thumb": false, "embed_meta": true
        })),
        // Mejor H.264 + AAC tal cual: mp4 que se reproduce en cualquier sitio.
        preset("H.264 compatible", json!({
            "mode": "video", "quality": "h264", "direct": "1",
            "subs": false, "subs_embed": false, "embed_thumb": false, "embed_meta": true
        })),
        // 1080p reencodeado a H.264 con subtitulos incrustados.
        preset("1080p H.264 + subtitles", json!({
            "mode": "video", "quality": "1080", "direct": "0", "codec": "h264", "cq": "21",
            "audio_codec": "aac", "audio_bitrate": "192",
            "subs": true, "subs_embed": true, "embed_thumb": false, "embed_meta": true
        })),
        // Archivos pequenos para compartir o guardar: 720p en HEVC.
        preset("720p light (HEVC)", json!({
            "mode": "video", "quality": "720", "direct": "0", "codec": "hevc", "cq": "28",
            "audio_codec": "aac", "audio_bitrate": "128",
            "subs": false, "subs_embed": false, "embed_thumb": false, "embed_meta": true
        })),
        // Musica: MP3 320 con caratula y etiquetas.
        preset("Music MP3 320", json!({
            "mode": "audio", "direct": "0", "audio_codec": "mp3", "audio_bitrate": "320",
            "audio_norm": false, "embed_thumb": true, "embed_meta": true
        })),
        // Audio AAC original (m4a) sin reconvertir: sin perdida extra.
        preset("Audio M4A · no re-encode", json!({
            "mode": "audio", "quality": "h264", "direct": "1",
            "embed_thumb": true, "embed_meta": true
        })),
        // Archivo sin perdida para editar o archivar.
        preset("FLAC lossless", json!({
            "mode": "audio", "direct": "0", "audio_codec": "flac",
            "audio_norm": false, "embed_thumb": true, "embed_meta": true
        })),
        // Voz (podcasts, clases): MP3 ligero con volumen normalizado.
        preset("Podcast / voice", json!({
            "mode": "audio", "direct": "0", "audio_codec": "mp3", "audio_bitrate": "128",
            "audio_norm": true, "embed_thumb": true, "embed_meta": true
        })),
    ]
}

/// Nombres de los presets que traian las versiones anteriores; se quitan al
/// actualizar (los presets propios del usuario se conservan).
const OLD_PRESETS: &[&str] = &["4K Original", "1080p MP4", "Audio MP3 320", "Original · max quality"];

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct HistoryEntry {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub ts: f64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub uploader: String,
    #[serde(default)]
    pub thumb: String,
    #[serde(default)]
    pub duration: Value,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub manual_bin: HashMap<String, Option<String>>,
    #[serde(default)]
    pub prefs: Map<String, Value>,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

pub fn path() -> PathBuf {
    util::config_dir().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        let mut cfg: Config = std::fs::read_to_string(path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.history.truncate(100);

        // Los presets por defecto se siembran una sola vez. El marcador evita
        // que reaparezcan si el usuario decide borrarlos todos.
        if !cfg.pref_bool("presets_seeded", false) {
            if cfg.presets.is_empty() {
                cfg.presets = default_presets();
            }
            cfg.set_pref("presets_seeded", Value::Bool(true));
            cfg.save();
        }
        // Instalaciones anteriores: una sola vez se quitan los presets viejos de
        // la app y se anaden los nuevos (sin tocar los que creo el usuario).
        if !cfg.pref_bool("presets_v4", false) {
            cfg.presets.retain(|p| !OLD_PRESETS.contains(&p.name.as_str()));
            for p in default_presets() {
                if !cfg.presets.iter().any(|x| x.name == p.name) {
                    cfg.presets.push(p);
                }
            }
            cfg.set_pref("presets_v4", Value::Bool(true));
            cfg.save();
        }

        cfg
    }

    pub fn save(&self) {
        let dir = util::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            // escritura atomica: evita dejar el JSON a medias si se cierra la app
            let tmp = dir.join("config.json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, path());
            }
        }
    }

    pub fn pref_bool(&self, key: &str, default: bool) -> bool {
        self.prefs.get(key).and_then(Value::as_bool).unwrap_or(default)
    }

    pub fn pref_str(&self, key: &str) -> String {
        self.prefs.get(key).and_then(Value::as_str).unwrap_or("").to_string()
    }

    pub fn set_pref(&mut self, key: &str, v: Value) {
        self.prefs.insert(key.to_string(), v);
    }
}

/// Lee un valor de ajustes tolerando numero/texto/bool (el JS guardaba todo como texto).
pub fn s_str(settings: &Value, key: &str) -> String {
    match settings.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => (if *b { "1" } else { "0" }).to_string(),
        _ => String::new(),
    }
}

pub fn s_bool(settings: &Value, key: &str) -> Option<bool> {
    settings.get(key).map(|v| match v {
        Value::Bool(b) => *b,
        Value::String(s) => s == "true" || s == "1",
        Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        _ => false,
    })
}
