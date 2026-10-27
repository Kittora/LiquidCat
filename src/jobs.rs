//! Trabajos: consulta de formatos, descarga + reencode, archivos locales y cola.
//! Port de la logica de app.py (clase Api).
use crate::core::{Core, Job};
use crate::lang::{tr, trf};
use crate::media;
use crate::util;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ====================================================================
//  Datos de entrada
// ====================================================================
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct Payload {
    pub url: String,
    pub mode: String, // video | audio
    pub direct: bool,
    pub lang: String,
    pub quality: String,
    pub video_id: Option<String>,
    pub audio_id: Option<String>,
    pub audio_codec: String,
    pub audio_bitrate: String,
    pub audio_norm: bool,
    pub out_dir: Option<String>,
    pub vendor: String,
    pub codec: String,
    pub cq: i32,
    pub trim_start: String,
    pub trim_end: String,
    pub subs: bool,
    pub subs_embed: bool,
    pub embed_thumb: bool,
    pub embed_meta: bool,
    pub cookies_browser: String,
    pub meta_title: String,
    pub meta_artist: String,
    pub meta_description: String,
    pub meta_date: String,
    pub meta_url: String,
    pub thumb_url: String,
    pub q_title: String,
    /// Entrada de una playlist: se espera un poco antes de descargarla para no
    /// disparar el limite de peticiones del sitio.
    #[serde(default)]
    pub playlist: bool,
}

#[derive(Clone, Default)]
pub struct LocalPayload {
    pub input: String,
    pub out_dir: String,
    pub mode: String, // video | audio | gif
    pub audio_codec: String,
    pub audio_norm: bool,
    pub vendor: String,
    pub codec: String,
    pub cq: i32,
    pub trim_start: String,
    pub trim_end: String,
    pub gif_fps: String,
    pub gif_width: String,
}

/// Lo que ejecuta un trabajo de la cola. Todo pasa por la misma cola (un solo
/// hilo), porque los trabajos comparten la cancelacion, el progreso y la
/// ultima salida de `Core`.
#[derive(Clone)]
pub enum Task {
    Download(Box<Payload>),
    Local(Box<LocalPayload>),
}

/// Datos de un formato para elegirlo segun la calidad rapida.
#[derive(Clone, Default)]
pub struct FmtInfo {
    pub codec: String,
    pub height: i64,
    pub ext: String,
}

pub struct FormatsResult {
    pub title: String,
    pub video: Vec<(String, String)>,
    pub audio: Vec<(String, String)>,
    pub video_info: Vec<FmtInfo>,
    pub audio_info: Vec<FmtInfo>,
    pub chips: Vec<String>,
    pub description: String,
    pub thumbnail: String,
    pub uploader: String,
    pub upload_date: String,
    pub webpage_url: String,
}

// ====================================================================
//  Consulta de formatos
// ====================================================================
pub fn fetch_formats(core: &Core, url: &str, cookies: &str) -> Result<FormatsResult, String> {
    let lang = core.lang();
    let ytdlp = core.bins().yt_dlp.ok_or_else(|| tr(&lang, "no_ytdlp"))?;
    let mut c = util::cmd(&ytdlp);
    c.args(["-J", "--no-warnings", "--no-playlist"]).args(core.cookies_args(cookies)).arg(url);
    let out = run_with_timeout(c, Duration::from_secs(90)).map_err(|e| if e == "timeout" { tr(&lang, "timeout") } else { e })?;
    if !out.status {
        let err = String::from_utf8_lossy(&out.stderr).trim().chars().take(500).collect::<String>();
        return Err(if err.is_empty() { tr(&lang, "ytdlp_failed") } else { err });
    }
    let data: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut video = Vec::new();
    let mut audio = Vec::new();
    let mut video_info = Vec::new();
    let mut audio_info = Vec::new();
    for f in data.get("formats").and_then(Value::as_array).into_iter().flatten() {
        let vcodec = f.get("vcodec").and_then(Value::as_str).unwrap_or("none");
        let acodec = f.get("acodec").and_then(Value::as_str).unwrap_or("none");
        let fid = f.get("format_id").and_then(Value::as_str).unwrap_or("?").to_string();
        let ext = f.get("ext").and_then(Value::as_str).unwrap_or("?");
        let size = f.get("filesize").and_then(Value::as_f64).or_else(|| f.get("filesize_approx").and_then(Value::as_f64)).unwrap_or(0.0);
        let size_txt = util::human_size_tight(size);
        if vcodec != "none" {
            let res = f
                .get("format_note")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}p", f.get("height").and_then(Value::as_i64).map(|h| h.to_string()).unwrap_or("?".into())));
            let fps = f.get("fps").and_then(Value::as_f64).filter(|v| *v > 30.0).map(|v| format!("{}", v as i64)).unwrap_or_default();
            // Corto para que quepa en media columna: "1080p60 · H.264 · 12.3MB · mp4"
            let label = format!("{res}{fps} · {} · {size_txt} · {ext}", codec_name(vcodec));
            video.push((fid.clone(), label));
            video_info.push(FmtInfo {
                codec: vcodec.to_string(),
                height: f.get("height").and_then(Value::as_i64).unwrap_or(0),
                ext: ext.to_string(),
            });
        }
        if acodec != "none" && vcodec == "none" {
            let abr = f.get("abr").and_then(Value::as_f64).map(|v| format!(" {}k", v as i64)).unwrap_or_default();
            let label = format!("{}{abr} · {size_txt} · {ext}", codec_name(acodec));
            audio.push((fid, label));
            audio_info.push(FmtInfo { codec: acodec.to_string(), height: 0, ext: ext.to_string() });
        }
    }
    video.reverse();
    audio.reverse();
    video_info.reverse();
    audio_info.reverse();

    let s = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let uploader = if !s("uploader").is_empty() { s("uploader") } else { s("channel") };
    let webpage_url = if !s("webpage_url").is_empty() { s("webpage_url") } else if !s("original_url").is_empty() { s("original_url") } else { url.to_string() };
    let label = |k: &str| crate::lang::ui(&lang, k);
    let mut chips = Vec::new();
    if !uploader.is_empty() {
        chips.push(format!("{}{}", label("chips_uploader"), uploader));
    }
    if let Some(d) = data.get("duration").and_then(Value::as_f64) {
        chips.push(format!("{}{}", label("chips_duration"), util::fmt_duration(d)));
    }
    if let Some(v) = data.get("view_count").and_then(Value::as_u64).filter(|v| *v > 0) {
        chips.push(format!("{}{}", label("chips_views"), util::fmt_count(v)));
    }
    let ud = s("upload_date");
    if ud.len() == 8 && ud.chars().all(|c| c.is_ascii_digit()) {
        chips.push(format!("{}{}/{}/{}", label("chips_date"), &ud[6..8], &ud[4..6], &ud[0..4]));
    }
    if let Some(v) = data.get("like_count").and_then(Value::as_u64).filter(|v| *v > 0) {
        chips.push(format!("{}{}", label("chips_like"), util::fmt_count(v)));
    }
    Ok(FormatsResult {
        title: s("title"),
        video,
        audio,
        video_info,
        audio_info,
        chips,
        description: s("description"),
        thumbnail: s("thumbnail"),
        uploader,
        upload_date: ud,
        webpage_url,
    })
}

