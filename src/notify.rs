//! Notificacion del sistema al terminar un trabajo.
//!
//! En Windows se usa directamente la API de notificaciones (WinRT) en lugar de
//! lanzar un PowerShell oculto: sale al instante y los antivirus no lo ven
//! como sospechoso.

/// Identificador con el que Windows agrupa las notificaciones de LiquidCat.
#[cfg(windows)]
const APP_ID: &str = "LiquidCat";

#[cfg(windows)]
pub fn show(title: &str, message: &str) {
    let (title, message) = (title.to_string(), message.to_string());
    // En su propio hilo: la primera vez Windows carga la API de notificaciones.
    std::thread::spawn(move || {
        register_app();
        let _ = toast(&title, &message);
    });
}

#[cfg(windows)]
fn toast(title: &str, message: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastTemplateType};
    // Plantilla clasica de dos lineas (titulo y mensaje). Con un XML
    // "ToastGeneric" escrito a mano, Windows aceptaba la notificacion pero no
    // la mostraba en apps sin instalador. El texto se inserta como nodos, asi
    // no hace falta escapar nada a mano.
    let doc = ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02)?;
    let lines = doc.GetElementsByTagName(&HSTRING::from("text"))?;
    for (i, text) in [title, message].into_iter().enumerate() {
        let node = doc.CreateTextNode(&HSTRING::from(text))?;
        lines.Item(i as u32)?.AppendChild(&node)?;
    }
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
}

/// Registra el nombre y el icono de LiquidCat para sus notificaciones (sin
/// esto Windows puede mostrar un icono generico). Solo toca la rama del
/// usuario del registro (HKCU), no necesita permisos de administrador.
#[cfg(windows)]
fn register_app() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let icon = crate::util::bin_dir().parent().map(|d| d.join("notify-icon.png"));
        let Some(icon) = icon else { return };
        if std::fs::write(&icon, include_bytes!("../assets/logo.png")).is_err() {
            return;
        }
        let key = format!("Software\\Classes\\AppUserModelId\\{APP_ID}");
        set_reg_string(&key, "DisplayName", "LiquidCat");
        set_reg_string(&key, "IconUri", &icon.to_string_lossy());
    });
}

#[cfg(windows)]
fn set_reg_string(key: &str, name: &str, value: &str) {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegCreateKeyW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, name, value) = (wide(key), wide(name), wide(value));
    unsafe {
        let mut h: HKEY = std::ptr::null_mut();
        if RegCreateKeyW(HKEY_CURRENT_USER, key.as_ptr(), &mut h) != 0 {
            return;
        }
        RegSetValueExW(h, name.as_ptr(), 0, REG_SZ, value.as_ptr() as *const u8, (value.len() * 2) as u32);
        RegCloseKey(h);
    }
}

#[cfg(target_os = "macos")]
pub fn show(title: &str, message: &str) {
    // AppleScript: las comillas y barras se escapan para no romper la cadena.
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", esc(message), esc(title));
    let _ = crate::util::cmd("osascript").args(["-e", &script]).spawn();
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn show(title: &str, message: &str) {
    let _ = crate::util::cmd("notify-send").arg(title).arg(message).spawn();
}
