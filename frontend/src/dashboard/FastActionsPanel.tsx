import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { Activity, CheckCircle2, CircleAlert, Clock3, Edit3, LoaderCircle, Play, Plus, ScrollText, Trash2, X } from 'lucide-react'
import { api, formatBytes, type Dashboard, type User } from '../lib/api'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Label } from '../components/ui/label'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../components/ui/dialog'
import { useConfirmDialog } from '../hooks/useConfirmDialog'

type FastAction = { id: number; title: string; operation: string; target: string; site_name: string | null; show_execution_logs: boolean; include_error_log: boolean }
type FastActionForm = Omit<FastAction, 'id'>
type Site = { name: string; enabled: boolean }
type Service = { unit: string; state: string; sub_state: string }
type RuntimeStatus = { installed?: boolean; active?: boolean; enabled?: boolean; version?: string; config_test?: { ok: boolean; output: string } }
type LogResult = { logs: string; path: string }
type CustomCommandResult = { success: boolean; exit_code: number; timed_out: boolean; stdout: string; stderr: string; logs_truncated: boolean }

const emptyForm: FastActionForm = { title: '', operation: 'nginx-test', target: '', site_name: null, show_execution_logs: false, include_error_log: false }
const operationLabels: Record<string, string> = {
  'nginx-test': 'بررسی کانفیگ Nginx', 'nginx-reload': 'Reload سرویس Nginx', 'nginx-restart': 'راه‌اندازی مجدد Nginx',
  'service-start': 'شروع سرویس systemd', 'service-restart': 'راه‌اندازی مجدد سرویس', 'service-reload': 'Reload سرویس systemd',
  'custom-command': 'اکشن سفارشی',
}

