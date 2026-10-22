//! ffmpeg: encoders, argumentos y ejecucion con progreso/cancelacion.
use crate::util;
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;

pub fn encoder_for(vendor: &str, codec: &str) -> &'static str {
    match (vendor, codec) {
        ("nvidia", "h264") => "h264_nvenc",
        ("nvidia", "hevc") => "hevc_nvenc",
        ("nvidia", "av1") => "av1_nvenc",
        ("amd", "h264") => if cfg!(windows) { "h264_amf" } else { "h264_vaapi" },
        ("amd", "hevc") => if cfg!(windows) { "hevc_amf" } else { "hevc_vaapi" },
        ("amd", "av1") => if cfg!(windows) { "av1_amf" } else { "av1_vaapi" },
        (_, "vp9") => "libvpx-vp9",
        (_, "hevc") => "libx265",
        (_, "av1") => "libsvtav1",
        _ => "libx264",
    }
}

static ENCODERS: Mutex<Option<(String, HashSet<String>)>> = Mutex::new(None);

/// Consulta (con cache) si el ffmpeg en uso trae un encoder.
pub fn encoder_available(ffmpeg: &Path, name: &str) -> bool {
    let key = ffmpeg.to_string_lossy().to_string();
    let mut g = ENCODERS.lock().unwrap();
    if let Some((k, set)) = g.as_ref() {
        if *k == key {
            return set.contains(name);
        }
    }
    let out = util::cmd(ffmpeg).args(["-hide_banner", "-encoders"]).output();
    let Ok(out) = out else { return false };
    let text = String::from_utf8_lossy(&out.stdout);
    let set: HashSet<String> = text
        .lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let flags = p.next()?;
            let name = p.next()?;
            (flags.len() == 6 && "VAS".contains(&flags[..1])).then(|| name.to_string())
        })
        .collect();
    let r = set.contains(name);
    *g = Some((key, set));
    r
}

pub fn video_encoder_args(ffmpeg: &Path, vendor: &str, codec: &str, cq: i32) -> Vec<String> {
    let mut enc = encoder_for(vendor, codec);
    // libsvtav1 no viene en el build "essentials": se cae a libaom-av1.
    if enc == "libsvtav1" && !encoder_available(ffmpeg, "libsvtav1") {
        enc = "libaom-av1";
    }
    let cq_s = cq.to_string();
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match enc {
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => v(&["-c:v", enc, "-preset", "p5", "-rc", "vbr", "-cq", &cq_s, "-b:v", "0"]),
        "h264_amf" | "hevc_amf" | "av1_amf" => v(&["-c:v", enc, "-quality", "balanced", "-rc", "cqp", "-qp_i", &cq_s, "-qp_p", &(cq + 2).to_string()]),
        "h264_vaapi" | "hevc_vaapi" | "av1_vaapi" => v(&["-vaapi_device", "/dev/dri/renderD128", "-vf", "format=nv12,hwupload", "-c:v", enc, "-qp", &cq_s]),
        "libx265" => v(&["-c:v", "libx265", "-preset", "medium", "-crf", &cq_s]),
        "libsvtav1" => v(&["-c:v", "libsvtav1", "-preset", "8", "-crf", &cq_s]),
        "libaom-av1" => v(&["-c:v", "libaom-av1", "-cpu-used", "8", "-b:v", "0", "-crf", &cq_s]),
        "libvpx-vp9" => v(&["-c:v", "libvpx-vp9", "-crf", &cq_s, "-b:v", "0", "-row-mt", "1"]),
        _ => v(&["-c:v", "libx264", "-preset", "medium", "-crf", &cq_s]),
    }
}

/// (argumentos de ffmpeg, extension) para el modo audio.
pub fn audio_output(codec: &str) -> (Vec<String>, &'static str) {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match codec {
        "aac" => (v(&["-c:a", "aac", "-b:a", "192k"]), "m4a"),
        "opus" => (v(&["-c:a", "libopus", "-b:a", "160k"]), "opus"),
        "ogg" => (v(&["-c:a", "libvorbis", "-q:a", "5"]), "ogg"),
        "flac" => (v(&["-c:a", "flac"]), "flac"),
        "wav" => (v(&["-c:a", "pcm_s16le"]), "wav"),
        _ => (v(&["-c:a", "libmp3lame", "-b:a", "192k"]), "mp3"),
    }
}

/// Aplica el bitrate elegido y la normalizacion de volumen.
pub fn audio_args(mut args: Vec<String>, codec: &str, bitrate: &str, norm: bool) -> Vec<String> {
    if !bitrate.is_empty() && bitrate.chars().all(|c| c.is_ascii_digit()) && matches!(codec, "mp3" | "aac" | "opus") {
        if let Some(i) = args.iter().position(|a| a == "-b:a") {
            args[i + 1] = format!("{bitrate}k");
        } else {
            args.extend(["-b:a".to_string(), format!("{bitrate}k")]);
        }
    }
    if norm {
        let mut v = vec!["-af".to_string(), "loudnorm".to_string()];
        v.extend(args);
        return v;
    }
    args
}

pub fn probe_duration(ffprobe: &Path, media: &Path) -> Option<f64> {
    let run = |entry: &str| {
        util::cmd(ffprobe)
            .args(["-v", "error", "-show_entries", entry, "-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(media)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    };
    if let Some(out) = run("format=duration") {
        if let Ok(v) = out.trim().parse::<f64>() {
            return Some(v);
        }
    }
    run("stream=duration")?.split_whitespace().find_map(|t| t.parse::<f64>().ok())
}

fn pump<R: Read + Send + 'static>(mut r: R, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        // yt-dlp/ffmpeg separan con \n o \r (progreso en la misma linea)
                        if b == b'\n' || b == b'\r' {
                            if !line.is_empty() {
                                let _ = tx.send(String::from_utf8_lossy(&line).to_string());
                                line.clear();
                            }
                        } else {
                            line.push(b);
                        }
                    }
                }
            }
        }
        if !line.is_empty() {
            let _ = tx.send(String::from_utf8_lossy(&line).to_string());
        }
    });
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        // yt-dlp.exe (PyInstaller) lanza un proceso hijo: hay que matar todo el arbol.
        let _ = util::cmd("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()]).output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Ejecuta un proceso entregando cada linea (stdout y stderr) a `on_line`.
/// Devuelve el codigo de salida, o -1 si se cancelo, o -2 si no pudo iniciar.
pub fn run_streaming(mut c: Command, cancel: &AtomicBool, mut on_line: impl FnMut(&str)) -> i32 {
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => {
            on_line(&format!("ERROR: {e}"));
            return -2;
        }
    };
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(o) = child.stdout.take() {
        pump(o, tx.clone());
    }
    if let Some(e) = child.stderr.take() {
        pump(e, tx.clone());
    }
    drop(tx);
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => on_line(&line),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if cancel.load(Ordering::SeqCst) {
            kill_tree(&mut child);
            return -1;
        }
    }
    child.wait().ok().and_then(|s| s.code()).unwrap_or(-3)
}
