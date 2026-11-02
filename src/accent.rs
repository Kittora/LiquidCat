//! Color de acento del sistema, para los temas que lo siguen (Material en
//! cualquier sistema y Fluent en Windows). Los temas son oscuros, asi que se
//! piden variantes claras del acento para que se lean bien sobre el fondo.

/// Acento como 0xRRGGBB: el color del sistema tal cual (`base`, semilla de
/// Material 3) y la version legible sobre fondo oscuro (`main`, con su
/// variante de hover) que usa Fluent.
#[derive(Clone, Copy)]
pub struct Accent {
    pub base: u32,
    pub main: u32,
    pub hover: u32,
}

/// Mezcla un color con blanco (`t` de 0 a 1): version mas clara.
fn lighten(c: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let v = ((c >> shift) & 0xff) as f32;
        ((v + (255.0 - v) * t).round() as u32).min(255)
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Luminancia relativa (0 negro, 1 blanco), como en las pautas WCAG.
fn luminance(c: u32) -> f32 {
    let lin = |shift: u32| {
        let v = ((c >> shift) & 0xff) as f32 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(16) + 0.7152 * lin(8) + 0.0722 * lin(0)
}

/// Acento legible sobre los fondos oscuros de los temas: si el del sistema es
/// muy oscuro (p. ej. un azul marino), se aclara conservando su tono hasta un
/// contraste de 3:1 con el fondo (lo minimo recomendado para controles).
/// El hover es un poco mas claro.
fn readable(base: u32) -> Accent {
    // Luminancia del fondo de Material/Fluent (~#141218-#1B1C1E): 3:1 pide
    // al menos ~0.12.
    const MIN: f32 = 0.12;
    let mut main = base;
    let mut t = 0.0;
    while luminance(main) < MIN && t < 1.0 {
        t += 0.05;
        main = lighten(base, t);
    }
    Accent { base, main, hover: lighten(main, 0.22) }
}

/// Texto legible sobre el acento: negro sobre colores claros, blanco sobre
/// oscuros.
pub fn text_on(c: u32) -> u32 {
    if luminance(c) > 0.32 { 0x111111 } else { 0xFFFFFF }
}

/// Windows: las variantes claras que el propio Windows usa en modo oscuro.
#[cfg(windows)]
pub fn system() -> Option<Accent> {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    let s = UISettings::new().ok()?;
    let get = |t: UIColorType| s.GetColorValue(t).ok().map(|c| (c.R as u32) << 16 | (c.G as u32) << 8 | c.B as u32);
    // La version legible parte de la variante clara que Windows usa en
    // modo oscuro; la semilla de Material 3 es el acento tal cual.
    let base = get(UIColorType::Accent)?;
    let light = get(UIColorType::AccentLight2).unwrap_or(base);
    Some(Accent { base, ..readable(light) })
}

/// macOS: el "color de resalte" de Ajustes del Sistema. Solo hay unos pocos
/// fijos; sin valor (modo multicolor) se usa el azul.
#[cfg(target_os = "macos")]
pub fn system() -> Option<Accent> {
    let out = crate::util::cmd("defaults").args(["read", "-g", "AppleAccentColor"]).output().ok();
    let idx = out.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<i32>().ok());
    let base = match idx {
        Some(-1) => 0x8C8C8C, // grafito
        Some(0) => 0xFF453A,  // rojo
        Some(1) => 0xFF9F0A,  // naranja
        Some(2) => 0xFFD60A,  // amarillo
        Some(3) => 0x32D74B,  // verde
        Some(5) => 0xBF5AF2,  // morado
        Some(6) => 0xFF375F,  // rosa
        _ => 0x0A84FF,        // azul (y multicolor)
    };
    Some(readable(base))
}

/// Linux: el acento de KDE Plasma (kdeglobals) o el de GNOME 47+. Otros
/// escritorios no tienen: entonces el tema usa su color propio.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn system() -> Option<Accent> {
    kde_accent().or_else(gnome_accent).map(readable)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn kde_accent() -> Option<u32> {
    let home = std::env::var_os("HOME")?;
    let text = std::fs::read_to_string(std::path::Path::new(&home).join(".config/kdeglobals")).ok()?;
    let line = text.lines().find(|l| l.trim_start().starts_with("AccentColor="))?;
    let rgb: Vec<u32> = line.split('=').nth(1)?.split(',').filter_map(|v| v.trim().parse().ok()).collect();
    (rgb.len() >= 3).then(|| (rgb[0].min(255) << 16) | (rgb[1].min(255) << 8) | rgb[2].min(255))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn gnome_accent() -> Option<u32> {
    let out = crate::util::cmd("gsettings").args(["get", "org.gnome.desktop.interface", "accent-color"]).output().ok()?;
    match String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'') {
        "blue" => Some(0x3584E4),
        "teal" => Some(0x2190A4),
        "green" => Some(0x3A944A),
        "yellow" => Some(0xC88800),
        "orange" => Some(0xED5B00),
        "red" => Some(0xE62D42),
        "pink" => Some(0xD56199),
        "purple" => Some(0x9141AC),
        "slate" => Some(0x6F8396),
        _ => None,
    }
}

/// Windows avisa cuando el usuario cambia el acento (o el modo claro/oscuro):
/// `on_change` se llama entonces para volver a aplicar el tema. El objeto que
/// escucha se guarda para toda la vida de la app.
#[cfg(windows)]
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    use std::sync::OnceLock;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::ViewManagement::UISettings;
    static SETTINGS: OnceLock<UISettings> = OnceLock::new();
    let Ok(s) = UISettings::new() else { return };
    let handler = TypedEventHandler::new(move |_, _| {
        on_change();
        Ok(())
    });
    if s.ColorValuesChanged(&handler).is_ok() {
        let _ = SETTINGS.set(s);
    }
}

#[cfg(not(windows))]
pub fn watch(_on_change: impl Fn() + Send + Sync + 'static) {}
