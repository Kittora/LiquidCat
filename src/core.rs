//! Estado compartido del backend + helpers para hablar con la interfaz desde hilos.
use crate::config::{Config, HistoryEntry};
use crate::deps::DepManager;
use crate::lang::tr;
use crate::util;
use crate::{App, LogLine, S};
use slint::{ComponentHandle, Model, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Default, Clone)]
pub struct Bins {
    pub yt_dlp: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
}

#[derive(Default, Clone)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub thumb: String,
    pub description: String,
    pub date: String,
    pub url: String,
}

#[derive(Default)]
pub struct Formats {
    pub video_ids: Vec<String>,
    pub audio_ids: Vec<String>,
    pub video_info: Vec<crate::jobs::FmtInfo>,
    pub audio_info: Vec<crate::jobs::FmtInfo>,
    pub loaded: bool,
}

#[derive(Clone)]
pub struct Job {
    pub id: i32,
    pub title: String,
    pub status: i32, // 0 espera, 1 procesando, 2 listo, 3 fallo
    pub output: String,
    pub task: crate::jobs::Task,
}

#[derive(Default)]
pub struct Queue {
    pub jobs: Vec<Job>,
    pub seq: i32,
    pub cancelled: bool,
    pub worker_alive: bool,
    pub current: Option<i32>,
}

pub struct Core {
    pub ui: slint::Weak<App>,
    pub cfg: Mutex<Config>,
    pub deps: DepManager,
    pub bins: Mutex<Bins>,
    pub lang: Mutex<String>,
    pub cancel: AtomicBool,
    pub queue: Mutex<Queue>,
    pub formats: Mutex<Formats>,
    pub meta: Mutex<Meta>,
    pub last_output: Mutex<String>,
    pub last_log: Mutex<String>,
    pub thumb_cache: Mutex<HashMap<String, (Vec<u8>, u32, u32)>>,
    pub running: AtomicBool,
}

// Modelos que viven en el hilo de la interfaz.
thread_local! {
    pub static LOG: RefCell<Option<Rc<VecModel<LogLine>>>> = const { RefCell::new(None) };
    // Ultimos eventos "amigables" (niveles 2..5) que se ven en la tarjeta de Actividad.
    pub static EVENTS: RefCell<Option<Rc<VecModel<LogLine>>>> = const { RefCell::new(None) };
    pub static TOAST: slint::Timer = slint::Timer::default();
    // Oculta la Actividad flotante un rato despues de terminar un trabajo.
    pub static ACT_HIDE: slint::Timer = slint::Timer::default();
}

pub fn init_models(app: &App) {
    let m = Rc::new(VecModel::<LogLine>::default());
    app.global::<S>().set_log(m.clone().into());
    LOG.with(|l| *l.borrow_mut() = Some(m));
    let e = Rc::new(VecModel::<LogLine>::default());
    app.global::<S>().set_events(e.clone().into());
    EVENTS.with(|l| *l.borrow_mut() = Some(e));
}

/// Cuantos eventos se ven en Actividad; el resto queda en el registro tecnico.
const EVENTS_SHOWN: usize = 3;

/// Nivel de una linea: 0 detalle, 1 comando, 2 ok, 3 error/aviso, 4 info,
/// 5 inicio de un trabajo. Actividad muestra los ultimos eventos 2..5; el
/// registro tecnico completo se ve en el modal de Detalles. El prefijo se
/// quita porque el nivel ya se ve con un icono/color.
pub fn push_log(app: &App, text: &str) {
    let (level, shown) = [("$ ", 1), ("[OK] ", 2), ("[OK]", 2), ("[ERROR] ", 3), ("[!] ", 3), ("[i] ", 4), ("[>] ", 5)]
        .iter()
        .find_map(|(p, lv)| text.strip_prefix(p).map(|rest| (*lv, if *lv == 1 { text } else { rest })))
        .unwrap_or((0, text));
    if level >= 2 {
        EVENTS.with(|l| {
            if let Some(m) = l.borrow().as_ref() {
                m.push(LogLine { text: SharedString::from(shown), level });
                while m.row_count() > EVENTS_SHOWN {
                    m.remove(0);
                }
            }
        });
    }
    LOG.with(|l| {
        if let Some(m) = l.borrow().as_ref() {
            m.push(LogLine { text: SharedString::from(shown), level });
            while m.row_count() > 400 {
                m.remove(0);
            }
        }
    });
    let s = app.global::<S>();
    s.set_log_rev(s.get_log_rev() + 1);
}

