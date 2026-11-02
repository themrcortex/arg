import { useEffect, useMemo, useState } from 'react'
import { ArrowDown, ArrowUp, GitBranch, Plus, Save, Trash2, Workflow, X } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'

type WorkflowStep = { id: string; kind: string; label: string; depends_on: string[] }
type WorkflowDocument = { schema_version: number; name: string; trigger: 'manual'; steps: WorkflowStep[] }
type DraftWorkflow = WorkflowDocument & { id: string; created_at: number; updated_at: number }

const stepOptions = [
  { kind: 'deploy', label: 'استقرار', description: 'جایگاه طراحی‌شده برای اتصال به Job استقرار.' },
  { kind: 'health_check', label: 'بررسی سلامت', description: 'جایگاه بررسی endpoint یا وضعیت release.' },
  { kind: 'backup', label: 'پشتیبان‌گیری', description: 'جایگاه درخواست پشتیبان از قابلیت موجود ARG.' },
  { kind: 'approval', label: 'تأیید انسانی', description: 'نقطهٔ توقف طراحی برای تأیید مدیر.' },
  { kind: 'notify', label: 'اعلان', description: 'جایگاه اتصال آینده به سامانهٔ اعلان.' },
] as const
const stepLabels: Record<string, string> = Object.fromEntries(stepOptions.map(({ kind, label }) => [kind, label]))

function stepId() {
  return `step_${Math.random().toString(36).slice(2, 9)}`
}

function makePreset(kind: 'deploy' | 'backup'): WorkflowDocument {
  if (kind === 'deploy') return {
    schema_version: 1,
    name: 'استقرار و بررسی سلامت',
    trigger: 'manual',
    steps: [
      { id: 'deploy', kind: 'deploy', label: 'استقرار نسخه', depends_on: [] },
      { id: 'health', kind: 'health_check', label: 'بررسی سلامت', depends_on: ['deploy'] },
      { id: 'notify', kind: 'notify', label: 'ثبت اعلان نتیجه', depends_on: ['health'] },
    ],
  }
  return {
    schema_version: 1,
    name: 'پشتیبان‌گیری و ثبت نتیجه',
    trigger: 'manual',
    steps: [
      { id: 'backup', kind: 'backup', label: 'ساخت پشتیبان', depends_on: [] },
      { id: 'notify', kind: 'notify', label: 'ثبت اعلان نتیجه', depends_on: ['backup'] },
    ],
  }
}

function toDocument(workflow: DraftWorkflow): WorkflowDocument {
  return {
    schema_version: workflow.schema_version,
    name: workflow.name,
    trigger: workflow.trigger,
    steps: workflow.steps.map((step) => ({ ...step, label: step.label || stepLabels[step.kind] || step.kind })),
  }
}