/// Nombre corto y conocido de un codec de yt-dlp ("avc1.640028" -> "H.264").
fn codec_name(c: &str) -> String {
    let l = c.to_lowercase();
    let name = if l.starts_with("avc") || l.starts_with("h264") {
        "H.264"
    } else if l.starts_with("hev") || l.starts_with("hvc") || l.starts_with("h265") {
        "HEVC"
    } else if l.starts_with("av01") || l == "av1" {
        "AV1"
    } else if l.starts_with("vp09") || l.starts_with("vp9") {
        "VP9"
    } else if l.starts_with("vp8") {
        "VP8"
    } else if l.starts_with("mp4a") || l == "aac" {
        "AAC"
    } else if l.starts_with("opus") {
        "Opus"
    } else if l.starts_with("vorbis") {
        "Vorbis"
    } else if l.starts_with("mp3") {
        "MP3"
    } else if l.starts_with("flac") {
        "FLAC"
    } else {
        return c.split('.').next().unwrap_or(c).to_string();
    };
    name.to_string()
}

/// Indices (video, audio) que corresponden a la calidad rapida elegida, sobre
/// listas ordenadas de mejor a peor. "" = manual: el mejor de cada lista.
pub fn pick_for_quality(quality: &str, video: &[FmtInfo], audio: &[FmtInfo]) -> (usize, usize) {
    let q = quality.trim();
    if q == "h264" {
        let v = video
            .iter()
            .position(|f| f.codec.starts_with("avc1") && f.ext == "mp4")
            .or_else(|| video.iter().position(|f| f.codec.starts_with("avc1")))
            .unwrap_or(0);
        let a = audio
            .iter()
            .position(|f| f.codec.starts_with("mp4a"))
            .or_else(|| audio.iter().position(|f| f.ext == "m4a"))
            .unwrap_or(0);
        return (v, a);
    }
    if let Ok(h) = q.parse::<i64>() {
        let v = video.iter().position(|f| f.height > 0 && f.height <= h).unwrap_or(0);
        return (v, 0);
    }
    (0, 0)
}

struct Output {
    status: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_with_timeout(mut c: Command, limit: Duration) -> Result<Output, String> {
    use std::io::Read;
    use std::process::Stdio;
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(|e| e.to_string())?;
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let t1 = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = so.read_to_end(&mut v);
        v
    });
    let t2 = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = se.read_to_end(&mut v);
        v
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.success(),
            Ok(None) => {
                if start.elapsed() > limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("timeout".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    Ok(Output { status, stdout: t1.join().unwrap_or_default(), stderr: t2.join().unwrap_or_default() })
}

pub fn enqueue_playlist(core: &Arc<Core>, url: &str, base: &Payload) -> Result<(usize, String), String> {
    let ytdlp = core.bins().yt_dlp.ok_or_else(|| tr(&core.lang(), "no_ytdlp"))?;
    let mut c = util::cmd(&ytdlp);
    c.args(["--flat-playlist", "-J", "--no-warnings"]).args(core.cookies_args(&base.cookies_browser)).arg(url);
    let out = run_with_timeout(c, Duration::from_secs(120))?;
    let data: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut count = 0;
    for e in data.get("entries").and_then(Value::as_array).into_iter().flatten() {
        let vid = e.get("id").and_then(Value::as_str);
        let mut vurl = e.get("url").and_then(Value::as_str).map(str::to_string);
        if vurl.is_none() {
            vurl = vid.map(|v| format!("https://www.youtube.com/watch?v={v}"));
        }
        let Some(mut vurl) = vurl else { continue };
        if !vurl.starts_with("http") {
            match vid {
                Some(v) => vurl = format!("https://www.youtube.com/watch?v={v}"),
                None => continue,
            }
        }
        let mut p = base.clone();
        p.url = vurl.clone();
        p.q_title = e.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or(vurl);
        p.video_id = None;
        p.audio_id = None;
        p.meta_title = e.get("title").and_then(Value::as_str).unwrap_or("").to_string();
        p.meta_artist = e.get("uploader").or_else(|| e.get("channel")).and_then(Value::as_str).unwrap_or("").to_string();
        p.meta_description = String::new();
        p.meta_date = e.get("upload_date").and_then(Value::as_str).unwrap_or("").to_string();
        // La URL vacía hace que download_worker consulte los metadatos completos
        // de esta entrada antes de descargarla.
        p.meta_url = String::new();
        p.thumb_url = e.get("thumbnail").and_then(Value::as_str).unwrap_or("").to_string();
        p.playlist = true;
        enqueue(core, p);
        count += 1;
    }
    Ok((count, data.get("title").and_then(Value::as_str).unwrap_or("").to_string()))
}

// ====================================================================
//  Cola
// ====================================================================
pub fn enqueue(core: &Arc<Core>, payload: Payload) -> i32 {
    let title = if !payload.q_title.is_empty() { payload.q_title.clone() } else { payload.url.clone() };
    enqueue_task(core, title, Task::Download(Box::new(payload)))
}

pub fn enqueue_local(core: &Arc<Core>, payload: LocalPayload) -> i32 {
    let title = Path::new(&payload.input).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| payload.input.clone());
    enqueue_task(core, title, Task::Local(Box::new(payload)))
}

