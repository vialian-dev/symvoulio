// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The collapsing page banner of the family, ported from the design system (packages/design-system/src/banner/Banner.tsx,
// source commit 3083a69) so Symvoulio needs no query library. Same mechanism, same numbers, same storage key:
// - banners are data: public/banners/manifest.json lists the pictures, `_places` says which screen shows which banner and
//   `_settings` holds `full_again_after_days` (7) and `collapse_after_ms` (3500);
// - on the first show of a page the banner is full, then it collapses into a 44 px strip with only its slogan; full again
//   at most once per 7 days per page; a click (or Enter / Space) expands it, another click collapses it; reduced motion
//   and no storage give the strip at once; nothing is stretched; the side margins fade calmly into the page.
// Two additions, both data-driven and proposed upstream:
// - `caption` of an entry: the slogan is HTML over the picture (Symvoulio pictures carry no letters);
// - `start: "strip"` of a place: the banner starts as the strip (S7 and S9 are working screens), a click still expands it.
import { useEffect, useState } from 'react'
import { motion, useReducedMotion } from 'motion/react'
import { ChevronDown } from 'lucide-react'
import { cx } from '@symvoulio/ui/components'
import { EASE_GLIDE, MOTION } from '@symvoulio/ui/tokens'

type Rect = [number, number, number, number]
interface Caption { kicker?: string; title: string; accent?: string }
interface Entry { file?: string; alt?: string; slogan?: string; crop?: Rect; caption?: Caption }
interface Settings { full_again_after_days?: number; collapse_after_ms?: number; label_expand?: string; label_collapse?: string }
interface Place { id: string; start?: 'full' | 'strip' }
type Manifest = Record<string, Entry> & { _settings?: Settings; _places?: Record<string, Place> }

const BASE = `${import.meta.env.BASE_URL}banners/`
const DAY = 86_400_000
export const STRIP_H = 44
/** localStorage key of the last full show of a page's banner (the family key, so the 7-day rule is the same). */
export const bannerStoreKey = (page: string) => `mz.banner.lastFull.${page}`

let cache: Promise<Manifest> | null = null
function loadManifest(): Promise<Manifest> {
  cache ??= fetch(`${BASE}manifest.json`, { cache: 'no-cache' }).then((r) => (r.ok ? r.json() : {})).catch(() => ({}))
  return cache
}
function useManifest() {
  const [m, setM] = useState<Manifest | null>(null)
  useEffect(() => { let live = true; void loadManifest().then((x) => { if (live) setM(x) }); return () => { live = false } }, [])
  return m
}

/** Pure read (safe under StrictMode double calls): full when this page has no full show within `days`. No storage: strip. */
function dueFull(page: string, days: number): boolean {
  try {
    const last = Number(window.localStorage.getItem(bannerStoreKey(page)) ?? 0)
    return !last || Date.now() - last >= days * DAY
  } catch {
    return false
  }
}
function rememberFull(page: string) {
  try { window.localStorage.setItem(bannerStoreKey(page), String(Date.now())) } catch { /* no storage: next time the strip */ }
}

/** The banner of a screen, from `_places` of the manifest; nothing when the screen has no place. */
export function PageBanner({ screen }: { screen: string }) {
  const data = useManifest()
  const place = data?._places?.[screen]
  if (!data || !place || !data[place.id]) return null
  return <CollapsibleBanner key={screen} page={screen} id={place.id} entry={data[place.id]} settings={data._settings ?? {}} strip={place.start === 'strip'} />
}

