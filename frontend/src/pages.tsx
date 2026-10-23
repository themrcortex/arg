import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Cpu,
  Database,
  Download,
  Globe,
  HardDrive,
  Info,
  Github,
  KeyRound,
  Network,
  Plus,
  Pencil,
  Power,
  Play,
  Square,
  RotateCw,
  Terminal,
  FolderOpen,
  ChevronUp,
  Sparkles,
  Code2,
  ChevronDown,
  Search,
  Save,
  Trash2,
  Radio,
  RefreshCw,
  ScrollText,
  ShieldCheck,
  UserRound,
  Users,
  Wifi,
  X,
} from 'lucide-react'
import {
  api,
  formatBytes,
  formatNumber,
  formatUptime,
  type AuditEvent,
  type Dashboard,
  type Firewall,
  type ManagedFirewallRule,
  type User,
  type Role,
} from './lib/api'
import { Badge } from './components/ui/badge'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Alert, AlertDescription, AlertTitle } from './components/ui/alert'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from './components/ui/dialog'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Progress } from './components/ui/progress'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './components/ui/select'
import { Skeleton } from './components/ui/skeleton'
import { Switch } from './components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './components/ui/table'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type Sample = { at: number; time: string; rx: number; tx: number }
type ResourceSample = { at: number; cpu: number; memory: number; disk: number }
type StoredSample = { at: number; cpu: number; memory: number; disk: number; rx: number; tx: number }
type HistoryRange = '5m' | '1h' | '24h' | '7d' | '30d' | 'custom'

function localDateTime(timestamp: number) {
  const date = new Date(timestamp)
  return new Date(date.getTime() - date.getTimezoneOffset() * 60000).toISOString().slice(0, 16)
}

function TimeRangeFilter({ range, setRange, from, setFrom, to, setTo, applyCustom }: { range: HistoryRange; setRange: (range: HistoryRange) => void; from: string; setFrom: (value: string) => void; to: string; setTo: (value: string) => void; applyCustom: () => void }) {
  const presets: HistoryRange[] = ['5m', '1h', '24h', '7d', '30d', 'custom']
  const labels: Record<HistoryRange, string> = { '5m': '5 دقیقه', '1h': '1 ساعت', '24h': '24 ساعت', '7d': '7 روز', '30d': '30 روز', custom: 'دلخواه' }
  return <div className="flex flex-wrap items-center gap-2" aria-label="فیلتر بازهٔ زمانی"><div className="flex flex-wrap gap-1 rounded-lg border border-border p-1">{presets.map((item) => <button key={item} type="button" onClick={() => setRange(item)} aria-pressed={range === item} className="min-h-8 rounded-md px-2 text-xs font-medium aria-pressed:bg-primary/20 aria-pressed:text-foreground">{labels[item]}</button>)}</div>{range === 'custom' && <div className="flex flex-wrap items-end gap-2"><label className="grid gap-1 text-[10px] text-muted-foreground">از تاریخ و ساعت<Input type="datetime-local" lang="en" dir="ltr" className="h-8 w-[190px] text-xs" value={from} max={to || undefined} onChange={(event) => setFrom(event.target.value)}/></label><label className="grid gap-1 text-[10px] text-muted-foreground">تا تاریخ و ساعت<Input type="datetime-local" lang="en" dir="ltr" className="h-8 w-[190px] text-xs" value={to} min={from || undefined} onChange={(event) => setTo(event.target.value)}/></label><Button size="sm" className="h-8" onClick={applyCustom}>اعمال بازه</Button></div>}</div>
}

function useDashboard(notify: (message: string) => void) {
  const [data, setData] = useState<Dashboard | null>(null)
  const [samples, setSamples] = useState<StoredSample[]>([])
  const [historyEnabled, setHistoryEnabled] = useState(true)
  const [range, setRange] = useState<HistoryRange>('5m')
  const [customFrom, setCustomFrom] = useState(() => localDateTime(Date.now() - 3600000))
  const [customTo, setCustomTo] = useState(() => localDateTime(Date.now()))
  const [appliedCustom, setAppliedCustom] = useState<{ from: number; to: number } | null>(null)
  const [loading, setLoading] = useState(true)
  const [dashboardError, setDashboardError] = useState(false)
  const [historyError, setHistoryError] = useState(false)
  const refresh = useCallback(async () => {
    try {
      const query = range === 'custom' && appliedCustom ? `from=${appliedCustom.from}&to=${appliedCustom.to}` : `range=${range === 'custom' ? '5m' : range}`
      const [dashboardResult, historyResult] = await Promise.allSettled([api<Dashboard>('dashboard'), api<{ samples: StoredSample[]; config: MonitoringConfig }>(`history?${query}`)])
      setDashboardError(dashboardResult.status === 'rejected')
      setHistoryError(historyResult.status === 'rejected')
      if (dashboardResult.status === 'fulfilled') setData(dashboardResult.value)
      if (historyResult.status === 'fulfilled') {
        setSamples(historyResult.value.samples)
        setHistoryEnabled(historyResult.value.config.enabled)
      }
      if (dashboardResult.status === 'rejected' && historyResult.status === 'rejected') throw dashboardResult.reason
      if (dashboardResult.status === 'rejected') notify('خواندن وضعیت فعلی سرور ناموفق بود')
      if (historyResult.status === 'rejected') notify('خواندن تاریخچهٔ منابع ناموفق بود')
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن اطلاعات سرور ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify, range, appliedCustom])
  useEffect(() => {
    void refresh()
    const timer = window.setInterval(refresh, 15000)
    return () => window.clearInterval(timer)
  }, [refresh])
  const history = samples.map((sample) => ({ at: sample.at, time: new Date(sample.at * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }), rx: sample.rx, tx: sample.tx }))
  const resourceHistory = samples.map(({ at, cpu, memory, disk }) => ({ at, cpu, memory, disk }))
  function applyCustomRange() {
    const from = new Date(customFrom).getTime()
    const to = new Date(customTo).getTime()
    if (!Number.isFinite(from) || !Number.isFinite(to) || from >= to || to > Date.now() || to - from > 366 * 86400000) {
      notify('بازهٔ دلخواه نامعتبر است؛ شروع و پایان را بررسی کن (حداکثر 366 روز).')
      return
    }
    setAppliedCustom({ from: Math.floor(from / 1000), to: Math.floor(to / 1000) })
    setRange('custom')
  }
  return { data, history, resourceHistory, cpu: samples.at(-1)?.cpu ?? null, loading, refresh, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, historyEnabled, dashboardError, historyError }
}

function SectionCard({
  title,
  description,
  right,
  children,
  className = '',
}: {
  title: string
  description?: string
  right?: ReactNode
  children: ReactNode
  className?: string
}) {
  return (
    <Card className={className}>
      <CardHeader className="flex-row items-start justify-between gap-4 space-y-0">
        <div>
          <CardTitle>{title}</CardTitle>
          {description && <CardDescription className="mt-1.5">{description}</CardDescription>}
        </div>
        {right}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  )
}
function Metric({
  icon: Icon,
  label,
  value,
  detail,
  progress,
}: {
  icon: typeof Cpu
  label: string
  value: string
  detail: string
  progress?: number
}) {
  return (
    <Card className="metric-card min-w-0">
      <CardContent className="p-5">
        <div className="flex items-start justify-between gap-2">
          <span className="text-xs font-medium text-muted-foreground">{label}</span>
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-primary/15 text-primary">
            <Icon size={19} strokeWidth={1.8} />
          </span>
        </div>
        <div className="mt-5 truncate text-[26px] font-extrabold tracking-tight text-white" title={value}>
          {value}
        </div>
        <div className="mt-1.5 truncate text-xs text-muted-foreground">{detail}</div>
        {progress !== undefined && <Progress className="mt-4" value={progress} />}
      </CardContent>
    </Card>
  )
}
function LoadingCards() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      {[0, 1, 2, 3].map((n) => (
        <Skeleton key={n} className="h-[170px]" />
      ))}
    </div>
  )
}

function AlertControls({ user, cpu, memory, disk, notify }: { user: User; cpu: number | null; memory: number | null; disk: number | null; notify: (message: string) => void }) {
  const [limits, setLimits] = useState({ cpu: 85, memory: 85, disk: 85 })
  const [busy, setBusy] = useState(false)
  useEffect(() => { void api<typeof limits>('monitoring/alerts').then(setLimits).catch(() => undefined) }, [])
  async function save(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    try { setLimits(await api<typeof limits>('monitoring/alerts', { method: 'PUT', body: JSON.stringify(limits) })); notify('آستانه‌های هشدار ذخیره شدند.') }
    catch (error) { notify(error instanceof Error ? error.message : 'ذخیره ناموفق بود') }
    finally { setBusy(false) }
  }
  return <SectionCard title="آستانه‌های هشدار" description="تنظیمات در سرور ذخیره می‌شوند و وضعیت فعلی با آنها سنجیده می‌شود">
    <form onSubmit={save} className="grid gap-4 md:grid-cols-3">{([['cpu', 'پردازنده', cpu], ['memory', 'حافظه', memory], ['disk', 'دیسک', disk]] as const).map(([key, label, value]) => <div key={key} className="rounded-xl border border-white/10 bg-white/[.025] p-4"><div className="mb-3 flex items-center justify-between text-xs"><Label htmlFor={`limit-${key}`}>{label}</Label><Badge variant={value !== null && value >= limits[key] ? 'warning' : 'success'}>{value !== null && value >= limits[key] ? 'بالاتر از حد' : 'عادی'}</Badge></div><div className="flex items-center gap-2"><Input id={`limit-${key}`} type="number" lang="en" dir="ltr" min={1} max={100} value={limits[key]} disabled={user.role !== 'admin'} onChange={(event) => setLimits((old) => ({ ...old, [key]: Number(event.target.value) }))}/><span className="text-sm">٪</span></div></div>)}{user.role === 'admin' && <Button type="submit" disabled={busy} className="md:col-span-3 md:w-fit">ذخیرهٔ آستانه‌ها</Button>}</form>
  </SectionCard>
}

export function PortProbeControls({ notify }: { notify: (message: string) => void }) {
  const [port, setPort] = useState('443')
  const [result, setResult] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  async function submit(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    try { const response = await api<{ port: number; open: boolean }>('network/probe', { method: 'POST', body: JSON.stringify({ port: Number(port) }) }); setResult(response.open ? `پورت ${response.port} روی خود سرور اتصال TCP می‌پذیرد.` : `پورت ${response.port} روی خود سرور اتصال TCP نمی‌پذیرد.`) }
    catch (error) { notify(error instanceof Error ? error.message : 'بررسی پورت ناموفق بود') }
    finally { setBusy(false) }
  }
  return <SectionCard title="آزمایش اتصال TCP" description="یک اتصال واقعی از داخل سرور به پورت موردنظر برقرار می‌شود"><form onSubmit={submit} className="flex flex-wrap items-end gap-3"><div className="space-y-2"><Label htmlFor="port-probe">پورت</Label><Input id="port-probe" type="number" lang="en" dir="ltr" min={1} max={65535} value={port} onChange={(event) => setPort(event.target.value)} className="w-36"/></div><Button type="submit" disabled={busy}>{busy ? 'در حال بررسی…' : 'بررسی اتصال'}</Button>{result && <span className="text-xs text-muted-foreground">{result}</span>}</form></SectionCard>
}

function ArgKpi({ icon, label, value, detail, progress, tone = 'copper', ltr = true }: {
  icon: string
  label: string
  value: string
  detail: string
  progress?: number
  tone?: 'copper' | 'green' | 'teal' | 'blue'
  ltr?: boolean
}) {
  return <div className="arg-kpi min-w-0 rounded-[14px] border p-4" data-tone={tone}>
    <div className="flex items-center justify-between gap-2"><span className="truncate text-xs text-muted-foreground">{label}</span><img src={`/arg/icons/${icon}.svg`} alt="" aria-hidden="true" className="h-5 w-5 shrink-0"/></div>
    <div className="mt-4 truncate text-[25px] font-bold leading-none tabular-nums text-foreground" dir={ltr ? 'ltr' : 'rtl'} title={value}>{value}</div>
    {progress !== undefined && <div className="arg-kpi-track mt-4 h-1 overflow-hidden rounded-full"><div className="arg-kpi-fill h-full rounded-full" style={{ width: `${Math.min(100, Math.max(0, progress))}%` }}/></div>}
    <div className="mt-3 truncate text-xs text-muted-foreground" title={detail}>{detail}</div>
  </div>
}

function ArgChartPanel({ title, icon, children, description }: { title: string; icon: string; children: ReactNode; description: string }) {
  return <section className="arg-dashboard-panel min-w-0 rounded-[14px] border p-4" aria-label={title}>
    <div className="mb-4 flex items-center gap-2"><img src={`/arg/icons/${icon}.svg`} alt="" aria-hidden="true" className="h-5 w-5"/><h2 className="text-sm font-bold text-foreground">{title}</h2></div>
    {children}
    <p className="mt-3 text-xs text-muted-foreground">{description}</p>
  </section>
}

