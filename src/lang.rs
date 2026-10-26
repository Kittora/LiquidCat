//! Textos del backend (mensajes de registro/progreso) en ES y EN.
pub fn tr(lang: &str, key: &str) -> String {
    let en = lang == "en";
    let s = match (key, en) {
        ("dl_video", false) => "Tomando video…",
        ("dl_video", true) => "Getting video…",
        ("dl_audio", false) | ("dl_audio_only", false) => "Tomando audio…",
        ("dl_audio", true) | ("dl_audio_only", true) => "Getting audio…",
        ("reencode", false) => "Reencodando…",
        ("reencode", true) => "Re-encoding…",
        ("conv_audio", false) => "Convirtiendo a {fmt}…",
        ("conv_audio", true) => "Converting to {fmt}…",
        ("final", false) => "Archivo final: {path}",
        ("final", true) => "Final file: {path}",
        ("stopped", false) => "Detenido.",
        ("stopped", true) => "Stopped.",
        ("done", false) => "¡Listo!",
        ("done", true) => "Done!",
        ("cancelled", false) => "[!] {phase} cancelado.",
        ("cancelled", true) => "[!] {phase} cancelled.",
        ("failed_dl_audio", false) => "Descarga de audio cancelada o fallida.",
        ("failed_dl_audio", true) => "Audio download cancelled or failed.",
        ("failed_no_audio", false) => "No se encontró el archivo de audio.",
        ("failed_no_audio", true) => "No audio file found.",
        ("failed_no_video", false) => "No se encontraron los archivos descargados.",
        ("failed_no_video", true) => "No downloaded files found.",
        ("failed_reencode", false) => "Reencode cancelado o fallido.",
        ("failed_reencode", true) => "Re-encode cancelled or failed.",
        ("failed_conv_audio", false) => "Conversión de audio cancelada o fallida.",
        ("failed_conv_audio", true) => "Audio conversion cancelled or failed.",
        ("cover_wav", false) => "[i] WAV no admite carátula; se incrustan solo los metadatos.",
        ("cover_wav", true) => "[i] WAV does not support cover art; embedding metadata only.",
        ("cover_thumb", false) => "no se pudo descargar la miniatura",
        ("cover_thumb", true) => "could not download the thumbnail",
        ("cover_failed", false) => "[!] No se pudo incrustar la carátula: {msg}",
        ("cover_failed", true) => "[!] Could not embed cover art: {msg}",
        ("tags_failed", false) => "[!] No se pudieron incrustar los metadatos: {msg}",
        ("tags_failed", true) => "[!] Could not embed metadata: {msg}",
        ("error", false) => "[!] Error inesperado: {msg}",
        ("error", true) => "[!] Unexpected error: {msg}",
        ("deps_downloading_pct", false) => "Descargando dependencias… {pct}%",
        ("deps_downloading_pct", true) => "Downloading dependencies… {pct}%",
        ("friendly_fetching", false) => "Obteniendo información del video…",
        ("friendly_fetching", true) => "Fetching video info…",
        ("friendly_merging", false) => "Combinando video y audio…",
        ("friendly_merging", true) => "Merging video & audio…",
        ("friendly_audio_extract", false) => "Extrayendo audio…",
        ("friendly_audio_extract", true) => "Extracting audio…",
        ("friendly_downloaded", false) => "Listo",
        ("friendly_downloaded", true) => "Done",
        ("cookie_ok", false) => "Cookies de {b} detectadas correctamente. Se usarán al descargar.",
        ("cookie_ok", true) => "Cookies from {b} detected successfully. They will be used when downloading.",
        ("cookie_err", false) => "No se encontraron cookies de {b}. Asegúrate de tener sesión iniciada en ese navegador.",
        ("cookie_err", true) => "No cookies found for {b}. Make sure you're logged in on that browser.",
        ("file_not_found", false) => "archivo no encontrado",
        ("file_not_found", true) => "file not found",
        ("no_ytdlp", false) => "yt-dlp o ffmpeg no están disponibles.",
        ("no_ytdlp", true) => "yt-dlp or ffmpeg is not available.",
        ("timeout", false) => "Tiempo de espera agotado.",
        ("timeout", true) => "Timed out.",
        ("ytdlp_failed", false) => "yt-dlp falló.",
        ("ytdlp_failed", true) => "yt-dlp failed.",
        ("prep_title", false) => "Preparando dependencias",
        ("prep_title", true) => "Preparing dependencies",
        ("prep_msg", false) => "LiquidCat necesita preparar sus herramientas multimedia. Esto solo ocurre una vez.",
        ("prep_msg", true) => "LiquidCat needs to prepare its media tools. This only happens once.",
        _ => return key.to_string(),
    };
    s.to_string()
}

pub fn trf(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = tr(lang, key);
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

/// Texto de la interfaz (tabla generada en i18n_data.rs) para usarlo desde el backend.
pub fn ui(lang: &str, key: &str) -> String {
    let table = if lang == "en" { crate::i18n_data::EN } else { crate::i18n_data::ES };
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string()).unwrap_or_else(|| key.to_string())
}

pub fn uif(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let mut s = ui(lang, key);
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}
