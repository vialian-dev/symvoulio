// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// i18n setup shared by the products: i18next + react-i18next + i18next-http-backend (https://www.i18next.com,
// https://react.i18next.com; i18next 26.5.11, react-i18next 17.1.9, i18next-http-backend 4.0.6).
// Every visible string is a key in public/locales/<lang>/<ns>.json — no text in components. The language list is data
// too: public/locales/languages.json. Adding a language = a locale folder + one line there. Plurals are i18next's own
// (`key_one`, `key_few`, `key_other` … by Intl.PluralRules of the language); RTL is set on <html dir> from `dir` there.
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import HttpBackend from 'i18next-http-backend'

export interface LanguageInfo { code: string; native: string; script: 'latin' | 'cyrillic' | 'cjk' | 'hangul' | 'devanagari' | 'hebrew'; dir?: 'rtl'; fonts?: string; draft?: boolean }
/** Pseudo-locale for finding hard-coded strings: every translated string is wrapped in ⟦ ⟧, so a bare string stands out. */
export const PSEUDO = 'pseudo'

export interface I18nOptions {
  /** localStorage key that remembers the chosen language, e.g. 'mens.lang'. */
  storageKey: string
  /** Namespaces loaded before the first render (the frame's). A screen's own namespace loads with the screen. */
  initialNamespaces?: string[]
  /** Namespace of `t('key')` without `ns:`. */
  defaultNS?: string
  /** ISO 4217 code for fmt.money. */
  currency?: string
  /** Base URL of `locales/`; the Vite BASE_URL by default. */
  baseUrl?: string
}

let languages: LanguageInfo[] = [{ code: 'en', native: 'English', script: 'latin' }]
let options: Required<I18nOptions> = { storageKey: 'lang', initialNamespaces: ['common'], defaultNS: 'common', currency: 'USD', baseUrl: import.meta.env.BASE_URL }
export const getLanguages = () => languages
const codes = () => [...languages.map((l) => l.code), PSEUDO]

function initialLanguage(): string {
  const fromUrl = new URLSearchParams(location.search).get('lang')
  if (fromUrl && codes().includes(fromUrl)) return fromUrl
  try {
    const saved = localStorage.getItem(options.storageKey)
    if (saved && codes().includes(saved)) return saved
  } catch { /* storage blocked */ }
  return 'en'
}

function applyDocumentLanguage(lng: string) {
  const info = languages.find((l) => l.code === lng)
  document.documentElement.lang = lng === PSEUDO ? 'en' : lng
  document.documentElement.dir = info?.dir ?? 'ltr' // RTL is built in from the first component: logical CSS only
  document.documentElement.dataset.script = info?.script === 'cjk' || info?.script === 'hangul' ? 'cjk' : (info?.script ?? 'latin')
  // Fonts load only for the language in use (brief §0б).
  if (info?.fonts && !document.querySelector(`link[data-font="${lng}"]`)) {
    const link = document.createElement('link')
    link.rel = 'stylesheet'
    link.href = info.fonts
    link.dataset.font = lng
    document.head.appendChild(link)
  }
}

/** a sentence that ends with a date already ending in «.» (fr «7 nov.», uk «7 лист.») gets no second period. */
export const collapseDoubledPeriod = (value: string) => (typeof value === 'string' ? value.replace(/(?<!\.)\.\.(?!\.)/g, '.') : value)

/** Load languages.json, then i18next with the doubled-period and pseudo post-processors. Await before the first render. */
export async function initI18n(opts: I18nOptions) {
  options = { ...options, ...opts }
  try {
    const r = await fetch(`${options.baseUrl}locales/languages.json`, { cache: 'no-cache' })
    if (r.ok) languages = await r.json()
  } catch { /* keep English */ }
  await i18n
    .use(HttpBackend)
    .use(initReactI18next)
    .use({ type: 'postProcessor', name: 'dots', process: collapseDoubledPeriod })
    .use({ type: 'postProcessor', name: 'pseudo', process: (value: string) => (i18n.language === PSEUDO ? `⟦${value}⟧` : value) })
    .init({
      lng: initialLanguage(),
      fallbackLng: 'en',
      supportedLngs: codes(),
      // Only the frame namespace before the first render; a screen's namespace loads with the screen (useTranslation
      // suspends). A product may prefetch further namespaces when idle with i18n.loadNamespaces.
      ns: options.initialNamespaces,
      defaultNS: options.defaultNS,
      load: 'currentOnly',
      postProcess: ['dots', 'pseudo'],
      interpolation: { escapeValue: false },
      backend: { loadPath: `${options.baseUrl}locales/{{lng}}/{{ns}}.json` },
      react: { useSuspense: true },
    })
  applyDocumentLanguage(i18n.language)
  i18n.on('languageChanged', applyDocumentLanguage)
}

/** The one way to change the language: switcher, ?lang=, an agent's action, and tests. */
export async function setLanguage(lng: string) {
  if (!codes().includes(lng)) throw new Error(`unsupported language: ${lng}`)
  try { localStorage.setItem(options.storageKey, lng) } catch { /* storage blocked */ }
  const url = new URL(location.href)
  if (url.searchParams.has('lang')) { url.searchParams.set('lang', lng); history.replaceState(null, '', url) }
  await i18n.changeLanguage(lng)
}

/** Intl formatting by the current language — numbers, money, percent, dates (brief §0б). Formatters are cached per
 * language: building an Intl object costs ~0.1 ms, which a 10 000-row list pays on every scroll frame otherwise. */
const cache = new Map<string, Intl.NumberFormat | Intl.DateTimeFormat>()
function cached<T extends Intl.NumberFormat | Intl.DateTimeFormat>(kind: string, make: (loc: string) => T): T {
  const key = `${kind}|${fmt.locale()}`
  let f = cache.get(key) as T | undefined
  if (!f) { f = make(fmt.locale()); cache.set(key, f) }
  return f
}
export const fmt = {
  locale: () => (i18n.language === PSEUDO ? 'en' : i18n.language),
  num: (n: number, digits = 0) => cached(`num${digits}`, (l) => new Intl.NumberFormat(l, { maximumFractionDigits: digits })).format(n),
  money: (n: number) => cached('money', (l) => new Intl.NumberFormat(l, { style: 'currency', currency: options.currency, maximumFractionDigits: 0 })).format(n),
  pct: (n: number) => cached('pct', (l) => new Intl.NumberFormat(l, { style: 'percent', maximumFractionDigits: 0 })).format(n),
  date: (iso: string) => cached('date', (l) => new Intl.DateTimeFormat(l, { month: 'short', day: 'numeric', timeZone: 'UTC' })).format(new Date(iso)),
  dateTime: (iso: string) => cached('dateTime', (l) => new Intl.DateTimeFormat(l, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', timeZone: 'UTC' })).format(new Date(iso)),
}

export { i18n }
export default i18n
