import { useCallback, useEffect, useState } from 'react'
import { Activity, Cpu, MemoryStick, RefreshCw, Save } from 'lucide-react'
import { api, formatBytes, formatNumber } from '../lib/api'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Label } from '../components/ui/label'
import { Skeleton } from '../components/ui/skeleton'

type RuntimeResponse = {
  configured: boolean
  site?: string
  unit?: string
  cpu_quota_percent?: number | null
  memory_max_bytes?: number | null
  cpu_percent?: number | null
  memory_current_bytes?: number | null
  measurement_error?: string | null
  sampled_at?: number
}
type ServiceOption = { unit: string; state: string; description: string }

export function NginxRuntimePanel({ siteName, notify }: { siteName: string; notify: (message: string) => void }) {
  const [runtime, setRuntime] = useState<RuntimeResponse | null>(null)
  const [services, setServices] = useState<ServiceOption[]>([])
  const [unit, setUnit] = useState('')
  const [cpu, setCpu] = useState('')
  const [memoryMiB, setMemoryMiB] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const refresh = useCallback(async () => {
    void api<{ services: ServiceOption[] }>('services').then((result) => setServices(result.services || [])).catch(() => setServices([]))
    try {
      const result = await api<RuntimeResponse>(`nginx/sites/${encodeURIComponent(siteName)}/runtime`)
      setRuntime(result)
      if (result.configured) {
        setUnit(result.unit || '')
        setCpu(result.cpu_quota_percent == null ? '' : String(result.cpu_quota_percent))
        setMemoryMiB(result.memory_max_bytes == null ? '' : String(Math.floor(result.memory_max_bytes / (1024 * 1024))))
      }
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'مصرف سرویس خوانده نشد.')
    } finally { setLoading(false) }
  }, [siteName])

  useEffect(() => { void refresh() }, [refresh])

  const refreshRuntime = useCallback(async () => {
    try {
      const result = await api<RuntimeResponse>(`nginx/sites/${encodeURIComponent(siteName)}/runtime`)
      setRuntime(result)
      setError('')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'مصرف سرویس خوانده نشد.')
    }
  }, [siteName])

  useEffect(() => {
    if (!runtime?.configured) return
    const timer = window.setInterval(() => {
      if (!document.hidden) void refreshRuntime()
    }, 15000)
    return () => window.clearInterval(timer)
  }, [runtime?.configured, refreshRuntime])

  async function save() {
    setBusy(true)
    try {
      await api(`nginx/sites/${encodeURIComponent(siteName)}/runtime`, {
        method: 'PUT',
        body: JSON.stringify({
          unit: unit.trim(),
          cpu_quota_percent: cpu.trim() ? Number(cpu) : null,
          memory_max_bytes: memoryMiB.trim() ? Number(memoryMiB) * 1024 * 1024 : null,
        }),
      })
      notify('محدودیت‌های systemd ذخیره شد؛ این سقف‌ها روی کل سرویس انتخاب‌شده اعمال می‌شوند.')
      await refresh()
    } catch (cause) { notify(cause instanceof Error ? cause.message : 'ذخیرهٔ محدودیت‌ها ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <section className="space-y-4 rounded-xl border border-white/10 bg-background/35 p-4">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div><h3 className="flex items-center gap-2 text-sm font-semibold text-white"><Activity size={15} className="text-primary"/>مصرف و سقف منابع برنامه</h3><p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">برای CPU و RAM باید برنامهٔ سایت در یک سرویس systemd جدا اجرا شود. این تنظیم روی کل سرویس اعمال می‌شود؛ اگر چند سایت از PHP-FPM مشترک استفاده کنند، محدودیت روی همهٔ آن‌ها اثر می‌گذارد.</p></div>
      <Button size="sm" variant="outline" disabled={loading || busy} onClick={() => { setLoading(true); void refresh() }}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>خواندن مصرف</Button>
    </div>
    {loading ? <Skeleton className="h-20"/> : <>
      {error && <p role="status" className="rounded-lg border border-amber-300/20 bg-amber-300/[.04] p-3 text-xs text-amber-100">{error}</p>}
      {runtime?.configured && runtime.measurement_error && <p role="status" className="rounded-lg border border-amber-300/20 bg-amber-300/[.04] p-3 text-xs leading-5 text-amber-100">{runtime.measurement_error}</p>}
      {runtime?.configured && <div className="grid gap-2 sm:grid-cols-3">
        <div className="rounded-lg border border-white/[.07] bg-card/60 p-3"><div className="flex items-center gap-2 text-xs text-muted-foreground"><Cpu size={14}/>CPU فعلی سرویس</div><div className="mt-2 font-mono text-lg text-white" dir="ltr">{runtime.cpu_percent == null ? 'در حال نمونه‌گیری…' : `${formatNumber(Math.round(runtime.cpu_percent * 10) / 10)}%`}</div><p className="mt-1 text-[10px] leading-4 text-muted-foreground">{runtime.cpu_percent == null ? 'نمونه‌گیری خودکار هر 15 ثانیه انجام می‌شود.' : `سقف ثبت‌شده: ${runtime.cpu_quota_percent == null ? 'بدون سقف' : `${formatNumber(runtime.cpu_quota_percent)}%`}`}</p></div>
        <div className="rounded-lg border border-white/[.07] bg-card/60 p-3"><div className="flex items-center gap-2 text-xs text-muted-foreground"><MemoryStick size={14}/>RAM فعلی سرویس</div><div className="mt-2 font-mono text-lg text-white" dir="ltr">{runtime.memory_current_bytes == null ? '—' : formatBytes(runtime.memory_current_bytes)}</div><p className="mt-1 text-[10px] text-muted-foreground">سقف: {runtime.memory_max_bytes == null ? 'بدون سقف' : formatBytes(runtime.memory_max_bytes)}</p></div>
        <div className="rounded-lg border border-white/[.07] bg-card/60 p-3"><div className="text-xs text-muted-foreground">واحد systemd متصل</div><div dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{runtime.unit}</div><p className="mt-1 text-[10px] text-muted-foreground">خوانده‌شده از cgroup سرویس</p></div>
      </div>}
      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
        <div className="space-y-2"><Label htmlFor={`runtime-service-${siteName}`}>انتخاب سرویس برنامه</Label><select id={`runtime-service-${siteName}`} value={unit} onChange={(event) => setUnit(event.target.value)} className="h-10 w-full rounded-md border border-input bg-background px-3 text-left font-mono text-xs text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" dir="ltr"><option value="">انتخاب از سرویس‌های systemd</option>{services.map((service) => <option key={service.unit} value={service.unit}>{service.unit} · {service.state}</option>)}</select><Input aria-label="نام واحد systemd سفارشی" dir="ltr" className="text-left font-mono" value={unit} onChange={(event) => setUnit(event.target.value)} placeholder="یا نام دقیق مثل customer-app.service"/><p className="text-[10px] text-muted-foreground">سرویس انتخابی باید برنامهٔ همین سایت را اجرا کند؛ Nginx و PHP-FPM مشترک قابل محدودکردن برای یک سایت نیستند.</p></div>
        <div className="grid grid-cols-2 gap-2"><div className="space-y-2"><Label htmlFor={`runtime-cpu-${siteName}`}>سقف CPU (%)</Label><Input id={`runtime-cpu-${siteName}`} type="number" inputMode="decimal" min="1" max="10000" step="0.1" lang="en" dir="ltr" value={cpu} onChange={(event) => setCpu(event.target.value)} placeholder="بدون سقف"/></div><div className="space-y-2"><Label htmlFor={`runtime-memory-${siteName}`}>سقف RAM (MiB)</Label><Input id={`runtime-memory-${siteName}`} type="number" inputMode="numeric" min="16" max="1048576" lang="en" dir="ltr" value={memoryMiB} onChange={(event) => setMemoryMiB(event.target.value)} placeholder="بدون سقف"/></div></div>
        <Button disabled={busy || !unit.trim()} onClick={() => void save()}><Save size={14}/>{busy ? 'در حال اعمال…' : 'اعمال محدودیت'}</Button>
      </div>
      <p className="text-[10px] leading-5 text-muted-foreground">خالی‌گذاشتن CPU یا RAM، سقف همان منبع را از این سرویس حذف می‌کند. برای پاک‌کردن هر دو، هر دو فیلد را خالی بگذار. systemd این محدودیت را پایدار ثبت می‌کند؛ سرویس و فایل‌های سایت حذف یا جابه‌جا نمی‌شوند.</p>
    </>}
  </section>
}
