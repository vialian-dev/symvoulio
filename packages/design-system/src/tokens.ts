// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Motion and layout constants used by components in code (the CSS side lives in styles/theme.css).
/** --ease-glide as a motion/react easing. */
export const EASE_GLIDE = [0.22, 1, 0.36, 1] as const
export const MOTION = {
  /** Banner: height to the strip, picture fade out / in, strip fade-in delay (s). */
  bannerCollapse: 0.9,
  bannerFadeIn: 0.5,
  bannerFadeOut: 0.6,
  bannerStripDelay: 0.35,
  /** Buttons: transition of background, box-shadow and press (ms, Tailwind duration-150). */
  press: 150,
} as const
