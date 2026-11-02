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

/// Tarjetas de los temas en Ajustes. Los que siguen el acento del sistema
/// lo muestran en su muestra de color (`accent`: el del sistema, si se usa).
pub fn entries(accent: Option<crate::accent::Accent>) -> ModelRc<ThemeEntry> {
    let v: Vec<ThemeEntry> = THEMES
        .iter()
        .map(|t| ThemeEntry {
            id: SharedString::from(t.id),
            name: SharedString::from(t.name),
            c1: col(t.sw[0]),
            c2: col(match t.id {
                "material" => material_scheme(accent).primary,
                _ => accent.filter(|_| follows_system_accent(t.id)).map(|a| a.main).unwrap_or(t.sw[1]),
            }),
            c3: col(t.sw[2]),
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

/// Temas que usan el color de acento del sistema: Material en cualquier
/// sistema y Fluent solo en Windows (su acento es el de Windows).
fn follows_system_accent(id: &str) -> bool {
    id == "material" || (id == "fluent" && cfg!(windows))
}

/// Semilla de respaldo de Material (Pink A200), sin acento del sistema.
const MATERIAL_SEED: u32 = 0xFF4081;

/// Roles de color de Material 3 (modo oscuro) que usa LiquidCat, como 0xRRGGBB.
struct MaterialScheme {
    primary: u32,
    on_primary: u32,
    secondary_container: u32,
    on_secondary_container: u32,
    tertiary: u32,
    error: u32,
    surface: u32,
    surface_container_lowest: u32,
    surface_container_low: u32,
    surface_container: u32,
    surface_container_high: u32,
    surface_container_highest: u32,
    on_surface: u32,
    on_surface_variant: u32,
    outline: u32,
    outline_variant: u32,
}

/// Esquema oscuro de Material 3 generado con el algoritmo de Google
/// (material-color-utilities, variante "tonal spot", la de Android) a partir
/// del acento del sistema; sin el, del rosa de respaldo.
fn material_scheme(accent: Option<crate::accent::Accent>) -> MaterialScheme {
    use material_colors::color::Argb;
    use material_colors::theme::ThemeBuilder;
    let seed = accent.map(|a| a.base).unwrap_or(MATERIAL_SEED);
    let s = ThemeBuilder::with_source(Argb::from_u32(0xFF00_0000 | seed)).build().schemes.dark;
    let c = |a: Argb| (a.red as u32) << 16 | (a.green as u32) << 8 | a.blue as u32;
    MaterialScheme {
        primary: c(s.primary),
        on_primary: c(s.on_primary),
        secondary_container: c(s.secondary_container),
        on_secondary_container: c(s.on_secondary_container),
        tertiary: c(s.tertiary),
        error: c(s.error),
        surface: c(s.surface),
        surface_container_lowest: c(s.surface_container_lowest),
        surface_container_low: c(s.surface_container_low),
        surface_container: c(s.surface_container),
        surface_container_high: c(s.surface_container_high),
        surface_container_highest: c(s.surface_container_highest),
        on_surface: c(s.on_surface),
        on_surface_variant: c(s.on_surface_variant),
        outline: c(s.outline),
        outline_variant: c(s.outline_variant),
    }
}

/// Aplica el esquema Material 3 a la paleta: todos los colores salen de los
/// roles de Material (superficies teñidas con el acento, primario,
/// contenedores...), no solo el acento.
fn apply_material(p: &Pal, m: &MaterialScheme) {
    p.set_bg(col(m.surface));
    p.set_bg_accent(col(m.surface_container_low));
    p.set_surface(col(m.surface_container_lowest));
    p.set_panel(col(m.surface_container));
    p.set_panel_strong(col(m.surface_container_high));
    p.set_input(col(m.surface_container_highest));
    p.set_line(col(m.outline_variant));
    p.set_line_strong(col(m.outline));
    p.set_text(col(m.on_surface));
    p.set_muted(col(m.on_surface_variant));
    p.set_accent(col(m.primary));
    p.set_accent2(col(m.tertiary));
    p.set_on_accent(col(m.on_primary));
    p.set_danger(col(m.error));
    p.set_tonal(col(m.secondary_container));
    p.set_on_tonal(col(m.on_secondary_container));
    p.set_log_bg(col(m.surface_container_lowest));
    p.set_log_text(col(m.on_surface));
    p.set_log_border(col(m.outline_variant));
    p.set_log_cmd(col(m.primary));
    p.set_log_info(col(m.on_surface_variant));
    p.set_log_err(col(m.error));
    for set in [Pal::set_c_search, Pal::set_c_preview, Pal::set_c_layers, Pal::set_c_sliders, Pal::set_c_folder, Pal::set_c_file, Pal::set_c_terminal] {
        set(p, col(m.primary));
    }
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
        // Material usa el de Mocha teñido con su color primario (tint_to_hue).
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
    let mut img = img.resize_exact(px, px, image::imageops::FilterType::Lanczos3).to_rgba8();
    if id == "material" {
        tint_to_hue(&mut img, MATERIAL_TINT.load(std::sync::atomic::Ordering::Relaxed));
    }
    Some(img)
}

/// Color primario del esquema Material actual: el gato de Material es el de
/// Mocha teñido con este color (ver `tint_to_hue`), asi va a juego con el
/// acento del sistema y cambia con el.
static MATERIAL_TINT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(MATERIAL_SEED);

/// Lleva todos los pixeles al tono (hue) de `color`, conservando su luz y su
/// saturacion: el gato queda monocromo en el color del tema, como los
/// colores tonales de Material.
fn tint_to_hue(img: &mut image::RgbaImage, color: u32) {
    let (target, _, _) = rgb_to_hsl(((color >> 16) & 0xff) as u8, ((color >> 8) & 0xff) as u8, (color & 0xff) as u8);
    for p in img.pixels_mut() {
        let (_, s, l) = rgb_to_hsl(p[0], p[1], p[2]);
        let (r, g, b) = hsl_to_rgb(target, s, l);
        p[0] = r;
        p[1] = g;
        p[2] = b;
    }
}

/// RGB (0-255) -> HSL (tono 0-1, saturacion 0-1, luz 0-1).
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let to = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    if s == 0.0 {
        return (to(l), to(l), to(l));
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let channel = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (to(channel(h + 1.0 / 3.0)), to(channel(h)), to(channel(h - 1.0 / 3.0)))
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
    // Acento del sistema ("Usar el color de acento del sistema" en Ajustes):
    // sustituye al acento propio del tema y a los iconos que lo usan.
    let s = app.global::<crate::S>();
    let system = if s.get_pref_sysaccent() { crate::accent::system() } else { None };
    let material = t.id == "material";
    p.set_material(material);
    if material {
        let scheme = material_scheme(system);
        MATERIAL_TINT.store(scheme.primary, std::sync::atomic::Ordering::Relaxed);
        apply_material(&p, &scheme);
    } else if let Some(a) = system.filter(|_| follows_system_accent(t.id)) {
        let (main, hover) = (col(a.main), col(a.hover));
        p.set_accent(main);
        p.set_accent2(hover);
        p.set_on_accent(col(crate::accent::text_on(a.main)));
        p.set_log_cmd(main);
        p.set_c_search(main);
        p.set_c_preview(main);
        p.set_c_layers(main);
        p.set_c_sliders(main);
        p.set_c_folder(main);
        p.set_c_file(main);
        p.set_c_terminal(main);
    }
    s.set_themes(entries(system));
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
    } else if material {
        // Material 3: superficies solidas (sin cristal) y esquinas grandes en
        // las tarjetas (24 px, entre "large" y "extra large").
        p.set_radius(24.0);
        p.set_radius_sm(12.0);
        p.set_border_w(1.0);
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
