// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { clsx } from 'clsx'

/** Class names joined, falsy parts dropped (clsx). */
export function cx(...a: Parameters<typeof clsx>) { return clsx(...a) }
