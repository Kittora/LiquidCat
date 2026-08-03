# LiquidCat banner concept

## Direction

A premium, cinematic GitHub hero banner that combines translucent **Liquid Glass** surfaces with the deep, soft contrast of **Catppuccin Mocha**. The visual should feel refined and technical—not playful or cartoonish—even though the cat motif remains recognizable.

## Editable generation prompt

```text
Create an ultra-wide premium hero banner for a desktop multimedia application named “LiquidCat”. Use a dark Catppuccin Mocha background (#11111B) with subtle layered gradients toward #181825 and #1E1E2E. Center a sophisticated translucent liquid-glass emblem: an abstract cat silhouette formed from a flowing audio waveform, a film strip and a downward download curve. The emblem should look like sculpted glass with realistic refraction, soft internal reflections, smooth rounded edges and restrained chromatic highlights in mauve #CBA6F7, blue #89B4FA, sapphire #74C7EC, green #A6E3A1 and peach #FAB387.

Place the word “LiquidCat” in a crisp modern geometric sans-serif, large and highly legible, with the tagline “Download · Convert · Re-encode” beneath it. Add very subtle floating glass particles and one faint waveform line across the background. Use generous negative space, balanced symmetry, soft studio lighting, controlled bloom, sharp typography and a polished software-brand aesthetic. No device mockups, no interface screenshot, no busy scenery. Compose for a GitHub README header at 1600 × 480 pixels, keeping all essential content inside the central 1200 × 320 safe area.
```

## Negative prompt

```text
cartoon mascot, anime, furry character, realistic animal, childish, neon cyberpunk overload, clutter, excessive glow, illegible text, misspelled words, warped typography, harsh reflections, low contrast, white background, stock-photo look, watermark, UI mockup, laptop, phone, extra logos
```

## Layout specification

| Property | Specification |
|---|---|
| Canvas | 1600 × 480 px, 10:3 ratio |
| Safe area | Centered 1200 × 320 px |
| Background | `#11111B` → `#181825` → `#1E1E2E` |
| Primary accent | Mauve `#CBA6F7` |
| Secondary accents | Blue `#89B4FA`, Sapphire `#74C7EC` |
| Detail accents | Green `#A6E3A1`, Peach `#FAB387` |
| Main text | Text `#CDD6F4` |
| Secondary text | Subtext `#A6ADC8` |
| Typography | Geometric sans-serif; medium or semibold |
| Finish | Frosted glass, restrained refraction, subtle grain |

## Recommended layer structure

1. Mocha gradient background
2. Subtle radial color bloom
3. Faint waveform and floating particles
4. Central glass cat/media emblem
5. `LiquidCat` wordmark
6. `Download · Convert · Re-encode` tagline
7. Fine grain and final contrast adjustment

## Export

- Primary: `assets/readme/liquidcat-banner.png`
- Resolution: 1600 × 480 px
- Color space: sRGB
- Format: optimized PNG or high-quality WebP
- Keep an editable source in Figma, SVG, PSD or the generator's native project format.

For the cleanest typography, generate the emblem and background without text, then add the wordmark and tagline manually in an editor.
