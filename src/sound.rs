//! Sonidos de aviso. Se sintetizan al vuelo (campanita suave: seno con un par
//! de armonicos y caida exponencial) y se reproducen con la API de Windows en
//! segundo plano, sin bloquear. Antes se usaba `Beep`, un pitido cuadrado de
//! "altavoz de PC" que ademas detenia el hilo mientras sonaba.
use std::sync::OnceLock;

#[derive(Clone, Copy)]
pub enum Sound {
    /// Trabajo terminado bien: dos notas ascendentes (Do6 -> Sol6).
    Done,
    /// Algo fallo: dos notas descendentes, mas graves y suaves.
    Error,
    /// Formatos listos: un toque corto y discreto.
    Ready,
}

const RATE: u32 = 44_100;

/// (frecuencia Hz, inicio s, duracion s, volumen relativo)
fn notes(s: Sound) -> &'static [(f32, f32, f32, f32)] {
    match s {
        Sound::Done => &[(1046.5, 0.0, 0.55, 0.9), (1568.0, 0.11, 0.75, 0.8)],
        Sound::Error => &[(587.3, 0.0, 0.45, 0.8), (440.0, 0.16, 0.65, 0.8)],
        Sound::Ready => &[(1318.5, 0.0, 0.35, 0.7)],
    }
}

/// WAV PCM 16 bits mono con el sonido ya mezclado.
pub fn wav(s: Sound) -> Vec<u8> {
    let ns = notes(s);
    let len = ns.iter().map(|n| n.1 + n.2).fold(0.0f32, f32::max);
    let total = (len * RATE as f32) as usize;
    let mut buf = vec![0f32; total];
    for &(freq, start, dur, vol) in ns {
        let s0 = (start * RATE as f32) as usize;
        let count = (dur * RATE as f32) as usize;
        for i in 0..count {
            let t = i as f32 / RATE as f32;
            // ataque de 6 ms (sin "clic") y caida exponencial tipo campana
            let env = (t / 0.006).min(1.0) * (-t * 6.5 / dur).exp();
            let w = std::f32::consts::TAU * freq * t;
            let v = (w.sin() + 0.25 * (2.0 * w).sin() + 0.08 * (3.0 * w).sin()) * env * vol;
            if let Some(x) = buf.get_mut(s0 + i) {
                *x += v;
            }
        }
    }
    // Volumen moderado (pico al 30 %) y fundido final de 15 ms.
    let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    let fade = (0.015 * RATE as f32) as usize;
    let n = buf.len();
    let samples: Vec<i16> = buf
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let tail = if i + fade > n { (n - i) as f32 / fade as f32 } else { 1.0 };
            (x / peak * 0.30 * tail * i16::MAX as f32) as i16
        })
        .collect();

    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // tamano del bloque fmt
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes por segundo
    out.extend_from_slice(&2u16.to_le_bytes()); // alineacion de bloque
    out.extend_from_slice(&16u16.to_le_bytes()); // bits por muestra
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in samples {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Reproduce el sonido sin esperar a que termine.
pub fn play(s: Sound) {
    // PlaySound con SND_ASYNC lee el WAV desde memoria mientras suena: los
    // datos tienen que vivir todo el programa, por eso se guardan aqui.
    static CACHE: [OnceLock<Vec<u8>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let idx = s as usize;
    let data = CACHE[idx].get_or_init(|| wav(s));
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        PlaySoundW(data.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
    #[cfg(not(windows))]
    let _ = data;
}