export function FastActionsPanel({ user, notify }: { user: User; notify: (message: string) => void }) {
  const { confirm, confirmDialog } = useConfirmDialog()
  const [actions, setActions] = useState<FastAction[]>([])
  const [sites, setSites] = useState<Site[]>([])
  const [services, setServices] = useState<Service[]>([])
  const [editorOpen, setEditorOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [form, setForm] = useState<FastActionForm>(emptyForm)
  const [saving, setSaving] = useState(false)
  const [runOpen, setRunOpen] = useState(false)
  const [running, setRunning] = useState(false)
  const [runAction, setRunAction] = useState<FastAction | null>(null)
  const [stage, setStage] = useState('')
  const [before, setBefore] = useState<{ dashboard?: Dashboard; nginx?: RuntimeStatus; service?: Service } | null>(null)
  const [after, setAfter] = useState<{ dashboard?: Dashboard; nginx?: RuntimeStatus; service?: Service } | null>(null)
  const [operationResult, setOperationResult] = useState('')
  const [logs, setLogs] = useState<{ execution?: LogResult; error?: LogResult }>({})
  const [runError, setRunError] = useState('')

  const refresh = useCallback(async () => {
    if (user.role !== 'admin') return
    try { setActions((await api<{ actions: FastAction[] }>('fast-actions')).actions) }
    catch (error) { notify(error instanceof Error ? error.message : 'Fast Actionها بارگذاری نشدند.') }
  }, [notify, user.role])
  useEffect(() => { void refresh() }, [refresh])

  async function openEditor(action?: FastAction) {
    setEditingId(action?.id ?? null)
    setForm(action ? { title: action.title, operation: action.operation, target: action.target, site_name: action.site_name, show_execution_logs: action.operation === 'custom-command' && action.show_execution_logs, include_error_log: action.include_error_log } : emptyForm)
    setEditorOpen(true)
    try {
      const [siteResult, serviceResult] = await Promise.allSettled([
        api<{ sites: Site[] }>('nginx/sites'), api<{ services: Service[] }>('services'),
      ])
      if (siteResult.status === 'fulfilled') setSites(siteResult.value.sites)
      if (serviceResult.status === 'fulfilled') setServices(serviceResult.value.services)
    } catch { /* One list may be unavailable; the editor still remains usable for Nginx actions. */ }
  }

  async function save(event: FormEvent) {
    event.preventDefault(); setSaving(true)
    try {
      await api(editingId === null ? 'fast-actions' : `fast-actions/${editingId}`, { method: editingId === null ? 'POST' : 'PUT', body: JSON.stringify(form) })
      setEditorOpen(false); notify(editingId === null ? 'Fast Action ساخته شد.' : 'Fast Action ویرایش شد.'); await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ Fast Action ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function remove(action: FastAction) {
    if (!await confirm({ title: 'حذف Fast Action', description: `اکشن «${action.title}» از داشبورد حذف شود؟`, confirmLabel: 'حذف اکشن', destructive: true })) return
    try { await api(`fast-actions/${action.id}`, { method: 'DELETE' }); notify('Fast Action حذف شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف Fast Action ناموفق بود.') }
  }

  async function execute(action: FastAction) {
    const isCustom = action.operation === 'custom-command'
    if (isCustom && !await confirm({ title: 'اجرای اکشن سفارشی', description: `اکشن «${action.title}» به‌عنوان کاربر سرویس ARG اجرا می‌شود؛ دسترسی root ندارد و حداکثر 30 ثانیه اجرا خواهد شد.`, confirmLabel: 'اجرای اکشن', destructive: true })) return
    setRunAction(action); setRunOpen(true); setRunning(true); setStage('در حال خواندن وضعیت فعلی پروژه'); setBefore(null); setAfter(null); setOperationResult(''); setLogs({}); setRunError('')
    const isNginx = action.operation.startsWith('nginx-')
    try {
      const dashboard = await api<Dashboard>('dashboard').catch(() => undefined)
      const currentNginx = isNginx ? await api<RuntimeStatus>('nginx/status').catch(() => undefined) : undefined
      const currentServices = !isNginx && !isCustom ? await api<{ services: Service[] }>('services').catch(() => undefined) : undefined
      const currentService = currentServices?.services.find((service) => service.unit === action.target)
      setBefore({ dashboard, nginx: currentNginx, service: currentService })

      setStage(operationLabels[action.operation] || 'در حال اجرای عملیات')
      let output = ''
      if (isNginx) {
        const result = await api<Record<string, unknown>>('nginx/action', { method: 'POST', body: JSON.stringify({ action: action.operation.replace('nginx-', '') }) })
        output = String(result.message || result.output || result.config_test && JSON.stringify(result.config_test) || 'عملیات Nginx با موفقیت انجام شد.')
      } else if (isCustom) {
        const result = await api<CustomCommandResult>(`fast-actions/${action.id}/run`, { method: 'POST' })
        output = result.timed_out ? 'اکشن به سقف زمان 30 ثانیه رسید.' : result.success ? 'اکشن سفارشی با موفقیت اجرا شد.' : `اکشن با کد خروج ${result.exit_code} پایان یافت.`
        if (action.show_execution_logs) {
          const chunks = [result.stdout && `STDOUT\n${result.stdout}`, result.stderr && `STDERR\n${result.stderr}`].filter(Boolean)
          setLogs((current) => ({ ...current, execution: { logs: chunks.join('\n\n') || 'خروجی‌ای ثبت نشد.', path: result.logs_truncated ? 'نمایش محدود به 64 KiB برای هر جریان' : 'stdout / stderr' } }))
        }
        if (!result.success) setRunError(result.timed_out ? 'اجرای اکشن از 30 ثانیه گذشت و متوقف شد.' : `اکشن سفارشی با کد خروج ${result.exit_code} شکست خورد.`)
      } else {
        const result = await api<{ output?: string }>('services/action', { method: 'POST', body: JSON.stringify({ operation: action.operation.replace('service-', ''), unit: action.target }) })
        output = result.output || `${action.target}: ${operationLabels[action.operation]}`
      }
      setOperationResult(output)
      setStage('در حال بررسی وضعیت نهایی و لاگ‌ها')
      const finalDashboard = await api<Dashboard>('dashboard').catch(() => undefined)
      const finalNginx = isNginx ? await api<RuntimeStatus>('nginx/status').catch(() => undefined) : undefined
      const finalServices = !isNginx && !isCustom ? await api<{ services: Service[] }>('services').catch(() => undefined) : undefined
      setAfter({ dashboard: finalDashboard, nginx: finalNginx, service: finalServices?.services.find((service) => service.unit === action.target) })
      const logResults: { error?: LogResult } = {}
      if (action.site_name && action.include_error_log) {
        const requested: Promise<void>[] = []
        requested.push(api<LogResult>(`nginx/sites/${encodeURIComponent(action.site_name)}/logs`, { method: 'POST', body: JSON.stringify({ kind: 'error' }) }).then((result) => { logResults.error = result }).catch(() => { logResults.error = { logs: 'خواندن Error Log ناموفق بود.', path: '' } }))
        await Promise.all(requested); setLogs((current) => ({ ...current, ...logResults }))
      }
      setStage('عملیات تمام شد')
    } catch (error) {
      setRunError(error instanceof Error ? error.message : 'اجرای Fast Action ناموفق بود.')
      setStage('عملیات با خطا متوقف شد')
    } finally { setRunning(false) }
  }

  if (user.role !== 'admin') return null
  const isServiceAction = form.operation.startsWith('service-')
  const isCustomAction = form.operation === 'custom-command'
  return <>
    <section className="arg-dashboard-panel min-w-0 rounded-[14px] border p-4"><div className="mb-4 flex flex-wrap items-start justify-between gap-3"><div><h2 className="font-bold text-foreground">Fast Actions</h2><p className="mt-1 text-xs leading-5 text-muted-foreground">اکشن‌های Nginx، systemd یا دستور سفارشی را اجرا کن و نتیجه و وضعیت قبل و بعد را ببین.</p></div><Button size="sm" onClick={() => void openEditor()}><Plus size={14}/>افزودن اکشن</Button></div>
      {actions.length ? <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{actions.map((action) => <div key={action.id} className="flex min-w-0 items-center gap-2 rounded-xl border border-white/[.08] bg-background/50 p-3"><button className="grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-primary/20 bg-primary/[.06] text-primary transition hover:bg-primary/15 disabled:opacity-50" aria-label={`اجرای ${action.title}`} disabled={running} onClick={() => void execute(action)}><Play size={16}/></button><button className="min-w-0 flex-1 text-right" disabled={running} onClick={() => void execute(action)}><span className="block truncate text-sm font-semibold text-white">{action.title}</span><span className="mt-1 flex items-center gap-1 truncate text-[10px] text-muted-foreground"><Activity size={11}/>{operationLabels[action.operation]}{action.target && action.operation !== 'custom-command' && <code dir="ltr" className="truncate">· {action.target}</code>}</span></button><div className="flex shrink-0"><Button variant="ghost" size="icon" className="h-8 w-8" aria-label={`ویرایش ${action.title}`} onClick={() => void openEditor(action)}><Edit3 size={13}/></Button><Button variant="ghost" size="icon" className="h-8 w-8 text-rose-300" aria-label={`حذف ${action.title}`} onClick={() => void remove(action)}><Trash2 size={13}/></Button></div></div>)}</div> : <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-dashed border-white/10 p-4"><p className="text-xs text-muted-foreground">هنوز اکشنی نساخته‌ای. عملیات Nginx، systemd یا دستور سفارشی را ذخیره کن تا از داشبورد اجرا شود.</p><Button size="sm" variant="outline" onClick={() => void openEditor()}><Plus size={14}/>ساخت اولین اکشن</Button></div>}
    </section>

    <Dialog open={editorOpen} onOpenChange={setEditorOpen}><DialogContent><DialogHeader><DialogTitle>{editingId === null ? 'ساخت Fast Action' : 'ویرایش Fast Action'}</DialogTitle><DialogDescription>{isCustomAction ? 'دستور سفارشی با دسترسی کاربر سرویس ARG اجرا می‌شود؛ root نیست و اجرای آن حداکثر 30 ثانیه طول می‌کشد.' : 'اکشن از فهرست عملیات پشتیبانی‌شدهٔ Nginx و systemd اجرا می‌شود.'}</DialogDescription></DialogHeader><form onSubmit={save} className="space-y-4"><div className="space-y-2"><Label htmlFor="fast-action-title">عنوان دکمه</Label><Input id="fast-action-title" maxLength={48} value={form.title} onChange={(event) => setForm((old) => ({ ...old, title: event.target.value }))} placeholder="بررسی و Reload سایت" required/></div><div className="space-y-2"><Label htmlFor="fast-action-operation">عملیات</Label><select id="fast-action-operation" value={form.operation} onChange={(event) => setForm((old) => ({ ...old, operation: event.target.value, target: '', site_name: null, show_execution_logs: false, include_error_log: false }))} className="h-10 w-full rounded-lg border border-white/10 bg-background px-3 text-sm text-white">{Object.entries(operationLabels).map(([value,label]) => <option key={value} value={value}>{label}</option>)}</select></div>{isCustomAction ? <div className="space-y-2"><Label htmlFor="fast-action-command">دستور یا اسکریپت</Label><textarea id="fast-action-command" required rows={5} maxLength={2048} value={form.target} onChange={(event) => setForm((old) => ({ ...old, target: event.target.value }))} placeholder="مثال: php artisan queue:restart" dir="ltr" className="w-full rounded-lg border border-white/10 bg-background px-3 py-2 text-left font-mono text-sm text-white placeholder:text-muted-foreground"/><p className="text-[11px] leading-5 text-muted-foreground">حداکثر 2048 بایت؛ با کاربر سرویس ARG اجرا می‌شود، نه root. از قراردادن رمز یا توکن در دستور خودداری کن.</p></div> : null}{isServiceAction ? <div className="space-y-2"><Label htmlFor="fast-action-unit">واحد systemd</Label><select id="fast-action-unit" required value={form.target} onChange={(event) => setForm((old) => ({ ...old, target: event.target.value }))} className="h-10 w-full rounded-lg border border-white/10 bg-background px-3 font-mono text-sm text-white"><option value="">انتخاب سرویس…</option>{services.map((service) => <option key={service.unit} value={service.unit}>{service.unit} · {service.state}</option>)}</select></div> : null}{form.include_error_log || form.operation === 'nginx-test' ? <div className="space-y-2"><Label htmlFor="fast-action-site">سایت Nginx برای وضعیت و لاگ</Label><select id="fast-action-site" value={form.site_name || ''} onChange={(event) => setForm((old) => ({ ...old, site_name: event.target.value || null }))} className="h-10 w-full rounded-lg border border-white/10 bg-background px-3 text-sm text-white"><option value="">بدون سایت مشخص</option>{sites.map((site) => <option key={site.name} value={site.name}>{site.name}{site.enabled ? ' · فعال' : ' · غیرفعال'}</option>)}</select></div> : null}<div className="grid gap-2 sm:grid-cols-2">{isCustomAction && <label className="flex items-center gap-2 rounded-lg border border-white/10 p-3 text-xs"><input type="checkbox" checked={form.show_execution_logs} onChange={(event) => setForm((old) => ({ ...old, show_execution_logs: event.target.checked }))} className="accent-primary"/>نمایش لاگ‌های اجرا</label>}<label className="flex items-center gap-2 rounded-lg border border-white/10 p-3 text-xs"><input type="checkbox" checked={form.include_error_log} onChange={(event) => setForm((old) => ({ ...old, include_error_log: event.target.checked, site_name: event.target.checked ? old.site_name : null }))} className="accent-primary"/>نمایش Error Log سایت</label></div><p className="text-[11px] leading-5 text-muted-foreground">لاگ اجرا شامل خروجی stdout و stderr همین اکشن است؛ Error Log از سایت Nginx انتخاب‌شده خوانده می‌شود.</p><Button className="w-full" disabled={saving || !form.title.trim() || ((isServiceAction || isCustomAction) && !form.target.trim())}>{saving ? 'در حال ذخیره…' : editingId === null ? 'ساخت اکشن' : 'ذخیره تغییرات'}</Button></form></DialogContent></Dialog>

    <Dialog open={runOpen} onOpenChange={(open) => { if (!running) setRunOpen(open) }}><DialogContent className="max-h-[90vh] max-w-3xl overflow-y-auto"><DialogHeader><DialogTitle className="flex items-center gap-2">{running ? <LoaderCircle size={17} className="animate-spin text-primary"/> : runError ? <CircleAlert size={17} className="text-rose-300"/> : <CheckCircle2 size={17} className="text-emerald-300"/>}{runAction?.title || 'Fast Action'}</DialogTitle><DialogDescription>{runAction && <span>{operationLabels[runAction.operation]} {runAction.target && runAction.operation !== 'custom-command' && <code dir="ltr">· {runAction.target}</code>}</span>}</DialogDescription></DialogHeader><div className="space-y-4"><div className={`flex items-center gap-2 rounded-lg border p-3 text-xs ${running ? 'border-primary/20 bg-primary/[.04] text-primary' : runError ? 'border-rose-300/20 bg-rose-300/[.04] text-rose-100' : 'border-emerald-300/20 bg-emerald-300/[.04] text-emerald-100'}`} aria-live="polite">{running ? <LoaderCircle size={14} className="animate-spin"/> : <Clock3 size={14}/>}<span>{stage}</span></div><div className="grid gap-3 sm:grid-cols-2"><StatusSummary title="وضعیت پیش از اجرا" dashboard={before?.dashboard} nginx={before?.nginx} service={before?.service}/><StatusSummary title="وضعیت پس از اجرا" dashboard={after?.dashboard} nginx={after?.nginx} service={after?.service}/></div>{operationResult && <section className="space-y-2"><h3 className="text-sm font-semibold text-white">نتیجهٔ عملیات</h3><pre dir="ltr" className="max-h-48 overflow-auto whitespace-pre-wrap rounded-xl border border-white/10 bg-background p-3 text-left font-mono text-xs leading-5 text-muted-foreground">{operationResult}</pre></section>}{runError && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-3 text-xs text-rose-100">{runError}</p>}{(logs.execution || logs.error) && <section className="space-y-3">{logs.execution && <LogPanel title="لاگ‌های اجرا" result={logs.execution}/>}{logs.error && <LogPanel title="Error Log" result={logs.error}/>}</section>}<div className="flex justify-end"><Button variant="outline" disabled={running} onClick={() => setRunOpen(false)}><X size={14}/>بستن</Button></div></div></DialogContent></Dialog>
    {confirmDialog}
  </>
}

function StatusSummary({ title, dashboard, nginx, service }: { title: string; dashboard?: Dashboard; nginx?: RuntimeStatus; service?: Service }) {
  return <section className="space-y-2 rounded-xl border border-white/[.08] bg-background/40 p-3"><h3 className="text-xs font-semibold text-white">{title}</h3><div className="grid grid-cols-2 gap-2 text-[11px]"><div className="rounded-lg bg-card/70 p-2"><span className="text-muted-foreground">سرور</span><p dir="ltr" className="mt-1 truncate font-mono text-white">{dashboard?.hostname || '—'}</p></div><div className="rounded-lg bg-card/70 p-2"><span className="text-muted-foreground">RAM</span><p dir="ltr" className="mt-1 font-mono text-white">{dashboard ? formatBytes((dashboard.memory_kib - dashboard.available_kib) * 1024) : '—'}</p></div><div className="rounded-lg bg-card/70 p-2"><span className="text-muted-foreground">Nginx</span><p className="mt-1 text-white">{nginx ? nginx.installed ? nginx.active ? 'در حال اجرا' : 'متوقف' : 'نصب نیست' : '—'}</p></div><div className="rounded-lg bg-card/70 p-2"><span className="text-muted-foreground">سرویس هدف</span><p dir="ltr" className="mt-1 truncate font-mono text-white">{service ? `${service.state}/${service.sub_state}` : nginx?.config_test ? nginx.config_test.ok ? 'کانفیگ معتبر' : 'خطای کانفیگ' : '—'}</p></div></div>{nginx?.config_test?.output && <p dir="ltr" className="line-clamp-2 break-all font-mono text-[10px] text-muted-foreground">{nginx.config_test.output}</p>}</section>
}

function LogPanel({ title, result }: { title: string; result: LogResult }) {
  return <div className="rounded-xl border border-white/[.08] p-3"><div className="mb-2 flex items-center gap-2 text-xs font-semibold text-white"><ScrollText size={14} className="text-primary"/>{title}</div>{result.path && <p dir="ltr" className="mb-2 truncate text-left font-mono text-[10px] text-muted-foreground">{result.path}</p>}<pre dir="ltr" className="max-h-52 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[10px] leading-5 text-muted-foreground">{result.logs || 'لاگی ثبت نشده است.'}</pre></div>
}