fn enqueue_task(core: &Arc<Core>, title: String, task: Task) -> i32 {
    let id = {
        let mut q = core.queue.lock().unwrap();
        q.cancelled = false; // un trabajo nuevo reactiva la cola
        q.seq += 1;
        let id = q.seq;
        q.jobs.push(Job { id, title, status: 0, output: String::new(), task });
        id
    };
    core.emit_queue();
    ensure_worker(core);
    id
}

fn ensure_worker(core: &Arc<Core>) {
    {
        let mut q = core.queue.lock().unwrap();
        if q.worker_alive {
            return;
        }
        q.worker_alive = true;
    }
    let core = core.clone();
    std::thread::spawn(move || queue_run(core));
}

fn queue_run(core: Arc<Core>) {
    loop {
        let job = {
            let mut q = core.queue.lock().unwrap();
            if q.cancelled {
                q.worker_alive = false;
                break;
            }
            let next = q.jobs.iter().position(|j| j.status == 0);
            match next {
                Some(i) => {
                    q.jobs[i].status = 1;
                    let job = q.jobs[i].clone();
                    q.current = Some(job.id);
                    job
                }
                None => {
                    q.worker_alive = false;
                    break;
                }
            }
        };
        core.emit_queue();
        core.cancel.store(false, Ordering::SeqCst);
        core.running.store(true, Ordering::SeqCst);
        core.log(&format!("[>] {}", job.title));
        {
            let title = job.title.clone();
            core.ui(move |a| {
                use slint::ComponentHandle;
                let s = a.global::<crate::S>();
                s.set_current_title(title.as_str().into());
                s.set_current_state(1);
            });
        }
        core.ui(|a| {
            use slint::ComponentHandle;
            let s = a.global::<crate::S>();
            s.set_busy(true);
            s.set_can_open_folder(false);
        });
        match &job.task {
            Task::Download(p) => download_worker(&core, p),
            Task::Local(p) => local_worker(&core, p),
        }
        core.queue.lock().unwrap().current = None;
        core.running.store(false, Ordering::SeqCst);
    }
    core.queue.lock().unwrap().current = None;
    core.ui(|a| {
        use slint::ComponentHandle;
        a.global::<crate::S>().set_busy(false);
    });
    core.emit_queue();
}

pub fn cancel_all(core: &Arc<Core>) {
    core.cancel.store(true, Ordering::SeqCst);
    {
        let mut q = core.queue.lock().unwrap();
        q.cancelled = true;
        q.jobs.retain(|j| j.status == 1);
    }
    core.emit_queue();
}

// ====================================================================
//  Registro amigable de lineas de yt-dlp / ffmpeg
// ====================================================================
fn friendly_log(lang: &str, raw: &str) -> Option<String> {
    let line = raw.trim();
    if line.is_empty() {
        return None;
    }
    const NOISE: &[&str] = &[
        "ffmpeg version", "built with", "configuration:", "libavutil", "libavcodec", "libavformat", "libavdevice", "libavfilter",
        "libswscale", "libswresample", "Metadata:", "major_brand", "minor_version", "compatible_brands", "creation_time",
        "handler_name", "timebase", "start_time", "Duration:", "Stream #", "Input #", "Output #", "Press [q]", "trailing",
        "frame=", "fps=", "speed=", "bitrate=", "total_size", "out_time", "progress=", "size=", "dup_frames", "drop_frames",
        "stream_", "chapter", "encoder:", "vendor_id", "isom",
    ];
    if NOISE.iter().any(|p| line.starts_with(p)) {
        return None;
    }
    let low = line.to_lowercase();
    const NOISE_LOW: &[&str] = &[
        "libx264", "libx265", "libsvtav1", "libaom", "libvpx", "aac @", "mp3 @", "flac @", "opus @", "mov,mp4", "matroska",
        "webm @", "flv @", "video:", "audio:", "exact 2 pass", "lookahead", "x264 [info]", "avc1", "h264", "hevc", "vp9", "av1",
    ];
    if NOISE_LOW.iter().any(|p| low.starts_with(p)) {
        return None;
    }
    if line.starts_with("[download]") {
        return None; // el progreso se muestra en la barra; el resto es ruido
    }
    if line.starts_with("[youtube]") || line.starts_with("[generic]") || line.starts_with("[Extractor]") {
        if line.contains("Extracting URL") {
            return Some(format!("[i] {}", tr(lang, "friendly_fetching")));
        }
        return None;
    }
    if line.contains("[Merger]") {
        return Some(format!("[i] {}", tr(lang, "friendly_merging")));
    }
    if line.contains("[ExtractAudio]") {
        return Some(format!("[i] {}", tr(lang, "friendly_audio_extract")));
    }
    if line.starts_with("[info]") {
        return None;
    }
    if line.contains("100%") && line.contains("of") {
        return Some(format!("[OK] {}", tr(lang, "friendly_downloaded")));
    }
    if line.starts_with("ERROR") {
        return Some(format!("[!] {line}"));
    }
    if line.chars().count() > 200 {
        return None;
    }
    Some(line.to_string())
}

/// Busca "NN.N%" en una linea de yt-dlp.
fn parse_percent(line: &str) -> Option<f64> {
    let idx = line.find('%')?;
    let head = &line[..idx];
    let start = head.rfind(|c: char| !(c.is_ascii_digit() || c == '.')).map(|i| i + 1).unwrap_or(0);
    head[start..].parse::<f64>().ok().map(|v| v.clamp(0.0, 100.0))
}

fn run_ytdlp(core: &Core, cmd: Command, label: &str) -> i32 {
    let lang = core.lang();
    let mut last = Instant::now() - Duration::from_secs(1);
    let rc = media::run_streaming(cmd, &core.cancel, |line| {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if line.starts_with("[download]") {
            if let Some(p) = parse_percent(line) {
                if last.elapsed() > Duration::from_millis(100) || p >= 100.0 {
                    core.progress(Some((p / 100.0) as f32), Some(format!("{label}: {p:.1}%")));
                    last = Instant::now();
                }
                return;
            }
        }
        if line.chars().count() < 300 {
            if let Some(f) = friendly_log(&lang, line) {
                core.log(&f);
            }
        }
    });
    if rc == -1 {
        core.log(&trf(&lang, "cancelled", &[("phase", label)]));
    }
    rc
}

