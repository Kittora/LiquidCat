<div align="center">

<!-- Replace this placeholder with your final banner: assets/readme/liquidcat-banner.png -->
<img src="assets/readme/liquidcat-banner-placeholder.svg" alt="LiquidCat — Download, convert and re-encode media" width="100%">

<br>

# LiquidCat

### Download · Convert · Re-encode

**A polished, native desktop workspace for downloading and transforming multimedia—powered by yt-dlp and FFmpeg, without the usual setup friction or WebView bloat.**

<br>

[![Platform](https://img.shields.io/badge/Platform-Windows-89B4FA?style=for-the-badge&logo=windows11&logoColor=11111B)](#installation)
[![Rust](https://img.shields.io/badge/Rust-Native-000000?style=for-the-badge&logo=rust&logoColor=white)](#tech-stack)
[![Slint](https://img.shields.io/badge/UI-Slint-74C7EC?style=for-the-badge)](#tech-stack)
[![yt-dlp](https://img.shields.io/badge/Powered_by-yt--dlp-A6E3A1?style=for-the-badge&logo=youtube&logoColor=11111B)](https://github.com/yt-dlp/yt-dlp)
[![FFmpeg](https://img.shields.io/badge/Powered_by-FFmpeg-F9E2AF?style=for-the-badge&logo=ffmpeg&logoColor=11111B)](https://ffmpeg.org/)
[![Theme](https://img.shields.io/badge/Theme-Catppuccin_Mocha-CBA6F7?style=for-the-badge)](https://catppuccin.com/)

<br>

[Features](#features) · [Screenshots](#screenshots) · [Installation](#installation) · [Roadmap](#roadmap) · [Authors](#authors)

</div>

---

## Overview

LiquidCat brings media downloading, conversion and re-encoding into one focused desktop application. Paste a compatible link or select a local file, choose the result you want, and let LiquidCat handle the processing pipeline.

This version is a **complete native rewrite** from the ground up. Tauri and WebView2 have been completely removed; the interface is now drawn using **Rust and Slint**, resulting in a snappy, single-executable experience.

> [!NOTE]
> LiquidCat is lighter than the previous WebView2-based build. Expect around 60 MB at idle in Task Manager on Windows; actual usage varies with the active renderer and theme.

> [!NOTE]
> LiquidCat is designed for media you own or are authorized to download and process. You are responsible for respecting applicable copyright rules and platform terms.

## Features

<table>
<tr>
<td width="50%" valign="top">

### Download

- Download video or audio from links supported by yt-dlp
- Batch import: paste many links at once or load them from a `.txt`
- Choose format, resolution and quality
- Include subtitles, thumbnails and metadata
- Select the destination folder before processing

</td>
<td width="50%" valign="top">

### Convert

- Convert media to MP3, M4A, FLAC and other formats
- Re-encode local video and audio files (pick or drag several at once)
- Use FFmpeg with hardware acceleration (NVENC/AMF/CPU)
- Trim a specific segment without leaving the app

</td>
</tr>
<tr>
<td width="50%" valign="top">

### Organize

- Manage multiple jobs through a processing queue
- Review completed and previous jobs in history
- Track each task from preparation to completion
- Start with eight built-in presets, including **H.264 compatible** (best H.264 + AAC with no re-encode, plays everywhere)
- Keep your existing presets and history (fully compatible with the old `config.json`)

</td>
<td width="50%" valign="top">

### Finish

- Open the resulting file immediately
- Jump directly to its destination folder
- Keep the complete workflow inside one interface
- 14 built-in themes (including Catppuccin), each with its own cat icon
- Available in English, Spanish, Portuguese, French, German and Italian

</td>
</tr>
</table>

## Screenshots

<div align="center">

<!-- Screenshots captured at 1180 × 800. -->

| Downloader | Converter |
|:---:|:---:|
| <img src="assets/readme/downloader.png" alt="LiquidCat downloader" width="680"> | <img src="assets/readme/converter.png" alt="LiquidCat local file converter" width="680"> |
| Paste a link, inspect the available options and choose the output. | Convert or re-encode local files with precise control. |

| Queue & history | Output settings |
|:---:|:---:|
| <img src="assets/readme/queue-history.png" alt="LiquidCat queue and history" width="680"> | <img src="assets/readme/output-settings.png" alt="LiquidCat output settings" width="680"> |
| Follow active jobs and revisit completed work. | Configure quality, metadata, subtitles and destination. |

</div>

## Tech stack

| Component | Role |
|---|---|
| [Rust](https://www.rust-lang.org/) | Core backend, performance, and logic |
| [Slint](https://slint.dev/) | Native UI toolkit (no WebView required) |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Link extraction and media downloads |
| [FFmpeg](https://ffmpeg.org/) | Conversion, re-encoding, trimming and media processing |

## Installation

### Windows

1. Open the repository's **[Releases](https://github.com/Kittora/LiquidCat/releases)** page.
2. Download the latest `LiquidCat-0.3.0-windows-x86_64.exe`.
3. Run `LiquidCat.exe`. 

*Note: **Python is not required.** LiquidCat automatically downloads and manages yt-dlp (which bundles its own runtime) and FFmpeg/ffprobe in the background.*

> [!IMPORTANT]
> **Building from source:** You will need Rust 1.88+ installed. Simply clone the repository and run `cargo build --release`. 
> Alternatively, you can just push the code to GitHub and use the included **Build Windows** GitHub Action to get the `.exe` automatically.

## How it works

```text
Link or local file
        ↓
Choose format, quality and optional extras
        ↓
LiquidCat prepares the yt-dlp / FFmpeg job
        ↓
Track progress in the queue
        ↓
Open the finished file or its destination folder
```

## Roadmap

- [x] Link-based video and audio downloads
- [x] Format, resolution and quality selection
- [x] Local file conversion and re-encoding
- [x] Trimming, subtitles, thumbnails and metadata
- [x] Processing queue and job history
- [x] Self-contained runtime for end users
- [x] Reusable download and conversion presets
- [x] Batch import for links and local files
- [x] Additional application languages (PT, FR, DE, IT)
- [ ] Expanded accessibility and keyboard navigation

Roadmap items may change as LiquidCat evolves.

## Contributing

Issues and focused pull requests are welcome. Before proposing a large change, open an issue so the scope and expected behavior can be discussed first.

## Authors

Built by **JuanFX** and **Adaki**.

## Acknowledgements

LiquidCat is made possible by the excellent work behind [yt-dlp](https://github.com/yt-dlp/yt-dlp), [FFmpeg](https://ffmpeg.org/) and the [Catppuccin](https://catppuccin.com/) color palette.

---

<div align="center">

<sub>Crafted with glass, color and a little bit of cat energy.</sub>

</div>
