//! Gestion de dependencias (yt-dlp y ffmpeg): deteccion, descarga oficial por HTTPS,
//! verificacion de integridad y versiones. Port de DependencyManager.py.
use crate::util::{self, EXE};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// En Windows se usa la version "carpeta" (yt-dlp.exe + _internal): el exe de
// un solo archivo descomprime su Python en cada ejecucion (~3-4 s cada vez que
// se obtienen formatos o empieza una descarga); la carpeta se descomprime una
// sola vez, al instalar o actualizar. Tampoco necesita Python instalado.
#[cfg(windows)]
const YTDLP_SOURCES: &[&str] = &[
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_win.zip",
    "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_win.zip",
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
const FFMPEG_REPO: &str = "GyanD/codexffmpeg";
/// Builds nightly de yt-dlp (las que descarga la app primero).
const YTDLP_REPO: &str = "yt-dlp/yt-dlp-nightly-builds";
pub const LIQUIDCAT_REPO: &str = "Kittora/LiquidCat";

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

/// Ultima version publicada de un repositorio (su tag), SIN usar la API de
/// GitHub: la API solo admite 60 consultas por hora por IP, compartidas por
/// todos los que esten detras de la misma (universidad, oficina, VPN).
/// github.com/<repo>/releases/latest responde con una redireccion a
/// .../releases/tag/<tag>; basta con leerla, sin seguirla ni bajar la pagina.
pub fn latest_tag(repo: &str) -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls())
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into();
    let resp = agent.get(&format!("https://github.com/{repo}/releases/latest")).header("User-Agent", UA).call().ok()?;
    let location = resp.headers().get("location")?.to_str().ok()?;
    let tag = location.rsplit_once("/releases/tag/")?.1.trim_end_matches('/');
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Texto de las notas de una release. Esto si usa la API, pero solo se pide
/// cuando hay una version nueva de LiquidCat (y se guarda: una vez por version).
fn fetch_release_body(repo: &str, tag: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let v: serde_json::Value = serde_json::from_slice(&fetch_bytes(&url, 1024 * 1024)?).ok()?;
    Some(v.get("body")?.as_str()?.to_string())
}

fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build()
}
const FFMPEG_URL: &str = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// Cliente HTTP que usa el almacen de certificados del sistema (funciona tras proxies/antivirus).
pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(tls())
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
    // versions.json y latest.json se leen, modifican y escriben desde varios
    // hilos a la vez al arrancar: sin esto una escritura pisaba a la otra.
    files: Mutex<()>,
}

impl DepManager {
    pub fn new() -> Self {
        DepManager { dir: util::bin_dir(), ver_cache: Mutex::new(HashMap::new()), files: Mutex::new(()) }
    }

    fn invalidate(&self) {
        self.ver_cache.lock().unwrap().clear();
    }

    /// Carpetas candidatas, en orden de prioridad: gestionada, ubicaciones tipicas, PATH.
    pub fn candidate_paths(&self, name: &str) -> Vec<PathBuf> {
        let file = format!("{name}{EXE}");
        // yt-dlp en Windows: primero la copia "carpeta" de la app.
        let mut first = Vec::new();
        if cfg!(windows) && name == "yt-dlp" {
            first.push(self.ytdlp_dir().join(&file));
        }
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
        first.extend(folders.into_iter().filter(|f| seen.insert(f.clone())).map(|f| f.join(&file)));
        first
    }

    pub fn find_binary(&self, name: &str) -> Option<PathBuf> {
        self.candidate_paths(name).into_iter().find(|p| p.is_file())
    }

    /// Borra el yt-dlp.exe de un solo archivo de versiones anteriores cuando ya
    /// esta la version "carpeta" (si estaba en uso al migrar, no se pudo
    /// borrar entonces). Las carpetas temporales de una actualizacion a medias
    /// no se tocan aqui: las limpia la siguiente instalacion.
    pub fn remove_leftovers(&self) {
        if cfg!(windows) && self.managed_path("yt-dlp").is_some() {
            let _ = std::fs::remove_file(self.dir.join("yt-dlp.exe"));
        }
    }

    /// Carpeta de la copia de yt-dlp en Windows (yt-dlp.exe + _internal).
    fn ytdlp_dir(&self) -> PathBuf {
        self.dir.join("yt-dlp")
    }

