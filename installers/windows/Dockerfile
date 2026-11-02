# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
# Cross-compile the Windows NSIS installer on Linux (Tauri «Windows Installer», cross-compiling section):
# NSIS + lld/llvm (llvm-rc) + the MSVC target through cargo-xwin. MSI needs WiX on Windows and is not built here.
FROM rust:1.99.1-bookworm
RUN apt-get update \
 && apt-get install -y --no-install-recommends nsis lld llvm clang ca-certificates curl git \
 && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-msvc \
 && cargo install --locked cargo-xwin \
 && cargo install --locked tauri-cli@2.12.4
WORKDIR /src