function CollapsibleBanner({ page, id, entry, settings, strip }: { page: string; id: string; entry: Entry; settings: Settings; strip: boolean }) {
  const still = useReducedMotion()
  const days = settings.full_again_after_days ?? 7
  const delay = settings.collapse_after_ms ?? 3500
  const [open, setOpen] = useState(() => !still && !strip && dueFull(page, days))
  const [auto, setAuto] = useState(open) // the first-show timer runs only until the user acts
  useEffect(() => { if (auto) rememberFull(page) }, [auto, page])
  useEffect(() => {
    if (!auto) return
    const t = window.setTimeout(() => { setOpen(false); setAuto(false) }, delay)
    return () => window.clearTimeout(t)
  }, [auto, delay])
  const toggle = () => { setAuto(false); setOpen((o) => !o) }
  const ease = EASE_GLIDE
  const slogan = entry.slogan ?? entry.alt ?? id
  return (
    <button type="button" data-banner={id} data-banner-page={page} data-state={open ? 'full' : 'strip'} aria-expanded={open} data-testid="banner"
      aria-label={`${slogan} — ${open ? settings.label_collapse ?? 'Collapse' : settings.label_expand ?? 'Expand'}`}
      onClick={toggle}
      className="group relative mb-5 block w-full cursor-pointer overflow-hidden rounded-[var(--radius-card)] border border-[var(--line)] text-start outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2"
      style={{ background: 'linear-gradient(120deg, var(--surface) 0%, var(--accent-soft) 60%, var(--value-soft) 100%)' }}>
      <motion.div initial={false} animate={{ height: open ? 'auto' : STRIP_H }} transition={still ? { duration: 0 } : { duration: MOTION.bannerCollapse, ease }}
        className="overflow-hidden">
        <motion.div initial={false} animate={{ opacity: open ? 1 : 0 }} transition={still ? { duration: 0 } : { duration: open ? MOTION.bannerFadeIn : MOTION.bannerFadeOut, ease }}>
          <Picture id={id} entry={entry} maxH={300} />
        </motion.div>
      </motion.div>
      <motion.div aria-hidden initial={false} animate={{ opacity: open ? 0 : 1 }} transition={still ? { duration: 0 } : { duration: MOTION.bannerFadeIn, delay: open ? 0 : MOTION.bannerStripDelay }}
        data-banner-strip className="pointer-events-none absolute inset-x-0 top-0 flex items-center justify-center gap-2 px-4" style={{ height: STRIP_H }}>
        <span dir="ltr" className="serif-title truncate text-[17px] leading-none text-[var(--ink)]"><bdi>{slogan}</bdi></span>
        <ChevronDown size={15} className="shrink-0 text-[var(--accent)]" />
      </motion.div>
    </button>
  )
}

/**
 * Side margins of the full banner: the outermost 6 % strip of each side is averaged in 6 vertical bins;
 * the bins become a soft vertical gradient that fades horizontally into the page background and feathers 24 px over the
 * picture edge, so there is no seam. The picture itself is never scaled.
 */
const STRIP = 0.06
const BINS = 6
export const FEATHER = 24
function edgeGradients(img: HTMLImageElement, [x0, y0, x1, y1]: Rect): { left: string; right: string } | null {
  try {
    const W = img.naturalWidth, H = img.naturalHeight
    const sx0 = Math.floor(x0 * W), sx1 = Math.ceil(x1 * W), sy = Math.floor(y0 * H), sh = Math.max(BINS, Math.floor((y1 - y0) * H))
    const sw = Math.max(2, Math.round((sx1 - sx0) * STRIP))
    const c = document.createElement('canvas')
    c.width = sw; c.height = sh
    const g = c.getContext('2d', { willReadFrequently: true })
    if (!g) return null
    const side = (fromX: number) => {
      g.clearRect(0, 0, sw, sh)
      g.drawImage(img, fromX, sy, sw, sh, 0, 0, sw, sh)
      const d = g.getImageData(0, 0, sw, sh).data
      const stops: string[] = []
      for (let b = 0; b < BINS; b++) {
        const from = Math.floor((b * sh) / BINS), to = Math.floor(((b + 1) * sh) / BINS)
        let r = 0, gg = 0, bl = 0, n = 0
        for (let y = from; y < to; y++) for (let x = 0; x < sw; x++) {
          const k = (y * sw + x) * 4
          r += d[k]; gg += d[k + 1]; bl += d[k + 2]; n++
        }
        stops.push(`rgb(${Math.round(r / n)} ${Math.round(gg / n)} ${Math.round(bl / n)}) ${(((b + 0.5) / BINS) * 100).toFixed(1)}%`)
      }
      return `linear-gradient(to bottom, ${stops.join(', ')})`
    }
    return { left: side(sx0), right: side(sx1 - sw) }
  } catch {
    return null // a broken image keeps the default background
  }
}

