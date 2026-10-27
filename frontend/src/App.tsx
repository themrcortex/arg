import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  AlertTriangle,
  ArrowDownToLine,
  ArrowLeft,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  Eye,
  EyeOff,
  Github,
  LifeBuoy,
  Menu,
  RefreshCw,
  RotateCw,
  Settings,
  UserRound,
  WifiOff,
  X,
} from 'lucide-react'
import { api, getActiveWorkspaceNode, setActiveWorkspaceNode, type Dashboard, type User } from './lib/api'
import { clearHeavyCache } from './lib/heavy-cache'
import { PanelTimeZoneContext } from './lib/timezone'
import { cn } from './lib/utils'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Skeleton } from './components/ui/skeleton'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Alert, AlertDescription, AlertTitle } from './components/ui/alert'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Avatar, AvatarFallback } from './components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './components/ui/dropdown-menu'
import { navigation, pageFromHash, type PageId } from './navigation'
import { toast } from 'sonner'
import { Toaster } from './components/ui/sonner'
import { AppShell } from './components/shell/AppShell'
import { SidebarNavigation } from './components/shell/SidebarNavigation'
import { TopNavigation } from './components/shell/TopNavigation'
import { ThemeToggle } from './components/shell/ThemeToggle'
import { UnifiedStepper } from './components/ui/stepper'
const TerminalPage = lazy(() => import('./TerminalPage').then((module) => ({ default: module.TerminalPage })))
const S3StoragePage = lazy(() => import('./S3StoragePage').then((module) => ({ default: module.S3StoragePage })))
const DnsPage = lazy(() => import('./DnsPage').then((module) => ({ default: module.DnsPage })))
const ApplicationsPage = lazy(() => import('./ApplicationsPage').then((module) => ({ default: module.ApplicationsPage })))
const DashboardPage = lazy(() => import('./pages').then((module) => ({ default: module.DashboardPage })))
const SystemPage = lazy(() => import('./pages').then((module) => ({ default: module.SystemPage })))
const NetworkPage = lazy(() => import('./pages').then((module) => ({ default: module.NetworkPage })))
const PortsPage = lazy(() => import('./pages').then((module) => ({ default: module.PortsPage })))
const FirewallPage = lazy(() => import('./pages').then((module) => ({ default: module.FirewallPage })))
const NginxPage = lazy(() => import('./pages').then((module) => ({ default: module.NginxPage })))
const UsersPage = lazy(() => import('./pages').then((module) => ({ default: module.UsersPage })))
const AuditPage = lazy(() => import('./pages').then((module) => ({ default: module.AuditPage })))
const SettingsPage = lazy(() => import('./pages').then((module) => ({ default: module.SettingsPage })))
const AccountPage = lazy(() => import('./pages').then((module) => ({ default: module.AccountPage })))
const AptPage = lazy(() => import('./AdminOpsPage').then((module) => ({ default: module.AptPage })))
const ServicesPage = lazy(() => import('./AdminOpsPage').then((module) => ({ default: module.ServicesPage })))
const DockerPage = lazy(() => import('./DockerPage').then((module) => ({ default: module.DockerPage })))
const FileManagerPage = lazy(() => import('./FileManagerPage').then((module) => ({ default: module.FileManagerPage })))
const MaintenancePage = lazy(() => import('./MaintenancePage').then((module) => ({ default: module.MaintenancePage })))
const MysqlPage = lazy(() => import('./MysqlPage').then((module) => ({ default: module.MysqlPage })))
const NodesPage = lazy(() => import('./NodesPage').then((module) => ({ default: module.NodesPage })))
const AllServersPage = lazy(() => import('./AllServersPage').then((module) => ({ default: module.AllServersPage })))
const BugReportPage = lazy(() => import('./BugReportPage').then((module) => ({ default: module.BugReportPage })))
const notifyUser = (message: string) => {
  if (/ناموفق|خطا|نامعتبر|نشد|مشخص نشد|نپذیرفت|failed|error|invalid|could not|unable/i.test(message)) toast.error(message)
  else toast.success(message)
}

type AlertThresholds = { cpu: number; memory: number; disk: number }
type ResourceAlert = { key: string; label: string; value: number; threshold: number; critical: boolean }
type UpdateProgress = { updating: boolean; started_at: number | null; instance: string; job_id?: string | null; state?: 'idle' | 'running' | 'succeeded' | 'failed' | 'cancelled'; phase?: string; operation?: 'update' | 'rollback'; branch?: 'main' | 'development' | ''; build_mode?: 'fast' | 'complete' | ''; message?: string }
type UpdateStage = 1 | 2 | 3 | 4 | 5
const updateStageFor = (phase?: string): UpdateStage => {
  switch (phase) {
    case 'building': return 2
    case 'installing': return 3
    case 'restarting': return 4
    case 'verifying':
    case 'complete': return 5
    default: return 1
  }
}
const WORKSPACE_MODE_KEY = 'arg.workspace-mode'
type WorkspaceMode = 'single' | 'multi'

function OfflineNotice() {
  const [online, setOnline] = useState(() => navigator.onLine)
  useEffect(() => {
    const update = () => setOnline(navigator.onLine)
    window.addEventListener('online', update)
    window.addEventListener('offline', update)
    return () => { window.removeEventListener('online', update); window.removeEventListener('offline', update) }
  }, [])
  if (online) return null
  return <Alert role="status" aria-live="polite" className="mb-5 border-amber-400/30 bg-amber-400/[.07] text-amber-100"><WifiOff size={17}/><div><AlertTitle>اتصال سرور در دسترس نیست</AlertTitle><AlertDescription className="text-amber-100/75">پوستهٔ ARG ممکن است آفلاین باز شود، اما اطلاعات سرور در حافظهٔ آفلاین ذخیره نمی‌شود و عملیات در دسترس نیست. اتصال را دوباره بررسی کن.</AlertDescription></div></Alert>
}

