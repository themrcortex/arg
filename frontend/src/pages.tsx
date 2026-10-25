import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
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
  LogOut,
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
  type ManagedFirewallRulesResponse,
  type User,
  type Role,
} from './lib/api'
import { Badge } from './components/ui/badge'
import { NginxRuntimePanel } from './nginx/NginxRuntimePanel'
import { SiteActionsPanel } from './nginx/SiteActionsPanel'
import { FastActionsPanel } from './dashboard/FastActionsPanel'
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
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './components/ui/tooltip'
import { formatPanelDate, usePanelTimeZone } from './lib/timezone'
import { S3OverviewKpi, StorageInventorySection, StorageOverviewKpi } from './components/storage/StorageInventory'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type Sample = { at: number; time: string; rx: number; tx: number }
type ResourceSample = { at: number; cpu: number; memory: number; disk: number }
type StoredSample = { at: number; cpu: number; memory: number; disk: number; rx: number; tx: number; disk_read: number | null; disk_write: number | null; disk_used_bytes: number | null; disk_total_bytes: number | null }
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
  const timeZone = usePanelTimeZone()
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
  const history = samples.map((sample) => ({ at: sample.at, time: new Intl.DateTimeFormat('en-US', { timeZone, hour: '2-digit', minute: '2-digit' }).format(sample.at * 1000), rx: sample.rx, tx: sample.tx }))
  const resourceHistory = samples.map(({ at, cpu, memory, disk }) => ({ at, cpu, memory, disk }))
  const diskCapacityHistory = samples.filter((sample) => sample.disk_total_bytes != null && sample.disk_total_bytes > 0 && sample.disk_used_bytes != null).map((sample) => ({ at: sample.at, capacity: Math.min(100, sample.disk_used_bytes! / sample.disk_total_bytes! * 100) }))
  const diskIoHistory = samples.filter((sample) => sample.disk_read != null && sample.disk_write != null).map((sample) => ({ at: sample.at, read: sample.disk_read!, write: sample.disk_write! }))
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
  return { data, history, resourceHistory, diskCapacityHistory, diskIoHistory, cpu: samples.at(-1)?.cpu ?? null, loading, refresh, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, historyEnabled, dashboardError, historyError }
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
    <Card className={`arg-section-card ${className}`}>
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

function RefreshButton({ onRefresh, label = 'به‌روزرسانی', className = '' }: {
  onRefresh: () => Promise<unknown> | void
  label?: string
  className?: string
}) {
  const [busy, setBusy] = useState(false)
  async function run() {
    if (busy) return
    setBusy(true)
    try { await onRefresh() } finally { setBusy(false) }
  }
  return <Button variant="outline" size="sm" className={className} onClick={() => void run()} disabled={busy} aria-busy={busy}>
    <RefreshCw size={15} className={busy ? 'animate-spin' : ''}/>{busy ? 'در حال به‌روزرسانی…' : label}
  </Button>
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

function FirewallPageSkeleton() {
  return <div className="space-y-5" role="status" aria-label="در حال بارگذاری صفحهٔ فایروال">
    <div className="grid gap-3 sm:grid-cols-3">{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="h-[124px] rounded-[14px]"/>)}</div>
    <div className="rounded-[14px] border border-border bg-card p-5"><Skeleton className="mb-5 h-5 w-40"/><Skeleton className="mb-5 h-10 w-full"/><Skeleton className="h-48 w-full"/></div>
    <div className="rounded-[14px] border border-border bg-card p-5"><Skeleton className="mb-4 h-5 w-48"/><Skeleton className="h-48 w-full"/></div>
  </div>
}

function NginxPageSkeleton() {
  return <div className="space-y-5" role="status" aria-label="در حال بارگذاری صفحهٔ Nginx">
    <div className="rounded-[14px] border border-border bg-card p-5"><Skeleton className="mb-5 h-5 w-36"/><div className="grid gap-3 sm:grid-cols-3">{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="h-20"/>)}</div><Skeleton className="mt-4 h-10 w-full"/><Skeleton className="mt-4 h-20 w-full"/></div>
    <div className="grid gap-3 sm:grid-cols-3">{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="h-[112px] rounded-[14px]"/>)}</div>
    <div className="rounded-[14px] border border-border bg-card p-5"><Skeleton className="mb-5 h-5 w-44"/><Skeleton className="mb-5 h-10 w-full"/>{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="mb-3 h-28 w-full last:mb-0"/>)}</div>
  </div>
}

function AlertControls({ user, notify }: { user: User; notify: (message: string) => void }) {
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
  return <SectionCard title="آستانه‌های هشدار" description="وقتی مصرف CPU، حافظه یا دیسک از این مقدار عبور کند، اعلان بالای صفحات نمایش داده می‌شود.">
    <form onSubmit={save} className="grid gap-4 md:grid-cols-3">{([['cpu', 'پردازنده'], ['memory', 'حافظه'], ['disk', 'دیسک']] as const).map(([key, label]) => <div key={key} className="rounded-xl border border-white/10 bg-white/[.025] p-4"><div className="mb-3 flex items-center justify-between text-xs"><Label htmlFor={`limit-${key}`}>{label}</Label><Badge variant="outline">آستانهٔ اعلان</Badge></div><div className="flex items-center gap-2"><Input id={`limit-${key}`} type="number" lang="en" dir="ltr" min={1} max={100} value={limits[key]} disabled={user.role !== 'admin'} onChange={(event) => setLimits((old) => ({ ...old, [key]: Number(event.target.value) }))}/><span className="text-sm">٪</span></div></div>)}{user.role === 'admin' && <Button type="submit" disabled={busy} className="md:col-span-3 md:w-fit">ذخیرهٔ آستانه‌ها</Button>}</form>
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
  return <section className="arg-dashboard-panel arg-chart-surface min-w-0 rounded-[14px] border p-4" aria-label={title}>
    <div className="mb-4 flex items-center gap-2"><img src={`/arg/icons/${icon}.svg`} alt="" aria-hidden="true" className="h-5 w-5"/><h2 className="text-sm font-bold text-foreground">{title}</h2></div>
    {children}
    <p className="mt-3 text-xs text-muted-foreground">{description}</p>
  </section>
}

export function DashboardPage({ notify, user }: Common & { user: User }) {
  const { data, history, resourceHistory, cpu, loading, refresh, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, historyEnabled, dashboardError, historyError } = useDashboard(notify)
  const [firewall, setFirewall] = useState<Firewall | null>(null)
  const [ruleCount, setRuleCount] = useState<number | null>(null)
  const [s3Summary, setS3Summary] = useState<{ connections: number; healthy_connections: number; contracts_due_soon: number; usage_bytes: number | null; quota_bytes: number | null } | null>(null)
  useEffect(() => {
    let active = true
    const readFirewall = async () => {
      const [status, rules] = await Promise.allSettled([api<Firewall>('firewall'), api<{ rules: ManagedFirewallRule[] }>('firewall/rules')])
      if (!active) return
      if (status.status === 'fulfilled') setFirewall(status.value)
      if (rules.status === 'fulfilled') setRuleCount(rules.value.rules.length)
      try { setS3Summary(await api<{ connections: number; healthy_connections: number; contracts_due_soon: number; usage_bytes: number | null; quota_bytes: number | null }>('s3/summary')) } catch { /* The aggregate is not required for the rest of the dashboard. */ }
    }
    void readFirewall()
    const timer = window.setInterval(() => void readFirewall(), 30000)
    return () => { active = false; window.clearInterval(timer) }
  }, [])
  const memory = data?.memory_kib ? (1 - data.available_kib / data.memory_kib) * 100 : null
  const load = data?.load?.trim().split(/\s+/) || []
  const percent = (value: number | null) => value === null ? '—' : `${formatNumber(Math.round(value * 10) / 10)}%`
  const firewallActive = firewall?.available && /active/i.test(firewall.status) && !/inactive/i.test(firewall.status)
  return <div className="arg-dashboard space-y-3.5">
    <div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-xs font-semibold text-primary" dir="ltr">ARG / ارگ</div><h1 className="mt-1 text-[27px] font-bold text-foreground">نمای کلی سرور</h1></div><RefreshButton label="تازه‌سازی" onRefresh={refresh}/></div>
    <section className="arg-dashboard-hero relative min-h-[150px] overflow-hidden rounded-[14px] border p-5 md:p-6">
      <img src="/arg/illustrations/arg-dashboard-banner.svg" alt="" aria-hidden="true" className="arg-hero-art pointer-events-none absolute bottom-0 left-0 h-full w-[75%] object-cover opacity-[.28]"/>
      <div className="relative z-10"><div className="text-xs font-semibold text-primary" dir="ltr">ARG / SERVER OVERVIEW</div><h2 className="mt-4 text-xl font-bold text-foreground md:text-2xl">به ارگ خوش آمدید</h2><p className="mt-2 text-sm text-muted-foreground">وضعیت منابع، شبکه و امنیت سرور در یک نگاه.</p>{data?.hostname && <div className="mt-4 inline-flex max-w-full items-center gap-2 rounded-lg border border-border bg-background/60 px-3 py-1.5 text-xs"><span className="text-muted-foreground">سرور</span><span className="truncate font-mono text-foreground" dir="ltr">{data.hostname}</span></div>}</div>
    </section>
    <FastActionsPanel user={user} notify={notify}/>
    {(dashboardError || historyError) && <Alert className="border-amber-400/25 bg-amber-400/[.06]"><AlertDescription>{dashboardError && historyError ? 'دریافت وضعیت سرور و تاریخچه ناموفق بود.' : dashboardError ? 'دریافت وضعیت فعلی سرور ناموفق بود.' : 'دریافت تاریخچهٔ منابع ناموفق بود.'} <RefreshButton className="mr-2 inline-flex h-auto border-0 p-0 font-semibold text-primary underline" label="تلاش دوباره" onRefresh={refresh}/></AlertDescription></Alert>}
    {loading && !data ? <div className="arg-kpi-grid grid gap-3">{Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="h-[142px]"/>)}</div> : <section className="arg-kpi-grid grid gap-3" aria-label="شاخص‌های سرور">
      <ArgKpi icon="metric-cpu" label="پردازنده" value={percent(cpu)} detail={cpu === null ? 'نمونهٔ CPU موجود نیست' : 'آخرین نمونهٔ ثبت‌شده'} progress={cpu ?? undefined} tone="green"/>
      <ArgKpi icon="metric-memory" label="حافظهٔ RAM" value={percent(memory)} detail={data?.memory_kib ? `${formatBytes((data.memory_kib - data.available_kib) * 1024)} از ${formatBytes(data.memory_kib * 1024)}` : 'داده‌ای در دسترس نیست'} progress={memory ?? undefined}/>
      <StorageOverviewKpi inventory={data?.storage} rootTotalBytes={data?.disk_total_bytes} rootUsedBytes={data?.disk_used_bytes} rootFreeBytes={data?.disk_free_bytes}/>
      <S3OverviewKpi summary={s3Summary}/>
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
  </div>
}

function DataRows({ rows }: { rows: { label: string; value: string; detail?: string }[] }) {
  return <Table><TableHeader><TableRow><TableHead>شاخص</TableHead><TableHead>مقدار</TableHead><TableHead className="hidden md:table-cell">توضیح</TableHead></TableRow></TableHeader><TableBody>{rows.map((row) => <TableRow key={row.label}><TableCell className="font-medium">{row.label}</TableCell><TableCell><span className="font-mono font-semibold text-white tabular-nums" dir="ltr">{row.value}</span></TableCell><TableCell className="hidden text-xs text-muted-foreground md:table-cell">{row.detail || '—'}</TableCell></TableRow>)}</TableBody></Table>
}

export function SystemPage({ Heading, notify }: Common) {
  const { data, resourceHistory, diskCapacityHistory, diskIoHistory, cpu, range, setRange, customFrom, setCustomFrom, customTo, setCustomTo, applyCustomRange, loading, refresh, historyEnabled } = useDashboard(notify)
  const totalMemory = (data?.memory_kib || 0) * 1024
  const availableMemory = (data?.available_kib || 0) * 1024
  const memoryPercent = totalMemory ? Math.round((1 - availableMemory / totalMemory) * 100) : 0
  const rootVolume = data?.storage?.volumes.find((volume) => volume.mount_point === '/')
  const totalDisk = rootVolume?.total_bytes ?? data?.disk_total_bytes ?? 0
  const usedDisk = rootVolume?.used_bytes ?? data?.disk_used_bytes ?? Math.max(0, totalDisk - (data?.disk_free_bytes || 0))
  const freeDisk = rootVolume?.available_bytes ?? data?.disk_free_bytes ?? 0
  const diskPercent = rootVolume?.use_percent != null ? rootVolume.use_percent : totalDisk ? usedDisk / totalDisk * 100 : 0
  const load = (data?.load || '').split(' ')
  const memoryDetail = data?.memory_details
  return <>
    <Heading title="سیستم و منابع" description="پردازنده، حافظه و دیسک را با جزئیات واقعی سرور بررسی کن." action={<RefreshButton onRefresh={refresh}/>}/>
    {loading && !data ? <LoadingCards/> : <>
      <div className="grid gap-4 md:grid-cols-3"><Metric icon={Cpu} label="CPU" value={cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`} detail={`بار 1/5/15 دقیقه: ${data?.load || '—'}`} progress={cpu || 0}/><Metric icon={Database} label="حافظه" value={`${formatNumber(memoryPercent)}%`} detail={`${formatBytes(totalMemory - availableMemory)} از ${formatBytes(totalMemory)}`} progress={memoryPercent}/><Metric icon={HardDrive} label="دیسک ریشه /" value={`${formatNumber(Math.round(diskPercent * 10) / 10)}%`} detail={`${formatBytes(usedDisk)} مصرف‌شده · ${formatBytes(totalDisk)} ظرفیت`} progress={diskPercent}/></div>
      <div className="mt-5"><SectionCard title="تاریخچهٔ منابع" description="نمونه‌ها توسط سرویس Rust ذخیره می‌شوند" right={<TimeRangeFilter range={range} setRange={setRange} from={customFrom} setFrom={setCustomFrom} to={customTo} setTo={setCustomTo} applyCustom={applyCustomRange}/>}>{!historyEnabled && <HistoryPaused/>}<ResourceChart samples={resourceHistory}/></SectionCard></div>
      <div className="mt-5 grid gap-4 xl:grid-cols-2"><ArgChartPanel icon="metric-disk" title="ظرفیت دیسک ریشه /" description="درصد مصرف؛ مقدار کل و مصرف‌شده در جدول پایین آمده است."><TimeSeriesChart samples={diskCapacityHistory} series={[{ key: 'capacity', label: 'مصرف فضای ریشه', color: '#dda46f' }]} max={100} unit="%"/></ArgChartPanel><ArgChartPanel icon="metric-disk" title="Disk Read / Write ریشه /" description="سرعت خواندن و نوشتن بلاک‌دیوایس ریشه بر حسب بایت بر ثانیه"><TimeSeriesChart samples={diskIoHistory} series={[{ key: 'read', label: 'Read', color: '#69acde' }, { key: 'write', label: 'Write', color: '#dda46f' }]} max={Math.max(1024, ...diskIoHistory.flatMap((sample) => [sample.read, sample.write])) * 1.12} unit="bytes"/></ArgChartPanel></div>
      <div className="mt-5"><StorageInventorySection inventory={data?.storage} loading={loading && !data} onRefresh={refresh}/></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="پردازنده و بار سیستم" description="مقادیر لحظه‌ای و میانگین بار Linux"><DataRows rows={[{ label: 'مصرف CPU', value: cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`, detail: 'میانگین هسته‌ها' }, { label: 'هسته‌های در دسترس', value: formatNumber(data?.cpu_cores || 0) }, { label: 'بار 1 دقیقه', value: load[0] || '—' }, { label: 'بار 5 دقیقه', value: load[1] || '—' }, { label: 'بار 15 دقیقه', value: load[2] || '—' }, { label: 'زمان روشن‌بودن', value: `${formatNumber(Math.floor((data?.uptime_seconds || 0) / 86400))}d ${formatNumber(Math.floor(((data?.uptime_seconds || 0) % 86400) / 3600))}h` }]}/></SectionCard><SectionCard title="حافظه و Swap" description="مقادیر خوانده‌شده از /proc/meminfo"><DataRows rows={[{ label: 'کل RAM', value: formatBytes(totalMemory) }, { label: 'مصرف‌شده', value: formatBytes(totalMemory - availableMemory), detail: 'بر اساس MemAvailable' }, { label: 'در دسترس', value: formatBytes(availableMemory) }, { label: 'Cache', value: formatBytes((memoryDetail?.cached_kib || 0) * 1024) }, { label: 'Buffer', value: formatBytes((memoryDetail?.buffers_kib || 0) * 1024) }, { label: 'Swap کل', value: formatBytes((data?.swap_total_kib || 0) * 1024) }, { label: 'Swap مصرف‌شده', value: formatBytes(((data?.swap_total_kib || 0) - (data?.swap_free_kib || 0)) * 1024) }]}/></SectionCard></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="دیسک ریشه /" description="این اعداد از همان Mount Point ریشه در فهرست فضای ذخیره‌سازی آمده‌اند."><DataRows rows={[{ label: 'ظرفیت کل', value: totalDisk ? formatBytes(totalDisk) : '—' }, { label: 'مصرف‌شده', value: totalDisk ? formatBytes(usedDisk) : '—' }, { label: 'در دسترس', value: totalDisk ? formatBytes(freeDisk) : '—' }, { label: 'رزرو سیستم‌فایل', value: rootVolume?.reserved_bytes == null ? '—' : formatBytes(rootVolume.reserved_bytes) }, { label: 'درصد مصرف', value: totalDisk ? `${formatNumber(Math.round(diskPercent * 10) / 10)}%` : '—' }, { label: 'Read / Write فعلی', value: rootVolume?.read_bytes_per_sec == null || rootVolume?.write_bytes_per_sec == null ? 'در انتظار نمونه' : `${formatBytes(rootVolume.read_bytes_per_sec)}/s · ${formatBytes(rootVolume.write_bytes_per_sec)}/s`, detail: 'بلاک‌دیوایس ریشه' }]}/></SectionCard><SectionCard title="مصرف پنل ARG" description="اندازه‌گیری همین فرایند"><DataRows rows={[{ label: 'CPU پنل', value: `${formatNumber(Math.round((data?.process_cpu_percent || 0) * 10) / 10)}%` }, { label: 'RAM پنل', value: formatBytes(data?.process_rss_bytes || 0), detail: 'حافظهٔ مقیم فرایند (RSS)' }]}/></SectionCard></div>
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
    <Heading title="پورت‌های شنونده" description="پورت‌های باز در خود سرور را پیدا کن و اتصال TCP را آزمایش کن." action={<RefreshButton onRefresh={refresh}/>}/>
    <div className="mb-3 grid grid-cols-3 gap-2">{([{icon:Radio,label:'شنونده‌ها',value:data?.ports.length || 0},{icon:Network,label:'TCP',value:data?.ports.filter((port) => port.protocol === 'TCP').length || 0},{icon:Wifi,label:'UDP',value:data?.ports.filter((port) => port.protocol === 'UDP').length || 0}]).map((item) => <div key={item.label} className="flex min-w-0 items-center gap-2 rounded-xl border border-white/[.08] bg-card/70 px-3 py-2"><item.icon size={15} className="shrink-0 text-primary"/><span className="truncate text-[11px] text-muted-foreground">{item.label}</span><b className="mr-auto font-mono text-sm text-white" dir="ltr">{formatNumber(item.value)}</b></div>)}</div>
    <div><SectionCard title="فهرست پورت‌ها" description="فهرست فشرده با جست‌وجوی فوری و فیلترهای سریع" right={<Badge variant="outline">{formatNumber(ports.length)} مورد</Badge>}>
      <div className="mb-3 space-y-2 rounded-xl border border-white/[.08] bg-[#121b23] p-2.5">
        <div className="flex flex-wrap items-center gap-2"><label className="relative min-w-[180px] flex-1"><span className="sr-only">جست‌وجوی شمارهٔ پورت</span><Search size={15} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="h-9 pr-9 text-sm" inputMode="numeric" lang="en" dir="ltr" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="شمارهٔ پورت…"/><span className="sr-only" aria-live="polite">{formatNumber(ports.length)} نتیجه</span></label><div className="flex gap-1" aria-label="فیلتر پروتکل">{(['all','TCP','UDP'] as const).map((value) => <button key={value} onClick={() => setProtocol(value)} aria-pressed={protocol === value} className={`rounded-lg px-2.5 py-2 text-[11px] transition ${protocol === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:bg-white/[.04] hover:text-white'}`}>{value === 'all' ? 'همه' : value}</button>)}</div><div className="flex gap-1" aria-label="فیلتر نسخهٔ IP">{(['all','IPv4','IPv6'] as const).map((value) => <button key={value} onClick={() => setIpVersion(value)} aria-pressed={ipVersion === value} className={`rounded-lg px-2.5 py-2 text-[11px] transition ${ipVersion === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:bg-white/[.04] hover:text-white'}`}>{value === 'all' ? 'همهٔ IP' : value}</button>)}</div>{(query || protocol !== 'all' || ipVersion !== 'all') && <button className="px-1 text-[11px] text-[#dda46f] hover:text-white" onClick={() => { setQuery(''); setProtocol('all'); setIpVersion('all') }}>پاک‌کردن</button>}</div>
        <div className="flex flex-wrap items-center gap-1.5 border-t border-white/[.06] pt-2"><span className="ml-1 text-[10px] text-muted-foreground">پرتکرار:</span>{[['22','SSH'],['53','DNS'],['80','HTTP'],['443','HTTPS']] .map(([port,label]) => <button key={port} onClick={() => setQuery(port)} className={`rounded-full border px-2.5 py-1 text-[10px] transition ${query === port ? 'border-primary/40 bg-primary/10 text-white' : 'border-white/10 text-muted-foreground hover:text-white'}`}><span dir="ltr" className="font-mono">{port}</span> {label}</button>)}</div>
      </div>
      {loading && !data ? <Skeleton className="h-36"/> : <div className="max-h-[62vh] overflow-y-auto rounded-lg border border-white/[.06]"><PortsTable ports={ports}/></div>}
      <details className="mt-3 rounded-lg border border-white/[.07] px-3 py-2"><summary className="cursor-pointer text-xs text-muted-foreground">آزمایش اتصال TCP</summary><div className="pt-3"><PortProbeControls notify={notify}/></div></details>
    </SectionCard></div>
    <Alert className="mt-5"><Info size={17} className="mb-2 text-primary"/><AlertTitle>شنود محلی و دسترسی بیرونی</AlertTitle><AlertDescription>وجود پورت در این فهرست به معنی دسترسی از اینترنت نیست. قانون UFW و فایروال ارائه‌دهنده را نیز بررسی کن. حجم ترافیک به‌تفکیک پورت هنوز اندازه‌گیری نمی‌شود.</AlertDescription></Alert>
  </>
}

