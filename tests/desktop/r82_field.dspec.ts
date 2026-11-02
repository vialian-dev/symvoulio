// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { test, expect, type Page } from '@playwright/test'
import { launchApp, relaunch, ipc, BASE, type App } from './harness'
import { readFileSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'

async function rejected(p: Page, name: string, buffer = Buffer.from([0xff]), filename = 'invalid.txt') {
  await p.getByTestId('nav-new').click()
  await p.getByTestId('s6-name').fill(name)
  await p.getByTestId('s6-objective').fill('Check the document against the criterion.')
  await p.getByTestId('add-criterion').click()
  await p.getByTestId('criterion-0').fill('Facts are supported.')
  await p.getByTestId('approve-criteria').click()
  await p.getByTestId('primary-input').setInputFiles({name: filename, mimeType: 'application/octet-stream', buffer})
  await p.getByTestId('start').click()
  await expect(p.getByTestId('main').getByText('Failed', {exact:true})).toBeVisible({timeout:60000})
  return p.url().split('/').at(-1)!
}
async function normalClose(a: App) {
  // CloseMainWindow sends WM_CLOSE, the same normal close request as the titlebar X.
  expect(execFileSync('powershell.exe',['-NoProfile','-Command', `(Get-Process -Id ${a.proc.pid}).CloseMainWindow()`],{encoding:'utf8'}).trim()).toBe('True')
  await expect.poll(() => a.proc.exitCode, {timeout:15000}).not.toBeNull()
}

test('sample export states its availability truthfully', async () => {
 const a = await launchApp(); try {
  await a.page.getByTestId('open-sample').first().click(); await a.page.getByTestId('export').click()
  if (await a.page.getByTestId('do-export').isEnabled()) {
   await a.page.getByTestId('do-export').click()
   await expect(a.page.getByTestId('export-result')).toContainText('Exported to')
   const path=await a.page.getByTestId('run-path').innerText()
   console.log(`Sample export claimed path=${path}; final document exists=${existsSync(join(path,'final','final_document.md'))}`)
   expect(existsSync(join(path,'final','final_document.md'))).toBe(true)
  }
  await expect(a.page.getByTestId('sample-export-unavailable')).toBeVisible()
  await expect(a.page.getByTestId('do-export')).toBeDisabled()
  await expect(a.page.getByTestId('open-folder')).toBeDisabled()
  await expect(a.page.getByTestId('main')).not.toContainText('Exported to')
 } finally { await a.stop() }
})
test('sample round trip and normal restart retain both live shells with empty saved preferences', async () => {
 let a = await launchApp({env:{FAKE_CLI_ABSENT:'claude,codex', FAKE_CLI_PTY_BYTES_LOG:'r82-input.log'}})
 try {
  const settings: any = await ipc(a.page,'settings_get'); settings.shell = {claude:'',codex:''}; await ipc(a.page,'settings_set',{settings})
  for (let pass=0; pass<2; pass++) {
   if(pass) { await normalClose(a); a = await relaunch(a) }
   await a.page.getByTestId('nav-reviews').click(); await a.page.getByTestId('open-sample').first().click()
   await a.page.getByTestId('nav-about').click(); await a.page.getByTestId('nav-new').click()
   await a.page.getByTestId('nav-settings').click()
   for (const panel of ['claude','codex']) await expect(a.page.getByTestId(`actual-shell-${panel}`)).not.toContainText('No running panel')
   await a.page.getByTestId('nav-new').click()
   for (const panel of ['claude','codex']) {
    await a.page.getByTestId(`term-${panel}`).click(); await a.page.keyboard.type(`echo r82-${panel}-${pass}`); await a.page.keyboard.press('Enter')
    await expect.poll(()=>existsSync(join(a.dataDir,'r82-input.log')) ? readFileSync(join(a.dataDir,'r82-input.log'),'utf8') : '').toContain(`echo r82-${panel}-${pass}`)
   }
   await expect.poll(()=>existsSync(join(a.dataDir,'r82-input.log')) ? readFileSync(join(a.dataDir,'r82-input.log'),'utf8') : '').toContain(`echo r82-claude-${pass}`)
   await expect.poll(()=>readFileSync(join(a.dataDir,'r82-input.log'),'utf8')).toContain(`echo r82-codex-${pass}`)
  }
 } finally { await a.stop() }
})
test('failed preflight review survives normal close and restart, and deletion persists', async () => {
 let a = await launchApp(); try {
  const id = await rejected(a.page,'R82 rejected')
  await normalClose(a); a = await relaunch(a)
  await a.page.getByTestId('nav-reviews').click()
  await expect(a.page.getByTestId(`run-${id}`)).toContainText('R82 rejected')
  await a.page.getByTestId(`run-${id}`).getByTestId('open').click()
  await expect(a.page.getByRole('alert')).toContainText('UTF-8')
  await a.page.getByRole('button',{name:'Delete',exact:true}).click()
  await normalClose(a); a = await relaunch(a)
  await a.page.getByTestId('nav-reviews').click(); await expect(a.page.getByTestId(`run-${id}`)).toHaveCount(0)
 } finally { await a.stop() }
})
test('rejected PDF, UTF-8 and oversized input have no completed claim or endless loading', async () => {
 const a = await launchApp(); try {
  const blankPdf=Buffer.from(readFileSync('crates/symvoulio-core/tests/fixtures/text.pdf','utf8').replace(/BT.*?ET/, text=>' '.repeat(text.length)))
  for(const [name,bytes,filename,error] of [['No text layer',blankPdf,'scan.pdf','no text layer'],['UTF-8',Buffer.from([0xff]),'invalid.txt','UTF-8'],['Oversized',Buffer.alloc(8*1024*1024+1,97),'large.txt','8 MB']] as const) {
   await rejected(a.page,name,bytes,filename)
   await expect(a.page.getByRole('alert')).toContainText(error)
   await expect(a.page.getByTestId('status-state')).toHaveText('Failed')
   await expect(a.page.getByTestId('status-strip')).not.toContainText('Round')
   await expect(a.page.getByTestId('main')).not.toContainText('Loading')
   await expect(a.page.getByTestId('tab-current')).toHaveCount(0)
  }
 } finally { await a.stop() }
})
test('sample decision fits narrow and wide panes', async () => {
 const a = await launchApp(); try {
  await a.page.getByTestId('open-sample').first().click(); await a.page.getByTestId('human-decision').click()
  for(const width of [1366,1920,390]) {
   await a.page.setViewportSize({width,height:1080}); const main=a.page.getByTestId('main')
   await expect(a.page.getByTestId('reviewer-status')).toBeVisible()
   await a.page.screenshot({path:test.info().outputPath(`decision-${width}.png`)})
   expect(await main.evaluate(e=>({scroll:e.scrollWidth,client:e.clientWidth}))).toEqual(expect.objectContaining({scroll:expect.any(Number)}))
   expect(await main.evaluate(e=>e.scrollWidth <= e.clientWidth+1), `horizontal overflow at ${width}`).toBe(true)
   const gap=await a.page.getByTestId('reviewer-status').evaluate(e=>{const range=document.createRange();range.selectNodeContents(e);const card=e.closest('[data-testid="k-reviewer"]')!.getBoundingClientRect();return card.right-range.getBoundingClientRect().right})
   expect(gap).toBeGreaterThanOrEqual(12)
  }
 } finally { await a.stop() }
})
test('API guidance never renders an empty command', async () => {
 const a = await launchApp(); try {
  const settings: any = await ipc(a.page,'settings_get'); settings.shell.codex='unavailable-shell'; await ipc(a.page,'settings_set',{settings})
  await a.page.goto(`${BASE}/setup/codex`); await a.page.getByTestId('method-api-key').click()
  await expect(a.page.getByTestId('guide-step-2')).toContainText('OPENAI_API_KEY')
  await expect(a.page.getByTestId('guide-step-2')).toContainText('Settings')
  await expect(a.page.getByTestId('paste-login')).toBeDisabled()
 } finally { await a.stop() }
})
test('custom percentages validate and normalize', async () => {
 const a=await launchApp(); try {
  await a.page.goto(`${BASE}/setup/policy`); await a.page.getByTestId('pol-new-custom').click(); const input=a.page.getByTestId('pol-new-pct')
  for(const text of ['150','-5','abc']) { await input.fill(text); await input.blur(); await expect(a.page.getByTestId('pol-pct-error')).toBeVisible() }
  await input.fill('015'); await input.blur(); await expect(input).toHaveValue('15'); await expect(a.page.getByTestId('pol-pct-error')).toHaveCount(0)
 } finally { await a.stop() }
})
test('empty new review lists all five requirements', async () => {
 const a=await launchApp(); try { await a.page.getByTestId('nav-new').click(); await expect(a.page.getByTestId('missing').locator('li')).toHaveCount(5) } finally { await a.stop() }
})