export function DashboardPage({ user, notify }: { user: User } & Common) {
  const { data, history, resourceHistory, cpu, loading, refresh, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, historyEnabled, dashboardError, historyError } = useDashboard(notify)
  const [firewall, setFirewall] = useState<Firewall | null>(null)
  const [ruleCount, setRuleCount] = useState<number | null>(null)
  useEffect(() => {
    let active = true
    const readFirewall = async () => {
      const [status, rules] = await Promise.allSettled([api<Firewall>('firewall'), api<{ rules: ManagedFirewallRule[] }>('firewall/rules')])
      if (!active) return
      if (status.status === 'fulfilled') setFirewall(status.value)
      if (rules.status === 'fulfilled') setRuleCount(rules.value.rules.length)
    }
    void readFirewall()
    const timer = window.setInterval(() => void readFirewall(), 30000)
    return () => { active = false; window.clearInterval(timer) }
  }, [])
  const memory = data?.memory_kib ? (1 - data.available_kib / data.memory_kib) * 100 : null
  const disk = data?.disk_total_bytes && typeof data.disk_free_bytes === 'number' ? (1 - data.disk_free_bytes / data.disk_total_bytes) * 100 : null
  const load = data?.load?.trim().split(/\s+/) || []
  const percent = (value: number | null) => value === null ? '—' : `${formatNumber(Math.round(value * 10) / 10)}%`
  const firewallActive = firewall?.available && /active/i.test(firewall.status) && !/inactive/i.test(firewall.status)
  return <div className="arg-dashboard space-y-3.5">
    <div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-xs font-semibold text-primary" dir="ltr">ARG / ارگ</div><h1 className="mt-1 text-[27px] font-bold text-foreground">نمای کلی سرور</h1></div><Button variant="outline" size="sm" onClick={() => void refresh()}><RefreshCw size={15}/> تازه‌سازی</Button></div>
    <section className="arg-dashboard-hero relative min-h-[150px] overflow-hidden rounded-[14px] border p-5 md:p-6">
      <img src="/arg/illustrations/arg-dashboard-banner.svg" alt="" aria-hidden="true" className="arg-hero-art pointer-events-none absolute bottom-0 left-0 h-full w-[75%] object-cover opacity-[.28]"/>
      <div className="relative z-10"><div className="text-xs font-semibold text-primary" dir="ltr">ARG / SERVER OVERVIEW</div><h2 className="mt-4 text-xl font-bold text-foreground md:text-2xl">به ارگ خوش آمدید</h2><p className="mt-2 text-sm text-muted-foreground">وضعیت منابع، شبکه و امنیت سرور در یک نگاه.</p>{data?.hostname && <div className="mt-4 inline-flex max-w-full items-center gap-2 rounded-lg border border-border bg-background/60 px-3 py-1.5 text-xs"><span className="text-muted-foreground">سرور</span><span className="truncate font-mono text-foreground" dir="ltr">{data.hostname}</span></div>}</div>
    </section>
    {(dashboardError || historyError) && <Alert className="border-amber-400/25 bg-amber-400/[.06]"><AlertDescription>{dashboardError && historyError ? 'دریافت وضعیت سرور و تاریخچه ناموفق بود.' : dashboardError ? 'دریافت وضعیت فعلی سرور ناموفق بود.' : 'دریافت تاریخچهٔ منابع ناموفق بود.'} <button type="button" className="mr-2 font-semibold text-primary underline" onClick={() => void refresh()}>تلاش دوباره</button></AlertDescription></Alert>}
    {loading && !data ? <div className="arg-kpi-grid grid gap-3">{Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="h-[142px]"/>)}</div> : <section className="arg-kpi-grid grid gap-3" aria-label="شاخص‌های سرور">
      <ArgKpi icon="metric-cpu" label="پردازنده" value={percent(cpu)} detail={cpu === null ? 'نمونهٔ CPU موجود نیست' : 'آخرین نمونهٔ ثبت‌شده'} progress={cpu ?? undefined} tone="green"/>
      <ArgKpi icon="metric-memory" label="حافظهٔ RAM" value={percent(memory)} detail={data?.memory_kib ? `${formatBytes((data.memory_kib - data.available_kib) * 1024)} از ${formatBytes(data.memory_kib * 1024)}` : 'داده‌ای در دسترس نیست'} progress={memory ?? undefined}/>
      <ArgKpi icon="metric-disk" label="دیسک ریشه /" value={percent(disk)} detail={disk !== null && data?.disk_total_bytes && typeof data.disk_free_bytes === 'number' ? `${formatBytes(data.disk_total_bytes - data.disk_free_bytes)} از ${formatBytes(data.disk_total_bytes)}` : 'داده‌ای در دسترس نیست'} progress={disk ?? undefined} tone="teal"/>
      <ArgKpi icon="metric-clock" label="زمان روشن‌بودن" value={data ? data.uptime_seconds >= 86400 ? `${formatNumber(Math.floor(data.uptime_seconds / 86400))} روز` : formatUptime(data.uptime_seconds) : '—'} detail={data ? formatUptime(data.uptime_seconds) : 'از آخرین راه‌اندازی سرور'} ltr={false}/>
      <ArgKpi icon="metric-load" label="میانگین بار" value={load[0] || '—'} detail={load.length >= 3 ? `5 دقیقه: ${load[1]} · 15 دقیقه: ${load[2]}` : 'میانگین بار Linux'}/>
      <ArgKpi icon="metric-app" label="مصرف پنل ARG" value={data ? `${formatNumber(Math.round(data.process_cpu_percent * 10) / 10)}%` : '—'} detail={data ? `CPU · RAM ${formatBytes(data.process_rss_bytes)}` : 'داده‌ای در دسترس نیست'} tone="blue"/>
    </section>}
    <div className="flex flex-wrap items-center justify-between gap-3 pt-2"><h2 className="text-base font-bold text-foreground">تاریخچهٔ منابع</h2><TimeRangeFilter range={range} setRange={setRange} from={customFrom} setFrom={setCustomFrom} to={customTo} setTo={setCustomTo} applyCustom={applyCustomRange}/></div>
    {!historyEnabled && <HistoryPaused/>}
    <div className="arg-chart-grid grid gap-3">
      <ArgChartPanel icon="metric-cpu" title="استفاده از پردازنده" description="درصد مصرف CPU در بازهٔ انتخاب‌شده"><TimeSeriesChart samples={resourceHistory} series={[{ key: 'cpu', label: 'پردازنده', color: '#53C992' }]} max={100} unit="%"/></ArgChartPanel>
      <ArgChartPanel icon="metric-memory" title="مصرف حافظه" description="درصد RAM استفاده‌شده در بازهٔ انتخاب‌شده"><TimeSeriesChart samples={resourceHistory} series={[{ key: 'memory', label: 'حافظه', color: '#DDA46F' }]} max={100} unit="%"/></ArgChartPanel>
      <ArgChartPanel icon="metric-network" title="ترافیک شبکه" description="نرخ دریافت و ارسال بر حسب بایت بر ثانیه"><TrafficChart history={history}/></ArgChartPanel>
    </div>
    <div className="arg-operation-grid grid gap-3">
      <section className="arg-dashboard-panel min-w-0 rounded-[14px] border p-4"><div className="mb-4 flex items-center justify-between gap-2"><h2 className="font-bold text-foreground">رابط‌های شبکه</h2><a href="#/network" className="text-xs text-primary hover:underline">مشاهدهٔ همه</a></div>{data?.interfaces.length ? <div className="overflow-x-auto"><table className="w-full text-xs"><thead><tr className="border-b border-border text-muted-foreground"><th className="pb-3 text-right">رابط</th><th className="pb-3 text-right">نشانی</th><th className="pb-3 text-left">وضعیت</th></tr></thead><tbody>{data.interfaces.slice(0, 4).map((item) => <tr key={item.name} className="border-b border-border/60 last:border-0"><td className="py-3 font-mono" dir="ltr">{item.name}</td><td className="max-w-24 truncate py-3 font-mono text-muted-foreground" dir="ltr" title={item.addresses?.join(', ')}>{item.addresses?.[0] || '—'}</td><td className="py-3 text-left"><span className={item.state === 'up' ? 'text-emerald-300' : 'text-amber-300'}>{item.state === 'up' ? 'فعال' : item.state}</span></td></tr>)}</tbody></table></div> : <p className="py-6 text-sm text-muted-foreground">رابط شبکه‌ای گزارش نشده است.</p>}</section>
      <section className="arg-dashboard-panel min-w-0 rounded-[14px] border p-4"><div className="mb-4 flex items-center justify-between gap-2"><h2 className="font-bold text-foreground">پورت‌های شنونده</h2><a href="#/ports" className="text-xs text-primary hover:underline">جزئیات</a></div>{data?.ports.length ? <div className="overflow-x-auto"><table className="w-full text-xs"><thead><tr className="border-b border-border text-muted-foreground"><th className="pb-3 text-right">پورت</th><th className="pb-3 text-right">پروتکل</th><th className="pb-3 text-left">نسخهٔ IP</th></tr></thead><tbody>{data.ports.slice(0, 4).map((port, index) => <tr key={`${port.port}-${port.protocol}-${port.ipv6}-${index}`} className="border-b border-border/60 last:border-0"><td className="py-3 font-mono" dir="ltr">{port.port}</td><td className="py-3 font-mono">{port.protocol}</td><td className="py-3 text-left font-mono">{port.ipv6 ? 'IPv6' : 'IPv4'}</td></tr>)}</tbody></table></div> : <p className="py-6 text-sm text-muted-foreground">پورت شنونده‌ای گزارش نشده است.</p>}</section>
      <section className="arg-dashboard-panel min-w-0 rounded-[14px] border p-4"><div className="mb-4 flex items-center justify-between gap-2"><h2 className="font-bold text-foreground">وضعیت فایروال</h2><a href="#/firewall" className="text-xs text-primary hover:underline">جزئیات</a></div><div className="grid grid-cols-2 gap-3"><div className="rounded-lg border border-border bg-secondary/40 p-3"><span className="text-xs text-muted-foreground">وضعیت UFW</span><div className={`mt-3 font-bold ${firewall?.available ? firewallActive ? 'text-emerald-300' : 'text-amber-300' : 'text-muted-foreground'}`}>{firewall ? firewall.available ? firewallActive ? 'فعال' : 'غیرفعال' : 'نصب نشده' : 'نامشخص'}</div></div><div className="rounded-lg border border-border bg-secondary/40 p-3"><span className="text-xs text-muted-foreground">قوانین مدیریت‌شده</span><div className="mt-3 font-mono text-lg font-bold" dir="ltr">{ruleCount === null ? '—' : formatNumber(ruleCount)}</div></div></div></section>
    </div>
    {data && <AlertControls user={user} cpu={cpu} memory={memory} disk={disk} notify={notify}/>}
  </div>
}

function DataRows({ rows }: { rows: { label: string; value: string; detail?: string }[] }) {
  return <Table><TableHeader><TableRow><TableHead>شاخص</TableHead><TableHead>مقدار</TableHead><TableHead className="hidden md:table-cell">توضیح</TableHead></TableRow></TableHeader><TableBody>{rows.map((row) => <TableRow key={row.label}><TableCell className="font-medium">{row.label}</TableCell><TableCell><span className="font-mono font-semibold text-white tabular-nums" dir="ltr">{row.value}</span></TableCell><TableCell className="hidden text-xs text-muted-foreground md:table-cell">{row.detail || '—'}</TableCell></TableRow>)}</TableBody></Table>
}

export function SystemPage({ user, Heading, notify }: { user: User } & Common) {
  const { data, resourceHistory, cpu, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, loading, refresh, historyEnabled } = useDashboard(notify)
  const totalMemory = (data?.memory_kib || 0) * 1024
  const availableMemory = (data?.available_kib || 0) * 1024
  const memoryPercent = totalMemory ? Math.round((1 - availableMemory / totalMemory) * 100) : 0
  const totalDisk = data?.disk_total_bytes || 0
  const freeDisk = data?.disk_free_bytes || 0
  const diskPercent = totalDisk ? Math.round((1 - freeDisk / totalDisk) * 100) : 0
  const load = (data?.load || '').split(' ')
  const memoryDetail = data?.memory_details
  return <>
    <Heading title="سیستم و منابع" description="پردازنده، حافظه و دیسک را با جزئیات واقعی سرور بررسی کن." action={<Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button>}/>
    {loading && !data ? <LoadingCards/> : <>
      <div className="grid gap-4 md:grid-cols-3"><Metric icon={Cpu} label="CPU" value={cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`} detail={`بار 1/5/15 دقیقه: ${data?.load || '—'}`} progress={cpu || 0}/><Metric icon={Database} label="حافظه" value={`${formatNumber(memoryPercent)}%`} detail={`${formatBytes(totalMemory - availableMemory)} از ${formatBytes(totalMemory)}`} progress={memoryPercent}/><Metric icon={HardDrive} label="دیسک ریشه" value={`${formatNumber(diskPercent)}%`} detail={`${formatBytes(totalDisk - freeDisk)} از ${formatBytes(totalDisk)}`} progress={diskPercent}/></div>
      <div className="mt-5"><SectionCard title="تاریخچهٔ منابع" description="نمونه‌ها توسط سرویس Rust ذخیره می‌شوند" right={<TimeRangeFilter range={range} setRange={setRange} from={customFrom} setFrom={setCustomFrom} to={customTo} setTo={setCustomTo} applyCustom={applyCustomRange}/>}>{!historyEnabled && <HistoryPaused/>}<ResourceChart samples={resourceHistory}/></SectionCard></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="پردازنده و بار سیستم" description="مقادیر لحظه‌ای و میانگین بار Linux"><DataRows rows={[{ label: 'مصرف CPU', value: cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`, detail: 'میانگین هسته‌ها' }, { label: 'هسته‌های در دسترس', value: formatNumber(data?.cpu_cores || 0) }, { label: 'بار 1 دقیقه', value: load[0] || '—' }, { label: 'بار 5 دقیقه', value: load[1] || '—' }, { label: 'بار 15 دقیقه', value: load[2] || '—' }, { label: 'زمان روشن‌بودن', value: `${formatNumber(Math.floor((data?.uptime_seconds || 0) / 86400))}d ${formatNumber(Math.floor(((data?.uptime_seconds || 0) % 86400) / 3600))}h` }]}/></SectionCard><SectionCard title="حافظه و Swap" description="مقادیر خوانده‌شده از /proc/meminfo"><DataRows rows={[{ label: 'کل RAM', value: formatBytes(totalMemory) }, { label: 'مصرف‌شده', value: formatBytes(totalMemory - availableMemory), detail: 'بر اساس MemAvailable' }, { label: 'در دسترس', value: formatBytes(availableMemory) }, { label: 'Cache', value: formatBytes((memoryDetail?.cached_kib || 0) * 1024) }, { label: 'Buffer', value: formatBytes((memoryDetail?.buffers_kib || 0) * 1024) }, { label: 'Swap کل', value: formatBytes((data?.swap_total_kib || 0) * 1024) }, { label: 'Swap مصرف‌شده', value: formatBytes(((data?.swap_total_kib || 0) - (data?.swap_free_kib || 0)) * 1024) }]}/></SectionCard></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="دیسک ریشه /" description="فضای فایل‌سیستم ریشه"><DataRows rows={[{ label: 'ظرفیت کل', value: totalDisk ? formatBytes(totalDisk) : '—' }, { label: 'مصرف‌شده', value: totalDisk ? formatBytes(totalDisk - freeDisk) : '—' }, { label: 'آزاد', value: totalDisk ? formatBytes(freeDisk) : '—' }, { label: 'درصد مصرف', value: totalDisk ? `${formatNumber(diskPercent)}%` : '—' }]}/></SectionCard><SectionCard title="مصرف پنل ARG" description="اندازه‌گیری همین فرایند"><DataRows rows={[{ label: 'CPU پنل', value: `${formatNumber(Math.round((data?.process_cpu_percent || 0) * 10) / 10)}%` }, { label: 'RAM پنل', value: formatBytes(data?.process_rss_bytes || 0), detail: 'حافظهٔ مقیم فرایند (RSS)' }]}/></SectionCard></div>
      <div className="mt-5"><AlertControls user={user} cpu={cpu} memory={memoryPercent} disk={diskPercent} notify={notify}/></div>
    </>}
  </>
}

export function PortsPage({ Heading, notify }: Common) {
  const { data, loading, refresh } = useDashboard(notify)
  const [query, setQuery] = useState('')
  const [protocol, setProtocol] = useState<'all' | 'TCP' | 'UDP'>('all')
  const [ipVersion, setIpVersion] = useState<'all' | 'IPv4' | 'IPv6'>('all')
  const ports = (data?.ports || []).filter((port) => (protocol === 'all' || port.protocol === protocol) && (ipVersion === 'all' || (port.ipv6 ? 'IPv6' : 'IPv4') === ipVersion) && (!query.trim() || String(port.port).includes(query.trim())))
  return <>
    <Heading title="پورت‌های شنونده" description="پورت‌های باز در خود سرور را پیدا کن و اتصال TCP را آزمایش کن." action={<Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button>}/>
    <div className="grid gap-4 sm:grid-cols-3"><Metric icon={Radio} label="همهٔ شنونده‌ها" value={formatNumber(data?.ports.length || 0)} detail="TCP و UDP روی IPv4 و IPv6"/><Metric icon={Network} label="TCP" value={formatNumber(data?.ports.filter((port) => port.protocol === 'TCP').length || 0)} detail="در حالت LISTEN"/><Metric icon={Wifi} label="UDP" value={formatNumber(data?.ports.filter((port) => port.protocol === 'UDP').length || 0)} detail="سوکت‌های UDP ثبت‌شده"/></div>
    <div className="mt-5"><PortProbeControls notify={notify}/></div>
    <div className="mt-5"><SectionCard title="فهرست پورت‌ها" description="جست‌وجو و فیلتر مستقیم در فهرست کرنل" right={<Badge variant="outline">{formatNumber(ports.length)} مورد</Badge>}>
      <div className="mb-5 space-y-3 rounded-[14px] border border-white/[.08] bg-[#121b23] p-3">
        <label className="relative block"><span className="sr-only">جست‌وجوی شمارهٔ پورت</span><Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="pr-9" inputMode="numeric" lang="en" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="شمارهٔ پورت را جست‌وجو کن…"/></label>
        <div className="flex flex-wrap items-center gap-3"><div className="flex flex-wrap gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1" aria-label="فیلتر پروتکل">{(['all','TCP','UDP'] as const).map((value) => <button key={value} onClick={() => setProtocol(value)} aria-pressed={protocol === value} className={`rounded-lg px-3 py-1.5 text-xs transition ${protocol === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:text-white'}`}>{value === 'all' ? 'همهٔ پروتکل‌ها' : value}</button>)}</div><div className="flex flex-wrap gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1" aria-label="فیلتر نسخهٔ IP">{(['all','IPv4','IPv6'] as const).map((value) => <button key={value} onClick={() => setIpVersion(value)} aria-pressed={ipVersion === value} className={`rounded-lg px-3 py-1.5 text-xs transition ${ipVersion === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:text-white'}`}>{value === 'all' ? 'همهٔ IPها' : value}</button>)}</div>{(query || protocol !== 'all' || ipVersion !== 'all') && <button className="text-xs text-[#dda46f] hover:text-white" onClick={() => { setQuery(''); setProtocol('all'); setIpVersion('all') }}>پاک‌کردن فیلترها</button>}</div>
      </div>
      {loading && !data ? <Skeleton className="h-48"/> : <PortsTable ports={ports}/>}
    </SectionCard></div>
    <Alert className="mt-5"><Info size={17} className="mb-2 text-primary"/><AlertTitle>شنود محلی و دسترسی بیرونی</AlertTitle><AlertDescription>وجود پورت در این فهرست به معنی دسترسی از اینترنت نیست. قانون UFW و فایروال ارائه‌دهنده را نیز بررسی کن. حجم ترافیک به‌تفکیک پورت هنوز اندازه‌گیری نمی‌شود.</AlertDescription></Alert>
  </>
}

