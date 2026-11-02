#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
set -euo pipefail
cd "$(dirname "$0")/.."
node --input-type=module <<'JS'
import fs from 'node:fs';
import {execFileSync} from 'node:child_process';
const json = f => JSON.parse(fs.readFileSync(f,'utf8'));
const version = json('package.json').version;
const cargo = fs.readFileSync('Cargo.toml','utf8').match(/^version = "([^"]+)"/m)?.[1];
if (json('src-tauri/tauri.conf.json').version !== version || cargo !== version) throw Error('App versions differ');
for (const f of ['src-tauri/Cargo.toml','crates/symvoulio-core/Cargo.toml']) {
 const text = fs.readFileSync(f,'utf8');
 if (!/^version\.workspace = true$/m.test(text) && text.match(/^version = "([^"]+)"/m)?.[1] !== version) throw Error(`${f}: version differs`);
}
const equal = (actual, label) => { if (actual !== version) throw Error(`${label}: ${actual} differs from ${version}`); };
const lock = json('package-lock.json');
equal(lock.version, 'npm lock');
for (const key of ['', 'packages/design-system']) equal(lock.packages[key].version, `npm lock ${key}`);
equal(json('packages/design-system/package.json').version, 'design system');
const cargoLock = fs.readFileSync('Cargo.lock', 'utf8');
for (const name of ['symvoulio', 'symvoulio-core']) {
 const entry = cargoLock.split('[[package]]').find(x => x.includes(`name = "${name}"`));
 equal(entry?.match(/^version = "([^"]+)"/m)?.[1], `Cargo.lock ${name}`);
}
equal(json('src/services/wire.fixture.json').appInfo.version, 'wire metadata');
equal(fs.readFileSync('src/services/canned.ts', 'utf8').match(/appInfo:.*version: '([^']+)'/)?.[1], 'canned metadata');
const about = json('src/i18n/en.json').s13.version;
if (!about.startsWith(`Version ${version} `)) throw Error('en.json version differs');
let tag;
try { tag = execFileSync('git',['describe','--tags','--exact-match'],{encoding:'utf8',stdio:['ignore','pipe','ignore']}).trim(); } catch {}
if (tag && tag !== `v${version}`) throw Error(`Tag ${tag} differs from v${version}`);
console.log(`Version ${version}: consistent${tag ? ` (${tag})` : ' (no exact tag)'}`);
JS
