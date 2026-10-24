import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  AlertTriangle,
  ArrowDownToLine,
  ArrowLeft,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  Github,
  LifeBuoy,
  Menu,
  RefreshCw,
  Settings,
  UserRound,
  X,
} from 'lucide-react'
import { api, type Dashboard, type User } from './lib/api'
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
import { AccountPage, AuditPage, DashboardPage, FirewallPage, NetworkPage, NginxPage, PortsPage, SettingsPage, SystemPage, UsersPage } from './pages'
import { AptPage, ServicesPage } from './AdminOpsPage'
import { DockerPage } from './DockerPage'
import { FileManagerPage } from './FileManagerPage'
import { navigation, pageFromHash, type PageId } from './navigation'
import { toast } from 'sonner'
import { Toaster } from './components/ui/sonner'
import changelog from '../../CHANGELOG.md?raw'
const TerminalPage = lazy(() => import('./TerminalPage').then((module) => ({ default: module.TerminalPage })))
const notifyUser = (message: string) => {
  if (/ناموفق|خطا|نامعتبر|نشد|مشخص نشد/.test(message)) toast.error(message)
  else toast.success(message)
}

type AlertThresholds = { cpu: number; memory: number; disk: number }
type ResourceAlert = { key: string; label: string; value: number; threshold: number; critical: boolean }
type UpdateProgress = { updating: boolean; started_at: number | null; instance: string; job_id?: string | null; state?: 'idle' | 'running' | 'succeeded' | 'failed'; phase?: string; message?: string }
const updateStageFor = (phase?: string): 1 | 2 | 3 => {
  if (['building', 'installing'].includes(phase || '')) return 2
  if (['restarting', 'verifying', 'complete'].includes(phase || '')) return 3
  return 1
}

