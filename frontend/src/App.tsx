import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowLeft,
  ChevronDown,
  ChevronLeft,
  Eye,
  EyeOff,
  LogOut,
  Menu,
  Search,
  Settings,
  X,
} from 'lucide-react'
import { api, formatNumber, type User } from './lib/api'
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
import { ExplorerPage } from './explorer/ExplorerPage'
import { entryById, explorerGroups } from './explorer/catalog'
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
                    placeholder={installer ? 'حداقل ۱۲ نویسه' : 'گذرواژهٔ حساب'}
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
  const [page, setPage] = useState(() => {
    const fromHash = window.location.hash.replace(/^#\/?/, '')
    return entryById.has(fromHash) ? fromHash : 'dashboard'
  })
  const [mobileOpen, setMobileOpen] = useState(false)
  const [navQuery, setNavQuery] = useState('')
  const [openGroups, setOpenGroups] = useState<string[]>(['overview'])
  const [openCategories, setOpenCategories] = useState<string[]>(['overview-main'])
  const [notice, setNotice] = useState('')
  const [loadError, setLoadError] = useState('')
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
      const id = window.location.hash.replace(/^#\/?/, '')
      if (entryById.has(id)) setPage(id)
    }
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
  useEffect(() => {
    for (const group of explorerGroups) {
      const category = group.categories.find((section) => section.items.some((entry) => entry.id === page))
      if (category) {
        setOpenGroups((items) => items.includes(group.id) ? items : [...items, group.id])
        setOpenCategories((items) => items.includes(category.id) ? items : [...items, category.id])
        break
      }
    }
  }, [page])
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
  const allowedGroups = explorerGroups.filter((group) => !group.admin || user.role === 'admin')
  const allowedIds = new Set(allowedGroups.flatMap((group) => group.categories.flatMap((category) => category.items.map((entry) => entry.id))))
  const currentPage = allowedIds.has(page) ? page : 'dashboard'
  const active = entryById.get(currentPage) || entryById.get('dashboard')!
  const activeGroup = allowedGroups.find((group) => group.categories.some((category) => category.items.some((entry) => entry.id === currentPage)))
  const filter = navQuery.trim().toLocaleLowerCase('fa-IR')
  function toggleGroup(id: string) {
    setOpenGroups((items) => items.includes(id) ? items.filter((item) => item !== id) : [...items, id])
  }
  function toggleCategory(id: string) {
    setOpenCategories((items) => items.includes(id) ? items.filter((item) => item !== id) : [...items, id])
  }
  function selectPage(next: string) {
    if (!allowedIds.has(next)) return
    setPage(next)
    window.location.hash = `/${next}`
    setMobileOpen(false)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }
  return (
    <TooltipProvider delayDuration={150}>
      <div className="app-shell min-h-screen lg:grid lg:grid-cols-[310px_minmax(0,1fr)]">
        <aside
          className={cn(
            'app-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(88vw,310px)] flex-col border-l border-white/10 px-4 py-6 transition-transform lg:sticky lg:top-0 lg:h-screen lg:w-[310px] lg:translate-x-0',
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
          <div className="mt-8 rounded-2xl border border-white/10 bg-white/[.035] p-3">
            <label className="relative block">
              <Search size={16} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
              <input value={navQuery} onChange={(event) => setNavQuery(event.target.value)} placeholder={`جست‌وجو در ${formatNumber(allowedIds.size)} نما…`} aria-label="جست‌وجو در گزینه‌ها" className="h-10 w-full rounded-xl border border-white/10 bg-[#11121c] pr-9 pl-3 text-xs text-foreground outline-none transition focus:border-primary/60" />
            </label>
            <div className="mt-2 flex items-center justify-between px-1 text-[10px] text-muted-foreground"><span>فضای کاری</span><span className="font-mono" dir="ltr">{allowedIds.size} VIEWS</span></div>
          </div>
          <nav className="sidebar-scroll mt-3 min-h-0 flex-1 space-y-1 overflow-y-auto pl-1" aria-label="ناوبری اصلی">
            {allowedGroups.map((group) => {
              const categories = group.categories.map((category) => ({ ...category, items: category.items.filter((item) => !filter || `${item.label} ${item.description} ${item.value}`.toLocaleLowerCase('fa-IR').includes(filter)) })).filter((category) => category.items.length)
              if (!categories.length) return null
              const expanded = Boolean(filter) || openGroups.includes(group.id)
              const count = group.categories.reduce((total, category) => total + category.items.length, 0)
              return <div key={group.id} className="rounded-xl">
                <button onClick={() => toggleGroup(group.id)} className={cn('nav-group flex w-full items-center gap-3 rounded-xl px-3 py-3 text-right text-sm font-semibold transition', activeGroup?.id === group.id ? 'text-white' : 'text-[#a8a9be] hover:text-white')} aria-expanded={expanded}>
                  <span className={cn('grid h-8 w-8 shrink-0 place-items-center rounded-lg', activeGroup?.id === group.id ? 'bg-primary/15 text-primary' : 'bg-white/[.035] text-[#8c91a8]')}><group.icon size={17}/></span>
                  <span className="flex-1">{group.label}</span><span className="font-mono text-[10px] text-muted-foreground">{formatNumber(count)}</span><ChevronLeft size={15} className={cn('text-muted-foreground transition-transform', expanded && '-rotate-90')}/>
                </button>
                {expanded && <div className="mr-7 border-r border-white/10 pb-2 pr-3">
                  {categories.map((category) => {
                    const catOpen = Boolean(filter) || openCategories.includes(category.id)
                    return <div key={category.id} className="mt-1">
                      <button onClick={() => toggleCategory(category.id)} className="flex w-full items-center justify-between gap-2 rounded-lg px-2 py-2 text-right text-[11px] font-semibold text-[#777d96] transition hover:text-white" aria-expanded={catOpen}><span>{category.label}</span><ChevronDown size={13} className={cn('transition-transform', !catOpen && 'rotate-90')}/></button>
                      {catOpen && <div className="space-y-0.5">{category.items.map((item) => <button key={item.id} onClick={() => selectPage(item.id)} title={item.description || item.label} aria-current={currentPage === item.id ? 'page' : undefined} className={cn('nav-leaf flex w-full items-center gap-2 rounded-lg px-3 py-2 text-right text-xs transition', currentPage === item.id ? 'bg-[#302446] font-semibold text-white' : 'text-[#a7a8ba] hover:bg-white/[.055] hover:text-white')}><span className={cn('h-1.5 w-1.5 shrink-0 rounded-full', currentPage === item.id ? 'bg-[#a6eddf] shadow-[0_0_10px_#a6eddf]' : 'bg-[#55546e]')}/><span className="min-w-0 flex-1 truncate">{item.label}</span>{currentPage === item.id && <ArrowLeft size={13} className="text-primary"/>}</button>)}</div>}
                    </div>
                  })}
                </div>}
              </div>
            })}
          </nav>
          <div className="mt-3 space-y-3 border-t border-white/10 pt-3">
            <div className="rounded-2xl border border-white/10 bg-white/[.035] p-3">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <span className="h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_12px_#4ade80]" />
                پنل آنلاین است
              </div>
              <p className="mt-1.5 font-mono text-[10px] text-muted-foreground" dir="ltr">DONATELLO / LIVE SERVER</p>
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
          <header className="sticky top-0 z-20 flex h-[78px] items-center justify-between gap-3 border-b border-white/10 bg-background/85 px-5 backdrop-blur-xl md:px-9">
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
                <span className="font-semibold text-foreground">{active.label}</span>
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
            {currentPage === 'dashboard' && <DashboardPage user={user} Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'settings' && <SettingsPage Heading={PageHeading} logout={logout} notify={setNotice} />}
            {active.kind !== 'page' && <ExplorerPage key={currentPage} item={active} user={user} onNavigate={selectPage} notify={setNotice} />}
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
