// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Drop C gates: the the shared UI kit style (light by default, the theme toggle), the family banners (full once, then the strip,
// a click toggles, the 7-day memory), «© 2026 Vialian», the Figma carry-overs and one test per eyes defect …014.
import { test, expect, FAST, setupDone, noHorizontalScroll, term } from './fixtures'
import type { Page } from '@playwright/test'

/** Mark every banner place as already shown in full, so a page opens with its strip (the 7-day rule). */
async function bannersSeen(page: Page) {
  await page.addInitScript(() => { for (const k of ['S11', 'S7', 'S9']) localStorage.setItem(`mz.banner.lastFull.${k}`, String(Date.now())) })
}
const box = async (page: Page, sel: string) => (await page.locator(sel).first().boundingBox())!

test.describe('Theme', () => {
  test('light by default; the toggle in the top bar switches to dark and is remembered', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/reviews?${FAST}`)
    await expect(page.locator('html')).not.toHaveClass(/dark/)
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor)
    const toggle = page.getByTestId('titlebar').getByTestId('theme-toggle')
    await expect(toggle).toHaveAttribute('aria-pressed', 'false')
    // In the place of the shared UI kit: the last control at the end of the top bar.
    const t = await toggle.boundingBox(), bar = await page.getByTestId('titlebar').boundingBox()
    expect(bar!.x + bar!.width - (t!.x + t!.width)).toBeLessThan(40)
    await toggle.click()
    await expect(page.locator('html')).toHaveClass(/dark/)
    await expect(toggle).toHaveAttribute('aria-pressed', 'true')
    expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).not.toBe(bg)
    await page.reload()
    await expect(page.locator('html')).toHaveClass(/dark/)
    await page.getByTestId('theme-toggle').click()
    await expect(page.locator('html')).not.toHaveClass(/dark/)
    await page.reload()
    await expect(page.locator('html')).not.toHaveClass(/dark/)
  })

  test('the terminals follow the theme', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/review/sample?${FAST}`)
    const termBg = () => page.getByTestId('term-claude').locator('.xterm-viewport').evaluate((e) => getComputedStyle(e).backgroundColor)
    const light = await termBg()
    await page.getByTestId('theme-toggle').click()
    await expect.poll(termBg).not.toBe(light)
  })
})