export function ApplicationWorkflowDraftPanel({ appId, notify }: { appId: string; notify: (message: string) => void }) {
  const [open, setOpen] = useState(false)
  const [workflows, setWorkflows] = useState<DraftWorkflow[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<WorkflowDocument | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [loadError, setLoadError] = useState('')
  const selected = useMemo(() => workflows.find((item) => item.id === selectedId), [workflows, selectedId])

  async function load() {
    const result = await api<{ workflows: DraftWorkflow[] }>(`apps/${encodeURIComponent(appId)}/workflows`)
    setWorkflows(result.workflows || [])
  }

  useEffect(() => {
    if (!open) return
    let active = true
    void api<{ workflows: DraftWorkflow[] }>(`apps/${encodeURIComponent(appId)}/workflows`)
      .then((result) => { if (active) { setWorkflows(result.workflows || []); setLoadError('') } })
      .catch((cause) => { if (active) setLoadError(cause instanceof Error ? cause.message : 'طرح‌ها بارگذاری نشدند.') })
    return () => { active = false }
  }, [appId, open])

  function startNew(kind?: 'deploy' | 'backup') {
    setSelectedId(null)
    setDraft(kind ? makePreset(kind) : { schema_version: 1, name: 'گردش‌کار جدید', trigger: 'manual', steps: [{ id: stepId(), kind: 'health_check', label: 'بررسی سلامت', depends_on: [] }] })
    setError('')
  }

  function edit(workflow: DraftWorkflow) {
    setSelectedId(workflow.id)
    setDraft(toDocument(workflow))
    setError('')
  }

  function addStep(kind: string) {
    const option = stepOptions.find((item) => item.kind === kind)
    if (!option || !draft || draft.steps.length >= 24) return
    const previous = draft.steps.at(-1)
    setDraft({ ...draft, steps: [...draft.steps, { id: stepId(), kind, label: option.label, depends_on: previous ? [previous.id] : [] }] })
  }

  function updateStep(stepIdValue: string, patch: Partial<WorkflowStep>) {
    if (!draft) return
    setDraft({ ...draft, steps: draft.steps.map((step) => step.id === stepIdValue ? { ...step, ...patch } : step) })
  }

  function moveStep(stepIdValue: string, offset: -1 | 1) {
    if (!draft) return
    const from = draft.steps.findIndex((step) => step.id === stepIdValue)
    const to = from + offset
    if (from < 0 || to < 0 || to >= draft.steps.length) return
    const steps = [...draft.steps]
    const [step] = steps.splice(from, 1)
    steps.splice(to, 0, step)
    setDraft({ ...draft, steps })
  }

  function removeStep(stepIdValue: string) {
    if (!draft || draft.steps.length <= 1) return
    setDraft({ ...draft, steps: draft.steps.filter((step) => step.id !== stepIdValue).map((step) => ({ ...step, depends_on: step.depends_on.filter((dependency) => dependency !== stepIdValue) })) })
  }

  async function save() {
    if (!draft) return
    setBusy(true); setError('')
    try {
      const payload = { ...draft, name: draft.name.trim(), steps: draft.steps.map((step) => ({ ...step, label: step.label.trim() })) }
      if (selectedId) {
        await api(`apps/${encodeURIComponent(appId)}/workflows/${encodeURIComponent(selectedId)}`, { method: 'PUT', body: JSON.stringify(payload) })
        notify('طرح گردش‌کار ذخیره شد.')
      } else {
        await api(`apps/${encodeURIComponent(appId)}/workflows`, { method: 'POST', body: JSON.stringify(payload) })
        notify('طرح گردش‌کار ساخته شد.')
      }
      await load()
      setDraft(null); setSelectedId(null)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ طرح گردش‌کار ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function remove(id: string) {
    setBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(appId)}/workflows/${encodeURIComponent(id)}`, { method: 'DELETE' })
      await load()
      if (selectedId === id) { setSelectedId(null); setDraft(null) }
      notify('طرح گردش‌کار حذف شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف طرح ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <section className="rounded-xl border border-primary/15 bg-primary/[.02] p-3">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex min-w-0 items-center gap-2"><Workflow size={15} className="shrink-0 text-primary"/><div><p className="text-xs font-medium">طراح گردش‌کار</p><p className="mt-1 text-[10px] text-muted-foreground">گام‌ها و وابستگی‌های دستی را طراحی، اعتبارسنجی و ذخیره کن.</p></div></div>
      <Button size="sm" variant="outline" onClick={() => { setOpen((value) => !value); setDraft(null); setSelectedId(null) }} aria-expanded={open}>{open ? 'بستن' : 'طراحی گردش‌کار'}</Button>
    </div>
    {open && <div className="mt-3 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" variant="outline" disabled={busy} onClick={() => startNew()}><Plus size={13}/>طرح خالی</Button>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => startNew('deploy')}>قالب استقرار و سلامت</Button>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => startNew('backup')}>قالب پشتیبان‌گیری</Button>
        <span className="mr-auto rounded-md border border-border/70 px-2 py-1 text-[10px] text-muted-foreground">Trigger فعلی: اجرای دستی</span>
      </div>
      <div className="grid gap-3 xl:grid-cols-[minmax(220px,.8fr)_minmax(0,1.6fr)]">
        <aside className="space-y-2 rounded-lg border border-border/70 bg-background/35 p-2.5">
          <div className="flex items-center justify-between gap-2"><p className="text-xs font-medium">طرح‌های ذخیره‌شده</p><Button size="icon" variant="ghost" aria-label="تازه‌سازی طرح‌ها" disabled={busy} onClick={() => { setLoadError(''); void load().catch((cause) => setLoadError(cause instanceof Error ? cause.message : 'طرح‌ها دریافت نشدند.')) }}><GitBranch size={13}/></Button></div>
          {loadError && <p role="alert" className="text-[10px] text-rose-200">{loadError}</p>}
          {workflows.length ? <ul className="space-y-1.5">{workflows.map((workflow) => <li key={workflow.id} className={`flex items-start gap-1 rounded-md border p-2 ${selectedId === workflow.id ? 'border-primary/35 bg-primary/[.06]' : 'border-border/60'}`}><button type="button" onClick={() => edit(workflow)} className="min-w-0 flex-1 text-right"><span className="block truncate text-[11px] font-medium">{workflow.name}</span><span className="mt-1 block text-[9px] text-muted-foreground">{workflow.steps.length} گام · به‌روزرسانی {new Date(workflow.updated_at * 1000).toLocaleDateString('fa-IR')}</span></button><Button size="icon" variant="ghost" disabled={busy} onClick={() => void remove(workflow.id)} aria-label={`حذف ${workflow.name}`}><Trash2 size={12}/></Button></li>)}</ul> : <p className="rounded-md border border-dashed border-border p-2 text-center text-[10px] text-muted-foreground">هنوز طرحی ذخیره نشده است.</p>}
        </aside>
        <div className="min-w-0 space-y-3">
          {draft ? <div className="space-y-3 rounded-lg border border-border/70 bg-background/35 p-3">
            <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">{selected ? 'ویرایش طرح' : 'طرح جدید'}</p><p className="mt-1 text-[10px] text-muted-foreground">وابستگی هر گام را با تیک‌زدن گام‌های پیشین مشخص کن؛ گراف چرخه‌دار ذخیره نمی‌شود.</p></div><Badge variant="outline">{draft.steps.length}/24 گام</Badge></div>
            <div className="grid gap-2 sm:grid-cols-[1fr_180px]"><label className="space-y-1 text-[10px] text-muted-foreground">نام گردش‌کار<Input maxLength={80} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })}/></label><label className="space-y-1 text-[10px] text-muted-foreground">Trigger<select value={draft.trigger} disabled className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs"><option value="manual">اجرای دستی</option></select></label></div>
            <div className="flex flex-wrap gap-1.5">{stepOptions.map((option) => <Button key={option.kind} type="button" size="sm" variant="outline" disabled={busy || draft.steps.length >= 24} onClick={() => addStep(option.kind)}><Plus size={12}/>{option.label}</Button>)}</div>
            <ol className="space-y-2">{draft.steps.map((step, index) => <li key={step.id} className="relative rounded-lg border border-border/70 bg-card/50 p-2.5">
              {index > 0 && <span aria-hidden="true" className="absolute -top-2 right-5 h-2 border-r border-dashed border-primary/50"/>}
              <div className="flex flex-wrap items-center gap-2"><span className="grid h-6 w-6 place-items-center rounded-md border border-primary/20 bg-primary/[.06] text-[10px] text-primary">{index + 1}</span><label className="min-w-[145px] flex-1 space-y-1 text-[9px] text-muted-foreground">نام گام<Input maxLength={80} value={step.label} onChange={(event) => updateStep(step.id, { label: event.target.value })}/></label><label className="min-w-[145px] flex-1 space-y-1 text-[9px] text-muted-foreground">نوع گام<select value={step.kind} onChange={(event) => { const option = stepOptions.find((item) => item.kind === event.target.value); updateStep(step.id, { kind: event.target.value, label: option?.label || step.label }) }} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs">{stepOptions.map((option) => <option key={option.kind} value={option.kind}>{option.label}</option>)}</select></label><Button type="button" size="icon" variant="ghost" disabled={busy || index === 0} onClick={() => moveStep(step.id, -1)} aria-label={`انتقال گام ${step.label} به بالا`}><ArrowUp size={14}/></Button><Button type="button" size="icon" variant="ghost" disabled={busy || index === draft.steps.length - 1} onClick={() => moveStep(step.id, 1)} aria-label={`انتقال گام ${step.label} به پایین`}><ArrowDown size={14}/></Button><Button type="button" size="icon" variant="ghost" disabled={busy || draft.steps.length <= 1} onClick={() => removeStep(step.id)} aria-label={`حذف گام ${step.label}`}><X size={14}/></Button></div>
              <p className="mt-2 text-[9px] leading-4 text-muted-foreground">{stepOptions.find((item) => item.kind === step.kind)?.description}</p>
              <fieldset className="mt-2 rounded-md border border-border/60 p-2"><legend className="px-1 text-[9px] text-muted-foreground">پس از این گام‌ها اجرا شود</legend><div className="flex flex-wrap gap-x-3 gap-y-1">{draft.steps.filter((candidate) => candidate.id !== step.id).map((candidate) => <label key={candidate.id} className="flex items-center gap-1 text-[9px] text-muted-foreground"><input type="checkbox" checked={step.depends_on.includes(candidate.id)} onChange={(event) => updateStep(step.id, { depends_on: event.target.checked ? [...step.depends_on, candidate.id] : step.depends_on.filter((id) => id !== candidate.id) })}/>{candidate.label || stepLabels[candidate.kind] || candidate.kind}</label>)}</div></fieldset>
            </li>)}</ol>
            {error && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-2 text-xs text-rose-100">{error}</p>}
            <div className="flex flex-wrap justify-end gap-2"><Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => { setDraft(null); setSelectedId(null); setError('') }}>انصراف</Button><Button size="sm" disabled={busy || !draft.name.trim()} onClick={() => void save()}><Save size={13}/>{busy ? 'در حال ذخیره…' : selected ? 'ذخیرهٔ تغییرات' : 'ذخیرهٔ طرح'}</Button></div>
          </div> : selected ? <div className="rounded-lg border border-border/70 bg-background/35 p-3"><p className="text-xs font-medium">{selected.name}</p><p className="mt-2 text-[10px] text-muted-foreground">گراف ذخیره‌شده را انتخاب کن تا ویرایشش کنی.</p><Button size="sm" variant="outline" className="mt-2" onClick={() => edit(selected)}>ویرایش</Button></div> : <div className="grid min-h-36 place-items-center rounded-lg border border-dashed border-border px-4 text-center text-[11px] leading-6 text-muted-foreground">یک قالب یا طرح خالی انتخاب کن، گام‌ها و اتصال‌های DAG را تنظیم کن و ذخیره بزن.</div>}
        </div>
      </div>
      <p className="rounded-lg border border-amber-300/15 bg-amber-300/[.035] p-2.5 text-[10px] leading-5 text-amber-100/80">این نسخه طراح و اعتبارسنج گراف را کامل می‌کند، اما اجرای workflow، زمان‌بندی، Webhook و retry خودکار هنوز فعال نیستند. ذخیره یا ویرایش طرح هیچ دستور، Deploy، Backup یا اعلان واقعی اجرا نمی‌کند؛ هر نوع گام فقط تعریف طرح است.</p>
    </div>}
  </section>
}
