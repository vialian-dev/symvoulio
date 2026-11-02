// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Self-hosted CJK fallback: Noto Sans SC / JP / KR, sliced by unicode-range — a browser downloads only the
// slices of the glyphs on the page, and ideographs render on a machine without system CJK fonts (language names in an
// English UI too). Import this module once, BEFORE the product's CSS entry, so the @font-face rules precede the theme:
//   import '@symvoulio/ui/fonts'
//   import './index.css'
// The Latin families (Cormorant SC for titles, Inter for text) come from Google Fonts by a <link> in index.html — see
// README «Fonts»; per-language families (Noto Serif SC, Noto Sans Hebrew …) load on demand from `fonts` in languages.json.
import '@fontsource-variable/noto-sans-sc/wght.css'
import '@fontsource-variable/noto-sans-jp/wght.css'
import '@fontsource-variable/noto-sans-kr/wght.css'

/** The <link> tags the product's index.html must carry for the Latin families (one copy of the URL for every product). */
export const LATIN_FONTS_HREF =
  'https://fonts.googleapis.com/css2?family=Cormorant+SC:wght@500;600;700&family=Inter:wght@400;500;600&display=swap'