    pub fn managed_path(&self, name: &str) -> Option<PathBuf> {
        // Un yt-dlp.exe suelto de versiones anteriores no cuenta como copia de
        // la app: asi se reemplaza solo por la version "carpeta".
        let p = if cfg!(windows) && name == "yt-dlp" {
            self.ytdlp_dir().join(format!("{name}{EXE}"))
        } else {
            self.dir.join(format!("{name}{EXE}"))
        };
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
        // Windows baja un zip (se descomprime despues); el resto, el ejecutable.
        let (file, share) = if cfg!(windows) { ("yt-dlp.zip".to_string(), 0.75) } else { (format!("yt-dlp{EXE}"), 1.0) };
        let downloaded = self.dir.join(file);
        progress(0.0);
        let mut last_err = String::new();
        let n = YTDLP_SOURCES.len();
        for (i, url) in YTDLP_SOURCES.iter().enumerate() {
            for _ in 0..2 {
                let r = self
                    .download(url, &downloaded, |f| progress(f * share))
                    .and_then(|()| self.install_ytdlp(&downloaded, &mut progress));
                match r {
                    Ok(exe) => {
                        self.invalidate();
                        return Ok(exe);
                    }
                    Err(e) => {
                        last_err = e;
                        let _ = std::fs::remove_file(&downloaded);
                    }
                }
            }
            progress(((i + 1) as f64 / n as f64 * 0.9).min(0.9));
        }
        Err(format!("No se pudo descargar yt-dlp: {last_err}"))
    }

