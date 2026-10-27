import { useEffect, useState } from 'react'
import { GitBranch, Plus, Trash2, Workflow } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'

type DraftWorkflow = {
  id: string
  name: string
  schema_version: number
  trigger: string
  created_at: number
  workflow: { steps: { id: string; kind: string; depends_on: string[] }[] }
}

const presets = {
  deploy: {
    label: 'Deploy → Health check → Notify',
    name: 'Deploy health workflow',
    steps: [
      { id: 'deploy', kind: 'deploy', depends_on: [] },
      { id: 'health', kind: 'health_check', depends_on: ['deploy'] },
      { id: 'notify', kind: 'notify', depends_on: ['health'] },
    ],
  },
  backup: {
    label: 'Backup → Notify',
    name: 'Backup workflow',
    steps: [
      { id: 'backup', kind: 'backup', depends_on: [] },
      { id: 'notify', kind: 'notify', depends_on: ['backup'] },
    ],
  },
} as const

const stepLabels: Record<string, string> = { deploy: 'استقرار', health_check: 'بررسی سلامت', backup: 'پشتیبان‌گیری', notify: 'اعلان', approval: 'تأیید انسانی' }

export function ApplicationWorkflowDraftPanel({ appId, notify }: { appId: string; notify: (message: string) => void }) {
  const [open, setOpen] = useState(false)
  const [workflows, setWorkflows] = useState<DraftWorkflow[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  async function load() {
    const result = await api<{ workflows: DraftWorkflow[] }>(`apps/${encodeURIComponent(appId)}/workflows`)
    setWorkflows(result.workflows || [])
  }

  useEffect(() => {
    if (!open) return
    let active = true
    void api<{ workflows: DraftWorkflow[] }>(`apps/${encodeURIComponent(appId)}/workflows`)
      .then((result) => { if (active) { setWorkflows(result.workflows || []); setError('') } })
      .catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : 'پیش‌نویس‌ها خوانده نشدند.') })
    return () => { active = false }
  }, [appId, open])

  async function create(kind: keyof typeof presets) {
    setBusy(true); setError('')
    try {
      const preset = presets[kind]
      await api(`apps/${encodeURIComponent(appId)}/workflows`, {
        method: 'POST',
        body: JSON.stringify({ schema_version: 1, name: preset.name, trigger: 'manual', steps: preset.steps }),
      })
      await load()
      notify('پیش‌نویس workflow ذخیره شد؛ هیچ کاری روی سرور اجرا نشد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ پیش‌نویس ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function remove(id: string) {
    setBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(appId)}/workflows/${encodeURIComponent(id)}`, { method: 'DELETE' })
      await load()
      notify('پیش‌نویس workflow حذف شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف پیش‌نویس ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <section className="rounded-xl border border-primary/15 bg-primary/[.02] p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex min-w-0 items-center gap-2"><Workflow size={15} className="shrink-0 text-primary"/><div><p className="text-xs font-medium">طراح گردش‌کار</p><p className="mt-1 text-[10px] text-muted-foreground">طرح‌های نسخه‌دار و اعتبارسنجی‌شده؛ موتور اجرا هنوز فعال نیست.</p></div></div>
      <Button size="sm" variant="outline" onClick={() => setOpen((value) => !value)} aria-expanded={open}>{open ? 'بستن' : 'مدیریت طرح‌ها'}</Button>
    </div>
    {open && <div className="mt-3 space-y-3">
      <div className="flex flex-wrap gap-2">{(Object.keys(presets) as (keyof typeof presets)[]).map((key) => <Button key={key} size="sm" variant="outline" disabled={busy} onClick={() => void create(key)}><Plus size={13}/>{presets[key].label}</Button>)}</div>
      <p className="rounded-lg border border-amber-300/15 bg-amber-300/[.035] p-2.5 text-[10px] leading-5 text-amber-100/80">این بخش فقط DAG را نگه‌داری و چرخه‌ها را رد می‌کند. trigger زمان‌بندی/Webhook، اجرای Job، retry و rollback در این نسخه پیاده نشده‌اند؛ فشردن این دکمه‌ها Deploy یا Backup انجام نمی‌دهد.</p>
      {error && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-2.5 text-xs text-rose-100">{error}</p>}
      {workflows.length ? <ul className="space-y-2">{workflows.map((workflow) => <li key={workflow.id} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-border/70 bg-background/50 p-2.5"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><p className="truncate text-xs font-medium">{workflow.name}</p><Badge variant="outline">پیش‌نویس v{workflow.schema_version}</Badge></div><p className="mt-1 flex min-w-0 items-center gap-1 truncate text-[10px] text-muted-foreground"><GitBranch size={11}/>{workflow.workflow?.steps?.map((step) => stepLabels[step.kind] || step.kind).join(' ← ') || 'طرح خالی'}</p></div><Button size="icon" variant="ghost" disabled={busy} onClick={() => void remove(workflow.id)} aria-label={`حذف پیش‌نویس ${workflow.name}`} title="حذف پیش‌نویس"><Trash2 size={14}/></Button></li>)}</ul> : <p className="rounded-lg border border-dashed border-border p-3 text-center text-[11px] text-muted-foreground">هنوز طرحی برای این اپلیکیشن ذخیره نشده است.</p>}
    </div>}
  </section>
}
