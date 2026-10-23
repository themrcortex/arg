import { lazy, Suspense, useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowLeft,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Eye,
  EyeOff,
  Github,
  LifeBuoy,
  LogOut,
  Menu,
  RefreshCw,
  Settings,
  X,
} from 'lucide-react'
import { api, type User } from './lib/api'
import { cn } from './lib/utils'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Alert, AlertDescription } from './components/ui/alert'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Avatar, AvatarFallback } from './components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './components/ui/dropdown-menu'
import { AuditPage, DashboardPage, FirewallPage, NetworkPage, NginxPage, PortsPage, SettingsPage, SystemPage, UsersPage } from './pages'
import { navigation, pageFromHash, type PageId } from './navigation'
import { toast } from 'sonner'
import { Toaster } from './components/ui/sonner'
import changelog from '../../CHANGELOG.md?raw'
const TerminalPage = lazy(() => import('./TerminalPage').then((module) => ({ default: module.TerminalPage })))
const notifyUser = (message: string) => {
  if (/ناموفق|خطا|نامعتبر|نشد|مشخص نشد/.test(message)) toast.error(message)
  else toast.success(message)
}
async function forceFreshPanelReload() {
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
  try { window.sessionStorage.setItem('arg-update-finished', 'true') } catch { /* Keep the reload available when storage is blocked. */ }
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

function UpdateDialog({ open, onOpenChange, update, checking, updating, finished, canInstall, changelog, onCheck, onInstall, onForceRefresh }: {
  open: boolean
  onOpenChange: (open: boolean) => void
  update: { status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; error?: string }
  checking: boolean
  updating: boolean
  finished: boolean
  canInstall: boolean
  changelog: string
  onCheck: () => void
  onInstall: () => void
  onForceRefresh: () => void
}) {
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent className="max-h-[85vh] overflow-y-auto"><DialogHeader><DialogTitle>{finished ? 'به‌روزرسانی کامل شد' : 'به‌روزرسانی ARG'}</DialogTitle><DialogDescription>{finished ? 'نسخهٔ جدید نصب و سرویس دوباره راه‌اندازی شد. تغییرات این نسخه:' : 'نسخهٔ نصب‌شده را با آخرین نسخهٔ GitHub بررسی کن.'}</DialogDescription></DialogHeader>
    {finished ? <div className="space-y-4"><div className="flex items-center gap-2 rounded-xl border border-emerald-400/20 bg-emerald-400/[.07] p-3 text-sm text-emerald-200"><CheckCircle2 size={18}/>به‌روزرسانی با موفقیت انجام شد.</div><p className="text-xs leading-6 text-muted-foreground">اگر صفحه هنوز نسخهٔ قبلی را نشان می‌دهد، کش را پاک کن و رفرش کامل انجام بده.</p><div className="max-h-[42vh] overflow-y-auto rounded-xl border border-border bg-background p-4"><ChangelogView content={changelog}/></div><div className="grid gap-2 sm:grid-cols-2"><Button onClick={onForceRefresh}><RefreshCw size={15}/>پاک‌سازی کش و رفرش کامل</Button><Button variant="outline" onClick={() => onOpenChange(false)}>متوجه شدم</Button></div></div> : <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3"><div className="rounded-xl border border-border bg-secondary/35 p-3"><div className="text-xs text-muted-foreground">نسخهٔ نصب‌شده</div><div className="mt-2 truncate font-mono text-sm text-foreground" dir="ltr">{update.local || '—'}</div></div><div className="rounded-xl border border-border bg-secondary/35 p-3"><div className="text-xs text-muted-foreground">آخرین نسخه</div><div className="mt-2 truncate font-mono text-sm text-foreground" dir="ltr">{update.remote || '—'}</div></div></div>
      {checking ? <div role="status" className="flex items-center gap-2 rounded-xl border border-border p-4 text-sm text-muted-foreground"><RefreshCw size={16} className="animate-spin text-primary"/>در حال بررسی نسخهٔ GitHub…</div> : update.status === 'available' ? <div className="rounded-xl border border-amber-400/25 bg-amber-400/[.06] p-4 text-sm text-amber-100">نسخهٔ جدید {update.remote} آمادهٔ نصب است.</div> : update.status === 'current' ? <div className="rounded-xl border border-emerald-400/20 bg-emerald-400/[.06] p-4 text-sm text-emerald-200">پنل به‌روز است.</div> : <div className="rounded-xl border border-rose-400/20 bg-rose-400/[.06] p-4 text-sm text-rose-200">{update.error || 'بررسی نسخه انجام نشد. اتصال سرور و دسترسی helper را بررسی کن.'}</div>}
      <div className="flex flex-wrap justify-end gap-2"><Button variant="outline" disabled={checking || updating} onClick={onCheck}><RefreshCw size={15} className={checking ? 'animate-spin' : ''}/>بررسی دوباره</Button>{canInstall && update.status === 'available' && <Button disabled={checking || updating} onClick={onInstall}>نصب نسخهٔ جدید</Button>}</div>
    </div>}
  </DialogContent></Dialog>
}

function UpdateOverlay({ active, stage, startedAt }: { active: boolean; stage: 1 | 2 | 3; startedAt: number | null }) {
  const [elapsed, setElapsed] = useState(0)
  useEffect(() => {
    if (!active || !startedAt) { setElapsed(0); return }
    const tick = () => setElapsed(Math.floor((Date.now() - startedAt) / 1000))
    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [active, startedAt])
  const elapsedLabel = `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`
  if (!active) return null
  return <div className="fixed inset-0 z-[100] grid place-items-center bg-[#080d12]/95 p-5 backdrop-blur-sm" role="alertdialog" aria-modal="true" aria-labelledby="update-overlay-title" aria-describedby="update-overlay-description" onKeyDown={(event) => event.preventDefault()}>
    <div className="w-full max-w-md rounded-2xl border border-border bg-card p-7 text-center shadow-2xl"><div className="mx-auto grid h-14 w-14 place-items-center rounded-2xl border border-primary/25 bg-primary/10 text-primary"><RefreshCw size={25} className="animate-spin"/></div><h2 id="update-overlay-title" className="mt-5 text-xl font-bold text-foreground">در حال به‌روزرسانی ARG</h2><p id="update-overlay-description" className="mt-2 text-sm leading-7 text-muted-foreground">{stage === 1 ? 'در حال ثبت درخواست به‌روزرسانی…' : stage === 3 ? 'نسخهٔ جدید نصب شد؛ سرویس در حال راه‌اندازی و بررسی نهایی است.' : 'دریافت و ساخت نسخهٔ جدید در حال انجام است. این مرحله ممکن است چند دقیقه طول بکشد.'}</p><div className="mt-6 h-2 overflow-hidden rounded-full bg-secondary" role="progressbar" aria-label="پیشرفت به‌روزرسانی" aria-valuetext={stage === 1 ? 'ثبت درخواست' : stage === 3 ? 'راه‌اندازی مجدد سرویس' : 'دریافت و ساخت نسخه'}><div className="arg-update-progress h-full rounded-full"/></div><div className="mt-4 grid grid-cols-3 gap-2 text-[10px]"><span className={stage >= 1 ? 'text-primary' : 'text-muted-foreground'}>ثبت درخواست</span><span className={stage >= 2 ? 'text-primary' : 'text-muted-foreground'}>دریافت و ساخت</span><span className={stage >= 3 ? 'text-primary' : 'text-muted-foreground'}>راه‌اندازی</span></div><div className="mt-2 flex justify-end text-[11px] text-muted-foreground"><span>زمان سپری‌شده</span><span dir="ltr" className="mr-2 font-mono tabular-nums">{elapsedLabel}</span></div><p className="mt-3 text-xs text-muted-foreground" role="status" aria-live="polite">لطفاً تا پایان فرایند این صفحه را نبند.</p></div>
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

export default function App() {
  const [boot, setBoot] = useState(true)
  const [installed, setInstalled] = useState(false)
  const [user, setUser] = useState<User | null>(null)
  const [page, setPage] = useState<PageId>(() => pageFromHash(window.location.hash))
  const [mobileOpen, setMobileOpen] = useState(false)
  const mobileMenuButton = useRef<HTMLButtonElement>(null)
  const mobileCloseButton = useRef<HTMLButtonElement>(null)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    try { return window.localStorage.getItem('arg.arg-sidebar-collapsed') === 'true' } catch { return false }
  })
  const [loadError, setLoadError] = useState('')
  const [update, setUpdate] = useState<{ status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; error?: string; instance?: string }>({ status: 'unavailable' })
  const [updating, setUpdating] = useState(false)
  const [updateStage, setUpdateStage] = useState<1 | 2 | 3>(1)
  const [updateStartedAt, setUpdateStartedAt] = useState<number | null>(null)
  const updateStartInstance = useRef<string | null>(null)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [updateDialogOpen, setUpdateDialogOpen] = useState(false)
  const [updateFinished, setUpdateFinished] = useState(() => {
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
    const onHash = () => {
      setPage(pageFromHash(window.location.hash))
      setMobileOpen(false)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
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
      if (announce) notifyUser(result.status === 'available' ? `نسخهٔ جدید پنل ARG (${result.remote}) آماده است.` : result.status === 'current' ? 'پنل ARG به‌روز است.' : result.error || 'بررسی نسخه انجام نشد.')
    } catch (error) {
      setUpdate({ status: 'unavailable' })
      if (announce) notifyUser(error instanceof Error ? error.message : 'بررسی آپدیت ناموفق بود')
    }
  }, [installed, user])
  useEffect(() => {
    void checkUpdate()
    const timer = window.setInterval(() => void checkUpdate(), 300000)
    return () => window.clearInterval(timer)
  }, [checkUpdate])
  useEffect(() => {
    if (update.status === 'available' && !updating) notifyUser(`نسخهٔ جدید پنل ARG (${update.remote}) آماده است.`)
  }, [update.status, update.remote, updating])
  useEffect(() => { if (updateFinished) { setUpdateDialogOpen(true); notifyUser('به‌روزرسانی کامل شد.') } }, [updateFinished])
  async function openUpdateDialog() {
    setUpdateDialogOpen(true)
    setCheckingUpdate(true)
    await checkUpdate()
    setCheckingUpdate(false)
  }
  async function runUpdate() {
    if (user?.role !== 'admin' || updating) return
    let baselineInstance = update.instance || null
    try {
      const fresh = await api<typeof update>('update')
      setUpdate(fresh)
      baselineInstance = fresh.instance || baselineInstance
    } catch { /* The update endpoint may still schedule the helper if status checking is temporarily unavailable. */ }
    updateStartInstance.current = baselineInstance
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
      notifyUser(e instanceof Error ? e.message : 'شروع آپدیت ناموفق بود')
    }
  }
  useEffect(() => {
    if (!updating) return
    let cancelled = false
    let attempts = 0
    let timer = 0
    let serviceRestartSeen = false
    let currentSince: number | null = null
    const poll = async () => {
      if (cancelled) return
      attempts += 1
      try {
        const result = await api<typeof update>('update')
        if (cancelled) return
        setUpdate(result)
        if (updateStartInstance.current && result.instance && result.instance !== updateStartInstance.current) {
          serviceRestartSeen = true
          setUpdateStage(3)
        }
        if (result.status === 'current' && serviceRestartSeen) {
          currentSince ??= Date.now()
          if (Date.now() - currentSince >= 5000) {
            try { window.sessionStorage.setItem('arg-update-finished', 'true') } catch { /* Session storage may be disabled. */ }
            setUpdating(false)
            setUpdateStartedAt(null)
            void forceFreshPanelReload()
            return
          }
        } else currentSince = null
      } catch { /* Wait for the restarted panel API; an unavailable status is not proof the update completed. */ }
      if (attempts >= 400) {
        setUpdating(false)
        setUpdateStartedAt(null)
        notifyUser('وضعیت آپدیت پس از 10 دقیقه مشخص نشد. صفحه را تازه کن و گزارش سرویس را بررسی کن.')
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
  function selectPage(next: PageId) {
    if (!allowedPages.some((item) => item.id === next)) return
    setPage(next)
    window.location.hash = `/${next}`
    setMobileOpen(false)
  }
  return (
      <div className="app-shell min-h-screen lg:grid" inert={updating} aria-hidden={updating} style={{ gridTemplateColumns: `${sidebarCollapsed ? 56 : 224}px minmax(0, 1fr)` }}>
        <aside id="arg-sidebar" role={mobileOpen ? 'dialog' : undefined} aria-modal={mobileOpen ? true : undefined} aria-label="ناوبری اصلی" tabIndex={-1} className={cn('arg-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(80vw,260px)] max-w-[100vw] flex-col border-l px-3 py-4 transition-transform lg:sticky lg:top-0 lg:h-screen lg:translate-x-0', sidebarCollapsed ? 'arg-sidebar-collapsed lg:w-[56px]' : 'lg:w-[224px]', mobileOpen ? 'translate-x-0' : 'translate-x-full')}>
          <div className="arg-sidebar-pattern" aria-hidden="true" />
          <div className="arg-sidebar-corner" aria-hidden="true" />
          <div className="arg-brand-row relative z-10 flex min-h-[78px] items-center justify-between gap-2 px-2">
            <div className="arg-brand-lockup flex min-w-0 items-center gap-2" aria-label="ARG ارگ VPS Control Center">
              <img src="/arg/brand/arg-symbol.svg" alt="" className="arg-brand-mark h-10 w-10 shrink-0" />
              <div className="arg-brand-copy min-w-0"><div className="arg-wordmark" dir="ltr">ARG</div><div className="arg-persian-name">ارگ</div><div className="arg-product-label" dir="ltr">VPS CONTROL CENTER</div></div>
            </div>
            <Button variant="ghost" size="icon" className="arg-collapse hidden shrink-0 lg:inline-flex" onClick={() => setSidebarCollapsed((value) => !value)} aria-label={sidebarCollapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'} aria-expanded={!sidebarCollapsed} title={sidebarCollapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'}>{sidebarCollapsed ? <ChevronLeft size={18} /> : <ChevronRight size={18} />}</Button>
            <Button ref={mobileCloseButton} variant="ghost" size="icon" className="arg-mobile-close lg:hidden" onClick={() => setMobileOpen(false)} aria-label="بستن منو"><X size={18}/></Button>
          </div>
          <div className="arg-divider relative z-10 mx-2 my-3" aria-hidden="true" />
          <div className="arg-navigation relative z-10 flex-1 space-y-5 overflow-y-auto px-1 pb-4">
            {[
              { title: 'نمای کلی', pages: allowedPages.filter((item) => item.id === 'dashboard') },
              { title: 'زیرساخت', pages: allowedPages.filter((item) => ['system', 'network', 'ports', 'firewall', 'nginx', 'terminal'].includes(item.id)) },
              { title: 'مدیریت', pages: allowedPages.filter((item) => ['users', 'audit', 'settings'].includes(item.id)) },
            ].map((section) => <nav key={section.title} aria-label={section.title}><div className="arg-section-label mb-1.5 px-3">{section.title}</div><div className="space-y-1">{section.pages.map((item) => <button key={item.id} onClick={() => selectPage(item.id)} aria-current={currentPage === item.id ? 'page' : undefined} title={sidebarCollapsed ? item.label : undefined} className={cn('arg-nav-item group flex min-h-[46px] w-full items-center gap-3 rounded-[11px] px-3 text-right text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]', currentPage === item.id ? 'is-active' : '')}><item.icon className="arg-nav-icon shrink-0" size={21} strokeWidth={1.8}/><span className="arg-nav-label flex-1">{item.label}</span></button>)}</div></nav>)}
          </div>
          <div className="arg-sidebar-footer relative z-10 mt-auto pt-2">
            <div className="arg-divider mx-2 mb-3" aria-hidden="true" />
            <div className={cn('arg-service-status mb-2 flex min-h-8 items-center gap-2 px-3 text-[11px]', sidebarCollapsed && 'justify-center px-0')} title={sidebarCollapsed ? 'سرویس متصل' : undefined}><span className="h-2 w-2 shrink-0 rounded-full bg-emerald-400"/><span className="arg-nav-label">سرویس متصل</span></div>
            <DropdownMenu><DropdownMenuTrigger asChild><button className="arg-account flex min-h-[54px] w-full items-center gap-2.5 rounded-[11px] border px-2.5 text-right transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]" title={sidebarCollapsed ? user.username : undefined}><Avatar className="h-9 w-9 shrink-0"><AvatarFallback className="arg-avatar-fallback">{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="arg-account-copy min-w-0 flex-1"><span className="block truncate text-xs font-semibold">{user.username}</span><span className="mt-0.5 block text-[10px] text-muted-foreground">{user.role === 'admin' ? 'مدیر سیستم' : user.role === 'operator' ? 'اپراتور' : 'ناظر'}</span></span><ChevronDown className="arg-account-chevron" size={15}/></button></DropdownMenuTrigger><DropdownMenuContent align="end" side="top" className="w-52"><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات حساب</DropdownMenuItem><DropdownMenuItem onSelect={logout} className="text-rose-300"><LogOut size={15}/>خروج از حساب</DropdownMenuItem></DropdownMenuContent></DropdownMenu>
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
        <div className="min-w-0">
          <header className="utility-bar sticky top-0 z-20 flex h-[62px] items-center justify-between gap-3 border-b border-white/10 px-5 md:px-8">
            <div className="flex items-center gap-2"><Button ref={mobileMenuButton} variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(true)} aria-label="باز کردن منو" aria-expanded={mobileOpen} aria-controls="arg-sidebar"><Menu size={19}/></Button><span className="hidden text-xs font-bold text-primary sm:inline" dir="ltr">ARG <span className="text-muted-foreground">/</span> {navigation.find((item) => item.id === currentPage)?.label}</span><span className="hidden h-4 w-px bg-white/10 sm:inline-block"/><Badge variant="success" className="hidden sm:inline-flex"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400"/>LIVE</Badge></div>
            <div className="flex items-center gap-1.5"><Button variant="ghost" size="icon" onClick={() => void openUpdateDialog()} disabled={checkingUpdate || updating} aria-busy={checkingUpdate || updating} aria-label="بررسی و به‌روزرسانی پنل" title={checkingUpdate ? 'در حال بررسی نسخه‌ها…' : 'بررسی و به‌روزرسانی پنل'} className="relative text-muted-foreground hover:text-primary"><RefreshCw size={17} className={checkingUpdate || updating ? 'animate-spin' : ''}/>{update.status === 'available' && <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-amber-400"/>}</Button><a href="https://github.com/themrcortex/arg" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="GitHub پروژه"><Github size={18}/></a><a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="راهنما و پشتیبانی"><LifeBuoy size={18}/></a><span className="mx-1 h-5 w-px bg-white/10"/><Badge variant="outline" className="hidden sm:inline-flex">FA <span className="mr-1 text-muted-foreground">فارسی</span></Badge><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" className="h-10 gap-2 rounded-lg px-2"><Avatar className="h-8 w-8"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="hidden text-xs sm:inline">{user.username}</span><ChevronDown size={14} className="text-muted-foreground"/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات</DropdownMenuItem><DropdownMenuItem onSelect={logout} className="text-rose-300"><LogOut size={15}/>خروج</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>
          </header>
          <main className="mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            <div key={currentPage} className="arg-page-enter">
            {currentPage === 'dashboard' && <DashboardPage user={user} Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'system' && <SystemPage user={user} Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'ports' && <PortsPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={notifyUser} user={user} />}
            {currentPage === 'nginx' && user.role === 'admin' && <NginxPage Heading={PageHeading} notify={notifyUser} />}
            {currentPage === 'terminal' && user.role === 'admin' && <Suspense fallback={<div className="py-20 text-center text-sm text-muted-foreground">در حال آماده‌سازی ترمینال…</div>}><TerminalPage Heading={PageHeading} /></Suspense>}
            {currentPage === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={notifyUser} />
            )}
            {currentPage === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={notifyUser} />
            )}
            {currentPage === 'settings' && <SettingsPage Heading={PageHeading} logout={logout} notify={notifyUser} user={user} update={update} openUpdateDialog={() => void openUpdateDialog()} updating={updating} checkingUpdate={checkingUpdate} />}
            <footer className="mt-12 flex flex-wrap items-center justify-between gap-3 border-t border-border/70 pt-6 text-xs text-muted-foreground">
              <span>© {new Date().getFullYear()} ARG · Rust + React</span>
              <span>طراحی و توسعه: {developer}</span>
            </footer>
            </div>
          </main>
        </div>
        <Toaster />
        <UpdateOverlay active={updating} stage={updateStage} startedAt={updateStartedAt}/>
        <UpdateDialog open={updateDialogOpen} onOpenChange={(open) => { if (!updating) setUpdateDialogOpen(open); if (!open) setUpdateFinished(false) }} update={update} checking={checkingUpdate} updating={updating} finished={updateFinished} canInstall={user.role === 'admin'} changelog={changelog} onCheck={() => void openUpdateDialog()} onInstall={() => void runUpdate()} onForceRefresh={() => void forceFreshPanelReload()}/>
      </div>
  )
}

export { PageHeading }