function HistoryPaused() { return <div className="mb-4 flex items-center gap-2 rounded-xl border border-amber-400/20 bg-amber-400/[.06] px-4 py-3 text-xs text-amber-200"><span className="h-2 w-2 rounded-full bg-amber-400"/> ثبت تاریخچه متوقف است. برای ادامه، از تنظیمات پنل آن را فعال کن.</div> }

function ResourceChart({ samples }: { samples: ResourceSample[] }) {
  return <TimeSeriesChart samples={samples} series={[{ key: 'cpu', label: 'پردازنده', color: '#53c992' }, { key: 'memory', label: 'حافظه', color: '#dda46f' }, { key: 'disk', label: 'دیسک', color: '#49b8a8' }]} max={100} unit="%" />
}

type ChartSeries = { key: string; label: string; color: string }
function TimeSeriesChart({ samples, series, max, unit }: { samples: (ResourceSample | Sample)[]; series: ChartSeries[]; max: number; unit: string }) {
  const [hover, setHover] = useState<number | null>(null)
  if (!samples.length) return <div className="flex h-[195px] flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-background p-6 text-center text-sm text-muted-foreground"><img src="/arg/illustrations/arg-empty-state.svg" alt="" aria-hidden="true" className="h-12 w-12 opacity-50"/>داده‌ای در این بازه ثبت نشده است.</div>
  const width = 760, left = 50, right = 15, top = 20, bottom = 235
  const x = (index: number) => left + index * (width - left - right) / Math.max(samples.length - 1, 1)
  const y = (value: number) => bottom - Math.min(Math.max(value / max, 0), 1) * (bottom - top)
  const selected = hover === null ? samples.length - 1 : Math.max(0, Math.min(hover, samples.length - 1))
  const current = samples[selected]
  const read = (sample: ResourceSample | Sample, key: string) => Number((sample as unknown as Record<string, number>)[key]) || 0
  const display = (value: number) => unit === '%' ? `${formatNumber(Math.round(value * 10) / 10)}%` : `${formatBytes(value)}/s`
  const stamp = (at: number) => new Date(at * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  return <div className="overflow-hidden rounded-xl border border-border bg-background">
    <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">{series.map((item) => <div key={item.key} className="flex items-center gap-2 rounded-lg bg-secondary/40 px-2 py-1 text-xs"><span className="h-1.5 w-1.5 rounded-full" style={{ background: item.color }}/><span className="text-muted-foreground">{item.label}</span><b className="font-mono font-semibold text-foreground tabular-nums" dir="ltr">{display(read(current, item.key))}</b></div>)}<span className="mr-auto font-mono text-xs text-muted-foreground" dir="ltr">{stamp(current.at)}</span></div>
    <div className="h-[190px] px-2 pt-2 md:px-3" dir="ltr"><svg viewBox="0 0 760 276" width="100%" height="100%" preserveAspectRatio="none" onMouseLeave={() => setHover(null)} onMouseMove={(event) => { const rect = event.currentTarget.getBoundingClientRect(); setHover(Math.round(((event.clientX - rect.left) / rect.width * width - left) / (width - left - right) * (samples.length - 1))) }} role="img" aria-label={`تاریخچهٔ ${series.map((item) => item.label).join('، ')}`}>
      <defs><linearGradient id={`overview-fill-${series[0].key}`} x1="0" y1="0" x2="0" y2="1"><stop stopColor={series[0].color} stopOpacity=".24"/><stop offset="1" stopColor={series[0].color} stopOpacity="0"/></linearGradient></defs>
      {[0, .25, .5, .75, 1].map((fraction) => { const py = y(fraction * max); return <g key={fraction}><line x1={left} x2={width-right} y1={py} y2={py} stroke="#2a3540" strokeOpacity=".65" strokeDasharray="2 7"/><text x={left-11} y={py+4} fill="#93a0aa" fontSize="10" textAnchor="end">{unit === '%' ? `${Math.round(fraction * max)}%` : formatBytes(fraction * max)}</text></g> })}
      {series.map((item, lineIndex) => { const points = samples.map((sample, index) => `${x(index)},${y(read(sample, item.key))}`).join(' '); return <g key={item.key}>{lineIndex === 0 && <polygon points={`${left},${bottom} ${points} ${x(samples.length - 1)},${bottom}`} fill={`url(#overview-fill-${item.key})`}/>}<polyline points={points} fill="none" stroke={item.color} strokeOpacity={lineIndex === 0 ? 1 : .86} strokeWidth={lineIndex === 0 ? 3 : 2.4} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke"/></g> })}
      <line x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} stroke="#dda46f" strokeOpacity=".42" strokeDasharray="3 6"/>
      {series.map((item) => <circle key={item.key} cx={x(selected)} cy={y(read(current, item.key))} r="4.8" fill={item.color} stroke="#0c1219" strokeWidth="2"/>)}
      <text x={left} y="266" fill="#93a0aa" fontSize="11">{stamp(samples[0].at)}</text><text x={width-right} y="266" textAnchor="end" fill="#93a0aa" fontSize="11">{stamp(samples[samples.length - 1].at)}</text>
    </svg></div>
  </div>
}

function DistributionBars({ title, description, items }: {
  title: string
  description: string
  items: { label: string; value: number; display?: string }[]
}) {
  const max = Math.max(1, ...items.map((item) => item.value))
  return <Card className="overflow-hidden rounded-2xl border-border/80 bg-card/90 shadow-[0_16px_48px_-36px_rgba(0,0,0,.9)]">
    <CardHeader className="flex-row items-start justify-between gap-6 space-y-0 px-6 pb-5 pt-6 sm:px-7">
      <div className="min-w-0 text-right"><CardTitle className="text-[17px]">{title}</CardTitle><CardDescription className="mt-2 leading-6">{description}</CardDescription></div>
      <span className="grid h-9 w-9 shrink-0 place-items-center text-primary/90"><Activity size={21} strokeWidth={1.7}/></span>
    </CardHeader>
    <CardContent className="px-6 pb-7 pt-0 sm:px-7">
      {items.length ? <div className="grid gap-x-5 gap-y-5 sm:grid-cols-2">
        {items.map((item, index) => <div key={`${item.label}-${index}`} className="min-w-0">
          <div className="mb-2.5 flex items-center justify-between gap-3 text-xs"><span className="truncate text-muted-foreground">{item.label}</span><span className="font-mono font-medium tabular-nums text-foreground" dir="ltr">{item.display || formatNumber(item.value)}</span></div>
          <div className="h-2 overflow-hidden rounded-full bg-secondary/80"><div className="h-full rounded-full transition-[width] duration-500" style={{ width: `${item.value > 0 ? Math.max(2, item.value / max * 100) : 0}%`, background: index % 3 === 1 ? '#53c992' : index % 3 === 2 ? '#67b4d8' : '#dda46f' }}/></div>
        </div>)}
      </div> : <p className="py-6 text-center text-sm text-muted-foreground">داده‌ای برای نمایش نمودار وجود ندارد.</p>}
    </CardContent>
  </Card>
}

function TrafficChart({ history }: { history: Sample[] }) {
  return <TimeSeriesChart samples={history} series={[{ key: 'rx', label: 'دریافت', color: '#69acde' }, { key: 'tx', label: 'ارسال', color: '#e3b15d' }]} max={Math.max(1024, ...history.flatMap((sample) => [sample.rx, sample.tx])) * 1.12} unit="bytes" />
}

function PortsTable({ ports }: { ports: Dashboard['ports'] }) {
  return ports.length ? (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>پورت</TableHead>
          <TableHead>پروتکل</TableHead>
          <TableHead>نسخهٔ IP</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {ports.map((p, i) => (
          <TableRow key={`${p.protocol}-${p.port}-${p.ipv6}-${i}`}>
            <TableCell>
              <span className="mono text-primary">{p.port}</span>
            </TableCell>
            <TableCell>
              <Badge variant="outline">{p.protocol}</Badge>
            </TableCell>
            <TableCell className="text-muted-foreground">IPv{p.ipv6 ? '6' : '4'}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ) : (
    <div className="py-10 text-center text-sm text-muted-foreground">پورت شنونده‌ای شناسایی نشد.</div>
  )
}

export function NetworkPage({ Heading, notify }: Common) {
  const { data, history, loading, refresh, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, historyEnabled } = useDashboard(notify)
  const [showInactiveInterfaces, setShowInactiveInterfaces] = useState(false)
  const interfaces = data?.interfaces || []
  const activeInterfaces = interfaces.filter((item) => item.state.toLowerCase() === 'up' && item.carrier !== false)
  const visibleInterfaces = showInactiveInterfaces ? interfaces : activeInterfaces
  return (
    <>
      <Heading
        title="شبکه"
        description="رابط‌های شبکه، آدرس‌ها و جریان ترافیک را بررسی کن."
        action={
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw size={15} /> به‌روزرسانی
          </Button>
        }
      />
      {loading && !data ? (
        <LoadingCards />
      ) : (
        <>
          <div className="mb-5">
            <SectionCard title="نرخ ترافیک شبکه" description="نمونه‌های ذخیره‌شده توسط سرویس" right={<TimeRangeFilter range={range} setRange={setRange} from={customFrom} setFrom={setCustomFrom} to={customTo} setTo={setCustomTo} applyCustom={applyCustomRange}/>}>{!historyEnabled && <HistoryPaused/>}<TrafficChart history={history}/></SectionCard>
          </div>
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-card/70 px-4 py-3">
            <div><h2 className="text-sm font-semibold text-foreground">کارت‌های شبکه</h2><p className="mt-1 text-xs text-muted-foreground">{formatNumber(activeInterfaces.length)} فعال از {formatNumber(interfaces.length)} رابط</p></div>
            <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"><Switch checked={!showInactiveInterfaces} onCheckedChange={(checked) => setShowInactiveInterfaces(!checked)} aria-label="فقط نمایش کارت‌های فعال"/><span>فقط کارت‌های فعال</span></label>
          </div>
          <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
            {visibleInterfaces.map((item) => {
              const active = item.state.toLowerCase() === 'up' && item.carrier !== false
              const errors = (item.rx_errors || 0) + (item.tx_errors || 0)
              const dropped = (item.rx_dropped || 0) + (item.tx_dropped || 0)
              return <Card key={item.name} className="metric-card overflow-hidden">
                <CardHeader className="flex-row items-center justify-between gap-2 space-y-0 p-4 pb-2">
                  <div className="min-w-0"><CardTitle dir="ltr" className="truncate text-base">{item.name}</CardTitle><CardDescription dir="ltr" className="mt-1 truncate font-mono text-[10px]">{item.mac || 'MAC نامشخص'}</CardDescription></div>
                  <Badge variant={active ? 'success' : 'outline'}>{active ? 'فعال' : item.state || 'نامشخص'}</Badge>
                </CardHeader>
                <CardContent className="space-y-2 p-4 pt-2 text-xs">
                  <div className="flex items-center justify-between gap-2 text-muted-foreground"><span className="flex items-center gap-1.5"><ArrowDownLeft size={14} className="text-primary"/>دریافت کل</span><strong dir="ltr" className="font-mono text-foreground">{formatBytes(item.rx_bytes)}</strong></div>
                  <div className="flex items-center justify-between gap-2 text-muted-foreground"><span className="flex items-center gap-1.5"><ArrowUpRight size={14} className="text-emerald-300"/>ارسال کل</span><strong dir="ltr" className="font-mono text-foreground">{formatBytes(item.tx_bytes)}</strong></div>
                  <details className="group border-t border-border pt-2">
                    <summary className="flex cursor-pointer list-none items-center justify-between gap-2 rounded-md py-1 text-primary outline-none focus-visible:ring-2 focus-visible:ring-ring"><span>جزئیات بیشتر</span><ChevronDown size={14} className="transition group-open:rotate-180"/></summary>
                    <div className="mt-2 space-y-2 rounded-lg bg-background/65 p-3">
                      <div><span className="text-muted-foreground">آدرس‌های IP</span><p dir="ltr" className="mt-1 break-all text-left font-mono text-[10px] text-foreground">{item.addresses?.join(' · ') || 'ندارد'}</p></div>
                      <div className="grid grid-cols-2 gap-2"><span className="text-muted-foreground">MTU <b dir="ltr" className="mr-1 font-mono text-foreground">{item.mtu ?? '—'}</b></span><span className="text-muted-foreground">سرعت <b dir="ltr" className="mr-1 font-mono text-foreground">{item.speed_mbps ? `${formatNumber(item.speed_mbps)} Mbps` : '—'}</b></span><span className="text-muted-foreground">RX packets <b dir="ltr" className="mr-1 font-mono text-foreground">{formatNumber(item.rx_packets || 0)}</b></span><span className="text-muted-foreground">TX packets <b dir="ltr" className="mr-1 font-mono text-foreground">{formatNumber(item.tx_packets || 0)}</b></span><span className="text-muted-foreground">خطا <b dir="ltr" className="mr-1 font-mono text-foreground">{formatNumber(errors)}</b></span><span className="text-muted-foreground">حذف‌شده <b dir="ltr" className="mr-1 font-mono text-foreground">{formatNumber(dropped)}</b></span></div>
                      <div className="text-muted-foreground">Carrier: <span dir="ltr" className="font-mono text-foreground">{item.carrier === undefined ? '—' : item.carrier ? 'connected' : 'disconnected'}</span></div>
                    </div>
                  </details>
                </CardContent>
              </Card>
            })}
            {!visibleInterfaces.length && (
              <Card>
                <CardContent className="p-8 text-center text-sm text-muted-foreground">
                  {interfaces.length ? 'کارت فعالی پیدا نشد؛ فیلتر کارت‌های فعال را خاموش کن تا همهٔ رابط‌ها را ببینی.' : 'کارت شبکه‌ای یافت نشد.'}
                </CardContent>
              </Card>
            )}
          </div>
          <div className="mt-5">
            <DistributionBars title="توزیع ترافیک کارت‌ها" description="جمع بایت‌های دریافت و ارسال هر رابط از زمان شروع آن" items={(data?.interfaces || []).map((item) => ({ label: item.name, value: item.rx_bytes + item.tx_bytes, display: formatBytes(item.rx_bytes + item.tx_bytes) }))} />
          </div>
          <div className="mt-5"><SectionCard title="جزئیات رابط‌ها" description="شمارنده‌های فعلی هر کارت از کرنل خوانده می‌شوند"><div className="overflow-x-auto"><Table><TableHeader><TableRow><TableHead>رابط</TableHead><TableHead>وضعیت</TableHead><TableHead>MTU</TableHead><TableHead>سرعت لینک</TableHead><TableHead>بستهٔ دریافتی</TableHead><TableHead>بستهٔ ارسالی</TableHead><TableHead>خطا / حذف</TableHead></TableRow></TableHeader><TableBody>{(data?.interfaces || []).map((item) => <TableRow key={item.name}><TableCell className="font-mono text-white" dir="ltr">{item.name}</TableCell><TableCell>{item.state || '—'}</TableCell><TableCell className="font-mono" dir="ltr">{item.mtu ?? '—'}</TableCell><TableCell className="font-mono" dir="ltr">{item.speed_mbps ? `${formatNumber(item.speed_mbps)} Mbps` : '—'}</TableCell><TableCell className="font-mono" dir="ltr">{formatNumber(item.rx_packets || 0)}</TableCell><TableCell className="font-mono" dir="ltr">{formatNumber(item.tx_packets || 0)}</TableCell><TableCell className="font-mono" dir="ltr">{formatNumber((item.rx_errors || 0) + (item.tx_errors || 0) + (item.rx_dropped || 0) + (item.tx_dropped || 0))}</TableCell></TableRow>)}</TableBody></Table></div></SectionCard></div>
        </>
      )}
    </>
  )
}

export function FirewallPage({ Heading, notify, user }: Common & { user: User }) {
  const [data, setData] = useState<Firewall | null>(null)
  const [rules, setRules] = useState<ManagedFirewallRule[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<number | null>(null)
  const [form, setForm] = useState<{ port: string; protocol: 'tcp' | 'udp' | 'both'; policy: 'allow' | 'deny'; enabled: boolean }>({ port: '', protocol: 'tcp', policy: 'allow', enabled: true })
  const [query, setQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<'all' | 'enabled' | 'disabled'>('all')
  const refresh = useCallback(async () => {
    try { const [firewall, managed] = await Promise.all([api<Firewall>('firewall'), api<{ rules: ManagedFirewallRule[] }>('firewall/rules')]); setData(firewall); setRules(managed.rules) }
    catch (error) { notify(error instanceof Error ? error.message : 'دریافت قوانین ناموفق بود') }
    finally { setLoading(false) }
  }, [notify])
  useEffect(() => { void refresh() }, [refresh])
  const active = data?.available && /^Status: active/m.test(data.status)
  const filtered = rules.filter((rule) => (statusFilter === 'all' || (statusFilter === 'enabled') === rule.enabled) && (!query.trim() || String(rule.port).includes(query.trim())))
  function openEditor(rule?: ManagedFirewallRule) {
    setEditing(rule?.id ?? null)
    setForm(rule ? { port: String(rule.port), protocol: rule.protocol, policy: rule.policy, enabled: rule.enabled } : { port: '', protocol: 'both', policy: 'allow', enabled: true })
    setDialogOpen(true)
  }
  async function save(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    const created: number[] = []
    try {
      const path = editing === null ? 'firewall/rules' : `firewall/rules/${editing}`
      const protocols = editing === null && form.protocol === 'both' ? ['tcp', 'udp'] : [form.protocol]
      for (const protocol of protocols) {
        const result = await api<{ rule?: ManagedFirewallRule }>(path, { method: editing === null ? 'POST' : 'PUT', body: JSON.stringify({ ...form, protocol, port: Number(form.port) }) })
        if (editing === null && result.rule) created.push(result.rule.id)
      }
      setDialogOpen(false)
      notify(protocols.length === 2 ? 'قوانین TCP و UDP ساخته شدند.' : editing === null ? 'قانون فایروال ساخته شد.' : 'قانون فایروال ویرایش شد.')
      await refresh()
    } catch (error) {
      await Promise.all(created.map((id) => api(`firewall/rules/${id}`, { method: 'DELETE' }).catch(() => undefined)))
      notify(error instanceof Error ? error.message : 'ذخیرهٔ قانون ناموفق بود')
      await refresh()
    }
    finally { setBusy(false) }
  }
  async function toggleFirewall() {
    setBusy(true)
    try { await api('firewall', { method: 'PUT', body: JSON.stringify({ enabled: !active }) }); notify(active ? 'فایروال UFW غیرفعال شد.' : 'فایروال UFW فعال شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'تغییر وضعیت UFW ناموفق بود') }
    finally { setBusy(false) }
  }
  async function installFirewall() {
    setBusy(true)
    try { await api('firewall/install', { method: 'POST' }); notify('UFW نصب شد. اکنون می‌توانی آن را فعال کنی.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'نصب UFW ناموفق بود') }
    finally { setBusy(false) }
  }
  async function toggle(rule: ManagedFirewallRule) {
    setBusy(true)
    try { await api(`firewall/rules/${rule.id}`, { method: 'PUT', body: JSON.stringify({ port: rule.port, protocol: rule.protocol, policy: rule.policy, enabled: !rule.enabled }) }); notify(rule.enabled ? 'قانون غیرفعال شد.' : 'قانون فعال شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'تغییر وضعیت ناموفق بود') }
    finally { setBusy(false) }
  }
  async function remove(rule: ManagedFirewallRule) {
    if (!window.confirm(`قانون ${rule.port}/${rule.protocol} حذف شود؟`)) return
    setBusy(true)
    try { await api(`firewall/rules/${rule.id}`, { method: 'DELETE' }); notify('قانون حذف شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف قانون ناموفق بود') }
    finally { setBusy(false) }
  }
  return <>
    <Heading title="فایروال UFW" description="قوانین پنل را بساز، ویرایش کن یا موقتاً غیرفعال کن." action={<div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button>{user.role === 'admin' && <Button size="sm" onClick={() => openEditor()}><Plus size={15}/> قانون جدید</Button>}</div>}/>
    <div className="mb-5 grid gap-3 sm:grid-cols-3"><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">وضعیت UFW</div><div className="mt-3 flex items-center gap-2 text-xl font-bold text-white"><span className={`h-2 w-2 rounded-full ${active ? 'bg-emerald-400' : 'bg-amber-400'}`}/>{loading ? 'در حال بررسی' : data?.available ? active ? 'فعال' : 'غیرفعال' : 'نصب نشده'}</div>{user.role === 'admin' && data?.available && <div className="mt-4 flex items-center gap-2 border-t border-white/10 pt-3 text-xs text-muted-foreground"><Switch checked={!!active} disabled={busy} onCheckedChange={() => void toggleFirewall()} aria-label="تغییر وضعیت UFW"/> تغییر وضعیت کلی UFW</div>}{user.role === 'admin' && !data?.available && <div className="mt-4 border-t border-white/10 pt-3"><Button size="sm" disabled={busy} onClick={() => void installFirewall()}>{busy ? 'در حال نصب…' : 'نصب UFW'}</Button><p className="mt-2 text-[11px] leading-5 text-amber-200/80">{data?.error || 'UFW روی این سرور نصب نشده است.'}</p></div>}</div><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">قوانین فعال پنل</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => rule.enabled).length)}</div></div><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">قوانین غیرفعال</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => !rule.enabled).length)}</div></div></div>
    <SectionCard title="قوانین مدیریت‌شده" description="تغییر وضعیت و ویرایش فقط روی قوانین ساخته‌شده توسط پنل اعمال می‌شود" right={<Badge variant="outline">{formatNumber(filtered.length)} قانون</Badge>}>
      <div className="mb-5 flex flex-wrap items-center gap-3"><label className="relative min-w-[180px] flex-1"><span className="sr-only">جست‌وجوی پورت</span><Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input lang="en" inputMode="numeric" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="جست‌وجوی شمارهٔ پورت" className="pr-9"/></label><div className="flex gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['all','همه'], ['enabled','فعال'], ['disabled','غیرفعال']] as const).map(([value, label]) => <button key={value} onClick={() => setStatusFilter(value)} aria-pressed={statusFilter === value} className={`rounded-lg px-3 py-2 text-xs transition ${statusFilter === value ? 'bg-primary/20 text-white' : 'text-muted-foreground hover:text-white'}`}>{label}</button>)}</div></div>
      {loading ? <Skeleton className="h-48"/> : filtered.length ? <div className="overflow-x-auto"><Table><TableHeader><TableRow><TableHead>پورت</TableHead><TableHead>پروتکل</TableHead><TableHead>رفتار</TableHead><TableHead>وضعیت</TableHead>{user.role === 'admin' && <TableHead>مدیریت</TableHead>}</TableRow></TableHeader><TableBody>{filtered.map((rule) => <TableRow key={rule.id}><TableCell className="font-mono text-white" dir="ltr">{rule.port}</TableCell><TableCell className="font-mono uppercase">{rule.protocol}</TableCell><TableCell><Badge variant={rule.policy === 'allow' ? 'success' : 'warning'}>{rule.policy === 'allow' ? 'اجازه' : 'مسدود'}</Badge></TableCell><TableCell>{user.role === 'admin' ? <div className="flex items-center gap-2"><Switch checked={rule.enabled} disabled={busy} onCheckedChange={() => void toggle(rule)} aria-label={`${rule.enabled ? 'غیرفعال‌کردن' : 'فعال‌کردن'} قانون ${rule.port}`}/><span className="text-xs text-muted-foreground">{rule.enabled ? 'فعال' : 'غیرفعال'}</span></div> : <Badge variant={rule.enabled ? 'success' : 'outline'}>{rule.enabled ? 'فعال' : 'غیرفعال'}</Badge>}</TableCell>{user.role === 'admin' && <TableCell><div className="flex gap-1"><Button variant="ghost" size="icon" disabled={busy} onClick={() => openEditor(rule)} aria-label={`ویرایش قانون ${rule.port}`}><Pencil size={15}/></Button><Button variant="ghost" size="icon" disabled={busy} onClick={() => void remove(rule)} aria-label={`حذف قانون ${rule.port}`}><Trash2 size={15}/></Button></div></TableCell>}</TableRow>)}</TableBody></Table></div> : <div className="rounded-xl border border-dashed border-white/10 py-12 text-center text-sm text-muted-foreground">{query || statusFilter !== 'all' ? 'قانونی مطابق فیلتر پیدا نشد.' : 'هنوز قانونی در پنل ساخته نشده است.'}</div>}
    </SectionCard>
    <details className="mt-5 rounded-[14px] border border-white/10 bg-card p-5"><summary className="cursor-pointer text-sm font-semibold text-white">همهٔ قوانین UFW و خروجی خام</summary><div className="mt-4 flex justify-end"><Button variant="outline" size="sm" disabled={!data?.status} onClick={() => downloadText('arg-ufw.txt', data?.status || '')}><Download size={14}/> دریافت خروجی</Button></div><pre className="mt-3 max-h-[480px] overflow-auto whitespace-pre-wrap rounded-xl bg-background p-4 text-left font-mono text-xs leading-6 text-muted-foreground" dir="ltr">{data?.status || data?.error || 'UFW unavailable'}</pre></details>
    <Alert className="mt-5"><ShieldCheck size={17} className="mb-2 text-primary"/><AlertTitle>حفاظت دسترسی</AlertTitle><AlertDescription>پورت‌های SSH، HTTP، HTTPS و پورت‌های پنل از ویرایش در این بخش محافظت می‌شوند. هنگام فعال‌کردن UFW، دسترسی SSH و پنل ابتدا مجاز می‌شود.</AlertDescription></Alert>
    <Dialog open={dialogOpen} onOpenChange={setDialogOpen}><DialogContent><DialogHeader><DialogTitle>{editing === null ? 'قانون جدید' : 'ویرایش قانون'}</DialogTitle><DialogDescription>قانون در UFW اعمال می‌شود و می‌توانی بعداً آن را غیرفعال کنی.</DialogDescription></DialogHeader><form onSubmit={save} className="space-y-4"><div className="grid grid-cols-2 gap-3"><div className="space-y-2"><Label htmlFor="firewall-port">پورت</Label><Input id="firewall-port" type="number" lang="en" dir="ltr" min={1} max={65535} required value={form.port} onChange={(event) => setForm((old) => ({ ...old, port: event.target.value }))}/></div><div className="space-y-2"><Label>پروتکل</Label><div role="group" aria-label="انتخاب پروتکل شبکه" className="grid grid-cols-3 gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['tcp','TCP'], ['udp','UDP'], ['both','TCP + UDP']] as const).map(([value,label]) => <button key={value} type="button" aria-pressed={form.protocol === value} onClick={() => setForm((old) => ({ ...old, protocol: value }))} className={`rounded-lg px-2 py-2.5 text-xs font-mono font-semibold transition ${form.protocol === value ? 'bg-primary/25 text-white shadow-sm' : 'text-muted-foreground hover:bg-white/[.04] hover:text-white'}`}>{label}</button>)}</div><p className="text-[11px] text-muted-foreground">{form.protocol === 'both' ? 'دو قانون جداگانه برای TCP و UDP ساخته می‌شود.' : `فقط ترافیک ${form.protocol.toUpperCase()} برای این پورت مدیریت می‌شود.`}</p></div></div><div className="space-y-2"><Label htmlFor="firewall-policy">رفتار</Label><Select value={form.policy} onValueChange={(value) => setForm((old) => ({ ...old, policy: value as 'allow' | 'deny' }))}><SelectTrigger id="firewall-policy"><SelectValue/></SelectTrigger><SelectContent><SelectItem value="allow">اجازه</SelectItem><SelectItem value="deny">مسدود</SelectItem></SelectContent></Select></div><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span>قانون فعال باشد</span><Switch checked={form.enabled} onCheckedChange={(checked) => setForm((old) => ({ ...old, enabled: checked }))}/></label><Button type="submit" disabled={busy || !form.port} className="w-full">{busy ? 'در حال ذخیره…' : 'ذخیرهٔ قانون'}</Button></form></DialogContent></Dialog>
  </>
}

type NginxSite = { name: string; enabled: boolean; bytes?: number; content?: string; message?: string; description?: string }
type NginxStatus = { installed: boolean; active: boolean; enabled: boolean; version: string; config_test: { ok: boolean; output: string } }
type NginxPreset = 'static' | 'proxy' | 'php' | 'redirect'
type NginxBuilder = { preset: NginxPreset; domain: string; listen: string; root: string; index: string; proxyPass: string; phpSocket: string; redirectTo: string; ssl: boolean; certificate: string; certificateKey: string; gzip: boolean; maxBody: string }
type NginxFileEntry = { name: string; path: string; directory: boolean; selectable: boolean }
type NginxFileListing = { path: string; roots: string[]; entries: NginxFileEntry[] }
type NginxDomainResult = { host: string; resolved: boolean; addresses: string[]; error?: string }
export function NginxPage({ Heading, notify }: Common) {
  const [sites, setSites] = useState<NginxSite[]>([])
  const [status, setStatus] = useState<NginxStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [loadError, setLoadError] = useState('')
  const [editing, setEditing] = useState<string | null>(null)
  const [content, setContent] = useState('')
  const [editorOpen, setEditorOpen] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  const [newName, setNewName] = useState('')
  const [newContent, setNewContent] = useState('server {\n    listen 80;\n    listen [::]:80;\n    server_name example.com;\n\n    root /var/www/example.com;\n    index index.html;\n\n    location / {\n        try_files $uri $uri/ =404;\n    }\n}\n')
  const [logsOpen, setLogsOpen] = useState(false)
  const [logs, setLogs] = useState('')
  const [siteMetaOpen, setSiteMetaOpen] = useState(false)
  const [managedSite, setManagedSite] = useState<NginxSite | null>(null)
  const [siteNewName, setSiteNewName] = useState('')
  const [siteDescription, setSiteDescription] = useState('')
  const [siteToolsOpen, setSiteToolsOpen] = useState(false)
  const [toolSite, setToolSite] = useState<NginxSite | null>(null)
  const [toolConfig, setToolConfig] = useState('')
  const [domainPrimary, setDomainPrimary] = useState('')
  const [domainAliases, setDomainAliases] = useState('')
  const [domainResults, setDomainResults] = useState<NginxDomainResult[]>([])
  const [domainChecking, setDomainChecking] = useState(false)
  const [siteLog, setSiteLog] = useState('')
  const [siteLogPath, setSiteLogPath] = useState('')
  const [siteLogKind, setSiteLogKind] = useState<'access' | 'error' | null>(null)
  const [siteLogLoading, setSiteLogLoading] = useState(false)
  const [duplicateNotice, setDuplicateNotice] = useState(false)
  const [builderOpen, setBuilderOpen] = useState(false)
  const [builderTarget, setBuilderTarget] = useState<'create' | 'edit'>('create')
  const [builder, setBuilder] = useState<NginxBuilder>({ preset: 'static', domain: 'example.com', listen: '80', root: '/var/www/example.com', index: 'index.html', proxyPass: 'http://127.0.0.1:3000', phpSocket: 'unix:/run/php/php8.2-fpm.sock', redirectTo: 'https://example.com$request_uri', ssl: false, certificate: '', certificateKey: '', gzip: true, maxBody: '32m' })
  const [extraDirectives, setExtraDirectives] = useState<Array<{ name: string; value: string }>>([])
  const [extraLocations, setExtraLocations] = useState<Array<{ path: string; config: string }>>([])
  const [filePickerOpen, setFilePickerOpen] = useState(false)
  const [filePickerFor, setFilePickerFor] = useState<'certificate' | 'certificateKey'>('certificate')
  const [fileListing, setFileListing] = useState<NginxFileListing | null>(null)
  const [fileLoading, setFileLoading] = useState(false)

  const refresh = useCallback(async () => {
    const [serviceResult, sitesResult] = await Promise.allSettled([
      api<NginxStatus>('nginx/status'),
      api<{ sites: NginxSite[] }>('nginx/sites'),
    ])
    if (serviceResult.status === 'fulfilled') setStatus(serviceResult.value)
    if (sitesResult.status === 'fulfilled') {
      setSites(sitesResult.value.sites)
      setLoadError('')
    } else {
      setSites([])
      setLoadError(sitesResult.reason instanceof Error ? sitesResult.reason.message : 'خواندن سایت‌های Nginx ناموفق بود.')
    }
    if (serviceResult.status === 'rejected' && sitesResult.status === 'rejected') {
      setLoadError(serviceResult.reason instanceof Error ? serviceResult.reason.message : 'وضعیت Nginx در دسترس نیست.')
    }
    setLoading(false)
  }, [])
  useEffect(() => { void refresh() }, [refresh])

  async function runAction(action: string) {
    setBusy(true)
    try {
      const result = await api<{ message?: string; logs?: string; output?: string }>('nginx/action', { method: 'POST', body: JSON.stringify({ action }) })
      if (action === 'logs') { setLogs(result.logs || ''); setLogsOpen(true) }
      else notify(result.message || 'عملیات Nginx انجام شد.')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'عملیات Nginx ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function createSite(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    try {
      const result = await api<{ message?: string }>('nginx/sites', { method: 'POST', body: JSON.stringify({ name: newName.trim(), content: newContent }) })
      setCreateOpen(false); setNewName(''); setDuplicateNotice(false); notify(result.message || 'فایل سایت ساخته شد.'); await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ساخت سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  function openBuilder(target: 'create' | 'edit') {
    setBuilderTarget(target)
    if (target === 'create') {
      const initialName = newName.trim() || 'example.com'
      setBuilder((old) => ({ ...old, domain: initialName, root: `/var/www/${initialName.split(/\s+/)[0]}` }))
      setDuplicateNotice(false)
    } else {
      const directive = (key: string) => content.match(new RegExp(`^\\s*${key}\\s+([^;]+);`, 'm'))?.[1]?.trim()
      const domain = directive('server_name') || editing || 'example.com'
      const listens = [...content.matchAll(/^\s*listen\s+([^;]+);/gm)].map((match) => match[1].trim())
      const proxy = directive('proxy_pass')
      const php = directive('fastcgi_pass')
      const redirect = content.match(/^\s*return\s+301\s+([^;]+);/m)?.[1]?.trim()
      setBuilder((old) => ({ ...old, preset: redirect ? 'redirect' : php ? 'php' : proxy ? 'proxy' : 'static', domain, listen: listens.find((value) => !value.startsWith('443'))?.split(/\s+/)[0] || '80', root: directive('root') || old.root, index: directive('index') || old.index, proxyPass: proxy || old.proxyPass, phpSocket: php || old.phpSocket, redirectTo: redirect || old.redirectTo, ssl: /listen\s+443\s+ssl/.test(content), certificate: directive('ssl_certificate') || '', certificateKey: directive('ssl_certificate_key') || '', gzip: /gzip\s+on\s*;/.test(content), maxBody: directive('client_max_body_size') || '' }))
    }
    setExtraDirectives([]); setExtraLocations([])
    setFilePickerOpen(false)
    setBuilderOpen(true)
  }

  function generatedConfig() {
    const serverName = builder.domain.trim() || '_'
    const custom = extraDirectives.filter((item) => /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(item.name.trim()) && item.value.trim() && !/[;{}\r\n]/.test(item.value)).map((item) => `    ${item.name.trim()} ${item.value.trim()};`).join('\n')
    const locations = extraLocations.filter((item) => item.path.trim() && item.config.trim()).map((item) => `    location ${item.path.trim()} {\n${item.config.split('\n').map((line) => `        ${line}`).join('\n')}\n    }`).join('\n\n')
    const common = [builder.gzip ? '    gzip on;' : '', builder.maxBody.trim() ? `    client_max_body_size ${builder.maxBody.trim()};` : '', custom].filter(Boolean).join('\n')
    if (builder.preset === 'redirect') {
      const destination = builder.redirectTo.trim() || 'https://$host$request_uri'
      if (builder.ssl) return `server {\n    listen ${builder.listen.trim() || '80'};\n    server_name ${serverName};\n    return 301 https://$host$request_uri;\n}\n\nserver {\n    listen 443 ssl;\n    server_name ${serverName};\n    ssl_certificate ${builder.certificate.trim()};\n    ssl_certificate_key ${builder.certificateKey.trim()};\n    ssl_protocols TLSv1.2 TLSv1.3;\n    return 301 ${destination};\n}\n`
      return `server {\n    listen ${builder.listen.trim() || '80'};\n    server_name ${serverName};\n    return 301 ${destination};\n}\n`
    }
    let handler = ''
    if (builder.preset === 'proxy') handler = `    location / {\n        proxy_pass ${builder.proxyPass.trim()};\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n    }`
    else if (builder.preset === 'php') handler = `    location / {\n        try_files $uri $uri/ /index.php?$query_string;\n    }\n\n    location ~ \\.php$ {\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass ${builder.phpSocket.trim()};\n    }\n\n    location ~ /\\.ht {\n        deny all;\n    }`
    else handler = `    location / {\n        try_files $uri $uri/ =404;\n    }`
    const tls = builder.ssl ? `    listen 443 ssl;\n    ssl_certificate ${builder.certificate.trim()};\n    ssl_certificate_key ${builder.certificateKey.trim()};\n    ssl_protocols TLSv1.2 TLSv1.3;\n` : ''
    const root = builder.preset === 'proxy' ? '' : `    root ${builder.root.trim()};\n    index ${builder.index.trim() || 'index.html'};\n`
    return `server {\n    listen ${builder.listen.trim() || '80'};\n${tls}    server_name ${serverName};\n${root}${common ? `${common}\n` : ''}\n${handler}${locations ? `\n\n${locations}` : ''}\n}\n`
  }

  async function browseFiles(path?: string) {
    setFileLoading(true)
    try {
      const query = path ? `?path=${encodeURIComponent(path)}` : ''
      setFileListing(await api<NginxFileListing>(`nginx/files${query}`))
    } catch (error) { notify(error instanceof Error ? error.message : 'خواندن فایل‌ها ناموفق بود.') }
    finally { setFileLoading(false) }
  }

  function openFilePicker(target: 'certificate' | 'certificateKey') {
    setFilePickerFor(target); setFileListing(null); setFilePickerOpen(true); void browseFiles()
  }

  function applyBuilder() {
    const config = generatedConfig()
    if (builderTarget === 'edit') setContent(config)
    else {
      setNewContent(config)
      setNewName((builder.domain.trim().split(/\s+/)[0] || 'site').replace(/[^a-zA-Z0-9._-]/g, ''))
    }
    setBuilderOpen(false); setFilePickerOpen(false)
    if (builderTarget === 'edit') setEditorOpen(true)
    else setCreateOpen(true)
  }

  async function deleteSite(site: NginxSite) {
    if (!window.confirm(`فایل تنظیمات ${site.name} و symlink آن حذف شود؟`)) return
    setBusy(true)
    try { const result = await api<{ message?: string }>(`nginx/sites/${encodeURIComponent(site.name)}`, { method: 'DELETE' }); notify(result.message || 'سایت حذف شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  function editSiteMetadata(site: NginxSite) {
    setManagedSite(site)
    setSiteNewName(site.name)
    setSiteDescription(site.description || '')
    setSiteMetaOpen(true)
  }

  async function saveSiteMetadata(event: FormEvent) {
    event.preventDefault()
    if (!managedSite) return
    setBusy(true)
    try {
      let finalName = managedSite.name
      if (siteNewName.trim() !== managedSite.name) {
        const renamed = await api<{ name: string; message?: string }>(`nginx/sites/${encodeURIComponent(managedSite.name)}/rename`, { method: 'POST', body: JSON.stringify({ new_name: siteNewName.trim() }) })
        finalName = renamed.name
        setManagedSite((old) => old ? { ...old, name: finalName } : old)
      }
      const saved = await api<{ message?: string }>(`nginx/sites/${encodeURIComponent(finalName)}/metadata`, { method: 'PUT', body: JSON.stringify({ description: siteDescription }) })
      setSiteMetaOpen(false)
      notify(saved.message || 'مشخصات سایت ذخیره شد.')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ مشخصات سایت ناموفق بود.'); await refresh() }
    finally { setBusy(false) }
  }

  async function editSite(site: NginxSite) {
    setBusy(true)
    try {
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(site.name)}`)
      setEditing(site.name)
      setContent(result.site.content || '')
      setEditorOpen(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'خواندن تنظیمات سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function openSiteTools(site: NginxSite) {
    setBusy(true)
    try {
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(site.name)}`)
      const config = result.site.content || ''
      const names = [...config.matchAll(/^\s*server_name\s+([^;]+);/gm)].flatMap((match) => match[1].trim().split(/\s+/)).filter((name) => name && name !== '_')
      setToolSite(site); setToolConfig(config); setDomainPrimary(names[0] || ''); setDomainAliases(names.slice(1).join(', ')); setDomainResults([]); setSiteLog(''); setSiteLogKind(null); setSiteToolsOpen(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'خواندن مشخصات سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  function domainList() {
    return [domainPrimary, ...domainAliases.split(/[\s,]+/)].map((host) => host.trim()).filter(Boolean)
  }

  async function checkSiteDomains() {
    const hosts = domainList()
    if (!hosts.length) { notify('ابتدا دامنهٔ اصلی را وارد کنید.'); return }
    setDomainChecking(true)
    try {
      const result = await api<{ domains: NginxDomainResult[] }>(`nginx/domains/check?hosts=${encodeURIComponent(hosts.join(','))}`)
      setDomainResults(result.domains)
    } catch (error) { notify(error instanceof Error ? error.message : 'بررسی DNS ناموفق بود.') }
    finally { setDomainChecking(false) }
  }

  async function saveSiteDomains() {
    if (!toolSite) return
    const hosts = domainList()
    const valid = (host: string) => {
      const value = host.startsWith('*.') ? host.slice(2) : host
      return value.length <= 253 && value.split('.').every((label) => label.length > 0 && label.length <= 63 && /^[a-zA-Z0-9](?:[a-zA-Z0-9-]*[a-zA-Z0-9])?$/.test(label))
    }
    if (!domainPrimary.trim() || hosts.some((host) => !valid(host)) || hosts.length > 20 || new Set(hosts).size !== hosts.length) {
      notify('دامنهٔ اصلی لازم است؛ دامنه‌ها باید معتبر و یکتا باشند (حداکثر 20 مورد).')
      return
    }
    setBusy(true)
    try {
      const replacement = `server_name ${hosts.join(' ')};`
      if (!/^\s*server_name\s+[^;]+;/m.test(toolConfig)) throw new Error('در کانفیگ این سایت server_name وجود ندارد؛ آن را از ویرایشگر کانفیگ اضافه کنید.')
      const updated = toolConfig.replace(/^([ \t]*)server_name\s+[^;]+;/gm, (_line, indent: string) => `${indent}${replacement}`)
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(toolSite.name)}`, { method: 'PUT', body: JSON.stringify({ content: updated }) })
      setToolConfig(result.site.content || updated)
      setDomainPrimary(hosts[0]); setDomainAliases(hosts.slice(1).join(', ')); setDomainResults([])
      notify(result.site.message || 'دامنه‌های سایت ذخیره و کانفیگ بررسی شد.')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ دامنه‌ها ناموفق بود.') }
    finally { setBusy(false) }
  }

  function duplicateSite() {
    if (!toolSite) return
    const base = `${toolSite.name}-copy`.slice(0, 128)
    let candidate = base
    let suffix = 2
    while (sites.some((site) => site.name === candidate)) {
      const tail = `-${suffix++}`
      candidate = `${base.slice(0, 128 - tail.length)}${tail}`
    }
    setNewName(candidate); setNewContent(toolConfig); setDuplicateNotice(true); setSiteToolsOpen(false); setCreateOpen(true)
  }

  async function loadSiteLog(kind: 'access' | 'error') {
    if (!toolSite) return
    setSiteLogLoading(true); setSiteLogKind(kind); setSiteLog(''); setSiteLogPath('')
    try {
      const result = await api<{ logs: string; path: string }>(`nginx/sites/${encodeURIComponent(toolSite.name)}/logs`, { method: 'POST', body: JSON.stringify({ kind }) })
      setSiteLog(result.logs); setSiteLogPath(result.path)
    } catch (error) { setSiteLog(error instanceof Error ? error.message : 'خواندن لاگ ناموفق بود.') }
    finally { setSiteLogLoading(false) }
  }

  async function saveSite(event: FormEvent) {
    event.preventDefault()
    if (!editing) return
    setBusy(true)
    try {
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(editing)}`, { method: 'PUT', body: JSON.stringify({ content }) })
      setEditorOpen(false)
      notify(result.site.message || 'تنظیمات ذخیره و بررسی شد.')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیره تنظیمات ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function toggleSite(site: NginxSite) {
    setBusy(true)
    try {
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(site.name)}/enabled`, { method: 'PUT', body: JSON.stringify({ enabled: !site.enabled }) })
      notify(result.site.message || (site.enabled ? 'سایت خاموش شد.' : 'سایت روشن شد.'))
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'تغییر وضعیت سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  const enabledCount = sites.filter((site) => site.enabled).length
  return <>
    <Heading title="مدیریت کامل Nginx" description="نصب و کنترل سرویس، اعتبارسنجی پیکربندی، لاگ‌ها و مدیریت سایت‌ها را از همین صفحه انجام بده." action={<div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" disabled={busy || loading} onClick={() => { setLoading(true); void refresh() }}><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/> تازه‌سازی</Button>{status?.installed && <Button size="sm" disabled={busy} onClick={() => openBuilder('create')}><Sparkles size={15}/> سایت جدید</Button>}</div>}/>
    <SectionCard title="سرویس Nginx" description="وضعیت سرویس، اجرای دستورهای مدیریتی و اعتبارسنجی پیکربندی" right={status?.installed ? <Badge variant={status.active ? 'success' : 'warning'}>{status.active ? 'فعال' : 'متوقف'}</Badge> : <Badge variant="outline">نصب نشده</Badge>}>
      {status?.installed ? <>
        <div className="grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">وضعیت اجرا</p><p className="mt-2 font-semibold text-white">{status.active ? 'در حال اجرا' : 'متوقف'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">اجرا هنگام بوت</p><p className="mt-2 font-semibold text-white">{status.enabled ? 'فعال' : 'غیرفعال'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">نسخه</p><p dir="ltr" className="mt-2 truncate text-left font-mono text-xs text-white">{status.version || 'نامشخص'}</p></div></div>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-white/[.07] pt-4"><Button size="sm" disabled={busy || status.active} onClick={() => void runAction('start')}><Play size={14}/>شروع</Button><Button size="sm" variant="outline" disabled={busy || !status.active} onClick={() => void runAction('stop')}><Square size={14}/>توقف</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => void runAction('restart')}><RotateCw size={14}/>راه‌اندازی مجدد</Button><Button size="sm" variant="outline" disabled={busy || !status.active} onClick={() => void runAction('reload')}><RefreshCw size={14}/>Reload</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => void runAction('test')}><ShieldCheck size={14}/>بررسی تنظیمات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void runAction(status.enabled ? 'disable' : 'enable')}>{status.enabled ? 'غیرفعال‌کردن اجرا هنگام بوت' : 'فعال‌کردن اجرا هنگام بوت'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void runAction('logs')}><Terminal size={14}/>لاگ سرویس</Button></div>
        <div className={`mt-4 rounded-xl border p-3 text-xs ${status.config_test.ok ? 'border-emerald-400/20 bg-emerald-400/[.04] text-emerald-200' : 'border-amber-400/20 bg-amber-400/[.04] text-amber-100'}`}><div className="font-semibold">{status.config_test.ok ? 'پیکربندی معتبر است' : 'پیکربندی نیاز به بررسی دارد'}</div><pre dir="ltr" className="mt-2 max-h-24 overflow-auto whitespace-pre-wrap text-left font-mono text-[11px] opacity-80">{status.config_test.output}</pre></div>
      </> : <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-dashed border-white/10 p-4"><div><p className="font-medium text-white">Nginx نصب نشده است</p><p className="mt-1 text-xs text-muted-foreground">نصب از مخزن سیستم‌عامل با apt، dnf یا yum انجام می‌شود.</p></div><Button disabled={busy} onClick={() => void runAction('install')}><Download size={15}/>{busy ? 'در حال نصب…' : 'نصب Nginx'}</Button></div>}
    </SectionCard>
    <div className="mb-5 grid gap-3 sm:grid-cols-3">
      <Metric icon={Globe} label="همهٔ سایت‌ها" value={formatNumber(sites.length)} detail="فایل‌های معتبر sites-available"/>
      <Metric icon={Power} label="فعال" value={formatNumber(enabledCount)} detail="symlink فعال در sites-enabled"/>
      <Metric icon={ShieldCheck} label="غیرفعال" value={formatNumber(sites.length - enabledCount)} detail="آمادهٔ فعال‌سازی با یک کلیک"/>
    </div>
    {loadError && <Alert className="mb-5 border-amber-300/20 bg-amber-300/[.06]"><AlertTitle>فهرست سایت‌ها در دسترس نیست</AlertTitle><AlertDescription className="leading-6">{loadError}<span className="mt-1 block">برای مدیریت سایت‌ها، Nginx و پوشه‌های استاندارد <code dir="ltr">sites-available</code> و <code dir="ltr">sites-enabled</code> باید موجود باشند.</span></AlertDescription></Alert>}
    <SectionCard title="سایت‌های موجود" description="فعال‌سازی، symlink استاندارد را می‌سازد؛ خاموش‌کردن فقط symlink همان سایت را حذف می‌کند." right={<Badge variant="outline">{formatNumber(sites.length)} سایت</Badge>}>
      {loading ? <Skeleton className="h-48"/> : sites.length ? <div className="grid gap-3 xl:grid-cols-2">{sites.map((site) => <article key={site.name} className="rounded-[14px] border border-white/10 bg-white/[.025] p-4 transition hover:border-primary/25 hover:bg-white/[.04]">
        <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><Globe size={16} className="shrink-0 text-primary"/><h2 className="truncate font-mono text-sm font-semibold text-white" dir="ltr">{site.name}</h2></div>{site.description && <p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{site.description}</p>}<p className="mt-2 font-mono text-[11px] text-muted-foreground" dir="ltr">/etc/nginx/sites-available/{site.name}</p><p className="mt-1 text-[11px] text-muted-foreground" dir="ltr">{site.enabled ? `symlink → ../sites-available/${site.name}` : 'بدون symlink فعال'}</p></div><Badge variant={site.enabled ? 'success' : 'outline'}>{site.enabled ? 'روشن' : 'خاموش'}</Badge></div>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-white/[.07] pt-3"><Button size="sm" variant={site.enabled ? 'outline' : 'default'} disabled={busy} onClick={() => void toggleSite(site)}><Power size={14}/>{site.enabled ? 'خاموش‌کردن سایت' : 'روشن‌کردن و ساخت symlink'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void editSite(site)}><Pencil size={14}/>ویرایش تنظیمات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => editSiteMetadata(site)}><Info size={14}/>نام و توضیحات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void openSiteTools(site)}><Terminal size={14}/>ابزارهای سایت</Button>{site.name !== 'default' && <Button size="sm" variant="ghost" disabled={busy} onClick={() => void deleteSite(site)}><Trash2 size={14}/>حذف</Button>}{typeof site.bytes === 'number' && <span className="mr-auto self-center font-mono text-[10px] text-muted-foreground" dir="ltr">{formatNumber(site.bytes)} B</span>}</div>
      </article>)}</div> : !loadError ? <div className="rounded-xl border border-dashed border-white/10 py-14 text-center"><Globe size={22} className="mx-auto text-muted-foreground"/><p className="mt-3 text-sm font-medium text-white">سایتی پیدا نشد</p><p className="mt-1 text-xs text-muted-foreground">فایل‌های پیکربندی Nginx در مسیر sites-available نمایش داده می‌شوند.</p></div> : null}
    </SectionCard>
    <Alert className="mt-5"><ShieldCheck size={16} className="mb-2 text-primary"/><AlertTitle>اعمال ایمن تنظیمات</AlertTitle><AlertDescription>قبل از ذخیره یا تغییر symlink، دستور <code dir="ltr">nginx -t</code> اجرا می‌شود. اگر تنظیمات معتبر نباشند، تغییر برگردانده می‌شود؛ در صورت روشن‌بودن سرویس، پس از اعتبارسنجی Nginx reload خواهد شد.</AlertDescription></Alert>
    <Dialog open={createOpen} onOpenChange={setCreateOpen}><DialogContent className="max-h-[92vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>{duplicateNotice ? 'ساخت کپی سایت' : 'ساخت سایت Nginx'}</DialogTitle><DialogDescription>فایل در sites-available ساخته و با nginx -t بررسی می‌شود. سایت در شروع غیرفعال می‌ماند.</DialogDescription></DialogHeader>{duplicateNotice && <Alert><AlertTitle>پیش‌نویس کپی‌شده</AlertTitle><AlertDescription>کانفیگ و server_name از سایت اصلی کپی شده‌اند. پیش از فعال‌کردن این کپی، دامنه‌ها و مسیرهای وابسته را در کانفیگ ویرایش کن.</AlertDescription></Alert>}<form onSubmit={createSite} className="space-y-4"><div className="space-y-2"><Label htmlFor="nginx-site-name">نام فایل</Label><Input id="nginx-site-name" dir="ltr" value={newName} onChange={(event) => setNewName(event.target.value.replace(/[^a-zA-Z0-9._-]/g, ''))} placeholder="example.com" required maxLength={128}/></div><div className="flex justify-end"><Button type="button" variant="outline" size="sm" onClick={() => { setCreateOpen(false); openBuilder('create') }}><Sparkles size={14}/>ویرایش با سازندهٔ بصری</Button></div><div className="space-y-2"><Label htmlFor="nginx-site-content">پیکربندی Nginx</Label><textarea id="nginx-site-content" spellCheck={false} dir="ltr" value={newContent} onChange={(event) => setNewContent(event.target.value)} className="min-h-[45vh] w-full resize-y rounded-xl border border-white/10 bg-background p-4 font-mono text-xs leading-6 text-[#f2f0eb] outline-none focus:border-primary/50"/></div><Button type="submit" disabled={busy || !newName.trim() || !newContent.trim()} className="w-full">{busy ? 'در حال بررسی و ساخت…' : 'اعتبارسنجی و ساخت سایت'}</Button></form></DialogContent></Dialog>
    <Dialog open={logsOpen} onOpenChange={setLogsOpen}><DialogContent className="max-h-[85vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>لاگ سرویس Nginx</DialogTitle><DialogDescription>۱۰۰ رویداد آخر ثبت‌شده در systemd journal</DialogDescription></DialogHeader><pre dir="ltr" className="max-h-[65vh] overflow-auto whitespace-pre-wrap rounded-xl bg-background p-4 text-left font-mono text-[11px] leading-5 text-muted-foreground">{logs || 'لاگی موجود نیست.'}</pre></DialogContent></Dialog>
    <Dialog open={siteMetaOpen} onOpenChange={setSiteMetaOpen}><DialogContent><DialogHeader><DialogTitle>مشخصات سایت</DialogTitle><DialogDescription>نام فایل در sites-available تغییر می‌کند؛ اگر سایت روشن باشد symlink فعال نیز هماهنگ می‌شود.</DialogDescription></DialogHeader><form onSubmit={saveSiteMetadata} className="space-y-4"><div className="space-y-2"><Label htmlFor="nginx-site-rename">نام / مسیر کانفیگ</Label><Input id="nginx-site-rename" dir="ltr" value={siteNewName} onChange={(event) => setSiteNewName(event.target.value.replace(/[^a-zA-Z0-9._-]/g, ''))} maxLength={128} required/><p className="text-[11px] text-muted-foreground" dir="ltr">/etc/nginx/sites-available/{siteNewName || 'site-name'}</p></div><div className="space-y-2"><Label htmlFor="nginx-site-description">توضیحات</Label><textarea id="nginx-site-description" maxLength={500} value={siteDescription} onChange={(event) => setSiteDescription(event.target.value)} placeholder="کاربرد این سایت، مالک سرویس یا نکات عملیاتی…" className="min-h-28 w-full rounded-xl border border-white/10 bg-background p-3 text-sm leading-6 text-white outline-none focus:border-primary/50"/><p className="text-left text-[11px] text-muted-foreground" dir="ltr">{siteDescription.length}/500</p></div><Button type="submit" className="w-full" disabled={busy || !siteNewName.trim()}>{busy ? 'در حال ذخیره…' : 'ذخیرهٔ مشخصات'}</Button></form></DialogContent></Dialog>
    <Dialog open={siteToolsOpen} onOpenChange={setSiteToolsOpen}><DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>ابزارهای سایت <span dir="ltr" className="font-mono text-primary">{toolSite?.name}</span></DialogTitle><DialogDescription>تنظیمات تشخیص‌داده‌شده و عملیات اختصاصی این سایت</DialogDescription></DialogHeader>{toolSite && <div className="space-y-4"><div className="grid gap-3 sm:grid-cols-2"><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">دامنه‌ها</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*server_name\s+([^;]+);/m)?.[1]?.trim() || 'در کانفیگ پیدا نشد'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Port و TLS</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{[...toolConfig.matchAll(/^\s*listen\s+([^;]+);/gm)].map((match) => match[1].trim()).join(' · ') || 'نامشخص'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Document root</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*root\s+([^;]+);/m)?.[1]?.trim() || 'تعریف نشده'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Upstream / PHP-FPM</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{[toolConfig.match(/^\s*proxy_pass\s+([^;]+);/m)?.[1], toolConfig.match(/^\s*fastcgi_pass\s+([^;]+);/m)?.[1]].filter(Boolean).join(' · ') || 'تعریف نشده'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4 sm:col-span-2"><p className="text-xs text-muted-foreground">گواهی SSL</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*ssl_certificate\s+([^;]+);/m)?.[1]?.trim() || 'TLS در کانفیگ شناسایی نشد'}</p></div></div><section className="space-y-3 rounded-xl border border-primary/20 bg-primary/[.04] p-4"><div><h3 className="text-sm font-semibold text-white">مدیریت دامنه‌ها</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">دامنهٔ اصلی و aliasها را تنظیم کن. ذخیره، همهٔ دستورهای server_name این فایل را به‌روزرسانی می‌کند و فقط پس از اعتبارسنجی Nginx اعمال می‌شود.</p></div><div className="grid gap-3 md:grid-cols-2"><div className="space-y-2"><Label htmlFor="nginx-primary-domain">دامنهٔ اصلی</Label><Input id="nginx-primary-domain" dir="ltr" className="text-left" placeholder="example.com" value={domainPrimary} onChange={(event) => setDomainPrimary(event.target.value.trim())}/></div><div className="space-y-2"><Label htmlFor="nginx-domain-aliases">دامنه‌های جایگزین</Label><Input id="nginx-domain-aliases" dir="ltr" className="text-left" placeholder="www.example.com, shop.example.com" value={domainAliases} onChange={(event) => setDomainAliases(event.target.value)}/></div></div><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" disabled={domainChecking || !domainList().length} onClick={() => void checkSiteDomains()}><RefreshCw size={14} className={domainChecking ? 'animate-spin' : ''}/>{domainChecking ? 'در حال بررسی DNS…' : 'بررسی DNS دامنه‌ها'}</Button><Button size="sm" disabled={busy} onClick={() => void saveSiteDomains()}><Save size={14}/>{busy ? 'در حال اعتبارسنجی…' : 'ذخیره دامنه‌ها'}</Button></div>{domainResults.length > 0 && <div className="space-y-2" aria-live="polite">{domainResults.map((item) => <div key={item.host} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-white/[.07] bg-background/60 px-3 py-2"><span dir="ltr" className="font-mono text-xs text-white">{item.host}</span><span dir="ltr" className={`text-left font-mono text-[11px] ${item.resolved ? 'text-emerald-300' : 'text-amber-300'}`}>{item.resolved ? item.addresses.join(' · ') : item.error || 'رکورد DNS پیدا نشد'}</span></div>)}<p className="text-[11px] text-muted-foreground">این بررسی فقط پاسخ DNS را نشان می‌دهد؛ برای اتصال سایت، رکورد A/AAAA را به IP سرور تنظیم کن.</p></div>}</section><div className="flex flex-wrap gap-2 border-y border-white/[.07] py-3"><Button size="sm" variant="outline" onClick={() => downloadText(`${toolSite.name}.conf`, toolConfig)}><Download size={14}/>دریافت کانفیگ</Button><Button size="sm" variant="outline" onClick={() => void runAction('test')}><ShieldCheck size={14}/>بررسی کل کانفیگ Nginx</Button><Button size="sm" variant="outline" disabled={!status?.active || busy} onClick={() => void runAction('reload')}><RefreshCw size={14}/>Reload سرویس</Button><Button size="sm" variant="outline" onClick={duplicateSite}><Plus size={14}/>ساخت کپی غیرفعال</Button><Button size="sm" variant="ghost" onClick={() => void loadSiteLog('access')}>Access log</Button><Button size="sm" variant="ghost" onClick={() => void loadSiteLog('error')}>Error log</Button></div>{siteLogKind && <section className="rounded-xl border border-white/10 p-3"><div className="mb-2 flex flex-wrap items-center justify-between gap-2"><h3 className="text-sm font-semibold text-white">{siteLogKind === 'access' ? 'Access log' : 'Error log'}</h3><span dir="ltr" className="max-w-full truncate font-mono text-[10px] text-muted-foreground">{siteLogPath}</span></div>{siteLogLoading ? <Skeleton className="h-28"/> : <pre dir="ltr" className="max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[10px] leading-5 text-muted-foreground">{siteLog || 'لاگی برای نمایش نیست.'}</pre>}</section>}</div>}</DialogContent></Dialog>
    <Dialog open={editorOpen} onOpenChange={setEditorOpen}><DialogContent className="max-h-[92vh] max-w-5xl overflow-y-auto"><DialogHeader><DialogTitle>ویرایش سایت <span className="font-mono text-primary" dir="ltr">{editing}</span></DialogTitle><DialogDescription>مسیر فایل: <code dir="ltr">/etc/nginx/sites-available/{editing}</code>. ذخیره فقط پس از موفقیت <code dir="ltr">nginx -t</code> اعمال می‌شود.</DialogDescription></DialogHeader><div className="flex justify-end"><Button type="button" variant="outline" size="sm" onClick={() => { setEditorOpen(false); openBuilder('edit') }}><Sparkles size={14}/>ساخت با فرم بصری</Button></div><form onSubmit={saveSite} className="space-y-4"><textarea aria-label="تنظیمات Nginx" spellCheck={false} dir="ltr" value={content} onChange={(event) => setContent(event.target.value)} className="min-h-[55vh] w-full resize-y rounded-xl border border-white/10 bg-background p-4 font-mono text-xs leading-6 text-[#f2f0eb] outline-none transition focus:border-primary/50 focus:ring-2 focus:ring-primary/15"/><div className="flex flex-wrap items-center justify-between gap-3"><p className="text-xs text-muted-foreground">سقف اندازهٔ فایل 512 KB است؛ در صورت خطا نسخهٔ قبلی حفظ می‌شود.</p><Button type="submit" disabled={busy}><Save size={15}/>{busy ? 'در حال اعتبارسنجی…' : 'اعتبارسنجی و ذخیره'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={builderOpen} onOpenChange={(open) => { setBuilderOpen(open); if (!open) setFilePickerOpen(false) }}><DialogContent className="max-h-[94vh] max-w-5xl overflow-y-auto"><DialogHeader><DialogTitle>{filePickerOpen ? 'انتخاب فایل SSL' : 'سازندهٔ بصری کانفیگ'}</DialogTitle><DialogDescription>{filePickerOpen ? 'فقط مسیر و نام فایل نمایش داده می‌شود؛ محتوای گواهی و کلید خصوصی خوانده نمی‌شود.' : 'قالب مناسب را انتخاب کن، فیلدها را پر کن و پیش‌نمایش کانفیگ را وارد ویرایشگر کن.'}</DialogDescription></DialogHeader>
      {filePickerOpen ? <div className="space-y-4"><div className="flex flex-wrap items-center gap-2"><Button type="button" size="sm" variant="outline" onClick={() => { setFileListing(null); void browseFiles() }}>پوشه‌های مجاز</Button>{fileListing?.path && fileListing.roots.some((root) => fileListing.path !== root) && <Button type="button" size="sm" variant="outline" onClick={() => { const current = fileListing.path; const parent = current.slice(0, current.lastIndexOf('/')) || '/'; void browseFiles(parent) }}><ChevronUp size={14}/>پوشهٔ بالاتر</Button>}<span dir="ltr" className="min-w-0 flex-1 truncate text-left font-mono text-[11px] text-muted-foreground">{fileListing?.path || 'انتخاب پوشه'}</span><Button type="button" size="sm" variant="ghost" onClick={() => setFilePickerOpen(false)}>بازگشت</Button></div>
        {!fileListing || !fileListing.path ? <div className="grid gap-2 sm:grid-cols-2">{fileListing?.roots.map((root) => <button key={root} type="button" dir="ltr" onClick={() => void browseFiles(root)} className="flex items-center gap-3 rounded-xl border border-white/10 bg-background/50 p-3 text-left font-mono text-xs text-white hover:border-primary/40"><FolderOpen size={16} className="shrink-0 text-primary"/>{root}</button>)}</div> : fileLoading ? <Skeleton className="h-40"/> : <div className="max-h-[45vh] space-y-1 overflow-auto rounded-xl border border-white/10 p-2">{fileListing.entries.map((entry) => <button key={entry.path} type="button" disabled={!entry.directory && (!entry.selectable || (filePickerFor === 'certificateKey' ? !/\.(key|pem)$/i.test(entry.name) : /\.key$/i.test(entry.name)))} onClick={() => entry.directory ? void browseFiles(entry.path) : (setBuilder((old) => ({ ...old, [filePickerFor]: entry.path })), setFilePickerOpen(false))} className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-xs text-muted-foreground hover:bg-white/[.04] disabled:cursor-not-allowed disabled:opacity-35"><FolderOpen size={15} className={entry.directory ? 'text-primary' : 'text-muted-foreground'}/><span dir="ltr" className="min-w-0 flex-1 truncate font-mono">{entry.name}</span>{entry.directory ? <span>پوشه</span> : entry.selectable ? <span>انتخاب</span> : null}</button>)}{!fileListing.entries.length && <p className="p-6 text-center text-xs text-muted-foreground">فایل SSL در این پوشه پیدا نشد.</p>}</div>}
      </div> : <div className="space-y-5">
        <div className="grid gap-2 sm:grid-cols-4">{([['static','سایت استاتیک','فایل‌های HTML/CSS'],['proxy','Reverse proxy','اتصال به برنامهٔ محلی'],['php','PHP / FastCGI','اجرای اسکریپت‌های PHP'],['redirect','تغییر مسیر','Redirect با کد 301']] as const).map(([value,title,detail]) => <button key={value} type="button" onClick={() => setBuilder((old) => ({ ...old, preset: value }))} className={`rounded-xl border p-3 text-right transition ${builder.preset === value ? 'border-primary/50 bg-primary/10' : 'border-white/10 bg-background/40 hover:border-white/20'}`}><span className="block text-sm font-semibold text-white">{title}</span><span className="mt-1 block text-[11px] text-muted-foreground">{detail}</span></button>)}</div>
        <div className="grid gap-4 md:grid-cols-2"><div className="space-y-2"><Label htmlFor="nginx-builder-domain">دامنه‌ها</Label><Input id="nginx-builder-domain" dir="ltr" value={builder.domain} onChange={(event) => setBuilder((old) => ({ ...old, domain: event.target.value }))} placeholder="example.com www.example.com"/></div><div className="space-y-2"><Label htmlFor="nginx-builder-listen">پورت HTTP</Label><Input id="nginx-builder-listen" dir="ltr" inputMode="numeric" value={builder.listen} onChange={(event) => setBuilder((old) => ({ ...old, listen: event.target.value.replace(/[^0-9]/g, '') }))}/></div>
          {builder.preset === 'static' || builder.preset === 'php' ? <><div className="space-y-2"><Label htmlFor="nginx-builder-root">مسیر ریشهٔ سایت</Label><Input id="nginx-builder-root" dir="ltr" value={builder.root} onChange={(event) => setBuilder((old) => ({ ...old, root: event.target.value }))} placeholder="/var/www/example.com"/></div><div className="space-y-2"><Label htmlFor="nginx-builder-index">فایل‌های پیش‌فرض</Label><Input id="nginx-builder-index" dir="ltr" value={builder.index} onChange={(event) => setBuilder((old) => ({ ...old, index: event.target.value }))}/></div></> : null}
          {builder.preset === 'proxy' && <div className="space-y-2 md:col-span-2"><Label htmlFor="nginx-builder-proxy">آدرس برنامهٔ مقصد</Label><Input id="nginx-builder-proxy" dir="ltr" value={builder.proxyPass} onChange={(event) => setBuilder((old) => ({ ...old, proxyPass: event.target.value }))} placeholder="http://127.0.0.1:3000"/></div>}
          {builder.preset === 'php' && <div className="space-y-2 md:col-span-2"><Label htmlFor="nginx-builder-php">Socket یا آدرس PHP-FPM</Label><Input id="nginx-builder-php" dir="ltr" value={builder.phpSocket} onChange={(event) => setBuilder((old) => ({ ...old, phpSocket: event.target.value }))}/><p className="text-[11px] text-muted-foreground">اسکریپت‌های PHP با FastCGI به این سرویس ارسال می‌شوند.</p></div>}
          {builder.preset === 'redirect' && <div className="space-y-2 md:col-span-2"><Label htmlFor="nginx-builder-redirect">نشانی مقصد</Label><Input id="nginx-builder-redirect" dir="ltr" value={builder.redirectTo} onChange={(event) => setBuilder((old) => ({ ...old, redirectTo: event.target.value }))}/></div>}
        </div>
        <div className="rounded-xl border border-white/10 p-4"><label className="flex items-center justify-between gap-3"><span><span className="block text-sm font-medium text-white">فعال‌سازی HTTPS</span><span className="mt-1 block text-xs text-muted-foreground">مسیر certificate و private key را از فایل‌منیجر انتخاب کن.</span></span><Switch checked={builder.ssl} onCheckedChange={(ssl) => setBuilder((old) => ({ ...old, ssl }))}/></label>{builder.ssl && <div className="mt-4 grid gap-3 md:grid-cols-2"><div className="space-y-2"><Label>گواهی SSL</Label><div className="flex gap-2"><Input dir="ltr" value={builder.certificate} onChange={(event) => setBuilder((old) => ({ ...old, certificate: event.target.value }))} placeholder="/etc/letsencrypt/live/.../fullchain.pem"/><Button type="button" variant="outline" size="icon" aria-label="انتخاب گواهی" onClick={() => openFilePicker('certificate')}><FolderOpen size={15}/></Button></div></div><div className="space-y-2"><Label>کلید خصوصی SSL</Label><div className="flex gap-2"><Input dir="ltr" value={builder.certificateKey} onChange={(event) => setBuilder((old) => ({ ...old, certificateKey: event.target.value }))} placeholder="/etc/letsencrypt/live/.../privkey.pem"/><Button type="button" variant="outline" size="icon" aria-label="انتخاب کلید خصوصی" onClick={() => openFilePicker('certificateKey')}><FolderOpen size={15}/></Button></div></div></div>}</div>
        <div className="grid gap-3 sm:grid-cols-2"><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span>فعال‌سازی Gzip</span><Switch checked={builder.gzip} onCheckedChange={(gzip) => setBuilder((old) => ({ ...old, gzip }))}/></label><div className="space-y-2"><Label htmlFor="nginx-builder-body">حداکثر اندازهٔ درخواست</Label><Input id="nginx-builder-body" dir="ltr" value={builder.maxBody} onChange={(event) => setBuilder((old) => ({ ...old, maxBody: event.target.value.replace(/[^0-9kKmMgG]/g, '') }))} placeholder="32m"/></div></div>
        <section className="space-y-3 rounded-xl border border-white/10 p-4"><div className="flex flex-wrap items-center justify-between gap-2"><div><h3 className="text-sm font-semibold text-white">Directiveهای اضافه</h3><p className="mt-1 text-xs text-muted-foreground">برای افزودن تنظیم‌هایی مثل timeout یا header.</p></div><Button type="button" variant="outline" size="sm" onClick={() => setExtraDirectives((old) => [...old, { name: '', value: '' }])}><Plus size={14}/>افزودن ورودی</Button></div>{extraDirectives.map((item,index) => <div key={index} className="grid gap-2 sm:grid-cols-[1fr_2fr_auto]"><Input dir="ltr" placeholder="proxy_read_timeout" value={item.name} onChange={(event) => setExtraDirectives((old) => old.map((row,i) => i === index ? { ...row, name: event.target.value } : row))}/><Input dir="ltr" placeholder="60s" value={item.value} onChange={(event) => setExtraDirectives((old) => old.map((row,i) => i === index ? { ...row, value: event.target.value } : row))}/><Button type="button" variant="ghost" size="icon" aria-label="حذف ورودی" onClick={() => setExtraDirectives((old) => old.filter((_,i) => i !== index))}><X size={15}/></Button></div>)}</section>
        <section className="space-y-3 rounded-xl border border-white/10 p-4"><div className="flex flex-wrap items-center justify-between gap-2"><div><h3 className="text-sm font-semibold text-white">Location و تنظیمات سفارشی</h3><p className="mt-1 text-xs text-muted-foreground">برای مسیرهای خاص یا اتصال اسکریپت؛ PHP-FPM هم در قالب PHP آماده است.</p></div><Button type="button" variant="outline" size="sm" onClick={() => setExtraLocations((old) => [...old, { path: '/api/', config: 'proxy_pass http://127.0.0.1:3000;' }])}><Code2 size={14}/>افزودن location</Button></div>{extraLocations.map((item,index) => <div key={index} className="space-y-2 rounded-lg border border-white/[.07] p-3"><div className="flex gap-2"><Input dir="ltr" placeholder="/api/" value={item.path} onChange={(event) => setExtraLocations((old) => old.map((row,i) => i === index ? { ...row, path: event.target.value } : row))}/><Button type="button" variant="ghost" size="icon" aria-label="حذف location" onClick={() => setExtraLocations((old) => old.filter((_,i) => i !== index))}><X size={15}/></Button></div><textarea dir="ltr" spellCheck={false} value={item.config} onChange={(event) => setExtraLocations((old) => old.map((row,i) => i === index ? { ...row, config: event.target.value } : row))} placeholder="proxy_pass http://127.0.0.1:3000;" className="min-h-20 w-full rounded-lg border border-white/10 bg-background p-3 font-mono text-xs leading-5 text-white"/></div>)}</section>
        {builderTarget === 'edit' && <Alert><AlertTitle>پیش‌نویس جایگزین</AlertTitle><AlertDescription>فیلدهای قابل‌تشخیص از کانفیگ فعلی پر شده‌اند. ساخت پیش‌نویس، متن ویرایشگر را با خروجی فرم جایگزین می‌کند؛ قبل از ذخیره آن را بررسی کن.</AlertDescription></Alert>}
        <details className="rounded-xl border border-white/10 p-4"><summary className="cursor-pointer text-sm font-medium text-white">پیش‌نمایش کانفیگ تولیدشده</summary><pre dir="ltr" className="mt-3 max-h-64 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[11px] leading-5 text-muted-foreground">{generatedConfig()}</pre></details>
        <div className="flex flex-wrap justify-end gap-2"><Button type="button" variant="outline" onClick={() => setBuilderOpen(false)}>انصراف</Button><Button type="button" onClick={applyBuilder} disabled={builder.ssl && (!builder.certificate.trim() || !builder.certificateKey.trim())}><Sparkles size={15}/>{builderTarget === 'edit' ? 'انتقال به ویرایشگر' : 'ساخت پیش‌نویس'}</Button></div>
      </div>}
    </DialogContent></Dialog>
  </>
}

const roleNames: Record<Role, string> = { admin: 'مدیر', operator: 'اپراتور', viewer: 'ناظر' }
export function UsersPage({ current, Heading, notify }: { current: User } & Common) {
  const [users, setUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<Role>('viewer')
  const refresh = useCallback(async () => {
    try {
      setUsers((await api<{ users: User[] }>('users')).users)
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن کاربران ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify])
  useEffect(() => {
    void refresh()
  }, [refresh])
  async function create(e: FormEvent) {
    e.preventDefault()
    setBusy(true)
    try {
      await api('users', { method: 'POST', body: JSON.stringify({ username, password, role }) })
      setOpen(false)
      setUsername('')
      setPassword('')
      setRole('viewer')
      notify('کاربر با موفقیت اضافه شد.')
      await refresh()
    } catch (e) {
      notify(e instanceof Error ? e.message : 'ساخت کاربر ناموفق بود')
    } finally {
      setBusy(false)
    }
  }
  async function update(user: User, change: Partial<Pick<User, 'role' | 'enabled'>>) {
    try {
      await api(`users/${user.id}`, {
        method: 'PUT',
        body: JSON.stringify({ role: change.role || user.role, enabled: change.enabled ?? user.enabled }),
      })
      notify('دسترسی کاربر به‌روزرسانی شد.')
      await refresh()
    } catch (e) {
      notify(e instanceof Error ? e.message : 'به‌روزرسانی ناموفق بود')
    }
  }
  return (
    <>
      <Heading
        title="مدیریت کاربران"
        description="حساب‌ها و سطح دسترسی اعضای پنل"
        action={
          <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger asChild>
              <Button>
                <Plus size={17} /> افزودن کاربر
              </Button>
            </DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>افزودن کاربر جدید</DialogTitle>
                <DialogDescription>برای این شخص یک حساب و نقش دسترسی بسازید.</DialogDescription>
              </DialogHeader>
              <form onSubmit={create} className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="new-username">نام کاربری</Label>
                  <Input
                    id="new-username"
                    required
                    minLength={3}
                    maxLength={32}
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    placeholder="username"
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="new-password">گذرواژه</Label>
                  <Input
                    id="new-password"
                    type="password"
                    required
                    minLength={12}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="حداقل 12 نویسه"
                  />
                </div>
                <div className="space-y-2">
                  <Label>نقش</Label>
                  <Select value={role} onValueChange={(value) => setRole(value as Role)}>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="viewer">ناظر — فقط مشاهده</SelectItem>
                      <SelectItem value="operator">اپراتور — آمادهٔ قابلیت‌های عملیاتی</SelectItem>
                      <SelectItem value="admin">مدیر — مدیریت کاربران</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <Button type="submit" className="w-full" disabled={busy}>
                  {busy ? 'در حال ساخت…' : 'ساخت حساب'}
                </Button>
              </form>
            </DialogContent>
          </Dialog>
        }
      />
      <div className="grid gap-4 sm:grid-cols-3">
        <Metric
          icon={Users}
          label="کل کاربران"
          value={formatNumber(users.length)}
          detail="حساب‌های ثبت‌شده"
        />
        <Metric
          icon={ShieldCheck}
          label="مدیران فعال"
          value={formatNumber(users.filter((u) => u.role === 'admin' && u.enabled).length)}
          detail="دسترسی کامل به پنل"
        />
        <Metric
          icon={UserRound}
          label="حساب‌های فعال"
          value={formatNumber(users.filter((u) => u.enabled).length)}
          detail="امکان ورود به پنل"
        />
      </div>
      <div className="mt-5"><DistributionBars title="ترکیب نقش‌ها" description="تعداد حساب‌ها در هر سطح دسترسی" items={(['admin', 'operator', 'viewer'] as Role[]).map((role) => ({ label: roleNames[role], value: users.filter((item) => item.role === role).length }))} /></div>
      <div className="mt-5">
        <SectionCard title="فهرست کاربران" description="تغییر نقش یا غیرفعال‌سازی در گزارش عملیات ثبت می‌شود">
          {loading ? (
            <Skeleton className="h-48" />
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>کاربر</TableHead>
                  <TableHead>نقش</TableHead>
                  <TableHead>وضعیت</TableHead>
                  <TableHead>فعال</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.map((u) => (
                  <TableRow key={u.id}>
                    <TableCell>
                      <div className="flex items-center gap-3">
                        <span className="grid h-9 w-9 place-items-center rounded-full bg-primary/20 font-semibold text-primary">
                          {u.username.slice(0, 1).toUpperCase()}
                        </span>
                        <div>
                          <div className="font-semibold">{u.username}</div>
                          {u.id === current.id && (
                            <span className="text-xs text-muted-foreground">حساب شما</span>
                          )}
                        </div>
                      </div>
                    </TableCell>
                    <TableCell>
                      {u.id === current.id ? (
                        <Badge>{roleNames[u.role]}</Badge>
                      ) : (
                        <Select value={u.role} onValueChange={(value) => update(u, { role: value as Role })}>
                          <SelectTrigger aria-label={`نقش ${u.username}`}>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="viewer">ناظر</SelectItem>
                            <SelectItem value="operator">اپراتور</SelectItem>
                            <SelectItem value="admin">مدیر</SelectItem>
                          </SelectContent>
                        </Select>
                      )}
                    </TableCell>
                    <TableCell>
                      <Badge variant={u.enabled ? 'success' : 'warning'}>
                        {u.enabled ? 'فعال' : 'غیرفعال'}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      <Switch
                        aria-label={`وضعیت ${u.username}`}
                        checked={u.enabled}
                        disabled={u.id === current.id}
                        onCheckedChange={(value) => update(u, { enabled: value })}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </SectionCard>
      </div>
      <Alert className="mt-5">
        <KeyRound size={17} className="mb-2 text-primary" />
        <AlertTitle>سطوح دسترسی</AlertTitle>
        <AlertDescription>
          مدیر می‌تواند حساب‌ها را مدیریت کند. ناظر و اپراتور اکنون به داده‌های خواندنی دسترسی دارند؛ مجوزهای
          عملیاتی اپراتور همراه با قابلیت‌های مربوطه افزوده می‌شوند.
        </AlertDescription>
      </Alert>
    </>
  )
}

function downloadText(filename: string, content: string, type = 'text/plain') {
  const url = URL.createObjectURL(new Blob([content], { type }))
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function AuditPage({ Heading, notify }: Common) {
  const [events, setEvents] = useState<AuditEvent[]>([])
  const [loading, setLoading] = useState(true)
  const refresh = useCallback(async () => {
    try {
      setEvents((await api<{ events: AuditEvent[] }>('audit')).events)
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن گزارش ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify])
  useEffect(() => {
    void refresh()
  }, [refresh])
  return (
    <>
      <Heading
        title="گزارش عملیات"
        description="ردیابی آخرین تغییرات و ورودهای پنل"
        action={
          <div className="flex gap-2"><Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button><Button variant="outline" size="sm" disabled={!events.length} onClick={() => { const rows = [['at','actor','action','detail'], ...events.map((event) => [new Date(event.at * 1000).toISOString(), event.actor, event.action, event.detail])]; downloadText('arg-audit.csv', '\uFEFF' + rows.map((row) => row.map((cell) => `"${String(cell).replaceAll('"', '""')}"`).join(',')).join('\n'), 'text/csv;charset=utf-8') }}><Download size={15}/> دریافت CSV</Button></div>
        }
      />
      <div className="mb-5"><DistributionBars title="ترکیب عملیات اخیر" description="از میان 100 رویداد آخر ثبت‌شده" items={['login', 'logout', 'account.password', 'user.create', 'user.update'].map((action) => ({ label: action, value: events.filter((event) => event.action === action).length }))} /></div>
      <SectionCard
        title="آخرین رویدادها"
        description="100 رویداد اخیر"
        right={<ScrollText size={18} className="text-primary" />}
      >
        {loading ? (
          <Skeleton className="h-48" />
        ) : events.length ? (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>زمان</TableHead>
                <TableHead>کاربر</TableHead>
                <TableHead>عملیات</TableHead>
                <TableHead>جزئیات</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {events.map((e, i) => (
                <TableRow key={`${e.at}-${i}`}>
                  <TableCell className="whitespace-nowrap text-muted-foreground">
                    {new Date(e.at * 1000).toLocaleString('en-US')}
                  </TableCell>
                  <TableCell className="font-semibold">{e.actor}</TableCell>
                  <TableCell>
                    <Badge variant="outline" className="mono">
                      {e.action}
                    </Badge>
                  </TableCell>
                  <TableCell className="text-muted-foreground">{e.detail}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : (
          <div className="py-12 text-center text-sm text-muted-foreground">هنوز رویدادی ثبت نشده است.</div>
        )}
      </SectionCard>
    </>
  )
}

type MonitoringConfig = { enabled: boolean; interval_seconds: number; retention_days: number; session_hours: number }

type UpdateInfo = { status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string }
export function SettingsPage({ Heading, logout, notify, user, update, openUpdateDialog, updating }: { Heading: Heading; logout: () => void; notify: (message: string) => void; user: User; update: UpdateInfo; openUpdateDialog: () => void; updating: boolean }) {
  const [config, setConfig] = useState<MonitoringConfig | null>(null)
  const [port, setPort] = useState(location.port || (location.protocol === 'https:' ? '443' : '80'))
  const [busy, setBusy] = useState(false)
  const [scheduledPort, setScheduledPort] = useState<number | null>(null)
  const isAdmin = user.role === 'admin'
  useEffect(() => { void api<MonitoringConfig>('monitoring/config').then(setConfig).catch((error) => notify(error instanceof Error ? error.message : 'دریافت تنظیمات ناموفق بود')) }, [notify])
  useEffect(() => { void api<{port: number}>('settings/port').then((result) => setPort(String(result.port))).catch(() => undefined) }, [])
  async function saveConfig(event: FormEvent) {
    event.preventDefault()
    if (!config) return
    setBusy(true)
    try { setConfig(await api<MonitoringConfig>('monitoring/config', { method: 'PUT', body: JSON.stringify(config) })); notify('تنظیمات تاریخچه و نشست ذخیره شد.') }
    catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ تنظیمات ناموفق بود') }
    finally { setBusy(false) }
  }
  async function changePort(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    try {
      const value = Number(port)
      await api('settings/port', { method: 'PUT', body: JSON.stringify({ port: value }) })
      setScheduledPort(value)
      notify('تغییر پورت زمان‌بندی شد. پس از راه‌اندازی مجدد، آدرس جدید را باز کن.')
    } catch (error) { notify(error instanceof Error ? error.message : 'تغییر پورت ناموفق بود') }
    finally { setBusy(false) }
  }
  function selector(label: string, key: 'interval_seconds' | 'retention_days' | 'session_hours', values: number[], unit: string) {
    return <label className="space-y-2 text-sm"><span className="block font-medium text-white">{label}</span><Select disabled={!isAdmin} value={String(config?.[key] ?? values[0])} onValueChange={(value) => setConfig((old) => old && { ...old, [key]: Number(value) })}><SelectTrigger dir="ltr"><SelectValue/></SelectTrigger><SelectContent>{values.map((value) => <SelectItem key={value} value={String(value)}>{value} {unit}</SelectItem>)}</SelectContent></Select></label>
  }
  return <>
    <Heading title="تنظیمات" description="زمان نمونه‌برداری، نگه‌داری تاریخچه، امنیت نشست و پورت دسترسی" />
    <div className="grid gap-5 xl:grid-cols-[1.25fr_.75fr]">
      <SectionCard title="ثبت تاریخچه" description="تنظیمات روی سرویس Rust ذخیره و بدون نصب دوباره اعمال می‌شود" right={<Badge variant={config?.enabled ? 'success' : 'outline'}>{config?.enabled ? 'در حال ثبت' : 'متوقف'}</Badge>}>
        {config ? <form onSubmit={saveConfig} className="space-y-5">
          <label className="flex items-center justify-between gap-4 rounded-[14px] border border-primary/20 bg-primary/[.06] p-4"><span><span className="block text-sm font-semibold text-white">ثبت داده‌های CPU، حافظه، دیسک و شبکه</span><span className="mt-1 block text-xs text-muted-foreground">در صورت غیرفعال‌کردن، داده‌های قبلی تا پایان مدت نگه‌داری باقی می‌مانند.</span></span><Switch disabled={!isAdmin} checked={config.enabled} onCheckedChange={(checked) => setConfig((old) => old && { ...old, enabled: checked })}/></label>
          <div className="grid gap-4 sm:grid-cols-3">{selector('فاصلهٔ نمونه‌برداری', 'interval_seconds', [5,10,15,30,60], 'ثانیه')}{selector('مدت نگه‌داری', 'retention_days', [1,3,7,14,30], 'روز')}{selector('اعتبار نشست', 'session_hours', [1,6,12,24,72], 'ساعت')}</div>
          <p className="text-xs leading-6 text-muted-foreground">تغییر اعتبار نشست روی ورودهای بعدی اعمال می‌شود. جمع‌آوری داده در پس‌زمینه انجام می‌شود و به بازبودن این صفحه وابسته نیست.</p>
          {isAdmin && <Button type="submit" disabled={busy}>{busy ? 'در حال ذخیره…' : 'ذخیرهٔ تنظیمات'}</Button>}
        </form> : <Skeleton className="h-48"/>}
      </SectionCard>
      <SectionCard title="دسترسی به پنل" description="نشانی جاری و تغییر پورت HTTP">
        <div className="rounded-xl border border-white/10 bg-white/[.03] p-4"><div className="mb-2 text-xs text-muted-foreground">آدرس فعلی</div><div className="break-all font-mono text-sm text-white" dir="ltr">{location.origin}</div></div>
        <Button variant="outline" className="mt-3" onClick={() => void navigator.clipboard.writeText(location.origin).then(() => notify('آدرس پنل کپی شد.')).catch(() => notify('کپی آدرس ناموفق بود.'))}>کپی آدرس</Button>
        {isAdmin && <form onSubmit={changePort} className="mt-6 space-y-3 border-t border-white/10 pt-5"><Label htmlFor="panel-port">پورت HTTP پنل</Label><div className="flex gap-2"><Input id="panel-port" type="number" min={1024} max={65535} required dir="ltr" lang="en" value={port} onChange={(event) => setPort(event.target.value)}/><Button type="submit" disabled={busy}>تغییر پورت</Button></div><p className="text-xs leading-6 text-muted-foreground">پورت HTTPS مستقل است. پس از تغییر، سرویس دوباره راه‌اندازی می‌شود و اتصال HTTP فعلی قطع خواهد شد.</p></form>}
        {scheduledPort && <Alert className="mt-4"><AlertDescription>نشانی HTTP جدید: <a className="font-mono text-primary underline" dir="ltr" href={`http://${location.hostname}:${scheduledPort}`}>http://{location.hostname}:{scheduledPort}</a></AlertDescription></Alert>}
      </SectionCard>
    </div>
    <div className="mt-5"><SectionCard title="درباره و به‌روزرسانی" description="وضعیت نصب‌شده را ببین و از یک پنجره نسخهٔ جدید را بررسی یا نصب کن." right={<Badge variant={updating || update.status === 'available' ? 'warning' : update.status === 'current' ? 'success' : 'outline'}>{updating ? 'در حال نصب' : update.status === 'available' ? 'نسخهٔ جدید' : update.status === 'current' ? 'به‌روز' : 'بررسی نشده'}</Badge>}>
      <div className="flex flex-wrap items-center justify-between gap-4"><div className="grid min-w-0 flex-1 gap-3 sm:grid-cols-2"><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 truncate font-mono text-lg font-semibold text-foreground" dir="ltr">{update.local || '—'}</div></div><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">آخرین نسخهٔ بررسی‌شده</div><div className="mt-2 truncate font-mono text-lg font-semibold text-foreground" dir="ltr">{update.remote || '—'}</div></div></div><div className="flex gap-2"><Button onClick={openUpdateDialog} disabled={updating}><RefreshCw size={15}/>{update.status === 'available' ? 'نصب نسخهٔ جدید' : 'بررسی و به‌روزرسانی'}</Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noopener noreferrer" className="grid h-10 w-10 place-items-center rounded-[10px] border border-border text-muted-foreground hover:bg-accent" aria-label="مخزن ARG در GitHub"><Github size={17}/></a></div></div>
    </SectionCard></div>
    <div className="mt-5"><PasswordCard notify={notify}/></div>
    <Button variant="outline" className="mt-7" onClick={logout}><X size={16}/> خروج از حساب</Button>
  </>
}

function PasswordCard({ notify }: { notify: (message: string) => void }) {
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  async function submit(e: FormEvent) {
    e.preventDefault()
    if (newPassword !== confirm) {
      notify('تکرار گذرواژه برابر نیست.')
      return
    }
    setBusy(true)
    try {
      await api('account/password', {
        method: 'POST',
        body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
      })
      setCurrentPassword('')
      setNewPassword('')
      setConfirm('')
      notify('گذرواژه تغییر کرد و نشست‌های دیگر بسته شدند.')
    } catch (e) {
      notify(e instanceof Error ? e.message : 'تغییر گذرواژه ناموفق بود')
    } finally {
      setBusy(false)
    }
  }
  return (
    <SectionCard
      title="تغییر گذرواژه"
      description="گذرواژهٔ حساب فعلی و خروج از نشست‌های دیگر"
      right={<KeyRound size={18} className="text-primary" />}
    >
      <form onSubmit={submit} className="grid gap-4 md:grid-cols-3">
        <div className="space-y-2">
          <Label htmlFor="current-password">گذرواژهٔ فعلی</Label>
          <Input
            id="current-password"
            type="password"
            autoComplete="current-password"
            required
            value={currentPassword}
            onChange={(e) => setCurrentPassword(e.target.value)}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="next-password">گذرواژهٔ جدید</Label>
          <Input
            id="next-password"
            type="password"
            autoComplete="new-password"
            required
            minLength={12}
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="confirm-password">تکرار گذرواژه</Label>
          <Input
            id="confirm-password"
            type="password"
            autoComplete="new-password"
            required
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
          />
        </div>
        <div>
          <Button type="submit" disabled={busy}>
            {busy ? 'در حال ذخیره…' : 'ذخیرهٔ گذرواژه'}
          </Button>
        </div>
      </form>
    </SectionCard>
  )
}
