import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Archive, CircleAlert, CircleHelp, CirclePlus, Code2, Database, Globe2, Layers3, Pencil, RefreshCw, RotateCcw, Search, Server, Upload, X, Terminal, Wrench } from 'lucide-react'
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
type LaravelOperation = { id: string; action: string; state: string; output: string; exit_code: number | null; error: string | null; created_at: number; started_at: number | null; finished_at: number | null }
type LaravelInspection = { framework: string; state: string; release_path: string; public_path: string; runtime_version: string; message: string; composer: { php_constraint?: string; laravel_constraint?: string; required_extensions?: string[]; has_lockfile?: boolean }; files: Record<string, boolean | string[]> & { app_key_configured?: boolean; environment_keys?: string[] } }
type LaravelService = 'queue' | 'scheduler' | 'horizon' | 'reverb' | 'octane'
type LaravelWebState = { configured: boolean; site?: { site_name: string; domain: string; php_fpm_socket: string; enabled: boolean } | null; runtime_version: string; services: Record<LaravelService, string> & { reverb_port?: number | null; octane_port?: number | null } }
type NodeInspection = { state: string; release_path: string; node_version: string | null; requested_node_version: string; package_name?: string; package_version?: string; package_manager?: string; node_engine?: string; scripts: string[]; dependencies_installed: boolean; environment_keys: string[]; message: string }
type NodeService = { unit: string; state: string; enabled: boolean }

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
  const deploymentDescription = app.framework === 'node'
    ? 'وابستگی‌ها و scripts.build با lockfile در sandbox اجرا می‌شوند؛ install scripts خاموش‌اند و فایل .env به build داده نمی‌شود. اگر سرویس Node روشن باشد پس از جابه‌جایی release restart و بررسی loopback می‌شود.'
    : 'منبع را در پوشهٔ جدا آماده می‌کند و اشاره‌گر release را اتمی جابه‌جا می‌کند؛ اجرای runtime و تغییر Nginx برای این قالب خودکار انجام نمی‌شود.'
  return <section className="mt-3 rounded-xl border border-primary/15 bg-primary/[.025] p-3">
    <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">انتشار منبع و مدیریت release</p><p className="mt-1 max-w-3xl text-[10px] leading-5 text-muted-foreground">{deploymentDescription}</p></div><Button size="sm" variant="outline" onClick={() => { setError(''); setOpen((value) => !value) }}><Upload size={13}/>{open ? 'بستن' : 'استقرار منبع'}</Button></div>
    {open && <form onSubmit={(event) => void start(event)} className="mt-3 space-y-3 rounded-lg border border-border/70 bg-background/40 p-3">
      <div className="flex flex-wrap gap-2" role="tablist" aria-label="نوع منبع">{([['git','Git'],['directory','پوشهٔ VPS'],['archive','فایل ZIP']] as const).map(([value,label]) => <Button type="button" size="sm" variant={mode === value ? 'default' : 'outline'} key={value} onClick={() => setMode(value)}>{label}</Button>)}</div>
      {mode === 'git' && <div className="grid gap-2 sm:grid-cols-2"><label className="space-y-1 text-[11px] text-muted-foreground">مخزن عمومی HTTPS<Input dir="ltr" required value={repository} onChange={(event) => setRepository(event.target.value)} placeholder="https://github.com/org/project.git"/></label><label className="space-y-1 text-[11px] text-muted-foreground">Branch / tag / commit<Input dir="ltr" required value={reference} onChange={(event) => setReference(event.target.value)} placeholder="main یا commit SHA"/></label><p className="sm:col-span-2 text-[10px] leading-5 text-muted-foreground">فقط GitHub، GitLab، Bitbucket و Codeberg؛ URL دارای رمز، SSH key یا credential پذیرفته نیست. {app.framework === 'node' ? 'برای Node فقط install و build تعریف‌شدهٔ پروژه اجرا می‌شود؛ فایل .env از محیط build جداست.' : 'برای این قالب اسکریپت پروژه اجرا نمی‌شود.'}</p></div>}
      {mode === 'directory' && <label className="block space-y-1 text-[11px] text-muted-foreground">مسیر پوشهٔ سرور<Input dir="ltr" required value={directory} onChange={(event) => setDirectory(event.target.value)} placeholder="/var/www/site"/><span className="block text-[10px]">مسیر باید زیر /var/www یا /opt/arg/apps باشد؛ symlink رد می‌شود.</span></label>}
      {mode === 'archive' && <label className="block space-y-1 text-[11px] text-muted-foreground">ZIP پروژه<input type="file" accept=".zip,application/zip" required onChange={(event) => setArchive(event.target.files?.[0] || null)} className="block w-full rounded-md border border-input bg-background p-2 text-xs file:ml-2 file:rounded-md file:border-0 file:bg-primary file:px-2 file:py-1"/><span className="block text-[10px]">حداکثر ۲۵۶ MiB؛ محتوای بازشده حداکثر ۲ GiB. مسیرهای traversal، symlink و فایل‌های خاص رد می‌شوند.</span></label>}
      {mode !== 'archive' && <label className="block space-y-1 text-[11px] text-muted-foreground">فایل بررسی اختیاری (نسبی به ریشهٔ منبع)<Input dir="ltr" value={healthcheck} onChange={(event) => setHealthcheck(event.target.value)} placeholder="index.html"/></label>}
      <p className="rounded-lg border border-amber-300/15 bg-amber-300/[.04] p-2 text-[10px] leading-5 text-amber-100/80">فایل‌های محیطی مثل <code dir="ltr">.env</code>، پوشهٔ <code dir="ltr">.git</code> و <code dir="ltr">node_modules</code> از منبع کپی نمی‌شوند. نسخهٔ قبلی تا تأیید موفق سرویس حفظ می‌شود؛ در اپ Node با شکست restart اشاره‌گر به release قبلی برمی‌گردد.</p>
      {error && <p role="alert" className="text-xs text-rose-200">{error}</p>}
      <div className="flex justify-end gap-2"><Button type="button" size="sm" variant="outline" onClick={() => setOpen(false)} disabled={busy}>انصراف</Button><Button size="sm" disabled={busy}>{busy ? 'در حال ثبت…' : 'ساخت Job'}</Button></div>
    </form>}
    {error && !open && <p role="alert" className="mt-2 text-xs text-rose-200">{error}</p>}
    {jobs.length > 0 && <div className="mt-3 space-y-2">{jobs.slice(0,3).map((job) => <details key={job.id} className="rounded-lg border border-border/70 bg-background/35 p-2"><summary className="flex cursor-pointer list-none flex-wrap items-center justify-between gap-2 text-xs"><span className="font-medium">{labels[job.state] || job.state}</span><span dir="ltr" className="font-mono text-[10px] text-muted-foreground">{job.id.slice(0,8)} · {new Date(job.created_at * 1000).toLocaleString('fa-IR')}</span></summary><div className="mt-2 space-y-1.5">{job.steps.map((step) => <div key={`${job.id}:${step.sequence}`} className="flex items-start gap-2 border-r border-border/70 pr-2 text-[10px]"><span className={step.state === 'running' ? 'text-amber-200' : step.state === 'failed' ? 'text-rose-200' : 'text-emerald-200'}>●</span><div><p className="font-medium">{step.step}</p><p className="leading-5 text-muted-foreground">{step.message}</p></div></div>)}{job.error && <p className="rounded bg-rose-400/[.05] p-2 text-[10px] text-rose-200">{job.error}</p>}{job.release_path && <p dir="ltr" className="break-all text-left font-mono text-[10px] text-muted-foreground">{job.release_path}</p>}<div className="flex justify-end gap-2">{['queued','validating','running','verifying'].includes(job.state) ? <Button size="sm" variant="destructive" disabled={busy} onClick={() => void action(job,'cancel')}><X size={12}/>لغو Job</Button> : <>{job.state === 'failed' && mode !== 'archive' && <Button size="sm" variant="outline" disabled={busy} onClick={() => void action(job,'retry')}><RotateCcw size={12}/>اجرای دوباره</Button>}{job.state === 'succeeded' && job.previous_release && <Button size="sm" variant="outline" disabled={busy} onClick={() => void rollback(job)}><RotateCcw size={12}/>بازگشت به نسخهٔ قبلی</Button>}</>}</div></div></details>)}</div>}
    <Dialog open={!!rollbackTarget} onOpenChange={(isOpen) => { if (!isOpen && !busy) setRollbackTarget(null) }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>بازگشت به release قبلی؟</DialogTitle><DialogDescription>{app.framework === 'node' ? 'اشاره‌گر نسخه برمی‌گردد و اگر سرویس فعال باشد restart و روی loopback بررسی می‌شود. در صورت شکست، پنل تلاش می‌کند وضعیت جاری پیش از درخواست را نگه دارد.' : 'اشاره‌گر release به نسخهٔ پیشین تغییر می‌کند. برای این قالب، سرویس یا Nginx راه‌اندازی مجدد نمی‌شود و بررسی سلامت HTTP اجرا نمی‌شود.'}</DialogDescription></DialogHeader>{error && <p role="alert" className="text-xs text-rose-200">{error}</p>}<div className="flex justify-end gap-2"><Button variant="outline" disabled={busy} onClick={() => setRollbackTarget(null)}>انصراف</Button><Button disabled={busy} onClick={() => void confirmRollback()}>{busy ? 'در حال بازگشت…' : 'تأیید بازگشت'}</Button></div></DialogContent></Dialog>
  </section>
}