pub fn show_toast(app: &App, msg: &str, kind: i32) {
    let s = app.global::<S>();
    s.set_toast_text(SharedString::from(msg));
    s.set_toast_kind(kind);
    s.set_toast_on(true);
    let weak = app.as_weak();
    TOAST.with(|t| {
        t.start(slint::TimerMode::SingleShot, std::time::Duration::from_millis(4000), move || {
            if let Some(a) = weak.upgrade() {
                a.global::<S>().set_toast_on(false);
            }
        });
    });
}

impl Core {
    pub fn new(ui: slint::Weak<App>) -> Arc<Core> {
        let cfg = Config::load();
        let lang = {
            let l = cfg.pref_str("lang");
            if crate::lang::supported(&l) { l } else { crate::lang::system() }
        };
        Arc::new(Core {
            ui,
            cfg: Mutex::new(cfg),
            deps: DepManager::new(),
            bins: Mutex::new(Bins::default()),
            lang: Mutex::new(lang),
            cancel: AtomicBool::new(false),
            queue: Mutex::new(Queue::default()),
            formats: Mutex::new(Formats::default()),
            meta: Mutex::new(Meta::default()),
            last_output: Mutex::new(String::new()),
            last_log: Mutex::new(String::new()),
            thumb_cache: Mutex::new(HashMap::new()),
            running: AtomicBool::new(false),
        })
    }

    pub fn lang(&self) -> String {
        self.lang.lock().unwrap().clone()
    }

    pub fn t(&self, key: &str) -> String {
        tr(&self.lang(), key)
    }

