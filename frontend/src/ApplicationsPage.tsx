import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Archive, CircleAlert, CircleHelp, CirclePlus, Code2, Database, Globe2, Layers3, Pencil, RefreshCw, RotateCcw, Search, Server, Upload, X } from 'lucide-react'
import { api, apiForWorkspace } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'
import { Skeleton } from './components/ui/skeleton'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type AppRecord = {
  id: string
  owner_server_uuid: string
  name: string
  description: string
  framework: string
  runtime_version: string
  build_runtime_version: string
  source_type: string
  domain: string | null
  listen_port: number | null
  labels: string[]
  desired_state: 'none' | 'running' | 'stopped'
  observed_state: 'unknown' | 'running' | 'stopped' | 'failed' | 'offline'
  observed_at: number | null
  lifecycle_state: string
  reconcile_policy: string
  version: number
  created_by: string
  created_at: number
  updated_at: number
  deleted_at: number | null
}
type Registry = { server_uuid: string; apps: AppRecord[]; features: { deploy: boolean; environment_secrets: boolean; storage_plans: boolean; automatic_reconciliation: boolean } }
type NodeRef = { id: number; workspace_key: string; name: string; public_name?: string; endpoint: string; workspace_ready?: boolean }
type RegistryRow = { app: AppRecord; serverUuid: string; serverName: string; workspaceKey: string; local: boolean }
type ServerError = { key: string; name: string; workspaceKey: string; message: string }
type Draft = { name: string; description: string; framework: string; runtime_version: string; build_runtime_version: string; source_type: string; domain: string; listen_port: string; labels: string; desired_state: string }
type RuntimeInventory = { runtimes: { family: string; state: string; versions: { version: string; fpm_installed?: boolean }[] }[] }
type DeployStep = { sequence: number; step: string; state: string; message: string; started_at: number; finished_at: number | null }
type DeployJob = { id: string; application_id: string; state: string; attempt: number; created_at: number; started_at: number | null; finished_at: number | null; release_path: string | null; previous_release: string | null; error: string | null; steps: DeployStep[] }

const emptyDraft: Draft = { name: '', description: '', framework: 'custom', runtime_version: 'system', build_runtime_version: 'system', source_type: 'manual', domain: '', listen_port: '', labels: '', desired_state: 'none' }
const frameworkLabels: Record<string, string> = { laravel: 'Laravel', node: 'Node.js', python: 'Python', wordpress: 'WordPress', static: 'سایت استاتیک', custom: 'سفارشی / تشخیص‌داده‌نشده' }
const sourceLabels: Record<string, string> = { manual: 'ثبت دستی', git: 'مخزن Git', archive: 'فایل آرشیو', docker: 'Docker', 'existing-directory': 'پوشهٔ موجود', image: 'Image' }

