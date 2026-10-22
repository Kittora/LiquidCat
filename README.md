# LiquidCat (Rust nativo)

Reescritura de LiquidCat en **Rust + [Slint](https://slint.dev)**: sin WebView, sin Python y sin
navegador embebido. Es un solo `.exe` que dibuja su propia interfaz.
Sigue usando **yt-dlp** y **ffmpeg** como motor, igual que antes.

## Por qué Slint y no Tauri

Tauri **sí usa un WebView** (WebView2 en Windows): solo es más ligero que Electron.
Como el objetivo era dejar de usar WebView, la interfaz se reescribió con un toolkit nativo.
Slint permite mantener el diseño (paneles redondeados, 14 temas, modales, etc.) con
una huella de memoria pequeña.

## Qué se conserva

- Misma estructura: Descargador, Archivos locales, Cola, Actividad, Historial, Ajustes, Wiki, Créditos.
- Los 14 temas, español/inglés, presets, historial y preferencias.
- Usa el **mismo `config.json`** (`%APPDATA%\LiquidCat`), así que tus presets e historial siguen ahí.
- Descarga automática de yt-dlp y ffmpeg a `%LOCALAPPDATA%\LiquidCat\bin` (con verificación).
- Aceleración NVENC/AMF/CPU, recorte, GIF, audio (MP3/AAC/Opus/OGG/FLAC/WAV), subtítulos,
  carátula y metadatos, cookies del navegador, cola y playlists.

## Diferencias inevitables

- **No hay desenfoque real del fondo** (backdrop blur). El "cristal" se imita con transparencias,
  bordes y un reflejo superior. El fondo dinámico usa la miniatura reducida y suavizada.
- Los iconos son vectores dibujados con trazos (no SVG ni imágenes).
- Por defecto se usa el renderer por **software** (el que menos RAM usa, ~25 MB medidos).
  Para forzar OpenGL: variable de entorno `LIQUIDCAT_GL=1`.

## Compilar

Requisitos: Rust 1.88+ (`rustup`), y en Windows las *Build Tools* de Visual Studio (C++).

```powershell
cargo build --release
# resultado: target\release\liquidcat.exe
```

Atajo en Windows: doble clic a `build-windows.bat`.

**Sin instalar nada:** sube la carpeta a un repositorio de GitHub y ejecuta la acción
*Build Windows* (pestaña Actions); deja `LiquidCat.exe` como artefacto descargable
(`.github/workflows/windows.yml`).

La primera compilación es lenta (varios minutos; ~20 en una máquina de un solo núcleo).
En Linux hacen falta además `libfontconfig1-dev libxkbcommon-dev libx11-dev` (y yt-dlp/ffmpeg en el PATH).

## Regenerar assets (opcional)

Los archivos generados ya están incluidos. Si cambias textos/iconos/temas del proyecto original:

```powershell
pip install fonttools brotli cairosvg
python tools/gen.py        "ruta\LiquidCat by Kittora"
python tools/gen_themes.py "ruta\LiquidCat by Kittora"
```

## Estado de las pruebas

Verificado en Linux (escritorio virtual) con yt-dlp/ffmpeg simulados: interfaz, ajustes, wiki,
consulta de formatos, descarga + reencode, historial. **Sin probar en Windows**: sonido, notificaciones,
cancelación con `taskkill`, maximizado sobre la barra de tareas, redimensionado sin marco y la
descarga real de yt-dlp/ffmpeg.

## Estructura

```
ui/            Interfaz Slint (theme, widgets, views, modals, app)
src/main.rs    Arranque
src/ui_bridge  Conecta la interfaz con el backend (callbacks, ventana, historial)
src/jobs.rs    Formatos, descarga + reencode, archivos locales, cola
src/media.rs   Encoders y ejecución de ffmpeg con progreso/cancelación
src/deps.rs    Descarga e instalación de yt-dlp / ffmpeg
src/config.rs  config.json (compatible con la versión Python)
```
