import { useCallback, useEffect, useState, type CSSProperties, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Clock3,
  Cpu,
  Database,
  Download,
  Gauge,
  Globe,
  HardDrive,
  Info,
  Github,
  KeyRound,
  Network,
  Plus,
  Pencil,
  Power,
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

function useDashboard(notify: (message: string) => void) {
  const [data, setData] = useState<Dashboard | null>(null)
  const [samples, setSamples] = useState<StoredSample[]>([])
  const [historyEnabled, setHistoryEnabled] = useState(true)
  const [range, setRange] = useState<'5m' | '1h' | '24h'>('5m')
  const [loading, setLoading] = useState(true)
  const refresh = useCallback(async () => {
    try {
      const [next, history] = await Promise.all([api<Dashboard>('dashboard'), api<{ samples: StoredSample[]; config: MonitoringConfig }>(`history?range=${range}`)])
      setData(next)
      setSamples(history.samples)
      setHistoryEnabled(history.config.enabled)
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن اطلاعات سرور ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify, range])
  useEffect(() => {
    void refresh()
    const timer = window.setInterval(refresh, 5000)
    return () => window.clearInterval(timer)
  }, [refresh])
  const history = samples.map((sample) => ({ at: sample.at, time: new Date(sample.at * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' }), rx: sample.rx, tx: sample.tx }))
  const resourceHistory = samples.map(({ at, cpu, memory, disk }) => ({ at, cpu, memory, disk }))
  return { data, history, resourceHistory, cpu: samples.at(-1)?.cpu ?? null, loading, refresh, range, setRange, historyEnabled }
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

function AlertControls({ user, cpu, memory, disk, notify }: { user: User; cpu: number | null; memory: number; disk: number; notify: (message: string) => void }) {
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

function DashboardMetric({ icon: Icon, label, value, subtitle, progress, samples, tone }: {
  icon: typeof Cpu
  label: string
  value: string
  subtitle: string
  progress?: number
  samples: number[]
  tone: 'violet' | 'cyan' | 'amber' | 'blue'
}) {
  const colors = { violet: '#bd96ff', cyan: '#61dfd1', amber: '#f0b980', blue: '#85a8ff' }
  const color = colors[tone]
  const peak = Math.max(100, ...samples)
  const points = samples.length > 1 ? samples.map((sample, index) => `${index * 150 / (samples.length - 1)},${46 - Math.min(sample / peak, 1) * 38}`).join(' ') : ''
  return <div className="overview-metric group relative overflow-hidden rounded-[22px] border border-white/[.085] p-5 md:p-6" style={{ '--metric-color': color } as CSSProperties}>
    <div className="flex items-start justify-between gap-3"><span className="text-[11px] font-semibold text-[#a8a4b7]">{label}</span><span className="grid h-10 w-10 place-items-center rounded-xl border border-white/[.07] bg-white/[.045]" style={{ color }}><Icon size={19} strokeWidth={1.8}/></span></div>
    <div className="mt-6 text-[29px] font-extrabold leading-none tracking-tight text-white tabular-nums" dir="ltr">{value}</div>
    <div className="mt-2 text-[11px] text-[#8f90a6]">{subtitle}</div>
    <div className="mt-5 flex h-11 items-end gap-3"><div className="min-w-0 flex-1">{progress !== undefined && <div className="h-1.5 overflow-hidden rounded-full bg-white/[.075]"><div className="h-full rounded-full transition-all duration-500" style={{ width: `${Math.min(Math.max(progress, 0), 100)}%`, background: color, boxShadow: `0 0 12px ${color}80` }}/></div>}</div>{points && <svg viewBox="0 0 150 50" preserveAspectRatio="none" className="h-10 w-28 shrink-0 opacity-80" aria-hidden="true"><polyline points={points} fill="none" stroke={color} strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke"/></svg>}</div>
  </div>
}

export function DashboardPage({ user, notify }: { user: User } & Common) {
  const { data, history, resourceHistory, cpu, loading, refresh, range, setRange, historyEnabled } = useDashboard(notify)
  const memory = data?.memory_kib ? Math.round((1 - data.available_kib / data.memory_kib) * 100) : 0
  const disk = data?.disk_total_bytes ? Math.round((1 - (data.disk_free_bytes || 0) / data.disk_total_bytes) * 100) : 0
  const throughput = history.at(-1) ? history[history.length - 1].rx + history[history.length - 1].tx : 0
  const resourceValues = resourceHistory.length ? resourceHistory : [{ at: 0, cpu: 0, memory, disk }]
  return <div className="dashboard-overview space-y-5">
    <div className="mb-7 flex flex-wrap items-end justify-between gap-4"><div><div className="mb-3 flex items-center gap-2 text-[10px] font-bold uppercase tracking-[.2em] text-[#90e6d5]" dir="ltr"><span className="h-1.5 w-1.5 rounded-full bg-[#71e4cf] shadow-[0_0_12px_#71e4cf]"/> COMMAND CENTER / OVERVIEW</div><h1 className="text-3xl font-extrabold text-white md:text-[38px]">سرورت، در یک نگاه</h1><p className="mt-2 text-sm text-[#9996ab]">وضعیت زنده، تاریخچه و ابزارهای مدیریت در یک فضای روشن و منظم.</p></div><Button variant="outline" size="sm" onClick={refresh} className="h-10 rounded-xl border-white/10 bg-white/[.03]"><RefreshCw size={15}/> به‌روزرسانی</Button></div>

    <section className="overview-hero relative overflow-hidden rounded-[28px] border border-[#544276]/70 p-6 md:p-8 lg:p-9">
      <div className="relative z-10 grid gap-9 lg:grid-cols-[1.15fr_.85fr] lg:items-end"><div><div className="mb-5 flex flex-wrap items-center gap-2"><span className="inline-flex items-center gap-2 rounded-full border border-emerald-300/20 bg-emerald-300/[.07] px-3 py-1.5 text-[11px] font-semibold text-[#91e1c7]"><span className="h-1.5 w-1.5 rounded-full bg-[#75e7c1] shadow-[0_0_9px_#75e7c1]"/> سرویس فعال</span><span className="rounded-full border border-white/10 bg-white/[.04] px-3 py-1.5 text-[10px] font-bold tracking-wider text-[#bcb4ce]" dir="ltr">RUST · LIVE TELEMETRY</span></div><h2 className="max-w-xl text-[27px] font-extrabold leading-[1.6] text-white md:text-[38px]">سلام {user.username}،<br/><span className="text-[#c7a7ff]">کنترل سرور دست توست.</span></h2><p className="mt-3 text-sm leading-7 text-[#c1b9d3]">داده‌ها هر 5 ثانیه در خود سرویس ثبت می‌شوند؛ حتی وقتی این صفحه بسته باشد.</p><div className="mt-6 flex flex-wrap items-center gap-3"><span className="text-[11px] text-[#a7a0ba]">سرور</span><span className="rounded-lg border border-white/10 bg-black/20 px-3 py-1.5 font-mono text-xs text-white" dir="ltr">{data?.hostname || 'Loading...'}</span><span className="text-[11px] text-[#a7a0ba]">بار سیستم</span><span className="font-mono text-xs text-[#93e7d7]" dir="ltr">{data?.load || '—'}</span></div></div>
      <div className="grid grid-cols-2 gap-3"><div className="overview-hero-stat"><span className="text-[#ac9cc8]"><Clock3 size={18}/></span><div className="mt-5 text-[22px] font-bold text-white" dir="ltr">{`${formatNumber(Math.floor((data?.uptime_seconds || 0) / 86400))}d ${formatNumber(Math.floor(((data?.uptime_seconds || 0) % 86400) / 3600))}h`}</div><div className="mt-1 text-xs text-[#ad9fbe]">زمان روشن‌بودن</div></div><div className="overview-hero-stat"><span className="text-[#87dfd1]"><Network size={18}/></span><div className="mt-5 text-[22px] font-bold text-white tabular-nums" dir="ltr">{formatNumber(data?.interfaces.length || 0)}</div><div className="mt-1 text-xs text-[#ad9fbe]">کارت شبکه</div></div><div className="overview-hero-stat"><span className="text-[#d7b4ff]"><Radio size={18}/></span><div className="mt-5 text-[22px] font-bold text-white tabular-nums" dir="ltr">{formatNumber(data?.ports.length || 0)}</div><div className="mt-1 text-xs text-[#ad9fbe]">پورت شنونده</div></div><div className="overview-hero-stat"><span className="text-[#f0bf92]"><Gauge size={18}/></span><div className="mt-5 text-[22px] font-bold text-white tabular-nums" dir="ltr">{formatNumber(Math.round(data?.process_cpu_percent || 0))}%</div><div className="mt-1 text-xs text-[#ad9fbe]">CPU پنل</div></div></div></div>
      <div className="overview-hero-orbit pointer-events-none absolute -bottom-32 -left-20 h-[430px] w-[430px] rounded-full border border-white/[.065]"/><div className="overview-hero-orbit pointer-events-none absolute -bottom-20 -left-6 h-[300px] w-[300px] rounded-full border border-white/[.065]"/>
    </section>

    {loading && !data ? <LoadingCards/> : <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label="شاخص‌های سرور">
      <DashboardMetric icon={Cpu} tone="violet" label="پردازنده" value={cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`} subtitle="مصرف فعلی CPU" progress={cpu || 0} samples={resourceValues.map((s) => s.cpu)}/>
      <DashboardMetric icon={Database} tone="cyan" label="حافظه" value={`${formatNumber(memory)}%`} subtitle={`${formatBytes(((data?.memory_kib || 0) - (data?.available_kib || 0)) * 1024)} از ${formatBytes((data?.memory_kib || 0) * 1024)}`} progress={memory} samples={resourceValues.map((s) => s.memory)}/>
      <DashboardMetric icon={HardDrive} tone="amber" label="دیسک ریشه" value={`${formatNumber(disk)}%`} subtitle={data?.disk_total_bytes ? `${formatBytes(data.disk_total_bytes - (data.disk_free_bytes || 0))} از ${formatBytes(data.disk_total_bytes)}` : 'داده‌ای در دسترس نیست'} progress={disk} samples={resourceValues.map((s) => s.disk)}/>
      <DashboardMetric icon={Activity} tone="blue" label="ترافیک لحظه‌ای" value={`${formatBytes(throughput)}/s`} subtitle="مجموع دریافت و ارسال" samples={history.map((s) => s.rx + s.tx)}/>
    </section>}

    <section className="overview-section rounded-[24px] border border-white/[.08] p-5 md:p-7"><div className="mb-5 flex flex-wrap items-start justify-between gap-4"><div><div className="mb-2 flex items-center gap-2 text-[10px] font-bold tracking-[.16em] text-[#9ee4d6]" dir="ltr"><span className="h-1.5 w-1.5 rounded-full bg-[#69e2cc]"/> RESOURCE HISTORY</div><h2 className="text-xl font-bold text-white">روند مصرف منابع</h2><p className="mt-1 text-xs text-[#9492a6]">تاریخچهٔ ثبت‌شدهٔ CPU، حافظه و دیسک روی سرور</p></div><div className="flex gap-1 rounded-xl border border-white/10 bg-[#10111b] p-1" aria-label="بازه زمانی نمودار">{(['5m','1h','24h'] as const).map((item) => <button key={item} onClick={() => setRange(item)} aria-pressed={range === item} className={`min-w-12 rounded-lg px-3 py-2 text-xs font-semibold transition ${range === item ? 'bg-[#553d7d] text-white shadow-lg' : 'text-[#9690a9] hover:text-white'}`} dir="ltr">{item}</button>)}</div></div>{!historyEnabled && <HistoryPaused/>}<ResourceChart samples={resourceHistory}/></section>

    <div className="grid gap-5 xl:grid-cols-[1.4fr_1fr]"><section className="overview-section rounded-[24px] border border-white/[.08] p-5 md:p-7"><div className="mb-5"><h2 className="text-xl font-bold text-white">جریان شبکه</h2><p className="mt-1 text-xs text-[#9492a6]">نرخ دریافت و ارسال، با همان بازهٔ نمودار منابع</p></div>{!historyEnabled && <HistoryPaused/>}<TrafficChart history={history}/></section><section className="overview-section rounded-[24px] border border-white/[.08] p-5 md:p-7"><div className="mb-4 flex items-start justify-between"><div><h2 className="text-xl font-bold text-white">وضعیت کارت‌ها</h2><p className="mt-1 text-xs text-[#9492a6]">رابط‌های شبکهٔ شناسایی‌شده</p></div><Network size={18} className="text-[#8fdfd3]"/></div><div className="space-y-2">{data?.interfaces.slice(0, 5).map((item) => <div key={item.name} className="flex items-center justify-between gap-3 rounded-xl border border-white/[.07] bg-white/[.025] px-4 py-3"><div className="min-w-0"><div className="truncate font-mono text-sm text-white" dir="ltr">{item.name}</div><div className="mt-1 truncate font-mono text-[10px] text-[#89899e]" dir="ltr">{item.addresses?.join(' · ') || item.mac}</div></div><span className={`h-2 w-2 shrink-0 rounded-full ${item.state === 'up' ? 'bg-[#6bdfbf] shadow-[0_0_10px_#6bdfbf]' : 'bg-[#77758e]'}`}/></div>)}{!data?.interfaces.length && <p className="py-9 text-center text-xs text-muted-foreground">کارت شبکه‌ای پیدا نشد.</p>}</div></section></div>

    <div className="grid gap-5 xl:grid-cols-[1fr_1fr]"><section className="overview-section rounded-[24px] border border-white/[.08] p-5 md:p-7"><div className="mb-4"><h2 className="text-xl font-bold text-white">مصرف خود Donatello</h2><p className="mt-1 text-xs text-[#9492a6]">اندازه‌گیری مستقیم فرایند پنل</p></div><div className="grid grid-cols-2 gap-3"><div className="rounded-xl border border-white/[.07] bg-white/[.025] p-4"><span className="text-xs text-muted-foreground">CPU</span><div className="mt-3 text-2xl font-bold text-white tabular-nums" dir="ltr">{formatNumber(Math.round((data?.process_cpu_percent || 0) * 10) / 10)}%</div></div><div className="rounded-xl border border-white/[.07] bg-white/[.025] p-4"><span className="text-xs text-muted-foreground">RAM</span><div className="mt-3 text-2xl font-bold text-white tabular-nums" dir="ltr">{formatBytes(data?.process_rss_bytes || 0)}</div></div></div><p className="mt-4 text-[11px] leading-6 text-[#8f8da1]">RAM برابر حافظهٔ مقیم فرایند است؛ CPU نسبت به توان کل هسته‌ها محاسبه می‌شود.</p></section><section className="overview-section rounded-[24px] border border-white/[.08] p-5 md:p-7"><div className="mb-4"><h2 className="text-xl font-bold text-white">پورت‌های فعال</h2><p className="mt-1 text-xs text-[#9492a6]">نمونه‌ای از شنونده‌های TCP و UDP</p></div><div className="flex flex-wrap gap-2">{data?.ports.slice(0, 12).map((port, index) => <span key={`${port.port}-${port.protocol}-${index}`} className="rounded-lg border border-white/10 bg-white/[.035] px-3 py-2 font-mono text-xs text-[#c8bfd9]" dir="ltr">{port.port}<span className="text-[#8e85a2]">/{port.protocol.toLowerCase()}</span></span>)}{!data?.ports.length && <p className="py-5 text-xs text-muted-foreground">پورت شنونده‌ای پیدا نشد.</p>}</div></section></div>
    <AlertControls user={user} cpu={cpu} memory={memory} disk={disk} notify={notify}/>
  </div>
}

function DataRows({ rows }: { rows: { label: string; value: string; detail?: string }[] }) {
  return <Table><TableHeader><TableRow><TableHead>شاخص</TableHead><TableHead>مقدار</TableHead><TableHead className="hidden md:table-cell">توضیح</TableHead></TableRow></TableHeader><TableBody>{rows.map((row) => <TableRow key={row.label}><TableCell className="font-medium">{row.label}</TableCell><TableCell><span className="font-mono font-semibold text-white tabular-nums" dir="ltr">{row.value}</span></TableCell><TableCell className="hidden text-xs text-muted-foreground md:table-cell">{row.detail || '—'}</TableCell></TableRow>)}</TableBody></Table>
}

export function SystemPage({ user, Heading, notify }: { user: User } & Common) {
  const { data, resourceHistory, cpu, range, setRange, loading, refresh, historyEnabled } = useDashboard(notify)
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
      <div className="mt-5"><SectionCard title="تاریخچهٔ منابع" description="نمونه‌ها توسط سرویس Rust ذخیره می‌شوند" right={<div className="flex gap-1 rounded-lg border border-white/10 p-1">{(['5m','1h','24h'] as const).map((item) => <button key={item} onClick={() => setRange(item)} aria-pressed={range === item} className={`rounded-md px-3 py-1.5 text-xs ${range === item ? 'bg-primary/25 text-white' : 'text-muted-foreground'}`} dir="ltr">{item}</button>)}</div>}>{!historyEnabled && <HistoryPaused/>}<ResourceChart samples={resourceHistory}/></SectionCard></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="پردازنده و بار سیستم" description="مقادیر لحظه‌ای و میانگین بار Linux"><DataRows rows={[{ label: 'مصرف CPU', value: cpu === null ? '—' : `${formatNumber(Math.round(cpu))}%`, detail: 'میانگین هسته‌ها' }, { label: 'هسته‌های در دسترس', value: formatNumber(data?.cpu_cores || 0) }, { label: 'بار 1 دقیقه', value: load[0] || '—' }, { label: 'بار 5 دقیقه', value: load[1] || '—' }, { label: 'بار 15 دقیقه', value: load[2] || '—' }, { label: 'زمان روشن‌بودن', value: `${formatNumber(Math.floor((data?.uptime_seconds || 0) / 86400))}d ${formatNumber(Math.floor(((data?.uptime_seconds || 0) % 86400) / 3600))}h` }]}/></SectionCard><SectionCard title="حافظه و Swap" description="مقادیر خوانده‌شده از /proc/meminfo"><DataRows rows={[{ label: 'کل RAM', value: formatBytes(totalMemory) }, { label: 'مصرف‌شده', value: formatBytes(totalMemory - availableMemory), detail: 'بر اساس MemAvailable' }, { label: 'در دسترس', value: formatBytes(availableMemory) }, { label: 'Cache', value: formatBytes((memoryDetail?.cached_kib || 0) * 1024) }, { label: 'Buffer', value: formatBytes((memoryDetail?.buffers_kib || 0) * 1024) }, { label: 'Swap کل', value: formatBytes((data?.swap_total_kib || 0) * 1024) }, { label: 'Swap مصرف‌شده', value: formatBytes(((data?.swap_total_kib || 0) - (data?.swap_free_kib || 0)) * 1024) }]}/></SectionCard></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-2"><SectionCard title="دیسک ریشه /" description="فضای فایل‌سیستم ریشه"><DataRows rows={[{ label: 'ظرفیت کل', value: totalDisk ? formatBytes(totalDisk) : '—' }, { label: 'مصرف‌شده', value: totalDisk ? formatBytes(totalDisk - freeDisk) : '—' }, { label: 'آزاد', value: totalDisk ? formatBytes(freeDisk) : '—' }, { label: 'درصد مصرف', value: totalDisk ? `${formatNumber(diskPercent)}%` : '—' }]}/></SectionCard><SectionCard title="مصرف Donatello" description="اندازه‌گیری همین فرایند"><DataRows rows={[{ label: 'CPU پنل', value: `${formatNumber(Math.round((data?.process_cpu_percent || 0) * 10) / 10)}%` }, { label: 'RAM پنل', value: formatBytes(data?.process_rss_bytes || 0), detail: 'حافظهٔ مقیم فرایند (RSS)' }]}/></SectionCard></div>
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
      <div className="mb-5 space-y-3 rounded-2xl border border-white/[.08] bg-[#11131e] p-3">
        <label className="relative block"><span className="sr-only">جست‌وجوی شمارهٔ پورت</span><Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="pr-9" inputMode="numeric" lang="en" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="شمارهٔ پورت را جست‌وجو کن…"/></label>
        <div className="flex flex-wrap items-center gap-3"><div className="flex flex-wrap gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1" aria-label="فیلتر پروتکل">{(['all','TCP','UDP'] as const).map((value) => <button key={value} onClick={() => setProtocol(value)} aria-pressed={protocol === value} className={`rounded-lg px-3 py-1.5 text-xs transition ${protocol === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:text-white'}`}>{value === 'all' ? 'همهٔ پروتکل‌ها' : value}</button>)}</div><div className="flex flex-wrap gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1" aria-label="فیلتر نسخهٔ IP">{(['all','IPv4','IPv6'] as const).map((value) => <button key={value} onClick={() => setIpVersion(value)} aria-pressed={ipVersion === value} className={`rounded-lg px-3 py-1.5 text-xs transition ${ipVersion === value ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:text-white'}`}>{value === 'all' ? 'همهٔ IPها' : value}</button>)}</div>{(query || protocol !== 'all' || ipVersion !== 'all') && <button className="text-xs text-[#bda0ec] hover:text-white" onClick={() => { setQuery(''); setProtocol('all'); setIpVersion('all') }}>پاک‌کردن فیلترها</button>}</div>
      </div>
      {loading && !data ? <Skeleton className="h-48"/> : <PortsTable ports={ports}/>}
    </SectionCard></div>
    <Alert className="mt-5"><Info size={17} className="mb-2 text-primary"/><AlertTitle>شنود محلی و دسترسی بیرونی</AlertTitle><AlertDescription>وجود پورت در این فهرست به معنی دسترسی از اینترنت نیست. قانون UFW و فایروال ارائه‌دهنده را نیز بررسی کن. حجم ترافیک به‌تفکیک پورت هنوز اندازه‌گیری نمی‌شود.</AlertDescription></Alert>
  </>
}

function HistoryPaused() { return <div className="mb-4 flex items-center gap-2 rounded-xl border border-amber-400/20 bg-amber-400/[.06] px-4 py-3 text-xs text-amber-200"><span className="h-2 w-2 rounded-full bg-amber-400"/> ثبت تاریخچه متوقف است. برای ادامه، از تنظیمات پنل آن را فعال کن.</div> }

function ResourceChart({ samples }: { samples: ResourceSample[] }) {
  return <TimeSeriesChart samples={samples} series={[{ key: 'cpu', label: 'پردازنده', color: '#be9bff' }, { key: 'memory', label: 'حافظه', color: '#55d8cb' }, { key: 'disk', label: 'دیسک', color: '#f1bb8a' }]} max={100} unit="%" />
}

type ChartSeries = { key: string; label: string; color: string }
function TimeSeriesChart({ samples, series, max, unit }: { samples: (ResourceSample | Sample)[]; series: ChartSeries[]; max: number; unit: string }) {
  const [hover, setHover] = useState<number | null>(null)
  if (!samples.length) return <div className="grid h-[290px] place-items-center rounded-2xl border border-dashed border-white/10 bg-[#10111a] p-6 text-center text-sm text-muted-foreground">داده‌ای در این بازه ثبت نشده است.</div>
  const width = 760, left = 50, right = 15, top = 20, bottom = 235
  const x = (index: number) => left + index * (width - left - right) / Math.max(samples.length - 1, 1)
  const y = (value: number) => bottom - Math.min(Math.max(value / max, 0), 1) * (bottom - top)
  const selected = hover === null ? samples.length - 1 : Math.max(0, Math.min(hover, samples.length - 1))
  const current = samples[selected]
  const read = (sample: ResourceSample | Sample, key: string) => Number((sample as unknown as Record<string, number>)[key]) || 0
  const display = (value: number) => unit === '%' ? `${formatNumber(Math.round(value * 10) / 10)}%` : `${formatBytes(value)}/s`
  const stamp = (at: number) => new Date(at * 1000).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })
  return <div className="overflow-hidden rounded-2xl border border-white/[.075] bg-[#10121d]">
    <div className="flex flex-wrap items-center gap-2 border-b border-white/[.065] px-4 py-3 md:px-5">{series.map((item) => <div key={item.key} className="flex items-center gap-2 rounded-lg bg-white/[.025] px-2.5 py-1.5 text-[11px]"><span className="h-1.5 w-1.5 rounded-full" style={{ background: item.color, boxShadow: `0 0 9px ${item.color}` }}/><span className="text-[#aaa7b9]">{item.label}</span><b className="font-mono font-semibold text-white tabular-nums" dir="ltr">{display(read(current, item.key))}</b></div>)}<span className="mr-auto rounded-lg border border-white/[.065] px-2 py-1 font-mono text-[10px] text-[#9895aa]" dir="ltr">{stamp(current.at)}</span></div>
    <div className="h-[255px] px-3 pt-3 md:h-[315px] md:px-5" dir="ltr"><svg viewBox="0 0 760 276" width="100%" height="100%" preserveAspectRatio="none" onMouseLeave={() => setHover(null)} onMouseMove={(event) => { const rect = event.currentTarget.getBoundingClientRect(); setHover(Math.round(((event.clientX - rect.left) / rect.width * width - left) / (width - left - right) * (samples.length - 1))) }} role="img" aria-label={`تاریخچهٔ ${series.map((item) => item.label).join('، ')}`}>
      <defs><linearGradient id={`overview-fill-${series[0].key}`} x1="0" y1="0" x2="0" y2="1"><stop stopColor={series[0].color} stopOpacity=".24"/><stop offset="1" stopColor={series[0].color} stopOpacity="0"/></linearGradient></defs>
      {[0, .25, .5, .75, 1].map((fraction) => { const py = y(fraction * max); return <g key={fraction}><line x1={left} x2={width-right} y1={py} y2={py} stroke="#38394c" strokeOpacity=".65" strokeDasharray="2 7"/><text x={left-11} y={py+4} fill="#818095" fontSize="10" textAnchor="end">{unit === '%' ? `${Math.round(fraction * max)}%` : formatBytes(fraction * max)}</text></g> })}
      {series.map((item, lineIndex) => { const points = samples.map((sample, index) => `${x(index)},${y(read(sample, item.key))}`).join(' '); return <g key={item.key}>{lineIndex === 0 && <polygon points={`${left},${bottom} ${points} ${x(samples.length - 1)},${bottom}`} fill={`url(#overview-fill-${item.key})`}/>}<polyline points={points} fill="none" stroke={item.color} strokeOpacity={lineIndex === 0 ? 1 : .86} strokeWidth={lineIndex === 0 ? 3 : 2.4} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke"/></g> })}
      <line x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} stroke="#c6b9df" strokeOpacity=".42" strokeDasharray="3 6"/>
      {series.map((item) => <circle key={item.key} cx={x(selected)} cy={y(read(current, item.key))} r="4.8" fill={item.color} stroke="#10121d" strokeWidth="2"/>)}
      <text x={left} y="266" fill="#8d8ca2" fontSize="11">{stamp(samples[0].at)}</text><text x={width-right} y="266" textAnchor="end" fill="#8d8ca2" fontSize="11">{stamp(samples[samples.length - 1].at)}</text>
    </svg></div>
  </div>
}

function DistributionBars({ title, description, items }: {
  title: string
  description: string
  items: { label: string; value: number; display?: string }[]
}) {
  const max = Math.max(1, ...items.map((item) => item.value))
  return <SectionCard title={title} description={description} right={<Activity size={18} className="text-primary" />}>
    {items.length ? <div className="grid gap-4 sm:grid-cols-2">
      {items.map((item, index) => <div key={`${item.label}-${index}`}>
        <div className="mb-2 flex items-center justify-between gap-2 text-xs"><span className="truncate text-muted-foreground">{item.label}</span><span className="font-mono text-foreground">{item.display || formatNumber(item.value)}</span></div>
        <div className="h-2 overflow-hidden rounded-full bg-white/5"><div className="h-full rounded-full" style={{ width: `${item.value ? Math.max(3, item.value / max * 100) : 0}%`, background: index % 2 ? '#59d3bc' : '#a987ff' }}/></div>
      </div>)}
    </div> : <p className="py-6 text-center text-sm text-muted-foreground">داده‌ای برای نمایش نمودار وجود ندارد.</p>}
  </SectionCard>
}

function TrafficChart({ history }: { history: Sample[] }) {
  return <TimeSeriesChart samples={history} series={[{ key: 'rx', label: 'دریافت', color: '#be9bff' }, { key: 'tx', label: 'ارسال', color: '#55d8cb' }]} max={Math.max(1024, ...history.flatMap((sample) => [sample.rx, sample.tx])) * 1.12} unit="bytes" />
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
  const { data, history, loading, refresh, range, setRange, historyEnabled } = useDashboard(notify)
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
          <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
            {data?.interfaces.map((item) => (
              <Card key={item.name} className="metric-card">
                <CardHeader className="flex-row items-start justify-between space-y-0">
                  <div>
                    <CardTitle dir="ltr" className="text-lg">
                      {item.name}
                    </CardTitle>
                    <CardDescription className="mt-1 mono text-xs">{item.mac}</CardDescription>
                  </div>
                  <Badge variant={item.state === 'up' ? 'success' : 'outline'}>
                    {item.state === 'up' ? 'فعال' : item.state || 'نامشخص'}
                  </Badge>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3 text-sm">
                    <div className="flex items-center justify-between text-muted-foreground">
                      <span className="flex items-center gap-2">
                        <ArrowDownLeft size={16} className="text-primary" /> دریافت کل
                      </span>
                      <strong className="text-foreground">{formatBytes(item.rx_bytes)}</strong>
                    </div>
                    <div className="flex items-center justify-between text-muted-foreground">
                      <span className="flex items-center gap-2">
                        <ArrowUpRight size={16} className="text-emerald-300" /> ارسال کل
                      </span>
                      <strong className="text-foreground">{formatBytes(item.tx_bytes)}</strong>
                    </div>
                    <div className="border-t border-border pt-3 text-xs text-muted-foreground">
                      آدرس‌ها:{' '}
                      <span className="mono text-foreground">{item.addresses?.join(', ') || 'نامشخص'}</span>
                    </div>
                  </div>
                </CardContent>
              </Card>
            ))}
            {!data?.interfaces.length && (
              <Card>
                <CardContent className="p-8 text-center text-muted-foreground">
                  کارت شبکه‌ای یافت نشد.
                </CardContent>
              </Card>
            )}
          </div>
          <div className="mt-5">
            <DistributionBars title="توزیع ترافیک کارت‌ها" description="جمع بایت‌های دریافت و ارسال هر رابط از زمان شروع آن" items={(data?.interfaces || []).map((item) => ({ label: item.name, value: item.rx_bytes + item.tx_bytes, display: formatBytes(item.rx_bytes + item.tx_bytes) }))} />
          </div>
          <div className="mt-5"><SectionCard title="نرخ ترافیک شبکه" description="نمونه‌های ذخیره‌شده توسط سرویس" right={<div className="flex gap-1 rounded-lg border border-white/10 p-1">{(['5m','1h','24h'] as const).map((item) => <button key={item} onClick={() => setRange(item)} aria-pressed={range === item} className={`rounded-md px-3 py-1.5 text-xs ${range === item ? 'bg-primary/25 text-white' : 'text-muted-foreground'}`} dir="ltr">{item}</button>)}</div>}>{!historyEnabled && <HistoryPaused/>}<TrafficChart history={history}/></SectionCard></div>
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
    <Heading title="فایروال UFW" description="قوانین Donatello را بساز، ویرایش کن یا موقتاً غیرفعال کن." action={<div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button>{user.role === 'admin' && <Button size="sm" onClick={() => openEditor()}><Plus size={15}/> قانون جدید</Button>}</div>}/>
    <div className="mb-5 grid gap-3 sm:grid-cols-3"><div className="rounded-2xl border border-white/10 bg-[#171824] p-5"><div className="text-xs text-muted-foreground">وضعیت UFW</div><div className="mt-3 flex items-center gap-2 text-xl font-bold text-white"><span className={`h-2 w-2 rounded-full ${active ? 'bg-emerald-400' : 'bg-amber-400'}`}/>{loading ? 'در حال بررسی' : data?.available ? active ? 'فعال' : 'غیرفعال' : 'نصب نشده'}</div>{user.role === 'admin' && data?.available && <div className="mt-4 flex items-center gap-2 border-t border-white/10 pt-3 text-xs text-muted-foreground"><Switch checked={!!active} disabled={busy} onCheckedChange={() => void toggleFirewall()} aria-label="تغییر وضعیت UFW"/> تغییر وضعیت کلی UFW</div>}{user.role === 'admin' && !data?.available && <div className="mt-4 border-t border-white/10 pt-3"><Button size="sm" disabled={busy} onClick={() => void installFirewall()}>{busy ? 'در حال نصب…' : 'نصب UFW'}</Button><p className="mt-2 text-[11px] leading-5 text-amber-200/80">{data?.error || 'UFW روی این سرور نصب نشده است.'}</p></div>}</div><div className="rounded-2xl border border-white/10 bg-[#171824] p-5"><div className="text-xs text-muted-foreground">قوانین فعال Donatello</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => rule.enabled).length)}</div></div><div className="rounded-2xl border border-white/10 bg-[#171824] p-5"><div className="text-xs text-muted-foreground">قوانین غیرفعال</div><div className="mt-3 text-2xl font-bold text-white" dir="ltr">{formatNumber(rules.filter((rule) => !rule.enabled).length)}</div></div></div>
    <SectionCard title="قوانین مدیریت‌شده" description="تغییر وضعیت و ویرایش فقط روی قوانین ساخته‌شده توسط Donatello اعمال می‌شود" right={<Badge variant="outline">{formatNumber(filtered.length)} قانون</Badge>}>
      <div className="mb-5 flex flex-wrap items-center gap-3"><label className="relative min-w-[180px] flex-1"><span className="sr-only">جست‌وجوی پورت</span><Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input lang="en" inputMode="numeric" value={query} onChange={(event) => setQuery(event.target.value.replace(/[^0-9]/g, ''))} placeholder="جست‌وجوی شمارهٔ پورت" className="pr-9"/></label><div className="flex gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['all','همه'], ['enabled','فعال'], ['disabled','غیرفعال']] as const).map(([value, label]) => <button key={value} onClick={() => setStatusFilter(value)} aria-pressed={statusFilter === value} className={`rounded-lg px-3 py-2 text-xs transition ${statusFilter === value ? 'bg-primary/20 text-white' : 'text-muted-foreground hover:text-white'}`}>{label}</button>)}</div></div>
      {loading ? <Skeleton className="h-48"/> : filtered.length ? <div className="overflow-x-auto"><Table><TableHeader><TableRow><TableHead>پورت</TableHead><TableHead>پروتکل</TableHead><TableHead>رفتار</TableHead><TableHead>وضعیت</TableHead>{user.role === 'admin' && <TableHead>مدیریت</TableHead>}</TableRow></TableHeader><TableBody>{filtered.map((rule) => <TableRow key={rule.id}><TableCell className="font-mono text-white" dir="ltr">{rule.port}</TableCell><TableCell className="font-mono uppercase">{rule.protocol}</TableCell><TableCell><Badge variant={rule.policy === 'allow' ? 'success' : 'warning'}>{rule.policy === 'allow' ? 'اجازه' : 'مسدود'}</Badge></TableCell><TableCell>{user.role === 'admin' ? <div className="flex items-center gap-2"><Switch checked={rule.enabled} disabled={busy} onCheckedChange={() => void toggle(rule)} aria-label={`${rule.enabled ? 'غیرفعال‌کردن' : 'فعال‌کردن'} قانون ${rule.port}`}/><span className="text-xs text-muted-foreground">{rule.enabled ? 'فعال' : 'غیرفعال'}</span></div> : <Badge variant={rule.enabled ? 'success' : 'outline'}>{rule.enabled ? 'فعال' : 'غیرفعال'}</Badge>}</TableCell>{user.role === 'admin' && <TableCell><div className="flex gap-1"><Button variant="ghost" size="icon" disabled={busy} onClick={() => openEditor(rule)} aria-label={`ویرایش قانون ${rule.port}`}><Pencil size={15}/></Button><Button variant="ghost" size="icon" disabled={busy} onClick={() => void remove(rule)} aria-label={`حذف قانون ${rule.port}`}><Trash2 size={15}/></Button></div></TableCell>}</TableRow>)}</TableBody></Table></div> : <div className="rounded-xl border border-dashed border-white/10 py-12 text-center text-sm text-muted-foreground">{query || statusFilter !== 'all' ? 'قانونی مطابق فیلتر پیدا نشد.' : 'هنوز قانونی با Donatello ساخته نشده است.'}</div>}
    </SectionCard>
    <details className="mt-5 rounded-2xl border border-white/10 bg-[#14151f] p-5"><summary className="cursor-pointer text-sm font-semibold text-white">همهٔ قوانین UFW و خروجی خام</summary><div className="mt-4 flex justify-end"><Button variant="outline" size="sm" disabled={!data?.status} onClick={() => downloadText('donatello-ufw.txt', data?.status || '')}><Download size={14}/> دریافت خروجی</Button></div><pre className="mt-3 max-h-[480px] overflow-auto whitespace-pre-wrap rounded-xl bg-[#0e101a] p-4 text-left font-mono text-xs leading-6 text-muted-foreground" dir="ltr">{data?.status || data?.error || 'UFW unavailable'}</pre></details>
    <Alert className="mt-5"><ShieldCheck size={17} className="mb-2 text-primary"/><AlertTitle>حفاظت دسترسی</AlertTitle><AlertDescription>پورت‌های SSH، HTTP، HTTPS و پورت‌های پنل از ویرایش در این بخش محافظت می‌شوند. هنگام فعال‌کردن UFW، دسترسی SSH و پنل ابتدا مجاز می‌شود.</AlertDescription></Alert>
    <Dialog open={dialogOpen} onOpenChange={setDialogOpen}><DialogContent><DialogHeader><DialogTitle>{editing === null ? 'قانون جدید' : 'ویرایش قانون'}</DialogTitle><DialogDescription>قانون در UFW اعمال می‌شود و می‌توانی بعداً آن را غیرفعال کنی.</DialogDescription></DialogHeader><form onSubmit={save} className="space-y-4"><div className="grid grid-cols-2 gap-3"><div className="space-y-2"><Label htmlFor="firewall-port">پورت</Label><Input id="firewall-port" type="number" lang="en" dir="ltr" min={1} max={65535} required value={form.port} onChange={(event) => setForm((old) => ({ ...old, port: event.target.value }))}/></div><div className="space-y-2"><Label>پروتکل</Label><div role="group" aria-label="انتخاب پروتکل شبکه" className="grid grid-cols-3 gap-1 rounded-xl border border-white/10 bg-white/[.025] p-1">{([['tcp','TCP'], ['udp','UDP'], ['both','TCP + UDP']] as const).map(([value,label]) => <button key={value} type="button" aria-pressed={form.protocol === value} onClick={() => setForm((old) => ({ ...old, protocol: value }))} className={`rounded-lg px-2 py-2.5 text-xs font-mono font-semibold transition ${form.protocol === value ? 'bg-primary/25 text-white shadow-sm' : 'text-muted-foreground hover:bg-white/[.04] hover:text-white'}`}>{label}</button>)}</div><p className="text-[11px] text-muted-foreground">{form.protocol === 'both' ? 'دو قانون جداگانه برای TCP و UDP ساخته می‌شود.' : `فقط ترافیک ${form.protocol.toUpperCase()} برای این پورت مدیریت می‌شود.`}</p></div></div><div className="space-y-2"><Label htmlFor="firewall-policy">رفتار</Label><Select value={form.policy} onValueChange={(value) => setForm((old) => ({ ...old, policy: value as 'allow' | 'deny' }))}><SelectTrigger id="firewall-policy"><SelectValue/></SelectTrigger><SelectContent><SelectItem value="allow">اجازه</SelectItem><SelectItem value="deny">مسدود</SelectItem></SelectContent></Select></div><label className="flex items-center justify-between rounded-xl border border-white/10 p-3 text-sm"><span>قانون فعال باشد</span><Switch checked={form.enabled} onCheckedChange={(checked) => setForm((old) => ({ ...old, enabled: checked }))}/></label><Button type="submit" disabled={busy || !form.port} className="w-full">{busy ? 'در حال ذخیره…' : 'ذخیرهٔ قانون'}</Button></form></DialogContent></Dialog>
  </>
}

type NginxSite = { name: string; enabled: boolean; bytes?: number; content?: string; message?: string }
export function NginxPage({ Heading, notify }: Common) {
  const [sites, setSites] = useState<NginxSite[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [loadError, setLoadError] = useState('')
  const [editing, setEditing] = useState<string | null>(null)
  const [content, setContent] = useState('')
  const [editorOpen, setEditorOpen] = useState(false)

  const refresh = useCallback(async () => {
    try {
      const result = await api<{ sites: NginxSite[] }>('nginx/sites')
      setSites(result.sites)
      setLoadError('')
    } catch (error) {
      const message = error instanceof Error ? error.message : 'خواندن سایت‌های Nginx ناموفق بود.'
      setLoadError(message)
    } finally { setLoading(false) }
  }, [])
  useEffect(() => { void refresh() }, [refresh])

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
    <Heading title="مدیریت سایت‌های Nginx" description="سایت‌ها را فعال یا غیرفعال کن، symlink را خودکار بساز و تنظیمات را با اعتبارسنجی ویرایش کن." action={<Button variant="outline" size="sm" disabled={loading} onClick={() => { setLoading(true); void refresh() }}><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/> تازه‌سازی</Button>}/>
    <div className="mb-5 grid gap-3 sm:grid-cols-3">
      <Metric icon={Globe} label="همهٔ سایت‌ها" value={formatNumber(sites.length)} detail="فایل‌های معتبر sites-available"/>
      <Metric icon={Power} label="فعال" value={formatNumber(enabledCount)} detail="symlink فعال در sites-enabled"/>
      <Metric icon={ShieldCheck} label="غیرفعال" value={formatNumber(sites.length - enabledCount)} detail="آمادهٔ فعال‌سازی با یک کلیک"/>
    </div>
    {loadError && <Alert className="mb-5 border-amber-300/20 bg-amber-300/[.06]"><AlertTitle>فهرست سایت‌ها در دسترس نیست</AlertTitle><AlertDescription className="leading-6">{loadError}<span className="mt-1 block">برای مدیریت سایت‌ها، Nginx و پوشه‌های استاندارد <code dir="ltr">sites-available</code> و <code dir="ltr">sites-enabled</code> باید موجود باشند.</span></AlertDescription></Alert>}
    <SectionCard title="سایت‌های موجود" description="فعال‌سازی، symlink استاندارد را می‌سازد؛ خاموش‌کردن فقط symlink همان سایت را حذف می‌کند." right={<Badge variant="outline">{formatNumber(sites.length)} سایت</Badge>}>
      {loading ? <Skeleton className="h-48"/> : sites.length ? <div className="grid gap-3 xl:grid-cols-2">{sites.map((site) => <article key={site.name} className="rounded-2xl border border-white/10 bg-white/[.025] p-4 transition hover:border-primary/25 hover:bg-white/[.04]">
        <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><Globe size={16} className="shrink-0 text-primary"/><h2 className="truncate font-mono text-sm font-semibold text-white" dir="ltr">{site.name}</h2></div><p className="mt-2 font-mono text-[11px] text-muted-foreground" dir="ltr">/etc/nginx/sites-available/{site.name}</p><p className="mt-1 text-[11px] text-muted-foreground" dir="ltr">{site.enabled ? `symlink → ../sites-available/${site.name}` : 'بدون symlink فعال'}</p></div><Badge variant={site.enabled ? 'success' : 'outline'}>{site.enabled ? 'روشن' : 'خاموش'}</Badge></div>
        <div className="mt-4 flex flex-wrap gap-2 border-t border-white/[.07] pt-3"><Button size="sm" variant={site.enabled ? 'outline' : 'default'} disabled={busy} onClick={() => void toggleSite(site)}><Power size={14}/>{site.enabled ? 'خاموش‌کردن سایت' : 'روشن‌کردن و ساخت symlink'}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void editSite(site)}><Pencil size={14}/>ویرایش تنظیمات</Button>{typeof site.bytes === 'number' && <span className="mr-auto self-center font-mono text-[10px] text-muted-foreground" dir="ltr">{formatNumber(site.bytes)} B</span>}</div>
      </article>)}</div> : !loadError ? <div className="rounded-xl border border-dashed border-white/10 py-14 text-center"><Globe size={22} className="mx-auto text-muted-foreground"/><p className="mt-3 text-sm font-medium text-white">سایتی پیدا نشد</p><p className="mt-1 text-xs text-muted-foreground">فایل‌های پیکربندی Nginx در مسیر sites-available نمایش داده می‌شوند.</p></div> : null}
    </SectionCard>
    <Alert className="mt-5"><ShieldCheck size={16} className="mb-2 text-primary"/><AlertTitle>اعمال ایمن تنظیمات</AlertTitle><AlertDescription>قبل از ذخیره یا تغییر symlink، دستور <code dir="ltr">nginx -t</code> اجرا می‌شود. اگر تنظیمات معتبر نباشند، تغییر برگردانده می‌شود؛ در صورت روشن‌بودن سرویس، پس از اعتبارسنجی Nginx reload خواهد شد.</AlertDescription></Alert>
    <Dialog open={editorOpen} onOpenChange={setEditorOpen}><DialogContent className="max-h-[92vh] max-w-5xl overflow-y-auto"><DialogHeader><DialogTitle>ویرایش سایت <span className="font-mono text-primary" dir="ltr">{editing}</span></DialogTitle><DialogDescription>مسیر فایل: <code dir="ltr">/etc/nginx/sites-available/{editing}</code>. ذخیره فقط پس از موفقیت <code dir="ltr">nginx -t</code> اعمال می‌شود.</DialogDescription></DialogHeader><form onSubmit={saveSite} className="space-y-4"><textarea aria-label="تنظیمات Nginx" spellCheck={false} dir="ltr" value={content} onChange={(event) => setContent(event.target.value)} className="min-h-[55vh] w-full resize-y rounded-xl border border-white/10 bg-[#0b0c12] p-4 font-mono text-xs leading-6 text-[#e3d8f5] outline-none transition focus:border-primary/50 focus:ring-2 focus:ring-primary/15"/><div className="flex flex-wrap items-center justify-between gap-3"><p className="text-xs text-muted-foreground">سقف اندازهٔ فایل 512 KB است؛ در صورت خطا نسخهٔ قبلی حفظ می‌شود.</p><Button type="submit" disabled={busy}><Save size={15}/>{busy ? 'در حال اعتبارسنجی…' : 'اعتبارسنجی و ذخیره'}</Button></div></form></DialogContent></Dialog>
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
          <div className="flex gap-2"><Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button><Button variant="outline" size="sm" disabled={!events.length} onClick={() => { const rows = [['at','actor','action','detail'], ...events.map((event) => [new Date(event.at * 1000).toISOString(), event.actor, event.action, event.detail])]; downloadText('donatello-audit.csv', '\uFEFF' + rows.map((row) => row.map((cell) => `"${String(cell).replaceAll('"', '""')}"`).join(',')).join('\n'), 'text/csv;charset=utf-8') }}><Download size={15}/> دریافت CSV</Button></div>
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
export function SettingsPage({ Heading, logout, notify, user, update, checkUpdate, runUpdate, updating }: { Heading: Heading; logout: () => void; notify: (message: string) => void; user: User; update: UpdateInfo; checkUpdate: () => Promise<void>; runUpdate: () => Promise<void>; updating: boolean }) {
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
          <label className="flex items-center justify-between gap-4 rounded-2xl border border-primary/20 bg-primary/[.06] p-4"><span><span className="block text-sm font-semibold text-white">ثبت داده‌های CPU، حافظه، دیسک و شبکه</span><span className="mt-1 block text-xs text-muted-foreground">در صورت غیرفعال‌کردن، داده‌های قبلی تا پایان مدت نگه‌داری باقی می‌مانند.</span></span><Switch disabled={!isAdmin} checked={config.enabled} onCheckedChange={(checked) => setConfig((old) => old && { ...old, enabled: checked })}/></label>
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
    <div className="mt-5"><SectionCard title="درباره و به‌روزرسانی" description="نسخهٔ Donatello، وضعیت مخزن و تغییرات اخیر" right={<Badge variant={updating ? 'warning' : update.status === 'available' ? 'warning' : update.status === 'current' ? 'success' : 'outline'}>{updating ? 'در حال به‌روزرسانی' : update.status === 'available' ? 'نسخهٔ جدید' : update.status === 'current' ? 'به‌روز' : 'بررسی ناموفق'}</Badge>}>
      <div className="grid gap-5 lg:grid-cols-[.8fr_1.2fr]">
        <div className="space-y-3"><div className="rounded-2xl border border-white/10 bg-white/[.03] p-4"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 font-mono text-2xl font-bold text-white" dir="ltr">0.2.0</div><div className="mt-1 text-xs text-muted-foreground" dir="ltr">commit {update.local || '—'}</div></div><div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" disabled={updating} onClick={() => void checkUpdate()}><RefreshCw size={15} className={updating ? 'animate-spin' : ''}/> بررسی دوباره</Button>{isAdmin && update.status === 'available' && <Button size="sm" disabled={updating} onClick={() => void runUpdate()}>{updating ? <><RefreshCw size={15} className="animate-spin"/> در حال نصب…</> : 'به‌روزرسانی پنل'}</Button>}<a href="https://github.com/themrcortex/donatello" target="_blank" rel="noopener noreferrer"><Button variant="ghost" size="sm"><Github size={15}/> GitHub</Button></a></div>{updating && <div role="status" aria-live="polite" className="rounded-xl border border-primary/25 bg-primary/[.08] p-4"><div className="flex items-center gap-2 text-sm font-semibold text-white"><RefreshCw size={16} className="animate-spin text-primary"/>در حال دریافت و نصب به‌روزرسانی</div><p className="mt-2 text-xs leading-6 text-muted-foreground">پنل تا پایان فرایند در دسترس می‌ماند. پس از تأیید نسخهٔ جدید، صفحه به‌طور خودکار تازه می‌شود.</p><div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/10"><div className="h-full w-1/3 animate-pulse rounded-full bg-primary"/></div></div>}{update.status === 'available' && !updating && <Alert className="border-primary/30 bg-primary/10"><AlertDescription>نسخهٔ جدید <code className="font-mono text-white" dir="ltr">{update.remote}</code> آمادهٔ نصب است.</AlertDescription></Alert>}{update.status === 'unavailable' && !updating && <p className="text-xs leading-6 text-amber-200/80">بررسی نسخه انجام نشد؛ اتصال سرور و دسترسی helper را بررسی کن.</p>}</div>
        <div className="rounded-2xl border border-white/10 bg-[#11131e] p-5"><div className="mb-3 flex items-center justify-between"><h3 className="text-sm font-semibold text-white">تغییرات اخیر</h3><span className="font-mono text-[11px] text-muted-foreground" dir="ltr">main</span></div><ul className="space-y-3 text-sm leading-6 text-muted-foreground"><li className="flex gap-2"><span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"/>کنترل کامل UFW: نصب، فعال‌سازی، غیرفعال‌سازی و مدیریت قوانین</li><li className="flex gap-2"><span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"/>اعلان نسخهٔ جدید و به‌روزرسانی امن از داخل پنل</li><li className="flex gap-2"><span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"/>تنظیمات قابل تغییر برای تاریخچه، نشست، پورت پنل و فیلترهای شبکه</li><li className="flex gap-2"><span className="mt-2 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"/>نمودارهای منابع و ترافیک با ثبت پس‌زمینه در Rust</li></ul></div>
      </div>
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