function asDraft(app?: AppRecord): Draft {
  return app ? { name: app.name, description: app.description, framework: app.framework, runtime_version: app.runtime_version || 'system', build_runtime_version: app.build_runtime_version || 'system', source_type: app.source_type, domain: app.domain || '', listen_port: app.listen_port ? String(app.listen_port) : '', labels: app.labels.join(', '), desired_state: app.desired_state } : { ...emptyDraft }
}
function draftPayload(draft: Draft) {
  return { name: draft.name.trim(), description: draft.description.trim(), framework: draft.framework, runtime_version: draft.runtime_version, build_runtime_version: draft.build_runtime_version, source_type: draft.source_type, domain: draft.domain.trim(), listen_port: draft.listen_port.trim() ? Number(draft.listen_port) : null, labels: draft.labels.split(',').map((label) => label.trim()).filter(Boolean), desired_state: draft.desired_state }
}
function runtimeFamilyForFramework(framework: string) {
  if (['laravel', 'wordpress'].includes(framework)) return 'php'
  if (framework === 'node') return 'node'
  if (framework === 'python') return 'python'
  return null
}
function newIdempotencyKey() {
  return typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}-${Math.random().toString(36).slice(2)}`
}

function DeploymentPanel({ app, notify }: { app: AppRecord; notify: (message: string) => void }) {
  const [jobs, setJobs] = useState<DeployJob[]>([])
  const [mode, setMode] = useState<'git' | 'directory' | 'archive'>('git')
  const [repository, setRepository] = useState('')
  const [reference, setReference] = useState('main')
  const [directory, setDirectory] = useState('')
  const [healthcheck, setHealthcheck] = useState(app.framework === 'static' ? 'index.html' : '')
  const [archive, setArchive] = useState<File | null>(null)
  const [rollbackTarget, setRollbackTarget] = useState<DeployJob | null>(null)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const active = jobs.some((job) => ['queued', 'validating', 'running', 'verifying'].includes(job.state))

  const loadJobs = useCallback(async () => {
    try {
      const result = await api<{ jobs: DeployJob[] }>(`apps/${encodeURIComponent(app.id)}/deploy/jobs`)
      setJobs(result.jobs)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'وضعیت استقرار خوانده نشد.')
    }
  }, [app.id])

  useEffect(() => { void loadJobs() }, [loadJobs])
  useEffect(() => {
    if (!active) return
    const timer = window.setInterval(() => void loadJobs(), 1200)
    return () => window.clearInterval(timer)
  }, [active, loadJobs])

  async function start(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      const key = newIdempotencyKey()
      if (mode === 'archive') {
        if (!archive) throw new Error('یک فایل ZIP انتخاب کن.')
        const result = await api<DeployJob>(`apps/${encodeURIComponent(app.id)}/deploy/archive`, { method: 'POST', body: archive, headers: { 'Content-Type': 'application/zip', 'Idempotency-Key': key } })
        setJobs((current) => [result, ...current.filter((item) => item.id !== result.id)])
      } else {
        const source = mode === 'git' ? { kind: 'git', repository: repository.trim(), reference: reference.trim() } : { kind: 'directory', path: directory.trim() }
        const result = await api<DeployJob>(`apps/${encodeURIComponent(app.id)}/deploy`, { method: 'POST', body: JSON.stringify({ source, idempotency_key: key, healthcheck_file: healthcheck.trim() || null }) })
        setJobs((current) => [result, ...current.filter((item) => item.id !== result.id)])
      }
      setOpen(false); notify('Job استقرار ثبت شد؛ تا پایان از همین کارت قابل پیگیری است.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت Job ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function action(job: DeployJob, kind: 'cancel' | 'retry') {
    setBusy(true); setError('')
    try {
      const result = kind === 'cancel'
        ? await api<{ job: DeployJob }>(`deploy/jobs/${encodeURIComponent(job.id)}/cancel`, { method: 'POST' }).then((value) => value.job)
        : await api<DeployJob>(`deploy/jobs/${encodeURIComponent(job.id)}/retry`, { method: 'POST', body: JSON.stringify({ idempotency_key: newIdempotencyKey() }) })
      setJobs((current) => [result, ...current.filter((item) => item.id !== result.id)])
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'عملیات Job ناموفق بود.') }
    finally { setBusy(false); void loadJobs() }
  }

  async function rollback(job: DeployJob) {
    const releaseId = job.previous_release?.split('/').at(-1)
    if (!releaseId) return
    setRollbackTarget(job)
  }

  async function confirmRollback() {
    const job = rollbackTarget
    const releaseId = job?.previous_release?.split('/').at(-1)
    if (!job || !releaseId) return
    setBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/deploy/rollback`, { method: 'POST', body: JSON.stringify({ release_id: releaseId }) })
      notify('اشاره‌گر نسخه به release قبلی برگشت.')
      setRollbackTarget(null); await loadJobs()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Rollback ناموفق بود.') }
    finally { setBusy(false) }
  }

  const labels: Record<string, string> = { queued: 'در صف', validating: 'اعتبارسنجی', running: 'در حال دریافت', verifying: 'بررسی و فعال‌سازی', succeeded: 'موفق', failed: 'ناموفق', cancelled: 'لغوشده' }
  return <section className="mt-3 rounded-xl border border-primary/15 bg-primary/[.025] p-3">
    <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">انتشار منبع و مدیریت release</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">منبع را در پوشهٔ جدا آماده می‌کند و اشاره‌گر release را اتمی جابه‌جا می‌کند؛ اجرای framework یا تغییر Nginx هنوز انجام نمی‌شود.</p></div><Button size="sm" variant="outline" onClick={() => { setError(''); setOpen((value) => !value) }}><Upload size={13}/>{open ? 'بستن' : 'استقرار منبع'}</Button></div>
    {open && <form onSubmit={(event) => void start(event)} className="mt-3 space-y-3 rounded-lg border border-border/70 bg-background/40 p-3">
      <div className="flex flex-wrap gap-2" role="tablist" aria-label="نوع منبع">{([['git','Git'],['directory','پوشهٔ VPS'],['archive','فایل ZIP']] as const).map(([value,label]) => <Button type="button" size="sm" variant={mode === value ? 'default' : 'outline'} key={value} onClick={() => setMode(value)}>{label}</Button>)}</div>
      {mode === 'git' && <div className="grid gap-2 sm:grid-cols-2"><label className="space-y-1 text-[11px] text-muted-foreground">مخزن عمومی HTTPS<Input dir="ltr" required value={repository} onChange={(event) => setRepository(event.target.value)} placeholder="https://github.com/org/project.git"/></label><label className="space-y-1 text-[11px] text-muted-foreground">Branch / tag / commit<Input dir="ltr" required value={reference} onChange={(event) => setReference(event.target.value)} placeholder="main یا commit SHA"/></label><p className="sm:col-span-2 text-[10px] leading-5 text-muted-foreground">فقط GitHub، GitLab، Bitbucket و Codeberg؛ URL دارای رمز، SSH key یا credential پذیرفته نیست. هیچ build hook یا اسکریپت مخزن اجرا نمی‌شود.</p></div>}
      {mode === 'directory' && <label className="block space-y-1 text-[11px] text-muted-foreground">مسیر پوشهٔ سرور<Input dir="ltr" required value={directory} onChange={(event) => setDirectory(event.target.value)} placeholder="/var/www/site"/><span className="block text-[10px]">مسیر باید زیر /var/www یا /opt/arg/apps باشد؛ symlink رد می‌شود.</span></label>}
      {mode === 'archive' && <label className="block space-y-1 text-[11px] text-muted-foreground">ZIP پروژه<input type="file" accept=".zip,application/zip" required onChange={(event) => setArchive(event.target.files?.[0] || null)} className="block w-full rounded-md border border-input bg-background p-2 text-xs file:ml-2 file:rounded-md file:border-0 file:bg-primary file:px-2 file:py-1"/><span className="block text-[10px]">حداکثر ۲۵۶ MiB؛ محتوای بازشده حداکثر ۲ GiB. مسیرهای traversal، symlink و فایل‌های خاص رد می‌شوند.</span></label>}
      {mode !== 'archive' && <label className="block space-y-1 text-[11px] text-muted-foreground">فایل بررسی اختیاری (نسبی به ریشهٔ منبع)<Input dir="ltr" value={healthcheck} onChange={(event) => setHealthcheck(event.target.value)} placeholder="index.html"/></label>}
      <p className="rounded-lg border border-amber-300/15 bg-amber-300/[.04] p-2 text-[10px] leading-5 text-amber-100/80">فایل‌های محیطی مثل <code dir="ltr">.env</code>، پوشهٔ <code dir="ltr">.git</code> و <code dir="ltr">node_modules</code> وارد release نمی‌شوند. نسخهٔ قبلی روی دیسک می‌ماند و هیچ سرویس زنده‌ای restart نمی‌شود.</p>
      {error && <p role="alert" className="text-xs text-rose-200">{error}</p>}
      <div className="flex justify-end gap-2"><Button type="button" size="sm" variant="outline" onClick={() => setOpen(false)} disabled={busy}>انصراف</Button><Button size="sm" disabled={busy}>{busy ? 'در حال ثبت…' : 'ساخت Job'}</Button></div>
    </form>}
    {error && !open && <p role="alert" className="mt-2 text-xs text-rose-200">{error}</p>}
    {jobs.length > 0 && <div className="mt-3 space-y-2">{jobs.slice(0,3).map((job) => <details key={job.id} className="rounded-lg border border-border/70 bg-background/35 p-2"><summary className="flex cursor-pointer list-none flex-wrap items-center justify-between gap-2 text-xs"><span className="font-medium">{labels[job.state] || job.state}</span><span dir="ltr" className="font-mono text-[10px] text-muted-foreground">{job.id.slice(0,8)} · {new Date(job.created_at * 1000).toLocaleString('fa-IR')}</span></summary><div className="mt-2 space-y-1.5">{job.steps.map((step) => <div key={`${job.id}:${step.sequence}`} className="flex items-start gap-2 border-r border-border/70 pr-2 text-[10px]"><span className={step.state === 'running' ? 'text-amber-200' : step.state === 'failed' ? 'text-rose-200' : 'text-emerald-200'}>●</span><div><p className="font-medium">{step.step}</p><p className="leading-5 text-muted-foreground">{step.message}</p></div></div>)}{job.error && <p className="rounded bg-rose-400/[.05] p-2 text-[10px] text-rose-200">{job.error}</p>}{job.release_path && <p dir="ltr" className="break-all text-left font-mono text-[10px] text-muted-foreground">{job.release_path}</p>}<div className="flex justify-end gap-2">{['queued','validating','running','verifying'].includes(job.state) ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => void action(job,'cancel')}><X size={12}/>لغو Job</Button> : <>{job.state === 'failed' && mode !== 'archive' && <Button size="sm" variant="outline" disabled={busy} onClick={() => void action(job,'retry')}><RotateCcw size={12}/>اجرای دوباره</Button>}{job.state === 'succeeded' && job.previous_release && <Button size="sm" variant="outline" disabled={busy} onClick={() => void rollback(job)}><RotateCcw size={12}/>بازگشت به نسخهٔ قبلی</Button>}</>}</div></div></details>)}</div>}
    <Dialog open={!!rollbackTarget} onOpenChange={(isOpen) => { if (!isOpen && !busy) setRollbackTarget(null) }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>بازگشت به release قبلی؟</DialogTitle><DialogDescription>اشاره‌گر release به نسخهٔ پیشین تغییر می‌کند. فایل‌ها حذف نمی‌شوند و سرویس یا Nginx راه‌اندازی مجدد نمی‌شود؛ بررسی سلامت HTTP هم در این فاز اجرا نمی‌شود.</DialogDescription></DialogHeader>{error && <p role="alert" className="text-xs text-rose-200">{error}</p>}<div className="flex justify-end gap-2"><Button variant="outline" disabled={busy} onClick={() => setRollbackTarget(null)}>انصراف</Button><Button disabled={busy} onClick={() => void confirmRollback()}>{busy ? 'در حال بازگشت…' : 'تأیید بازگشت'}</Button></div></DialogContent></Dialog>
  </section>
}

