//! Conecta la interfaz Slint con el backend: callbacks, preferencias, ventana, historial.
use crate::config::{s_bool, s_str};
use crate::core::{push_log, show_toast, Core};
use crate::jobs::{self, LocalPayload, Payload};
use crate::lang::{ui, uif};
use crate::{i18n_data, themes, util, App, DepInfo, HItem, S};
use serde_json::{json, Value};
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const MODES: [&str; 2] = ["video", "audio"];
// "h264": mejor video H.264 + audio AAC (mp4 compatible con todo).
const QUALITIES: [&str; 7] = ["", "2160", "1440", "1080", "720", "480", "h264"];
const VENDORS: [&str; 3] = ["nvidia", "amd", "cpu"];
const CODECS: [&str; 4] = ["h264", "hevc", "av1", "vp9"];
const ACODECS: [&str; 6] = ["mp3", "aac", "opus", "ogg", "flac", "wav"];
const BITRATES: [&str; 6] = ["", "320", "256", "192", "128", "96"];
const COOKIES: [&str; 7] = ["", "chrome", "edge", "firefox", "brave", "opera", "vivaldi"];
const LOCAL_MODES: [&str; 3] = ["video", "audio", "gif"];

thread_local! {
    static CORE: RefCell<Option<Arc<Core>>> = const { RefCell::new(None) };
    static HIST: RefCell<Option<Rc<VecModel<HItem>>>> = const { RefCell::new(None) };
    static URL_TIMER: slint::Timer = slint::Timer::default();
    // Archivos locales elegidos (uno o varios). El campo de la vista solo
    // muestra un resumen; la lista real vive aqui.
    static LOCAL_FILES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    // Al soltar varios archivos winit manda un evento por archivo: se juntan
    // y se aplican cuando dejan de llegar.
    static DROPPED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DROP_TIMER: slint::Timer = slint::Timer::default();
}

fn idx_of(list: &[&str], v: &str) -> i32 {
    list.iter().position(|x| *x == v).unwrap_or(0) as i32
}

fn pick<'a>(list: &[&'a str], i: i32) -> &'a str {
    list.get(i.max(0) as usize).copied().unwrap_or(list[0])
}

fn ss(s: &str) -> SharedString {
    SharedString::from(s)
}

/// Deja la descripcion en una sola linea compacta y corta en un espacio
/// cercano al limite, para que la vista previa nunca termine a mitad de palabra.
fn compact_desc(text: &str) -> String {
    const MAX: usize = 320;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MAX).collect();
    if let Some(pos) = out.rfind(' ') {
        out.truncate(pos);
    }
    out.push('…');
    out
}

// ====================================================================
//  Idioma
// ====================================================================
pub fn apply_lang(core: &Core, app: &App) {
    let lang = core.lang();
    i18n_data::apply(app, &lang);
    let s = app.global::<S>();
    s.set_lang(ss(&lang));
    let idx = i18n_data::LANGS.iter().position(|(code, _)| *code == lang).unwrap_or(0);
    s.set_lang_idx(idx as i32);
    if !core.formats.lock().unwrap().loaded {
        let ph = ui(&lang, "fmt_first");
        s.set_video_opts(ModelRc::new(VecModel::from(vec![ss(&ph)])));
        s.set_audio_opts(ModelRc::new(VecModel::from(vec![ss(&ph)])));
    }
    refresh_presets(core, app, None);
}

fn refresh_presets(core: &Core, app: &App, select: Option<&str>) {
    let lang = core.lang();
    let s = app.global::<S>();
    let cfg = core.cfg.lock().unwrap();
    let mut names = vec![ss(&ui(&lang, "preset_none"))];
    names.extend(cfg.presets.iter().map(|p| ss(&p.name)));
    let idx = select.and_then(|n| cfg.presets.iter().position(|p| p.name == n)).map(|i| i as i32 + 1).unwrap_or(0);
    s.set_preset_opts(ModelRc::new(VecModel::from(names)));
    s.set_preset_idx(idx);
}

// ====================================================================
//  Ajustes <-> interfaz
// ====================================================================
fn current_settings(app: &App) -> Value {
    let s = app.global::<S>();
    json!({
        "mode": pick(&MODES, s.get_mode_idx()),
        "quality": pick(&QUALITIES, s.get_quality_idx()),
        "direct": if s.get_direct_idx() == 1 { "1" } else { "0" },
        "audio_codec": pick(&ACODECS, s.get_acodec_idx()),
        "audio_bitrate": pick(&BITRATES, s.get_abitrate_idx()),
        "vendor": pick(&VENDORS, s.get_hw_idx()),
        "codec": pick(&CODECS, s.get_codec_idx()),
        "cq": s.get_cq().to_string(),
        "audio_norm": s.get_audio_norm(),
        "subs": s.get_subs(),
        "subs_embed": s.get_subs_embed(),
        "embed_thumb": s.get_embed_thumb(),
        "embed_meta": s.get_embed_meta(),
        "cookies": pick(&COOKIES, s.get_cookies_idx()),
    })
}

fn apply_settings(app: &App, v: &Value) {
    let s = app.global::<S>();
    let has = |k: &str| v.get(k).is_some();
    if has("mode") { s.set_mode_idx(idx_of(&MODES, &s_str(v, "mode"))); }
    if has("quality") { s.set_quality_idx(idx_of(&QUALITIES, &s_str(v, "quality"))); }
    if has("direct") { s.set_direct_idx(if s_str(v, "direct") == "1" { 1 } else { 0 }); }
    if has("audio_codec") { s.set_acodec_idx(idx_of(&ACODECS, &s_str(v, "audio_codec"))); }
    if has("audio_bitrate") { s.set_abitrate_idx(idx_of(&BITRATES, &s_str(v, "audio_bitrate"))); }
    if has("vendor") {
        // "auto" = la GPU detectada en este equipo
        let vendor = s_str(v, "vendor");
        let vendor = if vendor == "auto" { s.get_gpu().to_string() } else { vendor };
        s.set_hw_idx(idx_of(&VENDORS, &vendor).min(2));
    }
    if has("codec") { s.set_codec_idx(idx_of(&CODECS, &s_str(v, "codec"))); }
    if let Ok(cq) = s_str(v, "cq").parse::<i32>() { s.set_cq(cq.clamp(0, 51)); }
    if let Some(b) = s_bool(v, "audio_norm") { s.set_audio_norm(b); }
    if let Some(b) = s_bool(v, "subs") { s.set_subs(b); }
    if let Some(b) = s_bool(v, "subs_embed") { s.set_subs_embed(b); }
    if let Some(b) = s_bool(v, "embed_thumb") { s.set_embed_thumb(b); }
    if let Some(b) = s_bool(v, "embed_meta") { s.set_embed_meta(b); }
    if has("cookies") { s.set_cookies_idx(idx_of(&COOKIES, &s_str(v, "cookies"))); }
}