function LaravelRunnerPanel({ app, notify }: { app: AppRecord; notify: (message: string) => void }) {
  const [inspection, setInspection] = useState<LaravelInspection | null>(null)
  const [webState, setWebState] = useState<LaravelWebState | null>(null)
  const [operations, setOperations] = useState<LaravelOperation[]>([])
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [environmentBusy, setEnvironmentBusy] = useState(false)
  const [keyBusy, setKeyBusy] = useState(false)
  const [webBusy, setWebBusy] = useState(false)
  const [serviceBusy, setServiceBusy] = useState('')
  const [databaseBusy, setDatabaseBusy] = useState(false)
  const [confirmDatabase, setConfirmDatabase] = useState(false)
  const [databaseResult, setDatabaseResult] = useState<{ database: string; username: string } | null>(null)
  const [environmentDraft, setEnvironmentDraft] = useState<Record<string, string>>({ APP_NAME: '', APP_ENV: 'production', APP_DEBUG: 'false', APP_URL: '', DB_CONNECTION: 'mysql', DB_HOST: '127.0.0.1', DB_PORT: '3306', DB_DATABASE: '', DB_USERNAME: '', DB_PASSWORD: '' })
  const [confirmAction, setConfirmAction] = useState<'migrate' | 'install-dependencies' | null>(null)
  const active = operations.some((operation) => ['queued', 'running'].includes(operation.state))
  const actionNames: Record<string, string> = { about: 'اطلاعات Laravel', 'migrate-status': 'وضعیت Migration', migrate: 'اجرای Migration', 'install-dependencies': 'نصب Composer', 'cache-clear': 'پاک‌سازی Cache', 'config-cache': 'ساخت Config Cache', 'route-cache': 'ساخت Route Cache', 'view-cache': 'ساخت View Cache', 'queue-restart': 'راه‌اندازی مجدد Queue' }
  const environmentKeys = inspection?.files.environment_keys || []

  const load = useCallback(async () => {
    try {
      const [state, history, web] = await Promise.all([
        api<{ inspection: LaravelInspection }>(`apps/${encodeURIComponent(app.id)}/laravel`),
        api<{ operations: LaravelOperation[] }>(`apps/${encodeURIComponent(app.id)}/laravel/operations`),
        api<LaravelWebState>(`apps/${encodeURIComponent(app.id)}/laravel/web`),
      ])
      setInspection(state.inspection)
      setOperations(history.operations)
      setWebState(web)
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'وضعیت Laravel خوانده نشد.')
    } finally { setLoading(false) }
  }, [app.id])

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (!active) return
    const timer = window.setInterval(() => void load(), 1400)
    return () => window.clearInterval(timer)
  }, [active, load])

  async function run(action: string) {
    setBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/laravel/operations`, { method: 'POST', body: JSON.stringify({ action, confirmed: true }) })
      notify('عملیات Laravel ثبت شد؛ وضعیت و خروجی آن در همین بخش نمایش داده می‌شود.')
      setConfirmAction(null)
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ثبت عملیات Laravel ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function saveEnvironment(event: React.FormEvent) {
    event.preventDefault(); setEnvironmentBusy(true); setError('')
    try {
      const variables = Object.fromEntries(Object.entries(environmentDraft).filter(([, value]) => value.length > 0))
      if (!Object.keys(variables).length) throw new Error('حداقل یک مقدار وارد کن؛ خانهٔ خالی مقدار قبلی را پاک نمی‌کند.')
      await api(`apps/${encodeURIComponent(app.id)}/laravel/environment`, { method: 'PUT', body: JSON.stringify({ variables }) })
      notify('تنظیمات Laravel در فایل .env خصوصی ذخیره شد؛ مقدارها به مرورگر برنگشتند.')
      setEnvironmentDraft((current) => Object.fromEntries(Object.keys(current).map((key) => [key, ''])))
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ تنظیمات Laravel ناموفق بود.') }
    finally { setEnvironmentBusy(false) }
  }

  async function generateKey() {
    setKeyBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/laravel/environment`, { method: 'POST' })
      notify('APP_KEY تصادفی ساخته و مستقیماً در فایل خصوصی .env ذخیره شد.')
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت APP_KEY ناموفق بود.') }
    finally { setKeyBusy(false) }
  }

  async function configureWeb(enabled: boolean) {
    setWebBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/laravel/web`, { method: 'POST', body: JSON.stringify({ enabled }) })
      notify(enabled ? 'سایت Laravel با Nginx و PHP-FPM فعال شد.' : 'سایت Laravel از Nginx غیرفعال شد؛ فایل‌ها حفظ شدند.')
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر سرویس وب Laravel ناموفق بود.') }
    finally { setWebBusy(false) }
  }

  async function configureService(service: LaravelService, enabled: boolean) {
    setServiceBusy(service); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/laravel/services`, { method: 'POST', body: JSON.stringify({ service, enabled }) })
      const names: Record<LaravelService, string> = { queue: 'Queue Worker', scheduler: 'Scheduler', horizon: 'Horizon', reverb: 'Reverb', octane: 'Octane' }
      notify(enabled ? `${names[service]} فعال شد.` : `${names[service]} متوقف شد.`)
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر سرویس Laravel ناموفق بود.') }
    finally { setServiceBusy('') }
  }

  async function provisionDatabase() {
    setDatabaseBusy(true); setError('')
    try {
      const result = await api<{ database: string; username: string }>(`apps/${encodeURIComponent(app.id)}/laravel/database`, { method: 'POST', body: JSON.stringify({ confirmed: true }) })
      setDatabaseResult({ database: result.database, username: result.username })
      setConfirmDatabase(false)
      notify('دیتابیس Laravel ساخته شد و رمز تصادفی فقط در فایل خصوصی .env ذخیره شد.')
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت دیتابیس Laravel ناموفق بود.') }
    finally { setDatabaseBusy(false) }
  }

  const stateNames: Record<string, string> = { ready: 'آماده', dependencies_missing: 'وابستگی‌ها نصب نیست', environment_missing: 'تنظیم محیط لازم است', not_laravel: 'ساختار Laravel ناقص است' }
  return <section className="mt-3 rounded-xl border border-primary/15 bg-primary/[.025] p-3">
    <div className="flex flex-wrap items-start justify-between gap-2"><div><p className="flex items-center gap-2 text-xs font-medium"><Terminal size={14} className="text-primary"/>مدیریت Laravel</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">ساختار release، lockfile، مسیر public و فرمان‌های محدود Artisan را بررسی کن. migration با تأیید جداگانه اجرا می‌شود.</p></div><Button size="sm" variant="outline" onClick={() => void load()} disabled={loading}><RefreshCw size={13} className={loading ? 'animate-spin' : ''}/>بررسی</Button></div>
    {loading ? <Skeleton className="mt-3 h-16 rounded-lg"/> : inspection && <div className="mt-3 space-y-2 rounded-lg border border-border/70 bg-background/35 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2 text-[11px]"><span>وضعیت: <b className={inspection.state === 'ready' ? 'text-emerald-200' : 'text-amber-100'}>{stateNames[inspection.state] || inspection.state}</b></span><span>PHP انتخابی: <b dir="ltr" className="font-mono">{inspection.runtime_version === 'system' ? 'سیستم' : inspection.runtime_version}</b></span></div>
      <p className="text-[10px] leading-5 text-muted-foreground">{inspection.message}</p>
      <div className="grid gap-2 sm:grid-cols-2 text-[10px] text-muted-foreground"><p dir="ltr" className="truncate rounded border border-border/60 p-2 text-left font-mono">Public: {inspection.public_path}</p><p dir="ltr" className="truncate rounded border border-border/60 p-2 text-left font-mono">PHP: {inspection.composer?.php_constraint || 'بدون constraint ثبت‌شده'} · Laravel: {inspection.composer?.laravel_constraint || 'نامشخص'}</p></div>
      {!!inspection.composer?.required_extensions?.length && <p className="text-[10px] text-muted-foreground">افزونه‌های لازم PHP: <span dir="ltr" className="font-mono">{inspection.composer.required_extensions.join(', ')}</span></p>}
      <form onSubmit={(event) => void saveEnvironment(event)} className="space-y-2 border-t border-border/60 pt-3">
        <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">تنظیم امن فایل محیط</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">مقدارهای ذخیره‌شده نمایش داده نمی‌شوند؛ مقدار جدید فقط در فایل خصوصی VPS نوشته می‌شود. خانهٔ خالی مقدار قبلی را نگه می‌دارد.</p></div><Button type="button" size="sm" variant="outline" disabled={keyBusy || inspection.files.app_key_configured === true} onClick={() => void generateKey()}>{keyBusy ? 'در حال ساخت…' : inspection.files.app_key_configured === true ? 'APP_KEY تنظیم شده' : 'ساخت APP_KEY'}</Button></div>
        <div className="grid gap-2 sm:grid-cols-2">{Object.keys(environmentDraft).map((key) => <label key={key} className="space-y-1 text-[10px] text-muted-foreground">{key}{environmentKeys.includes(key) ? ' · ذخیره‌شده، برای جایگزینی مقدار وارد کن' : ' · تنظیم‌نشده'}<Input type={['APP_KEY','DB_PASSWORD'].includes(key) ? 'password' : 'text'} dir="ltr" autoComplete="new-password" value={environmentDraft[key]} onChange={(event) => setEnvironmentDraft((current) => ({ ...current, [key]: event.target.value }))} placeholder={key === 'APP_KEY' ? 'برای تولید امن از دکمه استفاده کن' : 'برای نگه‌داشتن مقدار فعلی خالی بگذار'}/></label>)}</div>
        <Button size="sm" disabled={environmentBusy || active}>{environmentBusy ? 'در حال ذخیره…' : 'ذخیرهٔ متغیرها'}</Button>
      </form>
      {inspection.state === 'dependencies_missing' && <Button size="sm" disabled={busy || active} onClick={() => setConfirmAction('install-dependencies')}><Wrench size={13}/>نصب وابستگی از روی lockfile</Button>}
      <div className="flex flex-wrap gap-1.5 border-t border-border/60 pt-2">{(['about','migrate-status','cache-clear','config-cache','route-cache','view-cache','queue-restart'] as const).map((action) => <Button key={action} size="sm" variant="outline" disabled={busy || active || inspection.state === 'dependencies_missing'} onClick={() => void run(action)}>{actionNames[action]}</Button>)}<Button size="sm" variant="outline" disabled={busy || active || inspection.state !== 'ready'} onClick={() => setConfirmAction('migrate')}>اجرای Migration</Button></div>
      <div className="space-y-2 border-t border-border/60 pt-3">
        <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">وب‌سایت و PHP-FPM</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">برای دامنهٔ ثبت‌شده، pool جداگانهٔ PHP-FPM و کانفیگ Nginx امن ساخته می‌شود. خاموش‌کردن، فایل‌ها و releaseها را حذف نمی‌کند.</p></div><Button size="sm" variant={webState?.site?.enabled ? 'outline' : 'default'} disabled={webBusy || !app.domain || inspection.state !== 'ready'} onClick={() => void configureWeb(!webState?.site?.enabled)}>{webBusy ? 'در حال اعمال…' : webState?.site?.enabled ? 'خاموش‌کردن سایت' : 'ساخت و فعال‌کردن سایت'}</Button></div>
        {webState?.site?.enabled && <div className="grid gap-2 sm:grid-cols-2 text-[10px]"><p dir="ltr" className="truncate rounded border border-border/60 p-2 text-left font-mono">http://{webState.site.domain}</p><p dir="ltr" className="truncate rounded border border-border/60 p-2 text-left font-mono">{webState.services.octane === 'active' ? `Octane · 127.0.0.1:${webState.services.octane_port || '—'}` : webState.site.php_fpm_socket}</p></div>}
        {!app.domain && <p className="text-[10px] text-amber-100/75">پیش از ساخت سایت، دامنه را در مشخصات اپلیکیشن ثبت کن.</p>}
        <p className="text-[10px] leading-5 text-amber-100/70">PHP-FPM تحت کاربر سیستمی www-data و با open_basedir محدود به همین اپ اجرا می‌شود. دسترسی فایل‌ها با ACL تنظیم می‌شود؛ اگر ابزار acl نصب نباشد، عملیات متوقف و نصب بستهٔ acl را درخواست می‌کند.</p>
      </div>
      <div className="space-y-2 border-t border-border/60 pt-3"><div><p className="text-xs font-medium">سرویس‌های Laravel</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">هر سرویس با systemd مستقل، فرمان ثابت و لاگ journal مدیریت می‌شود. Horizon جای Queue Worker مستقل را می‌گیرد. فعال‌کردن Octane در صورت روشن‌بودن سایت، Nginx را به loopback همان اپ وصل می‌کند و با خاموش‌کردنش به PHP-FPM برمی‌گردد.</p></div><div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{([['queue','Queue Worker'],['scheduler','Scheduler'],['horizon','Horizon'],['reverb','Reverb'],['octane','Octane'] ] as [LaravelService,string][]).map(([service,name]) => { const status = webState?.services?.[service] || 'نامشخص'; const running = status === 'active'; const unsupported = status === 'unsupported'; const port = service === 'reverb' ? webState?.services?.reverb_port : service === 'octane' ? webState?.services?.octane_port : null; const hints: Record<LaravelService,string> = { queue: 'مصرف jobها از صف Laravel', scheduler: 'اجرای زمان‌بندی Laravel', horizon: 'نیازمند پکیج laravel/horizon؛ هم‌زمان با Queue Worker اجرا نمی‌شود', reverb: `نیازمند laravel/reverb${port ? ` · 127.0.0.1:${port}` : ''}`, octane: `نیازمند laravel/octane و RoadRunner${port ? ` · 127.0.0.1:${port}` : ''}` }; return <div key={service} className="flex min-h-16 items-center justify-between gap-2 rounded-lg border border-border/70 p-2"><div className="min-w-0"><p className="text-[11px] font-medium">{name}</p><p className="mt-1 text-[9px] leading-4 text-muted-foreground">{unsupported ? 'وابستگی در release فعال نصب نیست' : hints[service]}</p><p dir="ltr" className="mt-1 text-[9px] text-muted-foreground">{status}</p></div><Button size="sm" variant={running ? 'outline' : 'default'} disabled={!!serviceBusy || inspection.state !== 'ready' || unsupported} onClick={() => void configureService(service, !running)}>{serviceBusy === service ? 'در حال اعمال…' : running ? 'توقف' : 'فعال‌سازی'}</Button></div> })}</div><p className="text-[10px] leading-5 text-muted-foreground">سرویس‌های Reverb و Octane فقط به آدرس loopback گوش می‌دهند و به اینترنت عمومی باز نمی‌شوند. برای اتصال عمومی Reverb باید تنظیمات وب‌سوکت و TLS سایت را جداگانه بررسی کنی.</p></div>
      <div className="space-y-2 border-t border-border/60 pt-3"><div className="flex flex-wrap items-center justify-between gap-2"><div><p className="text-xs font-medium">اتصال دیتابیس MySQL</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">دیتابیس و کاربر جداگانه می‌سازد؛ کاربر فقط به دیتابیس این اپ دسترسی دارد. رمز تصادفی فقط در فایل خصوصی .env ذخیره می‌شود و به مرورگر برنمی‌گردد.</p></div><Button size="sm" variant="outline" disabled={databaseBusy || active || environmentKeys.includes('DB_DATABASE') || environmentKeys.includes('DB_USERNAME')} onClick={() => setConfirmDatabase(true)}>{databaseBusy ? 'در حال ساخت…' : environmentKeys.includes('DB_DATABASE') || environmentKeys.includes('DB_USERNAME') ? 'اتصال دیتابیس ثبت شده' : 'ساخت دیتابیس و کاربر'}</Button></div>{databaseResult && <p className="text-[10px] text-emerald-200">آخرین دیتابیس ساخته‌شده: <code dir="ltr">{databaseResult.database}</code> · کاربر <code dir="ltr">{databaseResult.username}</code></p>}<p className="text-[10px] leading-5 text-muted-foreground">MySQL باید نصب و روشن باشد و یک بار در صفحهٔ MySQL با رمز پنل دسترسی مدیریتی را فعال کرده باشی. اگر قبلاً DB_DATABASE یا DB_USERNAME تنظیم شده باشد، اتصال موجود تغییر نمی‌کند.</p></div>
    </div>}
    {error && <p role="alert" className="mt-2 text-xs text-rose-200">{error}</p>}
    {operations.length > 0 && <div className="mt-3 space-y-2">{operations.slice(0,3).map((operation) => <details key={operation.id} open={['queued','running','failed'].includes(operation.state)} className="rounded-lg border border-border/70 bg-background/35 p-2"><summary className="flex cursor-pointer list-none items-center justify-between gap-2 text-[10px]"><span>{actionNames[operation.action] || operation.action} · {operation.state === 'succeeded' ? 'موفق' : operation.state === 'failed' ? 'ناموفق' : operation.state === 'interrupted' ? 'قطع‌شده' : 'در حال اجرا'}</span><span dir="ltr" className="font-mono text-muted-foreground">{operation.exit_code === null ? '…' : `exit ${operation.exit_code}`}</span></summary>{operation.error && <p className="mt-2 text-[10px] text-rose-200">{operation.error}</p>}{operation.output && <pre dir="ltr" className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap rounded bg-black/30 p-2 text-left font-mono text-[10px] leading-5 text-muted-foreground">{operation.output}</pre>}</details>)}</div>}
    <Dialog open={!!confirmAction} onOpenChange={(open) => { if (!open && !busy) setConfirmAction(null) }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>{confirmAction === 'migrate' ? 'اجرای migration دیتابیس؟' : 'نصب وابستگی‌های Composer؟'}</DialogTitle><DialogDescription>{confirmAction === 'migrate' ? 'Migration می‌تواند ساختار یا دادهٔ دیتابیس را تغییر دهد. Rollback کردن release کد، دیتابیس را به حالت قبل برنمی‌گرداند. پیش از ادامه، backup دیتابیس را بررسی کن.' : 'Composer فقط از composer.lock استفاده می‌کند و plugin/scriptها را اجرا نمی‌کند. دریافت بسته‌ها از شبکه انجام می‌شود و حداقل 512 MiB فضای آزاد لازم است.'}</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" disabled={busy} onClick={() => setConfirmAction(null)}>انصراف</Button><Button disabled={busy} onClick={() => void run(confirmAction || '')}>{busy ? 'در حال ثبت…' : 'تأیید و اجرا'}</Button></div></DialogContent></Dialog>
    <Dialog open={confirmDatabase} onOpenChange={(open) => { if (!open && !databaseBusy) setConfirmDatabase(false) }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>ساخت دیتابیس مخصوص این اپ؟</DialogTitle><DialogDescription>ARG یک نام یکتا بر اساس شناسهٔ اپ می‌سازد، کاربر MySQL با دسترسی فقط به همان دیتابیس ایجاد می‌کند و رمز تصادفی را در shared/.env می‌نویسد. رمز در پاسخ API یا لاگ نمایش داده نمی‌شود. اگر مجوز مدیریتی MySQL را هنوز فعال نکرده‌ای، به صفحهٔ MySQL برو و رمز پنل را یک بار تأیید کن.</DialogDescription></DialogHeader>{error && <p role="alert" className="text-xs text-rose-200">{error}</p>}<div className="flex justify-end gap-2"><Button variant="outline" disabled={databaseBusy} onClick={() => setConfirmDatabase(false)}>انصراف</Button><Button disabled={databaseBusy} onClick={() => void provisionDatabase()}>{databaseBusy ? 'در حال ساخت…' : 'تأیید و ساخت'}</Button></div></DialogContent></Dialog>
  </section>
}

function NodeRunnerPanel({ app, notify }: { app: AppRecord; notify: (message: string) => void }) {
  const [expanded, setExpanded] = useState(false)
  const [inspection, setInspection] = useState<NodeInspection | null>(null)
  const [service, setService] = useState<NodeService | null>(null)
  const [logs, setLogs] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [environmentBusy, setEnvironmentBusy] = useState(false)
  const [draft, setDraft] = useState<Record<string, string>>({})
  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const [node, currentService] = await Promise.all([
        api<{ inspection: NodeInspection }>(`apps/${encodeURIComponent(app.id)}/node`),
        api<{ service: NodeService; port: number | null }>(`apps/${encodeURIComponent(app.id)}/node/service`),
      ])
      setInspection(node.inspection)
      setService(currentService.service)
      setDraft((current) => {
        const next = { ...current }
        for (const key of node.inspection.environment_keys || []) if (!(key in next)) next[key] = ''
        return next
      })
      setError('')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'وضعیت Node.js خوانده نشد.') }
    finally { setLoading(false) }
  }, [app.id])

  useEffect(() => { if (expanded && !inspection) void load() }, [expanded, inspection, load])

  async function toggleService(enabled: boolean) {
    setBusy(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/node/service`, { method: 'PUT', body: JSON.stringify({ enabled }) })
      notify(enabled ? 'سرویس Node.js فعال و روی loopback بررسی شد.' : 'سرویس Node.js متوقف شد.')
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر وضعیت سرویس ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function saveEnvironment(event: React.FormEvent) {
    event.preventDefault(); setEnvironmentBusy(true); setError('')
    try {
      const variables = Object.fromEntries(Object.entries(draft).filter(([, value]) => value.length > 0))
      if (newKey.trim() && newValue) variables[newKey.trim()] = newValue
      if (!Object.keys(variables).length) throw new Error('برای ذخیره حداقل یک متغیر با مقدار جدید وارد کن.')
      await api(`apps/${encodeURIComponent(app.id)}/node/environment`, { method: 'PUT', body: JSON.stringify({ variables }) })
      notify('متغیرهای محیط در فایل خصوصی ذخیره شد؛ مقدارها از سرور بازخوانی نمی‌شوند.')
      setDraft((current) => Object.fromEntries(Object.keys(current).map((key) => [key, ''])))
      setNewKey(''); setNewValue(''); await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ متغیرهای Node ناموفق بود.') }
    finally { setEnvironmentBusy(false) }
  }

  async function readLogs() {
    setError('')
    try {
      const data = await api<{ logs: string }>(`apps/${encodeURIComponent(app.id)}/node/service/logs`)
      setLogs(data.logs || '')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'لاگ سرویس خوانده نشد.') }
  }

  const stateNames: Record<string, string> = { ready: 'آماده', lockfile_missing: 'Lockfile موجود نیست', runtime_missing_or_mismatched: 'Runtime نصب یا هماهنگ نیست', package_manager_missing: 'مدیر بسته نصب نیست', dependencies_missing: 'وابستگی‌ها نصب نیست', start_script_missing: 'اسکریپت start موجود نیست', manifest_missing_or_invalid: 'package.json نامعتبر است' }
  return <section className="mt-3 rounded-xl border border-primary/15 bg-primary/[.025] p-3">
    <div className="flex flex-wrap items-center justify-between gap-2"><div><p className="flex items-center gap-2 text-xs font-medium"><Terminal size={14} className="text-primary"/>مدیریت Node.js</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">جزئیات Runner فقط هنگام بازکردن خوانده می‌شود؛ نصب وابستگی و build هنگام استقرار در sandbox محدود اجرا می‌شوند.</p></div><div className="flex gap-1.5"><Button size="sm" variant="outline" onClick={() => setExpanded((value) => !value)}>{expanded ? 'بستن' : 'مدیریت'}</Button>{expanded && <Button size="icon" variant="outline" title="تازه‌سازی وضعیت" aria-label="تازه‌سازی وضعیت" onClick={() => void load()} disabled={loading}><RefreshCw size={13} className={loading ? 'animate-spin' : ''}/></Button>}</div></div>
    {expanded && (loading && !inspection ? <Skeleton className="mt-3 h-20 rounded-lg"/> : inspection && <div className="mt-3 space-y-3 rounded-lg border border-border/70 bg-background/35 p-3">
      <div className="grid gap-2 sm:grid-cols-2 text-[11px]"><p>وضعیت پروژه: <b className={inspection.state === 'ready' ? 'text-emerald-200' : 'text-amber-100'}>{stateNames[inspection.state] || inspection.state}</b></p><p>سرویس: <b className={service?.state === 'active' ? 'text-emerald-200' : 'text-muted-foreground'}>{service?.state === 'active' ? 'در حال اجرا' : service?.state || 'نامشخص'}</b></p><p>Node: <b dir="ltr" className="font-mono">{inspection.node_version || inspection.requested_node_version || 'نامشخص'}</b></p><p>مدیر بسته: <b dir="ltr" className="font-mono">{inspection.package_manager || 'نامشخص'}</b></p></div>
      <p className="text-[10px] leading-5 text-muted-foreground">{inspection.message}</p>
      {!!inspection.scripts?.length && <p className="text-[10px] text-muted-foreground">Scripts: <span dir="ltr" className="font-mono">{inspection.scripts.join(' · ')}</span></p>}
      {app.listen_port && <p dir="ltr" className="text-left font-mono text-[10px] text-muted-foreground">127.0.0.1:{app.listen_port}<span dir="rtl" className="mr-2 font-sans">· {app.domain ? 'با فعال‌سازی، ARG reverse proxy مستقل Nginx را می‌سازد؛ SSL از صفحهٔ Nginx فعال می‌شود.' : 'برای ساخت خودکار reverse proxy، دامنه را در مشخصات اپ ثبت کن.'} health check از GET / پاسخ ۵xx را رد می‌کند.</span></p>}
      <div className="flex flex-wrap gap-2 border-t border-border/60 pt-3"><Button size="sm" disabled={busy || loading || (service?.state !== 'active' && inspection.state !== 'ready')} onClick={() => void toggleService(service?.state !== 'active')}><Server size={13}/>{busy ? 'در حال اعمال…' : service?.state === 'active' ? 'توقف سرویس' : 'فعال‌سازی سرویس'}</Button><Button size="sm" variant="outline" onClick={() => void readLogs()}><Search size={13}/>نمایش لاگ‌های اجرا</Button></div>
      <form onSubmit={(event) => void saveEnvironment(event)} className="space-y-2 border-t border-border/60 pt-3"><div><p className="text-xs font-medium">متغیرهای Runtime</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">مقدارهای فعلی هرگز خوانده نمی‌شوند. مقدار خالی یعنی بدون تغییر. این متغیرها فقط هنگام اجرای سرویس در دسترس‌اند، نه برای اسکریپت build.</p></div>
        <div className="grid gap-2 sm:grid-cols-2">{Object.keys(draft).map((key) => <label key={key} className="space-y-1 text-[10px] text-muted-foreground"><span dir="ltr" className="font-mono">{key}</span><Input type={/(SECRET|TOKEN|PASSWORD|KEY)/i.test(key) ? 'password' : 'text'} dir="ltr" autoComplete="new-password" value={draft[key]} onChange={(event) => setDraft((current) => ({ ...current, [key]: event.target.value }))} placeholder="برای نگه‌داشتن مقدار فعلی خالی بگذار"/></label>)}</div>
        <div className="grid gap-2 sm:grid-cols-2"><Input aria-label="نام متغیر جدید" dir="ltr" value={newKey} onChange={(event) => setNewKey(event.target.value.toUpperCase())} placeholder="DB_HOST"/><Input aria-label="مقدار متغیر جدید" type={/(SECRET|TOKEN|PASSWORD|KEY)/i.test(newKey) ? 'password' : 'text'} dir="ltr" autoComplete="new-password" value={newValue} onChange={(event) => setNewValue(event.target.value)} placeholder="مقدار جدید"/></div>
        <Button size="sm" disabled={environmentBusy || busy}>{environmentBusy ? 'در حال ذخیره…' : 'ذخیرهٔ متغیرهای جدید'}</Button>
      </form>
      {logs && <div className="border-t border-border/60 pt-3"><p className="mb-2 text-xs font-medium">آخرین خروجی سرویس</p><pre dir="ltr" className="max-h-56 overflow-auto whitespace-pre-wrap rounded bg-black/30 p-2 text-left font-mono text-[10px] leading-5 text-muted-foreground">{logs}</pre></div>}
    </div>)}
    {expanded && error && <p role="alert" className="mt-2 text-xs text-rose-200">{error}</p>}
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
    <Card className="mb-5 border-[#dda46f]/15 bg-card/55"><CardContent className="flex items-start gap-3 p-4"><CircleHelp className="mt-0.5 shrink-0 text-primary" size={18}/><p className="text-xs leading-6 text-muted-foreground">از کارت هر اپ می‌توان Git عمومی، ZIP یا پوشهٔ مجاز VPS را به release جدا وارد کرد. مرحله‌ها و نتیجهٔ Job پس از refresh باقی می‌مانند و لغو، retry و بازگشت نسخه دارند. برای Node.js وابستگی و build در sandbox اجرا می‌شود و سرویس اختیاری loopback مدیریت می‌شود؛ Runnerهای Laravel و Node.js در همین کارت قابل مدیریت‌اند؛ Node با systemd محدود، health check محلی، متغیرهای Runtime و reverse proxy مستقل Nginx اجرا می‌شود. قالب‌های دیگر فعلاً فقط release را جابه‌جا می‌کنند.</p></CardContent></Card>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
    {allServers && serverErrors.length > 0 && <Card className="mb-4 border-amber-400/20 bg-amber-400/[.04]"><CardContent className="space-y-2 p-4"><p className="flex items-center gap-2 text-sm font-medium text-amber-100"><CircleAlert size={15}/>بعضی VPSها قابل خواندن نبودند</p>{serverErrors.map((item) => <div key={item.key} className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"><span>{item.name}: {item.message}</span><Button size="sm" variant="outline" onClick={() => onOpenWorkspace(item.workspaceKey)}>باز کردن VPS</Button></div>)}</CardContent></Card>}
    <div className="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-card/35 p-3">
      <div className="relative min-w-[190px] flex-1"><Search size={15} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجوی نام، دامنه یا برچسب" className="pr-9"/></div>
      <select value={frameworkFilter} onChange={(event) => setFrameworkFilter(event.target.value)} aria-label="فیلتر چارچوب" className="h-10 rounded-md border border-input bg-background px-3 text-xs"><option value="all">همهٔ چارچوب‌ها</option>{Object.entries(frameworkLabels).map(([key, label]) => <option value={key} key={key}>{label}</option>)}</select>
      <label className="flex h-10 items-center gap-2 rounded-md border border-input px-3 text-xs text-muted-foreground"><input type="checkbox" checked={showArchived} onChange={(event) => setShowArchived(event.target.checked)} className="accent-[#dda46f]"/>نمایش آرشیوی‌ها</label>
    </div>
        {loading ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{[0,1,2].map((item) => <Skeleton key={item} className="h-56 rounded-2xl"/>)}</div> : visibleRows.length === 0 ? <Card className="border-dashed"><CardContent className="flex min-h-48 flex-col items-center justify-center gap-3 p-6 text-center"><Layers3 size={26} className="text-primary"/><p className="font-medium">{showArchived ? 'رکوردی با این فیلتر پیدا نشد' : 'هنوز اپلیکیشنی ثبت نشده است'}</p><p className="max-w-lg text-xs leading-6 text-muted-foreground">اپلیکیشن یک موجودیت مدیریتی جدا از فایل Nginx یا کانتینر Docker است. ابتدا مشخصاتش را ثبت کن؛ دریافت سورس و releaseهای پایدار از کارت همان اپ انجام می‌شود.</p>{!allServers && <Button onClick={beginCreate}><CirclePlus size={15}/>ثبت اولین اپلیکیشن</Button>}</CardContent></Card> : <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{visibleRows.map(({ app, serverName, workspaceKey: ownerWorkspace, local }) => <Card key={`${app.id}:${app.owner_server_uuid || ''}`} className={`overflow-hidden ${app.deleted_at ? 'opacity-75' : ''}`}><CardHeader className="pb-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><CardTitle className="flex items-center gap-2 text-base"><Code2 size={17} className="shrink-0 text-primary"/><span className="truncate">{app.name}</span></CardTitle><CardDescription className="mt-2 flex items-center gap-1.5 truncate"><Server size={12}/>{serverName}</CardDescription></div><Badge variant={app.deleted_at ? 'outline' : 'success'}>{app.deleted_at ? 'آرشیو' : 'ثبت‌شده'}</Badge></div></CardHeader><CardContent className="space-y-3"><div className="grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">قالب</p><p className="mt-1 truncate">{frameworkLabels[app.framework] || app.framework}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">Runtime درخواستی</p><p className="mt-1 truncate">{app.runtime_version === 'system' ? 'نسخهٔ پیش‌فرض سیستم' : app.runtime_version}</p>{app.framework === 'node' && <p className="mt-1 truncate text-[10px] text-muted-foreground">ساخت: {app.build_runtime_version === 'system' ? 'نسخهٔ سیستم' : app.build_runtime_version}</p>}</div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">منبع انتخاب‌شده</p><p className="mt-1 truncate">{sourceLabels[app.source_type] || app.source_type}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت هدف</p><p className="mt-1">{app.desired_state === 'none' ? 'تنظیم نشده' : app.desired_state === 'running' ? 'درخواست اجرا' : 'درخواست توقف'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت واقعی</p><p className="mt-1">{app.observed_state === 'unknown' ? 'هنوز مشاهده نشده' : app.observed_state}</p></div></div>{app.domain && <p dir="ltr" className="flex items-center justify-end gap-1.5 truncate text-left font-mono text-xs text-muted-foreground"><Globe2 size={13}/>{app.domain}</p>}{app.listen_port && <p dir="ltr" className="text-left font-mono text-[11px] text-muted-foreground">TCP :{app.listen_port}</p>}{app.description && <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">{app.description}</p>}{app.labels.length > 0 && <div className="flex flex-wrap gap-1">{app.labels.map((label) => <Badge key={label} variant="outline" className="text-[10px]">{label}</Badge>)}</div>}<div className="flex items-center justify-between gap-2 border-t border-border/60 pt-3">{allServers && !local ? <Button size="sm" variant="outline" className="w-full" onClick={() => onOpenWorkspace(ownerWorkspace)}>رفتن به فضای کار میزبان</Button> : <span className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground"><Database size={12}/>منابع زنده هنوز متصل نیستند</span>}{!allServers && <div className="flex shrink-0 gap-1">{app.deleted_at ? <Button size="icon" variant="ghost" title="بازیابی رکورد" aria-label="بازیابی رکورد" onClick={() => { setTarget(app); setError(''); setDialog('restore') }}><RotateCcw size={15}/></Button> : <><Button size="icon" variant="ghost" title="ویرایش مشخصات" aria-label="ویرایش مشخصات" onClick={() => beginEdit({ app, serverUuid: '', serverName, workspaceKey: '', local: true })}><Pencil size={15}/></Button><Button size="icon" variant="ghost" className="text-rose-300" title="آرشیو رکورد" aria-label="آرشیو رکورد" onClick={() => { setTarget(app); setError(''); setDialog('delete') }}><Archive size={15}/></Button></>}</div>}</div>{!allServers && !app.deleted_at && <DeploymentPanel app={app} notify={notify}/>}{!allServers && !app.deleted_at && app.framework === 'laravel' && <LaravelRunnerPanel app={app} notify={notify}/>}{!allServers && !app.deleted_at && app.framework === 'node' && <NodeRunnerPanel app={app} notify={notify}/>}</CardContent></Card>)}</div>}
    <Dialog open={dialog === 'form'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-h-[92dvh] max-w-2xl overflow-y-auto"><DialogHeader><DialogTitle>{target ? 'ویرایش رکورد اپلیکیشن' : 'ثبت اپلیکیشن'}</DialogTitle><DialogDescription>نسخهٔ Runtime درخواستی را از نسخه‌های نصب‌شده انتخاب کن. این فاز نیازمندی و پیش‌بررسی را ثبت می‌کند؛ اجرای خودکار سرویس در فاز Runner انجام می‌شود.</DialogDescription></DialogHeader><form onSubmit={(event) => void submit(event)} className="space-y-4"><div className="grid gap-4 sm:grid-cols-2"><div className="space-y-2"><Label htmlFor="app-name">نام</Label><Input id="app-name" required maxLength={80} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-framework">قالب پیشنهادی</Label><select id="app-framework" value={draft.framework} onChange={(event) => setDraft({ ...draft, framework: event.target.value, runtime_version: 'system', build_runtime_version: 'system' })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(frameworkLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div>{requestedRuntimeFamily && <div className="space-y-2"><Label htmlFor="app-runtime-version">نسخهٔ Runtime موردنیاز</Label><select id="app-runtime-version" value={draft.runtime_version} onChange={(event) => setDraft({ ...draft, runtime_version: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="system">نسخهٔ پیش‌فرض سیستم</option>{availableRuntimeVersions.map((version) => <option key={version} value={version}>{version}</option>)}{draft.runtime_version !== 'system' && !availableRuntimeVersions.includes(draft.runtime_version) && <option value={draft.runtime_version}>{draft.runtime_version} · در inventory فعلی پیدا نشد</option>}</select><p className="text-[11px] leading-5 text-muted-foreground">{runtimeInventoryLoading ? 'در حال خواندن inventory نصب‌شدهٔ همین VPS…' : runtimeInventoryError || (availableRuntimeVersions.length ? `نسخه‌های نصب‌شدهٔ ${requestedRuntimeFamily} از inventory همین VPS آمده‌اند.` : `Runtime نصب‌شده برای ${requestedRuntimeFamily} پیدا نشد؛ پک نسخه را از مدیریت APT نصب و فرم را دوباره باز کن.`)}</p></div>}{draft.framework === 'node' && <div className="space-y-2"><Label htmlFor="app-build-runtime-version">نسخهٔ Node.js برای build</Label><select id="app-build-runtime-version" value={draft.build_runtime_version} onChange={(event) => setDraft({ ...draft, build_runtime_version: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="system">نسخهٔ پیش‌فرض سیستم</option>{availableRuntimeVersions.map((version) => <option key={version} value={version}>{version}</option>)}{draft.build_runtime_version !== 'system' && !availableRuntimeVersions.includes(draft.build_runtime_version) && <option value={draft.build_runtime_version}>{draft.build_runtime_version} · در inventory فعلی پیدا نشد</option>}</select><p className="text-[11px] leading-5 text-muted-foreground">نسخهٔ build جدا از نسخهٔ runtime ذخیره می‌شود تا مراحل بعد بتوانند lockfile و موتور ساخت پروژه را با هم بررسی کنند.</p></div>}<div className="space-y-2"><Label htmlFor="app-source">منبع پیش‌فرض استقرار</Label><select id="app-source" value={draft.source_type} onChange={(event) => setDraft({ ...draft, source_type: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(sourceLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div><div className="space-y-2"><Label htmlFor="app-domain">دامنهٔ رزروشده</Label><Input id="app-domain" dir="ltr" placeholder="app.example.com" value={draft.domain} onChange={(event) => setDraft({ ...draft, domain: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-port">پورت TCP موردنظر، اختیاری</Label><Input id="app-port" type="number" min={1} max={65535} dir="ltr" placeholder="3000" value={draft.listen_port} onChange={(event) => setDraft({ ...draft, listen_port: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-desired">وضعیت هدف (صرفاً یادداشت)</Label><select id="app-desired" value={draft.desired_state} onChange={(event) => setDraft({ ...draft, desired_state: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="none">تعیین‌نشده</option><option value="running">باید اجرا شود</option><option value="stopped">باید متوقف باشد</option></select></div></div><div className="space-y-2"><Label htmlFor="app-labels">برچسب‌ها</Label><Input id="app-labels" value={draft.labels} onChange={(event) => setDraft({ ...draft, labels: event.target.value })} placeholder="مشتری الف، production، وب"/><p className="text-[11px] text-muted-foreground">برچسب‌ها را با ویرگول جدا کن.</p></div><div className="space-y-2"><Label htmlFor="app-description">توضیحات</Label><textarea id="app-description" rows={3} maxLength={1000} value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} className="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]"/></div>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button type="button" variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button type="submit" disabled={saving}>{saving ? 'در حال ذخیره…' : target ? 'ذخیرهٔ تغییرات' : 'ثبت رکورد'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={dialog === 'delete' || dialog === 'restore'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>{dialog === 'delete' ? 'آرشیو اپلیکیشن' : 'بازیابی اپلیکیشن'}</DialogTitle><DialogDescription>{dialog === 'delete' ? `«${target?.name}» از فهرست فعال کنار می‌رود. فایل‌ها، کانتینرها، Nginx و سرویس‌های سیستم تغییر نمی‌کنند.` : `رکورد «${target?.name}» به فهرست فعال برمی‌گردد. اگر نام یا دامنه از آن زمان به مورد دیگری اختصاص یافته باشد، بازیابی انجام نمی‌شود.`}</DialogDescription></DialogHeader>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button variant={dialog === 'delete' ? 'destructive' : 'default'} disabled={saving} onClick={() => void (dialog === 'delete' ? archiveApp() : restoreApp())}>{saving ? 'در حال انجام…' : dialog === 'delete' ? 'آرشیو رکورد' : 'بازیابی رکورد'}</Button></div></DialogContent></Dialog>
  </>
}
