// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Collapsing page banner of the shared design system. Banner pictures and slogans belong to the product.
// Banners are data, not code: files in <base>/ listed by <base>/manifest.json, read at run time (default base:
// public/banners/ of the product). Requires a @tanstack/react-query QueryClientProvider above it.
// The manifest also says which screen shows which banner (`_places`) and how often (`_settings`), so a place, a picture or the
// period changes without a rebuild. Alt text, slogans and labels are English only.
//
// Behaviour: on the first show of a page the banner is full (form «hero above the title»), then it
// collapses with a short animation into a thin strip with only its slogan. Full again at most once per
// `_settings.full_again_after_days` per page (one number: 7 = a week, 30 = a month). A click (or Enter / Space) on the strip
// expands it, another click collapses it. Reduced motion: no animation, the strip at once. No storage: the strip.
// No stretching: the picture frame takes the picture's own ratio (or of its manifest crop); maxH narrows, never squeezes.
import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { motion, useReducedMotion } from 'motion/react'
import { ChevronDown } from 'lucide-react'
import { cx } from '../cx'
import { EASE_GLIDE, MOTION } from '../tokens'

export type Variant = 'clean_github' | 'clean_linkedin' | 'landscape_github' | 'landscape_linkedin'
type Rect = [number, number, number, number]
type Entry = Partial<Record<Variant, string>> & { alt?: string; slogan?: string; crop?: Partial<Record<Variant, Rect>> }
type Settings = { full_again_after_days?: number; collapse_after_ms?: number; label_expand?: string; label_collapse?: string }
type Place = { id: string; variant?: Variant }
export type Manifest = Record<string, Entry> & { _settings?: Settings; _places?: Record<string, Place> }

const defaultBase = `${import.meta.env.BASE_URL}banners/`
const DAY = 86_400_000
export const STRIP_H = 44
/** localStorage key of the last full show of a page's banner (the 7-day rule). */
export const bannerStoreKey = (page: string) => `mz.banner.lastFull.${page}`

export function useBannerManifest(base = defaultBase) {
  return useQuery({
    queryKey: ['banner-manifest', base],
    queryFn: async (): Promise<Manifest> => {
      const r = await fetch(`${base}manifest.json`, { cache: 'no-cache' })
      return r.ok ? r.json() : {}
    },
    staleTime: 60_000,
  })
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
export function PageBanner({ screen, base = defaultBase }: { screen: string; base?: string }) {
  const { data } = useBannerManifest(base)
  const place = data?._places?.[screen]
  if (!data || !place || !data[place.id]) return null
  return <CollapsibleBanner key={screen} base={base} page={screen} id={place.id} variant={place.variant ?? 'clean_linkedin'} entry={data[place.id]} settings={data._settings ?? {}} />
}

function CollapsibleBanner({ base, page, id, variant, entry, settings }: { base: string; page: string; id: string; variant: Variant; entry: Entry; settings: Settings }) {
  const still = useReducedMotion()
  const days = settings.full_again_after_days ?? 7
  const delay = settings.collapse_after_ms ?? 3500
  const [open, setOpen] = useState(() => !still && dueFull(page, days))
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
    <button type="button" data-banner={id} data-banner-page={page} data-state={open ? 'full' : 'strip'} aria-expanded={open}
      aria-label={`${slogan} — ${open ? settings.label_collapse ?? 'Collapse' : settings.label_expand ?? 'Expand'}`}
      onClick={toggle}
      className="group relative mb-5 block w-full cursor-pointer overflow-hidden rounded-[var(--radius-card)] border border-[var(--line)] text-start outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent)] focus-visible:ring-offset-2"
      style={{ background: 'linear-gradient(120deg, var(--surface) 0%, var(--accent-soft) 60%, var(--value-soft) 100%)' }}>
      <motion.div initial={false} animate={{ height: open ? 'auto' : STRIP_H }} transition={still ? { duration: 0 } : { duration: MOTION.bannerCollapse, ease }}
        className="overflow-hidden">
        <motion.div initial={false} animate={{ opacity: open ? 1 : 0 }} transition={still ? { duration: 0 } : { duration: open ? MOTION.bannerFadeIn : MOTION.bannerFadeOut, ease }}>
          <Picture base={base} id={id} variant={variant} entry={entry} maxH={300} />
        </motion.div>
      </motion.div>
      <motion.div aria-hidden initial={false} animate={{ opacity: open ? 0 : 1 }} transition={still ? { duration: 0 } : { duration: MOTION.bannerFadeIn, delay: open ? 0 : MOTION.bannerStripDelay }}
        data-banner-strip className="pointer-events-none absolute inset-x-0 top-0 flex items-center justify-center gap-2 px-4" style={{ height: STRIP_H }}>
        {/* English slogan keeps its own direction in RTL; on a narrow screen it wraps to two lines instead of being cut. */}
        <span dir="ltr" className="serif-title truncate text-[17px] leading-none text-[var(--ink)] max-lg:line-clamp-2 max-lg:whitespace-normal max-lg:text-center max-lg:text-[13px] max-lg:leading-tight"><bdi>{slogan}</bdi></span>
        <ChevronDown size={15} className="shrink-0 text-[var(--accent)]" />
      </motion.div>
    </button>
  )
}

