// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// The «Answered by [Your name]» field of the human questions. F6: the label and the field are separated by a space
// (0.3.0 rendered «Answered byYour name»).
export function AnsweredBy({ label, placeholder, value, onChange }: { label: string; placeholder: string; value: string; onChange: (value: string) => void }) {
  return <label data-testid="answered-by">{label}{' '}<input placeholder={placeholder} value={value} onChange={e => onChange(e.target.value)} /></label>
}
