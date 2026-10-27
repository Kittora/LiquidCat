//! Gestion de dependencias (yt-dlp y ffmpeg): deteccion, descarga oficial por HTTPS,
//! verificacion de integridad y versiones. Port de DependencyManager.py.
use crate::util::{self, EXE};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(windows)]
const YTDLP_SOURCES: &[&str] = &[
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe",
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
];
#[cfg(target_os = "macos")]
const YTDLP_SOURCES: &[&str] = &[
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_macos",
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
];
#[cfg(not(any(windows, target_os = "macos")))]
const YTDLP_SOURCES: &[&str] = &[
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_linux",
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
];
/// Build "essentials" de Gyan. Su servidor (gyan.dev) va muy lento (~0,1 MB/s
/// medido, unos 20 min para 115 MB), asi que primero se usa la copia oficial
/// que el mismo autor publica en GitHub (~7 MB/s) y gyan.dev queda de respaldo.
const FFMPEG_GITHUB_API: &str = "https://api.github.com/repos/GyanD/codexffmpeg/releases/latest";
/// Ultima build nightly de yt-dlp (la que descarga la app primero).
const YTDLP_GITHUB_API: &str = "https://api.github.com/repos/yt-dlp/yt-dlp-nightly-builds/releases/latest";

/// Error de `download_ffmpeg` fuera de Windows: alli ffmpeg se instala con el
/// gestor de paquetes. `error_text` lo convierte en el aviso traducido.
pub const FFMPEG_MANUAL: &str = "ffmpeg_manual";
#[cfg(target_os = "macos")]
const FFMPEG_INSTALL_CMD: &str = "brew install ffmpeg";
#[cfg(not(target_os = "macos"))]
const FFMPEG_INSTALL_CMD: &str = "sudo apt install ffmpeg";

/// Texto para mostrar de un error de dependencias, en el idioma de la app.
pub fn error_text(lang: &str, err: &str) -> String {
    if err == FFMPEG_MANUAL {
        crate::lang::trf(lang, FFMPEG_MANUAL, &[("cmd", FFMPEG_INSTALL_CMD)])
    } else {
        err.to_string()
    }
}

/// JSON de la ultima release de un repositorio (API publica de GitHub).
fn latest_release(api: &str) -> Option<serde_json::Value> {
    serde_json::from_slice(&fetch_bytes(api, 1024 * 1024)?).ok()
}
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Cliente HTTP que usa el almacen de certificados del sistema (funciona tras proxies/antivirus).
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .build()
        .into()
}

