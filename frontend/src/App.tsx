import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowLeft,
  ChevronDown,
  LayoutDashboard,
  LogOut,
  Menu,
  Network,
  ScrollText,
  Settings,
  Shield,
  Users,
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
import { Avatar, AvatarFallback } from './components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './components/ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './components/ui/tooltip'
import { AuditPage, DashboardPage, FirewallPage, NetworkPage, SettingsPage, UsersPage } from './pages'

type Page = 'dashboard' | 'network' | 'firewall' | 'users' | 'audit' | 'settings'
const pages = [
  { key: 'dashboard' as Page, name: 'نمای کلی', icon: LayoutDashboard },
  { key: 'network' as Page, name: 'شبکه و پورت‌ها', icon: Network },
  { key: 'firewall' as Page, name: 'فایروال', icon: Shield },
  { key: 'users' as Page, name: 'کاربران', icon: Users, admin: true },
  { key: 'audit' as Page, name: 'گزارش عملیات', icon: ScrollText, admin: true },
  { key: 'settings' as Page, name: 'تنظیمات', icon: Settings },
]
const developer = (
  <a
    className="font-semibold text-[#c4a1ff] transition hover:text-white hover:underline"
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
      <span
        className={cn(
          'grid shrink-0 place-items-center rounded-2xl bg-gradient-to-br from-[#cb9fff] via-[#9c6df0] to-[#633cc0] font-black text-white shadow-glow',
          large ? 'h-14 w-14 text-3xl' : 'h-11 w-11 text-2xl',
        )}
      >
        D
      </span>
      <div>
        <div className={cn('brand-text font-bold leading-none text-white', large ? 'text-3xl' : 'text-2xl')}>
          Donatello
        </div>
        <div className="mt-1 text-[10px] font-semibold uppercase tracking-[.19em] text-muted-foreground">
          Server manager
        </div>
      </div>
    </div>
  )
}