test.describe('Banners', () => {
  test('S11: full on the first show, the slogan strip after 3.5 s, a click toggles, remembered for 7 days', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/reviews?${FAST}`)
    const banner = page.getByTestId('banner')
    await expect(banner).toHaveAttribute('data-banner', 'SY01')
    await expect(banner).toHaveAttribute('data-state', 'full')
    await expect(banner.locator('img')).toBeVisible()
    await expect(banner.locator('[data-banner-caption]')).toContainText('Verify. Then approve.')
    await expect(banner.locator('[data-banner-caption]')).toContainText('No model grades its own work')
    // The picture keeps its own ratio (no stretching) and the side margins fade into the page.
    // Full-bleed at the the shared UI kit hero aspect (about 4:1, the manifest crop), filling its card edge to edge — once loaded.
    await expect.poll(() => banner.locator('img').evaluate((i) => (i as HTMLImageElement).naturalWidth), { timeout: 10_000 }).toBeGreaterThan(0)
    const ratio = await banner.locator('[data-banner-frame]').evaluate((e) => e.getBoundingClientRect().width / e.getBoundingClientRect().height)
    expect(Math.abs(ratio - 1600 / (600 * 0.66))).toBeLessThan(0.05)
    const frameW = await banner.locator('[data-banner-frame]').evaluate((e) => e.getBoundingClientRect().width)
    const cardW = await banner.evaluate((e) => e.clientWidth)
    expect(cardW - frameW).toBeLessThanOrEqual(2)
    await expect(banner.locator('[data-banner-margin="left"]')).toHaveCount(1)
    await expect(banner).toHaveAttribute('data-state', 'strip', { timeout: 6000 })
    await expect(banner).toHaveAttribute('aria-label', /No model grades its own work\. Verify\. Then approve\./)
    expect(await page.evaluate(() => localStorage.getItem('mz.banner.lastFull.S11'))).toMatch(/^\d+$/)
    await banner.click()
    await expect(banner).toHaveAttribute('data-state', 'full')
    await banner.press('Enter')
    await expect(banner).toHaveAttribute('data-state', 'strip')
    // Within 7 days the page opens with the strip.
    await page.reload()
    await expect(page.getByTestId('banner')).toHaveAttribute('data-state', 'strip')
    // After 7 days the full banner comes back once.
    await page.evaluate(() => localStorage.setItem('mz.banner.lastFull.S11', String(Date.now() - 8 * 86_400_000)))
    await page.reload()
    await expect(page.getByTestId('banner')).toHaveAttribute('data-state', 'full')
  })

  test('S7 and S9 start as strips and expand on a click; other screens have no banner', async ({ page }) => {
    await setupDone(page)
    for (const [path, id] of [['/review/sample', 'SY02'], ['/review/sample/decision', 'SY03']] as const) {
      await page.goto(`${path}?${FAST}`)
      const b = page.getByTestId('banner')
      await expect(b).toHaveAttribute('data-banner', id)
      await expect(b).toHaveAttribute('data-state', 'strip')
      await b.click()
      await expect(b).toHaveAttribute('data-state', 'full')
      await expect(b.locator('img')).toBeVisible()
    }
    for (const path of ['/new', '/settings', '/about', '/review/sample/export']) {
      await page.goto(`${path}?${FAST}`)
      await page.locator('[data-screen]').first().waitFor()
      await expect(page.getByTestId('banner')).toHaveCount(0)
    }
  })

  test('no storage: the strip at once', async ({ page }) => {
    await setupDone(page)
    await page.addInitScript(() => {
      const get = Storage.prototype.getItem
      Storage.prototype.getItem = function (this: Storage, k: string) { if (k.startsWith('mz.banner')) throw new Error('blocked'); return get.call(this, k) }
    })
    await page.goto(`/reviews?${FAST}`)
    await expect(page.getByTestId('banner')).toHaveAttribute('data-state', 'strip')
  })
})

test.describe('Copyright', () => {
  test('«© 2026 Vialian» on S13 and in the frame', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/about?${FAST}`)
    await expect(page.getByTestId('copyright')).toHaveText('© 2026 Vialian · MIT License')
    await expect(page.getByTestId('copyright-footer')).toContainText('© 2026 Vialian')
    await expect(page.getByTestId('copyright-frame')).toHaveText('© 2026 Vialian')
    await page.goto(`/reviews?${FAST}`)
    await expect(page.getByTestId('copyright-frame')).toBeVisible()
  })
})

test.describe('Widths (drop C)', () => {
  for (const [width, height] of [[1280, 800], [1920, 1080]] as const) {
    test(`no page-level horizontal scroll at ${width}, light and dark, banners full`, async ({ page }) => {
      await page.setViewportSize({ width, height })
      await setupDone(page)
      for (const theme of ['light', 'dark']) {
        for (const path of ['/reviews', '/new', '/review/sample', '/review/board-pack', '/review/sample/decision', '/review/board-pack/decision', '/review/sample/export', '/settings', '/about', '/setup/policy?from=settings']) {
          await page.goto(`${path}${path.includes('?') ? '&' : '?'}${FAST}${theme === 'dark' ? '&theme=dark' : ''}`)
          await page.locator('[data-screen]').first().waitFor()
          const b = page.getByTestId('banner')
          if (await b.count() && (await b.getAttribute('data-state')) === 'strip') await b.click()
          await page.waitForTimeout(400)
          await noHorizontalScroll(page, `${path} ${theme}`)
        }
      }
    })
  }
})

