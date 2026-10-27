//! Deteccion de la GPU para elegir el encoder por hardware.
//!
//! En Windows se hace con APIs nativas (DXGI y SetupAPI) en lugar de lanzar
//! PowerShell: es instantaneo y los antivirus no lo ven como sospechoso.
//! Se identifica al fabricante por su ID de PCI, no por el nombre.

const NVIDIA: u32 = 0x10DE;
const AMD: [u32; 2] = [0x1002, 0x1022];
const INTEL: u32 = 0x8086;

/// "nvidia", "amd", "intel" o "cpu" ("apple" en macOS). Con varias GPU (p. ej.
/// un portatil con Intel integrada y NVIDIA dedicada) gana la dedicada:
/// NVIDIA > AMD > Intel.
pub fn detect() -> &'static str {
    // Todos los Mac compatibles codifican por hardware con VideoToolbox.
    if cfg!(target_os = "macos") {
        return "apple";
    }
    let found = pick(&primary_vendors());
    if found != "cpu" {
        return found;
    }
    pick(&fallback_vendors())
}

fn pick(vendors: &[u32]) -> &'static str {
    if vendors.contains(&NVIDIA) {
        "nvidia"
    } else if vendors.iter().any(|v| AMD.contains(v)) {
        "amd"
    } else if vendors.contains(&INTEL) {
        "intel"
    } else {
        "cpu"
    }
}

/// Adaptadores activos segun DirectX (DXGI). Se ignora el adaptador por
/// software de Microsoft ("Basic Render Driver").
#[cfg(windows)]
fn primary_vendors() -> Vec<u32> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
    let mut out = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            if let Ok(desc) = adapter.GetDesc1() {
                if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0 {
                    out.push(desc.VendorId);
                }
            }
            i += 1;
        }
    }
    out
}

/// Respaldo para cuando DXGI solo ve el adaptador basico (Escritorio remoto,
/// driver a medio instalar): las tarjetas de video presentes segun el
/// administrador de dispositivos (SetupAPI), leyendo "VEN_xxxx" de su ID.
#[cfg(windows)]
fn fallback_vendors() -> Vec<u32> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
        DIGCF_PRESENT, GUID_DEVCLASS_DISPLAY, SPDRP_HARDWAREID, SP_DEVINFO_DATA,
    };
    let mut out = Vec::new();
    unsafe {
        let set = SetupDiGetClassDevsW(&GUID_DEVCLASS_DISPLAY, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT);
        if set as isize == -1 {
            return out;
        }
        let mut i = 0;
        loop {
            let mut info: SP_DEVINFO_DATA = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiEnumDeviceInfo(set, i, &mut info) == 0 {
                break;
            }
            let mut buf = [0u16; 1024];
            let ok = SetupDiGetDeviceRegistryPropertyW(
                set,
                &info,
                SPDRP_HARDWAREID,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut u8,
                (buf.len() * 2) as u32,
                std::ptr::null_mut(),
            );
            if ok != 0 {
                if let Some(v) = vendor_from_hwid(&String::from_utf16_lossy(&buf)) {
                    out.push(v);
                }
            }
            i += 1;
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

/// "PCI\VEN_10DE&DEV_2504&..." -> 0x10DE
#[cfg(windows)]
fn vendor_from_hwid(id: &str) -> Option<u32> {
    let rest = &id[id.to_uppercase().find("VEN_")? + 4..];
    u32::from_str_radix(rest.get(..4)?, 16).ok()
}

/// Linux: "lspci -n" da el ID del fabricante de cada tarjeta de video.
#[cfg(target_os = "linux")]
fn primary_vendors() -> Vec<u32> {
    let Ok(o) = crate::util::cmd("lspci").arg("-n").output() else { return Vec::new() };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        // Clases 0300 (VGA), 0302 (3D) y 0380 (display).
        .filter(|l| l.contains(" 0300: ") || l.contains(" 0302: ") || l.contains(" 0380: "))
        .filter_map(|l| {
            let ids = l.split(": ").nth(1)?;
            u32::from_str_radix(ids.get(..4)?, 16).ok()
        })
        .collect()
}

// En macOS ffmpeg usa VideoToolbox (Apple), que no depende del fabricante.
#[cfg(not(any(windows, target_os = "linux")))]
fn primary_vendors() -> Vec<u32> {
    Vec::new()
}

#[cfg(not(windows))]
fn fallback_vendors() -> Vec<u32> {
    Vec::new()
}
