import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowDownLeft,
  ArrowUpRight,
  Clock3,
  Cpu,
  Database,
  Download,
  Gauge,
  HardDrive,
  Info,
  KeyRound,
  LockKeyhole,
  Network,
  Plus,
  Radio,
  RefreshCw,
  ScrollText,
  Server,
  Shield,
  ShieldCheck,
  Terminal,
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type Sample = { at: number; time: string; rx: number; tx: number }
type ResourceSample = { at: number; cpu: number; memory: number; disk: number }
type StoredSample = { at: number; cpu: number; memory: number; disk: number; rx: number; tx: number }

function useDashboard(notify: (message: string) => void) {
  const [data, setData] = useState<Dashboard | null>(null)
  const [samples, setSamples] = useState<StoredSample[]>([])
  const [range, setRange] = useState<'5m' | '1h' | '24h'>('5m')
  const [loading, setLoading] = useState(true)
  const refresh = useCallback(async () => {
    try {
      const [next, history] = await Promise.all([api<Dashboard>('dashboard'), api<{ samples: StoredSample[] }>(`history?range=${range}`)])
      setData(next)
      setSamples(history.samples)
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
  const history = samples.map((sample) => ({ at: sample.at, time: new Date(sample.at * 1000).toLocaleTimeString('fa-IR', { hour: '2-digit', minute: '2-digit' }), rx: sample.rx, tx: sample.tx }))
  const resourceHistory = samples.map(({ at, cpu, memory, disk }) => ({ at, cpu, memory, disk }))
  return { data, history, resourceHistory, cpu: samples.at(-1)?.cpu ?? null, loading, refresh, range, setRange }
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
    <form onSubmit={save} className="grid gap-4 md:grid-cols-3">{([['cpu', 'پردازنده', cpu], ['memory', 'حافظه', memory], ['disk', 'دیسک', disk]] as const).map(([key, label, value]) => <div key={key} className="rounded-xl border border-white/10 bg-white/[.025] p-4"><div className="mb-3 flex items-center justify-between text-xs"><Label htmlFor={`limit-${key}`}>{label}</Label><Badge variant={value !== null && value >= limits[key] ? 'warning' : 'success'}>{value !== null && value >= limits[key] ? 'بالاتر از حد' : 'عادی'}</Badge></div><div className="flex items-center gap-2"><Input id={`limit-${key}`} type="number" min={1} max={100} value={limits[key]} disabled={user.role !== 'admin'} onChange={(event) => setLimits((old) => ({ ...old, [key]: Number(event.target.value) }))}/><span className="text-sm">٪</span></div></div>)}{user.role === 'admin' && <Button type="submit" disabled={busy} className="md:col-span-3 md:w-fit">ذخیرهٔ آستانه‌ها</Button>}</form>
  </SectionCard>
}

export function SystemControls({ user, notify }: { user: User; notify: (message: string) => void }) {
  const { data, cpu } = useDashboard(notify)
  const memory = data?.memory_kib ? Math.round((1 - data.available_kib / data.memory_kib) * 100) : 0
  const disk = data?.disk_total_bytes ? Math.round((1 - (data.disk_free_bytes || 0) / data.disk_total_bytes) * 100) : 0
  return <div className="mt-5"><AlertControls user={user} cpu={cpu} memory={memory} disk={disk} notify={notify}/></div>
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
  return <SectionCard title="آزمایش اتصال TCP" description="یک اتصال واقعی از داخل سرور به پورت موردنظر برقرار می‌شود"><form onSubmit={submit} className="flex flex-wrap items-end gap-3"><div className="space-y-2"><Label htmlFor="port-probe">پورت</Label><Input id="port-probe" type="number" min={1} max={65535} value={port} onChange={(event) => setPort(event.target.value)} className="w-36"/></div><Button type="submit" disabled={busy}>{busy ? 'در حال بررسی…' : 'بررسی اتصال'}</Button>{result && <span className="text-xs text-muted-foreground">{result}</span>}</form></SectionCard>
}

export function DashboardPage({ user, Heading, notify }: { user: User } & Common) {
  const { data, history, resourceHistory, cpu, loading, refresh, range, setRange } = useDashboard(notify)
  const memory = data && data.memory_kib ? Math.round((1 - data.available_kib / data.memory_kib) * 100) : 0
  const disk =
    data && data.disk_total_bytes
      ? Math.round((1 - (data.disk_free_bytes || 0) / data.disk_total_bytes) * 100)
      : 0
  return (
    <>
      <Heading
        title="نمای کلی"
        description="سلام، وضعیت سرورت را در یک نگاه ببین."
        action={
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw size={15} /> به‌روزرسانی
          </Button>
        }
      />
      <div className="hero-glow relative mb-5 flex min-h-[220px] items-center justify-between overflow-hidden rounded-[26px] border border-[#67517f] p-7 md:p-9">
        <div className="relative z-10">
          <Badge variant="success" className="mb-4">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> سیستم فعال
          </Badge>
          <div className="brand-text text-[38px] font-black leading-none sm:text-[52px] lg:text-[66px]" dir="ltr">DONATELLO</div>
          <div className="mt-3 flex items-center gap-3" dir="ltr"><span className="h-[2px] w-9 bg-[#63e4cb]"/><span className="font-mono text-[10px] tracking-[.22em] text-[#a6eddf]">YOUR SERVER. CLEARLY.</span></div>
          <h2 className="mt-6 text-lg font-extrabold md:text-xl">خوش آمدی، {user.username}</h2>
          <p className="mt-1.5 text-sm text-[#cbb8e0]">
            {data ? (
              <>
                سرور <span className="mono text-white">{data.hostname || 'Linux'}</span> آمادهٔ مدیریت است.
              </>
            ) : (
              'در حال دریافت اطلاعات سرور…'
            )}
          </p>
        </div>
        <div className="relative hidden h-36 w-36 shrink-0 place-items-center rounded-[36px] border border-white/10 bg-white/5 text-[#d1afff] shadow-[0_0_65px_#8b5cf633] md:grid">
          <div className="absolute inset-3 rounded-[28px] border border-dashed border-white/10"/>
          <Server size={66} strokeWidth={1.1} />
        </div>
      </div>
      {loading && !data ? (
        <LoadingCards />
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <Metric
            icon={Cpu}
            label="مصرف پردازنده"
            value={cpu === null ? 'در حال سنجش' : `${formatNumber(cpu)}٪`}
            detail="نمونه‌برداری هر ۵ ثانیه"
            progress={cpu ?? 0}
          />
          <Metric
            icon={Database}
            label="حافظهٔ مصرف‌شده"
            value={formatBytes(((data?.memory_kib || 0) - (data?.available_kib || 0)) * 1024)}
            detail={`از ${formatBytes((data?.memory_kib || 0) * 1024)} · ${formatNumber(memory)}٪`}
            progress={memory}
          />
          <Metric
            icon={HardDrive}
            label="فضای دیسک /"
            value={
              data?.disk_total_bytes
                ? formatBytes(data.disk_total_bytes - (data.disk_free_bytes || 0))
                : 'نامشخص'
            }
            detail={
              data?.disk_total_bytes
                ? `از ${formatBytes(data.disk_total_bytes)} · ${formatNumber(disk)}٪`
                : 'آمار دیسک در دسترس نیست'
            }
            progress={disk}
          />
          <Metric
            icon={Clock3}
            label="زمان روشن‌بودن"
            value={formatUptime(data?.uptime_seconds || 0)}
            detail={`بار سیستم: ${data?.load || '—'}`}
          />
        </div>
      )}
      <div className="mt-5">
        <SectionCard title="نبض منابع سرور" description="تاریخچهٔ ثبت‌شده توسط سرویس، حتی وقتی پنل باز نبوده است" right={<div className="flex gap-1 rounded-lg border border-white/10 p-1">{(['5m', '1h', '24h'] as const).map((item) => <button key={item} onClick={() => setRange(item)} className={`rounded-md px-2.5 py-1 text-[11px] ${range === item ? 'bg-primary/25 text-white' : 'text-muted-foreground hover:text-white'}`} dir="ltr">{item}</button>)}</div>}>
          <ResourceChart samples={resourceHistory} />
        </SectionCard>
      </div>
      {data && (
        <div className="mt-5 grid gap-5 md:grid-cols-2">
          <SectionCard title="مصرف خود Donatello" description="اندازه‌گیری فرایند پنل · به‌روزرسانی هر ۵ ثانیه">
            <div className="grid gap-4 sm:grid-cols-2">
              <Metric
                icon={Cpu}
                label="CPU پنل"
                value={`${formatNumber(Math.round(data.process_cpu_percent * 10) / 10)}٪`}
                detail="سهم از توان کل پردازنده‌های سرور"
                progress={data.process_cpu_percent}
              />
              <Metric
                icon={Database}
                label="RAM پنل"
                value={formatBytes(data.process_rss_bytes)}
                detail="حافظهٔ مقیم فرایند (RSS)، شامل کتابخانه‌های مشترک"
              />
            </div>
          </SectionCard>
          <SectionCard title="روش اندازه‌گیری" description="مقدار واقعی فرایند Donatello در همین سرور">
            <p className="text-sm leading-7 text-muted-foreground">
              RAM از حافظهٔ مقیم فرایند خوانده می‌شود. CPU از اختلاف نمونه‌های زمان پردازنده محاسبه شده و بر تعداد هسته‌ها تقسیم می‌شود؛ بنابراین مصرف کل سرور در کارت‌های بالا جداگانه نمایش داده می‌شود.
            </p>
          </SectionCard>
        </div>
      )}
      <div className="mt-5"><AlertControls user={user} cpu={cpu} memory={memory} disk={disk} notify={notify}/></div>
      <div className="mt-5 grid gap-5 xl:grid-cols-[1.55fr_1fr]">
        <SectionCard
          title="ترافیک لحظه‌ای شبکه"
          description={`نرخ واقعی دریافت و ارسال · تاریخچهٔ ${range} از سرویس`}
          right={
            <Badge variant="outline">
              <Activity size={13} /> زنده
            </Badge>
          }
        >
          <div dir="ltr">
            {history.length > 0 ? (
              <TrafficChart history={history} />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-3 text-center text-sm text-muted-foreground">
                <Activity size={28} className="text-primary" /> نمودار پس از دومین نمونه‌برداری نمایش داده
                می‌شود.
              </div>
            )}
          </div>
          <div className="mt-3 flex flex-wrap gap-4 border-t border-border pt-4 text-xs text-muted-foreground">
            <span className="flex items-center gap-2">
              <i className="h-2 w-2 rounded-full bg-[#b487ff]" /> دریافت
            </span>
            <span className="flex items-center gap-2">
              <i className="h-2 w-2 rounded-full bg-[#59d3bc]" /> ارسال
            </span>
          </div>
        </SectionCard>
        <SectionCard
          title="کارت‌های شبکه"
          description={`${formatNumber(data?.interfaces.length || 0)} کارت شناسایی شد`}
          right={<Network size={18} className="text-primary" />}
        >
          <div className="space-y-1">
            {data?.interfaces.length ? (
              data.interfaces.slice(0, 5).map((item) => (
                <div
                  key={item.name}
                  className="flex items-center justify-between gap-3 border-b border-border/65 py-3 last:border-0"
                >
                  <div className="flex min-w-0 items-center gap-3">
                    <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-secondary text-primary">
                      <Network size={17} />
                    </div>
                    <div className="min-w-0">
                      <div className="truncate text-sm font-semibold" dir="ltr">
                        {item.name}
                      </div>
                      <div className="truncate text-xs text-muted-foreground" dir="ltr">
                        {item.addresses?.join(' · ') || item.mac}
                      </div>
                    </div>
                  </div>
                  <Badge variant={item.state === 'up' ? 'success' : 'outline'}>
                    {item.state === 'up' ? 'فعال' : item.state || 'نامشخص'}
                  </Badge>
                </div>
              ))
            ) : (
              <div className="py-12 text-center text-sm text-muted-foreground">کارت شبکه‌ای پیدا نشد.</div>
            )}
          </div>
        </SectionCard>
      </div>
      <div className="mt-5">
        <SectionCard
          title="پورت‌های شنونده"
          description="پورت‌های TCP و UDP شناسایی‌شده روی سرور"
          right={<Badge variant="outline">{formatNumber(data?.ports.length || 0)} پورت</Badge>}
        >
          <PortsTable ports={data?.ports.slice(0, 8) || []} />
        </SectionCard>
      </div>
    </>
  )
}

function ResourceChart({ samples }: { samples: ResourceSample[] }) {
  return <TimeSeriesChart samples={samples} series={[{ key: 'cpu', label: 'پردازنده', color: '#be9bff' }, { key: 'memory', label: 'حافظه', color: '#55d8cb' }, { key: 'disk', label: 'دیسک', color: '#f1bb8a' }]} max={100} unit="٪" />
}

type ChartSeries = { key: string; label: string; color: string }
function TimeSeriesChart({ samples, series, max, unit }: { samples: (ResourceSample | Sample)[]; series: ChartSeries[]; max: number; unit: string }) {
  const [hover, setHover] = useState<number | null>(null)
  if (!samples.length) return <div className="grid h-[235px] place-items-center text-center text-sm text-muted-foreground">سرویس در حال جمع‌آوری نخستین نمونه است. تاریخچه از این پس در پس‌زمینه ذخیره می‌شود.</div>
  const width = 720, left = 42, right = 16, top = 18, bottom = 214
  const x = (index: number) => left + index * (width - left - right) / Math.max(samples.length - 1, 1)
  const y = (value: number) => bottom - Math.min(Math.max(value / max, 0), 1) * (bottom - top)
  const selected = hover === null ? samples.length - 1 : Math.max(0, Math.min(hover, samples.length - 1))
  const current = samples[selected]
  const stamp = (at: number) => new Date(at * 1000).toLocaleTimeString('fa-IR', { hour: '2-digit', minute: '2-digit' })
  return <div className="rounded-xl border border-white/[.055] bg-[#10111b] p-3 md:p-4">
    <div className="mb-4 flex flex-wrap items-center gap-x-5 gap-y-2 text-xs">{series.map((item) => <span key={item.key} className="flex items-center gap-2 text-muted-foreground"><i className="h-2 w-2 rounded-full" style={{ background: item.color, boxShadow: `0 0 10px ${item.color}88` }}/>{item.label} <b className="font-mono text-white" dir="ltr">{new Intl.NumberFormat('fa-IR', { maximumFractionDigits: 1 }).format(Number((current as unknown as Record<string, number>)[item.key]) || 0)}{unit}</b></span>)}<span className="mr-auto font-mono text-[10px] text-muted-foreground" dir="ltr">{stamp(current.at)}</span></div>
    <div className="h-[235px]" dir="ltr"><svg viewBox="0 0 720 250" width="100%" height="100%" preserveAspectRatio="none" onMouseLeave={() => setHover(null)} onMouseMove={(event) => { const rect = event.currentTarget.getBoundingClientRect(); setHover(Math.round(((event.clientX - rect.left) / rect.width * width - left) / (width - left - right) * (samples.length - 1))) }} role="img" aria-label={`تاریخچهٔ ${series.map((s) => s.label).join('، ')}`}>
      <defs>{series.map((item) => <linearGradient key={item.key} id={`chart-${item.key}`} x1="0" y1="0" x2="0" y2="1"><stop stopColor={item.color} stopOpacity=".24"/><stop offset="1" stopColor={item.color} stopOpacity="0"/></linearGradient>)}</defs>
      {[0, .25, .5, .75, 1].map((fraction) => { const py = y(fraction * max); return <g key={fraction}><line x1={left} x2={width-right} y1={py} y2={py} stroke="#363448" strokeDasharray="3 6"/><text x={left-10} y={py+4} fill="#8d8a9f" fontSize="11" textAnchor="end">{unit === '٪' ? `${Math.round(fraction*max)}%` : formatBytes(fraction*max)}</text></g> })}
      {series.map((item) => { const points = samples.map((sample, index) => `${x(index)},${y(Number((sample as unknown as Record<string, number>)[item.key]) || 0)}`).join(' '); return <g key={item.key}><polygon points={`${left},${bottom} ${points} ${x(samples.length-1)},${bottom}`} fill={`url(#chart-${item.key})`}/><polyline points={points} fill="none" stroke={item.color} strokeWidth="2.6" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke"/></g> })}
      <line x1={x(selected)} x2={x(selected)} y1={top} y2={bottom} stroke="#a999c3" strokeOpacity=".45" strokeDasharray="4 5"/>
      {series.map((item) => <circle key={item.key} cx={x(selected)} cy={y(Number((current as unknown as Record<string, number>)[item.key]) || 0)} r="4.5" fill={item.color} stroke="#10111b" strokeWidth="2"/>)}
      <text x={left} y="242" fill="#9995ac" fontSize="11">{stamp(samples[0].at)}</text><text x={width-right} y="242" textAnchor="end" fill="#9995ac" fontSize="11">{stamp(samples[samples.length-1].at)}</text>
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
  return <TimeSeriesChart samples={history} series={[{ key: 'rx', label: 'دریافت', color: '#be9bff' }, { key: 'tx', label: 'ارسال', color: '#55d8cb' }]} max={Math.max(1024, ...history.flatMap((sample) => [sample.rx, sample.tx])) * 1.12} unit=" B/s" />
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
  const { data, loading, refresh } = useDashboard(notify)
  return (
    <>
      <Heading
        title="شبکه و پورت‌ها"
        description="کارت‌ها، آدرس‌ها، شمارنده‌ها و پورت‌های شنونده"
        action={
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw size={15} /> به‌روزرسانی
          </Button>
        }
      />
      <div className="mb-5"><PortProbeControls notify={notify}/></div>
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
          <div className="mt-5">
            <SectionCard
              title="پورت‌های شنونده"
              description="داده‌ها مستقیماً از اطلاعات کرنل خوانده می‌شوند"
              right={<Badge variant="default">{formatNumber(data?.ports.length || 0)} مورد</Badge>}
            >
              <Tabs defaultValue="all" dir="rtl">
                <TabsList>
                  <TabsTrigger value="all">همه</TabsTrigger>
                  <TabsTrigger value="tcp">TCP</TabsTrigger>
                  <TabsTrigger value="udp">UDP</TabsTrigger>
                </TabsList>
                <TabsContent value="all">
                  <PortsTable ports={data?.ports || []} />
                </TabsContent>
                <TabsContent value="tcp">
                  <PortsTable ports={data?.ports.filter((p) => p.protocol === 'TCP') || []} />
                </TabsContent>
                <TabsContent value="udp">
                  <PortsTable ports={data?.ports.filter((p) => p.protocol === 'UDP') || []} />
                </TabsContent>
              </Tabs>
            </SectionCard>
          </div>
          <Alert className="mt-5">
            <Info size={17} className="mb-2 text-primary" />
            <AlertTitle>دربارهٔ ترافیک هر پورت</AlertTitle>
            <AlertDescription>
              شمارنده‌های کارت شبکه فعال‌اند. اندازه‌گیری بایت به‌تفکیک پورت به جمع‌آوری داده با nftables یا
              eBPF نیاز دارد و هنوز در این نسخه فعال نیست.
            </AlertDescription>
          </Alert>
        </>
      )}
    </>
  )
}

export function FirewallPage({ Heading, notify, user }: Common & { user: User }) {
  const [data, setData] = useState<Firewall | null>(null)
  const [rulePort, setRulePort] = useState('8080')
  const [ruleProtocol, setRuleProtocol] = useState<'tcp' | 'udp'>('tcp')
  const [changing, setChanging] = useState(false)
  const [loading, setLoading] = useState(true)
  const refresh = useCallback(async () => {
    try {
      setData(await api<Firewall>('firewall'))
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن UFW ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify])
  useEffect(() => {
    void refresh()
  }, [refresh])
  async function changeRule(action: 'add' | 'remove') {
    if (action === 'remove' && !window.confirm(`قانون مدیریت‌شدهٔ ${rulePort}/${ruleProtocol} حذف شود؟`)) return
    setChanging(true)
    try { await api('firewall/rules', { method: 'POST', body: JSON.stringify({ action, port: Number(rulePort), protocol: ruleProtocol }) }); notify(action === 'add' ? 'قانون دسترسی اضافه شد.' : 'قانون دسترسی حذف شد.'); await refresh() }
    catch (error) { notify(error instanceof Error ? error.message : 'تغییر قانون ناموفق بود') }
    finally { setChanging(false) }
  }
  const rules = (data?.status || '').split('\n').filter((line) => /^\s*\[\s*\d+\]/.test(line))
  return (
    <>
      <Heading
        title="فایروال UFW"
        description="وضعیت و قوانین ثبت‌شدهٔ فایروال سرور"
        action={
          <div className="flex gap-2"><Button variant="outline" size="sm" onClick={refresh}><RefreshCw size={15}/> به‌روزرسانی</Button><Button variant="outline" size="sm" disabled={!data?.status} onClick={() => downloadText('donatello-ufw.txt', data?.status || '')}><Download size={15}/> دریافت قوانین</Button></div>
        }
      />
      {user.role === 'admin' && <div className="mb-5"><SectionCard title="مدیریت قانون دسترسی" description="فقط قانون‌های دارای برچسب DONATELLO WEB از اینجا قابل حذف هستند"><div className="flex flex-wrap items-end gap-3"><div className="space-y-2"><Label htmlFor="rule-port">پورت</Label><Input id="rule-port" type="number" min={1} max={65535} value={rulePort} onChange={(event) => setRulePort(event.target.value)} className="w-32"/></div><div className="space-y-2"><Label htmlFor="rule-proto">پروتکل</Label><Select value={ruleProtocol} onValueChange={(value) => setRuleProtocol(value as 'tcp' | 'udp')}><SelectTrigger id="rule-proto" className="w-28"><SelectValue/></SelectTrigger><SelectContent><SelectItem value="tcp">TCP</SelectItem><SelectItem value="udp">UDP</SelectItem></SelectContent></Select></div><Button disabled={changing || !rulePort} onClick={() => void changeRule('add')}>افزودن اجازه</Button><Button variant="outline" disabled={changing || !rulePort} onClick={() => void changeRule('remove')}>حذف قانون Donatello</Button></div></SectionCard></div>}
      <div className="mb-5"><DistributionBars title="ترکیب قوانین UFW" description="بر اساس خروجی فعلی ufw status numbered" items={['ALLOW', 'DENY', 'REJECT', 'LIMIT'].map((action) => ({ label: action, value: rules.filter((line) => line.includes(action)).length }))} /></div>
      <div className="grid gap-5 lg:grid-cols-[1.5fr_.85fr]">
        <SectionCard
          title="وضعیت قوانین"
          description="خروجی دستور ufw status numbered"
          right={
            <Badge variant={data?.available ? 'success' : 'warning'}>
              {loading ? 'در حال بررسی' : data?.available ? 'در دسترس' : 'در دسترس نیست'}
            </Badge>
          }
        >
          {loading ? (
            <Skeleton className="h-48" />
          ) : (
            <pre
              className="mono max-h-[540px] overflow-auto whitespace-pre-wrap break-words rounded-xl border border-border bg-[#120e1b] p-5 text-left text-xs leading-7 text-[#d9cde8]"
              dir="ltr"
            >
              {data?.status || 'خروجی‌ای دریافت نشد.'}
              {data?.error && `\n${data.error}`}
            </pre>
          )}
        </SectionCard>
        <SectionCard title="حفاظت دسترسی" description="دسترسی به پنل را همیشه حفظ کنید">
          <div className="space-y-4">
            <div className="flex gap-3">
              <div className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-emerald-400/10 text-emerald-300">
                <ShieldCheck size={20} />
              </div>
              <p className="text-sm leading-7 text-muted-foreground">
                پیش از هر تغییر، قوانین دسترسی SSH و پورت پنل باید بررسی شوند.
              </p>
            </div>
            <Alert>
              <AlertDescription>
                ساخت و حذف قوانین فقط برای مدیر و از مسیر محدود Donatello انجام می‌شود. حذف قوانین دیگر سرور از این صفحه مجاز نیست.
              </AlertDescription>
            </Alert>
          </div>
        </SectionCard>
      </div>
    </>
  )
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
                    placeholder="حداقل ۱۲ نویسه"
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
      <div className="mb-5"><DistributionBars title="ترکیب عملیات اخیر" description="از میان ۱۰۰ رویداد آخر ثبت‌شده" items={['login', 'logout', 'account.password', 'user.create', 'user.update'].map((action) => ({ label: action, value: events.filter((event) => event.action === action).length }))} /></div>
      <SectionCard
        title="آخرین رویدادها"
        description="۱۰۰ رویداد اخیر"
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
                    {new Date(e.at * 1000).toLocaleString('fa-IR')}
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

export function SettingsPage({
  Heading,
  logout,
  notify,
}: {
  Heading: Heading
  logout: () => void
  notify: (message: string) => void
}) {
  const sections = [
    {
      icon: Radio,
      title: 'پورت پنل',
      value: location.port || (location.protocol === 'https:' ? '443' : '80'),
      description: 'پورت فعلی در آدرس مرورگر',
    },
    { icon: LockKeyhole, title: 'امنیت نشست', value: '۲۴ ساعت', description: 'کوکی HttpOnly و SameSite' },
    { icon: Gauge, title: 'دورهٔ مانیتورینگ', value: '۵ ثانیه', description: 'به‌روزرسانی خودکار داشبورد' },
  ]
  return (
    <>
      <Heading title="تنظیمات" description="اطلاعات اجرای پنل و مسیر توسعهٔ قابلیت‌ها" />
      <div className="grid gap-5 lg:grid-cols-[1.1fr_.9fr]">
        <SectionCard title="وضعیت فعلی پنل" description="پیکربندی و امنیت">
          <div className="space-y-1">
            {sections.map((item) => (
              <div
                key={item.title}
                className="flex items-center justify-between gap-4 border-b border-border/70 py-4 last:border-0"
              >
                <div className="flex items-center gap-3">
                  <span className="grid h-10 w-10 place-items-center rounded-xl bg-primary/15 text-primary">
                    <item.icon size={19} />
                  </span>
                  <div>
                    <div className="text-sm font-semibold">{item.title}</div>
                    <div className="mt-1 text-xs text-muted-foreground">{item.description}</div>
                  </div>
                </div>
                <Badge variant="outline">{item.value}</Badge>
              </div>
            ))}
          </div>
        </SectionCard>
        <SectionCard title="قابلیت‌های در حال توسعه" description="بر اساس نقشهٔ محصول Donatello">
          <div className="space-y-3">
            {[
              { icon: Terminal, name: 'ترمینال وب', detail: 'نشست محدود با مجوز مستقل' },
              { icon: Wifi, name: 'تست سرعت', detail: 'اندازه‌گیری دانلود، آپلود و تأخیر' },
              { icon: Shield, name: 'SSL و ACME', detail: 'صدور و تمدید گواهی با بازیابی سرویس' },
              { icon: Download, name: 'ترافیک هر پورت', detail: 'شمارنده‌های nftables یا eBPF' },
            ].map((item) => (
              <div
                key={item.name}
                className="flex items-center gap-3 rounded-xl border border-border/70 bg-secondary/35 p-3"
              >
                <item.icon size={18} className="shrink-0 text-primary" />
                <div>
                  <div className="text-sm font-semibold">{item.name}</div>
                  <div className="text-xs text-muted-foreground">{item.detail}</div>
                </div>
              </div>
            ))}
          </div>
        </SectionCard>
      </div>
      <div className="mt-5">
        <Alert>
          <Info size={17} className="mb-2 text-primary" />
          <AlertTitle>تغییر پورت</AlertTitle>
          <AlertDescription>
            در این نسخه، پورت از متغیر محیطی{' '}
            <code className="mono rounded bg-background px-1.5 py-0.5">DONATELLO_BIND</code> تنظیم می‌شود و پس
            از راه‌اندازی دوبارهٔ سرویس اعمال خواهد شد.
          </AlertDescription>
        </Alert>
      </div>
      <div className="mt-5">
        <PasswordCard notify={notify} />
      </div>
      <Button variant="outline" className="mt-7" onClick={logout}>
        <X size={16} /> خروج از حساب
      </Button>
    </>
  )
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
