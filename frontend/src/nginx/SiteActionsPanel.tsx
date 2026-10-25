import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { Activity, CheckCircle2, CircleAlert, Edit3, ExternalLink, FolderOpen, Globe, LoaderCircle, Play, Plus, Power, RotateCw, ScrollText, ShieldCheck, Terminal, Trash2, X, Zap } from 'lucide-react'
import { api } from '../lib/api'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Label } from '../components/ui/label'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../components/ui/dialog'

type SiteAction = { id: number; site_name: string; title: string; icon: string; operation: string }
type FormState = { title: string; icon: string; operation: string }
type ActionResult = { message?: string; output?: unknown; logs?: string; path?: string }

const icons = { Play, RotateCw, ShieldCheck, FolderOpen, ScrollText, Globe, Zap, Terminal, Power, Activity }
const iconLabels: Record<keyof typeof icons, string> = { Play: 'اجرا', RotateCw: 'تازه‌سازی', ShieldCheck: 'امنیت', FolderOpen: 'فایل‌ها', ScrollText: 'لاگ', Globe: 'وب', Zap: 'سریع', Terminal: 'ترمینال', Power: 'روشن/خاموش', Activity: 'وضعیت' }
const operations = [
  ['nginx-test', 'بررسی تنظیمات کل Nginx'], ['nginx-reload', 'Reload سرویس Nginx'], ['nginx-restart', 'راه‌اندازی مجدد Nginx'],
  ['site-enable', 'روشن‌کردن همین سایت'], ['site-disable', 'خاموش‌کردن همین سایت'], ['site-access-log', 'نمایش Access Log'], ['site-error-log', 'نمایش Error Log'], ['site-files', 'رفتن به فایل‌های سایت'],
  ['app-start', 'شروع سرویس برنامهٔ متصل'], ['app-restart', 'راه‌اندازی مجدد سرویس برنامه'], ['app-reload', 'Reload سرویس برنامه'],
] as const
const emptyForm: FormState = { title: '', icon: 'Zap', operation: 'nginx-test' }