fn save_prefs(core: &Core, app: &App) {
    let s = app.global::<S>();
    let mut cfg = core.cfg.lock().unwrap();
    cfg.set_pref("notify_on_done", json!(s.get_pref_notify()));
    cfg.set_pref("open_when_done", json!(s.get_pref_open()));
    cfg.set_pref("auto_update_deps", json!(s.get_pref_autoupd()));
    cfg.set_pref("remember_settings", json!(s.get_pref_remember()));
    cfg.set_pref("cookies_browser", json!(pick(&COOKIES, s.get_pref_cookies_idx())));
    cfg.set_pref("theme", json!(s.get_theme().to_string()));
    cfg.set_pref("lang", json!(core.lang()));
    cfg.set_pref("out_dir", json!(s.get_out_dir().to_string()));
    cfg.set_pref("local_out_dir", json!(s.get_local_out_dir().to_string()));
    if s.get_pref_remember() {
        let cur = current_settings(app);
        cfg.set_pref("dl_settings", cur);
    }
    cfg.save();
}

// ====================================================================
//  Carga inicial
// ====================================================================
pub fn load_initial(core: &Arc<Core>, app: &App) {
    let s = app.global::<S>();
    let (theme, remember, settings, out_dir, local_out) = {
        let cfg = core.cfg.lock().unwrap();
        let theme = cfg.pref_str("theme");
        (
            if themes::valid(&theme) { theme } else { themes::DEFAULT.to_string() },
            cfg.pref_bool("remember_settings", true),
            cfg.prefs.get("dl_settings").cloned(),
            cfg.pref_str("out_dir"),
            cfg.pref_str("local_out_dir"),
        )
    };
    s.set_themes(themes::entries());
    s.set_lang_names(ModelRc::new(VecModel::from(i18n_data::LANGS.iter().map(|(_, n)| ss(n)).collect::<Vec<_>>())));
    s.set_lang_codes(ModelRc::new(VecModel::from(i18n_data::LANGS.iter().map(|(c, _)| ss(c)).collect::<Vec<_>>())));
    themes::apply(app, &theme);
    {
        let cfg = core.cfg.lock().unwrap();
        s.set_pref_notify(cfg.pref_bool("notify_on_done", true));
        s.set_pref_open(cfg.pref_bool("open_when_done", false));
        s.set_pref_autoupd(cfg.pref_bool("auto_update_deps", true));
        s.set_pref_remember(remember);
        let ck = cfg.pref_str("cookies_browser");
        s.set_pref_cookies_idx(idx_of(&COOKIES, &ck));
        s.set_cookies_idx(idx_of(&COOKIES, &ck));
    }
    if remember {
        if let Some(v) = settings {
            apply_settings(app, &v);
        }
    }
    let default_dir = util::downloads_dir().to_string_lossy().to_string();
    s.set_out_dir(ss(if out_dir.is_empty() { &default_dir } else { &out_dir }));
    s.set_local_out_dir(ss(if local_out.is_empty() { &default_dir } else { &local_out }));
    s.set_st(0);
    apply_lang(core, app);
    update_space_hint(core, app);
}

pub fn update_space_hint(core: &Core, app: &App) {
    let s = app.global::<S>();
    let lang = core.lang();
    match util::free_space(&s.get_out_dir()) {
        Some(free) => {
            s.set_space_hint(ss(&uif(&lang, "space_free", &[("s", &util::human_size(free))])));
            s.set_space_low(free < 2 * 1024 * 1024 * 1024);
        }
        None => {
            s.set_space_hint(ss(""));
            s.set_space_low(false);
        }
    }
}

// ====================================================================
//  Dialogos
// ====================================================================
fn dialog(app: &App, kind: i32, title: &str, msg: &str, action: &str) {
    let s = app.global::<S>();
    s.set_dlg_title(ss(title));
    s.set_dlg_msg(ss(msg));
    s.set_dlg_action(ss(action));
    s.set_dlg_input(ss(""));
    s.set_dlg(kind);
}

// ====================================================================
//  Historial
// ====================================================================
pub fn refresh_history(app: &App) {
    let Some(core) = CORE.with(|c| c.borrow().clone()) else { return };
    let entries = core.cfg.lock().unwrap().history.clone();
    let cache = core.thumb_cache.lock().unwrap();
    let rows: Vec<HItem> = entries
        .iter()
        .map(|e| {
            let mut meta = vec![util::fmt_datetime(e.ts)];
            if !e.uploader.is_empty() { meta.push(e.uploader.clone()); }
            if let Some(d) = e.duration.as_f64() { meta.push(util::fmt_duration(d)); }
            let name = if e.title.is_empty() { e.name.clone() } else { e.title.clone() };
            let (thumb, has) = match cache.get(&e.thumb) {
                Some((px, w, h)) if !e.thumb.is_empty() => (buf_to_image(px, *w, *h), true),
                _ => (Image::default(), false),
            };
            HItem { name: ss(&name), path: ss(&e.path), meta: ss(&meta.join(" · ")), thumb, has_thumb: has }
        })
        .collect();
    drop(cache);
    let model = Rc::new(VecModel::from(rows));
    app.global::<S>().set_history(model.clone().into());
    HIST.with(|h| *h.borrow_mut() = Some(model));

    // miniaturas que faltan: se bajan en segundo plano, una a una
    let missing: Vec<(usize, String)> = {
        let cache = core.thumb_cache.lock().unwrap();
        entries.iter().enumerate().filter(|(_, e)| !e.thumb.is_empty() && !cache.contains_key(&e.thumb)).take(40).map(|(i, e)| (i, e.thumb.clone())).collect()
    };
    if missing.is_empty() { return; }
    let c2 = core.clone();
    std::thread::spawn(move || {
        for (i, url) in missing {
            if let Some((px, w, h)) = load_thumb(&url, 96, 54) {
                c2.thumb_cache.lock().unwrap().insert(url, (px.clone(), w, h));
                c2.ui(move |_a| {
                    HIST.with(|hm| {
                        if let Some(m) = hm.borrow().as_ref() {
                            if let Some(mut row) = m.row_data(i) {
                                row.thumb = buf_to_image(&px, w, h);
                                row.has_thumb = true;
                                m.set_row_data(i, row);
                            }
                        }
                    });
                });
            }
        }
    });
}

