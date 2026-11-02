// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The one i18n resource of v0.1 (map S12: English only): every UI string lives in ./en.json. i18next is the same
// singleton the design system's `fmt` reads. A missing key is a console error, so the console gate catches it.
import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import en from './en.json'

export function initI18n() {
  return i18n.use(initReactI18next).init({
    lng: 'en',
    fallbackLng: false,
    resources: { en: { common: en } },
    defaultNS: 'common',
    interpolation: { escapeValue: false },
    saveMissing: true,
    missingKeyHandler: (_l, _ns, key) => console.error(`i18n: missing key «${key}»`),
    react: { useSuspense: false },
  })
}
export { i18n }