/// Descarga una URL pequena a memoria (miniaturas).
pub fn fetch_bytes(url: &str, max: u64) -> Option<Vec<u8>> {
    let mut resp = agent().get(url).header("User-Agent", UA).call().ok()?;
    let mut buf = Vec::new();
    resp.body_mut().as_reader().take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

pub struct DepManager {
    pub dir: PathBuf,
    ver_cache: Mutex<HashMap<String, (Instant, String)>>,
}

impl DepManager {
    pub fn new() -> Self {
        DepManager { dir: util::bin_dir(), ver_cache: Mutex::new(HashMap::new()) }
    }

    fn invalidate(&self) {
        self.ver_cache.lock().unwrap().clear();
    }

    /// Carpetas candidatas, en orden de prioridad: gestionada, ubicaciones tipicas, PATH.
    pub fn candidate_paths(&self, name: &str) -> Vec<PathBuf> {
        let file = format!("{name}{EXE}");
        let mut folders: Vec<PathBuf> = vec![self.dir.clone()];
        if let Some(parent) = self.dir.parent().and_then(Path::parent) {
            folders.push(parent.join("YTReencoder").join("bin")); // carpeta de versiones antiguas
        }
        #[cfg(windows)]
        {
            if let Some(la) = std::env::var_os("LOCALAPPDATA") {
                let la = PathBuf::from(la);
                folders.push(la.join("Microsoft").join("WindowsApps"));
                folders.push(la.join("Programs").join("yt-dlp"));
            }
            for var in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(pf) = std::env::var_os(var) {
                    folders.push(PathBuf::from(pf).join("ffmpeg").join("bin"));
                }
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            folders.extend(std::env::split_paths(&path).filter(|p| !p.as_os_str().is_empty()));
        }
        let mut seen = std::collections::HashSet::new();
        folders.into_iter().filter(|f| seen.insert(f.clone())).map(|f| f.join(&file)).collect()
    }

    pub fn find_binary(&self, name: &str) -> Option<PathBuf> {
        self.candidate_paths(name).into_iter().find(|p| p.is_file())
    }

    pub fn managed_path(&self, name: &str) -> Option<PathBuf> {
        let p = self.dir.join(format!("{name}{EXE}"));
        p.is_file().then_some(p)
    }

    // ------------------------------------------------------------ descarga
    fn download(&self, url: &str, dest: &Path, mut progress: impl FnMut(f64)) -> Result<(), String> {
        std::fs::create_dir_all(dest.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
        let tmp = PathBuf::from(format!("{}.part", dest.to_string_lossy()));
        let result = (|| -> Result<(), String> {
            let mut resp = agent().get(url).header("User-Agent", UA).header("Accept", "*/*").call().map_err(|e| e.to_string())?;
            let total: u64 = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let mut reader = resp.body_mut().as_reader();
            let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 64 * 1024];
            let mut done = 0u64;
            let mut last = Instant::now();
            loop {
                let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                if total > 0 && last.elapsed() > Duration::from_millis(120) {
                    progress(done as f64 / total as f64);
                    last = Instant::now();
                }
            }
            out.flush().map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => std::fs::rename(&tmp, dest).map_err(|e| e.to_string()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Un ejecutable valido: PE ("MZ") en Windows, ELF/script en el resto.
    fn is_valid_exe(p: &Path) -> bool {
        let mut head = [0u8; 4];
        match std::fs::File::open(p).and_then(|mut f| f.read_exact(&mut head)) {
            Ok(()) => {
                if cfg!(windows) {
                    &head[..2] == b"MZ"
                } else {
                    // ELF (Linux), script, o Mach-O (macOS: 64 bits o universal).
                    &head == b"\x7fELF"
                        || &head[..2] == b"#!"
                        || head == [0xcf, 0xfa, 0xed, 0xfe]
                        || head == [0xca, 0xfe, 0xba, 0xbe]
                }
            }
            Err(_) => false,
        }
    }

    pub fn download_ytdlp(&self, mut progress: impl FnMut(f64)) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let target = self.dir.join(format!("yt-dlp{EXE}"));
        progress(0.0);
        let mut last_err = String::new();
        let n = YTDLP_SOURCES.len();
        for (i, url) in YTDLP_SOURCES.iter().enumerate() {
            for _ in 0..2 {
                match self.download(url, &target, &mut progress) {
                    Ok(()) if Self::is_valid_exe(&target) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
                        }
                        self.invalidate();
                        return Ok(target);
                    }
                    Ok(()) => {
                        last_err = "El archivo descargado no es un ejecutable valido (incompleto o corrupto).".into();
                        let _ = std::fs::remove_file(&target);
                    }
                    Err(e) => {
                        last_err = e;
                        let _ = std::fs::remove_file(&target);
                    }
                }
            }
            progress(((i + 1) as f64 / n as f64 * 0.9).min(0.9));
        }
        Err(format!("No se pudo descargar yt-dlp: {last_err}"))
    }

    /// URL del zip "essentials" de la ultima release de GyanD en GitHub.
    fn ffmpeg_github_url() -> Option<String> {
        let v = latest_release(FFMPEG_GITHUB_API)?;
        v.get("assets")?.as_array()?.iter().find_map(|a| {
            let name = a.get("name")?.as_str()?;
            if name.ends_with("essentials_build.zip") {
                a.get("browser_download_url")?.as_str().map(str::to_string)
            } else {
                None
            }
        })
    }

    pub fn download_ffmpeg(&self, mut progress: impl FnMut(f64)) -> Result<(), String> {
        if !cfg!(windows) {
            return Err(FFMPEG_MANUAL.into());
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let zip_path = self.dir.join("ffmpeg.zip");
        progress(0.02);
        let mut sources: Vec<String> = Self::ffmpeg_github_url().into_iter().collect();
        sources.push(FFMPEG_URL.to_string());
        let mut last_err = None;
        'sources: for url in &sources {
            for _ in 0..2 {
                match self.download(url, &zip_path, |f| progress(0.02 + f * 0.7)) {
                    Ok(()) => {
                        last_err = None;
                        break 'sources;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        let _ = std::fs::remove_file(&zip_path);
                    }
                }
            }
        }
        if let Some(e) = last_err {
            return Err(format!("No se pudo descargar FFmpeg: {e}"));
        }
        let mut extract = || -> Result<(), String> {
            let f = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
            let mut z = zip::ZipArchive::new(f).map_err(|_| "La descarga de FFmpeg llego corrupta o incompleta.".to_string())?;
            for i in 0..z.len() {
                let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
                let name = entry.name().replace('\\', "/");
                let tail = name.rsplit('/').next().unwrap_or("").to_string();
                if (tail == "ffmpeg.exe" || tail == "ffprobe.exe") && name.ends_with(&format!("/bin/{tail}")) {
                    let target = self.dir.join(&tail);
                    let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
                    // copiar hasta el final verifica el CRC del ZIP
                    std::io::copy(&mut entry, &mut out)
                        .map_err(|_| "La descarga de FFmpeg llego corrupta o incompleta. No se ejecutara ningun archivo no verificado.".to_string())?;
                    drop(out);
                    if !Self::is_valid_exe(&target) {
                        let _ = std::fs::remove_file(&target);
                        return Err("El binario de FFmpeg extraido no es un ejecutable valido.".into());
                    }
                    progress(if tail == "ffprobe.exe" { 0.92 } else { 0.82 });
                }
            }
            Ok(())
        };
        let r = extract();
        let _ = std::fs::remove_file(&zip_path);
        r?;
        progress(0.95);
        self.invalidate();
        Ok(())
    }

    // ------------------------------------------------------------ versiones
    pub fn binary_version(&self, path: &Path) -> String {
        let key = path.to_string_lossy().to_string();
        if let Some((t, v)) = self.ver_cache.lock().unwrap().get(&key) {
            if t.elapsed() < Duration::from_secs(600) {
                return v.clone();
            }
        }
        let line = util::cmd(path)
            .arg("--version")
            .output()
            .ok()
            .map(|o| {
                let out = if o.stdout.is_empty() { o.stderr } else { o.stdout };
                String::from_utf8_lossy(&out).lines().next().unwrap_or("").to_string()
            })
            .unwrap_or_default();
        let version = parse_version(&line);
        self.ver_cache.lock().unwrap().insert(key, (Instant::now(), version.clone()));
        version
    }

    pub fn ytdlp_channel(&self, path: &Path) -> &'static str {
        let v = self.binary_version(path);
        match find_date_version(&v) {
            Some(d) => {
                if d.split('.').count() >= 4 { "nightly" } else { "stable" }
            }
            None => "",
        }
    }

    /// Si la copia de la app ya es la ultima publicada, devuelve su version
    /// (para no volver a descargarla). `None` si hay que descargar: no hay
    /// copia propia, es mas vieja, o no se pudo consultar GitHub.
    pub fn up_to_date(&self, key: &str) -> Option<String> {
        if key == "ffmpeg" {
            let ff = self.managed_path("ffmpeg")?;
            self.managed_path("ffprobe")?;
            let installed = self.binary_version(&ff); // "9.0.2-essentials_build-www.gyan.dev"
            let latest = latest_release(FFMPEG_GITHUB_API)?.get("tag_name")?.as_str()?.to_string(); // "9.0.2"
            let base = installed.split('-').next().unwrap_or("");
            (!latest.is_empty() && base == latest).then_some(latest)
        } else {
            let yt = self.managed_path("yt-dlp")?;
            let installed = self.binary_version(&yt); // "2026.10.26.220202"
            let latest = latest_release(YTDLP_GITHUB_API)?.get("tag_name")?.as_str()?.to_string();
            (!latest.is_empty() && installed == latest).then_some(latest)
        }
    }

    // ------------------------------------------------------------ acciones
    /// Garantiza yt-dlp y ffmpeg (descarga lo que falte; cada uno es independiente).
    pub fn ensure(&self, mut progress: impl FnMut(f64)) -> Vec<String> {
        let mut errors = Vec::new();
        let _ = std::fs::create_dir_all(&self.dir);
        if self.managed_path("yt-dlp").or_else(|| self.find_binary("yt-dlp")).is_none() {
            if let Err(e) = self.download_ytdlp(|f| progress(f * 0.25)) {
                errors.push(e);
            }
        }
        let have_ff = (self.managed_path("ffmpeg").is_some() && self.managed_path("ffprobe").is_some())
            || (self.find_binary("ffmpeg").is_some() && self.find_binary("ffprobe").is_some());
        if !have_ff {
            if let Err(e) = self.download_ffmpeg(|f| progress(0.25 + f * 0.75)) {
                errors.push(e);
            }
        }
        self.invalidate();
        errors
    }
}

fn find_date_version(s: &str) -> Option<String> {
    // \d{4}\.\d{2}\.\d{2}(\.\d+)?
    let b = s.as_bytes();
    let mut i = 0;
    while i + 10 <= b.len() {
        let w = &b[i..i + 10];
        let ok = w[..4].iter().all(u8::is_ascii_digit)
            && w[4] == b'.'
            && w[5..7].iter().all(u8::is_ascii_digit)
            && w[7] == b'.'
            && w[8..10].iter().all(u8::is_ascii_digit);
        if ok {
            let mut end = i + 10;
            if end < b.len() && b[end] == b'.' && end + 1 < b.len() && b[end + 1].is_ascii_digit() {
                end += 1;
                while end < b.len() && b[end].is_ascii_digit() {
                    end += 1;
                }
            }
            return Some(s[i..end].to_string());
        }
        i += 1;
    }
    None
}

fn parse_version(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    if let Some(v) = find_date_version(line) {
        return v;
    }
    if let Some(pos) = line.find("version ") {
        if let Some(tok) = line[pos + 8..].split_whitespace().next() {
            return tok.to_string();
        }
    }
    line.split_whitespace().next().unwrap_or("").to_string()
}