fn buf_to_image(px: &[u8], w: u32, h: u32) -> Image {
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    buf.make_mut_bytes().copy_from_slice(px);
    Image::from_rgba8(buf)
}

/// Descarga y reduce una imagen: (RGBA, ancho, alto).
fn load_thumb(url: &str, w: u32, h: u32) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = crate::deps::fetch_bytes(url, 6_000_000)?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.resize_to_fill(w, h, image::imageops::FilterType::Triangle).to_rgba8();
    Some((img.into_raw(), w, h))
}

// ====================================================================
//  Utilidades de URL
// ====================================================================
fn is_http(u: &str) -> bool {
    let u = u.trim();
    (u.starts_with("http://") || u.starts_with("https://")) && u.len() > 12 && u.contains('.') && !u.contains(' ')
}

fn known_site(u: &str) -> bool {
    ["youtube.com", "youtu.be", "tiktok.com", "instagram.com", "twitter.com", "x.com", "facebook.com", "vimeo.com", "twitch.tv", "soundcloud.com", "reddit.com", "dailymotion.com", "bilibili.com"]
        .iter()
        .any(|d| u.contains(d))
}

fn set_url(core: &Arc<Core>, app: &App, url: &str, auto_fetch: bool) {
    app.global::<S>().set_url(ss(url));
    on_url_edited(core, app, url, auto_fetch);
}

fn on_url_edited(core: &Arc<Core>, app: &App, url: &str, auto_fetch: bool) {
    let s = app.global::<S>();
    s.set_is_playlist(url.contains("list="));
    // Al cambiar de URL no deben reutilizarse los formatos ni los metadatos
    // del video anterior.
    {
        let mut f = core.formats.lock().unwrap();
        f.loaded = false;
        f.video_ids.clear();
        f.audio_ids.clear();
        f.video_info.clear();
        f.audio_info.clear();
    }
    *core.meta.lock().unwrap() = Default::default();
    if auto_fetch && is_http(url) && s.get_st() == 1 && !s.get_fetching() {
        let weak = app.as_weak();
        let core = core.clone();
        let url = url.to_string();
        URL_TIMER.with(|t| {
            t.start(slint::TimerMode::SingleShot, Duration::from_millis(700), move || {
                if let Some(a) = weak.upgrade() {
                    if a.global::<S>().get_url() == url.as_str() {
                        fetch(&core, &a);
                    }
                }
            });
        });
    }
}