function ResourceAlertNotice() {
  const [alerts, setAlerts] = useState<ResourceAlert[]>([])
  const [dismissed, setDismissed] = useState<Set<string>>(() => new Set())
  const previousCpu = useRef<{ total: number; idle: number } | null>(null)
  const thresholdsCache = useRef<{ value: AlertThresholds; at: number } | null>(null)
  useEffect(() => {
    let cancelled = false
    let timer = 0
    const poll = async () => {
      try {
        const cachedThresholds = thresholdsCache.current
        const thresholdsRequest = cachedThresholds && Date.now() - cachedThresholds.at < 60_000
          ? Promise.resolve(cachedThresholds.value)
          : api<AlertThresholds>('monitoring/alerts').then((value) => { thresholdsCache.current = { value, at: Date.now() }; return value })
        const [dashboard, thresholds] = await Promise.all([api<Dashboard>('dashboard'), thresholdsRequest])
        if (cancelled) return
        const previous = previousCpu.current
        const totalDelta = previous && typeof dashboard.cpu_total === 'number'
          ? dashboard.cpu_total - previous.total
          : 0
        const idleDelta = previous && typeof dashboard.cpu_idle === 'number'
          ? dashboard.cpu_idle - previous.idle
          : 0
        if (typeof dashboard.cpu_total === 'number' && typeof dashboard.cpu_idle === 'number') {
          previousCpu.current = { total: dashboard.cpu_total, idle: dashboard.cpu_idle }
        }
        const cpu = totalDelta > 0 ? Math.min(100, Math.max(0, (1 - idleDelta / totalDelta) * 100)) : null
        const memory = dashboard.memory_kib > 0
          ? Math.min(100, Math.max(0, (1 - dashboard.available_kib / dashboard.memory_kib) * 100))
          : null
        const disk = (dashboard.disk_total_bytes || 0) > 0 && typeof dashboard.disk_free_bytes === 'number'
          ? Math.min(100, Math.max(0, (1 - dashboard.disk_free_bytes / dashboard.disk_total_bytes!) * 100))
          : null
        const readings = [
          { key: 'cpu', label: 'CPU', value: cpu, threshold: thresholds.cpu },
          { key: 'memory', label: 'حافظه', value: memory, threshold: thresholds.memory },
          { key: 'disk', label: 'دیسک', value: disk, threshold: thresholds.disk },
        ]
        const activeReadings = readings.flatMap((reading) => reading.value !== null && (reading.value >= reading.threshold || reading.value >= 95)
          ? [{ key: reading.key, label: reading.label, value: reading.value, threshold: reading.threshold, critical: reading.value >= 95 }]
          : [])
        setAlerts(activeReadings)
        const clearedKeys = new Set(readings.filter((reading) => reading.value !== null && reading.value < Math.min(reading.threshold, 95) - 3).map((reading) => reading.key))
        if (clearedKeys.size) setDismissed((current) => new Set([...current].filter((key) => !clearedKeys.has(key))))
      } catch { /* An unavailable metrics read should not block or interrupt panel navigation. */ }
      if (!cancelled) timer = window.setTimeout(() => void poll(), 30000)
    }
    void poll()
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [])

  const visibleAlerts = alerts.filter((alert) => !dismissed.has(alert.key))
  if (!visibleAlerts.length) return null
  const critical = visibleAlerts.some((alert) => alert.critical)
  return <Alert role="status" className={critical ? 'mb-5 border-rose-400/30 bg-rose-400/[.08] text-rose-100' : 'mb-5 border-amber-400/30 bg-amber-400/[.07] text-amber-100'}>
    <AlertTriangle size={17} className={critical ? 'text-rose-300' : 'text-amber-300'}/>
    <AlertTitle className="flex items-center justify-between gap-3"><span>{critical ? 'وضعیت بحرانی منابع سرور' : 'عبور از آستانهٔ هشدار'}</span><button type="button" className="rounded-md p-1 text-current/65 transition hover:bg-white/10 hover:text-current" aria-label="بستن اعلان آستانه تا زمان بازگشت هشدار" title="پنهان‌کردن تا وقتی مصرف از محدودهٔ هشدار پایین‌تر برود" onClick={() => setDismissed((current) => new Set([...current, ...visibleAlerts.map((alert) => alert.key)]))}><X size={15}/></button></AlertTitle>
    <AlertDescription className="mt-1 flex flex-wrap gap-x-4 gap-y-1">
      {visibleAlerts.map((alert) => <span key={alert.key}>{alert.label}: <b className="font-mono" dir="ltr">{alert.value.toFixed(1)}%</b> <span className="text-current/70">(آستانهٔ تنظیم‌شده: <b className="font-mono" dir="ltr">{alert.threshold}%</b>)</span></span>)}
    </AlertDescription>
  </Alert>
}
async function forceFreshPanelReload(showComplete = true) {
  const clearCaches = async () => {
    try {
      if ('caches' in window) {
        const cacheNames = await window.caches.keys()
        await Promise.all(cacheNames.map((name) => window.caches.delete(name)))
      }
    } catch { /* Cache Storage may be unavailable; a cache-busting navigation still follows. */ }
    try {
      if ('serviceWorker' in navigator) {
        const registrations = await navigator.serviceWorker.getRegistrations()
        await Promise.all(registrations.map((registration) => registration.unregister()))
      }
    } catch { /* Service Workers may be unavailable in this browser context. */ }
  }
  await Promise.race([clearCaches(), new Promise((resolve) => window.setTimeout(resolve, 1200))])
  try {
    if (showComplete) window.sessionStorage.setItem('arg-update-finished', 'true')
    else window.sessionStorage.removeItem('arg-update-finished')
  } catch { /* Keep the reload available when storage is blocked. */ }
  const url = new URL(window.location.href)
  url.searchParams.set('_arg_refresh', Date.now().toString())
  window.location.replace(url.toString())
}
const developer = (
  <a
    className="font-semibold text-primary transition hover:text-white hover:underline"
    href="https://t.me/mrc0rtex"
    target="_blank"
    rel="noopener noreferrer"
    dir="ltr"
  >
    𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁
  </a>
)

function Brand({ large = false }: { large?: boolean }) {
  return (
    <div className="flex items-center gap-3">
      <img src="/arg/brand/arg-symbol.svg" alt="" aria-hidden="true" className={large ? 'h-14 w-14' : 'h-11 w-11'} />
      <div>
        <div className={cn('brand-text font-black leading-none text-primary', large ? 'text-[31px]' : 'text-[21px]')} dir="ltr">ARG</div>
        <div className="mt-1 text-sm font-bold text-foreground">ارگ</div>
        <div className="mt-1 text-[10px] font-semibold uppercase tracking-[.2em] text-muted-foreground" dir="ltr">
          VPS CONTROL CENTER
        </div>
      </div>
    </div>
  )
}

function Auth({ installer, onSuccess }: { installer: boolean; onSuccess: () => void }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [passwordVisible, setPasswordVisible] = useState(false)
  const [setupToken, setSetupToken] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [created, setCreated] = useState(false)
  async function submit(event: FormEvent) {
    event.preventDefault()
    setError('')
    setBusy(true)
    try {
      await api(installer ? 'setup' : 'login', {
        method: 'POST',
        body: JSON.stringify({ username, password, ...(installer ? { setup_token: setupToken } : {}) }),
      })
      if (installer) {
        setCreated(true)
        setPassword('')
        setSetupToken('')
      } else onSuccess()
    } catch (e) {
      setError(e instanceof Error ? e.message : 'خطایی رخ داد')
    } finally {
      setBusy(false)
    }
  }
  if (created)
    return (
      <div className="flex min-h-screen items-center justify-center p-5">
        <Card className="w-full max-w-md soft-panel">
          <CardHeader>
            <Brand large />
            <CardTitle className="pt-8 text-2xl">نصب با موفقیت انجام شد</CardTitle>
            <CardDescription>حساب مدیر ساخته شد. اکنون می‌توانید وارد پنل شوید.</CardDescription>
          </CardHeader>
          <CardContent>
            <Button className="w-full" onClick={onSuccess}>
              رفتن به صفحهٔ ورود <ArrowLeft size={17} />
            </Button>
          </CardContent>
        </Card>
      </div>
    )
  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden p-5">
      <div className="pointer-events-none absolute inset-0 bg-[url('/arg/patterns/arg-card-corner.svg')] bg-[length:260px] bg-no-repeat opacity-[.08]" />
      <div className="relative w-full max-w-[440px]">
        <div className="mb-8 flex justify-center">
          <Brand large />
        </div>
        <Card className="soft-panel">
          <CardHeader className="pb-6">
            <Badge className="mb-4 w-fit">{installer ? 'راه‌اندازی اولیه' : location.protocol === 'https:' ? 'ورود امن' : 'اتصال HTTP'}</Badge>
            <CardTitle className="text-2xl">{installer ? 'به ارگ خوش آمدید' : 'خوش آمدید'}</CardTitle>
            <CardDescription className="leading-7">
              {installer
                ? 'حساب مدیر را بسازید تا مدیریت سرور را آغاز کنید.'
                : 'برای دسترسی به سرورتان وارد حساب خود شوید.'}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {location.protocol !== 'https:' && <Alert className="mb-5 border-amber-400/30 bg-amber-400/[.07] text-amber-100"><AlertTriangle size={17}/><AlertDescription className="leading-7"><strong className="block">این اتصال با HTTP رمزنگاری نشده است.</strong> ورود می‌تواند گذرواژه و نشست را در شبکه افشا کند. اگر از شبکهٔ عمومی وصل شده‌ای، ادامه نده؛ از SSH tunnel استفاده کن یا پس از ورود از تنظیمات، HTTPS را فعال کن.</AlertDescription></Alert>}
            {installer && (
              <Alert className="mb-5 border-primary/30 bg-primary/10 text-foreground">
                <AlertDescription className="leading-7">
                  <strong className="mb-2 block">برای ساخت مدیر، کد را از SSH همان سرور بگیر:</strong>
                  <ol className="list-inside list-decimal space-y-1.5">
                    <li>یک پنجرهٔ ترمینال باز کن و با SSH وارد همین VPS شو.</li>
                    <li>این دستور را اجرا کن: <code dir="ltr" className="rounded bg-background/70 px-1.5 py-0.5 font-mono text-xs">sudo arg setup-code</code></li>
                    <li>کد یک‌بارمصرف خروجی را در کادر پایین وارد کن و ساخت حساب را بزن.</li>
                  </ol>
                  <p className="mt-2">این صفحه باید با HTTPS باز شده باشد. کد را برای کسی نفرست؛ پس از ساخت حساب دیگر قابل استفاده نیست.</p>
                </AlertDescription>
              </Alert>
            )}
            <form onSubmit={submit} className="space-y-5">
              <div className="space-y-2">
                <Label htmlFor="username">نام کاربری</Label>
                <Input
                  id="username"
                  autoComplete="username"
                  required
                  minLength={3}
                  maxLength={32}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="مثلاً admin"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="password">گذرواژه</Label>
                <div className="relative">
                  <Input
                    id="password"
                    className="pl-12"
                    type={passwordVisible ? 'text' : 'password'}
                    autoComplete={installer ? 'new-password' : 'current-password'}
                    required
                    minLength={installer ? 12 : 1}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder={installer ? 'حداقل 12 نویسه' : 'گذرواژهٔ حساب'}
                  />
                  <button
                    type="button"
                    className="absolute left-3 top-1/2 grid h-8 w-8 -translate-y-1/2 place-items-center rounded-lg text-muted-foreground transition hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-label={passwordVisible ? 'مخفی‌کردن گذرواژه' : 'نمایش گذرواژه'}
                    aria-pressed={passwordVisible}
                    onClick={() => setPasswordVisible((visible) => !visible)}
                  >
                    {passwordVisible ? <EyeOff size={17} /> : <Eye size={17} />}
                  </button>
                </div>
              </div>
              {installer && (
                <div className="space-y-2">
                  <Label htmlFor="setup-token">کد یک‌بارمصرف راه‌اندازی</Label>
                  <Input
                    id="setup-token"
                    type="password"
                    autoComplete="one-time-code"
                    value={setupToken}
                    onChange={(e) => setSetupToken(e.target.value)}
                    placeholder="کد خروجی sudo arg setup-code"
                  />
                  <p className="text-xs leading-5 text-muted-foreground">به SSH سرور برگرد و دستور <code dir="ltr" className="font-mono">sudo arg setup-code</code> را اجرا کن؛ کدی که ترمینال چاپ می‌کند را اینجا بچسبان.</p>
                </div>
              )}
              {error && (
                <Alert className="border-rose-400/30 bg-rose-400/10 text-rose-200">
                  <AlertDescription className="text-rose-200">{error}</AlertDescription>
                </Alert>
              )}
              <Button type="submit" size="lg" className="w-full" disabled={busy}>
                {busy ? 'لطفاً صبر کنید…' : installer ? 'ساخت حساب مدیر' : 'ورود به پنل'}
                <ArrowLeft size={18} />
              </Button>
            </form>
          </CardContent>
        </Card>
        <p className="mt-7 text-center text-xs text-muted-foreground">
          طراحی و توسعه توسط {developer} · ساخته‌شده با Rust و React
        </p>
      </div>
    </div>
  )
}

function ChangelogView({ content }: { content: string }) {
  const lines = content.trim().split('\n').filter(Boolean)
  return <div className="space-y-2 text-sm leading-7 text-muted-foreground">{lines.map((line, index) => {
    if (line.startsWith('# ')) return <h3 key={index} className="text-base font-bold text-foreground">{line.slice(2)}</h3>
    if (line.startsWith('## ')) return <h4 key={index} className="pt-2 font-semibold text-primary">{line.slice(3)}</h4>
    if (line.startsWith('### ')) return <h5 key={index} className="pt-1 font-semibold text-foreground">{line.slice(4)}</h5>
    if (line.startsWith('- ')) return <div key={index} className="flex gap-2"><span className="mt-3 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"/><span>{line.slice(2)}</span></div>
    return <p key={index}>{line}</p>
  })}</div>
}