function ResourceAlertNotice() {
  const [alerts, setAlerts] = useState<ResourceAlert[]>([])
  const previousCpu = useRef<{ total: number; idle: number } | null>(null)
  useEffect(() => {
    let cancelled = false
    let timer = 0
    const poll = async () => {
      try {
        const [dashboard, thresholds] = await Promise.all([
          api<Dashboard>('dashboard'),
          api<AlertThresholds>('monitoring/alerts'),
        ])
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
        setAlerts(readings.flatMap((reading) => reading.value !== null && (reading.value >= reading.threshold || reading.value >= 95)
          ? [{ key: reading.key, label: reading.label, value: reading.value, threshold: reading.threshold, critical: reading.value >= 95 }]
          : []))
      } catch { /* An unavailable metrics read should not block or interrupt panel navigation. */ }
      if (!cancelled) timer = window.setTimeout(() => void poll(), 15000)
    }
    void poll()
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [])

  if (!alerts.length) return null
  const critical = alerts.some((alert) => alert.critical)
  return <Alert role="status" className={critical ? 'mb-5 border-rose-400/30 bg-rose-400/[.08] text-rose-100' : 'mb-5 border-amber-400/30 bg-amber-400/[.07] text-amber-100'}>
    <AlertTriangle size={17} className={critical ? 'text-rose-300' : 'text-amber-300'}/>
    <AlertTitle>{critical ? 'وضعیت بحرانی منابع سرور' : 'عبور از آستانهٔ هشدار'}</AlertTitle>
    <AlertDescription className="mt-1 flex flex-wrap gap-x-4 gap-y-1">
      {alerts.map((alert) => <span key={alert.key}>{alert.label}: <b className="font-mono" dir="ltr">{alert.value.toFixed(1)}%</b> <span className="text-current/70">(آستانهٔ تنظیم‌شده: <b className="font-mono" dir="ltr">{alert.threshold}%</b>)</span></span>)}
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
            <Badge className="mb-4 w-fit">{installer ? 'راه‌اندازی اولیه' : 'ورود امن'}</Badge>
            <CardTitle className="text-2xl">{installer ? 'به ارگ خوش آمدید' : 'خوش آمدید'}</CardTitle>
            <CardDescription className="leading-7">
              {installer
                ? 'حساب مدیر را بسازید تا مدیریت سرور را آغاز کنید.'
                : 'برای دسترسی به سرورتان وارد حساب خود شوید.'}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {installer && (
              <Alert className="mb-5 border-primary/30 bg-primary/10 text-foreground">
                <AlertDescription className="leading-7">
                  برای ساخت حساب از این نشانی عمومی، کد یک‌بارمصرف نمایش‌داده‌شده در کنسول سرور را
                  وارد کنید و مطمئن شوید صفحه با HTTPS باز شده است. ساخت حساب از اتصال SSH محلی هم
                  بدون کد امکان‌پذیر است.
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
                    placeholder="برای ورود عمومی، کد کنسول سرور را وارد کنید"
                  />
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

function UpdateDialog({ open, onOpenChange, update, checking, updating, finished, refreshing, failure, canInstall, changelog, onCheck, onInstall, onForceRefresh }: {
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
  onCheck: () => void
  onInstall: () => void
  onForceRefresh: () => void
}) {
  const [refreshingPanel, setRefreshingPanel] = useState(false)
  function refreshPanelManually() {
    setRefreshingPanel(true)
    onForceRefresh()
    window.setTimeout(() => setRefreshingPanel(false), 1800)
  }
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="max-h-[85vh] overflow-y-auto"><DialogHeader><DialogTitle>{failure ? 'به‌روزرسانی ناموفق بود' : finished ? 'به‌روزرسانی کامل شد' : 'به‌روزرسانی ARG'}</DialogTitle><DialogDescription>{failure ? 'نصب نسخه کامل نشد. گزارش زنده یا آخرین گزارش را برای جزئیات باز کن.' : finished ? refreshing ? 'نصب تمام شد؛ در حال آماده‌سازی صفحهٔ جدید هستیم.' : 'نسخهٔ جدید نصب و سرویس دوباره راه‌اندازی شد. تغییرات این نسخه:' : 'نسخهٔ نصب‌شده را با آخرین نسخهٔ GitHub بررسی کن.'}</DialogDescription></DialogHeader>
    {failure ? <div className="space-y-4"><div role="alert" className="rounded-xl border border-rose-400/25 bg-rose-400/[.07] p-4 text-sm text-rose-100"><div className="mb-1 flex items-center gap-2 font-semibold"><X size={17}/>به‌روزرسانی انجام نشد</div><p className="leading-6">{failure}</p></div><UpdateLogDetails/><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" disabled={checking || updating} onClick={onCheck}><RefreshCw size={15} className={checking ? 'animate-spin' : ''}/>بررسی دوباره</Button>{canInstall && update.status === 'available' && <Button disabled={checking || updating} onClick={onInstall}>تلاش دوباره برای نصب</Button>}<Button variant="ghost" onClick={() => onOpenChange(false)}>بستن</Button></div></div> : finished ? <div className="space-y-4"><div className="flex items-center gap-2 rounded-xl border border-emerald-400/20 bg-emerald-400/[.07] p-3 text-sm text-emerald-200"><CheckCircle2 size={18}/>{refreshing ? 'به‌روزرسانی با موفقیت انجام شد؛ در حال ساختن و بارگذاری صفحهٔ جدید…' : 'به‌روزرسانی با موفقیت انجام شد.'}</div><p className="text-xs leading-6 text-muted-foreground">رفرش خودکار انجام می‌شود. اگر پنل تازه نشد یا بخشی درست بارگذاری نشد، دکمهٔ زیر کش را پاک می‌کند و رفرش کامل انجام می‌دهد.</p><div className="max-h-[42vh] overflow-y-auto rounded-xl border border-border bg-background p-4"><ChangelogView content={changelog}/></div><UpdateLogDetails/><div className="grid gap-2 sm:grid-cols-2"><Button onClick={refreshPanelManually} aria-busy={refreshingPanel}><RefreshCw size={15} className={refreshingPanel ? 'animate-spin' : ''}/>{refreshingPanel ? 'در حال پاک‌سازی و رفرش…' : 'رفرش کامل صفحه'}</Button><Button variant="outline" onClick={() => onOpenChange(false)}>متوجه شدم</Button></div></div> : <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3"><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.local_version ? `v${update.local_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.local || '—'}</div></div><div className="rounded-xl border border-border bg-secondary/35 p-4"><div className="text-xs text-muted-foreground">آخرین نسخه</div><div className="mt-2 truncate font-mono text-2xl font-bold tracking-tight text-foreground sm:text-3xl" dir="ltr">{update.remote_version ? `v${update.remote_version}` : '—'}</div><div className="mt-2 truncate font-mono text-[10px] text-muted-foreground" dir="ltr">COMMIT {update.remote || '—'}</div></div></div>
      {checking ? <div role="status" className="flex items-center gap-2 rounded-xl border border-border p-4 text-sm text-muted-foreground"><RefreshCw size={16} className="animate-spin text-primary"/>در حال بررسی نسخهٔ GitHub…</div> : update.status === 'available' ? <div className="rounded-xl border border-amber-400/25 bg-amber-400/[.06] p-4 text-sm text-amber-100">نسخهٔ جدید {update.remote_version ? `v${update.remote_version}` : update.remote} آمادهٔ نصب است.</div> : update.status === 'current' ? <div className="rounded-xl border border-emerald-400/20 bg-emerald-400/[.06] p-4 text-sm text-emerald-200">پنل به‌روز است.</div> : <div className="rounded-xl border border-rose-400/20 bg-rose-400/[.06] p-4 text-sm text-rose-200">{update.error || 'بررسی نسخه انجام نشد. اتصال سرور و دسترسی helper را بررسی کن.'}</div>}
      <UpdateLogDetails/><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" disabled={checking || updating} onClick={onCheck}><RefreshCw size={15} className={checking ? 'animate-spin' : ''}/>بررسی دوباره</Button>{canInstall && update.status === 'available' && <Button disabled={checking || updating} onClick={onInstall}>نصب نسخهٔ جدید</Button>}</div>
    </div>}
  </DialogContent></Dialog>
}

function UpdateOverlay({ active, stage, startedAt, shared }: { active: boolean; stage: 1 | 2 | 3; startedAt: number | null; shared: boolean }) {
  const [elapsed, setElapsed] = useState(0)
  const overlayRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!active || !startedAt) { setElapsed(0); return }
    const tick = () => setElapsed(Math.floor((Date.now() - startedAt) / 1000))
    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [active, startedAt])
  useEffect(() => { if (active) overlayRef.current?.focus() }, [active])
  const elapsedLabel = `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`
  if (!active) return null
  const phaseLabels = ['آماده‌سازی و دریافت', 'ساخت و نصب', 'راه‌اندازی و بررسی']
  return <div ref={overlayRef} tabIndex={-1} className="fixed inset-0 z-[100] grid place-items-center bg-[#080d12]/95 p-5 backdrop-blur-sm outline-none" role="alertdialog" aria-modal="true" aria-labelledby="update-overlay-title" aria-describedby="update-overlay-description">
    <div className="max-h-[calc(100dvh-2.5rem)] w-full max-w-lg overflow-y-auto rounded-2xl border border-border bg-card p-5 text-center shadow-2xl sm:p-7"><div className="mx-auto grid h-12 w-12 place-items-center rounded-xl border border-primary/20 bg-primary/[.08] text-primary"><RefreshCw size={21} className="animate-spin"/></div><h2 id="update-overlay-title" className="mt-4 text-xl font-bold text-foreground">در حال به‌روزرسانی ARG</h2><p id="update-overlay-description" className="mt-2 text-sm leading-7 text-muted-foreground">{shared ? 'یک نشست دیگر این سرور را به‌روزرسانی می‌کند. پنل موقتاً برای جلوگیری از تداخل قفل شده است.' : stage === 1 ? 'در حال آماده‌سازی و دریافت نسخهٔ تازه…' : stage === 3 ? 'سرویس در حال راه‌اندازی و بررسی نسخهٔ نصب‌شده است…' : 'نسخهٔ تازه در حال ساخت و نصب است؛ این مرحله ممکن است چند دقیقه طول بکشد.'}</p>
      <div className="mt-6 rounded-xl border border-border bg-background/55 p-4 text-right shadow-inner"><div className="mb-4 flex items-center justify-between gap-4"><div><p className="text-[11px] text-muted-foreground">مرحلهٔ جاری</p><p className="mt-1 font-semibold text-foreground">{phaseLabels[stage - 1]}</p></div><span className="rounded-md border border-border bg-card px-2.5 py-1 font-mono text-xs tabular-nums text-muted-foreground" dir="ltr">{elapsedLabel}</span></div><div className="flex gap-2" role="progressbar" aria-label="مراحل به‌روزرسانی" aria-valuemin={0} aria-valuemax={3} aria-valuenow={stage - 1} aria-valuetext={`${phaseLabels[stage - 1]}؛ مرحلهٔ ${stage} از 3`}>{phaseLabels.map((label, index) => <span key={label} className="relative h-2 flex-1 overflow-hidden rounded-full bg-secondary"><span className={cn('absolute inset-0 rounded-full transition-colors duration-500', index < stage - 1 ? 'bg-primary' : index === stage - 1 ? 'arg-update-active-segment' : 'bg-transparent')}/></span>)}</div><div className="mt-3 grid grid-cols-3 gap-2 text-[10px]"><span className={stage >= 1 ? 'font-medium text-primary' : 'text-muted-foreground'}>آماده‌سازی</span><span className={stage >= 2 ? 'font-medium text-primary' : 'text-muted-foreground'}>ساخت و نصب</span><span className={stage >= 3 ? 'font-medium text-primary' : 'text-muted-foreground'}>بررسی نهایی</span></div></div>
      <div className="mt-4 text-right"><UpdateLogDetails active/></div><p className="mt-3 text-xs text-muted-foreground" role="status" aria-live="polite">لطفاً تا پایان فرایند این صفحه را نبند.</p></div>
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
    <div className="mb-7 flex flex-wrap items-end justify-between gap-4" dir="rtl">
      <div className="min-w-0 text-right" dir="rtl">
        <div className="mb-4 flex min-h-5 items-center justify-start text-xs font-bold tracking-[.12em] text-primary">
          <span className="inline-flex items-center gap-2 leading-5" dir="ltr" style={{ unicodeBidi: 'isolate' }}><span aria-hidden="true" className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-400"/>SERVER CONTROL</span>
        </div>
        <h1 dir="rtl" className="mt-0 block text-3xl font-extrabold leading-[1.4] tracking-tight text-white md:text-[34px]">{title}</h1>
        <p dir="rtl" className="mt-2 text-sm leading-7 text-muted-foreground">{description}</p>
      </div>
      {action}
    </div>
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
  const [panelTimeZone, setPanelTimeZone] = useState('UTC')
  const [page, setPage] = useState<PageId>(() => pageFromHash(window.location.hash))
  const [pageReady, setPageReady] = useState(false)
  const [mobileOpen, setMobileOpen] = useState(false)
  const mobileMenuButton = useRef<HTMLButtonElement>(null)
  const mobileCloseButton = useRef<HTMLButtonElement>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try { return window.localStorage.getItem('arg.arg-sidebar-collapsed') === 'true' } catch { return false }
  })
  const [loadError, setLoadError] = useState('')
  const [update, setUpdate] = useState<{ status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; local_version?: string; remote_version?: string; error?: string; instance?: string }>({ status: 'unavailable' })
  const [updating, setUpdating] = useState(false)
  const [updateFailure, setUpdateFailure] = useState<string | null>(null)
  const [updateStage, setUpdateStage] = useState<1 | 2 | 3>(1)
  const [updateStartedAt, setUpdateStartedAt] = useState<number | null>(null)
  const [sharedUpdate, setSharedUpdate] = useState<UpdateProgress>({ updating: false, started_at: null, instance: '' })
  const [sharedUpdateStage, setSharedUpdateStage] = useState<1 | 2 | 3>(1)
  const sharedUpdateWasActive = useRef(false)
  const sharedUpdateStartInstance = useRef<string | null>(null)
  const sharedUpdateStartVersion = useRef<string | null>(null)
  const localUpdateStarted = useRef(false)
  const updateStartInstance = useRef<string | null>(null)
  const currentUpdateJobId = useRef<string | null>(null)
  const reloadScheduled = useRef(false)
  const stalledJobId = useRef<string | null>(null)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false)
  const [reloadQueued, setReloadQueued] = useState(false)
  const [updateFinished, setUpdateFinished] = useState(false)
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
  useEffect(() => { if (updateReloadNotice) notifyUser('به‌روزرسانی کامل شد و صفحه با نسخهٔ تازه بارگذاری شد.') }, [updateReloadNotice])
  useEffect(() => {
    const onHash = () => {
      setPage(pageFromHash(window.location.hash))
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
  const checkUpdate = useCallback(async (announce = false) => {
    if (!installed || !user) return
    try {
      const result = await api<typeof update>('update')
      setUpdate(result)
      if (announce) notifyUser(result.status === 'available' ? `نسخهٔ جدید پنل ARG (${result.remote_version ? `v${result.remote_version}` : result.remote}) آماده است.` : result.status === 'current' ? 'پنل ARG به‌روز است.' : result.error || 'بررسی نسخه انجام نشد.')
    } catch (error) {
      setUpdate({ status: 'unavailable', error: error instanceof Error ? error.message : 'بررسی نسخه انجام نشد.' })
      if (announce) notifyUser(error instanceof Error ? error.message : 'بررسی آپدیت ناموفق بود')
    }
  }, [installed, user])
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
        const staleForRecovery = Boolean(progress.updating && progress.started_at && Date.now() - progress.started_at * 1000 >= 10 * 60 * 1000)
        if (progress.job_id && progress.job_id === stalledJobId.current && progress.updating) {
          setSharedUpdate((current) => ({ ...current, ...progress, updating: false }))
          setUpdateFailure('فرایند به‌روزرسانی بیش از 10 دقیقه بدون نتیجه مانده است. پنل از حالت قفل خارج شد؛ گزارش را بررسی و وضعیت را دوباره بررسی کن.')
          setUpdateDialogOpen(true)
        } else {
          setSharedUpdate((current) => current.updating === progress.updating && current.started_at === progress.started_at && current.instance === progress.instance && current.state === progress.state ? current : progress)
        }
        if (staleForRecovery && progress.job_id && stalledJobId.current !== progress.job_id) {
          stalledJobId.current = progress.job_id
          setSharedUpdate((current) => ({ ...current, ...progress, updating: false }))
          setUpdateFailure('فرایند به‌روزرسانی بیش از 10 دقیقه بدون نتیجه مانده است. پنل از حالت قفل خارج شد؛ گزارش را بررسی و وضعیت را دوباره بررسی کن.')
          setUpdateDialogOpen(true)
        }
        if (progress.updating && !(progress.job_id && progress.job_id === stalledJobId.current)) {
          setSharedUpdateStage(updateStageFor(progress.phase))
          if (!sharedUpdateWasActive.current) {
            sharedUpdateWasActive.current = true
            sharedUpdateStartInstance.current = progress.instance
            sharedUpdateStartVersion.current = update.local || null
            setSharedUpdateStage(2)
          } else if (sharedUpdateStartInstance.current && progress.instance !== sharedUpdateStartInstance.current) {
            setSharedUpdateStage(3)
          }
        } else if (!progress.updating && sharedUpdateWasActive.current) {
          sharedUpdateWasActive.current = false
          const previousInstance = sharedUpdateStartInstance.current
          const previousVersion = sharedUpdateStartVersion.current
          sharedUpdateStartInstance.current = null
          sharedUpdateStartVersion.current = null
          setSharedUpdateStage(1)
          if (!localUpdateStarted.current && previousInstance) {
            try {
              const latest = await api<typeof update>('update')
              if (cancelled) return
              setUpdate(latest)
              const restarted = progress.instance !== previousInstance
              const versionChanged = Boolean(previousVersion && latest.local && latest.local !== previousVersion)
              if (progress.state === 'failed') {
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
    setUpdateFinished(true)
    setReloadQueued(true)
    setUpdateDialogOpen(true)
    window.setTimeout(() => void forceFreshPanelReload(), 3000)
  }
  async function runUpdate() {
    if (user?.role !== 'admin' || updating) return
    setUpdateFailure(null)
    setUpdateFinished(false)
    setReloadQueued(false)
    reloadScheduled.current = false
    stalledJobId.current = null
    let baselineInstance = update.instance || null
    try {
      const fresh = await api<typeof update>('update')
      setUpdate(fresh)
      baselineInstance = fresh.instance || baselineInstance
    } catch { /* The update endpoint may still schedule the helper if status checking is temporarily unavailable. */ }
    updateStartInstance.current = baselineInstance
    localUpdateStarted.current = true
    setUpdateStage(1)
    setUpdateStartedAt(Date.now())
    setUpdating(true)
    try {
      await api('update', { method: 'POST' })
      setUpdateStage(2)
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
    let attempts = 0
    let timer = 0
    let currentSince: number | null = null
    const poll = async () => {
      if (cancelled) return
      attempts += 1
      try {
        const [result, progress] = await Promise.all([
          api<typeof update>('update'),
          api<UpdateProgress>('update/progress'),
        ])
        if (cancelled) return
        setUpdate(result)
        setUpdateStage(updateStageFor(progress.phase))
        if (progress.job_id) currentUpdateJobId.current = progress.job_id
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
          setUpdateStage(3)
        }
        const processRestarted = Boolean(updateStartInstance.current && progress.instance && progress.instance !== updateStartInstance.current)
        const updateConfirmed = progress.state === 'succeeded' && (!progress.updating || processRestarted || !updateStartInstance.current)
        if (result.status === 'current' && updateConfirmed) {
          setUpdateStage(3)
          currentSince ??= Date.now()
          if (Date.now() - currentSince >= 5000) {
            try { window.sessionStorage.setItem('arg-update-finished', 'true') } catch { /* Session storage may be disabled. */ }
            finishUpdateAndRefresh()
            return
          }
        } else currentSince = null
        if (!progress.updating && progress.state !== 'succeeded' && result.status !== 'current') {
          const message = result.error && result.error !== 'ARG_UPDATE_IN_PROGRESS'
            ? result.error
            : 'فرایند پایان یافت اما نسخهٔ جدید به‌درستی فعال نشد.'
          setUpdating(false)
          setUpdateStartedAt(null)
          localUpdateStarted.current = false
          setUpdateFailure(message)
          setUpdateDialogOpen(true)
          return
        }
      } catch { /* Wait for the restarted panel API; an unavailable status is not proof the update completed. */ }
      if (attempts >= 400) {
        const stalledId = currentUpdateJobId.current || sharedUpdate.job_id
        if (stalledId) stalledJobId.current = stalledId
        setUpdating(false)
        setUpdateStartedAt(null)
        localUpdateStarted.current = false
        const message = 'وضعیت به‌روزرسانی پس از 10 دقیقه مشخص نشد. گزارش فرایند را بررسی کن و در صورت نیاز رفرش کامل انجام بده.'
        setUpdateFailure(message)
        setUpdateDialogOpen(true)
        notifyUser(message)
        return
      }
      timer = window.setTimeout(() => void poll(), 1500)
    }
    timer = window.setTimeout(() => void poll(), 2500)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [updating])
  async function logout() {
    try {
      await api('logout', { method: 'POST' })
      clearHeavyCache()
      setUser(null)
      setPage('dashboard')
      window.location.hash = '/dashboard'
    } catch (e) {
      notifyUser(e instanceof Error ? e.message : 'خروج ناموفق بود')
    }
  }
  if (boot)
    return (
      <div className="flex min-h-screen items-center justify-center gap-3 text-muted-foreground">
        <Activity className="animate-pulse text-primary" /> در حال بارگذاری ارگ…
      </div>
    )
  if (loadError)
    return (
      <div className="flex min-h-screen items-center justify-center p-5">
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
    )
  if (!installed || !user) return <Auth installer={!installed} onSuccess={refresh} />
  const allowedPages = navigation.filter((item) => !item.admin || user.role === 'admin')
  const currentPage = allowedPages.some((item) => item.id === page) ? page : 'dashboard'
  const updateLocked = updating || (!reloadScheduled.current && sharedUpdate.updating && !(sharedUpdate.job_id && sharedUpdate.job_id === stalledJobId.current))
  const activeUpdateStage = updating ? updateStage : sharedUpdateStage
  const activeUpdateStartedAt = updating ? updateStartedAt : sharedUpdate.started_at ? sharedUpdate.started_at * 1000 : null
  function selectPage(next: PageId) {
    if (!allowedPages.some((item) => item.id === next)) return
    setPage(next)
    window.location.hash = `/${next}`
    setMobileOpen(false)
  }
  return (
    <PanelTimeZoneContext.Provider value={panelTimeZone}>
      <div className="app-shell min-h-screen lg:grid" style={{ gridTemplateColumns: `${sidebarCollapsed ? 56 : 224}px minmax(0, 1fr)` }}>
        <aside id="arg-sidebar" inert={updateLocked} aria-hidden={updateLocked} role={mobileOpen ? 'dialog' : undefined} aria-modal={mobileOpen ? true : undefined} aria-label="ناوبری اصلی" tabIndex={-1} className={cn('arg-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(80vw,260px)] max-w-[100vw] flex-col border-l px-3 py-4 transition-transform lg:sticky lg:top-0 lg:h-screen lg:translate-x-0', sidebarCollapsed ? 'arg-sidebar-collapsed lg:w-[56px]' : 'lg:w-[224px]', mobileOpen ? 'translate-x-0' : 'translate-x-full')}>
          <div className="arg-sidebar-pattern" aria-hidden="true" />
          <div className="arg-sidebar-corner" aria-hidden="true" />
          <div className="arg-brand-row relative z-10 flex min-h-[78px] items-center justify-between gap-1">
            <div className="arg-brand-lockup flex min-w-0 flex-1 items-center gap-1.5" aria-label="ARG VPS Control Manager">
              <img src="/arg/brand/arg-symbol.svg" alt="" className="arg-brand-mark h-9 w-9 shrink-0" />
              <div className="arg-brand-copy min-w-0"><div className="arg-wordmark" dir="ltr">ARG</div><div className="arg-product-label" dir="ltr"><span>VPS CONTROL</span><span>MANAGER</span></div></div>
            </div>
            <Button variant="ghost" size="icon" className="arg-collapse hidden h-8 w-8 shrink-0 lg:inline-flex" onClick={() => setSidebarCollapsed((value) => !value)} aria-label={sidebarCollapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'} aria-expanded={!sidebarCollapsed} title={sidebarCollapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'}>{sidebarCollapsed ? <ChevronLeft size={18} /> : <ChevronRight size={18} />}</Button>
            <Button ref={mobileCloseButton} variant="ghost" size="icon" className="arg-mobile-close h-8 w-8 shrink-0 lg:hidden" onClick={() => setMobileOpen(false)} aria-label="بستن منو"><X size={18}/></Button>
          </div>
          <div className="arg-divider relative z-10 mx-2 my-3" aria-hidden="true" />
          <div className="arg-navigation relative z-10 flex-1 space-y-5 overflow-y-auto px-1 pb-4">
            {[
              { title: 'نمای کلی', pages: allowedPages.filter((item) => item.id === 'dashboard') },
              { title: 'زیرساخت', pages: allowedPages.filter((item) => ['system', 'network', 'ports', 'firewall', 'nginx', 'docker', 'files', 'terminal'].includes(item.id)) },
              { title: 'عملیات سیستم', pages: allowedPages.filter((item) => ['packages', 'services'].includes(item.id)) },
              { title: 'مدیریت', pages: allowedPages.filter((item) => ['users', 'audit', 'settings', 'account'].includes(item.id)) },
            ].map((section) => <nav key={section.title} aria-label={section.title}><div className="arg-section-label mb-1.5 px-3">{section.title}</div><div className="space-y-1">{section.pages.map((item) => <button key={item.id} onClick={() => selectPage(item.id)} aria-current={currentPage === item.id ? 'page' : undefined} title={sidebarCollapsed ? item.label : undefined} className={cn('arg-nav-item group flex min-h-[46px] w-full items-center gap-3 rounded-[11px] px-3 text-right text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]', currentPage === item.id ? 'is-active' : '')}><item.icon className="arg-nav-icon shrink-0" size={21} strokeWidth={1.8}/><span className="arg-nav-label flex-1">{item.label}</span></button>)}</div></nav>)}
          </div>
          <div className="arg-sidebar-footer relative z-10 mt-auto pt-2">
            <div className="arg-divider mx-2 mb-3" aria-hidden="true" />
            <div className={cn('arg-service-status mb-2 flex min-h-8 items-center gap-2 px-3 text-[11px]', sidebarCollapsed && 'justify-center px-0')} title={sidebarCollapsed ? 'سرویس متصل' : undefined}><span className="h-2 w-2 shrink-0 rounded-full bg-emerald-400"/><span className="arg-nav-label">سرویس متصل</span></div>
            <DropdownMenu><DropdownMenuTrigger asChild><button className="arg-account flex min-h-[54px] w-full items-center gap-2.5 rounded-[11px] border px-2.5 text-right transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]" title={sidebarCollapsed ? user.username : undefined}><Avatar className="h-9 w-9 shrink-0"><AvatarFallback className="arg-avatar-fallback">{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="arg-account-copy min-w-0 flex-1"><span className="block truncate text-xs font-semibold">{user.username}</span><span className="mt-0.5 block text-[10px] text-muted-foreground">{user.role === 'admin' ? 'مدیر سیستم' : user.role === 'operator' ? 'اپراتور' : 'ناظر'}</span></span><ChevronDown className="arg-account-chevron" size={15}/></button></DropdownMenuTrigger><DropdownMenuContent align="end" side="top" collisionPadding={16} className="w-52"><DropdownMenuItem onSelect={() => selectPage('account')}><UserRound size={15}/>حساب کاربری</DropdownMenuItem><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات پنل</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
          </div>
          <img className="arg-fortress" src="/arg/illustrations/arg-fortress-minimal.svg" alt="" aria-hidden="true" />
        </aside>
        {mobileOpen && (
          <button
            aria-label="بستن منو"
            className="fixed inset-0 z-30 bg-black/70 lg:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
        <div className="min-w-0" inert={updateLocked} aria-hidden={updateLocked}>
          <header className="utility-bar sticky top-0 z-20 flex h-[62px] items-center justify-between gap-3 border-b border-white/10 px-5 md:px-8">
            <div className="flex items-center gap-2"><Button ref={mobileMenuButton} variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(true)} aria-label="باز کردن منو" aria-expanded={mobileOpen} aria-controls="arg-sidebar"><Menu size={19}/></Button><span className="hidden text-[10px] font-semibold tracking-[.18em] text-primary sm:inline" dir="ltr">VPS CONTROL CENTER</span><span className="hidden h-4 w-px bg-white/10 sm:inline-block"/><Badge variant="success" className="hidden sm:inline-flex"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400"/>LIVE</Badge></div>
            <div className="flex items-center gap-1.5"><Button variant="ghost" size="icon" onClick={() => void openUpdateDialog()} disabled={checkingUpdate || updateLocked} aria-busy={checkingUpdate || updateLocked} aria-label="بررسی و به‌روزرسانی پنل" title={updateLocked ? 'به‌روزرسانی سرور در حال انجام است' : checkingUpdate ? 'در حال بررسی نسخه‌ها…' : 'بررسی و به‌روزرسانی پنل'} className="relative text-muted-foreground hover:text-primary"><RefreshCw size={17} className={checkingUpdate || updateLocked ? 'animate-spin' : ''}/>{update.status === 'available' && <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-amber-400"/>}</Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="GitHub پروژه"><Github size={18}/></a><a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="راهنما و پشتیبانی"><LifeBuoy size={18}/></a><span className="mx-1 h-5 w-px bg-white/10"/><Badge variant="outline" className="hidden sm:inline-flex">FA <span className="mr-1 text-muted-foreground">فارسی</span></Badge><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" className="h-10 gap-2 rounded-lg px-2"><Avatar className="h-8 w-8"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="hidden text-xs sm:inline">{user.username}</span><ChevronDown size={14} className="text-muted-foreground"/></Button></DropdownMenuTrigger><DropdownMenuContent align="end" sideOffset={8} collisionPadding={16}><DropdownMenuItem onSelect={() => selectPage('account')}><UserRound size={15}/>حساب کاربری</DropdownMenuItem><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات پنل</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>
          </header>
          <main className="mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            <ResourceAlertNotice />
            <div key={currentPage}>
            {!pageReady ? <PageRouteSkeleton label={navigation.find((item) => item.id === currentPage)?.label || 'صفحه'}/> : <div className="arg-page-enter">
            {currentPage === 'dashboard' && <DashboardPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'system' && <SystemPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'ports' && <PortsPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={notifyUser} user={user} />}
            {currentPage === 'nginx' && user.role === 'admin' && <NginxPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'docker' && user.role === 'admin' && <DockerPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'files' && user.role === 'admin' && <FileManagerPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'terminal' && user.role === 'admin' && <Suspense fallback={<PageRouteSkeleton label="ترمینال"/>}><TerminalPage Heading={PageHeading} /></Suspense>}
            {currentPage === 'packages' && user.role === 'admin' && <AptPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'services' && user.role === 'admin' && <ServicesPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={notifyUser} />
            )}
            {currentPage === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={notifyUser} />
            )}
            {currentPage === 'settings' && <SettingsPage Heading={PageHeading} notify={notifyUser} user={user} update={update} openUpdateDialog={() => void openUpdateDialog()} updating={updateLocked} checkingUpdate={checkingUpdate} panelTimeZone={panelTimeZone} onTimeZoneChange={setPanelTimeZone} />}
            {currentPage === 'account' && <AccountPage Heading={PageHeading} user={user} setUser={(next) => setUser(next)} logout={logout} notify={notifyUser} />}
            <footer className="mt-12 flex flex-wrap items-center justify-between gap-3 border-t border-border/70 pt-6 text-xs text-muted-foreground">
              <span>© {new Date().getFullYear()} ARG · Rust + React</span>
              <span>طراحی و توسعه: {developer}</span>
            </footer>
            </div>}
            </div>
          </main>
        </div>
        <Toaster />
        <UpdateOverlay active={updateLocked} stage={activeUpdateStage} startedAt={activeUpdateStartedAt} shared={sharedUpdate.updating && !updating}/>
        <UpdateDialog open={updateDialogOpen} onOpenChange={(open) => { if (!updateLocked) setUpdateDialogOpen(open); if (!open) { setUpdateFinished(false); setUpdateFailure(null); setReloadQueued(false) } }} update={update} checking={checkingUpdate} updating={updateLocked} finished={updateFinished} refreshing={reloadQueued} failure={updateFailure} canInstall={user.role === 'admin'} changelog={changelog} onCheck={() => void openUpdateDialog()} onInstall={() => void runUpdate()} onForceRefresh={() => void forceFreshPanelReload()}/>
      </div>
    </PanelTimeZoneContext.Provider>
  )
}

export { PageHeading }
