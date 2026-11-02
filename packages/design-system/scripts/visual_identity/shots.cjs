// Full-page screenshots of every screen listed on /screens: en light, en dark, he light; 1920x1080 and 1280x800.
// node design-system/scripts/visual_identity/shots.cjs <baseUrl> <outDir>   (from ui/, against a vite preview)
// Visual identity gate: shoot before a change and after it, then pixdiff.py; the target is 0 changed pixels.
const { chromium } = require('@playwright/test')
const fs = require('node:fs')
const [base, out] = process.argv.slice(2)
const MODES = [
  { name: 'en-light', q: 'lang=en&role=coo' },
  { name: 'en-dark', q: 'lang=en&role=coo&theme=dark' },
  { name: 'he-light', q: 'lang=he&role=coo' },
]
const SIZES = [{ w: 1920, h: 1080 }, { w: 1280, h: 800 }]
const FREEZE = '*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}'
;(async () => {
  const browser = await chromium.launch()
  const probe = await browser.newPage()
  await probe.goto(`${base}/screens?lang=en&role=coo`)
  await probe.locator('[data-target]').first().waitFor()
  const routes = await probe.$$eval('a[data-target]', (as) => as.map((a) => [a.dataset.target, a.getAttribute('href')]))
  routes.push(['ALL', '/screens'])
  await probe.close()
  console.log('routes', routes.length, JSON.stringify(routes))
  for (const s of SIZES) for (const m of MODES) {
    const dir = `${out}/${s.w}x${s.h}/${m.name}`
    fs.mkdirSync(dir, { recursive: true })
    const ctx = await browser.newContext({ viewport: { width: s.w, height: s.h }, deviceScaleFactor: 1, reducedMotion: 'reduce' })
    const page = await ctx.newPage()
    await page.clock.setFixedTime(new Date('2026-10-30T09:00:00Z'))
    for (const [id, href] of routes) {
      await page.goto(`${base}${href}${href.includes('?') ? '&' : '?'}${m.q}`, { waitUntil: 'networkidle' })
      await page.addStyleTag({ content: FREEZE })
      await page.locator('[data-screen]').first().waitFor()
      await page.evaluate(() => document.fonts.ready)
      await page.waitForTimeout(2800) // the Motif bars and lines animate in JS (motion) for ~2.3 s; reduced motion stops only its dots
      // The content pane scrolls inside <main>; a full page shot of the window would cut it. Unclip it for the shot.
      await page.addStyleTag({ content: 'html,body,#root{height:auto!important}#root>div{height:auto!important;min-height:100vh}main{overflow:visible!important}nav[data-testid=nav]{overflow:visible!important}' })
      await page.waitForTimeout(150)
      await page.screenshot({ path: `${dir}/${id}.png`, fullPage: true })
    }
    await ctx.close()
    console.log('done', s.w, m.name)
  }
  await browser.close()
})().catch((e) => { console.error(e); process.exit(1) })
