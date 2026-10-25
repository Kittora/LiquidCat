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

/// Presets que acompanan a una instalacion nueva. Son deliberadamente pocos:
/// cubren maxima calidad, un MP4 compatible y extraccion de audio.
pub fn default_presets() -> Vec<Preset> {
    vec![
        Preset {
            name: "4K Original".into(),
            settings: json!({
                "mode": "video",
                "quality": "2160",
                "direct": "1",
                "audio_codec": "aac",
                "audio_bitrate": "320",
                "vendor": "cpu",
                "codec": "h264",
                "cq": "20",
                "audio_norm": false,
                "subs": false,
                "subs_embed": false,
                "embed_thumb": false,
                "embed_meta": true,
                "cookies": ""
            }),
        },
        Preset {
            name: "1080p MP4".into(),
            settings: json!({
                "mode": "video",
                "quality": "1080",
                "direct": "0",
                "audio_codec": "aac",
                "audio_bitrate": "192",
                "vendor": "cpu",
                "codec": "h264",
                "cq": "23",
                "audio_norm": false,
                "subs": true,
                "subs_embed": true,
                "embed_thumb": false,
                "embed_meta": true,
                "cookies": ""
            }),
        },
        Preset {
            name: "Audio MP3 320".into(),
            settings: json!({
                "mode": "audio",
                "quality": "",
                "direct": "0",
                "audio_codec": "mp3",
                "audio_bitrate": "320",
                "vendor": "cpu",
                "codec": "h264",
                "cq": "23",
                "audio_norm": true,
                "subs": false,
                "subs_embed": false,
                "embed_thumb": true,
                "embed_meta": true,
                "cookies": ""
            }),
        },
    ]
}

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