export function ApplicationsPage({ Heading: PageHeading, notify, allServers, workspaceKey, onOpenWorkspace }: { Heading: Heading; notify: (message: string) => void; allServers: boolean; workspaceKey: string; onOpenWorkspace: (key: string) => void }) {
  const [rows, setRows] = useState<RegistryRow[]>([])
  const [serverErrors, setServerErrors] = useState<ServerError[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [query, setQuery] = useState('')
  const [frameworkFilter, setFrameworkFilter] = useState('all')
  const [showArchived, setShowArchived] = useState(false)
  const [dialog, setDialog] = useState<'form' | 'delete' | 'restore' | null>(null)
  const [target, setTarget] = useState<AppRecord | null>(null)
  const [draft, setDraft] = useState<Draft>({ ...emptyDraft })
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [runtimeInventory, setRuntimeInventory] = useState<RuntimeInventory | null>(null)
  const [runtimeInventoryError, setRuntimeInventoryError] = useState('')
  const [runtimeInventoryLoading, setRuntimeInventoryLoading] = useState(false)

  const load = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true)
    setRefreshing(true)
    setError('')
    try {
      if (!allServers) {
        const registry = await api<Registry>(`apps${showArchived ? '?include_deleted=true' : ''}`)
        setRows(registry.apps.map((app) => ({ app, serverUuid: registry.server_uuid, serverName: 'این VPS', workspaceKey, local: !workspaceKey })))
        setServerErrors([])
        return
      }
      const [registry, local, nodeRegistry] = await Promise.all([
        api<Registry>('apps'),
        api<{ hostname: string }>('dashboard'),
        api<{ public_name?: string; nodes: NodeRef[] }>('nodes'),
      ])
      const result: RegistryRow[] = registry.apps.map((app) => ({ app, serverUuid: registry.server_uuid, serverName: nodeRegistry.public_name || local.hostname || 'این VPS', workspaceKey: '', local: true }))
      const failures: ServerError[] = []
      const nodes = nodeRegistry.nodes || []
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(4, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          const serverName = node.public_name || node.name
          if (!node.workspace_ready) { failures[index] = { key: node.workspace_key, name: serverName, workspaceKey: node.workspace_key, message: 'برای خواندن فهرست اپلیکیشن‌ها، اتصال فضای کار این Node را دوباره جفت کن.' }; continue }
          try {
            const remote = await apiForWorkspace<Registry>(node.workspace_key, `apps${showArchived ? '?include_deleted=true' : ''}`, { signal: AbortSignal.timeout(8000) })
            for (const app of remote.apps) result.push({ app, serverUuid: remote.server_uuid, serverName, workspaceKey: node.workspace_key, local: false })
          } catch (cause) {
            failures[index] = { key: node.workspace_key, name: serverName, workspaceKey: node.workspace_key, message: cause instanceof Error ? cause.message : 'این VPS پاسخ نداد.' }
          }
        }
      }))
      setRows(result)
      setServerErrors(failures.filter(Boolean))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'فهرست اپلیکیشن‌ها خوانده نشد.')
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [allServers, workspaceKey, showArchived])

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (dialog !== 'form') return
    let active = true
    setRuntimeInventoryLoading(true)
    void api<RuntimeInventory>('runtimes')
      .then((inventory) => { if (active) { setRuntimeInventory(inventory); setRuntimeInventoryError('') } })
      .catch((cause) => { if (active) setRuntimeInventoryError(cause instanceof Error ? cause.message : 'فهرست Runtimeهای نصب‌شده دریافت نشد.') })
      .finally(() => { if (active) setRuntimeInventoryLoading(false) })
    return () => { active = false }
  }, [dialog])

  const visibleRows = useMemo(() => rows.filter(({ app }) => {
    const search = `${app.name} ${app.description} ${app.domain || ''} ${app.labels.join(' ')} ${frameworkLabels[app.framework] || app.framework}`.toLocaleLowerCase()
    return (frameworkFilter === 'all' || app.framework === frameworkFilter) && search.includes(query.trim().toLocaleLowerCase())
  }), [rows, query, frameworkFilter])
  const requestedRuntimeFamily = runtimeFamilyForFramework(draft.framework)
  const availableRuntimeVersions = runtimeInventory?.runtimes.find((runtime) => runtime.family === requestedRuntimeFamily)?.versions.filter((item) => requestedRuntimeFamily !== 'php' || item.fpm_installed).map((item) => item.version) || []

  function beginCreate() { setTarget(null); setDraft({ ...emptyDraft }); setError(''); setDialog('form') }
  function beginEdit(row: RegistryRow) { setTarget(row.app); setDraft(asDraft(row.app)); setError(''); setDialog('form') }

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    setSaving(true); setError('')
    try {
      if (target) {
        await api(`apps/${encodeURIComponent(target.id)}`, { method: 'PUT', body: JSON.stringify({ ...draftPayload(draft), expected_version: target.version }) })
        notify('مشخصات اپلیکیشن به‌روز شد؛ وضعیت اجرای سرویس تغییر نکرده است.')
      } else {
        await api('apps', { method: 'POST', body: JSON.stringify(draftPayload(draft)) })
        notify('رکورد اپلیکیشن ثبت شد؛ هیچ کد یا سرویسی اجرا نشده است.')
      }
      setDialog(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function archiveApp() {
    if (!target) return
    setSaving(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(target.id)}`, { method: 'DELETE', body: JSON.stringify({ expected_version: target.version }) })
      notify('رکورد اپلیکیشن آرشیو شد؛ فایل‌ها و سرویس‌ها دست‌نخورده ماندند.')
      setDialog(null); setTarget(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'آرشیو اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function restoreApp() {
    if (!target) return
    setSaving(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(target.id)}/restore`, { method: 'POST', body: JSON.stringify({ expected_version: target.version }) })
      notify('رکورد اپلیکیشن بازیابی شد.')
      setDialog(null); setTarget(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'بازیابی اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  const action = <div className="flex flex-wrap gap-2">{!allServers && <Button onClick={beginCreate}><CirclePlus size={15}/>ثبت اپلیکیشن</Button>}<Button variant="outline" onClick={() => void load(true)} disabled={refreshing}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button></div>

  return <>
    <PageHeading title="اپلیکیشن‌ها" description={allServers ? 'فهرست خواندنی اپلیکیشن‌های ثبت‌شده روی VPSهای متصل؛ عملیات هر رکورد در فضای کار میزبان انجام می‌شود.' : 'فهرست مستقل اپلیکیشن‌ها و وضعیت مالکیت آن‌ها روی همین VPS.'} action={action}/>
    <Card className="mb-5 border-[#dda46f]/15 bg-card/55"><CardContent className="flex items-start gap-3 p-4"><CircleHelp className="mt-0.5 shrink-0 text-primary" size={18}/><p className="text-xs leading-6 text-muted-foreground">از هر کارت می‌توان Git عمومی، ZIP یا پوشهٔ مجاز VPS را به یک release جدا وارد کرد. Job، مرحله‌ها و نتیجه پس از refresh باقی می‌مانند و لغو/اجرای دوباره دارد. در این فاز اسکریپت build، اجرای runtime، تغییر Nginx و restart سرویس انجام نمی‌شود؛ اشاره‌گر release فقط پس از اعتبارسنجی اتمی جابه‌جا می‌شود و نسخهٔ قبلی حفظ می‌ماند.</p></CardContent></Card>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
    {allServers && serverErrors.length > 0 && <Card className="mb-4 border-amber-400/20 bg-amber-400/[.04]"><CardContent className="space-y-2 p-4"><p className="flex items-center gap-2 text-sm font-medium text-amber-100"><CircleAlert size={15}/>بعضی VPSها قابل خواندن نبودند</p>{serverErrors.map((item) => <div key={item.key} className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"><span>{item.name}: {item.message}</span><Button size="sm" variant="outline" onClick={() => onOpenWorkspace(item.workspaceKey)}>باز کردن VPS</Button></div>)}</CardContent></Card>}
    <div className="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-card/35 p-3">
      <div className="relative min-w-[190px] flex-1"><Search size={15} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجوی نام، دامنه یا برچسب" className="pr-9"/></div>
      <select value={frameworkFilter} onChange={(event) => setFrameworkFilter(event.target.value)} aria-label="فیلتر چارچوب" className="h-10 rounded-md border border-input bg-background px-3 text-xs"><option value="all">همهٔ چارچوب‌ها</option>{Object.entries(frameworkLabels).map(([key, label]) => <option value={key} key={key}>{label}</option>)}</select>
      <label className="flex h-10 items-center gap-2 rounded-md border border-input px-3 text-xs text-muted-foreground"><input type="checkbox" checked={showArchived} onChange={(event) => setShowArchived(event.target.checked)} className="accent-[#dda46f]"/>نمایش آرشیوی‌ها</label>
    </div>
    {loading ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{[0,1,2].map((item) => <Skeleton key={item} className="h-56 rounded-2xl"/>)}</div> : visibleRows.length === 0 ? <Card className="border-dashed"><CardContent className="flex min-h-48 flex-col items-center justify-center gap-3 p-6 text-center"><Layers3 size={26} className="text-primary"/><p className="font-medium">{showArchived ? 'رکوردی با این فیلتر پیدا نشد' : 'هنوز اپلیکیشنی ثبت نشده است'}</p><p className="max-w-lg text-xs leading-6 text-muted-foreground">اپلیکیشن یک موجودیت مدیریتی جدا از فایل Nginx یا کانتینر Docker است. ابتدا مشخصاتش را ثبت کن؛ دریافت سورس و releaseهای پایدار از کارت همان اپ انجام می‌شود.</p>{!allServers && <Button onClick={beginCreate}><CirclePlus size={15}/>ثبت اولین اپلیکیشن</Button>}</CardContent></Card> : <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{visibleRows.map(({ app, serverName, workspaceKey: ownerWorkspace, local }) => <Card key={`${app.id}:${app.owner_server_uuid || ''}`} className={`overflow-hidden ${app.deleted_at ? 'opacity-75' : ''}`}><CardHeader className="pb-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><CardTitle className="flex items-center gap-2 text-base"><Code2 size={17} className="shrink-0 text-primary"/><span className="truncate">{app.name}</span></CardTitle><CardDescription className="mt-2 flex items-center gap-1.5 truncate"><Server size={12}/>{serverName}</CardDescription></div><Badge variant={app.deleted_at ? 'outline' : 'success'}>{app.deleted_at ? 'آرشیو' : 'ثبت‌شده'}</Badge></div></CardHeader><CardContent className="space-y-3"><div className="grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">قالب</p><p className="mt-1 truncate">{frameworkLabels[app.framework] || app.framework}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">Runtime درخواستی</p><p className="mt-1 truncate">{app.runtime_version === 'system' ? 'نسخهٔ پیش‌فرض سیستم' : app.runtime_version}</p>{app.framework === 'node' && <p className="mt-1 truncate text-[10px] text-muted-foreground">ساخت: {app.build_runtime_version === 'system' ? 'نسخهٔ سیستم' : app.build_runtime_version}</p>}</div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">منبع انتخاب‌شده</p><p className="mt-1 truncate">{sourceLabels[app.source_type] || app.source_type}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت هدف</p><p className="mt-1">{app.desired_state === 'none' ? 'تنظیم نشده' : app.desired_state === 'running' ? 'درخواست اجرا' : 'درخواست توقف'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت واقعی</p><p className="mt-1">{app.observed_state === 'unknown' ? 'هنوز مشاهده نشده' : app.observed_state}</p></div></div>{app.domain && <p dir="ltr" className="flex items-center justify-end gap-1.5 truncate text-left font-mono text-xs text-muted-foreground"><Globe2 size={13}/>{app.domain}</p>}{app.listen_port && <p dir="ltr" className="text-left font-mono text-[11px] text-muted-foreground">TCP :{app.listen_port}</p>}{app.description && <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">{app.description}</p>}{app.labels.length > 0 && <div className="flex flex-wrap gap-1">{app.labels.map((label) => <Badge key={label} variant="outline" className="text-[10px]">{label}</Badge>)}</div>}<div className="flex items-center justify-between gap-2 border-t border-border/60 pt-3">{allServers && !local ? <Button size="sm" variant="outline" className="w-full" onClick={() => onOpenWorkspace(ownerWorkspace)}>رفتن به فضای کار میزبان</Button> : <span className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground"><Database size={12}/>منابع زنده هنوز متصل نیستند</span>}{!allServers && <div className="flex shrink-0 gap-1">{app.deleted_at ? <Button size="icon" variant="ghost" title="بازیابی رکورد" aria-label="بازیابی رکورد" onClick={() => { setTarget(app); setError(''); setDialog('restore') }}><RotateCcw size={15}/></Button> : <><Button size="icon" variant="ghost" title="ویرایش مشخصات" aria-label="ویرایش مشخصات" onClick={() => beginEdit({ app, serverUuid: '', serverName, workspaceKey: '', local: true })}><Pencil size={15}/></Button><Button size="icon" variant="ghost" className="text-rose-300" title="آرشیو رکورد" aria-label="آرشیو رکورد" onClick={() => { setTarget(app); setError(''); setDialog('delete') }}><Archive size={15}/></Button></>}</div>}</div>{!allServers && !app.deleted_at && <DeploymentPanel app={app} notify={notify}/>}</CardContent></Card>)}</div>}
    <Dialog open={dialog === 'form'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-h-[92dvh] max-w-2xl overflow-y-auto"><DialogHeader><DialogTitle>{target ? 'ویرایش رکورد اپلیکیشن' : 'ثبت اپلیکیشن'}</DialogTitle><DialogDescription>نسخهٔ Runtime درخواستی را از نسخه‌های نصب‌شده انتخاب کن. این فاز نیازمندی و پیش‌بررسی را ثبت می‌کند؛ اجرای خودکار سرویس در فاز Runner انجام می‌شود.</DialogDescription></DialogHeader><form onSubmit={(event) => void submit(event)} className="space-y-4"><div className="grid gap-4 sm:grid-cols-2"><div className="space-y-2"><Label htmlFor="app-name">نام</Label><Input id="app-name" required maxLength={80} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-framework">قالب پیشنهادی</Label><select id="app-framework" value={draft.framework} onChange={(event) => setDraft({ ...draft, framework: event.target.value, runtime_version: 'system', build_runtime_version: 'system' })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(frameworkLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div>{requestedRuntimeFamily && <div className="space-y-2"><Label htmlFor="app-runtime-version">نسخهٔ Runtime موردنیاز</Label><select id="app-runtime-version" value={draft.runtime_version} onChange={(event) => setDraft({ ...draft, runtime_version: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="system">نسخهٔ پیش‌فرض سیستم</option>{availableRuntimeVersions.map((version) => <option key={version} value={version}>{version}</option>)}{draft.runtime_version !== 'system' && !availableRuntimeVersions.includes(draft.runtime_version) && <option value={draft.runtime_version}>{draft.runtime_version} · در inventory فعلی پیدا نشد</option>}</select><p className="text-[11px] leading-5 text-muted-foreground">{runtimeInventoryLoading ? 'در حال خواندن inventory نصب‌شدهٔ همین VPS…' : runtimeInventoryError || (availableRuntimeVersions.length ? `نسخه‌های نصب‌شدهٔ ${requestedRuntimeFamily} از inventory همین VPS آمده‌اند.` : `Runtime نصب‌شده برای ${requestedRuntimeFamily} پیدا نشد؛ پک نسخه را از مدیریت APT نصب و فرم را دوباره باز کن.`)}</p></div>}{draft.framework === 'node' && <div className="space-y-2"><Label htmlFor="app-build-runtime-version">نسخهٔ Node.js برای build</Label><select id="app-build-runtime-version" value={draft.build_runtime_version} onChange={(event) => setDraft({ ...draft, build_runtime_version: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="system">نسخهٔ پیش‌فرض سیستم</option>{availableRuntimeVersions.map((version) => <option key={version} value={version}>{version}</option>)}{draft.build_runtime_version !== 'system' && !availableRuntimeVersions.includes(draft.build_runtime_version) && <option value={draft.build_runtime_version}>{draft.build_runtime_version} · در inventory فعلی پیدا نشد</option>}</select><p className="text-[11px] leading-5 text-muted-foreground">نسخهٔ build جدا از نسخهٔ runtime ذخیره می‌شود تا مراحل بعد بتوانند lockfile و موتور ساخت پروژه را با هم بررسی کنند.</p></div>}<div className="space-y-2"><Label htmlFor="app-source">منبع پیش‌فرض استقرار</Label><select id="app-source" value={draft.source_type} onChange={(event) => setDraft({ ...draft, source_type: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(sourceLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div><div className="space-y-2"><Label htmlFor="app-domain">دامنهٔ رزروشده</Label><Input id="app-domain" dir="ltr" placeholder="app.example.com" value={draft.domain} onChange={(event) => setDraft({ ...draft, domain: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-port">پورت TCP موردنظر، اختیاری</Label><Input id="app-port" type="number" min={1} max={65535} dir="ltr" placeholder="3000" value={draft.listen_port} onChange={(event) => setDraft({ ...draft, listen_port: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-desired">وضعیت هدف (صرفاً یادداشت)</Label><select id="app-desired" value={draft.desired_state} onChange={(event) => setDraft({ ...draft, desired_state: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="none">تعیین‌نشده</option><option value="running">باید اجرا شود</option><option value="stopped">باید متوقف باشد</option></select></div></div><div className="space-y-2"><Label htmlFor="app-labels">برچسب‌ها</Label><Input id="app-labels" value={draft.labels} onChange={(event) => setDraft({ ...draft, labels: event.target.value })} placeholder="مشتری الف، production، وب"/><p className="text-[11px] text-muted-foreground">برچسب‌ها را با ویرگول جدا کن.</p></div><div className="space-y-2"><Label htmlFor="app-description">توضیحات</Label><textarea id="app-description" rows={3} maxLength={1000} value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} className="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]"/></div>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button type="button" variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button type="submit" disabled={saving}>{saving ? 'در حال ذخیره…' : target ? 'ذخیرهٔ تغییرات' : 'ثبت رکورد'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={dialog === 'delete' || dialog === 'restore'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>{dialog === 'delete' ? 'آرشیو اپلیکیشن' : 'بازیابی اپلیکیشن'}</DialogTitle><DialogDescription>{dialog === 'delete' ? `«${target?.name}» از فهرست فعال کنار می‌رود. فایل‌ها، کانتینرها، Nginx و سرویس‌های سیستم تغییر نمی‌کنند.` : `رکورد «${target?.name}» به فهرست فعال برمی‌گردد. اگر نام یا دامنه از آن زمان به مورد دیگری اختصاص یافته باشد، بازیابی انجام نمی‌شود.`}</DialogDescription></DialogHeader>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button variant={dialog === 'delete' ? 'destructive' : 'default'} disabled={saving} onClick={() => void (dialog === 'delete' ? archiveApp() : restoreApp())}>{saving ? 'در حال انجام…' : dialog === 'delete' ? 'آرشیو رکورد' : 'بازیابی رکورد'}</Button></div></DialogContent></Dialog>
  </>
}
