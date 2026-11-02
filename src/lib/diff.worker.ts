// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { diffDocs } from './diff'
self.onmessage = event => { self.postMessage(diffDocs(event.data.before, event.data.after)) }
