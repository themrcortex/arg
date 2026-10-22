import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowLeft,
  ChevronDown,
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
import { Avatar, AvatarFallback } from './components/ui/avatar'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './components/ui/dropdown-menu'
import { AuditPage, DashboardPage, FirewallPage, NetworkPage, PortsPage, SettingsPage, SystemPage, UsersPage } from './pages'
import { navigation, pageFromHash, type PageId } from './navigation'
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
        role="img"
        aria-label="لوگوی دوناتلو"
        className={cn(
          'grid shrink-0 place-items-center rounded-2xl bg-gradient-to-br from-[#cb9fff] via-[#9c6df0] to-[#633cc0] text-white shadow-glow',
          large ? 'h-14 w-14 text-3xl' : 'h-11 w-11 text-2xl',
        )}
      >
        <svg viewBox="0 0 64 64" className={large ? 'h-11 w-11' : 'h-9 w-9'} fill="none" aria-hidden="true">
          <path
            d="M9 23 3 17l3 13-3 10 8-4 5-5-5-7 6-3 9 2 6 4 6-4 9-2 6 3-5 7 5 5 8 4-3-10 3-13-6 6-8-4-10 1-5 4-5-4-10-1-8 4Z"
            fill="#54258C"
          />
          <path d="M12 25c5-4 11-5 17-2l2 4-3 5c-6 2-11 1-15-2l-1-5Zm40 0c-5-4-11-5-17-2l-2 4 3 5c6 2 11 1 15-2l1-5Z" fill="#F4EAFE" />
          <path d="M22 25.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Zm20 0a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5Z" fill="#32104F" />
          <path d="m31 29 1-2 1 2-1 2-1-2Z" fill="#CB9FFF" />
        </svg>
      </span>
      <div>
        <div className={cn('brand-text font-black leading-none text-white', large ? 'text-[31px]' : 'text-[21px]')} dir="ltr">
          DONATELLO
        </div>
        <div className="mt-1 text-[9px] font-semibold uppercase tracking-[.25em] text-[#a6eddf]" dir="ltr">
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
        <div className="mb-3 flex items-center gap-2 text-[10px] font-bold tracking-[.17em] text-[#a6eddf]" dir="ltr"><span className="h-1.5 w-1.5 rounded-full bg-[#71e4cf]"/> LIVE SERVER DATA</div>
        <h1 className="text-3xl font-extrabold tracking-tight text-white md:text-[34px]">{title}</h1>
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
  const [page, setPage] = useState<PageId>(() => pageFromHash(window.location.hash))
  const [mobileOpen, setMobileOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const [loadError, setLoadError] = useState('')
  const [update, setUpdate] = useState<{ status: 'available' | 'current' | 'unavailable'; local?: string; remote?: string; error?: string }>({ status: 'unavailable' })
  const [updating, setUpdating] = useState(false)
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
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
  useEffect(() => {
    if (notice) {
      const timer = window.setTimeout(() => setNotice(''), 4000)
      return () => window.clearTimeout(timer)
    }
  }, [notice])
  const checkUpdate = useCallback(async (announce = false) => {
    if (!installed || !user) return
    try {
      const result = await api<typeof update>('update')
      setUpdate(result)
      if (announce) setNotice(result.status === 'available' ? `نسخهٔ جدید Donatello (${result.remote}) آماده است.` : result.status === 'current' ? 'Donatello به‌روز است.' : result.error || 'بررسی نسخه انجام نشد.')
    } catch (error) {
      setUpdate({ status: 'unavailable' })
      if (announce) setNotice(error instanceof Error ? error.message : 'بررسی آپدیت ناموفق بود')
    }
  }, [installed, user])
  useEffect(() => {
    void checkUpdate()
    const timer = window.setInterval(() => void checkUpdate(), 300000)
    return () => window.clearInterval(timer)
  }, [checkUpdate])
  useEffect(() => {
    if (update.status === 'available') setNotice(`نسخهٔ جدید Donatello (${update.remote}) آماده است.`)
  }, [update.status, update.remote])
  async function runUpdate() {
    if (user?.role !== 'admin' || updating) return
    setUpdating(true)
    try {
      await api('update', { method: 'POST' })
      setNotice('آپدیت شروع شد. پنل چند لحظه دیگر دوباره در دسترس خواهد بود.')
      setUpdate((old) => ({ ...old, status: 'current' }))
    } catch (e) {
      setNotice(e instanceof Error ? e.message : 'شروع آپدیت ناموفق بود')
    } finally { setUpdating(false) }
  }
  async function logout() {
    try {
      await api('logout', { method: 'POST' })
      setUser(null)
      setPage('dashboard')
      window.location.hash = '/dashboard'
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
  const allowedPages = navigation.filter((item) => !item.admin || user.role === 'admin')
  const currentPage = allowedPages.some((item) => item.id === page) ? page : 'dashboard'
  function selectPage(next: PageId) {
    if (!allowedPages.some((item) => item.id === next)) return
    setPage(next)
    window.location.hash = `/${next}`
    setMobileOpen(false)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }
  return (
      <div className="app-shell min-h-screen lg:grid lg:grid-cols-[248px_minmax(0,1fr)]">
        <aside className={cn('app-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(82vw,270px)] flex-col border-l border-white/10 px-4 py-6 transition-transform lg:sticky lg:top-0 lg:h-screen lg:w-[248px] lg:translate-x-0', mobileOpen ? 'translate-x-0' : 'translate-x-full')}>
          <div className="flex items-center justify-between px-2"><Brand /><Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(false)} aria-label="بستن منو"><X size={18}/></Button></div>
          <div className="mt-9 flex-1 space-y-6 overflow-y-auto pb-5">
            {[
              { title: 'نمای کلی', pages: allowedPages.filter((item) => item.id === 'dashboard') },
              { title: 'زیرساخت', pages: allowedPages.filter((item) => ['system', 'network', 'ports', 'firewall'].includes(item.id)) },
              { title: 'مدیریت', pages: allowedPages.filter((item) => ['users', 'audit', 'settings'].includes(item.id)) },
            ].map((section) => <nav key={section.title} aria-label={section.title}><div className="mb-2 px-3 text-[10px] font-bold tracking-[.12em] text-[#76778d]">{section.title}</div><div className="space-y-1">{section.pages.map((item) => <button key={item.id} onClick={() => selectPage(item.id)} aria-current={currentPage === item.id ? 'page' : undefined} className={cn('flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-right text-[13px] transition', currentPage === item.id ? 'border border-primary/25 bg-primary/[.13] font-semibold text-white shadow-[inset_-3px_0_#a987ff]' : 'border border-transparent text-[#a6a6bb] hover:bg-white/[.055] hover:text-white')}><item.icon size={17} strokeWidth={1.8}/><span className="flex-1">{item.label}</span>{currentPage === item.id && <span className="h-1.5 w-1.5 rounded-full bg-[#bd9aff]"/>}</button>)}</div></nav>)}
          </div>
          <div className="space-y-3 border-t border-white/10 pt-4"><div className="flex items-center gap-2 px-2 text-[11px] text-[#a9aabc]"><span className="h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_9px_#4ade80]"/> سرویس متصل</div><button onClick={() => selectPage('settings')} className="flex w-full items-center gap-2 rounded-xl border border-white/10 bg-white/[.03] p-2.5 text-right transition hover:bg-white/[.07]"><Avatar className="h-9 w-9"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="min-w-0 flex-1"><span className="block truncate text-xs font-semibold text-white">{user.username}</span><span className="mt-0.5 block text-[10px] text-muted-foreground">{user.role === 'admin' ? 'مدیر' : user.role === 'operator' ? 'اپراتور' : 'ناظر'}</span></span><Settings size={14} className="text-muted-foreground"/></button></div>
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
            <div className="flex items-center gap-2"><Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(true)} aria-label="باز کردن منو"><Menu size={19}/></Button><span className="hidden text-[10px] font-bold tracking-[.16em] text-[#c0a2f7] sm:inline" dir="ltr">DONATELLO <span className="text-[#7f768f]">/</span> {navigation.find((item) => item.id === currentPage)?.label}</span><span className="hidden h-4 w-px bg-white/10 sm:inline-block"/><Badge variant="success" className="hidden sm:inline-flex"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400"/>LIVE</Badge></div>
            <div className="flex items-center gap-1.5"><Button variant="ghost" size="icon" className="h-9 w-9" onClick={() => void checkUpdate(true)} aria-label="بررسی آپدیت" title="بررسی آپدیت"><RefreshCw size={16}/></Button><Button size="sm" className="gap-2" disabled={updating || (update.status === 'available' && user.role !== 'admin')} onClick={() => void (update.status === 'available' ? runUpdate() : checkUpdate(true))} title={update.status === 'available' ? `نسخهٔ جدید ${update.remote} آماده است` : 'بررسی آپدیت'}><RefreshCw size={15} className={updating ? 'animate-spin' : ''}/>{updating ? 'در حال شروع…' : update.status === 'available' ? 'آپدیت جدید' : 'بررسی آپدیت'}</Button>{update.status === 'available' && <Button size="sm" className="gap-2" disabled={updating || user.role !== 'admin'} onClick={() => void runUpdate()} title={`نسخهٔ جدید ${update.remote} آماده است`}><RefreshCw size={15} className={updating ? 'animate-spin' : ''}/>{updating ? 'در حال شروع…' : 'آپدیت جدید'}</Button>}<a href="https://github.com/themrcortex/donatello" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="GitHub پروژه"><Github size={18}/></a><a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="راهنما و پشتیبانی"><LifeBuoy size={18}/></a><span className="mx-1 h-5 w-px bg-white/10"/><Badge variant="outline" className="hidden sm:inline-flex">FA <span className="mr-1 text-muted-foreground">فارسی</span></Badge><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" className="h-10 gap-2 rounded-lg px-2"><Avatar className="h-8 w-8"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="hidden text-xs sm:inline">{user.username}</span><ChevronDown size={14} className="text-muted-foreground"/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات</DropdownMenuItem><DropdownMenuItem onSelect={logout} className="text-rose-300"><LogOut size={15}/>خروج</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>
          </header>
          <main className="mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            {currentPage === 'dashboard' && <DashboardPage user={user} Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'system' && <SystemPage user={user} Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'ports' && <PortsPage Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={setNotice} user={user} />}
            {currentPage === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'settings' && <SettingsPage Heading={PageHeading} logout={logout} notify={setNotice} user={user} update={update} checkUpdate={checkUpdate} runUpdate={runUpdate} updating={updating} />}
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
  )
}

export { PageHeading }