/** The picture at its own scale on both axes; the frame clips only the manifest crop; the caption is HTML over its sky. */
function Picture({ id, entry, maxH }: { id: string; entry: Entry; maxH: number }) {
  const file = entry.file
  const [strips, setStrips] = useState<{ file: string; left: string; right: string } | null>(null)
  const margin = strips && strips.file === file ? strips : null
  const [natural, setNatural] = useState<{ file: string; w: number; h: number } | null>(null)
  const [brokenFile, setBrokenFile] = useState<string | null>(null)
  const [x0, y0, x1, y1] = entry.crop ?? [0, 0, 1, 1]
  const known = natural && natural.file === file
  // Until the picture has loaded, hold the place at the series' 4:1 (the shown, cropped ratio), so it does not jump.
  const ratio = known ? (natural.w * (x1 - x0)) / (natural.h * (y1 - y0)) : 4
  const frame = { aspectRatio: `${ratio}`, maxWidth: `${Math.round(maxH * ratio)}px` }
  const caption = entry.caption
  if (!file || brokenFile === file) {
    return (
      <div data-banner-stub="1" style={frame} className="relative mx-auto grid w-full place-items-center">
        <span className="serif-title px-4 text-center text-[22px]">{entry.slogan ?? id}</span>
      </div>
    )
  }
  const fade = (to: 'right' | 'left') => `linear-gradient(to ${to}, #000 0%, transparent 100%)`
  const mask = (away: 'left' | 'right') => `linear-gradient(to ${away}, #000 0, #000 240px, transparent 100%)`
  return (
    <div data-banner-margins={margin ? 'edge' : undefined} style={frame} className="relative mx-auto w-full">
      {margin && ([['left', margin.left], ['right', margin.right]] as const).map(([edge, grad]) => (
        <div key={edge} aria-hidden data-banner-margin={edge} style={{ backgroundColor: 'var(--bg)' }}
          className={cx('pointer-events-none absolute inset-y-0 w-[50vw]', edge === 'left' ? 'right-full' : 'left-full')}>
          <div className="absolute inset-0" style={{ backgroundImage: grad, maskImage: mask(edge), WebkitMaskImage: mask(edge) }} />
        </div>
      ))}
      <div data-banner-frame style={{ aspectRatio: frame.aspectRatio, containerType: 'inline-size' }} className="relative w-full overflow-hidden">
        <img src={`${BASE}${file}`} alt={entry.alt ?? ''} draggable={false}
          onError={() => setBrokenFile(file)}
          onLoad={(e) => {
            const img = e.currentTarget
            setNatural({ file, w: img.naturalWidth, h: img.naturalHeight })
            const m = edgeGradients(img, [x0, y0, x1, y1])
            if (m) setStrips({ file, ...m })
          }}
          className="absolute block max-w-none select-none"
          style={{ width: `${100 / (x1 - x0)}%`, height: `${100 / (y1 - y0)}%`, left: `${(-x0 / (x1 - x0)) * 100}%`, top: `${(-y0 / (y1 - y0)) * 100}%` }} />
        {margin && <>
          <div aria-hidden data-banner-feather="left" className="pointer-events-none absolute inset-y-0 left-0" style={{ backgroundImage: margin.left, width: FEATHER, maskImage: fade('right'), WebkitMaskImage: fade('right') }} />
          <div aria-hidden data-banner-feather="right" className="pointer-events-none absolute inset-y-0 right-0" style={{ backgroundImage: margin.right, width: FEATHER, maskImage: fade('left'), WebkitMaskImage: fade('left') }} />
        </>}
        {caption && (
          // The pictures are light in both themes, so the caption keeps the series' own navy and cobalt.
          <div data-banner-caption aria-hidden dir="ltr" className="pointer-events-none absolute inset-x-0 top-[6%] flex flex-col items-center px-[6%] text-center">
            {caption.kicker && <span className="font-medium uppercase" style={{ color: '#0b1f44', letterSpacing: '0.28em', fontSize: 'clamp(8px, 1.15cqw, 14px)' }}>{caption.kicker}</span>}
            <span className="serif-title font-bold leading-[1.05]" style={{ color: '#0b1f44', fontSize: 'clamp(16px, 3.5cqw, 42px)', marginTop: '0.2em' }}>
              {caption.title}{caption.accent ? <> <span style={{ color: '#1f5fe0' }}>{caption.accent}</span></> : null}
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