/// Reencode de video con respaldo en CPU: si el encoder de la GPU falla (la
/// GPU no admite ese codec, p. ej. AV1 en NVENC antiguas, o no hay driver),
/// se repite el mismo trabajo con el encoder por software. `build` arma la
/// linea de ffmpeg a partir de los argumentos del encoder.
#[allow(clippy::too_many_arguments)]
fn encode_video(
    core: &Core,
    ffmpeg: &Path,
    vendor: &str,
    codec: &str,
    cq: i32,
    duration: Option<f64>,
    label: &str,
    build: impl Fn(Vec<String>) -> Vec<String>,
) -> i32 {
    let args = build(media::video_encoder_args(ffmpeg, vendor, codec, cq));
    let rc = run_ffmpeg(core, ffmpeg_cmd(ffmpeg, &args), duration, label);
    if rc == 0 || rc == -1 || vendor == "cpu" || core.cancel.load(Ordering::SeqCst) {
        return rc;
    }
    let gpu = match vendor {
        "nvidia" => "NVIDIA",
        "amd" => "AMD",
        other => other,
    };
    core.log(&trf(&core.lang(), "gpu_fallback", &[("gpu", gpu)]));
    core.progress(Some(0.0), Some(label.to_string()));
    let args = build(media::video_encoder_args(ffmpeg, "cpu", codec, cq));
    run_ffmpeg(core, ffmpeg_cmd(ffmpeg, &args), duration, label)
}

fn run_ffmpeg(core: &Core, cmd: Command, duration: Option<f64>, label: &str) -> i32 {
    let lang = core.lang();
    let mut duration = duration;
    let mut last = Instant::now() - Duration::from_secs(1);
    let rc = media::run_streaming(cmd, &core.cancel, |line| {
        let line = line.trim();
        if duration.is_none() && line.starts_with("Duration:") {
            if let Some(rest) = line.strip_prefix("Duration:") {
                let t = rest.trim().split(',').next().unwrap_or("");
                let p: Vec<&str> = t.split(':').collect();
                if p.len() == 3 {
                    if let (Ok(h), Ok(m), Ok(s)) = (p[0].parse::<f64>(), p[1].parse::<f64>(), p[2].parse::<f64>()) {
                        duration = Some(h * 3600.0 + m * 60.0 + s);
                    }
                }
            }
        }
        if let Some(v) = line.strip_prefix("out_time_ms=").or_else(|| line.strip_prefix("out_time_us=")) {
            if let (Ok(us), Some(d)) = (v.parse::<f64>(), duration) {
                if d > 0.0 && (last.elapsed() > Duration::from_millis(100)) {
                    let pct = (us / 1_000_000.0 / d * 100.0).clamp(0.0, 100.0);
                    core.progress(Some((pct / 100.0) as f32), Some(format!("{label}: {pct:.1}%")));
                    last = Instant::now();
                }
            }
            return;
        }
        const SKIP: &[&str] = &["out_time", "total_size", "progress=", "speed=", "fps=", "frame=", "bitrate=", "stream_", "dup_frames", "drop_frames"];
        if line.is_empty() || SKIP.iter().any(|p| line.starts_with(p)) {
            return;
        }
        if line.chars().count() < 300 {
            if let Some(f) = friendly_log(&lang, line) {
                core.log(&f);
            }
        }
    });
    if rc == -1 {
        core.log(&trf(&lang, "cancelled", &[("phase", label)]));
    }
    rc
}

// ====================================================================
//  Auxiliares de argumentos
// ====================================================================
/// Recorte como opciones de ENTRADA (van antes de cada `-i`). Con `-ss` de
/// entrada, un `-to` de salida se mide desde el inicio del recorte y acaba
/// funcionando como una duracion; como opcion de entrada es una posicion absoluta.
fn trim_in(start: &str, end: &str) -> Vec<String> {
    let (s, e) = (start.trim(), end.trim());
    let mut a = Vec::new();
    if !s.is_empty() {
        a.extend(["-ss".to_string(), s.to_string()]);
    }
    if !e.is_empty() {
        a.extend(["-to".to_string(), e.to_string()]);
    }
    a
}

fn download_sections(p: &Payload) -> Vec<String> {
    let (s, e) = (p.trim_start.trim(), p.trim_end.trim());
    if s.is_empty() && e.is_empty() {
        return vec![];
    }
    vec![
        "--download-sections".into(),
        format!("*{}-{}", if s.is_empty() { "0" } else { s }, if e.is_empty() { "inf" } else { e }),
        "--force-keyframes-at-cuts".into(),
    ]
}

/// Calidad "h264": se pide la mejor pista H.264 (avc1) y el mejor audio AAC
/// (mp4a/m4a), asi el mp4 final se reproduce en cualquier dispositivo aunque se
/// descargue tal cual, sin reencode.
fn h264_compat(p: &Payload) -> bool {
    p.quality.trim() == "h264"
}

fn video_format(p: &Payload) -> String {
    if h264_compat(p) {
        return "bestvideo[vcodec^=avc1][ext=mp4]/bestvideo[vcodec^=avc1]/bestvideo[ext=mp4]/bestvideo".into();
    }
    let q = p.quality.trim();
    if !q.is_empty() && q.chars().all(|c| c.is_ascii_digit()) {
        return format!("bestvideo[height<={q}]/best[height<={q}]");
    }
    p.video_id.clone().filter(|v| !v.is_empty()).unwrap_or_else(|| "bestvideo".into())
}

fn push_meta(a: &mut Vec<String>, key: &str, value: &str) {
    let v = value.trim().replace('\0', "");
    if !v.is_empty() {
        a.extend(["-metadata".to_string(), format!("{key}={v}")]);
    }
}

fn format_meta_date(raw: &str) -> String {
    let s = raw.trim();
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8])
    } else {
        s.to_string()
    }
}

