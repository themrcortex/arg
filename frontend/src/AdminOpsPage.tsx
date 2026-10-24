import { useCallback, useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { Activity, Check, CircleAlert, Download, LoaderCircle, Package, Power, RefreshCw, Search, ServerCog, Square, TerminalSquare, Trash2, Zap } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Alert, AlertDescription } from './components/ui/alert'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type PackageRow = { name: string; version: string; summary: string }
type ServiceRow = { unit: string; enabled: string; state: string; sub_state: string; description: string }

export function AptPage({ Heading, notify }: Common) {
  const [packages, setPackages] = useState<PackageRow[]>([])
  const [query, setQuery] = useState('')
  const [packageInput, setPackageInput] = useState('')
  const [busy, setBusy] = useState('list')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')
  const refresh = useCallback(async () => {
    setBusy('list'); setError('')
    try { const result = await api<{ packages: PackageRow[] }>('packages'); setPackages(result.packages) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'فهرست بسته‌ها دریافت نشد.') }
    finally { setBusy('') }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  const run = async (action: string, names: string[] = []) => {
    if (busy) return
    setBusy(action); setError(''); setOutput('')
    try {
      const result = await api<{ output: string }>('packages/action', { method: 'POST', body: JSON.stringify({ action, packages: names }) })
      setOutput(result.output || `APT ${action} completed successfully.`)
      notify(`عملیات APT (${action}) با موفقیت انجام شد.`)
      if (['install', 'remove', 'autoremove', 'upgrade', 'full-upgrade'].includes(action)) await refresh()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'عملیات APT ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  const install = (event: FormEvent) => { event.preventDefault(); const names = packageInput.trim().split(/[\s,]+/).filter(Boolean); if (names.length) { void run('install', names); setPackageInput('') } }
  const filtered = useMemo(() => packages.filter((pkg) => `${pkg.name} ${pkg.summary}`.toLowerCase().includes(query.toLowerCase())).slice(0, 250), [packages, query])
  const actions = [
    { id: 'update', title: 'به‌روزرسانی فهرست', note: 'دریافت فهرست تازهٔ مخازن', icon: RefreshCw },
    { id: 'upgrade', title: 'ارتقای بسته‌ها', note: 'نصب به‌روزرسانی‌های موجود', icon: Download },
    { id: 'full-upgrade', title: 'ارتقای کامل', note: 'مدیریت تغییر وابستگی‌ها', icon: Zap },
    { id: 'autoremove', title: 'حذف خودکار', note: 'پاک‌سازی وابستگی‌های بلااستفاده', icon: Trash2 },
    { id: 'autoclean', title: 'پاک‌سازی کش قدیمی', note: 'حذف فایل‌های بستهٔ منقضی', icon: Package },
    { id: 'clean', title: 'پاک‌سازی کامل کش', note: 'حذف آرشیوهای دانلودشدهٔ APT', icon: Square },
  ]
  if (busy === 'list' && !packages.length && !error) return <>
    <Heading title="مدیریت APT" description="در حال دریافت فهرست بسته‌های نصب‌شده…"/>
    <div className="mb-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{actions.map((action) => <Skeleton key={action.id} className="h-24 rounded-2xl"/>)}</div>
    <Skeleton className="mb-5 h-36 rounded-2xl"/><div className="rounded-2xl border border-border bg-card/50 p-5"><Skeleton className="mb-5 h-6 w-48"/><Skeleton className="mb-4 h-10 w-full"/>{Array.from({ length: 5 }, (_, index) => <Skeleton key={index} className="mb-3 h-11 w-full last:mb-0"/>)}</div>
  </>
  return <>
    <Heading title="مدیریت APT" description="مخازن، نصب و حذف بسته‌ها و نگهداری سیستم را از یک محل مدیریت کن." />
    <div className="mb-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{actions.map((item) => <button key={item.id} disabled={!!busy} onClick={() => void run(item.id)} className="group flex min-h-24 items-center gap-4 rounded-2xl border border-border bg-card/70 p-4 text-right transition hover:border-[#dda46f]/40 hover:bg-card disabled:opacity-50"><span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-[#dda46f]/10 text-[#dda46f]"><item.icon size={19} className={busy === item.id ? 'animate-spin' : ''}/></span><span className="min-w-0 flex-1"><b className="block text-sm">{item.title}</b><span className="mt-1 block text-xs text-muted-foreground">{item.note}</span></span>{busy === item.id && <LoaderCircle size={16} className="animate-spin"/>}</button>)}</div>
    <Card className="mb-5"><CardHeader><CardTitle>نصب بسته</CardTitle><CardDescription>نام یک یا چند بسته را با فاصله وارد کن.</CardDescription></CardHeader><CardContent><form onSubmit={install} className="flex flex-wrap gap-2"><Input value={packageInput} onChange={(event) => setPackageInput(event.target.value)} placeholder="مثال: curl nginx htop" dir="ltr" className="min-w-56 flex-1 text-left font-mono"/><Button type="submit" disabled={!!busy || !packageInput.trim()}><Download size={15}/>{busy === 'install' ? 'در حال نصب…' : 'نصب بسته'}</Button></form></CardContent></Card>
    {error && <Alert className="mb-4 border-rose-400/25 bg-rose-400/[.06]"><CircleAlert size={16}/><AlertDescription>{error}</AlertDescription></Alert>}
    {output && <Card className="mb-5"><CardHeader className="pb-2"><CardTitle className="text-sm">گزارش آخرین عملیات</CardTitle></CardHeader><CardContent><pre className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-[#090d12] p-4 text-left font-mono text-xs leading-5 text-muted-foreground" dir="ltr">{output}</pre></CardContent></Card>}
    <Card><CardHeader className="flex-row items-center justify-between gap-3"><div><CardTitle>بسته‌های نصب‌شده</CardTitle><CardDescription>{packages.length.toLocaleString('en-US')} بستهٔ نصب‌شده</CardDescription></div><Button size="sm" variant="outline" onClick={() => void refresh()} disabled={!!busy}><RefreshCw size={14} className={busy === 'list' ? 'animate-spin' : ''}/>بازخوانی</Button></CardHeader><CardContent><div className="mb-4 flex gap-2"><div className="relative flex-1"><Search size={15} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجوی نام یا توضیح بسته…" className="pr-9"/></div></div><div className="max-h-[560px] overflow-auto rounded-xl border border-border"><table className="w-full min-w-[680px] text-sm"><thead className="sticky top-0 bg-secondary/90 text-xs text-muted-foreground"><tr><th className="p-3 text-right">نام بسته</th><th className="p-3 text-right">نسخه</th><th className="p-3 text-right">توضیحات</th><th className="p-3"/></tr></thead><tbody>{filtered.map((pkg) => <tr key={pkg.name} className="border-t border-border/70 hover:bg-white/[.02]"><td className="p-3 font-mono text-xs" dir="ltr">{pkg.name}</td><td className="p-3 font-mono text-xs text-muted-foreground" dir="ltr">{pkg.version}</td><td className="max-w-md truncate p-3 text-xs text-muted-foreground">{pkg.summary}</td><td className="p-2"><Button size="sm" variant="ghost" disabled={!!busy} onClick={() => { if (window.confirm(`Remove ${pkg.name}?`)) void run('remove', [pkg.name]) }} aria-label={`حذف ${pkg.name}`}><Trash2 size={14}/></Button></td></tr>)}{!filtered.length && <tr><td colSpan={4} className="p-8 text-center text-sm text-muted-foreground">بسته‌ای پیدا نشد.</td></tr>}</tbody></table></div><p className="mt-3 text-[11px] text-muted-foreground">فهرست برای جلوگیری از بارگذاری سنگین، حداکثر 250 نتیجهٔ جست‌وجو را نمایش می‌دهد.</p></CardContent></Card>
  </>
}

const serviceActions = [
  { id: 'start', label: 'شروع', icon: Power }, { id: 'stop', label: 'توقف', icon: Square }, { id: 'restart', label: 'راه‌اندازی مجدد', icon: RefreshCw },
  { id: 'try-restart', label: 'راه‌اندازی مجدد در صورت اجرا', icon: RefreshCw }, { id: 'reload', label: 'بارگذاری مجدد', icon: Activity }, { id: 'reload-or-restart', label: 'بارگذاری یا راه‌اندازی', icon: Zap },
  { id: 'enable', label: 'فعال‌سازی خودکار', icon: Check }, { id: 'disable', label: 'غیرفعال‌سازی خودکار', icon: CircleAlert },
  { id: 'enable-now', label: 'فعال‌سازی و شروع', icon: Power }, { id: 'disable-now', label: 'غیرفعال‌سازی و توقف', icon: Square }, { id: 'reenable', label: 'بازفعال‌سازی', icon: Check },
  { id: 'mask', label: 'مسدودکردن سرویس', icon: CircleAlert }, { id: 'unmask', label: 'رفع مسدودی', icon: Check }, { id: 'reset-failed', label: 'پاک‌کردن وضعیت خطا', icon: RefreshCw },
  { id: 'status', label: 'جزئیات وضعیت', icon: Activity }, { id: 'logs', label: 'مشاهدهٔ گزارش‌ها', icon: TerminalSquare },
]

export function ServicesPage({ Heading, notify }: Common) {
  const [services, setServices] = useState<ServiceRow[]>([])
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState('all')
  const [busy, setBusy] = useState('list')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')
  const refresh = useCallback(async () => {
    setBusy('list'); setError('')
    try { const result = await api<{ services: ServiceRow[] }>('services'); setServices(result.services) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'فهرست سرویس‌ها دریافت نشد.') }
    finally { setBusy('') }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  const run = async (action: string, unit: string) => {
    if (busy) return
    if (['stop', 'restart', 'disable', 'disable-now', 'mask'].includes(action) && !window.confirm(`Run ${action} on ${unit}? This may interrupt services that depend on it.`)) return
    setBusy(`${action}:${unit}`); setError(''); setOutput('')
    try { const result = await api<{ output: string }>('services/action', { method: 'POST', body: JSON.stringify({ action, unit }) }); setOutput(`${action} ${unit}\n${result.output || 'Completed successfully.'}`); notify(`عملیات ${action} روی ${unit} انجام شد.`); await refresh() }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'عملیات سرویس ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  const filtered = useMemo(() => services.filter((service) => {
    const matches = `${service.unit} ${service.description}`.toLowerCase().includes(query.toLowerCase())
    return matches && (filter === 'all' || (filter === 'active' ? service.state === 'active' : filter === 'failed' ? service.state === 'failed' : service.state !== 'active'))
  }), [services, query, filter])
  if (busy === 'list' && !services.length && !error) return <>
    <Heading title="سرویس‌های سیستم" description="در حال شناسایی سرویس‌های systemd…"/>
    <div className="mb-5 grid gap-3 sm:grid-cols-3">{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="h-24 rounded-2xl"/>)}</div>
    <div className="rounded-2xl border border-border bg-card/50 p-5"><div className="mb-5 flex items-center justify-between"><Skeleton className="h-6 w-36"/><Skeleton className="h-9 w-28 rounded-lg"/></div><Skeleton className="mb-4 h-10 w-full"/>{Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="mb-2 h-14 w-full last:mb-0"/>)}</div>
  </>
  return <>
    <Heading title="سرویس‌های سیستم" description="سرویس‌های systemd را جست‌وجو کن و وضعیت اجرا و فعال‌سازی آن‌ها را مدیریت کن." />
    <div className="mb-5 grid gap-3 sm:grid-cols-3"><Card><CardContent className="flex items-center gap-3 p-4"><ServerCog className="text-[#dda46f]"/><div><div className="text-xl font-semibold tabular-nums">{services.length.toLocaleString('en-US')}</div><div className="text-xs text-muted-foreground">سرویس شناسایی‌شده</div></div></CardContent></Card><Card><CardContent className="flex items-center gap-3 p-4"><Activity className="text-emerald-300"/><div><div className="text-xl font-semibold tabular-nums">{services.filter((service) => service.state === 'active').length.toLocaleString('en-US')}</div><div className="text-xs text-muted-foreground">در حال اجرا</div></div></CardContent></Card><Card><CardContent className="flex items-center gap-3 p-4"><CircleAlert className="text-rose-300"/><div><div className="text-xl font-semibold tabular-nums">{services.filter((service) => service.state === 'failed').length.toLocaleString('en-US')}</div><div className="text-xs text-muted-foreground">دارای خطا</div></div></CardContent></Card></div>
    {error && <Alert className="mb-4 border-rose-400/25 bg-rose-400/[.06]"><CircleAlert size={16}/><AlertDescription>{error}</AlertDescription></Alert>}
    {output && <Card className="mb-4"><CardHeader className="pb-2"><CardTitle className="text-sm">گزارش آخرین عملیات</CardTitle></CardHeader><CardContent><pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded-xl bg-[#090d12] p-3 text-left font-mono text-xs text-muted-foreground" dir="ltr">{output}</pre></CardContent></Card>}
    <Card><CardHeader className="flex-row items-center justify-between gap-3"><div><CardTitle>سرویس‌ها</CardTitle><CardDescription>انتخاب سرویس برای نمایش عملیات systemctl</CardDescription></div><div className="flex gap-2"><Button size="sm" variant="outline" disabled={!!busy} onClick={() => void run('daemon-reload', 'system')}><RefreshCw size={14}/>بازخوانی پیکربندی مدیر systemd</Button><Button size="sm" variant="outline" onClick={() => void refresh()} disabled={!!busy}><RefreshCw size={14} className={busy === 'list' ? 'animate-spin' : ''}/>بازخوانی فهرست</Button></div></CardHeader><CardContent><div className="mb-4 flex flex-wrap gap-2"><div className="relative min-w-52 flex-1"><Search size={15} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجوی سرویس…" className="pr-9"/></div>{[['all','همه'],['active','در حال اجرا'],['inactive','متوقف'],['failed','دارای خطا']].map(([id,label]) => <Button key={id} size="sm" variant={filter === id ? 'default' : 'outline'} onClick={() => setFilter(id)}>{label}</Button>)}</div><div className="space-y-2">{filtered.slice(0,160).map((service) => <details key={service.unit} className="group rounded-xl border border-border bg-card/55 open:bg-card"><summary className="flex cursor-pointer list-none flex-wrap items-center gap-3 p-3"><span className={`h-2 w-2 rounded-full ${service.state === 'active' ? 'bg-emerald-400' : service.state === 'failed' ? 'bg-rose-400' : 'bg-slate-500'}`}/><span className="min-w-48 flex-1"><b dir="ltr" className="block text-left font-mono text-xs">{service.unit}</b><span className="mt-1 block truncate text-xs text-muted-foreground">{service.description}</span></span><Badge variant={service.state === 'active' ? 'success' : service.state === 'failed' ? 'destructive' : 'outline'}>{service.state} / {service.sub_state}</Badge><Badge variant="outline">{service.enabled}</Badge></summary><div className="border-t border-border/70 p-3"><div className="flex flex-wrap gap-2">{serviceActions.map((action) => <Button key={action.id} size="sm" variant="outline" disabled={!!busy} onClick={() => void run(action.id, service.unit)}><action.icon size={13}/>{action.label}{busy === `${action.id}:${service.unit}` && <LoaderCircle size={13} className="animate-spin"/>}</Button>)}</div></div></details>)}{!filtered.length && <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">سرویسی مطابق فیلتر پیدا نشد.</div>}</div>{filtered.length > 160 && <p className="mt-3 text-xs text-muted-foreground">برای سرعت، 160 سرویس نخست نمایش داده می‌شود؛ برای محدودکردن نتایج جست‌وجو کن.</p>}</CardContent></Card>
  </>
}