function Auth({ installer, onSuccess }: { installer: boolean; onSuccess: () => void }) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [setupToken, setSetupToken] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [created, setCreated] = useState(false)
  async function submit(event: FormEvent) {
    event.preventDefault()
    setError('')
    if (installer && password !== confirm) {
      setError('تکرار گذرواژه با گذرواژه برابر نیست.')
      return
    }
    setBusy(true)
    try {
      await api(installer ? 'setup' : 'login', {
        method: 'POST',
        body: JSON.stringify({ username, password, ...(installer ? { setup_token: setupToken } : {}) }),
      })
      if (installer) {
        setCreated(true)
        setPassword('')
        setConfirm('')
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
      <div className="pointer-events-none absolute -top-32 right-[18%] h-[520px] w-[520px] rounded-full bg-primary/10 blur-[100px]" />
      <div className="relative w-full max-w-[440px]">
        <div className="mb-8 flex justify-center">
          <Brand large />
        </div>
        <Card className="soft-panel border-[#55416e] shadow-[0_30px_90px_-32px_#000]">
          <CardHeader className="pb-6">
            <Badge className="mb-4 w-fit">{installer ? 'راه‌اندازی اولیه' : 'ورود امن'}</Badge>
            <CardTitle className="text-2xl">{installer ? 'به دوناتلو خوش آمدید' : 'خوش آمدید'}</CardTitle>
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
                <Input
                  id="password"
                  type="password"
                  autoComplete={installer ? 'new-password' : 'current-password'}
                  required
                  minLength={installer ? 12 : 1}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="حداقل ۱۲ نویسه"
                />
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
              {installer && (
                <div className="space-y-2">
                  <Label htmlFor="confirm">تکرار گذرواژه</Label>
                  <Input
                    id="confirm"
                    type="password"
                    autoComplete="new-password"
                    required
                    value={confirm}
                    onChange={(e) => setConfirm(e.target.value)}
                    placeholder="گذرواژه را دوباره وارد کنید"
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
    <div className="mb-7 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-extrabold tracking-tight text-white md:text-[29px]">{title}</h1>
        <p className="mt-2 text-sm text-muted-foreground">{description}</p>
      </div>
      {action}
    </div>
  )
}

export default function App() {
  const [boot, setBoot] = useState(true)
  const [installed, setInstalled] = useState(false)
  const [user, setUser] = useState<User | null>(null)
  const [page, setPage] = useState<Page>('dashboard')
  const [mobileOpen, setMobileOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const [loadError, setLoadError] = useState('')
  const refresh = useCallback(async () => {
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
    if (notice) {
      const timer = window.setTimeout(() => setNotice(''), 4000)
      return () => window.clearTimeout(timer)
    }
  }, [notice])
  async function logout() {
    try {
      await api('logout', { method: 'POST' })
      setUser(null)
      setPage('dashboard')
    } catch (e) {
      setNotice(e instanceof Error ? e.message : 'خروج ناموفق بود')
    }
  }
  if (boot)
    return (
      <div className="flex min-h-screen items-center justify-center gap-3 text-muted-foreground">
        <Activity className="animate-pulse text-primary" /> در حال بارگذاری دوناتلو…
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
  const visiblePages = pages.filter((p) => !p.admin || user.role === 'admin')
  const active = pages.find((p) => p.key === page) || pages[0]
  function selectPage(next: Page) {
    setPage(next)
    setMobileOpen(false)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }
  return (
    <TooltipProvider delayDuration={150}>
      <div className="min-h-screen lg:grid lg:grid-cols-[260px_minmax(0,1fr)]">
        <aside
          className={cn(
            'fixed inset-y-0 right-0 z-40 flex w-[260px] flex-col border-l border-border bg-[#191323] px-4 py-7 transition-transform lg:sticky lg:top-0 lg:h-screen lg:translate-x-0',
            mobileOpen ? 'translate-x-0' : 'translate-x-full',
          )}
        >
          <div className="flex items-center justify-between px-2">
            <Brand />
            <Button
              variant="ghost"
              size="icon"
              className="lg:hidden"
              onClick={() => setMobileOpen(false)}
              aria-label="بستن منو"
            >
              <X size={18} />
            </Button>
          </div>
          <div className="mt-11 px-3 text-[11px] font-bold tracking-wide text-muted-foreground/70">
            فضای کاری
          </div>
          <nav className="mt-3 space-y-1" aria-label="ناوبری اصلی">
            {visiblePages.map((item) => (
              <button
                key={item.key}
                onClick={() => selectPage(item.key)}
                className={cn(
                  'flex w-full items-center gap-3 rounded-xl px-4 py-3 text-right text-sm font-medium transition-colors hover:bg-accent hover:text-white',
                  page === item.key
                    ? 'bg-[#44305f] text-white shadow-[inset_-3px_0_#ad81ff]'
                    : 'text-muted-foreground',
                )}
                aria-current={page === item.key ? 'page' : undefined}
              >
                <item.icon size={18} strokeWidth={1.9} />
                {item.name}
              </button>
            ))}
          </nav>
          <div className="mt-auto space-y-4">
            <div className="rounded-2xl border border-border bg-[#281e38] p-4">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <span className="h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_12px_#4ade80]" />
                پنل آنلاین است
              </div>
              <p className="mt-1.5 text-xs text-muted-foreground">Donatello v0.2.0</p>
            </div>
            <p className="px-2 text-center text-[11px] text-muted-foreground">ساخته‌شده توسط {developer}</p>
          </div>
        </aside>
        {mobileOpen && (
          <button
            aria-label="بستن منو"
            className="fixed inset-0 z-30 bg-black/70 lg:hidden"
            onClick={() => setMobileOpen(false)}
          />
        )}
        <div className="min-w-0">
          <header className="sticky top-0 z-20 flex h-[78px] items-center justify-between gap-3 border-b border-border/70 bg-background/80 px-5 backdrop-blur-xl md:px-9">
            <div className="flex items-center gap-3">
              <Button
                variant="ghost"
                size="icon"
                className="lg:hidden"
                onClick={() => setMobileOpen(true)}
                aria-label="باز کردن منو"
              >
                <Menu size={20} />
              </Button>
              <div className="text-xs text-muted-foreground">
                Donatello <span className="mx-2 text-border">/</span>{' '}
                <span className="font-semibold text-foreground">{active.name}</span>
              </div>
            </div>
            <div className="flex items-center gap-3">
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="hidden sm:inline-flex">
                    <Badge variant="success">
                      <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /> متصل به سرور
                    </Badge>
                  </span>
                </TooltipTrigger>
                <TooltipContent>داده‌ها از سرور فعلی خوانده می‌شوند</TooltipContent>
              </Tooltip>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" className="h-11 gap-2 rounded-xl px-1.5">
                    <Avatar className="h-9 w-9">
                      <AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback>
                    </Avatar>
                    <span className="hidden text-sm sm:inline">{user.username}</span>
                    <ChevronDown size={14} className="text-muted-foreground" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onSelect={() => selectPage('settings')}>
                    <Settings size={15} /> تنظیمات
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={logout} className="text-rose-300">
                    <LogOut size={15} /> خروج
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </header>
          <main className="mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            {page === 'dashboard' && <DashboardPage user={user} Heading={PageHeading} notify={setNotice} />}
            {page === 'network' && <NetworkPage Heading={PageHeading} notify={setNotice} />}
            {page === 'firewall' && <FirewallPage Heading={PageHeading} notify={setNotice} />}
            {page === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={setNotice} />
            )}
            {page === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={setNotice} />
            )}
            {page === 'settings' && <SettingsPage Heading={PageHeading} logout={logout} notify={setNotice} />}
            <footer className="mt-12 flex flex-wrap items-center justify-between gap-3 border-t border-border/70 pt-6 text-xs text-muted-foreground">
              <span>© {new Date().getFullYear()} Donatello · Rust + React</span>
              <span>طراحی و توسعه: {developer}</span>
            </footer>
          </main>
        </div>
        {notice && (
          <div
            role="status"
            className="fixed bottom-5 left-5 z-50 max-w-sm rounded-xl border border-primary/40 bg-popover px-5 py-3 text-sm text-white shadow-2xl"
          >
            {notice}
          </div>
        )}
      </div>
    </TooltipProvider>
  )
}

export { PageHeading }
