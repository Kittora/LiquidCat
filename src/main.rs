#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config;
mod core;
mod deps;
mod i18n_data;
mod jobs;
mod lang;
mod media;
mod themes;
mod ui_bridge;
mod util;

slint::include_modules!();

fn main() {
    // Renderer por software por defecto: es el que menos memoria usa (~25 MB).
    // Para forzar OpenGL: LIQUIDCAT_GL=1
    if std::env::var_os("SLINT_BACKEND").is_none() && std::env::var_os("LIQUIDCAT_GL").is_none() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
    let app = App::new().expect("no se pudo crear la ventana");
    let core = core::Core::new(app.as_weak());
    core::init_models(&app);
    let first_run = core.cfg.lock().unwrap().prefs.get("dl_settings").is_none();
    ui_bridge::load_initial(&core, &app);
    ui_bridge::setup(&core, &app);
    ui_bridge::startup_deps(&core, first_run);
    app.run().expect("error en el bucle de eventos");
}
