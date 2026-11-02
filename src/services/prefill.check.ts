// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Failing before fix: AssertionError [ERR_ASSERTION]: R14 restart prefill must register revision bytes
/// <reference types="node" />
import assert from 'node:assert/strict'
import * as files from './files'
assert.equal(typeof files.revisionFile, 'function', 'R14 restart prefill must register revision bytes')
const chip = files.revisionFile(2, 'saved text')
assert.equal(new TextDecoder().decode(new Uint8Array(await files.fileBytes(chip))), 'saved text')
const sample = await files.sampleFile(async () => new Response('bundled sample'))
assert.equal(new TextDecoder().decode(new Uint8Array(await files.fileBytes(sample))), 'bundled sample')
console.log('R14 prefill bytes passed')