function UpdateLogDetails({ active = false }: { active?: boolean }) {
  const [open, setOpen] = useState(false)
  const [logs, setLogs] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [following, setFollowing] = useState(true)
  const logViewport = useRef<HTMLPreElement>(null)
  const followTail = useRef(true)
  useEffect(() => { if (active) { followTail.current = true; setFollowing(true); setOpen(true) } }, [active])
  useEffect(() => {
    if (!open) return
    let cancelled = false
    let timer = 0
    const read = async () => {
      setLoading(true)
      try {
        const result = await api<{ logs: string }>('update/logs')
        if (!cancelled) { setLogs(result.logs); setError('') }
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : 'دریافت گزارش به‌روزرسانی ناموفق بود.')
      } finally {
        if (!cancelled) setLoading(false)
      }
      if (active && !cancelled) timer = window.setTimeout(() => void read(), 3000)
    }
    void read()
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [open, active])
  useLayoutEffect(() => {
    const viewport = logViewport.current
    if (!viewport || !followTail.current) return
    const frame = window.requestAnimationFrame(() => {
      const current = logViewport.current
      if (current && followTail.current) current.scrollTo({ top: current.scrollHeight, behavior: 'auto' })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [logs, open])
  function jumpToLatest() {
    followTail.current = true
    setFollowing(true)
    const viewport = logViewport.current
    if (viewport) viewport.scrollTo({ top: viewport.scrollHeight, behavior: 'smooth' })
  }
  return <div className="rounded-xl border border-border bg-card/70 p-3 shadow-sm">
    <Button type="button" variant="outline" size="sm" className="w-full justify-between rounded-lg border-border bg-background/70 text-foreground transition-colors hover:border-primary/40 hover:bg-secondary/70" aria-expanded={open} onClick={() => { if (!open) { followTail.current = true; setFollowing(true) }; setOpen((value) => !value) }}><span className="flex items-center gap-2"><span className="grid h-6 w-6 place-items-center rounded-md border border-primary/15 bg-primary/10 text-primary"><Activity size={13}/></span>{open ? 'بستن جزئیات فرایند' : 'مشاهدهٔ جزئیات فرایند'}</span><span className="flex items-center gap-2 text-muted-foreground">{active && <i className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400"/>}{loading && <RefreshCw size={13} className="animate-spin"/>}<ChevronDown size={15} className={cn('transition-transform duration-300', open && 'rotate-180')}/></span></Button>
    {open && <div className="mt-3 rounded-lg border border-border/80 bg-background/55 p-2.5"><div className="mb-2 flex min-h-7 items-center justify-between gap-3 px-1 text-xs text-muted-foreground"><span>{active ? 'گزارش زندهٔ به‌روزرسانی' : 'گزارش آخرین به‌روزرسانی'}</span>{!following ? <Button type="button" variant="ghost" size="sm" onClick={jumpToLatest} className="h-7 gap-1.5 rounded-md px-2 text-[11px] text-primary hover:bg-primary/10 hover:text-primary"><ArrowDownToLine size={13}/>رفتن به آخرین گزارش</Button> : <span className="flex items-center gap-1.5 font-mono text-[10px] text-primary/80" dir="ltr">{active && <i className="h-1.5 w-1.5 animate-pulse rounded-full bg-primary"/>}FOLLOWING</span>}</div>{error ? <p className="rounded-lg border border-rose-400/20 bg-rose-400/[.06] p-3 text-xs text-rose-200">{error}</p> : <pre ref={logViewport} onScroll={(event) => { const node = event.currentTarget; const atTail = node.scrollHeight - node.scrollTop - node.clientHeight < 28; followTail.current = atTail; setFollowing(atTail) }} className="max-h-[min(42vh,360px)] min-h-24 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-border bg-[#111820] p-3 text-left font-mono text-[11px] leading-5 text-foreground/80 shadow-inner" dir="ltr">{logs || (loading ? 'در حال دریافت گزارش…' : 'هنوز گزارشی ثبت نشده است.')}</pre>}</div>}
  </div>
}

function UpdateBranchSelector({ branch, onChange }: { branch: 'main' | 'development'; onChange: (branch: 'main' | 'development') => void }) {
  return <section className="space-y-2 rounded-xl border border-border bg-background/35 p-3" aria-label="شاخهٔ دریافت به‌روزرسانی"><p className="text-xs font-semibold">شاخهٔ دریافت نسخه</p><div className="grid gap-2 sm:grid-cols-2"><button type="button" aria-pressed={branch === 'main'} onClick={() => onChange('main')} className={`rounded-lg border p-3 text-right transition-colors ${branch === 'main' ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}><span className="flex items-center justify-between gap-2"><span className="font-mono text-xs font-bold" dir="ltr">main</span><span className="text-[10px] text-emerald-300">پایدار · پیشنهادی</span></span><span className="mt-1 block text-[10px] leading-5 text-muted-foreground">نسخه‌های بررسی‌شده و مناسب استفادهٔ روزمره.</span></button><button type="button" aria-pressed={branch === 'development'} onClick={() => onChange('development')} className={`rounded-lg border p-3 text-right transition-colors ${branch === 'development' ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}><span className="flex items-center justify-between gap-2"><span className="font-mono text-xs font-bold" dir="ltr">development</span><span className="text-[10px] text-amber-300">آزمایشی</span></span><span className="mt-1 block text-[10px] leading-5 text-muted-foreground">تغییرات در حال توسعه؛ ممکن است ناپایدار یا منتشرنشده باشند.</span></button></div><p className="text-[10px] leading-5 text-muted-foreground">انتخاب شاخه، بررسی نسخه و منبع build را با هم تغییر می‌دهد. شاخهٔ انتخابی ذخیره می‌شود.</p></section>
}

function UpdateActionConfirmation({ action, busy, onCancel, onConfirm }: {
  action: 'cancel' | 'rollback' | null
  busy: boolean
  onCancel: () => void
  onConfirm: () => void
}) {
  const isRollback = action === 'rollback'
  return <Dialog open={action !== null} onOpenChange={(open) => { if (!open && !busy) onCancel() }}>
    <DialogContent className="z-[120] max-w-md">
      <DialogHeader>
        <DialogTitle>{isRollback ? 'بازگشت به نسخهٔ قبلی؟' : 'لغو به‌روزرسانی؟'}</DialogTitle>
        <DialogDescription>{isRollback
          ? 'نسخهٔ قبلی برنامه و snapshot پایگاه‌دادهٔ همان نسخه برمی‌گردند. تغییرات دیتابیس بعد از snapshot ممکن است از دست بروند.'
          : 'فرایند با توقف امن متوقف می‌شود. اگر جایگزینی فایل‌ها شروع شده باشد، نسخه و پایگاه‌دادهٔ قبلی بازیابی و سلامت سرویس بررسی می‌شود.'}</DialogDescription>
      </DialogHeader>
      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" variant="ghost" disabled={busy} onClick={onCancel}>انصراف</Button>
        <Button type="button" variant={isRollback ? 'default' : 'destructive'} disabled={busy} aria-busy={busy} onClick={onConfirm}>
          {busy ? <RefreshCw size={15} className="animate-spin"/> : isRollback ? <RotateCw size={15}/> : <X size={15}/>}
          {busy ? 'در حال انجام…' : isRollback ? 'بازگشت و بررسی سلامت' : 'لغو امن آپدیت'}
        </Button>
      </div>
    </DialogContent>
  </Dialog>
}

function UpdateDialog({ open, onOpenChange, update, checking, updating, finished, refreshing, failure, canInstall, changelog, buildMode, onBuildModeChange, updateBranch, onUpdateBranchChange, onCheck, onInstall, onRollback, rollingBack, onForceRefresh }: {
  open: boolean
  onOpenChange: (open: boolean) => void
  update: { status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; local_version?: string; remote_version?: string; error?: string }
  checking: boolean
  updating: boolean
  finished: boolean
  refreshing: boolean
  failure: string | null
  canInstall: boolean
  changelog: string
  buildMode: 'fast' | 'complete'
  onBuildModeChange: (mode: 'fast' | 'complete') => void
  updateBranch: 'main' | 'development'
  onUpdateBranchChange: (branch: 'main' | 'development') => void
  onCheck: () => void
  onInstall: (mode: 'fast' | 'complete') => void
  onRollback: () => void
  rollingBack: boolean
  onForceRefresh: () => void
}) {
  const [refreshingPanel, setRefreshingPanel] = useState(false)
  function refreshPanelManually() {
    setRefreshingPanel(true)
    onForceRefresh()
    window.setTimeout(() => setRefreshingPanel(false), 1800)
  }
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="max-h-[85vh] overflow-y-auto"><DialogHeader><DialogTitle>{failure ? 'به‌روزرسانی ناموفق بود' : finished ? 'به‌روزرسانی کامل شد' : 'به‌روزرسانی ARG'}</DialogTitle><DialogDescription>{failure ? 'نصب نسخه کامل نشد. گزارش زنده یا آخرین گزارش را برای جزئیات باز کن.' : finished ? 'نسخهٔ تازه بارگذاری شده است. تغییرات این نسخه:' : 'نسخهٔ نصب‌شده را با آخرین نسخهٔ GitHub بررسی کن.'}</DialogDescription></DialogHeader>
    {failure ? <div className="space-y-4">{canInstall && <UpdateBranchSelector branch={updateBranch} onChange={onUpdateBranchChange}/>}<div role="alert" className="rounded-xl border border-rose-400/25 bg-rose-400/[.07] p-4 text-sm text-rose-100"><div className="mb-1 flex items-center gap-2 font-semibold"><X size={17}/>به‌روزرسانی انجام نشد</div><p className="leading-6">{failure}</p></div><UpdateLogDetails/><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" disabled={checking || updating || rollingBack} onClick={onCheck}><RefreshCw size={15} className={checking ? 'animate-spin' : ''}/>بررسی دوباره</Button>{canInstall && <Button type="button" variant="outline" disabled={updating || rollingBack} onClick={onRollback}><RotateCw size={15} className={rollingBack ? 'animate-spin' : ''}/>{rollingBack ? 'در حال بازگشت…' : 'بازگشت به نسخهٔ قبل'}</Button>}{canInstall && update.status === 'available' && <Button disabled={checking || updating} onClick={() => onInstall(buildMode)}>تلاش دوباره برای نصب</Button>}<Button variant="ghost" onClick={() => onOpenChange(false)}>بستن</Button></div></div> : finished ? <div className="space-y-4"><div className="flex items-center gap-2 rounded-xl border border-emerald-400/20 bg-emerald-400/[.07] p-3 text-sm text-emerald-200"><CheckCircle2 size={18}/>{refreshing ? 'در حال تازه‌سازی پنل…' : 'به‌روزرسانی کامل شد و نسخهٔ تازه بارگذاری شد.'}</div><p className="text-xs leading-6 text-muted-foreground">تغییرات زیر مربوط به نسخهٔ تازه‌بارگذاری‌شده است. برای بارگذاری دوباره در صورت نیاز از دکمهٔ رفرش استفاده کن.</p><div className="max-h-[42vh] overflow-y-auto rounded-xl border border-border bg-background p-4"><ChangelogView content={changelog}/></div><UpdateLogDetails/><div className="grid gap-2 sm:grid-cols-3"><Button onClick={refreshPanelManually} aria-busy={refreshingPanel}><RefreshCw size={15} className={refreshingPanel ? 'animate-spin' : ''}/>{refreshingPanel ? 'در حال پاک‌سازی و رفرش…' : 'رفرش کامل صفحه'}</Button>{canInstall && <Button type="button" variant="outline" disabled={rollingBack} onClick={onRollback}><RotateCw size={15} className={rollingBack ? 'animate-spin' : ''}/>{rollingBack ? 'در حال بازگشت…' : 'بازگشت به نسخهٔ قبل'}</Button>}<Button variant="outline" onClick={() => onOpenChange(false)}>متوجه شدم</Button></div></div> : <div className="space-y-4">
      {canInstall && <UpdateBranchSelector branch={updateBranch} onChange={onUpdateBranchChange}/>}
      <div className="grid grid-cols-2 gap-3"><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.local_version ? `v${update.local_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.local || '—'}</div></div><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">آخرین نسخه</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.remote_version ? `v${update.remote_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.remote || '—'} · {updateBranch}</div></div></div>
      {checking ? <div role="status" className="flex items-center gap-2 rounded-xl border border-border p-4 text-sm text-muted-foreground"><RefreshCw size={16} className="animate-spin text-primary"/>در حال بررسی نسخهٔ GitHub…</div> : update.status === 'available' ? <div className="rounded-xl border border-amber-400/25 bg-amber-400/[.06] p-4 text-sm text-amber-100">نسخهٔ جدید {update.remote_version ? `v${update.remote_version}` : update.remote} آمادهٔ نصب است.</div> : update.status === 'current' ? <div className="rounded-xl border border-emerald-400/20 bg-emerald-400/[.06] p-4 text-sm text-emerald-200">پنل به‌روز است.</div> : <div className="rounded-xl border border-rose-400/20 bg-rose-400/[.06] p-4 text-sm text-rose-200">{update.error || 'بررسی نسخه انجام نشد. اتصال سرور و دسترسی helper را بررسی کن.'}</div>}
      {update.status === 'available' && canInstall && <section className="space-y-2 rounded-xl border border-border bg-background/35 p-3" aria-label="حالت ساخت نسخهٔ جدید"><p className="text-xs font-semibold">روش کامپایل و مصرف دیسک</p><div className="grid gap-2 sm:grid-cols-2"><button type="button" aria-pressed={buildMode === 'fast'} onClick={() => onBuildModeChange('fast')} className={`rounded-lg border p-3 text-right transition-colors ${buildMode === 'fast' ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}><span className="block text-xs font-semibold">آپدیت سریع</span><span className="mt-1 block text-[10px] leading-5 text-muted-foreground">Cache کامپایل را نگه می‌دارد تا آپدیت‌های بعدی سریع‌تر شوند. بر اساس اندازهٔ فعلی سرورتان، حدود 1.5 GB از دیسک برای فایل‌های Build اشغال می‌ماند.</span></button><button type="button" aria-pressed={buildMode === 'complete'} onClick={() => onBuildModeChange('complete')} className={`rounded-lg border p-3 text-right transition-colors ${buildMode === 'complete' ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}><span className="block text-xs font-semibold">آپدیت کامل · آزادسازی Cache</span><span className="mt-1 block text-[10px] leading-5 text-muted-foreground">Cache قبلی پیش از کامپایل و Cache جدید پس از نصب پاک می‌شود؛ هر آپدیت طولانی‌تر است. هنگام خودِ کامپایل موقتاً فضای دیسک لازم است و این مقدار با نسخه تغییر می‌کند.</span></button></div></section>}
      <UpdateLogDetails/><div className="flex flex-wrap justify-end gap-2"><Button type="button" variant="outline" disabled={updating || rollingBack || !canInstall} onClick={onRollback}><RotateCw size={15} className={rollingBack ? 'animate-spin' : ''}/>{rollingBack ? 'در حال بازگشت…' : 'بازگشت به نسخهٔ قبل'}</Button><Button variant="outline" disabled={checking || updating || rollingBack} onClick={onCheck}><RefreshCw size={15} className={checking ? 'animate-spin' : ''}/>بررسی دوباره</Button>{canInstall && update.status === 'available' && <Button disabled={checking || updating} onClick={() => onInstall(buildMode)}>نصب نسخهٔ جدید</Button>}</div>
    </div>}
  </DialogContent></Dialog>
}

function UpdateOverlay({ active, stage, startedAt, shared, rollback, branch, buildMode, refreshing, canCancel, canceling, onCancel }: {
  active: boolean; stage: UpdateStage; startedAt: number | null; shared: boolean; rollback: boolean; branch?: string; buildMode?: string; refreshing: boolean; canCancel: boolean; canceling: boolean; onCancel: () => void
}) {
  const [elapsed, setElapsed] = useState(0)
  const overlayRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!active || !startedAt) { setElapsed(0); return }
    const tick = () => setElapsed(Math.max(0, Math.floor((Date.now() - startedAt) / 1000)))
    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [active, startedAt])
  useEffect(() => { if (active) overlayRef.current?.focus() }, [active])
  if (!active) return null
  const elapsedLabel = [Math.floor(elapsed / 60), elapsed % 60].map((v) => String(v).padStart(2, '0')).join(':')
  const labels = rollback ? ['گرفتن قفل', 'ذخیرهٔ نسخهٔ جاری', 'بازیابی نسخهٔ قبل', 'راه‌اندازی سرویس', 'بررسی سلامت'] : ['آماده‌سازی', 'ساخت نسخه', 'نصب نسخه', 'راه‌اندازی', 'بررسی نهایی']
  const descriptions = rollback
    ? ['جلوگیری از تداخل با آپدیت', 'تهیهٔ snapshot برای امکان برگشت', 'جایگزینی نسخه و دیتابیس', 'راه‌اندازی مجدد سرویس ARG', 'تأیید پاسخ پنل']
    : ['آماده‌سازی کد و وابستگی‌ها', 'کامپایل نسخهٔ جدید پنل', 'نصب فایل‌های کامپایل‌شده', 'راه‌اندازی مجدد سرویس ARG', 'بررسی سلامت نسخهٔ جدید']
  return <div ref={overlayRef} tabIndex={-1} className="fixed inset-0 z-[100] grid place-items-center bg-[#080d12]/95 p-4 backdrop-blur-md outline-none" role="alertdialog" aria-modal="true" aria-labelledby="update-overlay-title" aria-describedby="update-overlay-description">
    <div className="arg-glass-popover max-h-[calc(100dvh-2rem)] w-full max-w-xl overflow-y-auto rounded-2xl border border-[#dda46f]/25 p-5 text-right sm:p-7">
      <div className="flex items-center gap-3"><div className="relative grid h-12 w-12 shrink-0 place-items-center rounded-xl border border-[#ffcb91]/40 bg-[#dda46f]/15 text-[#ffd19a] shadow-[0_0_26px_rgba(221,164,111,.25)]"><RefreshCw size={22} className="animate-spin"/><i className="absolute -right-1 -top-1 h-2.5 w-2.5 animate-pulse rounded-full bg-[#ffe2b9] shadow-[0_0_12px_#dda46f]"/></div><div><h2 id="update-overlay-title" className="text-lg font-bold text-white">{refreshing ? 'در حال بارگذاری نسخهٔ جدید' : rollback ? 'در حال بازگشت ARG' : 'در حال به‌روزرسانی ARG'}</h2><p id="update-overlay-description" className="mt-1 text-xs leading-6 text-slate-300">{refreshing ? 'پس از رفرش کامل، تغییرات نسخهٔ نصب‌شده نمایش داده می‌شود.' : shared ? (rollback ? 'بازگشت در نشست دیگری شروع شده؛ وضعیت واقعی سرور نمایش داده می‌شود.' : 'آپدیت در نشست دیگری شروع شده؛ مراحل واقعی سرور نمایش داده می‌شوند.') : rollback ? 'نسخه و دیتابیس قبلی در حال بازیابی و بررسی سلامت هستند.' : 'مراحل مطابق وضعیت سرور نمایش داده می‌شوند، نه درصد تخمینی.'}</p></div></div>
      <div className="mt-5 flex items-center justify-between gap-3 rounded-xl border border-white/10 bg-black/20 px-4 py-3"><span className="text-xs font-semibold text-[#ffd29c]">{refreshing ? 'آماده‌سازی رابط جدید…' : descriptions[stage - 1]}</span><span className="rounded-md bg-[#dda46f]/10 px-2 py-1 font-mono text-xs tabular-nums text-[#ffca91]" dir="ltr">{elapsedLabel}</span></div>
      {!refreshing && !rollback && (branch || buildMode) && <div className="mt-2 flex flex-wrap gap-2 text-[11px] text-slate-300"><span className="rounded-md border border-white/10 bg-white/[.03] px-2 py-1">شاخه: <b className="font-mono text-[#ffd19a]" dir="ltr">{branch || '—'}</b></span><span className="rounded-md border border-white/10 bg-white/[.03] px-2 py-1">حالت ساخت: <b className="font-mono text-[#ffd19a]" dir="ltr">{buildMode === 'fast' ? 'fast' : buildMode === 'complete' ? 'complete' : '—'}</b></span></div>}
      {!refreshing && <div className="mt-5"><UnifiedStepper progressLabel={rollback ? 'پیشرفت بازگشت نسخه' : 'پیشرفت به‌روزرسانی'} ariaLabel={rollback ? 'مراحل بازگشت نسخه' : 'مراحل به‌روزرسانی'} currentStep={stage - 1} compact={false} steps={labels.map((title, index) => ({ id: `${index + 1}`, title }))}/><p className="mt-3 text-xs leading-6 text-slate-400">مراحل پایان‌یافته تیک می‌خورند؛ وضعیت جاری از Backend گرفته می‌شود و پیشرفت تخمینی نمایش داده نمی‌شود.</p></div>}
      <div className="mt-5"><UpdateLogDetails active={!refreshing}/></div><div className="mt-3 flex flex-wrap items-center justify-between gap-3"><p role="status" aria-live="polite" className="text-xs leading-6 text-slate-400">{refreshing ? 'پس از بازشدن نسخهٔ جدید، گزارش تغییرات نمایش داده می‌شود.' : rollback ? 'Rollback تراکنش بازیابی باینری و دیتابیس است و در این مرحله قابل لغو نیست؛ نتیجهٔ بررسی سلامت نمایش داده می‌شود.' : stage === 2 ? 'کامپایل Rust ممکن است چند دقیقه طول بکشد. گزارش زنده نام بخش‌های درحال ساخت را نشان می‌دهد؛ درصد دقیق و قابل اتکایی از Cargo در دسترس نیست.' : 'در طول نصب، عملیات مدیریتی دیگر موقتاً متوقف می‌شوند.'}</p>{canCancel && !refreshing && !rollback && <Button type="button" variant="outline" disabled={canceling} onClick={onCancel} className="border-rose-300/25 bg-rose-300/[.06] text-rose-100 hover:bg-rose-300/10"><X size={15}/>{canceling ? 'در حال لغو امن…' : 'لغو به‌روزرسانی'}</Button>}</div>
    </div>
  </div>
}

function PageHeading({
  title,
  description,
  action,
}: {
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <header className="arg-page-heading mb-6 flex flex-wrap items-end justify-between gap-4" dir="rtl">
      <div className="min-w-0 text-right" dir="rtl">
        <span className="arg-page-eyebrow inline-flex items-center gap-2 leading-5" dir="ltr" style={{ unicodeBidi: 'isolate' }}><span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-400"/>SERVER / MANAGEMENT</span>
        <h1 dir="rtl" className="mt-2 block text-2xl font-bold leading-[1.4] tracking-tight text-foreground md:text-[30px]">{title}</h1>
        <p dir="rtl" className="mt-1.5 max-w-3xl text-sm leading-6 text-muted-foreground">{description}</p>
      </div>
      {action && <div className="flex flex-wrap items-center gap-2 sm:pb-1">{action}</div>}
    </header>
  )
}

function PageRouteSkeleton({ label }: { label: string }) {
  return <div className="arg-route-skeleton space-y-5" role="status" aria-busy="true" aria-label={`در حال بارگذاری ${label}`}>
    <span className="sr-only">در حال آماده‌سازی صفحه…</span>
    <div className="flex items-center justify-between gap-4"><div className="space-y-2"><Skeleton className="h-7 w-44"/><Skeleton className="h-4 w-64 max-w-[70vw]"/></div><Skeleton className="h-10 w-28 rounded-xl"/></div>
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">{Array.from({ length: 4 }, (_, index) => <Skeleton key={index} className="h-28 rounded-2xl"/>)}</div>
    <div className="grid gap-4 xl:grid-cols-[1.4fr_1fr]"><section className="rounded-2xl border border-border bg-card/50 p-5"><Skeleton className="mb-5 h-5 w-40"/><Skeleton className="h-48 w-full rounded-xl"/></section><section className="rounded-2xl border border-border bg-card/50 p-5"><Skeleton className="mb-5 h-5 w-32"/>{Array.from({ length: 4 }, (_, index) => <Skeleton key={index} className="mb-3 h-9 w-full last:mb-0"/>)}</section></div>
    <section className="rounded-2xl border border-border bg-card/50 p-5"><div className="mb-5 flex items-center justify-between"><Skeleton className="h-5 w-36"/><Skeleton className="h-9 w-24 rounded-lg"/></div>{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="mb-3 h-12 w-full last:mb-0"/>)}</section>
  </div>
}

export default function App() {
  const [boot, setBoot] = useState(true)
  const [installed, setInstalled] = useState(false)
  const [user, setUser] = useState<User | null>(null)
  const [workspaceNodes, setWorkspaceNodes] = useState<Array<{ id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; workspace_ready?: boolean }>>([])
  const [workspaceNodeId, setWorkspaceNodeId] = useState(() => getActiveWorkspaceNode())
  const [workspaceMode, setWorkspaceMode] = useState<WorkspaceMode>(() => {
    try { return window.localStorage.getItem(WORKSPACE_MODE_KEY) === 'single' ? 'single' : 'multi' } catch { return 'multi' }
  })
  const [panelTimeZone, setPanelTimeZone] = useState('UTC')
  const [page, setPage] = useState<PageId>(() => pageFromHash(window.location.hash))
  const [terminalMounted, setTerminalMounted] = useState(() => pageFromHash(window.location.hash) === 'terminal')
  const [pageReady, setPageReady] = useState(false)
  const [mobileOpen, setMobileOpen] = useState(false)
  const mobileMenuButton = useRef<HTMLButtonElement>(null)
  const mobileCloseButton = useRef<HTMLButtonElement>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try { return window.localStorage.getItem('arg.arg-sidebar-collapsed') === 'true' } catch { return false }
  })
  const [loadError, setLoadError] = useState('')
  const [update, setUpdate] = useState<{ status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; local_version?: string; remote_version?: string; error?: string; instance?: string }>({ status: 'unavailable' })
  const [updateBuildMode, setUpdateBuildMode] = useState<'fast' | 'complete'>(() => {
    try { return window.localStorage.getItem('arg.update-build-mode') === 'fast' ? 'fast' : 'complete' } catch { return 'complete' }
  })
  const [updateBranch, setUpdateBranch] = useState<'main' | 'development'>(() => {
    try { return window.localStorage.getItem('arg.update-branch') === 'development' ? 'development' : 'main' } catch { return 'main' }
  })
  const [updating, setUpdating] = useState(false)
  const [updateFailure, setUpdateFailure] = useState<string | null>(null)
  const [cancelingUpdate, setCancelingUpdate] = useState(false)
  const [rollingBack, setRollingBack] = useState(false)
  const [updateStage, setUpdateStage] = useState<UpdateStage>(1)
  const [updateStartedAt, setUpdateStartedAt] = useState<number | null>(null)
  const [sharedUpdate, setSharedUpdate] = useState<UpdateProgress>({ updating: false, started_at: null, instance: '' })
  const [sharedUpdateStage, setSharedUpdateStage] = useState<UpdateStage>(1)
  const sharedUpdateJobId = useRef<string | null>(null)
  const sharedUpdateWasActive = useRef(false)
  const sharedUpdateStartInstance = useRef<string | null>(null)
  const sharedUpdateStartVersion = useRef<string | null>(null)
  const localUpdateStarted = useRef(false)
  const updateStartInstance = useRef<string | null>(null)
  const currentUpdateJobId = useRef<string | null>(null)
  const reloadScheduled = useRef(false)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false)
  const [latestChangelog, setLatestChangelog] = useState('')
  const [updateConfirmAction, setUpdateConfirmAction] = useState<'cancel' | 'rollback' | null>(null)
  const [reloadQueued, setReloadQueued] = useState(false)
  const [updateFinished, setUpdateFinished] = useState(false)
  useEffect(() => {
    if (!updateDialogOpen || !updateFinished || latestChangelog) return
    let active = true
    void import('../../CHANGELOG.md?raw').then(({ default: content }) => {
      if (active) setLatestChangelog(content.match(/^## v[^\n]*(?:\n(?!## v)[^\n]*)*/m)?.[0] || content)
    }).catch(() => { if (active) setLatestChangelog('فهرست تغییرات نسخه بارگذاری نشد.') })
    return () => { active = false }
  }, [updateDialogOpen, updateFinished, latestChangelog])
  const postReloadShown = useRef(false)
  const [updateReloadNotice] = useState(() => {
    try { const done = window.sessionStorage.getItem('arg-update-finished') === 'true'; window.sessionStorage.removeItem('arg-update-finished'); return done } catch { return false }
  })
  const refresh = useCallback(async () => {
    setBoot(true)
    setLoadError('')
    try {
      const state = await api<{ installed: boolean; user?: User | null }>('state')
      setInstalled(state.installed)
      setUser(state.user || null)
      setLoadError('')
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : 'اتصال به سرور برقرار نشد')
    } finally {
      setBoot(false)
    }
  }, [])
  useEffect(() => {
    void refresh()
  }, [refresh])
  useEffect(() => {
    if (!user) return
    let active = true
    void api<{ timezone: string }>('settings/timezone').then((result) => {
      if (active) setPanelTimeZone(result.timezone)
    }).catch(() => undefined)
    return () => { active = false }
  }, [user?.username])
  useEffect(() => {
    if (user?.role !== 'admin') { setWorkspaceNodes([]); return }
    let active = true
    const loadNodes = () => { void api<{ nodes: Array<{ id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; workspace_ready?: boolean }> }>('nodes')
      .then((result) => { if (active) { const available = result.nodes || []; setWorkspaceNodes(available); const selected = getActiveWorkspaceNode(); if (!selected || (selected === 'all' && workspaceMode === 'multi')) return; if (workspaceMode === 'single') { setActiveWorkspaceNode(''); setWorkspaceNodeId(''); window.location.reload(); return } const exact = available.find((node) => node.workspace_key === selected); const legacySelection = selected.match(/^(?:legacy:)?(\d+)$/)?.[1]; const migrated = legacySelection ? available.find((node) => String(node.id) === legacySelection) : undefined; const target = exact || migrated; if (!target?.workspace_ready) { setActiveWorkspaceNode(''); setWorkspaceNodeId(''); window.location.reload() } else if (target.workspace_key !== selected) { setActiveWorkspaceNode(target.workspace_key); setWorkspaceNodeId(target.workspace_key); clearHeavyCache(); window.location.reload() } } })
      .catch(() => { if (active) setWorkspaceNodes([]) }) }
    loadNodes()
    window.addEventListener('arg-nodes-changed', loadNodes)
    return () => { active = false; window.removeEventListener('arg-nodes-changed', loadNodes) }
  }, [user?.id, user?.role, workspaceMode])
  useEffect(() => {
    if (!updateReloadNotice || boot || !installed || !user || postReloadShown.current) return
    postReloadShown.current = true
    setUpdateFailure(null)
    setUpdateFinished(true)
    setUpdateDialogOpen(true)
    notifyUser('نسخهٔ تازه بارگذاری شد؛ گزارش تغییرات آماده است.')
  }, [updateReloadNotice, boot, installed, user?.id])
  useEffect(() => {
    const onHash = () => {
      const next = pageFromHash(window.location.hash)
      setPage(next)
      if (next === 'terminal') setTerminalMounted(true)
      setMobileOpen(false)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
  useLayoutEffect(() => {
    if (boot || !installed || !user) { setPageReady(false); return }
    setPageReady(false)
    const timer = window.setTimeout(() => setPageReady(true), 260)
    return () => window.clearTimeout(timer)
  }, [page, boot, installed, user?.id, user?.role])
  useEffect(() => {
    window.scrollTo({ top: 0, left: 0, behavior: 'auto' })
    document.documentElement.scrollTop = 0
    document.body.scrollTop = 0
  }, [page])
  useEffect(() => {
    try { window.localStorage.setItem('arg.arg-sidebar-collapsed', String(sidebarCollapsed)) } catch { /* Storage can be disabled by browser policy. */ }
  }, [sidebarCollapsed])
  useEffect(() => {
    if (!mobileOpen) return
    const previousOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setMobileOpen(false); return }
      if (event.key !== 'Tab') return
      const drawer = document.getElementById('arg-sidebar')
      const controls = Array.from(drawer?.querySelectorAll<HTMLElement>('button, a[href], [tabindex]:not([tabindex="-1"])') || []).filter((element) => element.getClientRects().length > 0)
      if (!controls.length) return
      const first = controls[0]
      const last = controls[controls.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    window.addEventListener('keydown', onKeyDown)
    mobileCloseButton.current?.focus()
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      document.body.style.overflow = previousOverflow
      mobileMenuButton.current?.focus()
    }
  }, [mobileOpen])
  const checkUpdate = useCallback(async (announce = false, branch: 'main' | 'development' = updateBranch) => {
    if (!installed || !user) return
    try {
      const result = await api<typeof update>(`update?branch=${branch}`)
      setUpdate(result)
      if (announce) notifyUser(result.status === 'available' ? `نسخهٔ جدید ARG از شاخهٔ ${branch} (${result.remote_version ? `v${result.remote_version}` : result.remote}) آماده است.` : result.status === 'current' ? `ARG در شاخهٔ ${branch} به‌روز است.` : result.error || 'بررسی نسخه انجام نشد.')
    } catch (error) {
      setUpdate({ status: 'unavailable', error: error instanceof Error ? error.message : 'بررسی نسخه انجام نشد.' })
      if (announce) notifyUser(error instanceof Error ? error.message : 'بررسی آپدیت ناموفق بود')
    }
  }, [installed, user, updateBranch])
  useEffect(() => {
    void checkUpdate()
    const timer = window.setInterval(() => void checkUpdate(), 300000)
    return () => window.clearInterval(timer)
  }, [checkUpdate])
  useEffect(() => {
    if (!installed || !user) return
    let cancelled = false
    let timer = 0
    const poll = async () => {
      try {
        const progress = await api<UpdateProgress>('update/progress')
        if (cancelled) return
        // Long builds are not failures; follow the privileged helper's state.
        setSharedUpdate((current) => current.job_id === progress.job_id && current.updating === progress.updating && current.phase === progress.phase && current.state === progress.state && current.instance === progress.instance ? current : progress)
        if (progress.updating) {
          if (progress.job_id && sharedUpdateJobId.current !== progress.job_id) {
            sharedUpdateJobId.current = progress.job_id
            setSharedUpdateStage(updateStageFor(progress.phase))
          } else {
            setSharedUpdateStage((current) => Math.max(current, updateStageFor(progress.phase)) as UpdateStage)
          }
          if (!sharedUpdateWasActive.current) {
            sharedUpdateWasActive.current = true
            sharedUpdateStartInstance.current = progress.instance
            sharedUpdateStartVersion.current = update.local || null
          } else if (sharedUpdateStartInstance.current && progress.instance !== sharedUpdateStartInstance.current) {
            setSharedUpdateStage(5)
          }
        } else if (!progress.updating && sharedUpdateWasActive.current) {
          sharedUpdateWasActive.current = false
          const previousInstance = sharedUpdateStartInstance.current
          const previousVersion = sharedUpdateStartVersion.current
          sharedUpdateStartInstance.current = null
          sharedUpdateStartVersion.current = null
          if (!localUpdateStarted.current && previousInstance) {
            try {
              const latest = await api<typeof update>('update')
              if (cancelled) return
              setUpdate(latest)
              const restarted = progress.instance !== previousInstance
              const versionChanged = Boolean(previousVersion && latest.local && latest.local !== previousVersion)
              if (progress.state === 'succeeded' && progress.phase === 'rolled_back') {
                sharedUpdateWasActive.current = false
                sharedUpdateStartInstance.current = null
                sharedUpdateStartVersion.current = null
                setReloadQueued(true)
                notifyUser('بازگشت به نسخهٔ قبلی کامل شد؛ پنل برای بارگذاری نسخهٔ فعال تازه می‌شود.')
                window.setTimeout(() => void forceFreshPanelReload(false), 1200)
              } else if (progress.state === 'cancelled') {
                setUpdateFailure(progress.message || 'به‌روزرسانی به درخواست مدیر لغو شد؛ نسخهٔ قبلی حفظ شده است.')
                setUpdateDialogOpen(true)
              } else if (progress.state === 'failed') {
                setUpdateFinished(false)
                setUpdateFailure(progress.message || 'فرایند به‌روزرسانی شکست خورد. گزارش را برای جزئیات بررسی کن.')
                setUpdateDialogOpen(true)
              } else if (latest.status === 'current' && (progress.state === 'succeeded' || restarted || versionChanged || !previousVersion)) {
                sharedUpdateWasActive.current = false
                sharedUpdateStartInstance.current = null
                sharedUpdateStartVersion.current = null
                finishUpdateAndRefresh()
              } else if (progress.state === 'succeeded' && latest.status === 'unavailable') {
                // The script records success just before removing its lock marker; verify again after cleanup.
                sharedUpdateWasActive.current = true
                sharedUpdateStartInstance.current = previousInstance
                sharedUpdateStartVersion.current = previousVersion
              } else if (restarted) {
                setUpdateFinished(false)
                setUpdateFailure(latest.status === 'available' ? 'نسخهٔ جدید نصب یا فعال نشد. گزارش فرایند را برای جزئیات بررسی کن.' : latest.error || 'سرویس دوباره راه‌اندازی شد اما نسخهٔ جدید تأیید نشد.')
                setUpdateDialogOpen(true)
              }
            } catch {
              if (progress.state === 'succeeded') {
                sharedUpdateWasActive.current = true
                sharedUpdateStartInstance.current = previousInstance
                sharedUpdateStartVersion.current = previousVersion
              } else if (progress.instance !== previousInstance) {
                setUpdateFinished(false)
                setUpdateFailure('سرویس پس از به‌روزرسانی دوباره راه‌اندازی شد، اما وضعیت نسخه قابل بررسی نیست. گزارش فرایند را باز کن.')
                setUpdateDialogOpen(true)
              }
            }
          }
        }
      } catch { /* Retry after transient API loss while the panel restarts. */ }
      if (!cancelled) timer = window.setTimeout(() => void poll(), 2000)
    }
    void poll()
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [installed, user, update.local])
  useEffect(() => {
    if (update.status === 'available' && !updating && !sharedUpdate.updating) notifyUser(`نسخهٔ جدید پنل ARG (${update.remote_version ? `v${update.remote_version}` : update.remote}) آماده است.`)
  }, [update.status, update.remote, update.remote_version, updating, sharedUpdate.updating])
  useEffect(() => { if (updateFinished) notifyUser('به‌روزرسانی کامل شد؛ صفحه تا چند لحظهٔ دیگر تازه می‌شود.') }, [updateFinished])
  async function openUpdateDialog() {
    setUpdateFailure(null)
    setUpdateDialogOpen(true)
    setCheckingUpdate(true)
    await checkUpdate()
    setCheckingUpdate(false)
  }
  function finishUpdateAndRefresh() {
    if (reloadScheduled.current) return
    reloadScheduled.current = true
    setUpdating(false)
    setUpdateStartedAt(null)
    setUpdateFailure(null)
    // Only the newly loaded frontend opens its own changelog.
    setUpdateFinished(false)
    setUpdateDialogOpen(false)
    setReloadQueued(true)
    window.setTimeout(() => void forceFreshPanelReload(true), 1200)
  }
  async function cancelUpdate() {
    if (user?.role !== 'admin' || cancelingUpdate || rollingBack) return
    setUpdateConfirmAction('cancel')
  }
  async function executeCancelUpdate() {
    if (user?.role !== 'admin' || cancelingUpdate) return
    setUpdateConfirmAction(null)
    setCancelingUpdate(true)
    try {
      const result = await api<{ message?: string }>('update/cancel', { method: 'POST' })
      const finalProgress = await api<UpdateProgress>('update/progress')
      if (finalProgress.updating || (finalProgress.state !== 'cancelled' && finalProgress.state !== 'failed')) {
        throw new Error('درخواست لغو ثبت شد اما پایان ایمن فرایند هنوز تأیید نشده است. وضعیت را چند لحظهٔ دیگر بررسی کن.')
      }
      setUpdating(false)
      setUpdateStartedAt(null)
      localUpdateStarted.current = false
      sharedUpdateWasActive.current = false
      setUpdateConfirmAction(null)
      setUpdateFailure(result.message || 'به‌روزرسانی لغو شد؛ نسخهٔ قبلی حفظ شده است.')
      setUpdateDialogOpen(true)
      notifyUser('به‌روزرسانی لغو شد؛ نسخهٔ قبلی حفظ شده است.')
    } catch (cause) {
      notifyUser(cause instanceof Error ? cause.message : 'لغو به‌روزرسانی ناموفق بود.')
    } finally { setCancelingUpdate(false) }
  }
  async function rollbackUpdate() {
    if (user?.role !== 'admin' || rollingBack || cancelingUpdate) return
    setUpdateConfirmAction('rollback')
  }
  async function executeRollbackUpdate() {
    if (user?.role !== 'admin' || rollingBack) return
    setUpdateConfirmAction(null)
    setRollingBack(true)
    try {
      await api('update/rollback', { method: 'POST' })
      setUpdateConfirmAction(null)
      setUpdating(false)
      setUpdateStartedAt(null)
      localUpdateStarted.current = false
      sharedUpdateWasActive.current = false
      notifyUser('بازگشت انجام شد؛ نسخهٔ قبلی سلامت سرویس را گذراند. پنل تازه‌سازی می‌شود.')
      setUpdateDialogOpen(false)
      setReloadQueued(true)
      window.setTimeout(() => void forceFreshPanelReload(false), 1200)
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'بازگشت به نسخهٔ قبلی ناموفق بود.'
      setUpdateFailure(message)
      setUpdateDialogOpen(true)
      notifyUser(message)
    } finally { setRollingBack(false) }
  }
  function changeUpdateBuildMode(mode: 'fast' | 'complete') {
    setUpdateBuildMode(mode)
    try { window.localStorage.setItem('arg.update-build-mode', mode) } catch { /* The selected mode still applies for this session. */ }
  }
  function changeUpdateBranch(branch: 'main' | 'development') {
    setUpdateBranch(branch)
    try { window.localStorage.setItem('arg.update-branch', branch) } catch { /* The selected branch still applies for this session. */ }
    setCheckingUpdate(true)
    void checkUpdate(false, branch).finally(() => setCheckingUpdate(false))
  }
  async function runUpdate(mode: 'fast' | 'complete') {
    if (user?.role !== 'admin' || updating) return
    setUpdateFailure(null)
    setUpdateFinished(false)
    setReloadQueued(false)
    reloadScheduled.current = false
    let baselineInstance = update.instance || null
    try {
      const progress = await api<UpdateProgress>('update/progress')
      baselineInstance = progress.instance || baselineInstance
    } catch { /* The updater can still schedule if the progress endpoint is temporarily unavailable. */ }
    updateStartInstance.current = baselineInstance
    localUpdateStarted.current = true
    setUpdateStage(1)
    setUpdateStartedAt(Date.now())
    setUpdating(true)
    try {
      await api(`update?mode=${mode}&branch=${updateBranch}`, { method: 'POST' })
      setUpdateDialogOpen(false)
    } catch (e) {
      setUpdating(false)
      setUpdateStartedAt(null)
      localUpdateStarted.current = false
      const message = e instanceof Error ? e.message : 'شروع به‌روزرسانی ناموفق بود.'
      setUpdateFailure(message)
      setUpdateDialogOpen(true)
      notifyUser(message)
    }
  }
  useEffect(() => {
    if (!updating) return
    let cancelled = false
    let timer = 0
    let currentSince: number | null = null
    const poll = async () => {
      if (cancelled) return
      try {
        const progress = await api<UpdateProgress>('update/progress')
        if (cancelled) return
        setUpdateStage((current) => Math.max(current, updateStageFor(progress.phase)) as UpdateStage)
        if (progress.job_id) currentUpdateJobId.current = progress.job_id
        if (progress.state === 'cancelled') {
          setUpdating(false)
          setUpdateStartedAt(null)
          localUpdateStarted.current = false
          setUpdateFailure(progress.message || 'به‌روزرسانی به درخواست مدیر لغو شد؛ نسخهٔ قبلی حفظ شده است.')
          setUpdateDialogOpen(true)
          return
        }
        if (progress.state === 'failed') {
          const message = progress.message || 'به‌روزرسانی ناموفق بود. گزارش فرایند را بررسی کن و سپس دوباره تلاش کن.'
          setUpdating(false)
          setUpdateStartedAt(null)
          localUpdateStarted.current = false
          setUpdateFailure(message)
          setUpdateDialogOpen(true)
          return
        }
        if (updateStartInstance.current && progress.instance && progress.instance !== updateStartInstance.current) {
          setUpdateStage(5)
        }
        if (progress.state === 'succeeded' && !progress.updating) {
          setUpdateStage(5)
          currentSince ??= Date.now()
          if (Date.now() - currentSince >= 2000) {
            finishUpdateAndRefresh()
            return
          }
        } else currentSince = null
        const updaterNeverStarted = progress.state === 'idle' && updateStartedAt !== null && Date.now() - updateStartedAt > 20_000
        if ((!progress.updating && progress.state !== 'succeeded' && progress.state !== 'idle') || updaterNeverStarted) {
          const message = 'فرایند پایان یافت اما نسخهٔ جدید به‌درستی فعال نشد.'
          setUpdating(false)
          setUpdateStartedAt(null)
          localUpdateStarted.current = false
          setUpdateFailure(message)
          setUpdateDialogOpen(true)
          return
        }
      } catch { /* Keep following the helper during brief API restarts; the helper's succeeded state confirms completion. */ }
      // Follow the helper until it reports completion or failure.
      timer = window.setTimeout(() => void poll(), 2500)
    }
    timer = window.setTimeout(() => void poll(), 2500)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [updating])
  async function logout() {
    try {
      await api('logout', { method: 'POST' })
      clearHeavyCache()
      try {
        if ('caches' in window) {
          const names = await window.caches.keys()
          await Promise.all(names.filter((name) => name.startsWith('arg-shell-')).map((name) => window.caches.delete(name)))
        }
      } catch { /* No authenticated response is cached; offline shell cleanup is best effort. */ }
      setActiveWorkspaceNode('')
      setUser(null)
      setPage('dashboard')
      window.location.hash = '/dashboard'
    } catch (e) {
      notifyUser(e instanceof Error ? e.message : 'خروج ناموفق بود')
    }
  }
  if (boot)
    return (
      <div className="min-h-screen p-5 text-muted-foreground">
        <OfflineNotice />
        <div className="flex min-h-[70vh] items-center justify-center gap-3"><Activity className="animate-pulse text-primary" /> در حال بارگذاری ارگ…</div>
      </div>
    )
  if (loadError)
    return (
      <div className="min-h-screen p-5">
        <OfflineNotice />
        <div className="flex min-h-[70vh] items-center justify-center">
        <Card className="max-w-md">
          <CardHeader>
            <CardTitle>اتصال برقرار نشد</CardTitle>
            <CardDescription>{loadError}</CardDescription>
          </CardHeader>
          <CardContent>
            <Button onClick={refresh}>تلاش دوباره</Button>
          </CardContent>
        </Card>
        </div>
      </div>
    )
  if (!installed || !user) return <Auth installer={!installed} onSuccess={refresh} />
  const allowedPages = navigation.filter((item) => !item.admin || user.role === 'admin')
  const currentPage = allowedPages.some((item) => item.id === page) ? page : 'dashboard'
  const currentPageLabel = allowedPages.find((item) => item.id === currentPage)?.label || 'نمای کلی'
  const activeWorkspace = workspaceNodes.find((node) => node.workspace_key === workspaceNodeId)
  const currentWorkspaceLabel = workspaceNodeId === 'all' ? 'همهٔ سرورها' : activeWorkspace?.public_name || activeWorkspace?.name || 'این VPS'
  const updateLocked = updating || reloadQueued || (!reloadScheduled.current && sharedUpdate.updating)
  const activeUpdateStage: UpdateStage = reloadQueued ? 5 : updating ? updateStage : sharedUpdateStage
  const activeUpdateStartedAt = updating ? updateStartedAt : sharedUpdate.started_at ? sharedUpdate.started_at * 1000 : null
  function selectPage(next: PageId) {
    if (!allowedPages.some((item) => item.id === next)) return
    setPage(next)
    if (next === 'terminal') setTerminalMounted(true)
    window.location.hash = `/${next}`
    setMobileOpen(false)
  }
  function changeWorkspaceMode(mode: WorkspaceMode) {
    setWorkspaceMode(mode)
    try { window.localStorage.setItem(WORKSPACE_MODE_KEY, mode) } catch { /* The current UI session can still use this preference. */ }
    if (mode === 'single' && workspaceNodeId) {
      setActiveWorkspaceNode('')
      setWorkspaceNodeId('')
      setTerminalMounted(false)
      clearHeavyCache()
      window.location.reload()
    }
  }
  function openWorkspace(key: string) {
    setActiveWorkspaceNode(key)
    setWorkspaceNodeId(key)
    setTerminalMounted(false)
    clearHeavyCache()
    selectPage('dashboard')
    window.location.reload()
  }
  return (
    <PanelTimeZoneContext.Provider value={panelTimeZone}>
      <AppShell
        collapsed={sidebarCollapsed}
        sidebar={<SidebarNavigation items={allowedPages} currentPage={currentPage} collapsed={sidebarCollapsed} mobileOpen={mobileOpen} disabled={updateLocked} mobileCloseButton={mobileCloseButton} onSelectPage={selectPage} onToggleCollapsed={() => setSidebarCollapsed((value) => !value)} onCloseMobile={() => setMobileOpen(false)} />}
      >
        {mobileOpen && (
          <button
            aria-label="بستن منو"
            className="fixed inset-0 z-30 bg-black/70 lg:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
        <div className="min-w-0" inert={updateLocked} aria-hidden={updateLocked}>
          <TopNavigation
            leading={
              <>
                <Button ref={mobileMenuButton} variant="ghost" size="icon" className="h-9 w-9 shrink-0 lg:hidden" onClick={() => setMobileOpen(true)} aria-label="باز کردن منو" aria-expanded={mobileOpen} aria-controls="arg-sidebar"><Menu size={19}/></Button>
                <span className="hidden text-[10px] font-semibold tracking-[.18em] text-primary lg:inline" dir="ltr">VPS CONTROL CENTER</span>
                <Badge variant="success" className="hidden sm:inline-flex"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400"/>LIVE</Badge>
              </>
            }
            breadcrumb={
              <>
                <span className="text-muted-foreground">سرور</span><ChevronLeft size={13} className="text-muted-foreground/70"/>
                <span className="max-w-[18rem] truncate font-semibold text-foreground" aria-current="page">{currentPageLabel}</span>
                <span className="hidden xl:inline text-muted-foreground">· {currentWorkspaceLabel}</span>
              </>
            }
            actions={
              <>
                {user.role === 'admin' && workspaceMode === 'multi' && <label className="mr-1 flex h-9 max-w-[148px] shrink-0 items-center rounded-lg border border-border/70 bg-background/70 px-2 sm:max-w-[180px]"><span className="sr-only">فضای کار</span><select aria-label="انتخاب فضای کار VPS" value={workspaceNodeId} onChange={(event) => { const key = event.target.value; if (key === 'all') { openWorkspace('all'); return } const selected = workspaceNodes.find((node) => node.workspace_key === key); if (key && !selected?.workspace_ready) { notifyUser('این Node برای فضای کار کامل آماده نیست؛ آن را حذف و دوباره جفت‌سازی کن.'); return } openWorkspace(key) }} className="min-w-0 max-w-full bg-transparent text-xs text-foreground outline-none"><option value="">این VPS</option><option value="all">همهٔ سرورها · فقط خواندنی</option>{workspaceNodes.map((node) => <option key={node.workspace_key} value={node.workspace_key} disabled={!node.workspace_ready}>{node.public_name && node.public_name !== node.name ? `${node.public_name} · ${node.name}` : node.public_name || node.name}{node.workspace_ready ? '' : ' · نیاز به جفت‌سازی'}</option>)}</select></label>}
                <ThemeToggle />
                <Button variant="ghost" size="icon" onClick={() => void openUpdateDialog()} disabled={checkingUpdate || updateLocked} aria-busy={checkingUpdate || updateLocked} aria-label="بررسی و به‌روزرسانی پنل" title={updateLocked ? 'به‌روزرسانی سرور در حال انجام است' : checkingUpdate ? 'در حال بررسی نسخه‌ها…' : 'بررسی و به‌روزرسانی پنل'} className="relative h-9 w-9 shrink-0 text-muted-foreground hover:text-primary"><RefreshCw size={17} className={checkingUpdate || updateLocked ? 'animate-spin' : ''}/>{update.status === 'available' && <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-amber-400"/>}</Button>
                <a href="https://github.com/themrcortex/arg" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-muted-foreground transition hover:bg-accent hover:text-foreground" aria-label="GitHub پروژه" title="GitHub پروژه"><Github size={18}/></a>
                <a href="https://themrcortex.github.io/arg/" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-muted-foreground transition hover:bg-accent hover:text-foreground" aria-label="مستندات ARG" title="مستندات ARG"><BookOpen size={17}/></a>
                <a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-muted-foreground transition hover:bg-accent hover:text-foreground" aria-label="راهنما و پشتیبانی" title="راهنما و پشتیبانی"><LifeBuoy size={18}/></a>
                <Badge variant="outline" className="hidden shrink-0 md:inline-flex">FA <span className="mr-1 text-muted-foreground">فارسی</span></Badge>
                <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" className="h-9 shrink-0 gap-2 rounded-lg px-2"><Avatar className="h-8 w-8"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="hidden max-w-24 truncate text-xs sm:inline">{user.username}</span><ChevronDown size={14} className="text-muted-foreground"/></Button></DropdownMenuTrigger><DropdownMenuContent align="end" sideOffset={8} collisionPadding={16}><DropdownMenuItem onSelect={() => selectPage('account')}><UserRound size={15}/>حساب کاربری</DropdownMenuItem><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات پنل</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
              </>
            }
          />
          <main className="arg-main-canvas relative mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            <OfflineNotice />
            {location.protocol !== 'https:' && <Alert className="mb-6 border-amber-400/30 bg-amber-400/[.07] text-amber-100"><AlertTriangle size={17}/><div className="min-w-0 flex-1"><AlertTitle>اتصال این پنل امن نیست</AlertTitle><AlertDescription className="text-amber-100/75">این صفحه با HTTP باز شده است؛ رمز عبور و نشست می‌توانند در مسیر شبکه افشا شوند. برای راه‌اندازی HTTPS، گواهی SSL را در تنظیمات ثبت کن.</AlertDescription></div><Button size="sm" variant="outline" className="shrink-0 border-amber-300/30" onClick={() => selectPage('settings')}>تنظیم گواهی SSL</Button></Alert>}
            {workspaceNodeId !== 'all' && <ResourceAlertNotice />}
            {terminalMounted && workspaceNodeId !== 'all' && user.role === 'admin' && <div className={currentPage === 'terminal' && pageReady ? 'arg-terminal-page' : 'hidden'} aria-hidden={currentPage !== 'terminal'} inert={currentPage !== 'terminal'}><Suspense fallback={currentPage === 'terminal' ? <PageRouteSkeleton label="ترمینال"/> : null}><TerminalPage Heading={PageHeading} visible={currentPage === 'terminal' && pageReady} /></Suspense></div>}
            <div key={currentPage}>
            {!pageReady ? <PageRouteSkeleton label={navigation.find((item) => item.id === currentPage)?.label || 'صفحه'}/> : <Suspense fallback={<PageRouteSkeleton label={navigation.find((item) => item.id === currentPage)?.label || 'صفحه'}/>}><div className="arg-page-enter">
            {currentPage === 'apps' && user.role === 'admin' ? <Suspense fallback={<PageRouteSkeleton label="اپلیکیشن‌ها"/>}><ApplicationsPage Heading={PageHeading} notify={notifyUser} allServers={workspaceNodeId === 'all'} workspaceKey={workspaceNodeId} onOpenWorkspace={openWorkspace}/></Suspense> : workspaceNodeId === 'all' ? <AllServersPage Heading={PageHeading} onOpen={openWorkspace}/> : <>
              {currentPage === 'dashboard' && <DashboardPage Heading={PageHeading} notify={notifyUser} user={user} />}
              {currentPage === 'system' && <SystemPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'nodes' && user.role === 'admin' && <NodesPage Heading={PageHeading} notify={notifyUser} workspaceMode={workspaceMode} onWorkspaceModeChange={changeWorkspaceMode} />}
              {currentPage === 'maintenance' && user.role === 'admin' && <MaintenancePage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'ports' && <PortsPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={notifyUser} user={user} />}
              {currentPage === 'nginx' && user.role === 'admin' && <NginxPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'docker' && user.role === 'admin' && <DockerPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'files' && user.role === 'admin' && <FileManagerPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 's3' && user.role === 'admin' && <Suspense fallback={<PageRouteSkeleton label="فضای ابری S3"/>}><S3StoragePage Heading={PageHeading} notify={notifyUser}/></Suspense>}
              {currentPage === 'dns' && user.role === 'admin' && <Suspense fallback={<PageRouteSkeleton label="مدیریت DNS"/>}><DnsPage Heading={PageHeading} notify={notifyUser}/></Suspense>}
              {currentPage === 'terminal' && null}
              {currentPage === 'packages' && user.role === 'admin' && <AptPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'services' && user.role === 'admin' && <ServicesPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'mysql' && user.role === 'admin' && <MysqlPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'users' && user.role === 'admin' && <UsersPage current={user} Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'audit' && user.role === 'admin' && <AuditPage Heading={PageHeading} notify={notifyUser} />}
              {currentPage === 'settings' && <SettingsPage Heading={PageHeading} notify={notifyUser} user={user} update={update} openUpdateDialog={() => void openUpdateDialog()} updating={updateLocked} checkingUpdate={checkingUpdate} panelTimeZone={panelTimeZone} onTimeZoneChange={setPanelTimeZone} />}
              {currentPage === 'account' && <AccountPage Heading={PageHeading} user={user} setUser={(next) => setUser(next)} logout={logout} notify={notifyUser} />}
              {currentPage === 'bug-report' && <BugReportPage Heading={PageHeading} notify={notifyUser} version={update.local_version} commit={update.local} page={currentPage} />}
            </>}
            <footer className="arg-page-footer mt-12 flex flex-wrap items-center justify-between gap-3 border-t border-border/70 pt-6 text-xs text-muted-foreground">
              <span>© {new Date().getFullYear()} ARG · Rust + React</span>
              <span>طراحی و توسعه: {developer}</span>
            </footer>
            </div></Suspense>}
            </div>
          </main>
        </div>
        <Toaster />
        <UpdateOverlay active={updateLocked} stage={activeUpdateStage} startedAt={activeUpdateStartedAt} shared={sharedUpdate.updating && !updating} rollback={sharedUpdate.operation === 'rollback' || (sharedUpdate.job_id?.startsWith('arg-rollback-') ?? false)} branch={sharedUpdate.branch} buildMode={sharedUpdate.build_mode} refreshing={reloadQueued} canCancel={user.role === 'admin'} canceling={cancelingUpdate} onCancel={() => void cancelUpdate()}/>
        <UpdateActionConfirmation action={updateConfirmAction} busy={cancelingUpdate || rollingBack} onCancel={() => setUpdateConfirmAction(null)} onConfirm={() => void (updateConfirmAction === 'rollback' ? executeRollbackUpdate() : executeCancelUpdate())}/>
        <UpdateDialog open={updateDialogOpen} onOpenChange={(open) => { if (!updateLocked) setUpdateDialogOpen(open); if (!open) { setUpdateFinished(false); setUpdateFailure(null); setReloadQueued(false) } }} update={update} checking={checkingUpdate} updating={updateLocked} finished={updateFinished} refreshing={reloadQueued} failure={updateFailure} canInstall={user.role === 'admin'} changelog={latestChangelog || 'در حال بارگذاری تغییرات نسخه…'} buildMode={updateBuildMode} onBuildModeChange={changeUpdateBuildMode} updateBranch={updateBranch} onUpdateBranchChange={changeUpdateBranch} onCheck={() => void openUpdateDialog()} onInstall={(mode) => void runUpdate(mode)} onRollback={() => void rollbackUpdate()} rollingBack={rollingBack} onForceRefresh={() => void forceFreshPanelReload(false)}/>
      </AppShell>
    </PanelTimeZoneContext.Provider>
  )
}

export { PageHeading }