    /// Ejecuta `f` en el hilo de la interfaz.
    pub fn ui<F: FnOnce(&App) + Send + 'static>(&self, f: F) {
        let _ = self.ui.upgrade_in_event_loop(move |a| f(&a));
    }

    pub fn log(&self, text: &str) {
        {
            // yt-dlp suele repetir lineas de estado: se evitan duplicados consecutivos
            let mut last = self.last_log.lock().unwrap();
            if *last == text {
                return;
            }
            *last = text.to_string();
        }
        let t = text.to_string();
        self.ui(move |a| push_log(a, &t));
    }

    pub fn progress(&self, frac: Option<f32>, label: Option<String>) {
        self.ui(move |a| {
            let s = a.global::<S>();
            if let Some(f) = frac {
                s.set_progress(f.clamp(0.0, 1.0));
            }
            if let Some(l) = label {
                s.set_progress_label(SharedString::from(l));
            }
        });
    }

    pub fn toast(&self, msg: &str, kind: i32) {
        let m = msg.to_string();
        self.ui(move |a| show_toast(a, &m, kind));
    }

    pub fn bins(&self) -> Bins {
        self.bins.lock().unwrap().clone()
    }

    /// Binario respetando la ruta manual guardada en el config.
    pub fn resolve(&self, name: &str) -> Option<PathBuf> {
        let manual = self.cfg.lock().unwrap().manual_bin.get(name).cloned().flatten();
        if let Some(m) = manual.filter(|m| !m.is_empty()) {
            let p = PathBuf::from(m);
            if p.is_file() {
                return Some(p);
            }
        }
        self.deps.find_binary(name)
    }

    pub fn refresh_bins(&self) {
        let mut b = self.bins.lock().unwrap();
        b.yt_dlp = self.resolve("yt-dlp");
        b.ffmpeg = self.resolve("ffmpeg");
        b.ffprobe = self.resolve("ffprobe");
    }

    /// `browser` es lo elegido en la interfaz y manda siempre: vacio significa
    /// "Ninguna", no "usar el de Ajustes" (si no, elegir Ninguna no tenia efecto).
    pub fn cookies_args(&self, browser: &str) -> Vec<String> {
        let b = browser.trim();
        if ["chrome", "edge", "firefox", "brave", "opera", "vivaldi", "chromium"].contains(&b) {
            vec!["--cookies-from-browser".into(), b.to_string()]
        } else {
            vec![]
        }
    }

    // ------------------------------------------------------------ historial
    pub fn add_history(&self, path: &std::path::Path, kind: &str, duration: Option<f64>) {
        let meta = self.meta.lock().unwrap().clone();
        let entry = HistoryEntry {
            path: path.to_string_lossy().to_string(),
            name: path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            kind: kind.to_string(),
            ts: util::now_secs(),
            title: meta.title,
            uploader: meta.artist,
            thumb: meta.thumb,
            duration: duration.map(|d| serde_json::json!(d)).unwrap_or(serde_json::Value::Null),
        };
        {
            let mut cfg = self.cfg.lock().unwrap();
            cfg.history.insert(0, entry);
            cfg.history.truncate(100);
            cfg.save();
        }
        self.ui(|a| {
            if a.global::<S>().get_modal() == 4 {
                crate::ui_bridge::refresh_history(a);
            }
        });
    }

    // ------------------------------------------------------------ notificaciones
    pub fn notify(&self, title: &str, message: &str) {
        crate::notify::show(title, message);
    }

    pub fn beep(&self, ok: bool) {
        crate::sound::play(if ok { crate::sound::Sound::Done } else { crate::sound::Sound::Error });
    }

    /// Cierra un trabajo: registro, sonido, estado de la cola y de la interfaz.
    pub fn finish(&self, ok: bool, message: &str) {
        self.log(&format!("{}{}", if ok { "[OK] " } else { "[ERROR] " }, message));
        self.beep(ok);
        let out = if ok { self.last_output.lock().unwrap().clone() } else { String::new() };
        {
            let mut q = self.queue.lock().unwrap();
            if let Some(cur) = q.current {
                if let Some(j) = q.jobs.iter_mut().find(|j| j.id == cur) {
                    j.status = if ok { 2 } else { 3 };
                    if ok {
                        j.output = out.clone();
                    }
                }
            }
        }
        self.emit_queue();
        let label = if ok { self.t("done") } else { self.t("stopped") };
        let toast = if ok { 1 } else { 3 };
        // Los trabajos locales no tienen URL de origen: su aviso habla de
        // conversion, no de descarga.
        let local = self.meta.lock().unwrap().url.is_empty();
        let toast_key = match (ok, local) {
            (true, false) => "toast_done",
            (false, false) => "toast_stopped",
            (true, true) => "toast_conv_done",
            (false, true) => "toast_conv_stopped",
        };
        let toast_msg = crate::lang::ui(&self.lang(), toast_key);
        self.ui(move |a| {
            let s = a.global::<S>();
            s.set_busy(false);
            s.set_current_state(if ok { 2 } else { 3 });
            // La flotante se queda unos segundos para ver el resultado (mas si
            // fallo, para alcanzar a leer el error) y se oculta si no empezo otro.
            let weak = a.as_weak();
            ACT_HIDE.with(|t| {
                t.start(slint::TimerMode::SingleShot, std::time::Duration::from_secs(if ok { 6 } else { 10 }), move || {
                    if let Some(a) = weak.upgrade() {
                        let s = a.global::<S>();
                        if s.get_current_state() != 1 {
                            s.set_act_visible(false);
                        }
                    }
                });
            });
            s.set_progress_label(SharedString::from(label));
            if ok && !out.is_empty() {
                s.set_can_open_folder(true);
            }
            show_toast(a, &toast_msg, toast);
        });
    }

    pub fn finish_done(&self, out_path: &std::path::Path, kind: &str, duration: Option<f64>) {
        self.progress(Some(1.0), Some(self.t("done")));
        *self.last_output.lock().unwrap() = out_path.to_string_lossy().to_string();
        self.add_history(out_path, kind, duration);
        let (notify, open) = {
            let c = self.cfg.lock().unwrap();
            (c.pref_bool("notify_on_done", true), c.pref_bool("open_when_done", false))
        };
        if notify {
            let name = out_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.notify("LiquidCat", &format!("{} · {}", self.t("done"), name));
            self.ui(|a| {
                use slint::winit_030::{winit, WinitWindowAccessor};
                a.window().with_winit_window(|w| w.request_user_attention(Some(winit::window::UserAttentionType::Informational)));
            });
        }
        if open {
            if let Some(parent) = out_path.parent() {
                let _ = open::that(parent);
            }
        }
        // En Actividad basta el nombre del archivo; la ruta completa queda en el
        // registro tecnico (Detalles) y en "Abrir carpeta".
        self.log(&out_path.to_string_lossy());
        let name = out_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.finish(true, &crate::lang::trf(&self.lang(), "final", &[("path", &name)]));
    }

    // ------------------------------------------------------------ cola (UI)
    pub fn emit_queue(&self) {
        let (items, active): (Vec<crate::QItem>, i32) = {
            let q = self.queue.lock().unwrap();
            let items = q
                .jobs
                .iter()
                .map(|j| crate::QItem {
                    id: j.id,
                    title: SharedString::from(j.title.as_str()),
                    status: j.status,
                    output: SharedString::from(j.output.as_str()),
                })
                .collect();
            let active = q.jobs.iter().filter(|j| j.status == 0 || j.status == 1).count() as i32;
            (items, active)
        };
        self.ui(move |a| {
            let s = a.global::<S>();
            s.set_queue(slint::ModelRc::new(VecModel::from(items)));
            s.set_queue_active(active);
        });
    }
}
