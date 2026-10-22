import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
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
type Sample = { time: string; rx: number; tx: number }

function useDashboard(notify: (message: string) => void) {
  const [data, setData] = useState<Dashboard | null>(null)
  const [history, setHistory] = useState<Sample[]>([])
  const [cpu, setCpu] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const last = useRef<{ rx: number; tx: number; total: number; idle: number; at: number } | null>(null)
  const refresh = useCallback(async () => {
    try {
      const next = await api<Dashboard>('dashboard')
      setData(next)
      const rx = next.interfaces.reduce((a, n) => a + n.rx_bytes, 0)
      const tx = next.interfaces.reduce((a, n) => a + n.tx_bytes, 0)
      const at = Date.now()
      const prev = last.current
      if (prev) {
        const seconds = Math.max((at - prev.at) / 1000, 0.1)
        setHistory((old) =>
          [
            ...old,
            {
              time: new Date(at).toLocaleTimeString('fa-IR', { minute: '2-digit', second: '2-digit' }),
              rx: Math.max(0, (rx - prev.rx) / seconds),
              tx: Math.max(0, (tx - prev.tx) / seconds),
            },
          ].slice(-20),
        )
        const total = (next.cpu_total || 0) - prev.total
        const idle = (next.cpu_idle || 0) - prev.idle
        if (total > 0) setCpu(Math.max(0, Math.min(100, Math.round((1 - idle / total) * 100))))
      }
      last.current = { rx, tx, total: next.cpu_total || 0, idle: next.cpu_idle || 0, at }
    } catch (e) {
      notify(e instanceof Error ? e.message : 'خواندن اطلاعات سرور ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [notify])
  useEffect(() => {
    void refresh()
    const timer = window.setInterval(refresh, 5000)
    return () => window.clearInterval(timer)
  }, [refresh])
  return { data, history, cpu, loading, refresh }
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

export function DashboardPage({ user, Heading, notify }: { user: User } & Common) {
  const { data, history, cpu, loading, refresh } = useDashboard(notify)
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
      <div className="hero-glow mb-5 flex min-h-40 items-center justify-between overflow-hidden rounded-[24px] border border-[#6d4c91] p-7 md:p-8">
        <div>
          <Badge variant="success" className="mb-3">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> سیستم فعال
          </Badge>
          <h2 className="text-xl font-extrabold md:text-[27px]">خوش آمدی، {user.username} 👋</h2>
          <p className="mt-2 text-sm text-[#cbb8e0]">
            {data ? (
              <>
                سرور <span className="mono text-white">{data.hostname || 'Linux'}</span> آمادهٔ مدیریت است.
              </>
            ) : (
              'در حال دریافت اطلاعات سرور…'
            )}
          </p>
        </div>
        <div className="hidden h-24 w-24 place-items-center rounded-[28px] border border-white/10 bg-white/5 text-[#d1afff] md:grid">
          <Server size={44} strokeWidth={1.2} />
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
      <div className="mt-5 grid gap-5 xl:grid-cols-[1.55fr_1fr]">
        <SectionCard
          title="ترافیک لحظه‌ای شبکه"
          description="نرخ واقعی دریافت و ارسال، به‌روزرسانی هر ۵ ثانیه"
          right={
            <Badge variant="outline">
              <Activity size={13} /> زنده
            </Badge>
          }
        >
          <div className="h-[250px]" dir="ltr">
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

function TrafficChart({ history }: { history: Sample[] }) {
  const max = Math.max(1024, ...history.flatMap((sample) => [sample.rx, sample.tx]))
  const x = (index: number) => 12 + (index * 476) / Math.max(history.length - 1, 1)
  const y = (value: number) => 210 - (value / max) * 180
  const line = (key: 'rx' | 'tx') =>
    history.map((sample, index) => `${index ? 'L' : 'M'} ${x(index)} ${y(sample[key])}`).join(' ')
  const area = (key: 'rx' | 'tx') => `${line(key)} L ${x(history.length - 1)} 210 L 12 210 Z`
  const last = history[history.length - 1]
  return (
    <svg
      viewBox="0 0 500 250"
      width="100%"
      height="100%"
      preserveAspectRatio="none"
      role="img"
      aria-label={`نرخ دریافت ${formatBytes(last.rx)} و ارسال ${formatBytes(last.tx)} در ثانیه`}
    >
      <defs>
        <linearGradient id="rxGradient" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#b487ff" stopOpacity=".35" />
          <stop offset="100%" stopColor="#b487ff" stopOpacity="0" />
        </linearGradient>
        <linearGradient id="txGradient" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#59d3bc" stopOpacity=".22" />
          <stop offset="100%" stopColor="#59d3bc" stopOpacity="0" />
        </linearGradient>
      </defs>
      {[30, 90, 150, 210].map((position) => (
        <line
          key={position}
          x1="12"
          x2="488"
          y1={position}
          y2={position}
          stroke="#473851"
          strokeDasharray="3 5"
        />
      ))}
      <path d={area('rx')} fill="url(#rxGradient)" />
      <path d={area('tx')} fill="url(#txGradient)" />
      <path d={line('rx')} fill="none" stroke="#b487ff" strokeWidth="2.5" vectorEffect="non-scaling-stroke" />
      <path d={line('tx')} fill="none" stroke="#59d3bc" strokeWidth="2.5" vectorEffect="non-scaling-stroke" />
      <circle cx={x(history.length - 1)} cy={y(last.rx)} r="4" fill="#b487ff">
        <title>{`${last.time} · دریافت ${formatBytes(last.rx)}/ثانیه`}</title>
      </circle>
      <circle cx={x(history.length - 1)} cy={y(last.tx)} r="4" fill="#59d3bc">
        <title>{`${last.time} · ارسال ${formatBytes(last.tx)}/ثانیه`}</title>
      </circle>
      <text x="12" y="238" fill="#a698b6" fontSize="11">
        {history[0].time}
      </text>
      <text x="488" y="238" fill="#a698b6" fontSize="11" textAnchor="end">
        {last.time}
      </text>
    </svg>
  )
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

export function FirewallPage({ Heading, notify }: Common) {
  const [data, setData] = useState<Firewall | null>(null)
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
  return (
    <>
      <Heading
        title="فایروال UFW"
        description="وضعیت و قوانین ثبت‌شدهٔ فایروال سرور"
        action={
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw size={15} /> به‌روزرسانی
          </Button>
        }
      />
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
                ویرایش قوانین UFW پس از افزودن بررسی امنیتی، تأیید تغییر و مسیر بازیابی فعال خواهد شد. این
                صفحه فعلاً وضعیت واقعی را نشان می‌دهد.
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
          <Button variant="outline" size="sm" onClick={refresh}>
            <RefreshCw size={15} /> به‌روزرسانی
          </Button>
        }
      />
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