fn metadata_args(embed: bool, p: &Payload) -> Vec<String> {
    if !embed {
        return Vec::new();
    }
    let mut a = Vec::new();
    // La descripción puede ser muy larga; 4000 caracteres bastan para que
    // reproductores y exploradores la muestren sin inflar el archivo.
    let desc: String = p.meta_description.trim().chars().take(4000).collect();
    let url = if p.meta_url.trim().is_empty() { p.url.trim() } else { p.meta_url.trim() };
    push_meta(&mut a, "title", &p.meta_title);
    push_meta(&mut a, "artist", &p.meta_artist);
    push_meta(&mut a, "album_artist", &p.meta_artist);
    push_meta(&mut a, "album", &p.meta_artist);
    push_meta(&mut a, "comment", &desc);
    push_meta(&mut a, "description", &desc);
    push_meta(&mut a, "synopsis", &desc);
    push_meta(&mut a, "date", &format_meta_date(&p.meta_date));
    push_meta(&mut a, "purl", url);
    a
}

fn ffmpeg_cmd(ffmpeg: &Path, args: &[String]) -> Command {
    let mut c = util::cmd(ffmpeg);
    c.args(args);
    c
}

fn strs<const N: usize>(a: [&str; N]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

// ====================================================================
//  Descarga + reencode
// ====================================================================
pub fn download_worker(core: &Arc<Core>, p: &Payload) {
    // Si el usuario descarga sin pulsar "Consultar formatos" (o si viene de
    // una playlist), no hay metadatos en el payload. Se consultan una vez
    // aquí para que el archivo final no salga sin título/descripción.
    let mut payload = p.clone();
    if payload.meta_url.trim().is_empty() {
        if let Ok(res) = fetch_formats(core, &payload.url, &payload.cookies_browser) {
            payload.meta_title = res.title;
            payload.meta_artist = res.uploader;
            payload.meta_description = res.description;
            payload.meta_date = res.upload_date;
            payload.meta_url = res.webpage_url;
            payload.thumb_url = res.thumbnail;
            if payload.q_title.trim().is_empty() {
                payload.q_title = payload.meta_title.clone();
            }
        }
    }
    let p = &payload;
    let lang = if p.lang.is_empty() { core.lang() } else { p.lang.clone() };
    let t = |k: &str| tr(&lang, k);
    *core.last_output.lock().unwrap() = String::new();
    {
        let mut m = core.meta.lock().unwrap();
        m.title = p.meta_title.clone();
        m.artist = p.meta_artist.clone();
        m.thumb = p.thumb_url.clone();
        m.description = p.meta_description.clone();
        m.date = p.meta_date.clone();
        m.url = if p.meta_url.trim().is_empty() { p.url.clone() } else { p.meta_url.clone() };
    }
    let bins = core.bins();
    let (Some(ytdlp), Some(ffmpeg)) = (bins.yt_dlp.clone(), bins.ffmpeg.clone()) else {
        core.finish(false, &t("no_ytdlp"));
        return;
    };
    let ffprobe = bins.ffprobe.clone().unwrap_or_else(|| ffmpeg.with_file_name(format!("ffprobe{}", util::EXE)));
    let out_dir = p.out_dir.clone().filter(|d| !d.trim().is_empty()).map(PathBuf::from).unwrap_or_else(util::downloads_dir);
    let _ = std::fs::create_dir_all(&out_dir);
    let tmp = std::env::temp_dir().join(format!(
        "ytreenc_{}_{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    ));
    let _ = std::fs::create_dir_all(&tmp);

    let result = (|| -> Result<(), String> {
        let video_sel = video_format(p);
        let audio_sel = if h264_compat(p) {
            "bestaudio[acodec^=mp4a]/bestaudio[ext=m4a]/bestaudio".to_string()
        } else {
            p.audio_id.clone().filter(|a| !a.is_empty()).unwrap_or_else(|| "bestaudio".into())
        };
        let sections = if p.direct { download_sections(p) } else { vec![] };
        let mut ytd: Vec<String> = strs(["--retries", "15", "--fragment-retries", "15"]);
        ytd.extend(core.cookies_args(&p.cookies_browser));
        if let Some(dir) = ffmpeg.parent() {
            if !dir.as_os_str().is_empty() {
                ytd.extend(["--ffmpeg-location".to_string(), dir.to_string_lossy().to_string()]);
            }
        }
        // La espera aleatoria solo se usa con playlists, y solo en la primera
        // descarga de cada entrada: en un trabajo suelto es tiempo perdido.
        let ytcmd = |fmt: &str, out: &Path, sleep: bool| {
            let mut c = util::cmd(&ytdlp);
            c.args(["-f", fmt, "-o"]).arg(out).args(&ytd).args(&sections).args(["--newline", "--no-playlist"]);
            if sleep {
                c.args(["--sleep-interval", "3", "--max-sleep-interval", "10"]);
            }
            c.arg(&p.url);
            c
        };
        let find = |prefix: &str| -> Option<PathBuf> {
            std::fs::read_dir(&tmp).ok()?.filter_map(Result::ok).map(|e| e.path()).find(|f| {
                f.file_name().map(|n| n.to_string_lossy().starts_with(prefix)).unwrap_or(false)
                    && !f.extension().map(|e| e == "part" || e == "ytdl").unwrap_or(false)
            })
        };
        let title_for = || -> String {
            if !p.meta_title.trim().is_empty() {
                return util::safe_name(&p.meta_title, "video");
            }
            let o = util::cmd(&ytdlp).args(["--get-filename", "-o", "%(title)s", "--no-playlist"]).arg(&p.url).output();
            match o {
                Ok(o) => util::safe_name(String::from_utf8_lossy(&o.stdout).trim(), "video"),
                Err(_) => "video".into(),
            }
        };
        let cancelled = || core.cancel.load(Ordering::SeqCst);

        // ---------------- modo solo audio ----------------
        if p.mode == "audio" {
            core.progress(Some(0.0), Some(t("dl_audio_only")));
            let rc = run_ytdlp(core, ytcmd(&audio_sel, &tmp.join("audio_src.%(ext)s"), p.playlist), &t("dl_audio_only"));
            if rc != 0 || cancelled() {
                return Err(t("failed_dl_audio"));
            }
            let Some(audio_file) = find("audio_src.") else { return Err(t("failed_no_audio")) };
            let title = title_for();
            if p.direct {
                let ext = audio_file.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or("m4a".into());
                let out = util::unique_path(&out_dir.join(format!("{title}.{ext}")));
                std::fs::copy(&audio_file, &out).map_err(|e| e.to_string())?;
                embed_audio_tags(core, &ffmpeg, &out, p, &tmp);
                core.finish_done(&out, "audio", None);
                return Ok(());
            }
            let duration = media::probe_duration(&ffprobe, &audio_file);
            let (ac, ext) = media::audio_output(&p.audio_codec);
            let ac = media::audio_args(ac, &p.audio_codec, &p.audio_bitrate, p.audio_norm);
            let out = util::unique_path(&out_dir.join(format!("{title}.{ext}")));
            let mut args = trim_in(&p.trim_start, &p.trim_end);
            args.extend(["-y".to_string(), "-i".into(), audio_file.to_string_lossy().to_string()]);
            args.extend(ac);
            args.extend(strs(["-map_metadata", "0", "-progress", "pipe:1", "-nostats"]));
            args.push(out.to_string_lossy().to_string());
            let label = trf(&lang, "conv_audio", &[("fmt", &p.audio_codec)]);
            core.progress(Some(0.0), Some(label.clone()));
            let rc = run_ffmpeg(core, ffmpeg_cmd(&ffmpeg, &args), duration, &label);
            if rc != 0 || cancelled() {
                return Err(t("failed_conv_audio"));
            }
            embed_audio_tags(core, &ffmpeg, &out, p, &tmp);
            core.finish_done(&out, "audio", duration);
            return Ok(());
        }

        // ---------------- modo video ----------------
        core.progress(Some(0.0), Some(t("dl_video")));
        let rc = run_ytdlp(core, ytcmd(&video_sel, &tmp.join("video_src.%(ext)s"), p.playlist), &t("dl_video"));
        if rc != 0 || cancelled() {
            return Err(t("failed_no_video"));
        }
        core.progress(Some(0.0), Some(t("dl_audio")));
        let rc = run_ytdlp(core, ytcmd(&audio_sel, &tmp.join("audio_src.%(ext)s"), false), &t("dl_audio"));
        if rc != 0 || cancelled() {
            return Err(t("failed_dl_audio"));
        }
        let (Some(video_file), Some(audio_file)) = (find("video_src."), find("audio_src.")) else {
            return Err(t("failed_no_video"));
        };
        let title = title_for();
        let vs = video_file.to_string_lossy().to_string();
        let au = audio_file.to_string_lossy().to_string();

        if p.direct {
            let ext = video_file.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or("mp4".into());
            let out = util::unique_path(&out_dir.join(format!("{title}.{ext}")));
            let mut args = strs(["-y", "-i"]);
            args.extend([vs, "-i".into(), au]);
            args.extend(strs(["-map", "0:v:0", "-map", "1:a:0", "-c", "copy", "-movflags", "+faststart", "-map_metadata", "0"]));
            args.extend(metadata_args(p.embed_meta, p));
            args.push(out.to_string_lossy().to_string());
            core.progress(Some(0.0), Some("Muxing…".into()));
            let rc = run_ffmpeg(core, ffmpeg_cmd(&ffmpeg, &args), None, "Muxing");
            if rc != 0 || cancelled() {
                return Err(t("failed_reencode"));
            }
            handle_subtitles(core, &ytdlp, &ffmpeg, &p.url, p, &out, &tmp);
            core.finish_done(&out, "video", None);
            return Ok(());
        }

        let duration = media::probe_duration(&ffprobe, &video_file);
        let out_ext = if p.codec == "vp9" { "webm" } else { "mp4" };
        let out = util::unique_path(&out_dir.join(format!("{title}.{out_ext}")));
        let audio_out = if out_ext == "webm" { strs(["-c:a", "libopus", "-b:a", "192k"]) } else { strs(["-c:a", "aac", "-b:a", "192k"]) };
        let build = |enc: Vec<String>| {
            let mut args = strs(["-y"]);
            args.extend(trim_in(&p.trim_start, &p.trim_end));
            args.extend(["-i".to_string(), vs.clone()]);
            args.extend(trim_in(&p.trim_start, &p.trim_end));
            args.extend(["-i".to_string(), au.clone()]);
            args.extend(strs(["-map", "0:v:0", "-map", "1:a:0"]));
            args.extend(enc);
            args.extend(audio_out.clone());
            args.extend(strs(["-movflags", "+faststart", "-map_metadata", "0"]));
            args.extend(metadata_args(p.embed_meta, p));
            args.extend(strs(["-progress", "pipe:1", "-nostats"]));
            args.push(out.to_string_lossy().to_string());
            args
        };
        core.progress(Some(0.0), Some(t("reencode")));
        let rc = encode_video(core, &ffmpeg, &p.vendor, &p.codec, p.cq, duration, &t("reencode"), build);
        if rc != 0 || cancelled() {
            return Err(t("failed_reencode"));
        }
        handle_subtitles(core, &ytdlp, &ffmpeg, &p.url, p, &out, &tmp);
        core.finish_done(&out, "video", duration);
        Ok(())
    })();

    if let Err(msg) = result {
        core.finish(false, &msg);
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn supports_cover(ext: &str) -> bool {
    matches!(ext, "mp3" | "m4a" | "flac")
}

fn cover_urls(url: &str) -> Vec<String> {
    let u = url.trim();
    if u.is_empty() {
        return Vec::new();
    }
    let mut v = vec![u.to_string()];
    // maxresdefault/sddefault no existen para todos los videos: hqdefault sí.
    let fallback = u.replace("maxresdefault", "hqdefault").replace("sddefault", "hqdefault");
    if fallback != u {
        v.push(fallback);
    }
    v
}

fn fetch_cover(p: &Payload, tmp: &Path) -> Option<PathBuf> {
    for url in cover_urls(&p.thumb_url) {
        if let Some(bytes) = crate::deps::fetch_bytes(&url, 8_000_000) {
            let dest = tmp.join("cover.jpg");
            // se normaliza a JPEG por si la miniatura llega en WebP
            let ok = image::load_from_memory(&bytes)
                .ok()
                .map(|img| img.to_rgb8().save_with_format(&dest, image::ImageFormat::Jpeg).is_ok())
                .unwrap_or(false);
            if ok {
                return Some(dest);
            }
        }
    }
    None
}

fn replace_file(src: &Path, dst: &Path) -> bool {
    if std::fs::rename(src, dst).is_ok() {
        return true;
    }
    // En Windows el rename puede fallar si otro proceso tiene el archivo
    // abierto; se intenta sobrescribir sin borrar el original primero.
    let ok = std::fs::copy(src, dst).is_ok();
    let _ = std::fs::remove_file(src);
    ok
}

/// Añade metadatos y/o carátula a un audio, conservando los tags existentes.
/// Si la carátula falla, reintenta solo con metadatos para no perderlos.
fn tag_audio(ffmpeg: &Path, audio: &Path, ext: &str, meta: &[String], cover: Option<&Path>) -> Result<(), String> {
    let stem = audio.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let tagged = audio.with_file_name(format!("{stem}_tagged.{ext}"));
    let mut args = strs(["-y", "-i"]);
    args.push(audio.to_string_lossy().to_string());
    if let Some(c) = cover {
        args.extend(["-i".to_string(), c.to_string_lossy().to_string()]);
        args.extend(strs(["-map", "0:a", "-map", "1:v", "-c", "copy", "-disposition:v", "attached_pic"]));
        if ext == "mp3" {
            args.extend(strs(["-id3v2_version", "3"]));
        }
    } else {
        args.extend(strs(["-c", "copy"]));
    }
    args.extend(strs(["-map_metadata", "0"]));
    args.extend_from_slice(meta);
    args.push(tagged.to_string_lossy().to_string());
    let out = util::cmd(ffmpeg).args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() && tagged.exists() {
        if replace_file(&tagged, audio) {
            return Ok(());
        }
        return Err("no se pudo reemplazar el archivo final".into());
    }
    let err = String::from_utf8_lossy(&out.stderr)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string();
    let _ = std::fs::remove_file(&tagged);
    Err(if err.is_empty() { "ffmpeg falló".into() } else { err })
}

/// Incrusta caratula y/o metadatos en un audio ya generado (best-effort).
fn embed_audio_tags(core: &Core, ffmpeg: &Path, audio: &Path, p: &Payload, tmp: &Path) {
    if !(p.embed_meta || p.embed_thumb) {
        return;
    }
    let lang = core.lang();
    let ext = audio.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let meta = metadata_args(p.embed_meta, p);
    let mut cover: Option<PathBuf> = None;
    if p.embed_thumb {
        if !supports_cover(&ext) {
            if ext == "wav" {
                core.log(&core.t("cover_wav"));
            }
        } else if !p.thumb_url.trim().is_empty() {
            cover = fetch_cover(p, tmp);
            if cover.is_none() {
                core.log(&trf(&lang, "cover_failed", &[("msg", &tr(&lang, "cover_thumb"))]));
            }
        }
    }
    if meta.is_empty() && cover.is_none() {
        return;
    }
    if let Some(c) = cover.as_deref() {
        match tag_audio(ffmpeg, audio, &ext, &meta, Some(c)) {
            Ok(()) => return,
            Err(e) => {
                core.log(&trf(&lang, "cover_failed", &[("msg", &e.chars().take(200).collect::<String>())]));
                if meta.is_empty() {
                    return;
                }
            }
        }
    }
    if !meta.is_empty() {
        if let Err(e) = tag_audio(ffmpeg, audio, &ext, &meta, None) {
            core.log(&trf(&lang, "tags_failed", &[("msg", &e.chars().take(200).collect::<String>())]));
        }
    }
}

/// Índice global del stream de carátula incrustada, si existe.
fn attached_pic_index(ffprobe: &Path, media: &Path) -> Option<u32> {
    let out = util::cmd(ffprobe)
        .args(["-v", "error", "-select_streams", "v", "-show_entries", "stream=index:stream_disposition=attached_pic", "-of", "json"])
        .arg(media)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let data: Value = serde_json::from_slice(&out.stdout).ok()?;
    data.get("streams")?.as_array()?.iter().find_map(|s| {
        let attached = s.get("disposition").and_then(|d| d.get("attached_pic")).and_then(Value::as_i64).unwrap_or(0) == 1;
        if !attached {
            return None;
        }
        s.get("index").and_then(Value::as_u64).map(|v| v as u32)
    })
}

fn extract_cover(ffmpeg: &Path, src: &Path, stream: u32, dest: &Path) -> bool {
    util::cmd(ffmpeg)
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-map", &format!("0:{stream}"), "-frames:v", "1", "-c:v", "mjpeg", "-q:v", "2"])
        .arg(dest)
        .output()
        .map(|o| o.status.success() && dest.exists())
        .unwrap_or(false)
}

/// Conserva la carátula de un archivo local al reencodearlo a un formato que la soporta.
fn attach_local_cover(core: &Core, ffmpeg: &Path, ffprobe: &Path, src: &Path, audio: &Path) {
    let ext = audio.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !supports_cover(&ext) {
        return;
    }
    let Some(stream) = attached_pic_index(ffprobe, src) else { return };
    let cover = std::env::temp_dir().join(format!(
        "liquidcat_cover_{}_{}.jpg",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    ));
    if !extract_cover(ffmpeg, src, stream, &cover) {
        return;
    }
    if let Err(e) = tag_audio(ffmpeg, audio, &ext, &[], Some(&cover)) {
        let lang = core.lang();
        core.log(&trf(&lang, "cover_failed", &[("msg", &e.chars().take(200).collect::<String>())]));
    }
    let _ = std::fs::remove_file(&cover);
}

/// Descarga subtitulos y los incrusta (mp4) o los deja junto al archivo (best-effort).
fn handle_subtitles(core: &Core, ytdlp: &Path, ffmpeg: &Path, url: &str, p: &Payload, out: &Path, tmp: &Path) {
    if !p.subs {
        return;
    }
    let sub_base = tmp.join("subs");
    let _ = util::cmd(ytdlp)
        .args(["--skip-download", "--write-subs", "--write-auto-subs", "--sub-langs", "en,es", "--convert-subs", "srt"])
        .args(core.cookies_args(&p.cookies_browser))
        .arg("-o")
        .arg(&sub_base)
        .args(["--no-playlist", url])
        .output();
    let mut srts: Vec<PathBuf> = std::fs::read_dir(tmp)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|f| f.extension().map(|e| e == "srt").unwrap_or(false))
        .collect();
    srts.sort();
    let Some(first) = srts.first() else { return };
    if p.subs_embed && out.extension().map(|e| e == "mp4").unwrap_or(false) {
        let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let tmp_out = out.with_file_name(format!("{stem}_subbed.mp4"));
        let ok = util::cmd(ffmpeg)
            .args(["-y", "-i"])
            .arg(out)
            .arg("-i")
            .arg(first)
            .args(["-map", "0", "-map", "1", "-c", "copy", "-c:s", "mov_text", "-map_metadata", "0"])
            .arg(&tmp_out)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok && tmp_out.exists() {
            let _ = std::fs::rename(&tmp_out, out);
        } else {
            let _ = std::fs::remove_file(&tmp_out);
        }
    } else {
        for s in &srts {
            let name = s.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // "subs.en.srt" -> "<video>.en.srt"
            let suffix = name.strip_prefix("subs").unwrap_or(&name);
            let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let _ = std::fs::copy(s, out.with_file_name(format!("{stem}{suffix}")));
        }
    }
}

// ====================================================================
//  Archivo local
// ====================================================================
pub fn local_worker(core: &Arc<Core>, p: &LocalPayload) {
    *core.last_output.lock().unwrap() = String::new();
    *core.meta.lock().unwrap() = Default::default();
    let lang = core.lang();
    let t = |k: &str| tr(&lang, k);
    let bins = core.bins();
    let Some(ffmpeg) = bins.ffmpeg.clone() else {
        core.finish(false, &t("no_ytdlp"));
        return;
    };
    let ffprobe = bins.ffprobe.clone().unwrap_or_else(|| ffmpeg.with_file_name(format!("ffprobe{}", util::EXE)));
    let src = PathBuf::from(&p.input);
    if p.input.is_empty() || !src.is_file() {
        core.finish(false, &trf(&lang, "error", &[("msg", &t("file_not_found"))]));
        return;
    }
    let out_dir = if p.out_dir.trim().is_empty() { util::downloads_dir() } else { PathBuf::from(p.out_dir.trim()) };
    let _ = std::fs::create_dir_all(&out_dir);
    let duration = media::probe_duration(&ffprobe, &src);
    let stem = util::safe_name(&src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), "archivo");
    let input = src.to_string_lossy().to_string();
    let cancelled = || core.cancel.load(Ordering::SeqCst);

    match p.mode.as_str() {
        "gif" => {
            let fps = if p.gif_fps.trim().is_empty() { "12" } else { p.gif_fps.trim() };
            let width = if p.gif_width.trim().is_empty() { "480" } else { p.gif_width.trim() };
            let out = util::unique_path(&out_dir.join(format!("{stem}.gif")));
            let vf = format!("fps={fps},scale={width}:-1:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse");
            let mut args = strs(["-y"]);
            args.extend(trim_in(&p.trim_start, &p.trim_end));
            args.extend(["-i".to_string(), input]);
            args.extend(["-vf".to_string(), vf]);
            args.extend(strs(["-loop", "0", "-progress", "pipe:1", "-nostats"]));
            args.push(out.to_string_lossy().to_string());
            core.progress(Some(0.0), Some("GIF…".into()));
            let rc = run_ffmpeg(core, ffmpeg_cmd(&ffmpeg, &args), duration, "GIF");
            if rc != 0 || cancelled() {
                core.finish(false, &t("failed_reencode"));
                return;
            }
            core.finish_done(&out, "gif", duration);
        }
        "audio" => {
            let (ac, ext) = media::audio_output(&p.audio_codec);
            let ac = media::audio_args(ac, &p.audio_codec, "", p.audio_norm);
            let out = util::unique_path(&out_dir.join(format!("{stem}.{ext}")));
            let mut args = strs(["-y"]);
            args.extend(trim_in(&p.trim_start, &p.trim_end));
            args.extend(["-i".to_string(), input]);
            args.push("-vn".into());
            args.extend(ac);
            args.extend(strs(["-sn", "-dn", "-map_metadata", "0", "-progress", "pipe:1", "-nostats"]));
            args.push(out.to_string_lossy().to_string());
            let label = trf(&lang, "conv_audio", &[("fmt", &p.audio_codec)]);
            core.progress(Some(0.0), Some(label.clone()));
            let rc = run_ffmpeg(core, ffmpeg_cmd(&ffmpeg, &args), duration, &label);
            if rc != 0 || cancelled() {
                core.finish(false, &t("failed_conv_audio"));
                return;
            }
            attach_local_cover(core, &ffmpeg, &ffprobe, &src, &out);
            core.finish_done(&out, "audio", duration);
        }
        _ => {
            let out_ext = if p.codec == "vp9" { "webm" } else { "mp4" };
            let out = util::unique_path(&out_dir.join(format!("{stem}.{out_ext}")));
            let audio_out = if out_ext == "webm" { strs(["-c:a", "libopus", "-b:a", "192k"]) } else { strs(["-c:a", "aac", "-b:a", "192k"]) };
            let build = |enc: Vec<String>| {
                let mut args = strs(["-y"]);
                args.extend(trim_in(&p.trim_start, &p.trim_end));
                args.extend(["-i".to_string(), input.clone()]);
                args.extend(enc);
                args.extend(audio_out.clone());
                args.extend(strs(["-movflags", "+faststart", "-map_metadata", "0", "-progress", "pipe:1", "-nostats"]));
                args.push(out.to_string_lossy().to_string());
                args
            };
            core.progress(Some(0.0), Some(t("reencode")));
            let rc = encode_video(core, &ffmpeg, &p.vendor, &p.codec, p.cq, duration, &t("reencode"), build);
            if rc != 0 || cancelled() {
                core.finish(false, &t("failed_reencode"));
                return;
            }
            core.finish_done(&out, "video", duration);
        }
    }
}
