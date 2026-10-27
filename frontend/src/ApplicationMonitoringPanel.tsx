import { useCallback, useEffect, useState } from 'react'
import { Activity, AlertTriangle, ChevronDown, RefreshCw, Save } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Badge } from './components/ui/badge'

type Sample = { at: number; service_state: string; cpu_percent: number | null; memory_bytes: number | null; volume_bytes: number | null; volume_status: string }
type Alert = { metric: string; state: string; current_value: number | null; threshold: number | null; first_seen: number; last_seen: number; resolved_at: number | null }
type MonitoringEvent = { id: number; metric: string; event_type: 'triggered' | 'resolved'; lifecycle_started_at: number; created_at: number; current_value: number | null; threshold: number | null; acknowledged_at: number | null; acknowledged_by: string | null }
type Rules = { cpu_percent: number | null; memory_bytes: number | null; alert_when_inactive: boolean; enabled: boolean }
type Monitoring = {
  latest: (Sample & { measured_at: number; systemd_unit: string; error_code: string | null }) | null
  samples: Sample[]
  rules: Rules
  alerts: Alert[]
  events: MonitoringEvent[]
  sampling: { interval_seconds: number; retention_days: number; scope: string; network_bytes: number | null; http_latency_ms: number | null }
}

const dateTime = (seconds: number) => new Date(seconds * 1000).toLocaleString('fa-IR', { dateStyle: 'short', timeStyle: 'short' })
const bytes = (value: number | null | undefined) => {
  if (value == null || !Number.isFinite(value)) return 'نامشخص'
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let size = value; let unit = 0
  while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1 }
  return `${new Intl.NumberFormat('fa-IR', { maximumFractionDigits: 1 }).format(size)} ${units[unit]}`
}