export function SiteActionsPanel({ siteName, enabled, notify }: { siteName: string; enabled: boolean; notify: (message: string) => void }) {
  const [actions, setActions] = useState<SiteAction[]>([])
  const [loading, setLoading] = useState(true)
  const [editorOpen, setEditorOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm)
  const [saving, setSaving] = useState(false)
  const [runOpen, setRunOpen] = useState(false)
  const [running, setRunning] = useState(false)
  const [runningAction, setRunningAction] = useState<SiteAction | null>(null)
  const [stage, setStage] = useState('')
  const [beforeStatus, setBeforeStatus] = useState<{ active?: boolean; enabled?: boolean } | null>(null)
  const [afterStatus, setAfterStatus] = useState<{ active?: boolean; enabled?: boolean } | null>(null)
  const [result, setResult] = useState<ActionResult | null>(null)
  const [runError, setRunError] = useState('')

  const refresh = useCallback(async () => {
    try { setActions((await api<{ actions: SiteAction[] }>(`nginx/sites/${encodeURIComponent(siteName)}/actions`)).actions) }
    catch (error) { notify(error instanceof Error ? error.message : 'دکمه‌های اختصاصی سایت بارگذاری نشدند.') }
    finally { setLoading(false) }
  }, [notify, siteName])
  useEffect(() => { void refresh() }, [refresh])

  function edit(action?: SiteAction) {
    setEditingId(action?.id ?? null)
    setForm(action ? { title: action.title, icon: action.icon, operation: action.operation } : emptyForm)
    setEditorOpen(true)
  }

  async function save(event: FormEvent) {
    event.preventDefault(); setSaving(true)
    try {
      const path = `nginx/sites/${encodeURIComponent(siteName)}/actions${editingId === null ? '' : `/${editingId}`}`
      await api(path, { method: editingId === null ? 'POST' : 'PUT', body: JSON.stringify(form) })
      setEditorOpen(false); notify(editingId === null ? 'دکمهٔ سایت اضافه شد.' : 'دکمهٔ سایت ویرایش شد.'); await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ دکمه ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function remove(action: SiteAction) {
    if (!window.confirm(`دکمهٔ «${action.title}» حذف شود؟`)) return
    try { await api(`nginx/sites/${encodeURIComponent(siteName)}/actions/${action.id}`, { method: 'DELETE' }); notify('دکمه حذف شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف دکمه ناموفق بود.') }
  }

  async function run(action: SiteAction) {
    setRunningAction(action); setRunOpen(true); setRunning(true); setStage('خواندن وضعیت فعلی Nginx'); setBeforeStatus(null); setAfterStatus(null); setResult(null); setRunError('')
    try {
      const before = await api<{ active?: boolean; enabled?: boolean }>('nginx/status').catch(() => undefined)
      setBeforeStatus(before || { active: false })
      setStage(operations.find(([key]) => key === action.operation)?.[1] || 'در حال اجرای اکشن')
      const output = await api<ActionResult>(`nginx/sites/${encodeURIComponent(siteName)}/actions/${action.id}/run`, { method: 'POST' })
      setResult(output)
      if (action.operation === 'site-enable' || action.operation === 'site-disable') setAfterStatus({ ...before, enabled: action.operation === 'site-enable' })
      else setAfterStatus(await api<{ active?: boolean; enabled?: boolean }>('nginx/status').catch(() => undefined) || null)
      setStage('اجرا کامل شد')
    } catch (error) { setRunError(error instanceof Error ? error.message : 'اجرای اکشن ناموفق بود.'); setStage('اجرا با خطا متوقف شد') }
    finally { setRunning(false) }
  }

  return <>
    <section className="space-y-3 rounded-xl border border-primary/20 bg-primary/[.035] p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="flex items-center gap-2 text-sm font-semibold text-white"><Zap size={15} className="text-primary"/>دکمه‌های اختصاصی این سایت</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">میان‌برهای همین سایت را بساز؛ عنوان و آیکون را انتخاب کن و نتیجهٔ اجرای هرکدام را ببین.</p></div><Button size="sm" variant="outline" onClick={() => edit()}><Plus size={14}/>افزودن دکمه</Button></div>
      {loading ? <div className="h-14 animate-pulse rounded-lg bg-white/[.04]"/> : actions.length ? <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{actions.map((action) => { const Icon = icons[action.icon as keyof typeof icons] || Zap; return <div key={action.id} className="flex min-w-0 items-center gap-2 rounded-xl border border-white/[.08] bg-background/50 p-2.5"><button type="button" aria-label={`اجرای ${action.title}`} disabled={running} onClick={() => void run(action)} className="grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-primary/20 bg-primary/[.07] text-primary transition hover:bg-primary/15 disabled:opacity-50"><Icon size={16}/></button><button type="button" disabled={running} onClick={() => void run(action)} className="min-w-0 flex-1 text-right"><span className="block truncate text-xs font-semibold text-white">{action.title}</span><span className="mt-1 block truncate text-[10px] text-muted-foreground">{operations.find(([key]) => key === action.operation)?.[1] || action.operation}</span></button><Button variant="ghost" size="icon" className="h-7 w-7" aria-label={`ویرایش ${action.title}`} onClick={() => edit(action)}><Edit3 size={12}/></Button><Button variant="ghost" size="icon" className="h-7 w-7 text-rose-300" aria-label={`حذف ${action.title}`} onClick={() => void remove(action)}><Trash2 size={12}/></Button></div> })}</div> : <p className="rounded-lg border border-dashed border-white/10 px-3 py-5 text-center text-xs text-muted-foreground">برای این سایت هنوز دکمه‌ای نساخته‌ای.</p>}
    </section>

    <Dialog open={editorOpen} onOpenChange={setEditorOpen}><DialogContent><DialogHeader><DialogTitle>{editingId === null ? 'افزودن دکمه به سایت' : 'ویرایش دکمهٔ سایت'}</DialogTitle><DialogDescription>اکشن‌ها از فهرست امن انتخاب می‌شوند؛ دستور Shell دلخواه اجرا نمی‌شود.</DialogDescription></DialogHeader><form onSubmit={save} className="space-y-4"><div className="space-y-2"><Label htmlFor={`site-action-title-${siteName}`}>نام دکمه</Label><Input id={`site-action-title-${siteName}`} value={form.title} maxLength={40} onChange={(event) => setForm((old) => ({ ...old, title: event.target.value }))} placeholder="مثلاً Restart برنامه" required/></div><div className="space-y-2"><Label htmlFor={`site-action-operation-${siteName}`}>اکشن</Label><select id={`site-action-operation-${siteName}`} value={form.operation} onChange={(event) => setForm((old) => ({ ...old, operation: event.target.value }))} className="h-10 w-full rounded-lg border border-white/10 bg-background px-3 text-sm text-white">{operations.map(([key,label]) => <option key={key} value={key}>{label}</option>)}</select><p className="text-[10px] leading-4 text-muted-foreground">برای اکشن‌های سرویس برنامه، باید در بخش منابع همین سایت یک systemd service وصل شده باشد.</p></div><div className="space-y-2"><Label>آیکون دکمه</Label><div className="grid grid-cols-5 gap-2">{(Object.keys(icons) as (keyof typeof icons)[]).map((key) => { const Icon = icons[key]; return <button key={key} type="button" aria-pressed={form.icon === key} title={iconLabels[key]} onClick={() => setForm((old) => ({ ...old, icon: key }))} className={`grid h-10 place-items-center rounded-lg border transition ${form.icon === key ? 'border-primary/50 bg-primary/15 text-primary' : 'border-white/10 text-muted-foreground hover:bg-white/[.04]'}`}><Icon size={16}/></button> })}</div></div><Button className="w-full" disabled={saving || !form.title.trim()}>{saving ? 'در حال ذخیره…' : editingId === null ? 'افزودن دکمه' : 'ذخیره تغییرات'}</Button></form></DialogContent></Dialog>

    <Dialog open={runOpen} onOpenChange={(open) => { if (!running) setRunOpen(open) }}><DialogContent className="max-h-[88vh] max-w-2xl overflow-y-auto"><DialogHeader><DialogTitle className="flex items-center gap-2">{running ? <LoaderCircle size={17} className="animate-spin text-primary"/> : runError ? <CircleAlert size={17} className="text-rose-300"/> : <CheckCircle2 size={17} className="text-emerald-300"/>}{runningAction?.title || 'اکشن سایت'}</DialogTitle><DialogDescription dir="ltr" className="text-left">{siteName} · {operations.find(([key]) => key === runningAction?.operation)?.[1]}</DialogDescription></DialogHeader><div className="space-y-3"><div className={`rounded-lg border p-3 text-xs ${running ? 'border-primary/20 bg-primary/[.04] text-primary' : runError ? 'border-rose-300/20 bg-rose-300/[.04] text-rose-100' : 'border-emerald-300/20 bg-emerald-300/[.04] text-emerald-100'}`} aria-live="polite">{stage}</div><div className="grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-white/[.08] bg-background/50 p-3"><span className="text-muted-foreground">وضعیت سایت قبل از اجرا</span><p className="mt-2 font-semibold text-white">{beforeStatus ? enabled ? 'فعال' : 'غیرفعال' : 'در حال بررسی'}</p></div><div className="rounded-lg border border-white/[.08] bg-background/50 p-3"><span className="text-muted-foreground">وضعیت Nginx</span><p className="mt-2 font-semibold text-white">{beforeStatus ? beforeStatus.active ? 'سرویس فعال' : 'سرویس متوقف' : 'در حال بررسی'}</p></div><div className="rounded-lg border border-white/[.08] bg-background/50 p-3"><span className="text-muted-foreground">وضعیت سایت پس از اجرا</span><p className="mt-2 font-semibold text-white">{afterStatus ? afterStatus.enabled === undefined ? afterStatus.active ? 'سرویس فعال' : 'سرویس متوقف' : afterStatus.enabled ? 'فعال' : 'غیرفعال' : 'در انتظار نتیجه'}</p></div><div className="rounded-lg border border-white/[.08] bg-background/50 p-3"><span className="text-muted-foreground">نتیجهٔ اکشن</span><p className="mt-2 font-semibold text-white">{result?.message || (result ? 'انجام شد' : 'در حال اجرا')}</p></div></div>{runError && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-3 text-xs text-rose-100">{runError}</p>}{result?.logs && <section className="space-y-2"><h3 className="flex items-center gap-2 text-xs font-semibold text-white"><ScrollText size={14} className="text-primary"/>{runningAction?.operation === 'site-error-log' ? 'Error Log' : 'Access Log'}</h3><pre dir="ltr" className="max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[10px] leading-5 text-muted-foreground">{result.logs}</pre></section>}{result?.output !== undefined && <pre dir="ltr" className="max-h-48 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[10px] leading-5 text-muted-foreground">{typeof result.output === 'string' ? result.output : JSON.stringify(result.output, null, 2)}</pre>}{result?.path && <div className="flex items-center justify-between gap-2 rounded-lg border border-white/10 p-3"><code dir="ltr" className="truncate font-mono text-xs">{result.path}</code><Button size="sm" onClick={() => { setRunOpen(false); window.location.hash = `#/files?path=${encodeURIComponent(result.path!)}` }}><ExternalLink size={13}/>بازکردن فایل‌ها</Button></div>}<div className="flex justify-end"><Button variant="outline" disabled={running} onClick={() => setRunOpen(false)}><X size={14}/>بستن</Button></div></div></DialogContent></Dialog>
  </>
}