function HistoryPaused() { return <div className="mb-4 flex items-center gap-2 rounded-xl border border-amber-400/20 bg-amber-400/[.06] px-4 py-3 text-xs text-amber-200"><span className="h-2 w-2 rounded-full bg-amber-400"/> ثبت تاریخچه متوقف است. برای ادامه، از تنظیمات پنل آن را فعال کن.</div> }

function ResourceChart({ samples }: { samples: ResourceSample[] }) {
  return <TimeSeriesChart samples={samples} series={[{ key: 'cpu', label: 'پردازنده', color: '#53c992' }, { key: 'memory', label: 'حافظه', color: '#dda46f' }, { key: 'disk', label: 'دیسک ریشه /', color: '#49b8a8' }]} max={100} unit="%" />
}

type ChartSeries = { key: string; label: string; color: string }
type ChartSample = ResourceSample | Sample | { at: number; capacity: number } | { at: number; read: number; write: number }
function TimeSeriesChart({ samples, series, max, unit }: { samples: ChartSample[]; series: ChartSeries[]; max: number; unit: string }) {
  const timeZone = usePanelTimeZone()
  const [hover, setHover] = useState<number | null>(null)
  const chartHost = useRef<HTMLDivElement>(null)
  const [chartSize, setChartSize] = useState({ width: 760, height: 222 })
  const hasSamples = samples.length > 0
  useEffect(() => {
    if (!hasSamples) return
    const element = chartHost.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) => {
      const width = Math.max(240, Math.round(entry.contentRect.width))
      const height = Math.max(160, Math.round(entry.contentRect.height))
      setChartSize((current) => current.width === width && current.height === height ? current : { width, height })
    })
    observer.observe(element)
    return () => observer.disconnect()
  }, [hasSamples])
  if (!samples.length) return <div className="flex h-[195px] flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border bg-background p-6 text-center text-sm text-muted-foreground"><img src="/arg/illustrations/arg-empty-state.svg" alt="" aria-hidden="true" className="h-12 w-12 opacity-50"/>داده‌ای در این بازه ثبت نشده است.</div>
  const { width, height } = chartSize
  const left = Math.min(58, width * .16), right = 14, top = 18, bottom = height - 30
  const x = (index: number) => left + index * (width - left - right) / Math.max(samples.length - 1, 1)
  const y = (value: number) => bottom - Math.min(Math.max(value / max, 0), 1) * (bottom - top)
  const selected = hover === null ? samples.length - 1 : Math.max(0, Math.min(hover, samples.length - 1))
  const current = samples[selected]
  const read = (sample: ChartSample, key: string) => Number((sample as unknown as Record<string, number>)[key]) || 0
  const display = (value: number) => unit === '%' ? `${formatNumber(Math.round(value * 10) / 10)}%` : `${formatBytes(value)}/s`
  const timeSpan = samples.at(-1)!.at - samples[0].at
  const longRange = timeSpan > 24 * 60 * 60
  const stamp = (at: number) => new Intl.DateTimeFormat('en-US', { timeZone, ...(longRange ? { month: 'short' as const, day: 'numeric' as const } : {}), hour: '2-digit', minute: '2-digit', hour12: false }).format(at * 1000)
  const axisIndices = (width < 520 ? [0, .5, 1] : [0, .25, .5, .75, 1]).map((fraction) => Math.round(fraction * (samples.length - 1)).toString()).filter((value, index, all) => all.indexOf(value) === index).map(Number)
  return <div className="overflow-hidden rounded-xl border border-border bg-background">
    <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">{series.map((item) => <div key={item.key} className="flex items-center gap-2 rounded-lg bg-secondary/40 px-2 py-1 text-xs"><span className="h-1.5 w-1.5 rounded-full" style={{ background: item.color }}/><span className="text-muted-foreground">{item.label}</span><b className="font-mono font-semibold text-foreground tabular-nums" dir="ltr">{display(read(current, item.key))}</b></div>)}<span className="mr-auto font-mono text-xs text-muted-foreground" dir="ltr">{stamp(current.at)}</span></div>
    <div ref={chartHost} className="h-[230px] px-2 pt-2 md:px-3" dir="ltr"><svg viewBox={`0 0 ${width} ${height}`} width="100%" height="100%" onMouseLeave={() => setHover(null)} onMouseMove={(event) => { const rect = event.currentTarget.getBoundingClientRect(); setHover(Math.round(((event.clientX - rect.left) / rect.width * width - left) / (width - left - right) * (samples.length - 1))) }} role="img" aria-label={`تاریخچهٔ ${series.map((item) => item.label).join('، ')}`}>
      <defs><linearGradient id={`overview-fill-${series[0].key}`} x1="0" y1="0" x2="0" y2="1"><stop stopColor={series[0].color} stopOpacity=".24"/><stop offset="1" stopColor={series[0].color} stopOpacity="0"/></linearGradient></defs>
      {[0, .25, .5, .75, 1].map((fraction) => { const py = y(fraction * max); return <g key={fraction}><line x1={left} x2={width-right} y1={py} y2={py} stroke="#2a3540" strokeOpacity=".65" strokeDasharray="2 7"/><text x={left-11} y={py+4} fill="#93a0aa" fontSize="10" textAnchor="end">{unit === '%' ? `${Math.round(fraction * max)}%` : formatBytes(fraction * max)}</text></g> })}
      {series.map((item, lineIndex) => { const points = samples.map((sample, index) => `${x(index)},${y(read(sample, item.key))}`).join(' '); return <g key={item.key}>{lineIndex === 0 && <polygon points={`${left},${bottom} ${points} ${x(samples.length - 1)},${bottom}`} fill={`url(#overview-fill-${item.key})`}/>}<polyline points={points} fill="none" stroke={item.color} strokeOpacity={lineIndex === 0 ? 1 : .86} strokeWidth={lineIndex === 0 ? 3 : 2.4} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke"/></g> })}
      <line x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} stroke="#dda46f" strokeOpacity=".42" strokeDasharray="3 6"/>
      {series.map((item) => <circle key={item.key} cx={x(selected)} cy={y(read(current, item.key))} r="4.8" fill={item.color} stroke="#0c1219" strokeWidth="2"/>)}
      {axisIndices.map((index) => <text key={`time-${index}`} x={x(index)} y={height - 4} textAnchor={index === 0 ? 'start' : index === samples.length - 1 ? 'end' : 'middle'} fill="#aab4bc" fontSize={width < 520 ? 9 : 10}>{stamp(samples[index].at)}</text>)}
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
          <TableHead className="sticky top-0 z-[1] h-8 bg-card px-3">پورت</TableHead>
          <TableHead className="sticky top-0 z-[1] h-8 bg-card px-3">پروتکل</TableHead>
          <TableHead className="sticky top-0 z-[1] h-8 bg-card px-3">نسخهٔ IP</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {ports.map((p, i) => (
          <TableRow key={`${p.protocol}-${p.port}-${p.ipv6}-${i}`} className="h-9">
            <TableCell className="px-3 py-1.5">
              <span className="font-mono tabular-nums text-primary" dir="ltr">{p.port}</span>
            </TableCell>
            <TableCell className="px-3 py-1.5">
              <Badge variant="outline">{p.protocol}</Badge>
            </TableCell>
            <TableCell className="px-3 py-1.5 font-mono text-xs text-muted-foreground" dir="ltr">IPv{p.ipv6 ? '6' : '4'}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  ) : (
    <div className="py-10 text-center text-sm text-muted-foreground">پورت شنونده‌ای شناسایی نشد.</div>
  )
}

type TrafficBucket = { at: number; rx_bytes: number; tx_bytes: number; interfaces: number }
type TrafficVolumeResponse = { from: number; to: number; granularity: 'hour' | 'day'; interface: string; interfaces: string[]; buckets: TrafficBucket[]; fresh_at: number | null; enabled: boolean; timezone: string }

function NetworkTrafficVolume({ availableInterfaces }: { availableInterfaces: string[] }) {
  const timeZone = usePanelTimeZone()
  const [range, setRange] = useState<'24h' | '7d' | '30d' | 'custom'>('24h')
  const [granularity, setGranularity] = useState<'hour' | 'day'>('hour')
  const [interfaceName, setInterfaceName] = useState('all')
  const [fromInput, setFromInput] = useState('')
  const [toInput, setToInput] = useState('')
  const [data, setData] = useState<TrafficVolumeResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [refreshKey, setRefreshKey] = useState(0)
  const [selectedBucketIndex, setSelectedBucketIndex] = useState<number | null>(null)
  const fromEpoch = fromInput ? Math.floor(new Date(fromInput).getTime() / 1000) : null
  const toEpoch = toInput ? Math.floor(new Date(toInput).getTime() / 1000) : null
  useEffect(() => {
    if (range === 'custom' && (!fromEpoch || !toEpoch || fromEpoch >= toEpoch)) return
    const query = new URLSearchParams({ range, granularity, interface: interfaceName })
    if (range === 'custom' && fromEpoch && toEpoch) { query.set('from', String(fromEpoch)); query.set('to', String(toEpoch)) }
    let canceled = false
    setLoading(true)
    api<TrafficVolumeResponse>(`network/traffic?${query.toString()}`).then((result) => {
      if (!canceled) { setData(result); setSelectedBucketIndex(null); setError('') }
    }).catch((cause) => { if (!canceled) setError(cause instanceof Error ? cause.message : 'خواندن ترافیک حجمی ناموفق بود.') })
      .finally(() => { if (!canceled) setLoading(false) })
    return () => { canceled = true }
  }, [range, granularity, interfaceName, fromEpoch, toEpoch, refreshKey])
  const buckets = data?.buckets || []
  const totalRx = buckets.reduce((sum, item) => sum + item.rx_bytes, 0)
  const totalTx = buckets.reduce((sum, item) => sum + item.tx_bytes, 0)
  const maxVolume = Math.max(1, ...buckets.map((item) => Math.max(item.rx_bytes, item.tx_bytes)))
  const labelTime = (seconds: number) => new Intl.DateTimeFormat('en-US', { timeZone, ...(granularity === 'hour' ? { month: 'short', day: 'numeric', hour: '2-digit', hour12: false } : { month: 'short', day: 'numeric' }) }).format(new Date(seconds * 1000))
  const bucketTickStride = Math.max(1, Math.ceil(buckets.length / 5))
  const chartMinWidth = Math.max(640, Math.min(6000, buckets.length * (buckets.length > 180 ? 6 : 20)))
  const selectedBucket = selectedBucketIndex === null ? null : buckets[selectedBucketIndex]
  return <SectionCard title="حجم ترافیک منتقل‌شده" description="مجموع بایت‌های ثبت‌شده از اختلاف شمارنده‌های کرنل؛ نمودار سرعت جداگانه پایین صفحه است." right={<Button size="icon" variant="ghost" aria-label="تازه‌سازی حجم ترافیک" onClick={() => setRefreshKey((value) => value + 1)}><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/></Button>}>
    <div className="flex flex-wrap items-center justify-between gap-3">
      <div className="arg-range flex flex-wrap gap-1 rounded-lg border border-border bg-background/45 p-1">{(['24h','7d','30d','custom'] as const).map((item) => <button key={item} type="button" aria-pressed={range === item} onClick={() => { setRange(item); if (item === '24h') setGranularity('hour'); else if (item !== 'custom') setGranularity('day') }} className="rounded-md px-2.5 py-1.5 text-xs transition-colors">{{'24h':'24 ساعت','7d':'7 روز','30d':'30 روز','custom':'دلخواه'}[item]}</button>)}</div>
      <div className="flex flex-wrap items-center gap-2"><label className="text-xs text-muted-foreground">رابط<select value={interfaceName} onChange={(event) => setInterfaceName(event.target.value)} className="mr-2 rounded-lg border border-border bg-background px-2.5 py-1.5 text-xs text-foreground"><option value="all">رابط‌های اصلی</option>{availableInterfaces.map((name) => <option key={name} value={name}>{name}</option>)}</select></label><div className="flex rounded-lg border border-border bg-background/45 p-1"><button type="button" aria-pressed={granularity === 'hour'} onClick={() => setGranularity('hour')} disabled={range !== '24h' && range !== 'custom'} className="rounded-md px-2 py-1 text-[11px] text-muted-foreground aria-pressed:text-primary disabled:opacity-40">ساعتی</button><button type="button" aria-pressed={granularity === 'day'} onClick={() => setGranularity('day')} className="rounded-md px-2 py-1 text-[11px] text-muted-foreground aria-pressed:text-primary">روزانه</button></div></div>
    </div>
    {range === 'custom' && <div className="mt-3 flex flex-wrap items-end gap-3 rounded-xl border border-border/70 bg-background/30 p-3"><label className="space-y-1 text-xs text-muted-foreground">از (زمان محلی مرورگر)<Input type="datetime-local" value={fromInput} onChange={(event) => setFromInput(event.target.value)} className="h-9 min-w-48"/></label><label className="space-y-1 text-xs text-muted-foreground">تا<Input type="datetime-local" value={toInput} onChange={(event) => setToInput(event.target.value)} className="h-9 min-w-48"/></label><Button size="sm" disabled={!fromEpoch || !toEpoch || fromEpoch >= toEpoch} onClick={() => setRefreshKey((value) => value + 1)}>اعمال بازه</Button><span className="text-[10px] text-muted-foreground">گروه‌بندی ساعتی حداکثر تا 90 روز پشتیبانی می‌شود.</span></div>}
    <div className="mt-4 grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-border bg-background/40 p-3"><span className="text-xs text-muted-foreground">دریافت</span><strong dir="ltr" className="mt-2 block font-mono text-lg text-foreground">{formatBytes(totalRx)}</strong></div><div className="rounded-xl border border-border bg-background/40 p-3"><span className="text-xs text-muted-foreground">ارسال</span><strong dir="ltr" className="mt-2 block font-mono text-lg text-foreground">{formatBytes(totalTx)}</strong></div><div className="rounded-xl border border-border bg-background/40 p-3"><span className="text-xs text-muted-foreground">مجموع این بازه</span><strong dir="ltr" className="mt-2 block font-mono text-lg text-primary">{formatBytes(totalRx + totalTx)}</strong></div></div>
    <div className="mt-4 flex flex-wrap items-center gap-4 text-[11px] text-muted-foreground"><span className="flex items-center gap-2"><i className="h-2 w-2 rounded-sm bg-primary"/>دریافت RX</span><span className="flex items-center gap-2"><i className="h-2 w-2 rounded-sm bg-emerald-300"/>ارسال TX</span><span className="mr-auto" dir="ltr">{timeZone} · {data?.fresh_at ? `آخرین نمونه ${labelTime(data.fresh_at)}` : 'بدون نمونه'}</span></div>
    {error ? <Alert className="mt-3 border-rose-400/20 bg-rose-400/[.05]"><AlertDescription>{error}</AlertDescription></Alert> : loading && !data ? <Skeleton className="mt-4 h-56"/> : !data?.enabled ? <div className="mt-4 rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">ثبت تاریخچه در تنظیمات خاموش است؛ برای جمع‌آوری حجم ترافیک، ثبت تاریخچه را روشن کن.</div> : !buckets.length ? <div className="mt-4 rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">برای این بازه داده‌ای ثبت نشده است. حجم از زمان شروع مشاهدهٔ شمارنده‌ها جمع می‌شود و تاریخچهٔ قدیمی ساخته نمی‌شود.</div> : <div className="mt-4 overflow-x-auto rounded-xl border border-border/70 bg-background/25 p-4">{selectedBucket && <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-primary/15 bg-primary/[.04] px-3 py-2 text-xs"><span className="text-muted-foreground">{granularity === 'hour' ? 'بازهٔ ساعتی' : 'روز ثبت'}</span><strong dir="ltr" className="font-mono text-foreground">{labelTime(selectedBucket.at)} · {timeZone}</strong><span dir="ltr" className="font-mono text-muted-foreground">RX {formatBytes(selectedBucket.rx_bytes)} · TX {formatBytes(selectedBucket.tx_bytes)}</span></div>}<div className="flex h-52 items-end gap-0 border-b border-border/80 pb-2" style={{ minWidth: chartMinWidth }} aria-label="نمودار حجم ترافیک به تفکیک زمان">{buckets.map((item, index) => { const tick = index === 0 || index === buckets.length - 1 || index % bucketTickStride === 0; const selected = selectedBucketIndex === index; return <button key={item.at} type="button" title={`${labelTime(item.at)} ${timeZone}\nRX ${formatBytes(item.rx_bytes)} · TX ${formatBytes(item.tx_bytes)}`} aria-label={`${granularity === 'hour' ? 'بازهٔ ساعتی' : 'روز'} ${labelTime(item.at)}، دریافت ${formatBytes(item.rx_bytes)}، ارسال ${formatBytes(item.tx_bytes)}`} onClick={() => setSelectedBucketIndex(index)} onMouseEnter={() => setSelectedBucketIndex(index)} onFocus={() => setSelectedBucketIndex(index)} className={`group flex h-full min-w-0 flex-1 flex-col justify-end rounded-t-sm outline-none focus-visible:ring-1 focus-visible:ring-primary ${selected ? 'bg-white/[.035]' : ''}`}><span className="flex h-[calc(100%-24px)] items-end justify-center" style={{ gap: buckets.length > 180 ? 0 : 4 }}><span className="w-[42%] rounded-t-sm bg-primary/90 transition-[height,opacity] duration-300 group-hover:opacity-100" style={{ height: `${Math.max(item.rx_bytes ? 3 : 0, item.rx_bytes / maxVolume * 100)}%`, opacity: selected ? 1 : .82 }}/><span className="w-[42%] rounded-t-sm bg-emerald-300/85 transition-[height,opacity] duration-300 group-hover:opacity-100" style={{ height: `${Math.max(item.tx_bytes ? 3 : 0, item.tx_bytes / maxVolume * 100)}%`, opacity: selected ? 1 : .78 }}/></span><span className="mt-2 h-5 overflow-visible text-center font-mono text-[9px] text-muted-foreground" dir="ltr">{tick && <span className="block whitespace-nowrap">{labelTime(item.at)}</span>}</span></button>})}</div><div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-[10px] text-muted-foreground"><span>هر ستون حجم ثبت‌شده در بازه است؛ برای زمان و مقدار دقیق روی ستون بزن.</span><span dir="ltr">{formatNumber(buckets.length)} buckets · {granularity}</span></div></div>}
    <p className="mt-3 text-[10px] leading-5 text-muted-foreground">محاسبه از اختلاف شمارندهٔ رابط فیزیکی و فقط برای نمونه‌های پیوسته انجام می‌شود؛ پس از راه‌اندازی مجدد یا وقفهٔ طولانی، شکاف داده به‌عنوان مصرف تخمینی پر نمی‌شود. این عدد با صورتحساب حجمی ارائه‌دهندهٔ VPS لزوماً برابر نیست.</p>
  </SectionCard>
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
        action={<RefreshButton onRefresh={refresh}/>}
      />
      {loading && !data ? (
        <LoadingCards />
      ) : (
        <>
          <div className="mb-5"><NetworkTrafficVolume availableInterfaces={interfaces.map((item) => item.name)}/></div>
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
  const [protectedPorts, setProtectedPorts] = useState<number[]>([22, 8282, 8443])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState('')
  const [busy, setBusy] = useState(false)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editing, setEditing] = useState<number | null>(null)
  const [form, setForm] = useState<{ port: string; protocol: 'tcp' | 'udp' | 'both'; policy: 'allow' | 'deny'; enabled: boolean }>({ port: '', protocol: 'tcp', policy: 'allow', enabled: true })
  const [query, setQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<'all' | 'enabled' | 'disabled'>('all')
  const refresh = useCallback(async () => {
    try {
      const [firewall, managed] = await Promise.allSettled([api<Firewall>('firewall'), api<ManagedFirewallRulesResponse>('firewall/rules')])
      if (firewall.status === 'fulfilled') { setData(firewall.value); setLoadError('') }
      else { setLoadError(firewall.reason instanceof Error ? firewall.reason.message : 'وضعیت UFW دریافت نشد.') }
      if (managed.status === 'fulfilled') { setRules(managed.value.rules); setProtectedPorts(managed.value.protected_ports || [22, 8282, 8443]) }
      else notify(managed.reason instanceof Error ? managed.reason.message : 'دریافت قوانین ناموفق بود')
    }
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
  async function setWebsitePort(port: 80 | 443, enabled: boolean) {
    setBusy(true)
    try {
      const existing = rules.find((rule) => rule.port === port && rule.protocol === 'tcp')
      const body = { port, protocol: 'tcp', policy: enabled ? 'allow' : 'deny', enabled: true }
      if (existing) await api(`firewall/rules/${existing.id}`, { method: 'PUT', body: JSON.stringify(body) })
      else if (enabled) await api('firewall/rules', { method: 'POST', body: JSON.stringify(body) })
      notify(enabled ? `دسترسی ورودی TCP پورت ${port} وصل شد.` : `دسترسی ورودی TCP پورت ${port} قطع شد.`)
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : `تغییر دسترسی پورت ${port} ناموفق بود`) }
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
  if (loading) return <><Heading title="فایروال UFW" description="قوانین پنل را بساز، ویرایش کن یا موقتاً غیرفعال کن."/><FirewallPageSkeleton/></>
  if (loadError && !data) return <><Heading title="فایروال UFW" description="وضعیت فایروال از سرور دریافت نشد." action={<RefreshButton onRefresh={refresh}/>}/><Alert className="border-amber-400/25 bg-amber-400/[.06]"><AlertTitle>دریافت وضعیت UFW ناموفق بود</AlertTitle><AlertDescription>{loadError}</AlertDescription></Alert></>
  return <>
    <Heading title="فایروال UFW" description="قوانین پنل را بساز، ویرایش کن یا موقتاً غیرفعال کن." action={<div className="flex flex-wrap gap-2"><RefreshButton onRefresh={refresh}/>{user.role === 'admin' && <Button size="sm" onClick={() => openEditor()}><Plus size={15}/> قانون جدید</Button>}</div>}/>
    <div className="mb-5 grid gap-3 sm:grid-cols-3"><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">وضعیت UFW</div><div className="mt-3 flex items-center gap-2 text-xl font-bold text-white"><span className={`h-2 w-2 rounded-full ${active ? 'bg-emerald-400' : 'bg-amber-400'}`}/>{loading ? 'در حال بررسی' : data?.available ? active ? 'فعال' : 'غیرفعال' : 'نصب نشده'}</div>{user.role === 'admin' && data?.available && <div className="mt-4 flex items-center gap-2 border-t border-white/10 pt-3 text-xs text-muted-foreground"><Switch checked={!!active} disabled={busy} onCheckedChange={() => void toggleFirewall()} aria-label="تغییر وضعیت UFW"/> تغییر وضعیت کلی UFW</div>}{user.role === 'admin' && !data?.available && <div className="mt-4 border-t border-white/10 pt-3"><Button size="sm" disabled={busy} onClick={() => void installFirewall()}>{busy ? 'در حال نصب…' : 'نصب UFW'}</Button><p className="mt-2 text-[11px] leading-5 text-amber-200/80">{data?.error || 'UFW روی این سرور نصب نشده است.'}</p></div>}</div><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">قوانین فعال پنل</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => rule.enabled).length)}</div></div><div className="rounded-[14px] border border-white/10 bg-card p-5"><div className="text-xs text-muted-foreground">قوانین غیرفعال</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => !rule.enabled).length)}</div></div></div>
    <SectionCard title="پورت‌های باز و محافظت‌شده" description="پورت‌های حیاتی همیشه در فهرست دیده می‌شوند و تغییرشان قفل است؛ دسترسی وب روی 80 و 443 را می‌توانی روشن یا خاموش کنی."><div className="grid gap-3 sm:grid-cols-2">{protectedPorts.map((port) => <div key={`protected-${port}`} className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-emerald-400/15 bg-emerald-400/[.035] p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-lg border border-emerald-400/15 bg-emerald-400/[.06] text-emerald-300"><ShieldCheck size={18}/></span><div><div className="flex items-center gap-2 font-semibold">دسترسی محافظت‌شده <span className="font-mono text-xs text-muted-foreground" dir="ltr">:{port}/tcp</span></div><p className="mt-1 text-xs text-muted-foreground">برای جلوگیری از قطع دسترسی SSH یا پنل، قابل تغییر نیست.</p></div></div><div className="flex items-center gap-2"><Switch checked disabled aria-label={`پورت محافظت‌شده ${port} همیشه باز است`}/><Badge variant="success">محافظت‌شده</Badge></div></div>)}{([{ port: 80 as const, name: 'HTTP', description: 'وب‌سایت‌های بدون TLS' }, { port: 443 as const, name: 'HTTPS', description: 'وب‌سایت‌های دارای TLS' }]).map(({ port, name, description }) => { const rule = rules.find((item) => item.port === port && item.protocol === 'tcp'); const allowed = !!rule && rule.enabled && rule.policy === 'allow'; return <div key={port} className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-background/40 p-4"><div className="flex items-center gap-3"><span className="grid h-10 w-10 place-items-center rounded-lg border border-primary/20 bg-primary/10 text-primary"><Globe size={18}/></span><div><div className="flex items-center gap-2 font-semibold">{name}<span className="font-mono text-xs text-muted-foreground" dir="ltr">:{port}/tcp</span></div><p className="mt-1 text-xs text-muted-foreground">{description}</p></div></div><div className="flex items-center gap-2"><span className="text-xs text-muted-foreground">{allowed ? 'وصل' : 'قطع'}</span><Switch checked={allowed} disabled={busy || !data?.available || user.role !== 'admin'} onCheckedChange={(checked) => void setWebsitePort(port, checked)} aria-label={`${allowed ? 'قطع' : 'وصل'} دسترسی ${name} روی پورت ${port}`}/></div></div> })}</div>{!data?.available && <p className="mt-3 text-xs text-muted-foreground">برای تغییر دسترسی پورت‌ها ابتدا UFW را نصب کن.</p>}</SectionCard>
    <SectionCard title="قوانین مدیریت‌شده" description="تغییر وضعیت و ویرایش فقط روی قوانین ساخته‌شده توسط پنل اعمال می‌شود" right={<Badge variant="outline">{formatNumber(filtered.length)} قانون</Badge>}>
      <div className="mb-5 flex flex-wrap items-center gap-3"><label className="relative min-w-[180px] flex-1"><span className="sr-only">جست‌وجوی پورت</span><Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input lang="en" inputMode="numeric" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="جست‌وجوی شمارهٔ پورت" className="pr-9"/></label><div className="flex gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['all','همه'], ['enabled','فعال'], ['disabled','غیرفعال']] as const).map(([value, label]) => <button key={value} onClick={() => setStatusFilter(value)} aria-pressed={statusFilter === value} className={`rounded-lg px-3 py-2 text-xs transition ${statusFilter === value ? 'bg-primary/20 text-white' : 'text-muted-foreground hover:text-white'}`}>{label}</button>)}</div></div>
      {loading ? <Skeleton className="h-48"/> : filtered.length ? <div className="overflow-x-auto"><Table><TableHeader><TableRow><TableHead>پورت</TableHead><TableHead>پروتکل</TableHead><TableHead>رفتار</TableHead><TableHead>وضعیت</TableHead>{user.role === 'admin' && <TableHead>مدیریت</TableHead>}</TableRow></TableHeader><TableBody>{filtered.map((rule) => <TableRow key={rule.id}><TableCell className="font-mono text-white" dir="ltr">{rule.port}</TableCell><TableCell className="font-mono uppercase">{rule.protocol}</TableCell><TableCell><Badge variant={rule.policy === 'allow' ? 'success' : 'warning'}>{rule.policy === 'allow' ? 'اجازه' : 'مسدود'}</Badge></TableCell><TableCell>{user.role === 'admin' ? <div className="flex items-center gap-2"><Switch checked={rule.enabled} disabled={busy} onCheckedChange={() => void toggle(rule)} aria-label={`${rule.enabled ? 'غیرفعال‌کردن' : 'فعال‌کردن'} قانون ${rule.port}`}/><span className="text-xs text-muted-foreground">{rule.enabled ? 'فعال' : 'غیرفعال'}</span></div> : <Badge variant={rule.enabled ? 'success' : 'outline'}>{rule.enabled ? 'فعال' : 'غیرفعال'}</Badge>}</TableCell>{user.role === 'admin' && <TableCell><div className="flex gap-1"><Button variant="ghost" size="icon" disabled={busy} onClick={() => openEditor(rule)} aria-label={`ویرایش قانون ${rule.port}`}><Pencil size={15}/></Button><Button variant="ghost" size="icon" disabled={busy} onClick={() => void remove(rule)} aria-label={`حذف قانون ${rule.port}`}><Trash2 size={15}/></Button></div></TableCell>}</TableRow>)}</TableBody></Table></div> : <div className="rounded-xl border border-dashed border-white/10 py-12 text-center text-sm text-muted-foreground">{query || statusFilter !== 'all' ? 'قانونی مطابق فیلتر پیدا نشد.' : 'هنوز قانونی در پنل ساخته نشده است.'}</div>}
    </SectionCard>
    <details className="mt-5 rounded-[14px] border border-white/10 bg-card p-5"><summary className="cursor-pointer text-sm font-semibold text-white">همهٔ قوانین UFW و خروجی خام</summary><div className="mt-4 flex justify-end"><Button variant="outline" size="sm" disabled={!data?.status} onClick={() => downloadText('arg-ufw.txt', data?.status || '')}><Download size={14}/> دریافت خروجی</Button></div><pre className="mt-3 max-h-[480px] overflow-auto whitespace-pre-wrap rounded-xl bg-background p-4 text-left font-mono text-xs leading-6 text-muted-foreground" dir="ltr">{data?.status || data?.error || 'UFW unavailable'}</pre></details>
    <Alert className="mt-5"><ShieldCheck size={17} className="mb-2 text-primary"/><AlertTitle>حفاظت دسترسی</AlertTitle><AlertDescription>پورت SSH و پورت‌های پنل برای جلوگیری از قطع دسترسی محافظت می‌شوند. پورت‌های HTTP (80) و HTTPS (443) قابل مدیریت‌اند؛ فعال‌سازی UFW آن‌ها را باز می‌گذارد مگر این‌که برایشان قانون مسدودسازی مشخص کرده باشی.</AlertDescription></Alert>
    <Dialog open={dialogOpen} onOpenChange={setDialogOpen}><DialogContent><DialogHeader><DialogTitle>{editing === null ? 'قانون جدید' : 'ویرایش قانون'}</DialogTitle><DialogDescription>قانون در UFW اعمال می‌شود و می‌توانی بعداً آن را غیرفعال کنی.</DialogDescription></DialogHeader><form onSubmit={save} className="space-y-4"><div className="grid grid-cols-2 gap-3"><div className="space-y-2"><Label htmlFor="firewall-port">پورت</Label><Input id="firewall-port" type="number" lang="en" dir="ltr" min={1} max={65535} required value={form.port} onChange={(event) => setForm((old) => ({ ...old, port: event.target.value }))}/></div><div className="space-y-2"><Label>پروتکل</Label><div role="group" aria-label="انتخاب پروتکل شبکه" className="grid grid-cols-3 gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['tcp','TCP'], ['udp','UDP'], ['both','TCP + UDP']] as const).map(([value,label]) => <button key={value} type="button" aria-pressed={form.protocol === value} onClick={() => setForm((old) => ({ ...old, protocol: value }))} className={`rounded-lg px-2 py-2.5 text-xs font-mono font-semibold transition ${form.protocol === value ? 'bg-primary/25 text-white shadow-sm' : 'text-muted-foreground hover:bg-white/[.04] hover:text-white'}`}>{label}</button>)}</div><p className="text-[11px] text-muted-foreground">{form.protocol === 'both' ? 'دو قانون جداگانه برای TCP و UDP ساخته می‌شود.' : `فقط ترافیک ${form.protocol.toUpperCase()} برای این پورت مدیریت می‌شود.`}</p></div></div><div className="space-y-2"><Label htmlFor="firewall-policy">رفتار</Label><Select value={form.policy} onValueChange={(value) => setForm((old) => ({ ...old, policy: value as 'allow' | 'deny' }))}><SelectTrigger id="firewall-policy"><SelectValue/></SelectTrigger><SelectContent><SelectItem value="allow">اجازه</SelectItem><SelectItem value="deny">مسدود</SelectItem></SelectContent></Select></div><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span>قانون فعال باشد</span><Switch checked={form.enabled} onCheckedChange={(checked) => setForm((old) => ({ ...old, enabled: checked }))}/></label><Button type="submit" disabled={busy || !form.port} className="w-full">{busy ? 'در حال ذخیره…' : 'ذخیرهٔ قانون'}</Button></form></DialogContent></Dialog>
  </>
}

type NginxSite = { name: string; enabled: boolean; bytes?: number; content?: string; db_content?: string; message?: string; description?: string; sync_mismatch?: boolean }

function NginxMaintenancePanel({ siteName, notify }: { siteName: string; notify: (message: string) => void }) {
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState<{ enabled: boolean; mode?: string; file?: string }>({ enabled: false })
  const [mode, setMode] = useState<'project-root' | 'static-file' | 'arg-default'>('arg-default')
  const [file, setFile] = useState('maintenance.html')
  const endpoint = `nginx/sites/${encodeURIComponent(siteName)}/maintenance`
  async function refresh() {
    setLoading(true)
    try { const result = await api<{ enabled: boolean; mode?: string; file?: string }>(endpoint); setStatus(result); if (result.mode === 'project-root' || result.mode === 'static-file' || result.mode === 'arg-default') setMode(result.mode); if (result.file) setFile(result.file) }
    catch (error) { notify(error instanceof Error ? error.message : 'وضعیت تعمیرات دریافت نشد.') }
    finally { setLoading(false) }
  }
  useEffect(() => { if (open) void refresh() }, [open, siteName])
  async function change(enabled: boolean, force = false) {
    if (force && !window.confirm('حالت Force سرویس Nginx را فوراً راه‌اندازی مجدد می‌کند و ممکن است اتصال‌های جاری را قطع کند. ادامه می‌دهی؟')) return
    setBusy(true)
    try {
      const result = await api<{ enabled: boolean; mode?: string; file?: string; message: string }>(endpoint, { method: 'PUT', body: JSON.stringify({ enabled, mode, file, force }) })
      setStatus({ enabled: result.enabled, mode: result.mode, file: result.file })
      notify(result.message)
    } catch (error) { notify(error instanceof Error ? error.message : 'تغییر حالت تعمیرات ناموفق بود.') }
    finally { setBusy(false) }
  }
  return <div className="mt-3 border-t border-white/[.07] pt-3">
    <button type="button" aria-expanded={open} onClick={() => setOpen((value) => !value)} className="flex min-h-10 w-full items-center justify-between gap-3 rounded-lg px-2 text-right text-xs font-medium text-muted-foreground transition hover:bg-white/[.035] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><span className="flex items-center gap-2"><Activity size={14} className="text-primary"/>حالت تعمیرات سایت <span className={status.enabled ? 'text-amber-300' : 'text-emerald-300'}>{status.enabled ? 'فعال' : 'خاموش'}</span></span><ChevronDown size={15} className={`transition-transform duration-200 ${open ? 'rotate-180' : ''}`}/></button>
    {open && <div className="mt-2 rounded-xl border border-white/10 bg-background/40 p-3 sm:p-4">
      {loading ? <Skeleton className="h-20"/> : <>
        <p className="mb-3 text-xs leading-5 text-muted-foreground">در حالت عادی Nginx با reload نرم وارد تعمیرات می‌شود؛ درخواست‌های جاری HTTP فرصت پایان دارند و درخواست‌های تازه صفحهٔ انتخابی را می‌بینند. این کار پردازش‌های پس‌زمینهٔ خود برنامه را متوقف یا تخلیه نمی‌کند.</p>
        <div className="grid gap-2 sm:grid-cols-3" role="radiogroup" aria-label="روش نمایش صفحهٔ تعمیرات">{([['arg-default','صفحهٔ پیش‌فرض ARG'],['project-root','فایل داخل ریشهٔ پروژه'],['static-file','فایل استاتیک'] ] as const).map(([value,label]) => <button key={value} type="button" role="radio" aria-checked={mode===value} onClick={() => setMode(value)} className={`rounded-lg border px-3 py-2.5 text-xs transition ${mode===value ? 'border-primary/50 bg-primary/10 text-white' : 'border-white/10 text-muted-foreground hover:text-white'}`}>{label}</button>)}</div>
        {mode !== 'arg-default' && <div className="mt-3 space-y-1.5"><label htmlFor={`maintenance-file-${siteName}`} className="text-xs text-muted-foreground">{mode === 'project-root' ? 'مسیر نسبی داخل ریشهٔ سایت' : 'مسیر مطلق فایل استاتیک'}</label><Input id={`maintenance-file-${siteName}`} dir="ltr" value={file} onChange={(event) => setFile(event.target.value)} placeholder={mode === 'project-root' ? 'maintenance.html' : '/var/www/example/maintenance.html'}/><p className="text-[10px] leading-5 text-muted-foreground">{mode === 'project-root' ? 'فایل HTML باید داخل ریشهٔ تعریف‌شدهٔ سایت و حداکثر 2 MB باشد.' : 'فقط فایل‌های HTML زیر /var/www، /srv/www یا /opt/sites پذیرفته می‌شوند.'}</p></div>}
        {status.enabled && <p className="mt-3 rounded-lg border border-amber-400/20 bg-amber-400/[.05] p-2 text-xs text-amber-100">سایت اکنون در حالت تعمیرات است. {status.mode === 'project-root' ? 'فایل پروژه' : status.mode === 'static-file' ? 'فایل استاتیک' : 'صفحهٔ پیش‌فرض ARG'} نمایش داده می‌شود.</p>}
        <div className="mt-3 flex flex-wrap gap-2">{status.enabled ? <Button size="sm" variant="outline" disabled={busy} onClick={() => void change(false)}>{busy ? 'در حال خروج…' : 'پایان تعمیرات'}</Button> : <><Button size="sm" disabled={busy || (mode !== 'arg-default' && !file.trim())} onClick={() => void change(true)}>{busy ? 'در حال اعمال…' : 'ورود نرم به تعمیرات'}</Button><Button size="sm" variant="destructive" disabled={busy || (mode !== 'arg-default' && !file.trim())} onClick={() => void change(true, true)}>{busy ? 'در حال اعمال…' : 'Force mode'}</Button></>}</div>
        {!status.enabled && <p className="mt-2 text-[10px] leading-5 text-muted-foreground">Force فوراً Nginx را راه‌اندازی مجدد می‌کند و ممکن است اتصال‌های جاری را قطع کند.</p>}
      </>}
    </div>}
  </div>
}
type NginxStatus = { installed: boolean; active: boolean; enabled: boolean; version: string; config_test: { ok: boolean; output: string } }
type NginxPreset = 'static' | 'proxy' | 'php' | 'laravel' | 'symfony' | 'wordpress' | 'node' | 'python' | 'redirect'
type NginxBuilder = { preset: NginxPreset; domain: string; listen: string; root: string; index: string; proxyPass: string; phpSocket: string; redirectTo: string; ssl: boolean; certificate: string; certificateKey: string; gzip: boolean; maxBody: string; accessLog: string; errorLog: string; clientTimeout: string; proxyTimeout: string; proxyBuffering: boolean; limitRate: string; limitRateAfter: string; serverTokens: boolean; securityHeaders: boolean }
function applyNginxPreset(builder: NginxBuilder, preset: NginxPreset): NginxBuilder {
  const root = builder.root.replace(/\/$/, '')
  const publicRoot = ['laravel', 'symfony'].includes(preset) && !root.endsWith('/public') ? `${root}/public` : builder.root
  return {
    ...builder,
    preset,
    root: publicRoot,
    index: ['laravel', 'symfony', 'wordpress', 'php'].includes(preset) ? 'index.php' : builder.index,
    maxBody: preset === 'laravel' ? '10m' : builder.maxBody,
    securityHeaders: ['laravel', 'symfony', 'wordpress'].includes(preset) ? true : builder.securityHeaders,
  }
}
type NginxFileEntry = { name: string; path: string; directory: boolean; selectable: boolean }
type NginxFileListing = { path: string; roots: string[]; entries: NginxFileEntry[] }
type NginxDomainResult = { host: string; resolved: boolean; addresses: string[]; error?: string }
type NginxTraffic = { available: boolean; message?: string; path?: string; sample_requests?: number; sample_body_bytes?: number; status_groups?: number[]; sample_limit?: number; hourly?: Array<{ hour: string; requests: number; body_bytes: number }> }
type NginxRepository = { available: boolean; message?: string; path?: string; branch?: string; commit?: string; changes?: number; origin?: string; remote_commit?: string; differs_from_remote?: boolean }
function NginxHelp({ children }: { children: ReactNode }) {
  return <TooltipProvider delayDuration={200}><Tooltip><TooltipTrigger asChild><span role="img" tabIndex={0} aria-label="راهنمای این تنظیم" className="inline-grid h-5 w-5 shrink-0 cursor-help place-items-center rounded-full text-muted-foreground hover:bg-white/10 hover:text-white"><Info size={13}/></span></TooltipTrigger><TooltipContent side="top" className="max-w-64 text-right leading-5">{children}</TooltipContent></Tooltip></TooltipProvider>
}
export function NginxPage({ Heading, notify }: Common) {
  const [sites, setSites] = useState<NginxSite[]>([])
  const [status, setStatus] = useState<NginxStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [loadError, setLoadError] = useState('')
  const [statusError, setStatusError] = useState('')
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
  const [expandedRuntimeSite, setExpandedRuntimeSite] = useState<string | null>(null)
  const [toolSite, setToolSite] = useState<NginxSite | null>(null)
  const [toolConfig, setToolConfig] = useState('')
  const [domainPrimary, setDomainPrimary] = useState('')
  const [domainAliases, setDomainAliases] = useState('')
  const [domainResults, setDomainResults] = useState<NginxDomainResult[]>([])
  const [domainChecking, setDomainChecking] = useState(false)
  const [sslEmail, setSslEmail] = useState('')
  const [sslDomains, setSslDomains] = useState('')
  const [sslRedirect, setSslRedirect] = useState(true)
  const [sslIssueBusy, setSslIssueBusy] = useState(false)
  const [siteRepository, setSiteRepository] = useState<NginxRepository | null>(null)
  const [repositoryLoading, setRepositoryLoading] = useState(false)
  const [siteTraffic, setSiteTraffic] = useState<NginxTraffic | null>(null)
  const [trafficLoading, setTrafficLoading] = useState(false)
  const [siteLog, setSiteLog] = useState('')
  const [siteLogPath, setSiteLogPath] = useState('')
  const [siteLogKind, setSiteLogKind] = useState<'access' | 'error' | null>(null)
  const [siteLogLoading, setSiteLogLoading] = useState(false)
  const [duplicateNotice, setDuplicateNotice] = useState(false)
  const [builderOpen, setBuilderOpen] = useState(false)
  const [builderTarget, setBuilderTarget] = useState<'create' | 'edit'>('create')
  const [projectDetection, setProjectDetection] = useState<{ label: string; preset: NginxPreset } | null>(null)
  const [detectingProject, setDetectingProject] = useState(false)
  const [builder, setBuilder] = useState<NginxBuilder>({ preset: 'static', domain: 'example.com', listen: '80', root: '/var/www/example.com', index: 'index.html', proxyPass: 'http://127.0.0.1:3000', phpSocket: 'unix:/run/php/php8.2-fpm.sock', redirectTo: 'https://example.com$request_uri', ssl: false, certificate: '', certificateKey: '', gzip: true, maxBody: '32m', accessLog: '', errorLog: '', clientTimeout: '60s', proxyTimeout: '60s', proxyBuffering: true, limitRate: '', limitRateAfter: '', serverTokens: false, securityHeaders: false })
  const [extraDirectives, setExtraDirectives] = useState<Array<{ name: string; value: string }>>([])
  const [extraLocations, setExtraLocations] = useState<Array<{ path: string; config: string }>>([])
  const [filePickerOpen, setFilePickerOpen] = useState(false)
  const [filePickerFor, setFilePickerFor] = useState<'certificate' | 'certificateKey'>('certificate')
  const [fileListing, setFileListing] = useState<NginxFileListing | null>(null)
  const [fileLoading, setFileLoading] = useState(false)
  const [visualSettings, setVisualSettings] = useState<Record<string, unknown> | null>(null)
  const [reconcileSite, setReconcileSite] = useState<{ name: string; dbContent: string; fileContent: string } | null>(null)
  const [reconcileView, setReconcileView] = useState<'database' | 'file'>('file')
  const [databaseRestorePending, setDatabaseRestorePending] = useState(false)
  const [confirmDatabaseRestore, setConfirmDatabaseRestore] = useState(false)
  const [sectionCode, setSectionCode] = useState({ http: '', server: '', location: '', tls: '' })

  const refresh = useCallback(async (syncedSiteName?: string) => {
    const [serviceResult, sitesResult] = await Promise.allSettled([
      api<NginxStatus>('nginx/status'),
      api<{ sites: NginxSite[] }>('nginx/sites'),
    ])
    if (serviceResult.status === 'fulfilled') { setStatus(serviceResult.value); setStatusError('') }
    else setStatusError(serviceResult.reason instanceof Error ? serviceResult.reason.message : 'وضعیت سرویس Nginx دریافت نشد.')
    if (sitesResult.status === 'fulfilled') {
      setSites(sitesResult.value.sites)
      const drift = sitesResult.value.sites.find((site) => site.sync_mismatch && site.name !== syncedSiteName)
      if (drift) setReconcileSite({ name: drift.name, dbContent: drift.db_content || '', fileContent: drift.content || '' })
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
      const result = await api<{ message?: string }>('nginx/sites', { method: 'POST', body: JSON.stringify({ name: newName.trim(), content: newContent, settings: visualSettings }) })
      setCreateOpen(false); setNewName(''); setDuplicateNotice(false); notify(result.message || 'فایل سایت ساخته شد.'); await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ساخت سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  function openBuilder(target: 'create' | 'edit') {
    setBuilderTarget(target)
    if (target === 'create') {
      const initialName = newName.trim() || 'example.com'
      setProjectDetection(null)
      setBuilder((old) => ({ ...old, preset: 'static', domain: initialName, root: `/var/www/${initialName.split(/\s+/)[0]}` }))
      setDuplicateNotice(false)
    } else {
      const directive = (key: string) => content.match(new RegExp(`^\\s*${key}\\s+([^;]+);`, 'm'))?.[1]?.trim()
      const domain = directive('server_name') || editing || 'example.com'
      const listens = [...content.matchAll(/^\s*listen\s+([^;]+);/gm)].map((match) => match[1].trim())
      const proxy = directive('proxy_pass')
      const php = directive('fastcgi_pass')
      const redirect = content.match(/^\s*return\s+301\s+([^;]+);/m)?.[1]?.trim()
      setProjectDetection(null)
      setBuilder((old) => ({ ...old, preset: php && /location\s+=\s+\/index\.php/.test(content) ? 'laravel' : php ? 'php' : proxy ? 'proxy' : redirect ? 'redirect' : 'static', domain, listen: listens.find((value) => !value.startsWith('443'))?.split(/\s+/)[0] || '80', root: directive('root') || old.root, index: directive('index') || old.index, proxyPass: proxy || old.proxyPass, proxyTimeout: directive('proxy_read_timeout') || old.proxyTimeout, proxyBuffering: !/^\s*proxy_buffering\s+off\s*;/m.test(content), phpSocket: php || old.phpSocket, redirectTo: redirect || old.redirectTo, ssl: /listen\s+443\s+ssl/.test(content), certificate: directive('ssl_certificate') || '', certificateKey: directive('ssl_certificate_key') || '', gzip: /gzip\s+on\s*;/.test(content), maxBody: directive('client_max_body_size') || '' }))
    }
    setExtraDirectives([]); setExtraLocations([])
    const saved = visualSettings as { builder?: NginxBuilder; extraDirectives?: Array<{name:string;value:string}>; extraLocations?: Array<{path:string;config:string}>; sectionCode?: typeof sectionCode } | null
    if (target === 'edit' && saved?.builder) {
      setBuilder({ ...saved.builder, proxyTimeout: saved.builder.proxyTimeout || '60s', proxyBuffering: saved.builder.proxyBuffering ?? true })
      setExtraDirectives(saved.extraDirectives || [])
      setExtraLocations(saved.extraLocations || [])
      setSectionCode(saved.sectionCode || { http: '', server: '', location: '', tls: '' })
    } else {
      setSectionCode({ http: '', server: '', location: '', tls: '' })
    }
    setFilePickerOpen(false)
    setBuilderOpen(true)
  }

  async function detectProjectProfile() {
    setDetectingProject(true)
    try {
      const root = builder.root.trim()
      if (!root.startsWith('/var/www/')) throw new Error('تشخیص خودکار فعلاً برای مسیرهای زیر /var/www فعال است؛ نوع پروژه را می‌توانی دستی انتخاب کنی.')
      const result = await api<NginxFileListing>(`nginx/files?path=${encodeURIComponent(root)}`)
      const names = new Set(result.entries.map((entry) => entry.name.toLowerCase()))
      let preset: NginxPreset = 'static'
      let label = 'سایت استاتیک'
      if (names.has('bin') && names.has('composer.json') && names.has('public')) { preset = 'symfony'; label = 'Symfony' }
      else if (names.has('artisan') || (names.has('composer.json') && names.has('public'))) { preset = 'laravel'; label = 'Laravel' }
      else if (names.has('wp-config.php') || (names.has('wp-admin') && names.has('wp-content'))) { preset = 'wordpress'; label = 'WordPress' }
      else if (names.has('manage.py') || names.has('wsgi.py') || names.has('asgi.py')) { preset = 'python'; label = 'Python / Django / ASGI' }
      else if (names.has('package.json')) { preset = 'node'; label = 'Node.js / Next.js / Nuxt' }
      else if (names.has('composer.json')) { preset = 'php'; label = 'PHP / Composer' }
      else if (names.has('index.html')) { preset = 'static'; label = 'سایت استاتیک' }
      setProjectDetection({ label, preset })
      setBuilder((old) => applyNginxPreset(old, preset))
    } catch (error) { notify(error instanceof Error ? error.message : 'تشخیص نوع پروژه انجام نشد.') }
    finally { setDetectingProject(false) }
  }

  function generatedConfig() {
    const serverName = builder.domain.trim() || '_'
    const custom = extraDirectives.filter((item) => /^[a-zA-Z_][a-zA-Z0-9_]*$/.test(item.name.trim()) && item.value.trim() && !/[;{}\r\n]/.test(item.value)).map((item) => `    ${item.name.trim()} ${item.value.trim()};`).join('\n')
    const locations = extraLocations.filter((item) => item.path.trim() && item.config.trim()).map((item) => `    location ${item.path.trim()} {\n${item.config.split('\n').map((line) => `        ${line}`).join('\n')}\n    }`).join('\n\n')
    const common = [builder.gzip ? '    gzip on;' : '', builder.maxBody.trim() ? `    client_max_body_size ${builder.maxBody.trim()};` : '', builder.clientTimeout.trim() ? `    client_body_timeout ${builder.clientTimeout.trim()};\n    send_timeout ${builder.clientTimeout.trim()};` : '', (builder.limitRate || '').trim() ? `    limit_rate ${builder.limitRate.trim()};` : '', (builder.limitRateAfter || '').trim() ? `    limit_rate_after ${builder.limitRateAfter.trim()};` : '', builder.serverTokens ? '' : '    server_tokens off;', builder.accessLog.trim() ? `    access_log ${builder.accessLog.trim()};` : '', builder.errorLog.trim() ? `    error_log ${builder.errorLog.trim()} warn;` : '', builder.securityHeaders ? '    add_header X-Content-Type-Options nosniff always;\n    add_header X-Frame-Options SAMEORIGIN always;\n    add_header Referrer-Policy strict-origin-when-cross-origin always;' : '', custom, sectionCode.server.trim()].filter(Boolean).join('\n')
    if (['laravel', 'symfony', 'wordpress'].includes(builder.preset)) {
      const ssl = builder.ssl ? `    listen 443 ssl;\n    listen [::]:443 ssl;\n    ssl_certificate ${builder.certificate.trim()};\n    ssl_certificate_key ${builder.certificateKey.trim()};\n${sectionCode.tls.trim() ? `${sectionCode.tls.split('\n').map((line) => `    ${line}`).join('\n')}\n` : ''}` : `    listen ${builder.listen.trim() || '80'};\n    listen [::]:${builder.listen.trim() || '80'};\n`
      const routeFallback = builder.preset === 'wordpress' ? '/index.php?$args' : '/index.php?$query_string'
      const mainLocation = `    location / {\n        try_files $uri $uri/ ${routeFallback};${sectionCode.location.trim() ? `\n${sectionCode.location.split('\n').map((line) => `        ${line}`).join('\n')}` : ''}\n    }`
      const phpLocation = builder.preset === 'wordpress'
        ? `    location ~ ^/(?!wp-config\.php$|wp-content/uploads/.*\.php$).*\.php$ {\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $realpath_root$fastcgi_script_name;\n        fastcgi_param DOCUMENT_ROOT $realpath_root;\n        fastcgi_pass ${builder.phpSocket.trim()};\n        fastcgi_hide_header X-Powered-By;\n    }`
        : `    location = /index.php {\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $realpath_root/index.php;\n        fastcgi_param DOCUMENT_ROOT $realpath_root;\n        fastcgi_pass ${builder.phpSocket.trim()};\n        fastcgi_hide_header X-Powered-By;\n    }`
      const protectedLocations = `${builder.preset === 'wordpress' ? '' : String.raw`    location ~* \.(?:php|phtml|phar)(?:/|$) { return 404; }
    `}    location ~ /\.(?!well-known(?:/|$)) { deny all; }
    location ~* \.(?:env|ini|log|sql|sqlite|bak|zip|gz|tar)$ { deny all; }
    location = /favicon.ico { access_log off; log_not_found off; }
    location = /robots.txt { access_log off; log_not_found off; }${builder.preset === 'wordpress' ? String.raw`
    location ~* ^/(?:wp-config\.php|readme\.html|license\.txt) { deny all; }
    location ~* ^/wp-content/uploads/.*\.php$ { deny all; }
    ` : ''}`
      const server = `server {\n${ssl}    server_name ${serverName};\n    root ${builder.root.trim()};\n    index ${builder.index.trim() || 'index.php'};\n    charset utf-8;\n    autoindex off;\n${common ? `${common}\n` : ''}\n${mainLocation}\n\n${phpLocation}\n\n${protectedLocations}${locations ? `\n\n${locations}` : ''}\n}`
      const redirect = builder.ssl ? `\n\nserver {\n    listen ${builder.listen.trim() || '80'};\n    listen [::]:${builder.listen.trim() || '80'};\n    server_name ${serverName};\n    return 301 https://$host$request_uri;\n}` : ''
      return `${server}${redirect}\n`
    }
    if (builder.preset === 'redirect') {
      const destination = builder.redirectTo.trim() || 'https://$host$request_uri'
      const commonRedirect = common
      const httpServer = `server {\n    listen ${builder.listen.trim() || '80'};\n    server_name ${serverName};\n${commonRedirect ? `${commonRedirect}\n` : ''}    return 301 ${builder.ssl ? 'https://$host$request_uri' : destination};\n}`
      if (builder.ssl) return `${httpServer}\n\nserver {\n    listen 443 ssl;\n    server_name ${serverName};\n    ssl_certificate ${builder.certificate.trim()};\n    ssl_certificate_key ${builder.certificateKey.trim()};\n    ssl_protocols TLSv1.2 TLSv1.3;\n${sectionCode.tls.trim() ? `${sectionCode.tls.split('\n').map((line) => `    ${line}`).join('\n')}\n` : ''}    return 301 ${destination};\n}\n`
      return `${httpServer}\n`
    }
    let handler = ''
    if (['proxy', 'node', 'python'].includes(builder.preset)) handler = `    location / {\n        proxy_pass ${builder.proxyPass.trim()};\n        proxy_http_version 1.1;\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n        proxy_read_timeout ${builder.proxyTimeout.trim() || '60s'};\n        proxy_send_timeout ${builder.proxyTimeout.trim() || '60s'};\n        proxy_buffering ${builder.proxyBuffering ? 'on' : 'off'};\n${builder.preset === 'node' ? '        proxy_set_header Upgrade $http_upgrade;\n        proxy_set_header Connection "upgrade";\n' : ''}    }`
    else if (builder.preset === 'php') handler = `    location / {\n        try_files $uri $uri/ /index.php?$query_string;\n    }\n\n    location ~ \\.php$ {\n        include snippets/fastcgi-php.conf;\n        fastcgi_pass ${builder.phpSocket.trim()};\n    }\n\n    location ~ /\\.ht {\n        deny all;\n    }`
    else handler = `    location / {\n        try_files $uri $uri/ =404;\n    }`
    const tls = builder.ssl ? `    listen 443 ssl;\n    ssl_certificate ${builder.certificate.trim()};\n    ssl_certificate_key ${builder.certificateKey.trim()};\n    ssl_protocols TLSv1.2 TLSv1.3;\n${sectionCode.tls.trim() ? `${sectionCode.tls.split('\n').map((line) => `    ${line}`).join('\n')}\n` : ''}` : ''
    const root = ['proxy', 'node', 'python'].includes(builder.preset) ? '' : `    root ${builder.root.trim()};\n    index ${builder.index.trim() || 'index.html'};\n`
    const handlerWithManual = sectionCode.location.trim() ? handler.replace(/\n    }$/, `\n${sectionCode.location.split('\n').map((line) => `        ${line}`).join('\n')}\n    }`) : handler
    const server = `server {\n    listen ${builder.listen.trim() || '80'};\n${tls}    server_name ${serverName};\n${root}${common ? `${common}\n` : ''}\n${handlerWithManual}${locations ? `\n\n${locations}` : ''}\n}`
    return `${server}\n`
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
    const settings = { builder, extraDirectives, extraLocations, sectionCode }
    setVisualSettings(settings)
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
      const result = await api<{ site: NginxSite & { sync_mismatch?: boolean; db_content?: string; settings?: Record<string, unknown> | null } }>(`nginx/sites/${encodeURIComponent(site.name)}`)
      if (result.site.sync_mismatch) { setReconcileSite({ name: site.name, dbContent: result.site.db_content || '', fileContent: result.site.content || '' }); return }
      setEditing(site.name)
      setContent(result.site.content || '')
      setVisualSettings(result.site.settings || null)
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
      setToolSite(site); setToolConfig(config); setDomainPrimary(names[0] || ''); setDomainAliases(names.slice(1).join(', ')); setSslDomains(names.join(' ')); setDomainResults([]); setSiteLog(''); setSiteLogKind(null); setSiteTraffic(null); setSiteRepository(null); setSiteToolsOpen(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'خواندن مشخصات سایت ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function loadSiteTraffic() {
    if (!toolSite) return
    setTrafficLoading(true)
    try { setSiteTraffic(await api<NginxTraffic>(`nginx/sites/${encodeURIComponent(toolSite.name)}/traffic`)) }
    catch (error) { notify(error instanceof Error ? error.message : 'آمار ترافیک دریافت نشد.') }
    finally { setTrafficLoading(false) }
  }

  async function loadSiteRepository() {
    if (!toolSite) return
    setRepositoryLoading(true)
    try { setSiteRepository(await api<NginxRepository>(`nginx/sites/${encodeURIComponent(toolSite.name)}/repository`)) }
    catch (error) { notify(error instanceof Error ? error.message : 'وضعیت مخزن دریافت نشد.') }
    finally { setRepositoryLoading(false) }
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

  async function issueSiteSsl() {
    if (!toolSite) return
    const domains = sslDomains.split(/[\s,]+/).map((domain) => domain.trim()).filter(Boolean)
    if (!sslEmail.trim() || !domains.length) { notify('ایمیل معتبر و دست‌کم یک دامنه لازم است.'); return }
    setSslIssueBusy(true)
    try {
      const result = await api<{ message: string; domains: string[]; certificate: string; backup: string }>(`nginx/sites/${encodeURIComponent(toolSite.name)}/ssl`, { method: 'POST', body: JSON.stringify({ email: sslEmail.trim(), domains, redirect: sslRedirect }) })
      notify(`${result.message} · ${result.certificate}`)
      const refreshed = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(toolSite.name)}`)
      setToolConfig(refreshed.site.content || '')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'صدور و فعال‌سازی SSL ناموفق بود.') }
    finally { setSslIssueBusy(false) }
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
    setNewName(candidate); setNewContent(toolConfig); setVisualSettings(null); setDuplicateNotice(true); setSiteToolsOpen(false); setCreateOpen(true)
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
      const result = await api<{ site: NginxSite }>(`nginx/sites/${encodeURIComponent(editing)}`, { method: 'PUT', body: JSON.stringify({ content, settings: visualSettings }) })
      setEditorOpen(false)
      notify(result.site.message || 'تنظیمات ذخیره و بررسی شد.')
      await refresh()
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیره تنظیمات ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function reconcileNginxSite(source: 'database' | 'file') {
    if (!reconcileSite) return
    const name = reconcileSite.name
    setBusy(true)
    try {
      const result = await api<{ message: string }>(`nginx/sites/${encodeURIComponent(name)}/sync`, { method: 'POST', body: JSON.stringify({ source }) })
      setReconcileSite(null)
      setDatabaseRestorePending(false)
      setConfirmDatabaseRestore(false)
      notify(result.message)
      // A just-reconciled site's listing can briefly be served by the old
      // privileged helper during an update/restart. Do not reopen the same
      // confirmation dialog while the successful sync settles.
      await refresh(name)
    } catch (error) { notify(error instanceof Error ? error.message : 'همگام‌سازی انجام نشد.') }
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
  if (loading) return <><Heading title="مدیریت کامل Nginx" description="وضعیت سرویس و سایت‌های Nginx در حال بارگذاری است."/><NginxPageSkeleton/></>
  return <>
    <Heading title="مدیریت کامل Nginx" description="سایت‌های چندمشتری، دامنه، SSL، فایل‌ها، منابع برنامه و ترافیک هر سایت را مدیریت کن." action={<div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" disabled={busy || loading} onClick={() => { setLoading(true); void refresh() }}><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/> تازه‌سازی</Button>{status?.installed && <><Button size="sm" disabled={busy} onClick={() => openBuilder('create')}><Sparkles size={15}/>ساخت با راهنمای بصری</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => { setNewName(''); setNewContent('server {\n    listen 80;\n    server_name example.com;\n\n    location / {\n        return 404;\n    }\n}\n'); setVisualSettings(null); setDuplicateNotice(false); setCreateOpen(true) }}><Code2 size={15}/>کانفیگ سفارشی</Button></>}</div>}/>
    <SectionCard title="سرویس Nginx" description="وضعیت سرویس، اجرای دستورهای مدیریتی و اعتبارسنجی پیکربندی" right={status?.installed ? <Badge variant={status.active ? 'success' : 'warning'}>{status.active ? 'فعال' : 'متوقف'}</Badge> : statusError ? <Badge variant="warning">نامشخص</Badge> : <Badge variant="outline">نصب نشده</Badge>}>
      {status?.installed ? <>
        <div className="grid gap-3 sm:grid-cols-3"><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">وضعیت اجرا</p><p className="mt-2 font-semibold text-white">{status.active ? 'در حال اجرا' : 'متوقف'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">اجرا هنگام بوت</p><p className="mt-2 font-semibold text-white">{status.enabled ? 'فعال' : 'غیرفعال'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">نسخه</p><p dir="ltr" className="mt-2 truncate text-left font-mono text-xs text-white">{status.version || 'نامشخص'}</p></div></div>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-white/[.07] pt-4"><Button size="sm" disabled={busy || status.active} onClick={() => void runAction('start')}><Play size={14}/>شروع</Button><Button size="sm" variant="outline" disabled={busy || !status.active} onClick={() => void runAction('stop')}><Square size={14}/>توقف</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => void runAction('restart')}><RotateCw size={14}/>راه‌اندازی مجدد</Button><Button size="sm" variant="outline" disabled={busy || !status.active} onClick={() => void runAction('reload')}><RefreshCw size={14}/>Reload</Button><Button size="sm" variant="outline" disabled={busy} onClick={() => void runAction('test')}><ShieldCheck size={14}/>بررسی تنظیمات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void runAction(status.enabled ? 'disable' : 'enable')}>{status.enabled ? 'غیرفعال‌کردن اجرا هنگام بوت' : 'فعال‌کردن اجرا هنگام بوت'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void runAction('logs')}><Terminal size={14}/>لاگ سرویس</Button></div>
        <div className={`mt-4 rounded-xl border p-3 text-xs ${status.config_test.ok ? 'border-emerald-400/20 bg-emerald-400/[.04] text-emerald-200' : 'border-amber-400/20 bg-amber-400/[.04] text-amber-100'}`}><div className="font-semibold">{status.config_test.ok ? 'پیکربندی معتبر است' : 'پیکربندی نیاز به بررسی دارد'}</div><pre dir="ltr" className="mt-2 max-h-24 overflow-auto whitespace-pre-wrap text-left font-mono text-[11px] opacity-80">{status.config_test.output}</pre></div>
      </> : statusError ? <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-amber-400/20 bg-amber-400/[.04] p-4"><div><p className="font-medium text-amber-100">وضعیت نصب Nginx مشخص نشد</p><p className="mt-1 text-xs text-muted-foreground">{statusError}</p></div><Button variant="outline" size="sm" disabled={busy} onClick={() => { setLoading(true); void refresh() }}><RefreshCw size={14}/>تلاش دوباره</Button></div> : <div className="flex flex-wrap items-center justify-between gap-4 rounded-xl border border-dashed border-white/10 p-4"><div><p className="font-medium text-white">Nginx نصب نشده است</p><p className="mt-1 text-xs text-muted-foreground">نصب از مخزن سیستم‌عامل با apt، dnf یا yum انجام می‌شود.</p></div><Button disabled={busy} onClick={() => void runAction('install')}><Download size={15}/>{busy ? 'در حال نصب…' : 'نصب Nginx'}</Button></div>}
    </SectionCard>
    <div className="mb-5 grid gap-3 sm:grid-cols-3">
      <Metric icon={Globe} label="همهٔ سایت‌ها" value={formatNumber(sites.length)} detail="فایل‌های معتبر sites-available"/>
      <Metric icon={Power} label="فعال" value={formatNumber(enabledCount)} detail="symlink فعال در sites-enabled"/>
      <Metric icon={ShieldCheck} label="غیرفعال" value={formatNumber(sites.length - enabledCount)} detail="آمادهٔ فعال‌سازی با یک کلیک"/>
    </div>
    {loadError && <Alert className="mb-5 border-amber-300/20 bg-amber-300/[.06]"><AlertTitle>فهرست سایت‌ها در دسترس نیست</AlertTitle><AlertDescription className="leading-6">{loadError}<span className="mt-1 block">برای مدیریت سایت‌ها، Nginx و پوشه‌های استاندارد <code dir="ltr">sites-available</code> و <code dir="ltr">sites-enabled</code> باید موجود باشند.</span></AlertDescription></Alert>}
    <SectionCard title="سایت‌های موجود" description="فعال‌سازی، symlink استاندارد را می‌سازد؛ خاموش‌کردن فقط symlink همان سایت را حذف می‌کند." right={<Badge variant="outline">{formatNumber(sites.length)} سایت</Badge>}>
      {loading ? <Skeleton className="h-48"/> : sites.length ? <div className="grid gap-3 xl:grid-cols-2">{sites.map((site) => <article key={site.name} className="rounded-[14px] border border-white/10 bg-white/[.025] p-4 transition hover:border-primary/25 hover:bg-white/[.04]">
        <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><Globe size={16} className="shrink-0 text-primary"/><h2 className="truncate font-mono text-sm font-semibold text-white" dir="ltr">{site.name}</h2>{site.sync_mismatch && <Badge variant="warning">اختلاف DB / فایل</Badge>}</div>{site.description && <p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{site.description}</p>}<p className="mt-2 font-mono text-[11px] text-muted-foreground" dir="ltr">/etc/nginx/sites-available/{site.name}</p><p className="mt-1 text-[11px] text-muted-foreground" dir="ltr">{site.enabled ? `symlink → ../sites-available/${site.name}` : 'بدون symlink فعال'}</p></div><Badge variant={site.enabled ? 'success' : 'outline'}>{site.enabled ? 'روشن' : 'خاموش'}</Badge></div>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-white/[.07] pt-3"><Button size="sm" variant={site.enabled ? 'outline' : 'default'} disabled={busy} onClick={() => void toggleSite(site)}><Power size={14}/>{site.enabled ? 'خاموش‌کردن سایت' : 'روشن‌کردن و ساخت symlink'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void editSite(site)}><Pencil size={14}/>ویرایش تنظیمات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => editSiteMetadata(site)}><Info size={14}/>نام و توضیحات</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void openSiteTools(site)}><Terminal size={14}/>ابزارهای سایت</Button>{site.name !== 'default' && <Button size="sm" variant="ghost" disabled={busy} onClick={() => void deleteSite(site)}><Trash2 size={14}/>حذف</Button>}{typeof site.bytes === 'number' && <span className="mr-auto self-center font-mono text-[10px] text-muted-foreground" dir="ltr">{formatNumber(site.bytes)} B</span>}</div><NginxMaintenancePanel siteName={site.name} notify={notify}/><div className="mt-3 border-t border-white/[.07] pt-3"><button type="button" aria-expanded={expandedRuntimeSite === site.name} onClick={() => setExpandedRuntimeSite((current) => current === site.name ? null : site.name)} className="flex min-h-10 w-full items-center justify-between gap-3 rounded-lg px-2 text-right text-xs font-medium text-muted-foreground transition hover:bg-white/[.035] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><span className="flex items-center gap-2"><Activity size={14} className="text-primary"/>منابع سرویس این سایت <span className="text-[10px] font-normal">CPU، RAM و محدودیت‌ها</span></span><ChevronDown size={15} className={`transition-transform duration-200 ${expandedRuntimeSite === site.name ? 'rotate-180' : ''}`}/></button>{expandedRuntimeSite === site.name && <div className="mt-2"><NginxRuntimePanel key={site.name} siteName={site.name} notify={notify}/></div>}</div>
      </article>)}</div> : !loadError ? <div className="rounded-xl border border-dashed border-white/10 py-14 text-center"><Globe size={22} className="mx-auto text-muted-foreground"/><p className="mt-3 text-sm font-medium text-white">سایتی پیدا نشد</p><p className="mt-1 text-xs text-muted-foreground">فایل‌های پیکربندی Nginx در مسیر sites-available نمایش داده می‌شوند.</p></div> : null}
    </SectionCard>
    <Alert className="mt-5"><ShieldCheck size={16} className="mb-2 text-primary"/><AlertTitle>اعمال ایمن تنظیمات</AlertTitle><AlertDescription>قبل از ذخیره یا تغییر symlink، دستور <code dir="ltr">nginx -t</code> اجرا می‌شود. اگر تنظیمات معتبر نباشند، تغییر برگردانده می‌شود؛ در صورت روشن‌بودن سرویس، پس از اعتبارسنجی Nginx reload خواهد شد.</AlertDescription></Alert>
    <Dialog open={reconcileSite !== null} onOpenChange={(open) => { if (!open && !busy) { setReconcileSite(null); setDatabaseRestorePending(false); setConfirmDatabaseRestore(false) } }}>
      <DialogContent className="max-h-[90dvh] max-w-4xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>تغییرات متفاوت در تنظیمات Nginx شناسایی شد</DialogTitle>
          <DialogDescription>برای سایت <code dir="ltr" className="rounded bg-secondary px-1.5 py-0.5">{reconcileSite?.name}</code>، نسخهٔ ذخیره‌شده در ARG با فایل فعلی سرور تفاوت دارد. ممکن است فایل با SSH، Certbot یا ابزار دیگری تغییر کرده باشد؛ علت را نمی‌توان قطعی تشخیص داد. تا انتخاب تو هیچ نسخه‌ای بازنویسی نمی‌شود.</DialogDescription>
        </DialogHeader>
        <div className="mt-1 rounded-xl border border-amber-400/20 bg-amber-400/[.045] p-3 text-xs leading-6 text-amber-100"><b>دو منبع موجود است:</b> نسخهٔ پایگاه دادهٔ ARG و فایل فعلی سرور. انتخاب فایل، آن را وارد ARG می‌کند؛ بازگردانی ARG فایل سرور را جایگزین می‌کند و قبلش نسخهٔ فعلی پشتیبان می‌شود.</div>
        <div className="mt-4 flex gap-2 sm:hidden" role="tablist" aria-label="نسخه‌های تنظیمات"><Button size="sm" variant={reconcileView === 'file' ? 'default' : 'outline'} role="tab" aria-selected={reconcileView === 'file'} onClick={() => setReconcileView('file')}>فایل فعلی سرور</Button><Button size="sm" variant={reconcileView === 'database' ? 'default' : 'outline'} role="tab" aria-selected={reconcileView === 'database'} onClick={() => setReconcileView('database')}>نسخهٔ ARG</Button></div>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          {(['file', 'database'] as const).map((source) => <section key={source} className={`${reconcileView === source ? 'block' : 'hidden'} rounded-xl border ${source === 'file' ? 'border-emerald-400/15' : 'border-primary/20'} bg-background/35 p-3 sm:block sm:p-4`}>
            <div className="flex items-center justify-between gap-3"><h3 className="font-semibold text-foreground">{source === 'file' ? 'فایل فعلی روی سرور' : 'نسخهٔ ذخیره‌شده در ARG'}</h3><Badge variant={source === 'file' ? 'outline' : 'success'}>{source === 'file' ? 'روی دیسک' : 'در دیتابیس'}</Badge></div>
            <p className="mt-2 min-h-10 text-xs leading-5 text-muted-foreground">{source === 'file' ? 'این محتوا روی سرور می‌ماند و به‌عنوان نسخهٔ جاری در دیتابیس ARG ثبت می‌شود.' : 'این محتوا به فایل سرور نوشته می‌شود؛ تغییرات فعلی فایل جایگزین خواهد شد.'}</p>
            <pre dir="ltr" className="mt-3 max-h-[38vh] min-h-32 overflow-auto rounded-lg border border-border bg-[#0b1117] p-3 text-left font-mono text-[10px] leading-5 text-slate-300">{(source === 'file' ? reconcileSite?.fileContent : reconcileSite?.dbContent) || 'محتوایی برای نمایش موجود نیست.'}</pre>
            <div className="mt-3 rounded-lg border border-border/70 bg-card/60 p-3 text-[11px] leading-5 text-muted-foreground">{source === 'file' ? 'نتیجه: فایل روی سرور دست‌نخورده می‌ماند؛ پایگاه داده با همین فایل همگام می‌شود.' : 'نتیجه: ARG از نسخهٔ ذخیره‌شده استفاده می‌کند؛ فایل فعلی پیش از جایگزینی پشتیبان‌گیری می‌شود و nginx -t باید موفق باشد.'}</div>
          </section>)}
        </div>
        {databaseRestorePending && <label className="mt-3 flex cursor-pointer items-start gap-3 rounded-xl border border-rose-300/20 bg-rose-300/[.05] p-3 text-xs leading-5 text-rose-100"><input type="checkbox" checked={confirmDatabaseRestore} onChange={(event) => setConfirmDatabaseRestore(event.target.checked)} className="mt-1 accent-rose-400"/><span><b>تأیید می‌کنم:</b> نسخهٔ ذخیره‌شدهٔ ARG روی فایل فعلی سرور نوشته می‌شود. اگر اعتبارسنجی Nginx شکست بخورد، فایل پشتیبان بازگردانده خواهد شد.</span></label>}
        <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4"><Button variant="ghost" disabled={busy} onClick={() => setReconcileSite(null)}>بعداً تصمیم می‌گیرم</Button><div className="flex flex-wrap gap-2"><Button variant="outline" disabled={busy} onClick={() => void reconcileNginxSite('file')}>{busy ? 'در حال بررسی و همگام‌سازی…' : 'استفاده از فایل فعلی سرور'}</Button>{databaseRestorePending ? <><Button variant="ghost" disabled={busy} onClick={() => { setDatabaseRestorePending(false); setConfirmDatabaseRestore(false) }}>لغو بازگردانی</Button><Button disabled={busy || !confirmDatabaseRestore} variant="destructive" onClick={() => void reconcileNginxSite('database')}>{busy ? 'در حال پشتیبان‌گیری و بررسی…' : 'تأیید بازگردانی نسخهٔ ARG'}</Button></> : <Button disabled={busy} onClick={() => { setReconcileView('database'); setDatabaseRestorePending(true); setConfirmDatabaseRestore(false) }}>بازگردانی نسخهٔ ذخیره‌شدهٔ ARG</Button>}</div></div>
      </DialogContent>
    </Dialog>
    <Dialog open={createOpen} onOpenChange={setCreateOpen}><DialogContent className="max-h-[92vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>{duplicateNotice ? 'ساخت کپی سایت' : 'کانفیگ سفارشی Nginx'}</DialogTitle><DialogDescription>این مسیر برای کانفیگ کامل و دست‌نویس است. فایل با nginx -t اعتبارسنجی می‌شود و سایت در شروع غیرفعال می‌ماند.</DialogDescription></DialogHeader>{duplicateNotice && <Alert><AlertTitle>پیش‌نویس کپی‌شده</AlertTitle><AlertDescription>کانفیگ و server_name از سایت اصلی کپی شده‌اند. پیش از فعال‌کردن این کپی، دامنه‌ها و مسیرهای وابسته را در کانفیگ ویرایش کن.</AlertDescription></Alert>}<div className="grid gap-3 sm:grid-cols-2"><button type="button" onClick={() => { setCreateOpen(false); openBuilder('create') }} className="rounded-xl border border-primary/35 bg-primary/[.07] p-4 text-right transition hover:bg-primary/[.12]"><span className="flex items-center gap-2 text-sm font-semibold text-white"><Sparkles size={16} className="text-primary"/>رفتن به سازندهٔ بصری</span><span className="mt-1 block text-xs leading-5 text-muted-foreground">انتخاب Laravel، WordPress، Node، Python، استاتیک یا Reverse Proxy با پیشنهاد خودکار.</span></button><div className="rounded-xl border border-white/15 bg-white/[.025] p-4"><span className="flex items-center gap-2 text-sm font-semibold text-white"><Code2 size={16} className="text-primary"/>ویرایش کامل سفارشی</span><span className="mt-1 block text-xs leading-5 text-muted-foreground">کانفیگ خام Nginx؛ هر server/location و directive دلخواه را بنویس.</span></div></div><form onSubmit={createSite} className="space-y-4"><div className="space-y-2"><Label htmlFor="nginx-site-name">نام فایل سایت</Label><Input id="nginx-site-name" dir="ltr" value={newName} onChange={(event) => setNewName(event.target.value.replace(/[^a-zA-Z0-9._-]/g, ''))} placeholder="customer-example.ir" required maxLength={128}/><p className="text-xs text-muted-foreground">نام فایل داخلی است و می‌تواند با دامنه یا نام پروژه یکی باشد.</p></div><div className="space-y-2"><Label htmlFor="nginx-site-content">کد کامل Nginx</Label><textarea id="nginx-site-content" spellCheck={false} dir="ltr" value={newContent} onChange={(event) => { setNewContent(event.target.value); setVisualSettings(null) }} placeholder={'server {\n    listen 80;\n    server_name example.com;\n    root /var/www/example/public;\n    location / { try_files $uri $uri/ /index.php?$query_string; }\n}'} className="min-h-[50vh] w-full resize-y rounded-xl border border-white/10 bg-background p-4 font-mono text-xs leading-6 text-[#f2f0eb] outline-none focus:border-primary/50"/></div><Button type="submit" disabled={busy || !newName.trim() || !newContent.trim()} className="w-full">{busy ? 'در حال بررسی و ساخت…' : 'اعتبارسنجی و ساخت سایت'}</Button></form></DialogContent></Dialog>
    <Dialog open={logsOpen} onOpenChange={setLogsOpen}><DialogContent className="max-h-[85vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>لاگ سرویس Nginx</DialogTitle><DialogDescription>۱۰۰ رویداد آخر ثبت‌شده در systemd journal</DialogDescription></DialogHeader><pre dir="ltr" className="max-h-[65vh] overflow-auto whitespace-pre-wrap rounded-xl bg-background p-4 text-left font-mono text-[11px] leading-5 text-muted-foreground">{logs || 'لاگی موجود نیست.'}</pre></DialogContent></Dialog>
    <Dialog open={siteMetaOpen} onOpenChange={setSiteMetaOpen}><DialogContent><DialogHeader><DialogTitle>مشخصات سایت</DialogTitle><DialogDescription>نام فایل در sites-available تغییر می‌کند؛ اگر سایت روشن باشد symlink فعال نیز هماهنگ می‌شود.</DialogDescription></DialogHeader><form onSubmit={saveSiteMetadata} className="space-y-4"><div className="space-y-2"><Label htmlFor="nginx-site-rename">نام / مسیر کانفیگ</Label><Input id="nginx-site-rename" dir="ltr" value={siteNewName} onChange={(event) => setSiteNewName(event.target.value.replace(/[^a-zA-Z0-9._-]/g, ''))} maxLength={128} required/><p className="text-[11px] text-muted-foreground" dir="ltr">/etc/nginx/sites-available/{siteNewName || 'site-name'}</p></div><div className="space-y-2"><Label htmlFor="nginx-site-description">توضیحات</Label><textarea id="nginx-site-description" maxLength={500} value={siteDescription} onChange={(event) => setSiteDescription(event.target.value)} placeholder="کاربرد این سایت، مالک سرویس یا نکات عملیاتی…" className="min-h-28 w-full rounded-xl border border-white/10 bg-background p-3 text-sm leading-6 text-white outline-none focus:border-primary/50"/><p className="text-left text-[11px] text-muted-foreground" dir="ltr">{siteDescription.length}/500</p></div><Button type="submit" className="w-full" disabled={busy || !siteNewName.trim()}>{busy ? 'در حال ذخیره…' : 'ذخیرهٔ مشخصات'}</Button></form></DialogContent></Dialog>
    <Dialog open={siteToolsOpen} onOpenChange={setSiteToolsOpen}><DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>ابزارهای سایت <span dir="ltr" className="font-mono text-primary">{toolSite?.name}</span></DialogTitle><DialogDescription>تنظیمات تشخیص‌داده‌شده و عملیات اختصاصی این سایت</DialogDescription></DialogHeader>{toolSite && <div className="space-y-4"><div className="grid gap-3 sm:grid-cols-2"><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">دامنه‌ها</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*server_name\s+([^;]+);/m)?.[1]?.trim() || 'در کانفیگ پیدا نشد'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Port و TLS</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{[...toolConfig.matchAll(/^\s*listen\s+([^;]+);/gm)].map((match) => match[1].trim()).join(' · ') || 'نامشخص'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Document root</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*root\s+([^;]+);/m)?.[1]?.trim() || 'تعریف نشده'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4"><p className="text-xs text-muted-foreground">Upstream / PHP-FPM</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{[toolConfig.match(/^\s*proxy_pass\s+([^;]+);/m)?.[1], toolConfig.match(/^\s*fastcgi_pass\s+([^;]+);/m)?.[1]].filter(Boolean).join(' · ') || 'تعریف نشده'}</p></div><div className="rounded-xl border border-white/10 bg-background/50 p-4 sm:col-span-2"><p className="text-xs text-muted-foreground">گواهی SSL</p><p dir="ltr" className="mt-2 break-all text-left font-mono text-xs text-white">{toolConfig.match(/^\s*ssl_certificate\s+([^;]+);/m)?.[1]?.trim() || 'TLS در کانفیگ شناسایی نشد'}</p></div></div><SiteActionsPanel siteName={toolSite.name} enabled={toolSite.enabled} notify={notify}/><section className="space-y-3 rounded-xl border border-primary/20 bg-primary/[.04] p-4"><div><h3 className="text-sm font-semibold text-white">مدیریت دامنه‌ها</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">دامنهٔ اصلی و aliasها را تنظیم کن. ذخیره، همهٔ دستورهای server_name این فایل را به‌روزرسانی می‌کند و فقط پس از اعتبارسنجی Nginx اعمال می‌شود.</p></div><div className="grid gap-3 md:grid-cols-2"><div className="space-y-2"><Label htmlFor="nginx-primary-domain">دامنهٔ اصلی</Label><Input id="nginx-primary-domain" dir="ltr" className="text-left" placeholder="example.com" value={domainPrimary} onChange={(event) => setDomainPrimary(event.target.value.trim())}/></div><div className="space-y-2"><Label htmlFor="nginx-domain-aliases">دامنه‌های جایگزین</Label><Input id="nginx-domain-aliases" dir="ltr" className="text-left" placeholder="www.example.com, shop.example.com" value={domainAliases} onChange={(event) => setDomainAliases(event.target.value)}/></div></div><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" disabled={domainChecking || !domainList().length} onClick={() => void checkSiteDomains()}><RefreshCw size={14} className={domainChecking ? 'animate-spin' : ''}/>{domainChecking ? 'در حال بررسی DNS…' : 'بررسی DNS دامنه‌ها'}</Button><Button size="sm" disabled={busy} onClick={() => void saveSiteDomains()}><Save size={14}/>{busy ? 'در حال اعتبارسنجی…' : 'ذخیره دامنه‌ها'}</Button></div>{domainResults.length > 0 && <div className="space-y-2" aria-live="polite">{domainResults.map((item) => <div key={item.host} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-white/[.07] bg-background/60 px-3 py-2"><span dir="ltr" className="font-mono text-xs text-white">{item.host}</span><span dir="ltr" className={`text-left font-mono text-[11px] ${item.resolved ? 'text-emerald-300' : 'text-amber-300'}`}>{item.resolved ? item.addresses.join(' · ') : item.error || 'رکورد DNS پیدا نشد'}</span></div>)}<p className="text-[11px] text-muted-foreground">این بررسی فقط پاسخ DNS را نشان می‌دهد؛ برای اتصال سایت، رکورد A/AAAA را به IP سرور تنظیم کن.</p></div>}</section><section className="space-y-3 rounded-xl border border-emerald-300/15 bg-emerald-300/[.025] p-4"><div><h3 className="text-sm font-semibold text-foreground">دریافت و فعال‌سازی SSL</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">Certbot گواهی را برای دامنه‌های همین سایت می‌گیرد، HTTPS را پیکربندی می‌کند و تمدید خودکارش را به زمان‌بند سیستم می‌سپارد. wildcard پشتیبانی نمی‌شود.</p></div><div className="grid gap-3 md:grid-cols-2"><div className="space-y-2"><Label htmlFor="nginx-ssl-domains">دامنه‌ها برای گواهی</Label><Input id="nginx-ssl-domains" dir="ltr" className="text-left" value={sslDomains} onChange={(event) => setSslDomains(event.target.value)} placeholder="example.com www.example.com"/><p className="text-[10px] text-muted-foreground">فقط دامنه‌هایی که در server_name همین سایت هستند؛ با فاصله یا کاما جدا کن.</p></div><div className="space-y-2"><Label htmlFor="nginx-ssl-email">ایمیل اعلان‌های تمدید</Label><Input id="nginx-ssl-email" dir="ltr" type="email" className="text-left" value={sslEmail} onChange={(event) => setSslEmail(event.target.value)} placeholder="admin@example.com" autoComplete="email"/></div></div><label className="flex items-center gap-2 text-xs text-muted-foreground"><input type="checkbox" checked={sslRedirect} onChange={(event) => setSslRedirect(event.target.checked)} className="accent-primary"/>پس از نصب، درخواست‌های HTTP به HTTPS منتقل شوند.</label><div className="rounded-lg border border-amber-300/15 bg-amber-300/[.04] p-3 text-[11px] leading-5 text-amber-100">پیش‌نیازها: DNS دامنه باید به سرور برسد، پورت 80 برای challenge و پورت 443 برای HTTPS از بیرون قابل‌دسترسی باشد و Certbot نصب باشد. بررسی DNS به‌تنهایی دسترسی واقعی challenge را تضمین نمی‌کند؛ اگر CDN یا proxy داری، وضعیت آن را هم بررسی کن. ابزار، گواهی پنل ARG را تغییر نمی‌دهد.</div><Button size="sm" disabled={sslIssueBusy || busy || !sslEmail.trim() || !sslDomains.trim()} onClick={() => void issueSiteSsl()}><ShieldCheck size={14} className={sslIssueBusy ? 'animate-pulse' : ''}/>{sslIssueBusy ? 'در حال درخواست گواهی و بررسی Nginx…' : 'گرفتن و فعال‌کردن SSL'}</Button></section><div className="flex flex-wrap gap-2 border-y border-white/[.07] py-3"><Button size="sm" variant="outline" disabled={!toolConfig.match(/^\s*root\s+([^;]+);/m)} onClick={() => { const root = toolConfig.match(/^\s*root\s+([^;]+);/m)?.[1]?.trim(); if (root?.startsWith('/')) { setSiteToolsOpen(false); window.location.hash = `#/files?path=${encodeURIComponent(root)}` } }}><FolderOpen size={14}/>مدیریت فایل‌های سایت</Button><Button size="sm" variant="outline" onClick={() => downloadText(`${toolSite.name}.conf`, toolConfig)}><Download size={14}/>دریافت کانفیگ</Button><Button size="sm" variant="outline" onClick={() => void runAction('test')}><ShieldCheck size={14}/>بررسی کل کانفیگ Nginx</Button><Button size="sm" variant="outline" disabled={!status?.active || busy} onClick={() => void runAction('reload')}><RefreshCw size={14}/>Reload سرویس</Button><Button size="sm" variant="outline" onClick={duplicateSite}><Plus size={14}/>ساخت کپی غیرفعال</Button><Button size="sm" variant="outline" disabled={repositoryLoading} onClick={() => void loadSiteRepository()}><Github size={14} className={repositoryLoading ? 'animate-pulse' : ''}/>بررسی مخزن GitHub</Button><Button size="sm" variant="outline" disabled={trafficLoading} onClick={() => void loadSiteTraffic()}><Activity size={14} className={trafficLoading ? 'animate-pulse' : ''}/>آمار ترافیک</Button><Button size="sm" variant="ghost" onClick={() => void loadSiteLog('access')}>Access log</Button><Button size="sm" variant="ghost" onClick={() => void loadSiteLog('error')}>Error log</Button></div>{siteRepository && <section className="space-y-3 rounded-xl border border-white/10 p-4"><h3 className="text-sm font-semibold text-white">مخزن کد سایت</h3>{siteRepository.available ? <><div className="grid gap-2 sm:grid-cols-2"><div className="rounded-lg bg-background/60 p-3 text-xs">Branch: <code dir="ltr">{siteRepository.branch}</code></div><div className="rounded-lg bg-background/60 p-3 text-xs">تغییرات محلی: <strong dir="ltr">{formatNumber(siteRepository.changes || 0)}</strong></div></div><p dir="ltr" className="break-all text-left font-mono text-[11px] text-muted-foreground">{siteRepository.origin || 'Remote تنظیم نشده است.'}</p><p className="text-xs text-muted-foreground">{siteRepository.differs_from_remote ? 'HEAD محلی با آخرین commit شاخه در GitHub فرق دارد؛ پیش از deploy وضعیت مخزن را بررسی کن.' : siteRepository.remote_commit ? 'HEAD محلی با HEAD شاخهٔ دور یکسان است.' : 'وضعیت شاخهٔ دور قابل بررسی نیست.'}</p><p dir="ltr" className="font-mono text-[10px] text-muted-foreground">{siteRepository.commit}</p></> : <p className="text-xs text-muted-foreground">{siteRepository.message}</p>}</section>}{siteTraffic && <section className="space-y-3 rounded-xl border border-white/10 p-4"><h3 className="text-sm font-semibold text-white">ترافیک همین سایت</h3>{siteTraffic.available ? <><p className="text-xs text-muted-foreground">نمونهٔ آخرین {formatNumber(siteTraffic.sample_limit || 5000)} خط access log اختصاصی؛ مجموع دوره یا ترافیک شبکه نیست.</p><div className="grid gap-2 sm:grid-cols-2"><div className="rounded-lg bg-background/60 p-3 text-sm">درخواست‌ها: <strong dir="ltr">{formatNumber(siteTraffic.sample_requests || 0)}</strong></div><div className="rounded-lg bg-background/60 p-3 text-sm">حجم پاسخ: <strong dir="ltr">{formatBytes(siteTraffic.sample_body_bytes || 0)}</strong></div></div><div className="flex flex-wrap gap-2">{(siteTraffic.status_groups || []).map((count, index) => <Badge key={index} variant="outline" dir="ltr">{index + 1}xx: {formatNumber(count)}</Badge>)}</div>{!!siteTraffic.hourly?.length && <div className="space-y-2"><p className="text-xs text-muted-foreground">حجم پاسخ به تفکیک ساعت در همین نمونه</p><div className="flex h-24 items-end gap-1 overflow-x-auto rounded-lg bg-background/60 p-2" dir="ltr">{siteTraffic.hourly.map((row) => <div key={row.hour} title={`${row.hour} · ${formatBytes(row.body_bytes)} · ${formatNumber(row.requests)} requests`} className="group relative flex h-full min-w-4 flex-1 items-end"><div className="w-full rounded-t bg-primary/75 transition-colors group-hover:bg-primary" style={{ height: `${Math.max(4, 100 * row.body_bytes / Math.max(1, ...siteTraffic.hourly!.map((item) => item.body_bytes)))}%` }}/></div>)}</div><div className="flex justify-between text-[10px] text-muted-foreground" dir="ltr"><span>{siteTraffic.hourly[0]?.hour}</span><span>{siteTraffic.hourly.at(-1)?.hour}</span></div></div>}<p dir="ltr" className="break-all text-left font-mono text-[10px] text-muted-foreground">{siteTraffic.path}</p></> : <p className="text-xs text-muted-foreground">{siteTraffic.message}</p>}</section>}{siteLogKind && <section className="rounded-xl border border-white/10 p-3"><div className="mb-2 flex flex-wrap items-center justify-between gap-2"><h3 className="text-sm font-semibold text-white">{siteLogKind === 'access' ? 'Access log' : 'Error log'}</h3><span dir="ltr" className="max-w-full truncate font-mono text-[10px] text-muted-foreground">{siteLogPath}</span></div>{siteLogLoading ? <Skeleton className="h-28"/> : <pre dir="ltr" className="max-h-72 overflow-auto whitespace-pre-wrap rounded-lg bg-background p-3 text-left font-mono text-[10px] leading-5 text-muted-foreground">{siteLog || 'لاگی برای نمایش نیست.'}</pre>}</section>}</div>}</DialogContent></Dialog>
    <Dialog open={editorOpen} onOpenChange={setEditorOpen}><DialogContent className="max-h-[92vh] max-w-5xl overflow-y-auto"><DialogHeader><DialogTitle>ویرایش سایت <span className="font-mono text-primary" dir="ltr">{editing}</span></DialogTitle><DialogDescription>مسیر فایل: <code dir="ltr">/etc/nginx/sites-available/{editing}</code>. ذخیره فقط پس از موفقیت <code dir="ltr">nginx -t</code> اعمال می‌شود.</DialogDescription></DialogHeader><div className="flex justify-end"><Button type="button" variant="outline" size="sm" onClick={() => { setEditorOpen(false); openBuilder('edit') }}><Sparkles size={14}/>ساخت با فرم بصری</Button></div><form onSubmit={saveSite} className="space-y-4"><textarea aria-label="تنظیمات Nginx" spellCheck={false} dir="ltr" value={content} onChange={(event) => { setContent(event.target.value); setVisualSettings(null) }} className="min-h-[55vh] w-full resize-y rounded-xl border border-white/10 bg-background p-4 font-mono text-xs leading-6 text-[#f2f0eb] outline-none transition focus:border-primary/50 focus:ring-2 focus:ring-primary/15"/><div className="flex flex-wrap items-center justify-between gap-3"><p className="text-xs text-muted-foreground">سقف اندازهٔ فایل 512 KB است؛ در صورت خطا نسخهٔ قبلی حفظ می‌شود.</p><Button type="submit" disabled={busy}><Save size={15}/>{busy ? 'در حال اعتبارسنجی…' : 'اعتبارسنجی و ذخیره'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={builderOpen} onOpenChange={(open) => { setBuilderOpen(open); if (!open) setFilePickerOpen(false) }}><DialogContent className="max-h-[94vh] max-w-5xl overflow-y-auto"><DialogHeader><DialogTitle>{filePickerOpen ? 'انتخاب فایل SSL' : 'سازندهٔ بصری کانفیگ'}</DialogTitle><DialogDescription>{filePickerOpen ? 'فقط مسیر و نام فایل نمایش داده می‌شود؛ محتوای گواهی و کلید خصوصی خوانده نمی‌شود.' : 'قالب مناسب را انتخاب کن، فیلدها را پر کن و پیش‌نمایش کانفیگ را وارد ویرایشگر کن.'}</DialogDescription></DialogHeader>
      {filePickerOpen ? <div className="space-y-4"><div className="flex flex-wrap items-center gap-2"><Button type="button" size="sm" variant="outline" onClick={() => { setFileListing(null); void browseFiles() }}>پوشه‌های مجاز</Button>{fileListing?.path && fileListing.roots.some((root) => fileListing.path !== root) && <Button type="button" size="sm" variant="outline" onClick={() => { const current = fileListing.path; const parent = current.slice(0, current.lastIndexOf('/')) || '/'; void browseFiles(parent) }}><ChevronUp size={14}/>پوشهٔ بالاتر</Button>}<span dir="ltr" className="min-w-0 flex-1 truncate text-left font-mono text-[11px] text-muted-foreground">{fileListing?.path || 'انتخاب پوشه'}</span><Button type="button" size="sm" variant="ghost" onClick={() => setFilePickerOpen(false)}>بازگشت</Button></div>
        {!fileListing || !fileListing.path ? <div className="grid gap-2 sm:grid-cols-2">{fileListing?.roots.map((root) => <button key={root} type="button" dir="ltr" onClick={() => void browseFiles(root)} className="flex items-center gap-3 rounded-xl border border-white/10 bg-background/50 p-3 text-left font-mono text-xs text-white hover:border-primary/40"><FolderOpen size={16} className="shrink-0 text-primary"/>{root}</button>)}</div> : fileLoading ? <Skeleton className="h-40"/> : <div className="max-h-[45vh] space-y-1 overflow-auto rounded-xl border border-white/10 p-2">{fileListing.entries.map((entry) => <button key={entry.path} type="button" disabled={!entry.directory && (!entry.selectable || (filePickerFor === 'certificateKey' ? !/\.(key|pem)$/i.test(entry.name) : /\.key$/i.test(entry.name)))} onClick={() => entry.directory ? void browseFiles(entry.path) : (setBuilder((old) => ({ ...old, [filePickerFor]: entry.path })), setFilePickerOpen(false))} className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-xs text-muted-foreground hover:bg-white/[.04] disabled:cursor-not-allowed disabled:opacity-35"><FolderOpen size={15} className={entry.directory ? 'text-primary' : 'text-muted-foreground'}/><span dir="ltr" className="min-w-0 flex-1 truncate font-mono">{entry.name}</span>{entry.directory ? <span>پوشه</span> : entry.selectable ? <span>انتخاب</span> : null}</button>)}{!fileListing.entries.length && <p className="p-6 text-center text-xs text-muted-foreground">فایل SSL در این پوشه پیدا نشد.</p>}</div>}
      </div> : <div className="space-y-5">
        <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/20 bg-primary/[.04] p-4"><div><h3 className="text-sm font-semibold text-white">پیشنهاد بر اساس فایل‌های پروژه</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">مسیر ریشه را پایین وارد کن، بعد تشخیص را بزن. فقط نام فایل‌ها در مسیرهای مجاز وب بررسی می‌شوند؛ چیزی اجرا نمی‌شود.</p>{projectDetection && <p className="mt-2 text-xs text-primary">پیشنهاد تشخیص‌داده‌شده: {projectDetection.label}</p>}</div><Button type="button" variant="outline" disabled={detectingProject || !builder.root.trim()} onClick={() => void detectProjectProfile()}><Search size={14} className={detectingProject ? 'animate-pulse' : ''}/>{detectingProject ? 'در حال بررسی فایل‌ها…' : 'تشخیص پروژه'}</Button></section>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">{([['static','سایت استاتیک','HTML/CSS و فایل‌های آماده'],['laravel','Laravel','ریشهٔ public، front controller و محافظت فایل‌ها'],['symfony','Symfony','public/index.php و FastCGI امن'],['wordpress','WordPress','PHP-FPM، rewrite و محافظت wp-config/uploads'],['php','PHP عمومی','FastCGI برای برنامه‌های PHP'],['node','Node.js / Next / Nuxt','Reverse proxy با WebSocket headers'],['python','Python / Django / FastAPI','Reverse proxy به سرویس ASGI/WSGI'],['proxy','Reverse proxy عمومی','اتصال هر upstream HTTP محلی'],['redirect','Redirect','تغییر مسیر با کد 301']] as const).map(([value,title,detail]) => <button key={value} type="button" aria-pressed={builder.preset === value} onClick={() => { setBuilder((old) => applyNginxPreset(old, value)); setProjectDetection(null) }} className={`rounded-xl border p-3 text-right transition ${builder.preset === value ? 'border-primary/50 bg-primary/10' : 'border-white/10 bg-background/40 hover:border-white/20'}`}><span className="flex items-center gap-2 text-sm font-semibold text-white">{title}<NginxHelp>{detail}</NginxHelp></span><span className="mt-1 block text-[11px] leading-5 text-muted-foreground">{detail}</span></button>)}</div>
        <div className="grid gap-4 md:grid-cols-2"><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-domain">دامنه‌ها</Label><NginxHelp>مقادیر با فاصله جدا شوند؛ همین متن در server_name قرار می‌گیرد. برای wildcard از *.example.com استفاده کن.</NginxHelp></div><Input id="nginx-builder-domain" dir="ltr" value={builder.domain} onChange={(event) => setBuilder((old) => ({ ...old, domain: event.target.value }))} placeholder="example.com www.example.com"/></div><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-listen">پورت HTTP</Label><NginxHelp>پورت plain HTTP این virtual host؛ HTTPS از بخش گواهی SSL ساخته می‌شود.</NginxHelp></div><Input id="nginx-builder-listen" dir="ltr" inputMode="numeric" value={builder.listen} onChange={(event) => setBuilder((old) => ({ ...old, listen: event.target.value.replace(/[^0-9]/g, '') }))}/></div>
          {['static', 'php', 'laravel', 'symfony', 'wordpress'].includes(builder.preset) ? <><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-root">مسیر ریشهٔ سایت</Label><NginxHelp>مسیر پروژه روی سرور. برای Laravel و Symfony مسیر public به‌صورت پیش‌فرض انتخاب می‌شود.</NginxHelp></div><Input id="nginx-builder-root" dir="ltr" value={builder.root} onChange={(event) => setBuilder((old) => ({ ...old, root: event.target.value }))} placeholder="/var/www/example.com/public"/></div><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-index">فایل‌های پیش‌فرض</Label><NginxHelp>فهرست فایل‌های index به ترتیب اولویت؛ با فاصله جدا شوند، مانند index.php index.html.</NginxHelp></div><Input id="nginx-builder-index" dir="ltr" value={builder.index} onChange={(event) => setBuilder((old) => ({ ...old, index: event.target.value }))}/></div></> : null}
          {['proxy', 'node', 'python'].includes(builder.preset) && <><div className="space-y-2 md:col-span-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-proxy">آدرس برنامهٔ مقصد</Label><NginxHelp>آدرس upstream محلی؛ مثال Node: http://127.0.0.1:3000، مثال ASGI: http://127.0.0.1:8000. خود برنامه باید جداگانه اجرا شود.</NginxHelp></div><Input id="nginx-builder-proxy" dir="ltr" value={builder.proxyPass} onChange={(event) => setBuilder((old) => ({ ...old, proxyPass: event.target.value }))} placeholder="http://127.0.0.1:3000"/><p className="text-[11px] text-muted-foreground">سرویس برنامه، process manager و envها در این فیلد ساخته نمی‌شوند؛ آن‌ها را در بخش سرویس‌های systemd یا Docker مدیریت کن.</p></div><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-proxy-timeout">مهلت پاسخ Upstream</Label><NginxHelp>برای proxy_read_timeout و proxy_send_timeout. درخواست‌های طولانی، آپلود یا streaming ممکن است مقدار بیشتری بخواهند.</NginxHelp></div><Input id="nginx-proxy-timeout" dir="ltr" value={builder.proxyTimeout} onChange={(event) => setBuilder((old) => ({ ...old, proxyTimeout: event.target.value.replace(/[^0-9smh]/gi, '') }))} placeholder="60s"/></div><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span className="flex items-center gap-2">Proxy buffering<NginxHelp>روشن‌بودن buffering پاسخ‌ها را موقتاً بافر می‌کند؛ برای SSE و پاسخ زنده معمولاً خاموش شود.</NginxHelp></span><Switch checked={builder.proxyBuffering} onCheckedChange={(proxyBuffering) => setBuilder((old) => ({ ...old, proxyBuffering }))}/></label></>}
          {['php', 'laravel', 'symfony', 'wordpress'].includes(builder.preset) && <div className="space-y-2 md:col-span-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-php">Socket یا آدرس PHP-FPM</Label><NginxHelp>مقصد FastCGI برای اجرای PHP؛ با نسخهٔ PHP نصب‌شده روی سرور هماهنگ باشد.</NginxHelp></div><Input id="nginx-builder-php" dir="ltr" value={builder.phpSocket} onChange={(event) => setBuilder((old) => ({ ...old, phpSocket: event.target.value }))}/><p className="text-[11px] text-muted-foreground">برای جداسازی منابع مشتری‌ها، PHP-FPM pool مستقل بساز و از واحد systemd مجزای همان برنامه استفاده کن.</p></div>}
          {builder.preset === 'redirect' && <div className="space-y-2 md:col-span-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-redirect">نشانی مقصد</Label><NginxHelp>مقصد پاسخ 301؛ متغیرهای Nginx مثل $host و $request_uri قابل استفاده‌اند.</NginxHelp></div><Input id="nginx-builder-redirect" dir="ltr" value={builder.redirectTo} onChange={(event) => setBuilder((old) => ({ ...old, redirectTo: event.target.value }))}/></div>}
        </div>
        <div className="rounded-xl border border-white/10 p-4"><label className="flex items-center justify-between gap-3"><span><span className="flex items-center gap-2 text-sm font-medium text-white">فعال‌سازی HTTPS <NginxHelp>برای این گزینه، باید مسیر certificate و private key معتبر وجود داشته باشد؛ قبل از ذخیره nginx -t گواهی را بررسی می‌کند.</NginxHelp></span><span className="mt-1 block text-xs text-muted-foreground">مسیر certificate و private key را از فایل‌منیجر انتخاب کن.</span></span><Switch checked={builder.ssl} onCheckedChange={(ssl) => setBuilder((old) => ({ ...old, ssl }))}/></label>{builder.ssl && <div className="mt-4 grid gap-3 md:grid-cols-2"><div className="space-y-2"><div className="flex items-center gap-2"><Label>گواهی SSL</Label><NginxHelp>مسیر certificate chain به فایل PEM، معمولاً fullchain.pem.</NginxHelp></div><div className="flex gap-2"><Input dir="ltr" value={builder.certificate} onChange={(event) => setBuilder((old) => ({ ...old, certificate: event.target.value }))} placeholder="/etc/letsencrypt/live/.../fullchain.pem"/><Button type="button" variant="outline" size="icon" aria-label="انتخاب گواهی" onClick={() => openFilePicker('certificate')}><FolderOpen size={15}/></Button></div></div><div className="space-y-2"><div className="flex items-center gap-2"><Label>کلید خصوصی SSL</Label><NginxHelp>فایل private key مرتبط با گواهی؛ مقدار کلید از سرور خوانده یا نمایش داده نمی‌شود.</NginxHelp></div><div className="flex gap-2"><Input dir="ltr" value={builder.certificateKey} onChange={(event) => setBuilder((old) => ({ ...old, certificateKey: event.target.value }))} placeholder="/etc/letsencrypt/live/.../privkey.pem"/><Button type="button" variant="outline" size="icon" aria-label="انتخاب کلید خصوصی" onClick={() => openFilePicker('certificateKey')}><FolderOpen size={15}/></Button></div></div></div>}</div>
        <div className="grid gap-3 sm:grid-cols-2"><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span className="flex items-center gap-2">فعال‌سازی Gzip <NginxHelp>فشرده‌سازی پاسخ‌های متنی برای کاهش حجم انتقال. تنظیم کامل‌تر آن را در کد دستی بخش HTTP وارد کن.</NginxHelp></span><Switch checked={builder.gzip} onCheckedChange={(gzip) => setBuilder((old) => ({ ...old, gzip }))}/></label><div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-body">حداکثر اندازهٔ درخواست</Label><NginxHelp>مقدار client_max_body_size؛ برای مثال 32m. درخواست بزرگ‌تر با خطای 413 رد می‌شود.</NginxHelp></div><Input id="nginx-builder-body" dir="ltr" value={builder.maxBody} onChange={(event) => setBuilder((old) => ({ ...old, maxBody: event.target.value.replace(/[^0-9kKmMgG]/g, '') }))} placeholder="32m"/></div>
          <div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-builder-timeout">مهلت دریافت درخواست</Label><NginxHelp>برای client_body_timeout و send_timeout در سطح سایت اعمال می‌شود؛ نمونه: 60s.</NginxHelp></div><Input id="nginx-builder-timeout" dir="ltr" value={builder.clientTimeout} onChange={(event) => setBuilder((old) => ({ ...old, clientTimeout: event.target.value.replace(/[^0-9smh]/gi, '') }))}/></div>
          <div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-limit-rate">سقف سرعت پاسخ هر اتصال</Label><NginxHelp>دستور limit_rate؛ نمونه 1m برای محدود کردن سرعت هر اتصال. برای سقف مجموع سایت، سرویس یا زیرساخت جداگانه لازم است.</NginxHelp></div><Input id="nginx-limit-rate" dir="ltr" value={builder.limitRate || ''} onChange={(event) => setBuilder((old) => ({ ...old, limitRate: event.target.value.replace(/[^0-9kKmMgG]/g, '') }))} placeholder="1m"/></div>
          <div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-limit-after">شروع محدودسازی پس از</Label><NginxHelp>دستور limit_rate_after؛ چند بایت اول پاسخ بدون محدودیت منتقل می‌شود. نمونه 10m.</NginxHelp></div><Input id="nginx-limit-after" dir="ltr" value={builder.limitRateAfter || ''} onChange={(event) => setBuilder((old) => ({ ...old, limitRateAfter: event.target.value.replace(/[^0-9kKmMgG]/g, '') }))} placeholder="10m"/></div>
          <div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-access-log">مسیر Access log</Label><NginxHelp>مسیر اختصاصی ثبت درخواست‌های سایت. خالی باشد از مقدار پیش‌فرض Nginx استفاده می‌شود.</NginxHelp></div><Input id="nginx-access-log" dir="ltr" value={builder.accessLog} onChange={(event) => setBuilder((old) => ({ ...old, accessLog: event.target.value }))} placeholder="/var/log/nginx/example.access.log"/></div>
          <div className="space-y-2"><div className="flex items-center gap-2"><Label htmlFor="nginx-error-log">مسیر Error log</Label><NginxHelp>مسیر گزارش خطاهای همین سایت. سطح پیش‌فرض هشدار است؛ تنظیمات پیشرفته را در کد دستی اضافه کن.</NginxHelp></div><Input id="nginx-error-log" dir="ltr" value={builder.errorLog} onChange={(event) => setBuilder((old) => ({ ...old, errorLog: event.target.value }))} placeholder="/var/log/nginx/example.error.log"/></div>
          <label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span className="flex items-center gap-2">نمایش نسخهٔ Nginx <NginxHelp>به‌صورت پیش‌فرض خاموش است تا نسخهٔ سرویس در header خطا نمایش داده نشود.</NginxHelp></span><Switch checked={builder.serverTokens} onCheckedChange={(serverTokens) => setBuilder((old) => ({ ...old, serverTokens }))}/></label>
          <label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span className="flex items-center gap-2">هدرهای امنیتی پایه <NginxHelp>هدرهای nosniff، SAMEORIGIN و Referrer-Policy را اضافه می‌کند. برای CSP و HSTS از کد دستی TLS استفاده کن.</NginxHelp></span><Switch checked={builder.securityHeaders} onCheckedChange={(securityHeaders) => setBuilder((old) => ({ ...old, securityHeaders }))}/></label>
        </div>
        <section className="space-y-3 rounded-xl border border-white/10 p-4"><div className="flex flex-wrap items-center justify-between gap-2"><div><h3 className="flex items-center gap-2 text-sm font-semibold text-white">Directiveهای اضافه <NginxHelp>هر directive مجاز در context سایت با نام و مقدار جدا. برای چندخطی، بلاک‌ها یا دستورهای پیچیده از ویرایش دستی بخش استفاده کن.</NginxHelp></h3><p className="mt-1 text-xs text-muted-foreground">برای افزودن تنظیم‌هایی مثل timeout یا header.</p></div><Button type="button" variant="outline" size="sm" onClick={() => setExtraDirectives((old) => [...old, { name: '', value: '' }])}><Plus size={14}/>افزودن ورودی</Button></div>{extraDirectives.map((item,index) => <div key={index} className="grid gap-2 sm:grid-cols-[1fr_2fr_auto]"><Input dir="ltr" aria-label="نام directive" placeholder="proxy_read_timeout" value={item.name} onChange={(event) => setExtraDirectives((old) => old.map((row,i) => i === index ? { ...row, name: event.target.value } : row))}/><Input dir="ltr" aria-label="مقدار directive" placeholder="60s" value={item.value} onChange={(event) => setExtraDirectives((old) => old.map((row,i) => i === index ? { ...row, value: event.target.value } : row))}/><Button type="button" variant="ghost" size="icon" aria-label="حذف ورودی" onClick={() => setExtraDirectives((old) => old.filter((_,i) => i !== index))}><X size={15}/></Button></div>)}</section>
        <section className="space-y-3 rounded-xl border border-white/10 p-4"><div className="flex flex-wrap items-center justify-between gap-2"><div><h3 className="flex items-center gap-2 text-sm font-semibold text-white">Location و تنظیمات سفارشی <NginxHelp>مسیری جدا مثل /api/ یا /assets/ بساز و دستورهای مستقل همان مسیر را داخل آن قرار بده.</NginxHelp></h3><p className="mt-1 text-xs text-muted-foreground">برای مسیرهای خاص یا اتصال اسکریپت؛ PHP-FPM هم در قالب PHP آماده است.</p></div><Button type="button" variant="outline" size="sm" onClick={() => setExtraLocations((old) => [...old, { path: '/api/', config: 'proxy_pass http://127.0.0.1:3000;' }])}><Code2 size={14}/>افزودن location</Button></div>{extraLocations.map((item,index) => <div key={index} className="space-y-2 rounded-lg border border-white/[.07] p-3"><div className="flex gap-2"><Input dir="ltr" aria-label="مسیر location" placeholder="/api/" value={item.path} onChange={(event) => setExtraLocations((old) => old.map((row,i) => i === index ? { ...row, path: event.target.value } : row))}/><Button type="button" variant="ghost" size="icon" aria-label="حذف location" onClick={() => setExtraLocations((old) => old.filter((_,i) => i !== index))}><X size={15}/></Button></div><textarea dir="ltr" spellCheck={false} aria-label="دستورهای location" value={item.config} onChange={(event) => setExtraLocations((old) => old.map((row,i) => i === index ? { ...row, config: event.target.value } : row))} placeholder="proxy_pass http://127.0.0.1:3000;" className="min-h-20 w-full rounded-lg border border-white/10 bg-background p-3 font-mono text-xs leading-5 text-white"/></div>)}</section>
        <section className="space-y-3 rounded-xl border border-amber-300/15 bg-amber-300/[.025] p-4"><div><h3 className="text-sm font-semibold text-white">تنظیمات تکمیلی در context سایت</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">این ورودی‌ها فقط داخل server یا location همین سایت قرار می‌گیرند؛ Nginx پیش از ذخیره کانفیگ را بررسی می‌کند. دستورهای سراسری مانند map و limit_req_zone باید در فایل‌های سطح http قرار بگیرند و از فایل سایت درج نمی‌شوند.</p></div>
          {([['server','بخش Server','دستورهای مستقیم داخل server؛ مثل charset، error_page، allow/deny یا proxy_*_timeout.'],['location','بخش Location اصلی','دستورهای داخل location / قالب انتخاب‌شده؛ با دستورهای قالب تداخل نگذار.'],['tls','بخش TLS','دستورهای TLS داخل server؛ مثل ssl_ciphers، ssl_session_cache، stapling یا HSTS.']] as const).map(([key,title,help]) => <div key={key} className="space-y-2"><div className="flex items-center gap-2"><Label>{title}</Label><NginxHelp>{help}</NginxHelp></div><textarea dir="ltr" spellCheck={false} value={sectionCode[key]} onChange={(event) => setSectionCode((old) => ({ ...old, [key]: event.target.value }))} placeholder={key === 'tls' ? 'ssl_session_cache shared:SSL:10m;' : 'add_header X-Example value always;'} className="min-h-24 w-full rounded-lg border border-white/10 bg-background p-3 font-mono text-xs leading-5 text-white"/><p className="text-[11px] leading-5 text-muted-foreground">{help}</p></div>)}
        </section>
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
  const timeZone = usePanelTimeZone()
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
          <div className="flex gap-2"><RefreshButton onRefresh={refresh}/><Button variant="outline" size="sm" disabled={!events.length} onClick={() => { const rows = [['at','actor','action','detail'], ...events.map((event) => [new Date(event.at * 1000).toISOString(), event.actor, event.action, event.detail])]; downloadText('arg-audit.csv', '\uFEFF' + rows.map((row) => row.map((cell) => `"${String(cell).replaceAll('"', '""')}"`).join(',')).join('\n'), 'text/csv;charset=utf-8') }}><Download size={15}/> دریافت CSV</Button></div>
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
                    {formatPanelDate(e.at, timeZone, { dateStyle: 'medium', timeStyle: 'medium' })}
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

type UpdateInfo = { status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; local_version?: string; remote_version?: string }
type PanelTlsSettings = { enabled: boolean; certificate_path: string; private_key_path: string; port: number; certificate_present: boolean; private_key_present: boolean; default_certificate_path: string; default_private_key_path: string }
export function SettingsPage({ Heading, notify, user, update, openUpdateDialog, updating, checkingUpdate, panelTimeZone, onTimeZoneChange }: { Heading: Heading; notify: (message: string) => void; user: User; update: UpdateInfo; openUpdateDialog: () => void; updating: boolean; checkingUpdate: boolean; panelTimeZone: string; onTimeZoneChange: (timezone: string) => void }) {
  const [config, setConfig] = useState<MonitoringConfig | null>(null)
  const [port, setPort] = useState(location.port || (location.protocol === 'https:' ? '443' : '80'))
  const [busy, setBusy] = useState(false)
  const [scheduledPort, setScheduledPort] = useState<number | null>(null)
  const [timezoneDraft, setTimezoneDraft] = useState(panelTimeZone)
  const [timezoneBusy, setTimezoneBusy] = useState(false)
  const [tls, setTls] = useState<PanelTlsSettings | null>(null)
  const [tlsCertificate, setTlsCertificate] = useState('')
  const [tlsPrivateKey, setTlsPrivateKey] = useState('')
  const [tlsServerName, setTlsServerName] = useState(location.hostname)
  const [tlsPort, setTlsPort] = useState('443')
  const [tlsBusy, setTlsBusy] = useState(false)
  const isAdmin = user.role === 'admin'
  useEffect(() => { void api<MonitoringConfig>('monitoring/config').then(setConfig).catch((error) => notify(error instanceof Error ? error.message : 'دریافت تنظیمات ناموفق بود')) }, [notify])
  useEffect(() => { void api<{port: number}>('settings/port').then((result) => setPort(String(result.port))).catch(() => undefined) }, [])
  useEffect(() => { void api<PanelTlsSettings>('settings/tls').then((result) => { setTls(result); setTlsCertificate(result.certificate_path || result.default_certificate_path); setTlsPrivateKey(result.private_key_path || result.default_private_key_path); setTlsPort(String(result.port || 443)) }).catch((error) => notify(error instanceof Error ? error.message : 'خواندن وضعیت گواهی SSL ناموفق بود')) }, [notify])
  useEffect(() => { setTimezoneDraft(panelTimeZone) }, [panelTimeZone])
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
  async function saveTimezone(event: FormEvent) {
    event.preventDefault()
    setTimezoneBusy(true)
    try {
      const result = await api<{ timezone: string }>('settings/timezone', { method: 'PUT', body: JSON.stringify({ timezone: timezoneDraft.trim() }) })
      setTimezoneDraft(result.timezone)
      onTimeZoneChange(result.timezone)
      notify(`منطقهٔ زمانی پنل روی ${result.timezone} تنظیم شد.`)
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ منطقهٔ زمانی ناموفق بود.') }
    finally { setTimezoneBusy(false) }
  }
  async function saveTls(event: FormEvent) {
    event.preventDefault()
    setTlsBusy(true)
    try {
      const result = await api<{scheduled: boolean; port: number}>('settings/tls', { method: 'PUT', body: JSON.stringify({ certificate_path: tlsCertificate.trim(), private_key_path: tlsPrivateKey.trim(), server_name: tlsServerName.trim(), port: Number(tlsPort) }) })
      notify('گواهی بررسی و تنظیمات HTTPS ذخیره شد؛ ARG تا چند ثانیهٔ دیگر راه‌اندازی می‌شود.')
      window.setTimeout(() => {
        const portPart = result.port === 443 ? '' : `:${result.port}`
        window.location.assign(`https://${location.hostname}${portPart}/`)
      }, 3500)
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ تنظیمات گواهی ناموفق بود') }
    finally { setTlsBusy(false) }
  }
  function selector(label: string, key: 'interval_seconds' | 'retention_days' | 'session_hours', values: number[], unit: string) {
    return <label className="space-y-2 text-sm"><span className="block font-medium text-white">{label}</span><Select disabled={!isAdmin} value={String(config?.[key] ?? values[0])} onValueChange={(value) => setConfig((old) => old && { ...old, [key]: Number(value) })}><SelectTrigger dir="ltr"><SelectValue/></SelectTrigger><SelectContent>{values.map((value) => <SelectItem key={value} value={String(value)}>{value} {unit}</SelectItem>)}</SelectContent></Select></label>
  }
  const timezonePreview = (() => {
    try { return new Intl.DateTimeFormat('en-GB', { dateStyle: 'full', timeStyle: 'short', timeZone: timezoneDraft || 'UTC' }).format(new Date()) }
    catch { return 'نام منطقهٔ زمانی را بررسی کن.' }
  })()
  return <>
    <Heading title="تنظیمات" description="زمان نمونه‌برداری، نگه‌داری تاریخچه، امنیت نشست و پورت دسترسی" />
    <div className="mb-5"><AlertControls user={user} notify={notify}/></div>
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
    <div className="mt-5"><SectionCard title="گواهی‌های SSL پنل" description="گواهی و کلید خصوصی را از فایل‌های سرور تنظیم کن؛ ARG پیش از فعال‌سازی، انقضا و تطبیق کلید را بررسی می‌کند." right={<Badge variant={tls?.enabled ? 'success' : 'warning'}>{tls?.enabled ? 'HTTPS فعال' : 'HTTP ناامن'}</Badge>}>
      {tls ? <form onSubmit={saveTls} className="space-y-4">
        <Alert className={tls.enabled ? 'border-emerald-400/20 bg-emerald-400/[.04]' : 'border-amber-400/25 bg-amber-400/[.05]'}><ShieldCheck size={16}/><AlertDescription>{tls.enabled ? `گواهی از مسیرهای ثبت‌شده خوانده می‌شود. HTTPS روی پورت ${formatNumber(tls.port)} فعال است.` : 'گواهی برای TLS فعال نیست. تا زمان فعال‌سازی HTTPS، ورود و نشست‌ها روی HTTP در معرض شنود شبکه هستند.'}</AlertDescription></Alert>
        <div className="grid gap-4 md:grid-cols-2">
          <label className="space-y-2 text-sm"><span className="block font-medium text-white">دامنه یا IP پنل</span><Input dir="ltr" lang="en" autoComplete="off" spellCheck={false} value={tlsServerName} onChange={(event) => setTlsServerName(event.target.value)} placeholder="panel.example.com" disabled={!isAdmin || tlsBusy}/><span className="block text-xs text-muted-foreground">باید در SAN گواهی باشد؛ پیش از راه‌اندازی کنترل می‌شود.</span></label>
          <label className="space-y-2 text-sm"><span className="block font-medium text-white">مسیر گواهی عمومی (fullchain.pem)</span><Input dir="ltr" lang="en" autoComplete="off" spellCheck={false} value={tlsCertificate} onChange={(event) => setTlsCertificate(event.target.value)} placeholder="/etc/arg/tls/fullchain.pem" disabled={!isAdmin || tlsBusy}/><span className="block text-xs text-muted-foreground">گواهی و زنجیرهٔ میانی؛ فایل PEM عمومی است.</span></label>
          <label className="space-y-2 text-sm"><span className="block font-medium text-white">مسیر کلید خصوصی (privkey.pem)</span><Input dir="ltr" lang="en" autoComplete="off" spellCheck={false} value={tlsPrivateKey} onChange={(event) => setTlsPrivateKey(event.target.value)} placeholder="/etc/arg/tls/privkey.pem" disabled={!isAdmin || tlsBusy}/><span className="block text-xs text-muted-foreground">محتوای کلید هرگز به مرورگر فرستاده یا در پنل نمایش داده نمی‌شود.</span></label>
          <label className="space-y-2 text-sm"><span className="block font-medium text-white">پورت HTTPS</span><Input type="number" min={1024} max={65535} dir="ltr" lang="en" value={tlsPort} onChange={(event) => setTlsPort(event.target.value)} disabled={!isAdmin || tlsBusy}/><span className="block text-xs text-muted-foreground">پورت 443 پیش‌فرض است؛ اگر اشغال باشد، پورت آزاد دیگری وارد کن.</span></label>
        </div>
        <div className="rounded-xl border border-border bg-background/45 p-3 text-xs leading-6 text-muted-foreground">صدور خودکار ابتدا از Let’s Encrypt انجام می‌شود و در صورت شکست، ZeroSSL به‌عنوان منبع دوم پیشنهاد می‌شود؛ ZeroSSL برای دامنه به EAB نیاز دارد و گواهی IP خام صادر نمی‌کند. اگر هیچ‌کدام در دسترس نبودند، گواهی را از منبع دیگری بگیر و با SSH/SFTP به مسیرهای زیر منتقل کن؛ از انتقال کلید خصوصی در اتصال HTTP خودداری کن. پس از ذخیره، ARG تطبیق certificate، private key و دامنه/IP را کنترل می‌کند و سرویس را دوباره راه می‌اندازد.
          <div className="mt-2 grid gap-1 font-mono text-[11px] text-foreground" dir="ltr"><span>{tls.default_certificate_path}</span><span>{tls.default_private_key_path}</span></div>
        </div>
        {isAdmin && <Button type="submit" disabled={tlsBusy || !tlsCertificate.trim() || !tlsPrivateKey.trim() || !tlsServerName.trim() || !/^\d+$/.test(tlsPort)}><ShieldCheck size={15}/>{tlsBusy ? 'در حال بررسی و ذخیره…' : tls.enabled ? 'اعمال گواهی و راه‌اندازی مجدد' : 'فعال‌سازی HTTPS'}</Button>}
      </form> : <Skeleton className="h-48"/>}
    </SectionCard></div>
    <div className="mt-5"><SectionCard title="منطقهٔ زمانی پنل" description="زمان تاریخچه، گزارش‌ها، نشست‌ها و تغییر فایل‌ها بر اساس این منطقه نمایش داده می‌شود.">
      <form onSubmit={saveTimezone} className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <label className="min-w-0 space-y-2 text-sm"><span className="block font-medium text-white">شناسهٔ منطقهٔ زمانی (IANA)</span><Input list="arg-timezone-presets" dir="ltr" lang="en" autoComplete="off" spellCheck={false} value={timezoneDraft} onChange={(event) => setTimezoneDraft(event.target.value)} placeholder="Asia/Tehran" disabled={!isAdmin || timezoneBusy}/><datalist id="arg-timezone-presets">{['UTC','Asia/Tehran','Asia/Dubai','Europe/London','Europe/Berlin','America/New_York','America/Los_Angeles','Asia/Tokyo','Australia/Sydney'].map((zone) => <option key={zone} value={zone}/>)}</datalist><span className="block text-xs text-muted-foreground">مثال: <code dir="ltr">Asia/Tehran</code> یا <code dir="ltr">UTC</code>. ساعت تابستانی به‌صورت خودکار اعمال می‌شود.</span><span className="block text-xs text-primary">نمونهٔ زمان اکنون: <b dir="ltr" className="font-mono">{timezonePreview}</b></span></label>
        {isAdmin && <Button type="submit" disabled={timezoneBusy || !timezoneDraft.trim() || timezoneDraft.trim() === panelTimeZone}>{timezoneBusy ? 'در حال ذخیره…' : 'ذخیرهٔ منطقهٔ زمانی'}</Button>}
      </form>
    </SectionCard></div>
    <div className="mt-5"><SectionCard title="درباره و به‌روزرسانی" description="وضعیت نصب‌شده را ببین و از یک پنجره نسخهٔ جدید را بررسی یا نصب کن." right={<Badge variant={updating || update.status === 'available' ? 'warning' : update.status === 'current' ? 'success' : 'outline'}>{updating ? 'در حال نصب' : update.status === 'available' ? 'نسخهٔ جدید' : update.status === 'current' ? 'به‌روز' : 'بررسی نشده'}</Badge>}>
      <div className="flex flex-wrap items-center justify-between gap-4"><div className="grid min-w-0 flex-1 gap-3 sm:grid-cols-2"><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.local_version ? `v${update.local_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.local || '—'}</div></div><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">آخرین نسخهٔ بررسی‌شده</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.remote_version ? `v${update.remote_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.remote || '—'}</div></div></div><div className="flex gap-2"><Button onClick={openUpdateDialog} disabled={updating || checkingUpdate} aria-busy={checkingUpdate || updating}><RefreshCw size={15} className={checkingUpdate || updating ? 'animate-spin' : ''}/>{checkingUpdate ? 'در حال بررسی…' : updating ? 'در حال نصب…' : update.status === 'available' ? 'نصب نسخهٔ جدید' : 'بررسی و به‌روزرسانی'}</Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noopener noreferrer" className="grid h-10 w-10 place-items-center rounded-[10px] border border-border text-muted-foreground hover:bg-accent" aria-label="مخزن ARG در GitHub"><Github size={17}/></a></div></div>
    </SectionCard></div>
  </>
}

type AccountSession = { id: number; created_at: number | null; last_seen: number | null; expires_at: number; remote_ip: string | null; user_agent: string | null; current: boolean }
export function AccountPage({ Heading, user, setUser, logout, notify }: { Heading: Heading; user: User; setUser: (next: User) => void; logout: () => void; notify: (message: string) => void }) {
  const timeZone = usePanelTimeZone()
  const [sessions, setSessions] = useState<AccountSession[]>([])
  const [loading, setLoading] = useState(true)
  const [sessionError, setSessionError] = useState('')
  const [username, setUsername] = useState(user.username)
  const [currentPassword, setCurrentPassword] = useState('')
  const [saving, setSaving] = useState(false)
  const loadSessions = useCallback(async () => {
    setLoading(true)
    try { const result = await api<{ sessions: AccountSession[] }>('account/sessions'); setSessions(result.sessions); setSessionError('') }
    catch (error) { setSessionError(error instanceof Error ? error.message : 'دریافت نشست‌ها ناموفق بود.') }
    finally { setLoading(false) }
  }, [])
  useEffect(() => { void loadSessions() }, [loadSessions])
  useEffect(() => { setUsername(user.username) }, [user.username])
  async function changeName(event: FormEvent) {
    event.preventDefault()
    setSaving(true)
    try {
      const result = await api<{ username: string }>('account/username', { method: 'POST', body: JSON.stringify({ username, current_password: currentPassword }) })
      setUser({ ...user, username: result.username })
      setCurrentPassword('')
      notify('نام کاربری تغییر کرد.')
    } catch (error) { notify(error instanceof Error ? error.message : 'تغییر نام کاربری ناموفق بود.') }
    finally { setSaving(false) }
  }
  async function revoke(id: number) {
    try { await api(`account/sessions/${id}`, { method: 'DELETE' }); notify('نشست بسته شد.'); await loadSessions() }
    catch (error) { notify(error instanceof Error ? error.message : 'بستن نشست ناموفق بود.') }
  }
  async function revokeOthers() {
    try { const result = await api<{ revoked: number }>('account/sessions/revoke-others', { method: 'POST', body: '{}' }); notify(`${result.revoked} نشست دیگر بسته شد.`); await loadSessions() }
    catch (error) { notify(error instanceof Error ? error.message : 'بستن نشست‌ها ناموفق بود.') }
  }
  return <>
    <Heading title="حساب کاربری" description="اطلاعات ورود، امنیت حساب و نشست‌های فعال خودت را مدیریت کن." />
    <div className="grid gap-5 xl:grid-cols-[.8fr_1.2fr]">
      <SectionCard title="مشخصات حساب" description="نام کاربری و سطح دسترسی جاری" right={<UserRound size={18} className="text-primary" />}>
        <div className="mb-5 flex items-center gap-3 rounded-xl border border-border bg-secondary/25 p-4"><div className="grid h-11 w-11 place-items-center rounded-xl border border-primary/20 bg-primary/[.08] text-lg font-bold text-primary">{user.username.slice(0,1).toUpperCase()}</div><div><div className="font-semibold text-foreground">{user.username}</div><div className="mt-1 text-xs text-muted-foreground">{user.role === 'admin' ? 'مدیر سیستم' : user.role === 'operator' ? 'اپراتور' : 'ناظر'}</div></div></div>
        <form onSubmit={(event) => void changeName(event)} className="space-y-3"><div className="space-y-2"><Label htmlFor="account-username">نام کاربری جدید</Label><Input id="account-username" required minLength={3} maxLength={32} pattern="[A-Za-z0-9_-]{3,32}" dir="ltr" autoComplete="username" value={username} onChange={(event) => setUsername(event.target.value)} /></div><div className="space-y-2"><Label htmlFor="username-current-password">تأیید با گذرواژهٔ فعلی</Label><Input id="username-current-password" required type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></div><Button disabled={saving || username === user.username}>{saving ? 'در حال ذخیره…' : 'ذخیرهٔ نام کاربری'}</Button></form>
      </SectionCard>
      <div className="space-y-5"><PasswordCard notify={notify}/><SectionCard title="پایان نشست" description="از حساب کاربری در این مرورگر خارج شو." right={<LogOut size={18} className="text-muted-foreground" />}><Button variant="outline" onClick={logout}><LogOut size={15}/>خروج از حساب</Button></SectionCard></div>
    </div>
    <div className="mt-5"><SectionCard title="نشست‌های فعال" description="نشست‌ها فقط برای همین حساب نمایش داده می‌شوند؛ بستن نشست روی درخواست بعدی آن اعمال می‌شود." right={<div className="flex gap-2"><Button variant="ghost" size="icon" onClick={() => void loadSessions()} aria-label="بارگذاری دوبارهٔ نشست‌ها"><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/></Button><Button variant="outline" size="sm" disabled={loading || sessions.filter((session) => !session.current).length === 0} onClick={() => void revokeOthers()}>بستن همهٔ نشست‌های دیگر</Button></div>}>
      {sessionError ? <Alert className="border-rose-400/20 bg-rose-400/[.06] text-rose-100"><AlertDescription>{sessionError}</AlertDescription></Alert> : loading ? <Skeleton className="h-28"/> : !sessions.length ? <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">نشست فعالی پیدا نشد.</div> : <div className="overflow-x-auto"><table className="w-full min-w-[720px] text-right text-sm"><thead className="text-xs text-muted-foreground"><tr><th className="px-3 py-3 font-medium">دستگاه و نشانی</th><th className="px-3 py-3 font-medium">ایجاد</th><th className="px-3 py-3 font-medium">آخرین فعالیت</th><th className="px-3 py-3 font-medium">انقضا</th><th className="px-3 py-3"/></tr></thead><tbody>{sessions.map((session) => <tr key={session.id} className="border-t border-border/70"><td className="max-w-[360px] px-3 py-3"><div className="flex flex-wrap items-center gap-2"><span className="font-medium text-foreground">{session.user_agent || 'دستگاه نامشخص'}</span>{session.current && <Badge variant="success">نشست فعلی</Badge>}</div><div className="mt-1 font-mono text-xs text-muted-foreground" dir="ltr">{session.remote_ip || 'IP نامشخص'}</div></td><td className="px-3 py-3 text-xs text-muted-foreground" dir="ltr">{session.created_at ? formatPanelDate(session.created_at, timeZone) : 'نامشخص'}</td><td className="px-3 py-3 text-xs text-muted-foreground" dir="ltr">{session.last_seen ? formatPanelDate(session.last_seen, timeZone) : 'نامشخص'}</td><td className="px-3 py-3 text-xs text-muted-foreground" dir="ltr">{formatPanelDate(session.expires_at, timeZone)}</td><td className="px-3 py-3 text-left">{!session.current && <Button size="sm" variant="ghost" className="text-rose-200 hover:bg-rose-400/10" onClick={() => void revoke(session.id)}><X size={14}/>بستن</Button>}</td></tr>)}</tbody></table></div>}
    </SectionCard></div>
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