    /// Windows: descomprime el zip en una carpeta aparte y solo al final la
    /// cambia por la anterior, asi un fallo a medias no deja yt-dlp roto.
    #[cfg(windows)]
    fn install_ytdlp(&self, zip_path: &Path, progress: &mut impl FnMut(f64)) -> Result<PathBuf, String> {
        let corrupt = || "La descarga de yt-dlp llego corrupta o incompleta.".to_string();
        let staging = self.dir.join("yt-dlp.new");
        let _ = std::fs::remove_dir_all(&staging);
        let mut extract = || -> Result<(), String> {
            let f = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
            let mut z = zip::ZipArchive::new(f).map_err(|_| corrupt())?;
            let total = z.len().max(1);
            for i in 0..z.len() {
                let mut entry = z.by_index(i).map_err(|_| corrupt())?;
                // enclosed_name descarta rutas que intenten salir de la carpeta.
                let Some(rel) = entry.enclosed_name() else { continue };
                let out = staging.join(rel);
                if entry.is_dir() {
                    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                    continue;
                }
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
                // copiar hasta el final verifica el CRC del ZIP
                std::io::copy(&mut entry, &mut file).map_err(|_| corrupt())?;
                progress(0.75 + 0.2 * (i + 1) as f64 / total as f64);
            }
            if Self::is_valid_exe(&staging.join("yt-dlp.exe")) { Ok(()) } else { Err(corrupt()) }
        };
        let r = extract();
        let _ = std::fs::remove_file(zip_path);
        if let Err(e) = r {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        let dest = self.ytdlp_dir();
        let old = self.dir.join("yt-dlp.old");
        let _ = std::fs::remove_dir_all(&old);
        if dest.exists() && std::fs::rename(&dest, &old).is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err("yt-dlp esta en uso (hay una descarga en curso); intentalo de nuevo cuando termine.".into());
        }
        if let Err(e) = std::fs::rename(&staging, &dest) {
            let _ = std::fs::rename(&old, &dest);
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e.to_string());
        }
        let _ = std::fs::remove_dir_all(&old);
        // El yt-dlp.exe de un solo archivo de versiones anteriores ya sobra.
        let _ = std::fs::remove_file(self.dir.join("yt-dlp.exe"));
        Ok(dest.join("yt-dlp.exe"))
    }

    #[cfg(not(windows))]
    fn install_ytdlp(&self, exe: &Path, _progress: &mut impl FnMut(f64)) -> Result<PathBuf, String> {
        if !Self::is_valid_exe(exe) {
            return Err("El archivo descargado no es un ejecutable valido (incompleto o corrupto).".into());
        }
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755));
        Ok(exe.to_path_buf())
    }

    /// URL del zip "essentials" de la ultima release de GyanD en GitHub (el
    /// nombre del archivo sale de la version: ffmpeg-<tag>-essentials_build.zip).
    fn ffmpeg_github_url(&self) -> Option<String> {
        let tag = self.latest(FFMPEG_REPO, Duration::ZERO)?;
        Some(format!("https://github.com/{FFMPEG_REPO}/releases/download/{tag}/ffmpeg-{tag}-essentials_build.zip"))
    }

    pub fn download_ffmpeg(&self, mut progress: impl FnMut(f64)) -> Result<(), String> {
        if !cfg!(windows) {
            return Err(FFMPEG_MANUAL.into());
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let zip_path = self.dir.join("ffmpeg.zip");
        progress(0.02);
        let mut sources: Vec<String> = self.ffmpeg_github_url().into_iter().collect();
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
        // Si el ejecutable no cambio desde la ultima vez (mismo tamano y
        // fecha), su version guardada vale: no hace falta ejecutarlo al abrir.
        let stamp = file_stamp(path);
        if let Some(v) = stamp.as_deref().and_then(|s| self.saved_version(&key, s)) {
            self.ver_cache.lock().unwrap().insert(key, (Instant::now(), v.clone()));
            return v;
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
        if let (Some(s), false) = (stamp, version.is_empty()) {
            self.save_version(&key, &s, &version);
        }
        self.ver_cache.lock().unwrap().insert(key, (Instant::now(), version.clone()));
        version
    }

    /// versions.json: { ruta: { "stamp": "tamano-fecha", "version": "..." } }
    fn versions_file(&self) -> PathBuf {
        self.dir.join("versions.json")
    }

    fn saved_version(&self, key: &str, stamp: &str) -> Option<String> {
        let all: serde_json::Value = serde_json::from_slice(&std::fs::read(self.versions_file()).ok()?).ok()?;
        let e = all.get(key)?;
        (e.get("stamp")?.as_str()? == stamp).then(|| e.get("version")?.as_str().map(str::to_string))?
    }

    fn save_version(&self, key: &str, stamp: &str, version: &str) {
        let _guard = self.files.lock().unwrap();
        let path = self.versions_file();
        let mut all: serde_json::Value = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        all[key] = serde_json::json!({ "stamp": stamp, "version": version });
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(&path, serde_json::to_vec_pretty(&all).unwrap_or_default());
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
        let (installed, latest) = self.installed_and_latest(key, Duration::ZERO)?;
        (installed == latest).then_some(latest)
    }

    /// Hay una version nueva de la copia de la app (yt-dlp o ffmpeg). Consulta
    /// GitHub como mucho cada `max_age`; si no hay copia propia, no se pudo
    /// leer su version o GitHub no responde, devuelve false (sin aviso).
    pub fn update_available(&self, key: &str, max_age: Duration) -> bool {
        self.installed_and_latest(key, max_age).is_some_and(|(installed, latest)| installed != latest)
    }

    /// (version instalada, ultima publicada), comparables entre si.
    fn installed_and_latest(&self, key: &str, max_age: Duration) -> Option<(String, String)> {
        let installed = if key == "ffmpeg" {
            let ff = self.managed_path("ffmpeg")?;
            self.managed_path("ffprobe")?;
            // "9.0.2-essentials_build-www.gyan.dev" -> "9.0.2" (el tag de GyanD)
            self.binary_version(&ff).split('-').next().unwrap_or("").to_string()
        } else {
            self.binary_version(&self.managed_path("yt-dlp")?) // "2026.10.26.220202"
        };
        let latest = self.latest(if key == "ffmpeg" { FFMPEG_REPO } else { YTDLP_REPO }, max_age)?;
        (!installed.is_empty()).then_some((installed, latest))
    }

    // ------------------------------------------------------------ ultima version (con cache)
    /// latest.json: { repo: { "checked": segundos, "tag": "...", "notes_tag": "...", "notes": "..." } }
    fn latest_file(&self) -> PathBuf {
        self.dir.join("latest.json")
    }

    fn read_latest(&self) -> serde_json::Value {
        std::fs::read(self.latest_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}))
    }

    /// Cambia la entrada de `repo` en latest.json, releyendo el archivo con el
    /// cerrojo tomado para no perder lo que haya escrito otro hilo.
    fn update_latest(&self, repo: &str, change: impl FnOnce(&mut serde_json::Value)) {
        let _guard = self.files.lock().unwrap();
        let mut all = self.read_latest();
        change(&mut all[repo]);
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.latest_file(), serde_json::to_vec_pretty(&all).unwrap_or_default());
    }

    /// Ultima version de `repo`. Si se consulto hace menos de `max_age`, usa
    /// la guardada sin preguntar a GitHub; si GitHub no responde, tambien
    /// (aunque sea vieja). Duration::ZERO pregunta siempre.
    pub fn latest(&self, repo: &str, max_age: Duration) -> Option<String> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let all = self.read_latest();
        let saved = all.get(repo);
        let saved_tag = saved.and_then(|e| e.get("tag")).and_then(|t| t.as_str()).map(str::to_string);
        let checked = saved.and_then(|e| e.get("checked")).and_then(|t| t.as_u64()).unwrap_or(0);
        if saved_tag.is_some() && now.saturating_sub(checked) < max_age.as_secs() {
            return saved_tag;
        }
        let Some(tag) = latest_tag(repo) else { return saved_tag };
        self.update_latest(repo, |e| {
            e["checked"] = now.into();
            e["tag"] = tag.clone().into();
        });
        Some(tag)
    }

    /// Notas de la release `tag` de `repo`, guardadas para no volver a
    /// pedirlas a la API cada vez que se abre la app.
    pub fn release_notes(&self, repo: &str, tag: &str) -> Option<String> {
        let all = self.read_latest();
        if let Some(e) = all.get(repo).filter(|e| e.get("notes_tag").and_then(|t| t.as_str()) == Some(tag)) {
            return e.get("notes").and_then(|n| n.as_str()).map(str::to_string);
        }
        let body = fetch_release_body(repo, tag)?;
        self.update_latest(repo, |e| {
            e["notes_tag"] = tag.into();
            e["notes"] = body.clone().into();
        });
        Some(body)
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

/// "tamano-fecha de modificacion" de un archivo, para saber si cambio.
fn file_stamp(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", m.len(), t.as_nanos()))
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
