// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Symvoulio 0.3.1 fixes F5-F8 that the browser stand cannot show (it runs as Windows and has no human questions).
// Run: npx jiti tests/mac_031.check.ts
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import * as React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { createJiti } from 'jiti'
import { shortDate } from '../src/lib/runDate'
import { shellHintKey } from '../src/lib/platformNote'

const en = JSON.parse(await readFile(new URL('../src/i18n/en.json', import.meta.url), 'utf8'))

// F6: «Answered by» and the name field are apart («Answered byYour name» in 0.3.0).
;(globalThis as { React?: unknown }).React = React
const jiti = createJiti(import.meta.url, { jsx: true })
const { AnsweredBy } = await jiti.import<typeof import('../src/lib/AnsweredBy')>(new URL('../src/lib/AnsweredBy.tsx', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
const html = renderToStaticMarkup(React.createElement(AnsweredBy, { label: en.humanQuestions.answeredBy, placeholder: en.humanQuestions.yourName, value: '', onChange: () => {} }))
assert.match(html, /Answered by <input[^>]*placeholder="Your name"/, html)
const list = await readFile(new URL('../src/lib/HumanQuestionList.tsx', import.meta.url), 'utf8')
assert.match(list, /<AnsweredBy label=\{t\('humanQuestions\.answeredBy'\)\}/)

// F7: no Windows wording on a Mac; Windows keeps its text.
assert.equal(shellHintKey('macos'), 's12.shellHintMac')
for (const os of ['windows', 'linux', undefined]) assert.equal(shellHintKey(os), 's12.shellHint')
assert.doesNotMatch(en.s12.shellHintMac, /PowerShell|Windows/)
assert.match(en.s12.shellHintMac, /zsh/)
assert.equal(en.s12.shellHint, 'PowerShell 7 is used when it is installed, otherwise Windows PowerShell. On macOS the panels use zsh.')
const settings = await readFile(new URL('../src/screens/S12Settings.tsx', import.meta.url), 'utf8')
assert.match(settings, /t\(shellHintKey\(os\)\)/)

// F8: «Mon D, HH:MM» in UTC, never Intl's engine-specific joiner («Nov 7 at 23:04» on WebKit).
assert.equal(shortDate('2026-11-07T23:04:31Z'), 'Nov 7, 23:04')
assert.equal(shortDate('2026-10-30T09:05:00Z'), 'Oct 30, 09:05')
const OriginalFormat = Intl.DateTimeFormat
try {
  // Simulate WebKit: format() joins with « at »; formatToParts() gives the same parts plus that literal.
  ;(Intl as { DateTimeFormat: unknown }).DateTimeFormat = function (locale: string, options: Intl.DateTimeFormatOptions) {
    const real = new OriginalFormat(locale, options)
    return {
      format: (d: Date) => real.format(d).replace(', ', ' at '),
      formatToParts: (d: Date) => real.formatToParts(d).map((p) => (p.type === 'literal' && p.value === ', ' ? { ...p, value: ' at ' } : p)),
    }
  } as unknown as typeof Intl.DateTimeFormat
  assert.equal(shortDate('2026-11-07T23:04:31Z'), 'Nov 7, 23:04')
} finally {
  ;(Intl as { DateTimeFormat: unknown }).DateTimeFormat = OriginalFormat
}

// F5: the help text per step and state.
const { helpMessage, helpStep } = await jiti.import<typeof import('../src/lib/setupHelp')>(new URL('../src/lib/setupHelp.ts', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'))
assert.equal(helpStep('/setup/system'), 'system')
assert.equal(helpStep('/setup/claude'), 'claude')
assert.equal(helpStep('/setup/codex'), 'codex')
for (const p of ['/setup/roles', '/setup/policy', '/reviews', '/settings', '/setup/claude/x']) assert.equal(helpStep(p), null, p)
const conn = (claude: [boolean, boolean], codex: [boolean, boolean]) => ({
  claude: { installed: claude[0], connected: claude[1], version: '', method: null },
  codex: { installed: codex[0], connected: codex[1], version: '', method: null },
})
const idle = { claude: false, codex: false }
assert.deepEqual(helpMessage('system', conn([false, false], [true, false]), {}, idle), { key: 'help.system.install', provider: 'claude' })
assert.deepEqual(helpMessage('system', conn([true, false], [false, false]), { codex: true }, idle), { key: 'help.system.installing', provider: 'codex' })
assert.deepEqual(helpMessage('system', conn([true, false], [true, false]), {}, idle), { key: 'help.system.signIn' })
assert.deepEqual(helpMessage('system', conn([true, true], [true, true]), {}, idle), { key: 'help.system.done' })
assert.deepEqual(helpMessage('claude', conn([false, false], [true, true]), {}, idle), { key: 'help.connect.notInstalled', provider: 'claude' })
assert.deepEqual(helpMessage('claude', conn([true, false], [true, true]), {}, idle), { key: 'help.connect.signIn', provider: 'claude' })
assert.deepEqual(helpMessage('codex', conn([true, true], [true, false]), {}, { claude: false, codex: true }), { key: 'help.connect.waiting', provider: 'codex' })
assert.deepEqual(helpMessage('codex', conn([true, true], [true, true]), {}, idle), { key: 'help.connect.done', provider: 'codex' })
assert.deepEqual(helpMessage('codex', conn([true, true], [true, false]), {}, idle, { codex: 'api_key' }), { key: 'help.connect.apiKey', provider: 'codex' })
for (const group of ['system', 'connect'] as const) for (const [k, v] of Object.entries(en.help[group])) assert.ok(typeof v === 'string' && (v as string).length > 20, `help.${group}.${k}`)
assert.doesNotMatch(JSON.stringify(en.help), /Screenshot|PowerShell/)
// No picture placeholders left in the step cards.
const connect = await readFile(new URL('../src/screens/S2S3Connect.tsx', import.meta.url), 'utf8')
// macOS steps (macSteps and their list) have no picture slot; the Windows list keeps the 0.3.0 one (review r1).
const macList = connect.slice(connect.indexOf('{mac ? <ol'), connect.indexOf(': <ol', connect.indexOf('{mac ? <ol')))
assert.ok(macList.length > 200, 'macOS step list not found')
assert.doesNotMatch(macList, /Shot|shot/)
const macSteps = connect.slice(connect.indexOf('const macSteps'), connect.indexOf('return ('))
assert.doesNotMatch(macSteps, /shot/)
console.log('mac 0.3.1 check passed')