test.describe('Layout and interaction regressions', () => {
  test('S11 at 1280 — every column and action inside the card, one-line dates', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/reviews?${FAST}`)
    const card = await page.getByTestId('runs').evaluate((t) => t.parentElement!.getBoundingClientRect().right)
    const table = page.getByTestId('runs')
    expect(await table.evaluate((t) => t.parentElement!.scrollWidth - t.parentElement!.clientWidth)).toBe(0)
    for (const id of ['open', 'restart', 'delete']) {
      for (const b of await table.getByTestId(id).all()) {
        const r = (await b.boundingBox())!
        expect(r.x + r.width, id).toBeLessThanOrEqual(card)
      }
    }
    for (const d of await table.getByTestId('run-date').all()) {
      expect(await d.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true)
      const lh = await d.evaluate((e) => parseFloat(getComputedStyle(e).lineHeight))
      expect(await d.evaluate((e) => { const rg = document.createRange(); rg.selectNodeContents(e); return rg.getBoundingClientRect().height })).toBeLessThanOrEqual(lh + 2)
      expect(await d.evaluate((e) => getComputedStyle(e).whiteSpace)).toBe('nowrap')
    }
    // Roles take at most two lines.
    for (const r of await table.getByTestId('run-roles').all()) {
      const lh = await r.evaluate((e) => parseFloat(getComputedStyle(e).lineHeight))
      const h = await r.evaluate((e) => { const rg = document.createRange(); rg.selectNodeContents(e); return rg.getBoundingClientRect().height })
      expect(h).toBeLessThanOrEqual(lh * 2 + 2)
    }
  })

  test('«Pause» (disabled) on a run that is not paused; «Resume» only when paused', async ({ page }) => {
    await setupDone(page)
    for (const id of ['sample', 'board-pack', 'investment-memo']) {
      await page.goto(`/review/${id}?${FAST}`)
      await expect(page.getByTestId('pause')).toBeDisabled()
      await expect(page.getByTestId('resume')).toHaveCount(0)
    }
    await page.goto(`/review/operations-annex?${FAST}`)
    await expect(page.getByTestId('resume')).toBeEnabled()
  })

  test('the document note follows the step; the reviewed revision is shown', async ({ page }) => {
    await setupDone(page)
    await page.goto('/review/operations-annex?speed=3')
    await page.getByTestId('resume').click()
    await expect(page.getByTestId('doc-note')).toContainText('Claude Code is writing revision 4. Revision 3 is shown.')
    await expect(page.getByTestId('pause')).toBeEnabled()
    await expect(page.getByTestId('panel-codex')).toHaveAttribute('data-status', 'working', { timeout: 20_000 })
    await page.getByTestId('pause').click()
    await expect(page.getByTestId('doc-note')).toContainText('Codex is reviewing revision 4.')
    await expect(page.getByTestId('doc-current-rev')).toHaveText('Revision 4')
  })

  test('a run of another file says that the prototype shows the sample document', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/review/board-pack?${FAST}`)
    await expect(page.getByTestId('stub-note')).toContainText('northwind_board_pack.docx is not read yet')
    await page.goto(`/review/sample?${FAST}`)
    await expect(page.getByTestId('stub-note')).toHaveCount(0)
  })

  test('a stopped run is «Review stopped» with «Stopped: human review required»; still not an approval', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/review/board-pack/decision?${FAST}`)
    await expect(page.getByTestId('screen-title')).toHaveText('Review stopped')
    await expect(page.getByTestId('k-threshold')).toContainText('Failed')
    await expect(page.getByTestId('reviewer-status')).toHaveText('Stopped: human review required')
    await expect(page.getByTestId('not-approval')).toContainText('This is not an approval.')
    await expect(page.getByText('Recommended for human review')).toHaveCount(0)
    await expect(page.getByText('Review completed')).toHaveCount(0)
    await page.goto(`/review/sample/decision?${FAST}`)
    await expect(page.getByTestId('screen-title')).toHaveText('Review completed')
    await expect(page.getByTestId('reviewer-status')).toHaveText('Recommended for human review')
    await expect(page.getByTestId('not-approval')).toBeVisible()
  })

  test('S9 at 1280 — the reviewer status stays inside its card padding', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page)
    for (const id of ['sample', 'board-pack']) {
      await page.goto(`/review/${id}/decision?${FAST}`)
      const card = (await page.getByTestId('k-reviewer').locator('> *').first().boundingBox())!
      const text = await page.getByTestId('reviewer-status').evaluate((e) => { const rg = document.createRange(); rg.selectNodeContents(e); return rg.getBoundingClientRect().right })
      expect(card.x + card.width - text).toBeGreaterThanOrEqual(12)
    }
  })

  test('S4 cards on one baseline and equal height; the selected segment is filled', async ({ page }) => {
    await setupDone(page)
    await page.goto(`/setup/roles?${FAST}`)
    const a = (await page.getByTestId('s4-author').boundingBox())!, r = (await page.getByTestId('s4-reviewer').boundingBox())!
    expect(Math.abs(a.y - r.y)).toBeLessThanOrEqual(1)
    expect(Math.abs(a.height - r.height)).toBeLessThanOrEqual(1)
    const bg = (id: string) => page.getByTestId(id).evaluate((e) => getComputedStyle(e).backgroundColor)
    const on = await bg('s4-author-claude'), off = await bg('s4-author-codex')
    expect(on).not.toBe(off)
    expect(on).not.toMatch(/rgba\(0, 0, 0, 0\)|transparent/)
  })

  test('install commands are whole (wrapped, not cut); «codex --version» is not broken', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await page.goto(`/setup/system?${FAST}`)
    await page.getByTestId('s1-help').click()
    const cmds = page.getByTestId('s1-help-panel').getByTestId('cmd')
    await expect(cmds.filter({ hasText: 'powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"' })).toHaveCount(1)
    for (const c of await cmds.all()) {
      expect(await c.evaluate((e) => e.scrollWidth <= e.clientWidth && getComputedStyle(e).textOverflow !== 'ellipsis')).toBe(true)
    }
    for (const p of ['claude', 'codex']) {
      const c = page.getByTestId(`check-cmd-${p}`)
      await expect(c).toHaveText(`${p} --version`)
      expect(await c.evaluate((e) => new Set([...e.getClientRects()].map((r) => Math.round(r.y))).size), 'one line').toBe(1)
    }
  })

  test('«% new text» on one line beside the custom number', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page)
    await page.goto(`/new?${FAST}`)
    await page.getByTestId('policy-toggle').click()
    await page.getByTestId('s6pol-new-custom').click()
    const unit = page.getByTestId('s6pol-pct-unit')
    await expect(unit).toHaveText('% new text')
    expect(await unit.evaluate((e) => new Set([...e.getClientRects()].map((r) => Math.round(r.y))).size), 'one line').toBe(1)
  })

  test('thin scrollbars in the family colours, light and dark', async ({ page }) => {
    await setupDone(page)
    for (const q of ['', '&theme=dark']) {
      await page.goto(`/about?${FAST}${q}`)
      const s = await page.getByTestId('main').evaluate((e) => ({ w: getComputedStyle(e).scrollbarWidth, c: getComputedStyle(e).scrollbarColor }))
      expect(s.w).toBe('thin')
      expect(s.c).not.toBe('auto')
    }
  })

  test('the run folder path on S10 is whole', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page)
    await page.goto(`/review/sample/export?${FAST}`)
    const p = page.getByTestId('run-path')
    await expect(p).toHaveText(/\\runs\\sample\\output$/)
    expect(await p.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true)
  })

  test('the S9 tooltip is anchored to the hovered value', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 })
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/review/sample/decision?${FAST}`)
    const trigger = page.getByTestId('k-threshold').locator('[data-explain]')
    await trigger.hover()
    const tip = page.getByRole('tooltip')
    await expect(tip).toBeVisible()
    const t = (await trigger.boundingBox())!
    const content = (await page.locator('[data-radix-popper-content-wrapper]').first().boundingBox())!
    // Starts at the value's own left edge; never over the navigation rail.
    expect(Math.abs(content.x - t.x)).toBeLessThanOrEqual(2)
    expect(content.x).toBeGreaterThanOrEqual((await page.getByTestId('main').boundingBox())!.x)
    // Right above or right below the value (Radix flips the side to fit), never far away.
    expect(Math.min(Math.abs(content.y + content.height - t.y), Math.abs(content.y - (t.y + t.height)))).toBeLessThan(20)
  })

  test('Export is disabled while a round runs and enabled when the run is finished', async ({ page }) => {
    await setupDone(page)
    await page.goto('/review/operations-annex?speed=2')
    await page.getByTestId('resume').click()
    await expect(page.getByTestId('status-strip')).toContainText('Running')
    await expect(page.getByTestId('export')).toBeDisabled()
    await page.getByTestId('pause').click()
    await expect(page.getByTestId('export')).toBeDisabled()
    await page.goto(`/review/sample?${FAST}`)
    await expect(page.getByTestId('export')).toBeEnabled()
    await page.goto(`/review/board-pack?${FAST}`)
    await expect(page.getByTestId('export')).toBeEnabled()
  })

  test('the three reviewer-contribution headings on one line', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 })
    await setupDone(page)
    await page.goto(`/new?${FAST}`)
    await page.getByTestId('policy-toggle').click()
    const tops: number[] = []
    for (const c of ['verify', 'correct', 'improve']) tops.push((await page.getByTestId(`s6pol-contrib-${c}`).locator('> span').first().boundingBox())!.y)
    expect(Math.max(...tops) - Math.min(...tops)).toBeLessThanOrEqual(1)
  })
})