// ====================================================================
//  Consulta de formatos
// ====================================================================
fn fetch(core: &Arc<Core>, app: &App) {
    URL_TIMER.with(|t| t.stop());
    let s = app.global::<S>();
    let lang = core.lang();
    let url = s.get_url().trim().to_string();
    if url.is_empty() || s.get_fetching() || s.get_st() != 1 {
        if s.get_st() != 1 {
            dialog(app, 3, &ui(&lang, "err_no_deps"), "", "");
        }
        return;
    }
    s.set_fetching(true);
    s.set_title_line(ss(&ui(&lang, "msg_fetching")));
    push_log(app, &format!("$ yt-dlp -J {url}"));
    let cookies = pick(&COOKIES, s.get_cookies_idx()).to_string();
    let core = core.clone();
    std::thread::spawn(move || {
        let r = jobs::fetch_formats(&core, &url, &cookies);
        let lang = core.lang();
        match r {
            Ok(res) => {
                {
                    let mut f = core.formats.lock().unwrap();
                    f.video_ids = res.video.iter().map(|(i, _)| i.clone()).collect();
                    f.audio_ids = res.audio.iter().map(|(i, _)| i.clone()).collect();
                    f.video_info = res.video_info.clone();
                    f.audio_info = res.audio_info.clone();
                    f.loaded = true;
                }
                *core.meta.lock().unwrap() = crate::core::Meta {
                    title: res.title.clone(),
                    artist: res.uploader.clone(),
                    thumb: res.thumbnail.clone(),
                    description: res.description.clone(),
                    date: res.upload_date.clone(),
                    url: res.webpage_url.clone(),
                };
                let no_v = res.video.is_empty();
                let no_a = res.audio.is_empty();
                let mut vopts: Vec<String> = res.video.iter().map(|(_, l)| l.clone()).collect();
                let mut aopts: Vec<String> = res.audio.iter().map(|(_, l)| l.clone()).collect();
                if no_v { vopts.push(ui(&lang, "best_format")); }
                if no_a { aopts.push(ui(&lang, "best_format")); }
                let n = res.video.len() + res.audio.len();
                let found = uif(&lang, "log_found", &[("t", &res.title), ("n", &n.to_string())]);
                let thumb = (!res.thumbnail.is_empty()).then(|| load_thumb(&res.thumbnail, 440, 248)).flatten();
                core.ui(move |a| {
                    let s = a.global::<S>();
                    s.set_fetching(false);
                    s.set_video_opts(ModelRc::new(VecModel::from(vopts.iter().map(|x| ss(x)).collect::<Vec<_>>())));
                    s.set_audio_opts(ModelRc::new(VecModel::from(aopts.iter().map(|x| ss(x)).collect::<Vec<_>>())));
                    if let Some(c) = CORE.with(|c| c.borrow().clone()) {
                        sync_formats_to_quality(&c, a);
                    }
                    s.set_title_line(ss(&res.title));
                    s.set_pv_title(ss(&res.title));
                    // La descripcion de YouTube suele traer lineas vacias;
                    // al quitarlas las dos lineas visibles de la preview
                    // muestran texto real en vez de un hueco cortado.
                    let desc = compact_desc(
                        &res.description
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    s.set_pv_desc(ss(&desc));
                    s.set_pv_chips(ModelRc::new(VecModel::from(res.chips.iter().map(|x| ss(x)).collect::<Vec<_>>())));
                    match thumb {
                        Some((px, w, h)) => { s.set_pv_thumb(buf_to_image(&px, w, h)); s.set_pv_has_thumb(true); }
                        None => s.set_pv_has_thumb(false),
                    }
                    s.set_has_preview(true);
                    push_log(a, &found);
                    if no_v { push_log(a, &ui(&lang, "log_no_video")); }
                    if no_a { push_log(a, &ui(&lang, "log_no_audio")); }
                    show_toast(a, &ui(&lang, "toast_formats_ok"), 1);
                });
                // Aviso sonoro cuando los formatos ya estan disponibles.
                core.beep(true);
            }
            Err(e) => {
                core.ui(move |a| {
                    let s = a.global::<S>();
                    s.set_fetching(false);
                    s.set_title_line(ss(""));
                    push_log(a, &format!("[ERROR] {e}"));
                    dialog(a, 3, &ui(&lang, "p_error_formats"), &e.chars().take(300).collect::<String>(), "");
                });
            }
        }
    });
}

// ====================================================================
//  Payloads
// ====================================================================
fn build_payload(core: &Core, app: &App) -> Payload {
    let s = app.global::<S>();
    let f = core.formats.lock().unwrap();
    let quality = pick(&QUALITIES, s.get_quality_idx()).to_string();
    let manual = quality.is_empty();
    let vid = if manual && f.loaded { f.video_ids.get(s.get_video_idx().max(0) as usize).cloned() } else { None };
    let aid = if f.loaded { f.audio_ids.get(s.get_audio_idx().max(0) as usize).cloned() } else { None };
    let meta = core.meta.lock().unwrap().clone();
    let out_dir = s.get_out_dir().trim().to_string();
    Payload {
        url: s.get_url().trim().to_string(),
        mode: pick(&MODES, s.get_mode_idx()).into(),
        direct: s.get_direct_idx() == 1,
        lang: core.lang(),
        quality,
        video_id: vid,
        audio_id: aid,
        audio_codec: pick(&ACODECS, s.get_acodec_idx()).into(),
        audio_bitrate: pick(&BITRATES, s.get_abitrate_idx()).into(),
        audio_norm: s.get_audio_norm(),
        out_dir: (!out_dir.is_empty()).then_some(out_dir),
        vendor: pick(&VENDORS, s.get_hw_idx()).into(),
        codec: pick(&CODECS, s.get_codec_idx()).into(),
        cq: s.get_cq(),
        trim_start: s.get_trim_start().to_string(),
        trim_end: s.get_trim_end().to_string(),
        subs: s.get_subs(),
        subs_embed: s.get_subs_embed(),
        embed_thumb: s.get_embed_thumb(),
        embed_meta: s.get_embed_meta(),
        cookies_browser: pick(&COOKIES, s.get_cookies_idx()).into(),
        meta_title: meta.title.clone(),
        meta_artist: meta.artist,
        meta_description: meta.description,
        meta_date: meta.date,
        meta_url: meta.url,
        thumb_url: meta.thumb,
        q_title: meta.title,
        playlist: false,
    }
}

/// Marca y selecciona en las listas de formatos lo que corresponde a la
/// calidad rapida (p. ej. con "H.264 + AAC" se eligen las pistas avc1/mp4a),
/// para que lo que se ve coincida con lo que se va a descargar.
fn sync_formats_to_quality(core: &Core, app: &App) {
    let s = app.global::<S>();
    let f = core.formats.lock().unwrap();
    if !f.loaded {
        return;
    }
    let quality = pick(&QUALITIES, s.get_quality_idx());
    let (v, a) = jobs::pick_for_quality(quality, &f.video_info, &f.audio_info);
    s.set_video_idx(v as i32);
    s.set_audio_idx(a as i32);
    s.set_video_best(v as i32);
    s.set_audio_best(a as i32);
}

fn pick_folder(initial: &str) -> Option<String> {
    let mut d = rfd::FileDialog::new();
    if !initial.is_empty() && std::path::Path::new(initial).is_dir() {
        d = d.set_directory(initial);
    }
    d.pick_folder().map(|p| p.to_string_lossy().to_string())
}

// ====================================================================
//  Dependencias
// ====================================================================
pub fn refresh_deps(core: &Arc<Core>) {
    let core = core.clone();
    std::thread::spawn(move || {
        core.refresh_bins();
        let b = core.bins();
        let mut rows = Vec::new();
        for (key, name, path) in [("yt-dlp", "yt-dlp", b.yt_dlp.clone()), ("ffmpeg", "ffmpeg / ffprobe", b.ffmpeg.clone())] {
            let (installed, version, p) = match &path {
                Some(p) => (true, core.deps.binary_version(p), p.to_string_lossy().to_string()),
                None => (false, String::new(), String::new()),
            };
            let version = if key == "yt-dlp" && installed {
                let ch = core.deps.ytdlp_channel(path.as_ref().unwrap());
                if ch.is_empty() { version } else { format!("{version} ({ch})") }
            } else { version };
            rows.push((key.to_string(), name.to_string(), installed, version, p));
        }
        let missing: Vec<&str> = [("yt-dlp", b.yt_dlp.is_some()), ("ffmpeg", b.ffmpeg.is_some() && b.ffprobe.is_some())]
            .iter().filter(|(_, ok)| !ok).map(|(n, _)| *n).collect();
        let miss = missing.join(", ");
        let gpu = detect_gpu();
        core.ui(move |a| {
            let s = a.global::<S>();
            s.set_deps(ModelRc::new(VecModel::from(
                rows.into_iter().map(|(k, n, i, v, p)| DepInfo { key: ss(&k), name: ss(&n), installed: i, version: ss(&v), path: ss(&p) }).collect::<Vec<_>>(),
            )));
            s.set_st(if miss.is_empty() { 1 } else { 2 });
            s.set_st_missing(ss(&miss));
            if s.get_gpu() != gpu.as_str() {
                s.set_gpu(ss(&gpu));
            }
        });
    });
}

fn detect_gpu() -> String {
    if util::cmd("nvidia-smi").arg("-L").output().map(|o| o.status.success() && !o.stdout.is_empty()).unwrap_or(false) {
        return "nvidia".into();
    }
    #[cfg(windows)]
    {
        if let Ok(o) = util::cmd("powershell")
            .args(["-NoProfile", "-Command", "(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name) -join ';'"])
            .output()
        {
            let t = String::from_utf8_lossy(&o.stdout).to_lowercase();
            if t.contains("nvidia") { return "nvidia".into(); }
            if t.contains("amd") || t.contains("radeon") { return "amd".into(); }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(o) = util::cmd("sh").args(["-c", "lspci 2>/dev/null | grep -iE 'vga|3d'"]).output() {
            let t = String::from_utf8_lossy(&o.stdout).to_lowercase();
            if t.contains("nvidia") { return "nvidia".into(); }
            if t.contains("amd") || t.contains("radeon") { return "amd".into(); }
        }
    }
    "cpu".into()
}

/// Primer arranque: detecta y, si falta algo, lo descarga mostrando el modal.
pub fn startup_deps(core: &Arc<Core>, first_run_defaults: bool) {
    let core = core.clone();
    std::thread::spawn(move || {
        let lang = core.lang();
        core.refresh_bins();
        let b = core.bins();
        let missing = b.yt_dlp.is_none() || b.ffmpeg.is_none() || b.ffprobe.is_none();
        if missing {
            let (title, msg) = (crate::lang::tr(&lang, "prep_title"), crate::lang::tr(&lang, "prep_msg"));
            core.ui(move |a| {
                let s = a.global::<S>();
                s.set_dep_title(ss(&title));
                s.set_dep_msg(ss(&msg));
                s.set_dep_progress(0.0);
                s.set_dep_busy(true);
                s.set_dep_prep(true);
                s.set_dep_show(true);
            });
            let c2 = core.clone();
            let errors = core.deps.ensure(move |f| {
                let msg = crate::lang::trf(&c2.lang(), "deps_downloading_pct", &[("pct", &format!("{}", (f * 100.0) as i32))]);
                c2.ui(move |a| {
                    let s = a.global::<S>();
                    s.set_dep_progress(f as f32);
                    s.set_dep_msg(ss(&msg));
                });
            });
            let failed = !errors.is_empty();
            let err_text = errors.join("\n");
            core.ui(move |a| {
                let s = a.global::<S>();
                s.set_dep_busy(false);
                if failed {
                    s.set_dep_msg(ss(&err_text));
                } else {
                    s.set_dep_progress(1.0);
                    s.set_dep_show(false);
                    s.set_dep_prep(false);
                }
            });
        } else if core.cfg.lock().unwrap().pref_bool("auto_update_deps", true) {
            // actualiza yt-dlp (si lo gestiona la app) cuando tiene mas de 7 dias
            if let Some(p) = core.deps.managed_path("yt-dlp") {
                let old = std::fs::metadata(&p).and_then(|m| m.modified()).ok()
                    .and_then(|m| m.elapsed().ok()).map(|e| e > Duration::from_secs(7 * 86_400)).unwrap_or(false);
                if old {
                    let _ = core.deps.download_ytdlp(|_| {});
                }
            }
        }
        refresh_deps(&core);
        if first_run_defaults {
            let gpu = detect_gpu();
            let idx = idx_of(&VENDORS, &gpu);
            core.ui(move |a| {
                let s = a.global::<S>();
                s.set_hw_idx(idx);
                s.set_local_hw_idx(idx);
            });
        }
    });
}

fn dep_action(core: &Arc<Core>, app: &App, key: &str, _action: &str) {
    let s = app.global::<S>();
    if s.get_dep_busy() { return; }
    let lang = core.lang();
    s.set_dep_title(ss(&if key == "ffmpeg" { "ffmpeg / ffprobe".to_string() } else { "yt-dlp".to_string() }));
    s.set_dep_msg(ss(&ui(&lang, "dep_action_running")));
    s.set_dep_progress(0.0);
    s.set_dep_busy(true);
    s.set_dep_prep(false);
    s.set_dep_show(true);
    let key = key.to_string();
    let core = core.clone();
    std::thread::spawn(move || {
        let c2 = core.clone();
        let prog = move |f: f64| c2.ui(move |a| a.global::<S>().set_dep_progress(f as f32));
        let r = if key == "ffmpeg" { core.deps.download_ffmpeg(prog) } else { core.deps.download_ytdlp(prog).map(|_| ()) };
        let lang = core.lang();
        let (msg, ok) = match r { Ok(()) => (ui(&lang, "dep_action_done"), true), Err(e) => (format!("{}\n{}", ui(&lang, "dep_action_error"), e), false) };
        core.ui(move |a| {
            let s = a.global::<S>();
            s.set_dep_busy(false);
            s.set_dep_msg(ss(&msg));
            if ok { s.set_dep_progress(1.0); }
        });
        refresh_deps(&core);
    });
}

// ====================================================================
//  Cookies
// ====================================================================
fn cookies_available(browser: &str) -> bool {
    #[cfg(windows)]
    {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let roaming = std::env::var("APPDATA").unwrap_or_default();
        let chromium = |rel: &str| {
            let base = std::path::Path::new(&local).join(rel).join("User Data").join("Default");
            base.join("Network").join("Cookies").exists() || base.join("Cookies").exists()
        };
        match browser {
            "chrome" => chromium("Google\\Chrome"),
            "edge" => chromium("Microsoft\\Edge"),
            "brave" => chromium("BraveSoftware\\Brave-Browser"),
            "vivaldi" => chromium("Vivaldi"),
            "opera" => std::path::Path::new(&roaming).join("Opera Software\\Opera Stable\\Network\\Cookies").exists(),
            "firefox" => std::fs::read_dir(std::path::Path::new(&roaming).join("Mozilla\\Firefox\\Profiles"))
                .map(|d| d.filter_map(Result::ok).any(|e| e.path().join("cookies.sqlite").exists()))
                .unwrap_or(false),
            _ => true,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = browser;
        true
    }
}

// ====================================================================
//  Ventana
// ====================================================================
fn with_win<R>(app: &App, f: impl FnOnce(&winit::window::Window) -> R) -> Option<R> {
    app.window().with_winit_window(f)
}

fn resize_dir(i: i32) -> winit::window::ResizeDirection {
    use winit::window::ResizeDirection::*;
    match i { 0 => West, 1 => East, 2 => South, 3 => North, 4 => SouthEast, _ => SouthWest }
}

fn local_pick_set(core: &Arc<Core>, app: &App, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    let lang = core.lang();
    let s = app.global::<S>();
    let shown = if paths.len() == 1 {
        paths[0].clone()
    } else {
        let names: Vec<String> = paths
            .iter()
            .map(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.clone()))
            .collect();
        uif(&lang, "local_n_files", &[("n", &paths.len().to_string()), ("list", &names.join(", "))])
    };
    s.set_local_file(ss(&shown));
    s.set_tab(1);
    let toast = if paths.len() == 1 { ui(&lang, "toast_file_loaded") } else { uif(&lang, "toast_files_loaded", &[("n", &paths.len().to_string())]) };
    LOCAL_FILES.with(|f| *f.borrow_mut() = paths);
    show_toast(app, &toast, 1);
}

/// Enlaces http(s) de un texto (uno por linea), sin repetir y en orden.
fn parse_links(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if is_http(l) && !out.iter().any(|u| u == l) {
            out.push(l.to_string());
        }
    }
    out
}

// ====================================================================
//  Cableado de callbacks
// ====================================================================
pub fn setup(core: &Arc<Core>, app: &App) {
    CORE.with(|c| *c.borrow_mut() = Some(core.clone()));
    let s = app.global::<S>();
    let weak = app.as_weak();

    macro_rules! cb {
        ($on:ident, |$a:ident, $c:ident $(, $arg:ident : $ty:ty)*| $body:block) => {{
            let w = weak.clone();
            let $c = core.clone();
            s.$on(move |$($arg: $ty),*| {
                if let Some($a) = w.upgrade() { $body }
            });
        }};
    }

    // ---- ventana
    cb!(on_win_minimize, |a, _c| { with_win(&a, |w| w.set_minimized(true)); });
    cb!(on_win_maximize, |a, _c| {
        let max = with_win(&a, |w| { let m = !w.is_maximized(); w.set_maximized(m); m }).unwrap_or(false);
        a.global::<S>().set_maximized(max);
    });
    cb!(on_win_close, |a, c| { save_prefs(&c, &a); let _ = slint::quit_event_loop(); });
    cb!(on_win_drag, |a, _c| { with_win(&a, |w| { let _ = w.drag_window(); }); });
    cb!(on_win_resize, |a, _c, i: i32| { with_win(&a, |w| { let _ = w.drag_resize_window(resize_dir(i)); }); });

    // ---- idioma / tema
    cb!(on_toggle_lang, |a, c| {
        {
            let mut l = c.lang.lock().unwrap();
            // pasa al siguiente idioma de la lista
            let langs = i18n_data::LANGS;
            let i = langs.iter().position(|(code, _)| *code == l.as_str()).unwrap_or(0);
            *l = langs[(i + 1) % langs.len()].0.to_string();
        }
        apply_lang(&c, &a);
        save_prefs(&c, &a);
    });
    cb!(on_set_lang, |a, c, lang: SharedString| {
        let lang = if crate::lang::supported(&lang) { lang.as_str() } else { crate::lang::DEFAULT };
        {
            let mut l = c.lang.lock().unwrap();
            *l = lang.into();
        }
        apply_lang(&c, &a);
        save_prefs(&c, &a);
    });
    cb!(on_set_theme, |a, c, id: SharedString| {
        themes::apply(&a, &id);
        save_prefs(&c, &a);
    });

    // ---- URL
    cb!(on_url_edited, |a, c, t: SharedString| { on_url_edited(&c, &a, &t, true); });
    cb!(on_clear_url, |a, _c| {
        let s = a.global::<S>();
        s.set_url(ss(""));
        s.set_has_preview(false);
        s.set_title_line(ss(""));
        s.set_is_playlist(false);
    });
    cb!(on_paste_url, |a, c| {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            let t = text.trim().to_string();
            if is_http(&t) {
                set_url(&c, &a, &t, true);
                show_toast(&a, &ui(&c.lang(), "clip_pasted"), 0);
            }
        }
    });
    cb!(on_fetch_formats, |a, c| { fetch(&c, &a); });
    cb!(on_quality_changed, |a, c| { sync_formats_to_quality(&c, &a); });

    // ---- descargas
    cb!(on_start_download, |a, c| {
        let lang = c.lang();
        let s = a.global::<S>();
        if s.get_st() != 1 { dialog(&a, 3, &ui(&lang, "err_no_deps"), "", ""); return; }
        if s.get_url().trim().is_empty() { return; }
        let payload = build_payload(&c, &a);
        save_prefs(&c, &a);
        // Todo pasa por la cola (un solo hilo): si ya hay un trabajo en marcha,
        // este espera su turno en vez de ejecutarse a la vez.
        if c.running.load(Ordering::SeqCst) {
            show_toast(&a, &ui(&lang, "toast_enqueued"), 0);
        } else {
            s.set_progress(0.0);
            push_log(&a, &format!("$ {}", payload.url));
            show_toast(&a, &ui(&lang, "toast_start"), 0);
        }
        jobs::enqueue(&c, payload);
    });
    cb!(on_enqueue, |a, c| {
        let lang = c.lang();
        if a.global::<S>().get_url().trim().is_empty() { return; }
        jobs::enqueue(&c, build_payload(&c, &a));
        show_toast(&a, &ui(&lang, "toast_enqueued"), 1);
    });
    cb!(on_enqueue_playlist, |a, c| {
        let lang = c.lang();
        let url = a.global::<S>().get_url().trim().to_string();
        if url.is_empty() { return; }
        show_toast(&a, &ui(&lang, "toast_playlist_scan"), 0);
        let base = build_payload(&c, &a);
        let c2 = c.clone();
        std::thread::spawn(move || {
            let lang = c2.lang();
            match jobs::enqueue_playlist(&c2, &url, &base) {
                Ok((n, _)) if n > 0 => c2.toast(&uif(&lang, "toast_playlist_added", &[("n", &n.to_string())]), 1),
                Ok(_) => c2.toast(&ui(&lang, "toast_playlist_none"), 2),
                Err(e) => c2.toast(&e.chars().take(120).collect::<String>(), 3),
            }
        });
    });
    cb!(on_cancel, |_a, c| { jobs::cancel_all(&c); });
    cb!(on_batch_import, |a, c| {
        let lang = c.lang();
        if a.global::<S>().get_st() != 1 { dialog(&a, 3, &ui(&lang, "err_no_deps"), "", ""); return; }
        dialog(&a, 6, &ui(&lang, "batch_title"), &ui(&lang, "batch_hint"), "batch-links");
    });
    cb!(on_batch_load_file, |a, _c| {
        if let Some(f) = rfd::FileDialog::new().add_filter("Text", &["txt", "csv", "list"]).pick_file() {
            if let Ok(text) = std::fs::read_to_string(&f) {
                let s = a.global::<S>();
                let cur = s.get_dlg_input().to_string();
                let joined = if cur.trim().is_empty() { text } else { format!("{}\n{}", cur.trim_end(), text) };
                s.set_dlg_input(ss(&joined));
            }
        }
    });

    // ---- carpetas
    cb!(on_choose_folder, |a, c, which: i32| {
        let s = a.global::<S>();
        let cur = if which == 0 { s.get_out_dir() } else { s.get_local_out_dir() };
        if let Some(p) = pick_folder(&cur) {
            if which == 0 { s.set_out_dir(ss(&p)); update_space_hint(&c, &a); } else { s.set_local_out_dir(ss(&p)); }
            save_prefs(&c, &a);
        }
    });
    cb!(on_out_dir_edited, |a, c| { update_space_hint(&c, &a); });
    cb!(on_open_last_folder, |_a, c| {
        let last = c.last_output.lock().unwrap().clone();
        if let Some(parent) = std::path::Path::new(&last).parent() { let _ = open::that(parent); }
    });

    // ---- archivos locales
    cb!(on_pick_local, |a, c| {
        if let Some(f) = rfd::FileDialog::new()
            .add_filter("Media", &["mp4", "mkv", "webm", "mov", "avi", "flv", "wmv", "m4v", "ts", "mp3", "m4a", "aac", "wav", "flac", "ogg", "opus", "gif"])
            .pick_files()
        {
            local_pick_set(&c, &a, f.iter().map(|p| p.to_string_lossy().to_string()).collect());
        }
    });
    cb!(on_reencode_local, |a, c| {
        let lang = c.lang();
        let s = a.global::<S>();
        if s.get_st() != 1 { dialog(&a, 3, &ui(&lang, "err_no_deps"), "", ""); return; }
        let files = LOCAL_FILES.with(|f| f.borrow().clone());
        if files.is_empty() { return; }
        let p = LocalPayload {
            input: String::new(),
            out_dir: s.get_local_out_dir().to_string(),
            mode: pick(&LOCAL_MODES, s.get_local_mode_idx()).into(),
            audio_codec: pick(&ACODECS, s.get_local_acodec_idx()).into(),
            audio_norm: s.get_local_audio_norm(),
            vendor: pick(&VENDORS, s.get_local_hw_idx()).into(),
            codec: pick(&CODECS, s.get_local_codec_idx()).into(),
            cq: s.get_local_cq(),
            trim_start: s.get_local_trim_start().to_string(),
            trim_end: s.get_local_trim_end().to_string(),
            gif_fps: s.get_local_gif_fps().to_string(),
            gif_width: s.get_local_gif_width().to_string(),
        };
        save_prefs(&c, &a);
        let busy = c.running.load(Ordering::SeqCst);
        if files.len() > 1 {
            show_toast(&a, &uif(&lang, "toast_files_enqueued", &[("n", &files.len().to_string())]), 1);
        } else if busy {
            show_toast(&a, &ui(&lang, "toast_enqueued"), 0);
        }
        if !busy {
            s.set_progress(0.0);
        }
        // Un trabajo por archivo, todos con los mismos ajustes.
        for f in files {
            push_log(&a, &format!("$ ffmpeg -i \"{f}\""));
            jobs::enqueue_local(&c, LocalPayload { input: f, ..p.clone() });
        }
    });

    // ---- presets
    cb!(on_apply_preset, |a, c| {
        let lang = c.lang();
        let i = a.global::<S>().get_preset_idx();
        if i <= 0 {
            // "— Aplicar preset —": vuelve a los ajustes de fabrica.
            apply_settings(&a, &crate::config::default_settings());
            sync_formats_to_quality(&c, &a);
            show_toast(&a, &ui(&lang, "toast_defaults"), 1);
            return;
        }
        let settings = c.cfg.lock().unwrap().presets.get(i as usize - 1).map(|p| p.settings.clone());
        if let Some(v) = settings {
            apply_settings(&a, &v);
            sync_formats_to_quality(&c, &a);
            show_toast(&a, &ui(&lang, "toast_preset_applied"), 1);
        }
    });
    cb!(on_save_preset, |a, c| {
        dialog(&a, 5, &ui(&c.lang(), "preset_prompt"), "", "save-preset");
    });
    cb!(on_delete_preset, |a, c| {
        let lang = c.lang();
        let i = a.global::<S>().get_preset_idx();
        if i <= 0 { return; }
        {
            let mut cfg = c.cfg.lock().unwrap();
            if (i as usize) <= cfg.presets.len() { cfg.presets.remove(i as usize - 1); }
            cfg.save();
        }
        refresh_presets(&c, &a, None);
        show_toast(&a, &ui(&lang, "toast_preset_deleted"), 1);
    });

    // ---- dialogos
    cb!(on_dialog_accept, |a, c, action: SharedString, text: SharedString| {
        let lang = c.lang();
        let s = a.global::<S>();
        s.set_dlg(0);
        match action.as_str() {
            "save-preset" => {
                let name = text.trim().to_string();
                if name.is_empty() { return; }
                let settings = current_settings(&a);
                {
                    let mut cfg = c.cfg.lock().unwrap();
                    if let Some(p) = cfg.presets.iter_mut().find(|p| p.name == name) { p.settings = settings; }
                    else { cfg.presets.push(crate::config::Preset { name: name.clone(), settings }); }
                    cfg.save();
                }
                refresh_presets(&c, &a, Some(&name));
                show_toast(&a, &ui(&lang, "toast_preset_saved"), 1);
            }
            "batch-links" => {
                let links = parse_links(&text);
                if links.is_empty() {
                    show_toast(&a, &ui(&lang, "toast_batch_none"), 2);
                    return;
                }
                save_prefs(&c, &a);
                // Cada enlace con los ajustes actuales, pero sin los formatos
                // ni metadatos del video consultado: el trabajo los pide solo.
                let base = build_payload(&c, &a);
                let n = links.len();
                for url in links {
                    jobs::enqueue(&c, Payload {
                        q_title: url.clone(),
                        url,
                        video_id: None,
                        audio_id: None,
                        meta_title: String::new(),
                        meta_artist: String::new(),
                        meta_description: String::new(),
                        meta_date: String::new(),
                        meta_url: String::new(),
                        thumb_url: String::new(),
                        ..base.clone()
                    });
                }
                show_toast(&a, &uif(&lang, "toast_batch_added", &[("n", &n.to_string())]), 1);
            }
            "clear-history" => {
                {
                    let mut cfg = c.cfg.lock().unwrap();
                    cfg.history.clear();
                    cfg.save();
                }
                c.thumb_cache.lock().unwrap().clear();
                refresh_history(&a);
            }
            _ => {}
        }
        s.set_dlg_action(ss(""));
    });
    cb!(on_dialog_cancel, |a, _c| { a.global::<S>().set_dlg(0); });

    // ---- ajustes
    cb!(on_settings_opened, |a, c| {
        let s = a.global::<S>();
        s.set_pref_cookies_idx(s.get_cookies_idx());
        refresh_deps(&c);
    });
    cb!(on_prefs_changed, |a, c| { save_prefs(&c, &a); });
    cb!(on_test_cookies, |a, c| {
        let lang = c.lang();
        let i = a.global::<S>().get_cookies_idx();
        if i <= 0 { return; }
        let b = pick(&COOKIES, i);
        let name = ["", "Chrome", "Edge", "Firefox", "Brave", "Opera", "Vivaldi"][i as usize];
        if cookies_available(b) {
            dialog(&a, 2, &ui(&lang, "cookie_title_ok"), &crate::lang::trf(&lang, "cookie_ok", &[("b", name)]), "");
        } else {
            dialog(&a, 3, &ui(&lang, "cookie_title_err"), &crate::lang::trf(&lang, "cookie_err", &[("b", name)]), "");
        }
    });
    cb!(on_dep_action, |a, c, key: SharedString, action: SharedString| { dep_action(&c, &a, &key, &action); });
    cb!(on_redownload_deps, |a, c| {
        a.global::<S>().set_dep_show(false);
        startup_deps(&c, false);
    });

    // ---- cola
    cb!(on_queue_open, |_a, c, id: i32| {
        let out = c.queue.lock().unwrap().jobs.iter().find(|j| j.id == id).map(|j| j.output.clone());
        if let Some(o) = out {
            if let Some(parent) = std::path::Path::new(&o).parent() { let _ = open::that(parent); }
        }
    });
    cb!(on_queue_remove, |_a, c, id: i32| {
        c.queue.lock().unwrap().jobs.retain(|j| j.id != id || j.status == 1);
        c.emit_queue();
    });
    cb!(on_queue_clear, |_a, c| {
        c.queue.lock().unwrap().jobs.retain(|j| j.status == 1);
        c.emit_queue();
    });

    // ---- historial
    cb!(on_history_opened, |a, _c| { refresh_history(&a); });
    cb!(on_history_open, |_a, c, i: i32| {
        let p = c.cfg.lock().unwrap().history.get(i as usize).map(|e| e.path.clone());
        if let Some(p) = p { let _ = open::that(p); }
    });
    cb!(on_history_folder, |_a, c, i: i32| {
        let p = c.cfg.lock().unwrap().history.get(i as usize).map(|e| e.path.clone());
        if let Some(p) = p {
            if let Some(parent) = std::path::Path::new(&p).parent() { let _ = open::that(parent); }
        }
    });
    cb!(on_history_clear, |a, c| {
        let lang = c.lang();
        dialog(&a, 4, &ui(&lang, "hist_clear"), &ui(&lang, "hist_confirm"), "clear-history");
    });

    // ---- varios
    cb!(on_open_link, |_a, _c, u: SharedString| { let _ = open::that(u.as_str()); });
    cb!(on_soon, |a, c, _k: SharedString| {
        show_toast(&a, &ui(&c.lang(), "soon"), 0);
    });
    cb!(on_mode_changed, |_a, _c| {});

    // ---- eventos de la ventana: arrastrar archivos y portapapeles al volver
    let w2 = weak.clone();
    let c2 = core.clone();
    app.window().on_winit_window_event(move |_win, ev| {
        use winit::event::WindowEvent;
        if let Some(a) = w2.upgrade() {
            match ev {
                WindowEvent::DroppedFile(p) => {
                    a.global::<S>().set_local_dragging(false);
                    DROPPED.with(|d| d.borrow_mut().push(p.to_string_lossy().to_string()));
                    let (w, c) = (w2.clone(), c2.clone());
                    DROP_TIMER.with(|t| {
                        t.start(slint::TimerMode::SingleShot, Duration::from_millis(150), move || {
                            let paths = DROPPED.with(|d| std::mem::take(&mut *d.borrow_mut()));
                            if let Some(a) = w.upgrade() {
                                local_pick_set(&c, &a, paths);
                            }
                        });
                    });
                }
                WindowEvent::HoveredFile(_) => a.global::<S>().set_local_dragging(true),
                WindowEvent::HoveredFileCancelled => a.global::<S>().set_local_dragging(false),
                WindowEvent::Focused(true) => {
                    let s = a.global::<S>();
                    if s.get_modal() == 0 && s.get_url().is_empty() && s.get_tab() == 0 && s.get_st() == 1 {
                        if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                            let t = text.trim().to_string();
                            if is_http(&t) && known_site(&t) {
                                set_url(&c2, &a, &t, true);
                                show_toast(&a, &ui(&c2.lang(), "clip_pasted"), 0);
                            }
                        }
                    }
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    // Slint aplica la escala nueva despues de este evento.
                    let w = w2.clone();
                    slint::Timer::single_shot(Duration::ZERO, move || {
                        if let Some(a) = w.upgrade() {
                            themes::refresh_logo(&a);
                        }
                    });
                }
                WindowEvent::Resized(_) => {
                    let m = with_win(&a, |w| w.is_maximized()).unwrap_or(false);
                    if a.global::<S>().get_maximized() != m { a.global::<S>().set_maximized(m); }
                }
                _ => {}
            }
        }
        slint::winit_030::EventResult::Propagate
    });
}
