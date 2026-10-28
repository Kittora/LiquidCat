//! Notificacion del sistema al terminar un trabajo.
//!
//! En Windows se usa directamente la API de notificaciones (WinRT) en lugar de
//! lanzar un PowerShell oculto: sale al instante y los antivirus no lo ven
//! como sospechoso.

/// Identificador con el que Windows agrupa las notificaciones de LiquidCat.
/// Es el mismo que lleva el acceso directo del menu Inicio (`ensure_shortcut`),
/// de donde Windows saca el nombre y el icono de la notificacion.
#[cfg(windows)]
const APP_ID: &str = "LiquidCat";

#[cfg(windows)]
pub fn show(title: &str, message: &str) {
    let (title, message) = (title.to_string(), message.to_string());
    // En su propio hilo: la primera vez Windows carga la API de notificaciones.
    std::thread::spawn(move || {
        let _ = toast(&title, &message);
    });
}

#[cfg(windows)]
fn toast(title: &str, message: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastTemplateType};
    // Plantilla clasica de dos lineas (titulo y mensaje). Con un XML
    // "ToastGeneric" escrito a mano, Windows aceptaba la notificacion pero no
    // la mostraba en una app sin instalador. El texto se inserta como nodos,
    // asi no hace falta escapar nada a mano.
    let doc = ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02)?;
    let lines = doc.GetElementsByTagName(&HSTRING::from("text"))?;
    for (i, text) in [title, message].into_iter().enumerate() {
        let node = doc.CreateTextNode(&HSTRING::from(text))?;
        lines.Item(i as u32)?.AppendChild(&node)?;
    }
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
}

/// Acceso directo de LiquidCat en el menu Inicio del usuario, con el mismo
/// identificador que sus notificaciones. Sin el, Windows no reconoce una app
/// portable (sin instalador) y muestra sus notificaciones sin icono. Se crea al
/// abrir la app, y se rehace si el exe cambio de sitio o de nombre (cada
/// version se descarga con un nombre distinto).
#[cfg(windows)]
pub fn ensure_shortcut() {
    std::thread::spawn(|| {
        let (Ok(exe), Some(appdata)) = (std::env::current_exe(), std::env::var_os("APPDATA")) else { return };
        let lnk = std::path::PathBuf::from(appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\LiquidCat.lnk");
        // Ruta del exe con la que se hizo el acceso directo, para no
        // rehacerlo en cada arranque.
        let Some(marker) = crate::util::bin_dir().parent().map(|d| d.join("shortcut-target.txt")) else { return };
        let target = exe.to_string_lossy().to_string();
        if lnk.is_file() && std::fs::read_to_string(&marker).ok().as_deref() == Some(target.as_str()) {
            return;
        }
        if write_shortcut(&exe, &lnk).is_ok() {
            let _ = std::fs::write(&marker, &target);
        }
    });
}

#[cfg(windows)]
fn write_shortcut(exe: &std::path::Path, lnk: &std::path::Path) -> windows::core::Result<()> {
    use windows::core::{Interface, HSTRING, PWSTR};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let exe_w = HSTRING::from(exe.as_os_str());
        link.SetPath(&exe_w)?;
        link.SetIconLocation(&exe_w, 0)?;
        if let Some(dir) = exe.parent() {
            link.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        link.SetDescription(&HSTRING::from("LiquidCat"))?;
        // El identificador va como texto (VT_LPWSTR) en las propiedades del
        // acceso directo. `id` vive hasta el Commit.
        let mut id: Vec<u16> = APP_ID.encode_utf16().chain(std::iter::once(0)).collect();
        let mut value = PROPVARIANT::default();
        (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
        (*value.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(id.as_mut_ptr());
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&PKEY_AppUserModel_ID, &value)?;
        store.Commit()?;
        let file: IPersistFile = link.cast()?;
        file.Save(&HSTRING::from(lnk.as_os_str()), true)
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
