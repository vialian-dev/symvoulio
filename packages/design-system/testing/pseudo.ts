// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Pseudo-locale gate helper (brief §0б). Open a page with ?lang=pseudo: every translated string is wrapped in ⟦ ⟧ by the
// i18n setup, so any visible text with words that is neither wrapped nor product data is a hard-coded string.
// Use in a Playwright spec:
//   import { bareStrings } from '@symvoulio/ui/testing/pseudo'
//   const hard = (await bareStrings(page)).filter((s) => !isData(s))
//   expect(hard).toEqual([])
// The product decides what counts as data (names, ids, dates) — only it knows its stub.
import type { Page } from '@playwright/test'

/** Visible text nodes and accessible attributes (aria-label, placeholder, title, alt) in the page and in open portals
 * (A1, A2, D1, tooltips) that carry words but no pseudo brackets. Only aria-hidden, svg, option/select (language names are
 * data), pre/code (diffs) are skipped — no whole subtree is excluded by direction. */
export async function bareStrings(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = []
    const skip = (el: Element | null) => !el || !!el.closest('[aria-hidden="true"],svg,option,select,pre,code')
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const el = n.parentElement
      if (skip(el)) continue
      const style = getComputedStyle(el!)
      if (style.display === 'none' || style.visibility === 'hidden') continue
      const s = (n.textContent ?? '').trim()
      if (!/[A-Za-z]{3,}/.test(s)) continue
      if (/⟦[\s\S]*⟧/.test(s)) continue
      out.push(s)
    }
    for (const el of document.querySelectorAll('[aria-label],[placeholder],[title],img[alt]')) {
      if (skip(el)) continue
      for (const a of ['aria-label', 'placeholder', 'title', 'alt']) {
        const v = el.getAttribute(a)
        if (v && /[A-Za-z]{3,}/.test(v) && !/⟦[\s\S]*⟧/.test(v)) out.push(`@${a}=${v}`)
      }
    }
    return out
  })
}
