// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@symvoulio/ui/components'
import { humanQuestions } from './humanQuestions'
import { AnsweredBy } from './AnsweredBy'
import { useRunData } from '../services/useRunData'
import { decideIssue, type Run } from '../state/store'
export function HumanQuestions({ run, only }: { run: Run; only?: string }) {
  const { t } = useTranslation()
  const { snapshot } = useRunData(run.id)
  const [answers, setAnswers] = useState<Record<string, string>>({})
  const [name, setName] = useState('')
  const [pending, setPending] = useState<string | null>(null)
  const questions = humanQuestions(snapshot).filter(q => !only || q.id === only)
  if (!questions.length) return null
  return <>
    <AnsweredBy label={t('humanQuestions.answeredBy')} placeholder={t('humanQuestions.yourName')} value={name} onChange={setName} />
    <section className="my-4 space-y-2" data-testid="human-questions">
    <h2>{t('humanQuestions.title')}</h2>
    {questions.map(q => <div key={q.id} className="rounded-lg border border-[var(--line)] p-3">
      <p>{q.text}</p>
      {q.resolved ? <><span>{t('s7.resolved')}</span><p>{q.answer}</p><p>{t('humanQuestions.attribution', { name: q.answeredBy || t('humanQuestions.unknownName') })}{q.answeredAt ? t('humanQuestions.atTime', { time: q.answeredAt }) : ''}</p></> : <>
        <label>{t('humanQuestions.answer')}
          <input value={answers[q.id] ?? ''} onChange={e => setAnswers(a => ({ ...a, [q.id]: e.target.value }))} />
        </label>
        <Button disabled={!answers[q.id]?.trim() || pending !== null} onClick={async () => {
          const answer = answers[q.id]?.trim()
          if (!answer || pending) return
          setPending(q.id)
          try { await decideIssue(run.id, q.id, 'accept', answer, name.trim() || undefined) } finally { setPending(null) }
        }}>{t('s8.decide')}</Button>
      </>}
    </div>)}
  </section></>
}