test.describe('Carried over from Figma', () => {
  test('S7 Diff: two-column table with − / + rows and the «Revision 2 → Revision 3» selector', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/review/sample?${FAST}`)
    await page.getByTestId('tab-diff').click()
    await expect(page.getByTestId('diff-rev').locator('option:checked')).toHaveText('Revision 2 → Revision 3')
    await expect(page.getByTestId('diff-rev').locator('option').first()).toHaveText('Original → Revision 1')
    const side = page.getByTestId('diff-side')
    expect(await side.locator('[data-changed="1"]').count()).toBeGreaterThan(0)
    await expect(side.locator('[data-changed="1"]').first()).toContainText('−')
    await expect(side.locator('[data-changed="1"]').first()).toContainText('+')
  })

  test('S7 issues as rows and round chips; Raw output coloured; panel maximised folds the other one', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/review/sample?${FAST}`)
    const row = page.getByTestId('issue-R01-001')
    await expect(row.getByTestId('issue-status')).toHaveText('resolved')
    await expect(page.getByTestId('round-1').locator('[data-step="done"]')).toHaveCount(2)
    await page.getByTestId('mode-codex-raw').click()
    await expect(term(page, 'codex')).toContainText('"type":"thread.started"')
    expect(await page.getByTestId('term-codex').locator('.xterm-rows span[style*="color"]').count()).toBeGreaterThan(3)
    await page.getByTestId('panel-head-codex').dblclick()
    await expect(page.getByTestId('panel-folded-claude')).toBeVisible()
    await expect(page.getByTestId('restore-hint-codex')).toHaveText('Double-click to restore')
    await page.getByTestId('panel-folded-claude').click()
    await expect(page.getByTestId('panel-claude')).toBeVisible()
    await expect(page.getByTestId('panel-folded-claude')).toHaveCount(0)
  })

  test('S9 confirmation names human_decision.json; the approved state shows who, when, the note and Export', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/review/sample/decision?${FAST}`)
    await page.getByTestId('approve').click()
    await expect(page.getByTestId('confirm-dialog')).toContainText('human_decision.json')
    await page.getByTestId('decider-name').fill('Dana Kim')
    await page.getByTestId('decider-note').fill('Approved for executive review.')
    await page.getByTestId('confirm').click()
    await expect(page.getByTestId('decided-state')).toHaveText('Approved by Human')
    await expect(page.getByTestId('decided-by')).toContainText('Dana Kim')
    await expect(page.getByTestId('decided-note')).toContainText('Approved for executive review.')
    await page.getByTestId('decided-export').click()
    await expect(page.locator('[data-screen="S10"]')).toBeVisible()
  })
})

test.describe('Layout and interaction regressions', () => {
  test('S11 at 1920 — every Delete icon inside the table card', async ({ page }) => {
    await page.setViewportSize({ width: 1920, height: 1080 })
    await setupDone(page)
    await bannersSeen(page)
    await page.goto(`/reviews?${FAST}`)
    const table = page.getByTestId('runs')
    const card = await table.evaluate((t) => t.parentElement!.getBoundingClientRect().right)
    expect(await table.evaluate((t) => t.getBoundingClientRect().width - t.parentElement!.clientWidth)).toBeLessThanOrEqual(1)
    for (const b of await table.getByTestId('delete').all()) {
      const r = (await b.boundingBox())!
      expect(r.x + r.width).toBeLessThanOrEqual(card)
    }
  })

  test('the command waiting at the prompt stays whole when the panel is at its narrowest', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await page.goto(`/setup/claude?${FAST}`)
    await page.getByTestId('paste-login').click()
    const flat = async () => (await term(page, 'claude').locator(':scope > div').allTextContents()).join('').replace(/\s+/g, '')
    await expect.poll(flat).toMatch(/claudeauthlogin$/)
    // Narrow the panel area to its minimum with the width splitter.
    const s = (await page.getByTestId('width-splitter').boundingBox())!
    const before = (await page.getByTestId('panels-area').boundingBox())!.width
    await page.mouse.move(s.x + s.width / 2, s.y + s.height / 2)
    await page.mouse.down()
    await page.mouse.move(1275, s.y + s.height / 2, { steps: 8 })
    await page.mouse.up()
    const after = (await page.getByTestId('panels-area').boundingBox())!.width
    expect(after).toBeLessThan(before - 100)
    //  at the narrowest panel too: the header title never runs under the controls.
    for (const p of ['claude', 'codex'] as const) {
      const h = page.getByTestId(`panel-title-${p}`)
      expect(await h.evaluate((e) => e.scrollWidth <= e.clientWidth + 1)).toBe(true)
      const seg = (await page.getByTestId(`mode-${p}`).boundingBox())!
      const right = await h.evaluate((e) => { const rg = document.createRange(); rg.selectNodeContents(e); return rg.getBoundingClientRect().right })
      expect(right).toBeLessThanOrEqual(seg.x)
    }
    // The prompt is printed once, not twice.
    expect((await flat()).match(/workspaces\\author>/g)?.length).toBe(1)
    await expect.poll(flat).toMatch(/claudeauthlogin$/)
    // The command wraps onto its own rows inside the panel; nothing is clipped by the panel's edge.
    const host = (await page.getByTestId('term-claude').boundingBox())!
    for (const r of await term(page, 'claude').locator(':scope > div').all()) {
      const right = await r.evaluate((e) => { const rg = document.createRange(); rg.selectNodeContents(e); return rg.getBoundingClientRect().right })
      expect(right).toBeLessThanOrEqual(host.x + host.width + 1)
    }
    // And back to wide: still whole.
    const s2 = (await page.getByTestId('width-splitter').boundingBox())!
    await page.mouse.move(s2.x + s2.width / 2, s2.y + s2.height / 2)
    await page.mouse.down()
    await page.mouse.move(700, s2.y + s2.height / 2, { steps: 8 })
    await page.mouse.up()
    await expect.poll(flat).toMatch(/claudeauthlogin$/)
  })

  test('panel headers read provider and role in full at 1280, swapped roles and maximised', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page, { author: 'codex' })
    await bannersSeen(page)
    await page.goto(`/reviews?${FAST}`)
    const whole = async (p: 'claude' | 'codex', text: string) => {
      const h = page.getByTestId(`panel-title-${p}`)
      await expect(h).toHaveText(text)
      expect(await h.evaluate((e) => e.scrollWidth <= e.clientWidth + 1 && e.getBoundingClientRect().right <= e.parentElement!.getBoundingClientRect().right)).toBe(true)
    }
    await whole('claude', 'CLAUDE CODE · INDEPENDENT REVIEWER')
    await whole('codex', 'CODEX · AUTHOR')
    await page.getByTestId('max-claude').click()
    await whole('claude', 'CLAUDE CODE · INDEPENDENT REVIEWER')
  })

  test('stopped in round 1 then rejected — Stopped everywhere, «Not reviewed», Export partial on S7 and S9', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    await setupDone(page)
    await bannersSeen(page)
    await page.goto('/new?speed=1')
    await page.getByTestId('s6-name').fill('Width test')
    await page.getByTestId('s6-objective').fill('Improve this document for executive review without adding unsupported claims.')
    await page.getByTestId('templates').click()
    await page.getByTestId('template-business_case').click()
    await page.getByTestId('approve-criteria').click()
    await page.getByTestId('primary-input').setInputFiles({ name: 'business_case.md', mimeType: 'text/markdown', buffer: Buffer.alloc(2048, 'a') })
    await page.getByTestId('start').click()
    await expect(page.getByTestId('status-strip')).toContainText('Running')
    await page.getByTestId('stop').click()
    await expect(page.getByTestId('status-strip')).toContainText('Stopped')
    await expect(page.getByTestId('export')).toBeEnabled()
    await expect(page.getByTestId('export')).toHaveText('Export partial package')
    await page.getByTestId('human-decision').click()
    await expect(page.getByTestId('screen-title')).toHaveText('Review stopped')
    await expect(page.getByTestId('threshold')).toHaveText('Not reviewed')
    await expect(page.getByTestId('k-rounds')).toContainText('0 / 5')
    await page.getByTestId('reject').click()
    await page.getByTestId('decider-name').fill('Eyes Tester')
    await page.getByTestId('confirm').click()
    await expect(page.getByTestId('decided-state')).toHaveText('Rejected by Human')
    await expect(page.getByTestId('status-strip')).toContainText('Stopped')
    await expect(page.getByTestId('status-strip')).not.toContainText('Completed')
    await expect(page.getByTestId('decided-export')).toHaveText('Export partial package')
    await page.getByTestId('back-to-workspace').click()
    await expect(page.getByTestId('status-strip')).toContainText('Stopped')
    await expect(page.getByTestId('export')).toBeEnabled()
    await expect(page.getByTestId('export')).toHaveText('Export partial package')
    await page.getByTestId('export').click()
    await expect(page.getByTestId('export-partial')).toBeEnabled()
  })

  test('S2 and S3 slot captions inside their slots at 1280, clear of the picture mark', async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 })
    for (const path of ['/setup/claude', '/setup/codex']) {
      await page.goto(`${path}?${FAST}`)
      for (const slot of await page.getByTestId('shot-slot').all()) {
        const r = (await slot.boundingBox())!
        const c = (await slot.getByTestId('shot-caption').boundingBox())!
        const m = (await slot.getByTestId('shot-mark').boundingBox())!
        expect(c.y).toBeGreaterThanOrEqual(r.y)
        expect(c.y + c.height).toBeLessThanOrEqual(r.y + r.height)
        expect(m.y).toBeGreaterThanOrEqual(c.y + c.height - 0.5)
        expect(await slot.evaluate((e) => e.scrollHeight <= e.clientHeight + 1)).toBe(true)
      }
    }
  })
})

test.describe('Layout and interaction regressions', () => {
  test('Stop during review 4 keeps round 4 and revision 4 in the strip and the document', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    await page.goto('/review/operations-annex?speed=3')
    await page.getByTestId('resume').click()
    await expect(page.getByTestId('panel-codex')).toHaveAttribute('data-status', 'working', { timeout: 20_000 })
    await page.getByTestId('stop').click()
    await expect(page.getByTestId('status-strip')).toContainText('Round 4 of 5')
    await expect(page.getByTestId('status-strip')).toContainText('Stopped')
    await expect(page.getByTestId('doc-current-rev')).toHaveText('Revision 4')
    await expect(page.getByTestId('doc-note')).toContainText('Revision 4 was written; its review did not finish')
  })

  test('while a new round runs, the last verdict shows as past («Round 3: …»)', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    await page.goto('/review/operations-annex?speed=2')
    const chip = page.getByTestId('gate-current')
    await expect(chip).toHaveAttribute('data-past', '3')
    await expect(chip).toHaveText(/^Round 3: Stop: no new material issues/)
    await page.getByTestId('resume').click()
    await expect(page.getByTestId('status-strip')).toContainText('Running')
    await expect(chip).toHaveAttribute('data-past', '3')
    await page.goto(`/review/sample?${FAST}`)
    await expect(page.getByTestId('gate-current')).not.toHaveAttribute('data-past', /.*/)
    await expect(page.getByTestId('gate-current')).toHaveText('Stop: no new material issues for 2 rounds')
  })

  test('the terminal scrollbar is thin and in the family colours, light and dark', async ({ page }) => {
    await setupDone(page)
    await bannersSeen(page)
    for (const q of ['', '&theme=dark']) {
      await page.goto(`/review/sample?${FAST}${q}`)
      const bar = page.getByTestId('term-codex').locator('.xterm-scrollable-element > .scrollbar.vertical')
      await expect(bar).toHaveCount(1)
      expect(await bar.evaluate((e) => e.getBoundingClientRect().width)).toBeLessThanOrEqual(8)
      const slider = bar.locator('.slider')
      const st = await slider.evaluate((e) => ({ w: e.getBoundingClientRect().width, r: getComputedStyle(e).borderRadius, bg: getComputedStyle(e).backgroundColor }))
      expect(st.w).toBeLessThanOrEqual(6)
      expect(st.r).toBe('3px')
      expect(st.bg).not.toMatch(/^rgba\(121, 121, 121/) // not xterm's default grey
    }
  })

  test('the S9 picture is the human decision (a seal), and every banner fills its card in dark mode too', async ({ page, request }) => {
    const m = await (await request.get('/banners/manifest.json')).json()
    expect(m.SY03.alt).toMatch(/seal/i)
    expect(m.SY03.alt).not.toMatch(/pause/i)
    await setupDone(page)
    for (const [path, open] of [['/reviews', false], ['/review/sample', true], ['/review/sample/decision', true]] as const) {
      await page.goto(`${path}?${FAST}&theme=dark`)
      const b = page.getByTestId('banner')
      if (open || (await b.getAttribute('data-state')) === 'strip') await b.click()
      await expect(b).toHaveAttribute('data-state', 'full')
      await expect(b.locator('img')).toBeVisible()
      await page.waitForTimeout(1000)
      const frameW = await b.locator('[data-banner-frame]').evaluate((e) => e.getBoundingClientRect().width)
      expect(await b.evaluate((e) => e.clientWidth) - frameW).toBeLessThanOrEqual(2)
    }
  })
})
