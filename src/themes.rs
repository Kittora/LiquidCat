//! Aplica un tema (paleta + parametros de cristal/brutalismo) al global `Pal` de la UI.
use crate::{App, Pal, ThemeEntry};
use slint::{Color, ComponentHandle, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

include!("themes_data.rs");

pub const DEFAULT: &str = "catppuccin";

fn col(v: u32) -> Color {
    Color::from_rgb_u8(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

pub fn find(id: &str) -> &'static ThemeDef {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}

pub fn valid(id: &str) -> bool {
    THEMES.iter().any(|t| t.id == id)
}

pub fn entries() -> ModelRc<ThemeEntry> {
    let v: Vec<ThemeEntry> = THEMES
        .iter()
        .map(|t| ThemeEntry {
            id: SharedString::from(t.id),
            name: SharedString::from(t.name),
            c1: col(t.sw[0]),
            c2: col(t.sw[1]),
            c3: col(t.sw[2]),
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

/// Icono de la cabecera dibujado para cada tema (assets/theme-icons).
fn theme_icon_png(id: &str) -> &'static [u8] {
    match id {
        "catppuccin-macchiato" => include_bytes!("../assets/theme-icons/catppuccin-macchiato.png"),
        "catppuccin-frappe" => include_bytes!("../assets/theme-icons/catppuccin-frappe.png"),
        "catppuccin-latte" => include_bytes!("../assets/theme-icons/catppuccin-latte.png"),
        "dracula" => include_bytes!("../assets/theme-icons/dracula.png"),
        "amoled" => include_bytes!("../assets/theme-icons/amoled.png"),
        "dark" => include_bytes!("../assets/theme-icons/dark.png"),
        "light" => include_bytes!("../assets/theme-icons/light.png"),
        "fluent" => include_bytes!("../assets/theme-icons/fluent.png"),
        "gruvbox" => include_bytes!("../assets/theme-icons/gruvbox.png"),
        "nord" => include_bytes!("../assets/theme-icons/nord.png"),
        "tokyo-night" => include_bytes!("../assets/theme-icons/tokyo-night.png"),
        "neo-brutalist-light" => include_bytes!("../assets/theme-icons/neo-brutalist-light.png"),
        "neo-brutalist-dark" => include_bytes!("../assets/theme-icons/neo-brutalist-dark.png"),
        _ => include_bytes!("../assets/theme-icons/catppuccin.png"),
    }
}

/// Lado del recuadro del logo en la cabecera, en px logicos (ver app.slint).
const LOGO_PX: f32 = 46.0;

/// Slint reduce las imagenes con un filtro bilineal sin mipmaps: pasar de
/// 256 px a 46 dejaba el contorno en dientes de sierra. Se entrega ya reducida
/// con Lanczos al tamano exacto en pixeles fisicos, asi se pinta 1:1.
fn theme_icon_rgba(id: &str, px: u32) -> Option<image::RgbaImage> {
    let img = image::load_from_memory(theme_icon_png(id)).ok()?;
    Some(img.resize_exact(px, px, image::imageops::FilterType::Lanczos3).to_rgba8())
}

fn theme_icon(id: &str, px: u32) -> slint::Image {
    match theme_icon_rgba(id, px) {
        Some(img) => {
            let (w, h) = img.dimensions();
            slint::Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h))
        }
        None => slint::Image::default(),
    }
}

/// Icono de la ventana y de la barra de tareas con el gato del tema (solo
/// mientras la app esta abierta: el del .exe es fijo, assets/icon.ico).
/// Va directo a winit porque Slint no reaplica la propiedad `icon` de la
/// ventana una vez creada, y solo pone el icono pequeno; la barra de tareas
/// usa el grande (set_taskbar_icon).
fn set_window_icons(app: &App, id: &str) {
    use slint::winit_030::{winit, WinitWindowAccessor};
    let icon = |px: u32| theme_icon_rgba(id, px).and_then(|i| winit::window::Icon::from_rgba(i.into_raw(), px, px).ok());
    let (small, big) = (icon(SMALL_ICON_PX), icon(BIG_ICON_PX));
    app.window().with_winit_window(move |w| {
        w.set_window_icon(small);
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowExtWindows;
            w.set_taskbar_icon(big);
        }
        #[cfg(not(windows))]
        let _ = big;
    });
}

pub fn apply(app: &App, id: &str) {
    let t = find(id);
    let p = app.global::<Pal>();
    p.set_bg(col(t.bg));
    p.set_bg_accent(col(t.bg_accent));
    p.set_surface(col(t.surface));
    p.set_panel(col(t.panel));
    p.set_panel_strong(col(t.panel_strong));
    p.set_input(col(t.input));
    p.set_line(col(t.line));
    p.set_line_strong(col(t.line_strong));
    p.set_text(col(t.text));
    p.set_muted(col(t.muted));
    p.set_accent(col(t.accent));
    p.set_accent2(col(t.accent2));
    p.set_success(col(t.success));
    p.set_danger(col(t.danger));
    p.set_warning(col(t.warning));
    p.set_on_accent(col(t.on_accent));
    p.set_log_bg(col(t.log_bg));
    p.set_log_text(col(t.log_text));
    p.set_log_border(col(t.log_border));
    p.set_log_cmd(col(t.log_cmd));
    p.set_log_info(col(t.log_info));
    p.set_log_ok(col(t.log_ok));
    p.set_log_err(col(t.log_err));
    p.set_c_search(col(t.c_search));
    p.set_c_preview(col(t.c_preview));
    p.set_c_layers(col(t.c_layers));
    p.set_c_sliders(col(t.c_sliders));
    p.set_c_folder(col(t.c_folder));
    p.set_c_file(col(t.c_file));
    p.set_c_terminal(col(t.c_terminal));
    p.set_hard_panel(col(t.hard_panel));
    p.set_hard_ctrl(col(t.hard_ctrl));
    p.set_brutal(t.brutal);
    if t.brutal {
        // Esquinas rectas: la sombra dura son dos tiras rectas y con esquinas
        // redondeadas quedaba una muesca entre la tarjeta y su sombra.
        p.set_radius(0.0);
        p.set_radius_sm(0.0);
        p.set_border_w(3.0);
        p.set_a_panel(1.0);
        p.set_a_soft(1.0);
        p.set_a_strong(1.0);
        p.set_a_solid(1.0);
        p.set_a_border(1.0);
        p.set_highlight(Color::from_argb_u8(0, 255, 255, 255));
    } else {
        p.set_radius(12.0);
        p.set_radius_sm(8.0);
        p.set_border_w(1.0);
        p.set_a_panel(0.62);
        p.set_a_soft(0.46);
        p.set_a_strong(0.88);
        p.set_a_solid(0.96);
        p.set_a_border(0.36);
        p.set_highlight(Color::from_argb_u8(18, 255, 255, 255));
    }
    let s = app.global::<crate::S>();
    s.set_theme(SharedString::from(t.id));
    refresh_logo(app);
}

/// Lados de los iconos de la ventana: Windows los ajusta a 16-24 px (pequeno)
/// y 32-48 px (barra de tareas) segun la escala; asi solo reduce un poco.
const SMALL_ICON_PX: u32 = 32;
const BIG_ICON_PX: u32 = 64;

/// Vuelve a generar el logo para el tema y la escala actuales (al cambiar de
/// tema, al mostrar la ventana y al moverla a una pantalla con otro DPI), y
/// el icono de la ventana / barra de tareas con el mismo gato.
pub fn refresh_logo(app: &App) {
    let s = app.global::<crate::S>();
    let id = find(&s.get_theme()).id;
    let px = (LOGO_PX * app.window().scale_factor()).round().max(1.0) as u32;
    s.set_logo(theme_icon(id, px));
    set_window_icons(app, id);
}