/**
 * Side margins of the full banner. Per side: the outermost STRIP of the picture (6 % of its
 * width) is averaged in BINS vertical bins; the bins become a soft vertical gradient (one colour per row, no detail can
 * survive), which fades horizontally into the page background across the margin, and feathers FEATHER px over the picture
 * edge so there is no seam. The picture itself is never scaled.
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
        // A stop at the bin centre: the browser interpolates smoothly between centres (and holds the ends).
        stops.push(`rgb(${Math.round(r / n)} ${Math.round(gg / n)} ${Math.round(bl / n)}) ${(((b + 0.5) / BINS) * 100).toFixed(1)}%`)
      }
      return `linear-gradient(to bottom, ${stops.join(', ')})`
    }
    return { left: side(sx0), right: side(sx1 - sw) }
  } catch {
    return null // a tainted or broken image keeps the default background
  }
}

/** The picture at its own scale on both axes; the frame clips only the manifest crop. */
function Picture({ base, id, variant, entry, maxH }: { base: string; id: string; variant: Variant; entry: Entry; maxH?: number }) {
  const file = entry[variant]
  const [strips, setStrips] = useState<{ file: string; left: string; right: string } | null>(null)
  const margin = strips && strips.file === file ? strips : null
  const [natural, setNatural] = useState<{ file: string; w: number; h: number } | null>(null)
  const [brokenFile, setBrokenFile] = useState<string | null>(null)
  const wide = variant.endsWith('linkedin')
  const [x0, y0, x1, y1] = entry.crop?.[variant] ?? [0, 0, 1, 1]
  const known = natural && natural.file === file
  const ratio = ((known ? natural.w : wide ? 4 : 2) * (x1 - x0)) / ((known ? natural.h : 1) * (y1 - y0))
  const frame = { aspectRatio: `${ratio}`, maxWidth: maxH ? `${Math.round(maxH * ratio)}px` : undefined }
  if (!file || brokenFile === file) {
    // Series-style stub: gradient + slogan as text, never an empty hole or an error.
    return (
      <div data-banner-stub="1" style={frame} className="relative mx-auto grid w-full place-items-center">
        <span className="serif-title px-4 text-center text-[22px]">{entry.slogan ?? id}</span>
      </div>
    )
  }
  // Physical left/top on purpose: the picture is not mirrored in RTL, so its crop is not either.
  // The edge gradient, fading toward the page background (light or dark theme) away from the picture.
  const fade = (to: 'right' | 'left') => `linear-gradient(to ${to}, #000 0%, transparent 100%)`
  // Full colour for the first 240 px next to the picture, then fading into the page on wide screens.
  const margin_ = (grad: string, away: 'left' | 'right') => ({ backgroundColor: 'var(--bg)' as const, grad, mask: `linear-gradient(to ${away}, #000 0, #000 240px, transparent 100%)` })
  return (
    <div data-banner-margins={margin ? 'edge' : undefined} style={frame} className="relative mx-auto w-full">
      {margin && <>
        {([['left', margin.left], ['right', margin.right]] as const).map(([edge, grad]) => {
          const m = margin_(grad, edge)
          return (
            <div key={edge} aria-hidden data-banner-margin={edge} style={{ backgroundColor: m.backgroundColor }}
              className={cx('pointer-events-none absolute inset-y-0 w-[50vw]', edge === 'left' ? 'right-full' : 'left-full')}>
              <div className="absolute inset-0" style={{ backgroundImage: m.grad, maskImage: m.mask, WebkitMaskImage: m.mask }} />
            </div>
          )
        })}
      </>}
    <div data-banner-frame style={{ aspectRatio: frame.aspectRatio }} className="relative w-full overflow-hidden">
      <img src={`${base}${file}`} alt={entry.alt ?? ''} draggable={false}
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
    </div>
    </div>
  )
}
