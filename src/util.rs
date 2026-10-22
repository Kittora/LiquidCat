//! Utilidades pequenas: procesos ocultos, rutas, formatos de texto.
use std::path::{Path, PathBuf};
use std::process::Command;

pub const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

/// Crea un `Command` que no abre ventana de consola en Windows.
pub fn cmd<S: AsRef<std::ffi::OsStr>>(prog: S) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(prog);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

pub fn home() -> PathBuf {
    env_dir("USERPROFILE").or_else(|| env_dir("HOME")).unwrap_or_else(|| PathBuf::from("."))
}

pub fn downloads_dir() -> PathBuf {
    home().join("Downloads")
}

/// %APPDATA%\LiquidCat (config.json). Mismo lugar que la version en Python.
pub fn config_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env_dir("APPDATA").unwrap_or_else(home)
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
    };
    base.join("LiquidCat")
}

/// %LOCALAPPDATA%\LiquidCat\bin (yt-dlp y ffmpeg descargados por la app).
pub fn bin_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env_dir("LOCALAPPDATA").unwrap_or_else(home)
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))
    };
    base.join("LiquidCat").join("bin")
}

pub fn human_size(n: u64) -> String {
    let mut x = n as f64;
    for u in ["B", "KB", "MB", "GB", "TB"] {
        if x < 1024.0 {
            return format!("{:.1} {}", x, u);
        }
        x /= 1024.0;
    }
    format!("{:.1} PB", x)
}

/// Tamano sin espacio ("12.3MB") para la lista de formatos.
pub fn human_size_tight(n: f64) -> String {
    if n <= 0.0 {
        return "?".into();
    }
    let mut x = n;
    for u in ["B", "KB", "MB", "GB"] {
        if x < 1024.0 {
            return format!("{:.1}{}", x, u);
        }
        x /= 1024.0;
    }
    format!("{:.1}TB", x)
}

/// Quita caracteres no validos en nombres de archivo de Windows.
pub fn safe_name(s: &str, fallback: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if "\\/*?:\"<>|".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_string();
    if cleaned.is_empty() {
        return fallback.to_string();
    }
    cleaned.chars().take(120).collect()
}

pub fn unique_path(p: &Path) -> PathBuf {
    if !p.exists() {
        return p.to_path_buf();
    }
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    let parent = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut i = 1;
    loop {
        let c = parent.join(format!("{stem} ({i}){ext}"));
        if !c.exists() {
            return c;
        }
        i += 1;
    }
}

pub fn fmt_duration(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

pub fn fmt_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// "2026-08-06 14:30" a partir de segundos Unix, en hora local (sin dependencias de fecha).
pub fn fmt_datetime(ts: f64) -> String {
    let secs = ts as i64 + local_offset_secs();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // algoritmo de Howard Hinnant (civil from days)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

pub fn free_space(folder: &str) -> Option<u64> {
    let mut d = if folder.trim().is_empty() { downloads_dir() } else { PathBuf::from(folder.trim()) };
    while !d.exists() {
        if !d.pop() {
            return None;
        }
    }
    fs2::available_space(&d).ok()
}

/// Desfase de la zona horaria local respecto a UTC, en segundos.
pub fn local_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        unsafe {
            let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
            let id = GetTimeZoneInformation(&mut tz);
            let mut bias = tz.Bias as i64;
            if id == 2 {
                bias += tz.DaylightBias as i64;
            } else if id == 1 {
                bias += tz.StandardBias as i64;
            }
            -bias * 60
        }
    }
    #[cfg(not(windows))]
    {
        static OFF: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
        *OFF.get_or_init(|| {
            cmd("date")
                .arg("+%z")
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.len() != 5 {
                        return None;
                    }
                    let sign = if s.starts_with('-') { -1 } else { 1 };
                    let h: i64 = s[1..3].parse().ok()?;
                    let m: i64 = s[3..5].parse().ok()?;
                    Some(sign * (h * 3600 + m * 60))
                })
                .unwrap_or(0)
        })
    }
}
