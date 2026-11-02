#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
# Runs inside symvoulio-build: unpack one commit (git archive tarball at /in/src.tar), build the NSIS setup,
# write it with its SHA-256 to /out.
set -euo pipefail
mkdir -p /src/review-ce && tar -xf /in/src.tar -C /src/review-ce && cd /src/review-ce
npm ci --legacy-peer-deps --no-audit --no-fund
cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
cd target/x86_64-pc-windows-msvc/release/bundle/nsis
for f in *.exe; do sha256sum "$f" > "$f.sha256"; cp "$f" "$f.sha256" /out/; done
ls -la /out
