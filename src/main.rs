#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod core;
mod deps;
mod gpu;
mod i18n_data;
mod jobs;
mod lang;
mod media;
mod sound;
mod themes;
mod ui_bridge;
mod util;

slint::include_modules!();

/// Esquinas redondeadas nativas (Windows 11). Evita depender de una ventana
/// transparente: la app se pinta opaca y DWM recorta la silueta, asi que no
/// queda ningun fondo detras de las esquinas. En Windows 10 las llamadas
/// simplemente fallan y la ventana queda cuadrada.
#[cfg(windows)]
fn round_window(app: &App) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

    let slint_handle = app.window().window_handle();
    let Ok(handle) = slint_handle.window_handle() else { return };
    let RawWindowHandle::Win32(w) = handle.as_raw() else { return };
    let hwnd = w.hwnd.get() as *mut std::ffi::c_void;

    // DWMWA_WINDOW_CORNER_PREFERENCE (33) = DWMWCP_ROUND (2)
    let round: i32 = 2;
    // DWMWA_BORDER_COLOR (34) = DWMWA_COLOR_NONE (0xFFFFFFFE)
    let no_border: u32 = 0xFFFF_FFFE;
    unsafe {
        DwmSetWindowAttribute(hwnd, 33, &round as *const _ as *const _, std::mem::size_of::<i32>() as u32);
        DwmSetWindowAttribute(hwnd, 34, &no_border as *const _ as *const _, std::mem::size_of::<u32>() as u32);
    }
}

fn main() {
    // Renderer: por defecto Slint usa Skia (GPU, con vsync a la tasa del
    // monitor: 60/120/144 Hz). Se cambio desde FemtoVG porque este dejaba las
    // esquinas redondeadas y los bordes con dientes de sierra. Si no hay GPU
    // disponible, cae solo al renderer por software. Ese fallback automatico
    // solo ocurre si NO forzamos el backend a mano.
    // Para forzar software (menos memoria, sin vsync): LIQUIDCAT_SOFTWARE=1
    if std::env::var_os("SLINT_BACKEND").is_none() && std::env::var_os("LIQUIDCAT_SOFTWARE").is_some() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
    let app = App::new().expect("no se pudo crear la ventana");
    let core = core::Core::new(app.as_weak());
    core::init_models(&app);
    let first_run = core.cfg.lock().unwrap().prefs.get("dl_settings").is_none();
    ui_bridge::load_initial(&core, &app);
    ui_bridge::setup(&core, &app);
    ui_bridge::startup_deps(&core, first_run);
    ui_bridge::check_update(&core);
    // La ventana debe existir antes de pedir el HWND a DWM.
    app.show().expect("no se pudo mostrar la ventana");
    // La escala real de la pantalla solo se conoce con la ventana creada.
    themes::refresh_logo(&app);
    #[cfg(windows)]
    {
        round_window(&app);
        // El handle nativo puede tardar un frame en estar disponible: reintento
        // una vez ya arrancado el bucle de eventos.
        let weak = app.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(120), move || {
            if let Some(app) = weak.upgrade() {
                round_window(&app);
                themes::refresh_logo(&app);
                ui_bridge::fit_window(&app);
            }
        });
    }
    app.run().expect("error en el bucle de eventos");
}