export function ApplicationMonitoringPanel({ app }: { app: { id: string } }) {
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [data, setData] = useState<Monitoring | null>(null)
  const [cpu, setCpu] = useState('')
  const [memory, setMemory] = useState('')
  const [inactive, setInactive] = useState(true)
  const [enabled, setEnabled] = useState(true)

  const load = useCallback(async () => {
    setLoading(true); setError('')
    try {
      const result = await api<Monitoring>(`apps/${encodeURIComponent(app.id)}/monitoring`)
      setData(result)
      setCpu(result.rules.cpu_percent == null ? '' : String(result.rules.cpu_percent))
      setMemory(result.rules.memory_bytes == null ? '' : String(Math.round(result.rules.memory_bytes / (1024 * 1024))))
      setInactive(result.rules.alert_when_inactive); setEnabled(result.rules.enabled)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'پایش اپلیکیشن بارگذاری نشد.') }
    finally { setLoading(false) }
  }, [app.id])

  useEffect(() => { if (open) void load() }, [open, load])

  async function saveRules(event: React.FormEvent) {
    event.preventDefault(); setSaving(true); setError('')
    try {
      const cpuNumber = cpu.trim() ? Number(cpu) : null
      const memoryMiB = memory.trim() ? Number(memory) : null
      if (cpuNumber != null && (!Number.isFinite(cpuNumber) || cpuNumber < 1 || cpuNumber > 10_000)) throw new Error('آستانهٔ CPU باید بین ۱ تا ۱۰۰۰۰ درصد باشد.')
      if (memoryMiB != null && (!Number.isSafeInteger(memoryMiB) || memoryMiB < 1 || memoryMiB > Math.floor(Number.MAX_SAFE_INTEGER / 1048576))) throw new Error('آستانهٔ حافظه باید عدد صحیح مثبت بر حسب MiB باشد.')
      await api(`apps/${encodeURIComponent(app.id)}/monitoring/rules`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ cpu_percent: cpuNumber, memory_bytes: memoryMiB == null ? null : memoryMiB * 1048576, alert_when_inactive: inactive, enabled }) })
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ آستانه‌ها انجام نشد.') }
    finally { setSaving(false) }
  }

  async function acknowledge(eventId: number) {
    setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/monitoring/events/${eventId}/ack`, { method: 'POST' })
      await load()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تأیید اعلان انجام نشد.') }
  }

  const openAlerts = data?.alerts.filter((item) => item.state === 'open') ?? []
  return <section className="mt-3 rounded-xl border border-border/70 bg-background/25">
    <Button type="button" variant="ghost" aria-expanded={open} className="h-auto w-full justify-between px-3 py-3 text-right" onClick={() => setOpen((value) => !value)}>
      <span className="flex items-center gap-2"><Activity size={15} className="text-primary"/>پایش و آستانه‌های هشدار{openAlerts.length > 0 && <Badge variant="destructive">{openAlerts.length} هشدار</Badge>}</span>
      <ChevronDown size={15} className={`transition-transform ${open ? 'rotate-180' : ''}`}/>
    </Button>
    {open && <div className="space-y-3 border-t border-border/60 p-3">
      <div className="flex items-center justify-between gap-2"><p className="text-[11px] leading-5 text-muted-foreground">نمونه‌برداری دوره‌ای از سرویس systemd اختصاصی؛ تا ۲۰ سرویس در هر نوبت. نگهداری: ۳۰ روز.</p><Button size="icon" variant="outline" title="تازه‌سازی پایش" aria-label="تازه‌سازی پایش" onClick={() => void load()} disabled={loading}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/></Button></div>
      {error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-2 text-xs text-rose-200">{error}</p>}
      {loading && !data ? <div className="h-16 animate-pulse rounded-lg bg-muted/40"/> : data && <>
        <div className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4">
          <div className="rounded-lg border border-border/60 p-2"><p className="text-muted-foreground">وضعیت سرویس</p><p className="mt-1">{data.latest?.service_state ?? 'در انتظار نمونه'}</p></div>
          <div className="rounded-lg border border-border/60 p-2"><p className="text-muted-foreground">CPU سرویس</p><p className="mt-1" dir="ltr">{data.latest?.cpu_percent == null ? '—' : `${data.latest.cpu_percent.toFixed(1)}%`}</p></div>
          <div className="rounded-lg border border-border/60 p-2"><p className="text-muted-foreground">RAM سرویس</p><p className="mt-1" dir="ltr">{bytes(data.latest?.memory_bytes)}</p></div>
          <div className="rounded-lg border border-border/60 p-2"><p className="text-muted-foreground">دادهٔ پایدار اپ</p><p className="mt-1">{data.latest?.volume_status === 'fresh' || data.latest?.volume_status === 'measured' ? bytes(data.latest.volume_bytes) : data.latest?.volume_status === 'stale' ? `${bytes(data.latest.volume_bytes)} · قدیمی` : 'اندازه‌گیری نشده'}</p></div>
        </div>
        <p className="text-[10px] text-muted-foreground">{data.latest ? `آخرین نمونه: ${dateTime(data.latest.measured_at)} · ${data.latest.systemd_unit}` : 'هنوز نمونه‌ای ثبت نشده؛ جمع‌آوری پس‌زمینه برای سرویس‌های پشتیبانی‌شده انجام می‌شود.'} · CPU برحسب یک هسته است و ممکن است از ۱۰۰٪ بیشتر شود.</p>
        {openAlerts.length > 0 && <div className="space-y-1 rounded-lg border border-amber-300/25 bg-amber-300/[.04] p-2">{openAlerts.map((item) => <p key={item.metric} className="flex items-center gap-2 text-xs text-amber-100"><AlertTriangle size={13}/>{item.metric === 'service_inactive' ? 'سرویس در حال اجرا نیست' : item.metric === 'cpu_high' ? `مصرف CPU از ${item.threshold ?? '—'}٪ گذشته` : `مصرف RAM از ${bytes(item.threshold)} گذشته`}</p>)}</div>}
        {data.events.length > 0 && <div className="space-y-2 rounded-lg border border-border/60 p-3"><p className="text-xs font-medium">رویدادهای پایدار پایش</p><div className="max-h-48 space-y-1 overflow-y-auto">{data.events.slice(0, 12).map((item) => <div key={item.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted/20 px-2 py-2 text-[10px]"><span className="min-w-0 flex-1 leading-5">{item.event_type === 'triggered' ? 'هشدار فعال شد' : 'هشدار برطرف شد'} · {item.metric} · {dateTime(item.created_at)}</span>{item.acknowledged_at ? <Badge variant="outline">تأییدشده · {item.acknowledged_by}</Badge> : <Button size="sm" variant="outline" className="h-7" onClick={() => void acknowledge(item.id)}>تأیید</Button>}</div>)}</div><p className="text-[10px] text-muted-foreground">رویدادهای تغییر وضعیت در دیتابیس ذخیره می‌شوند؛ این نسخه اعلان بیرونی ارسال نمی‌کند.</p></div>}
        <form onSubmit={(event) => void saveRules(event)} className="space-y-3 rounded-lg border border-border/60 p-3">
          <div><p className="text-xs font-medium">آستانه‌های هشدار</p><p className="mt-1 text-[10px] text-muted-foreground">خالی‌گذاشتن آستانهٔ CPU یا RAM آن هشدار را غیرفعال می‌کند.</p></div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5"><Label htmlFor={`app-cpu-${app.id}`} className="text-xs">CPU سرویس (%)</Label><Input id={`app-cpu-${app.id}`} inputMode="decimal" dir="ltr" placeholder="مثلاً 85" value={cpu} onChange={(event) => setCpu(event.target.value)}/></div>
            <div className="space-y-1.5"><Label htmlFor={`app-mem-${app.id}`} className="text-xs">RAM سرویس (MiB)</Label><Input id={`app-mem-${app.id}`} inputMode="numeric" dir="ltr" placeholder="مثلاً 1024" value={memory} onChange={(event) => setMemory(event.target.value)}/></div>
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-2 text-xs"><label className="flex items-center gap-2"><input type="checkbox" checked={inactive} onChange={(event) => setInactive(event.target.checked)} className="accent-[#dda46f]"/>هشدار هنگام توقف ناخواسته</label><label className="flex items-center gap-2"><input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} className="accent-[#dda46f]"/>فعال‌بودن هشدارها</label></div>
          <Button size="sm" type="submit" disabled={saving || loading}><Save size={14}/>{saving ? 'در حال ذخیره…' : 'ذخیرهٔ آستانه‌ها'}</Button>
        </form>
        <div className="space-y-2"><p className="text-xs font-medium">نمونه‌های اخیر</p>{data.samples.length === 0 ? <p className="text-[11px] text-muted-foreground">تاریخچه‌ای برای نمایش نیست.</p> : <div className="max-h-40 space-y-1 overflow-y-auto">{data.samples.slice(0, 8).map((sample, index) => <div key={`${sample.at}-${index}`} className="grid grid-cols-[1fr_auto_auto] gap-3 rounded-md bg-muted/20 px-2 py-1.5 text-[10px]"><span>{dateTime(sample.at)}</span><span dir="ltr">CPU {sample.cpu_percent == null ? '—' : `${sample.cpu_percent.toFixed(1)}%`}</span><span dir="ltr">RAM {bytes(sample.memory_bytes)}</span></div>)}</div>}</div>
        <p className="text-[10px] leading-5 text-muted-foreground">محدودیت این نسخه: ترافیک شبکه و latency HTTP هنوز اندازه‌گیری نمی‌شوند؛ مقدار نامشخص عمداً صفر یا ساختگی نمایش داده نمی‌شود. این بخش فعلاً سرویس‌های systemd مدیریت‌شده را پایش می‌کند.</p>
      </>}
    </div>}
  </section>
}
