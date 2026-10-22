import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  Activity,
  ArrowLeft,
  ChevronDown,
  Eye,
  EyeOff,
  Headphones,
  LifeBuoy,
  LogOut,
  Menu,
  Command,
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
import { AuditPage, DashboardPage, FirewallPage, NetworkPage, SettingsPage, UsersPage, SystemControls, PortProbeControls } from './pages'
import { ExplorerPage } from './explorer/ExplorerPage'
import { entryById, explorerGroups, type ExplorerGroup } from './explorer/catalog'
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

function PageOptions({ group, currentPage, onSelect }: { group: ExplorerGroup; currentPage: string; onSelect: (id: string) => void }) {
  const [query, setQuery] = useState('')
  const [expanded, setExpanded] = useState<string | null>(null)
  useEffect(() => {
    setQuery('')
    setExpanded(group.categories.find((section) => section.items.some((item) => item.id === currentPage))?.id || null)
  }, [group.id, currentPage])
  const sections = group.categories.map((section) => ({ ...section, items: section.items.filter((item) => item.kind !== 'page' && `${item.label} ${item.description}`.toLocaleLowerCase('fa-IR').includes(query.toLocaleLowerCase('fa-IR'))) })).filter((section) => section.items.length)
  return <section id="page-options" className="mt-8 rounded-2xl border border-white/10 bg-[#15151f] p-5 md:p-7">
    <div className="flex flex-wrap items-center justify-between gap-4"><div><h2 className="text-lg font-bold text-white">ابزارها و نماهای {group.label}</h2><p className="mt-1 text-xs text-muted-foreground">زیرشاخهٔ موردنظر را اینجا باز کن.</p></div><label className="relative block"><Search size={15} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجو در این صفحه" className="w-[220px] pr-9"/></label></div>
    <div className="mt-6 grid gap-2 md:grid-cols-2">{sections.map((section) => { const open = Boolean(query) || expanded === section.id || (!expanded && section.items.some((item) => item.id === currentPage)); return <div key={section.id} className="self-start rounded-xl border border-white/[.08] bg-white/[.025]"><button onClick={() => setExpanded(open ? 'closed' : section.id)} aria-expanded={open} className="flex w-full items-center justify-between p-4 text-right text-sm font-semibold text-white"><span>{section.label} <span className="mr-1 text-[10px] text-muted-foreground">{formatNumber(section.items.length)}</span></span><ChevronDown size={15} className={cn('transition', !open && 'rotate-90')}/></button>{open && <div className="flex flex-wrap gap-2 border-t border-white/[.08] p-3">{section.items.map((item) => <button key={item.id} onClick={() => onSelect(item.id)} aria-current={currentPage === item.id ? 'page' : undefined} className={cn('rounded-lg border px-3 py-2 text-xs transition', currentPage === item.id ? 'border-primary/50 bg-primary/20 text-white' : 'border-white/10 bg-white/[.025] text-muted-foreground hover:border-primary/40 hover:text-white')}>{item.label}</button>)}</div>}</div> })}</div>
  </section>
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
  const groupFor = (id: string) => allowedGroups.find((group) => group.categories.some((section) => section.items.some((entry) => entry.id === id)))
  const activeGroup = groupFor(currentPage) || allowedGroups[0]
  const rootPage = (id: string) => ({ overview: 'dashboard', system: 'system-cpu-use', network: 'network', ports: 'ports-all', firewall: 'firewall', users: 'users', audit: 'audit', settings: 'settings' } as Record<string, string>)[id]
  function selectPage(next: string) {
    if (!allowedIds.has(next)) return
    setPage(next)
    window.location.hash = `/${next}`
    setMobileOpen(false)
    window.scrollTo({ top: 0, behavior: 'smooth' })
  }
  return (
    <TooltipProvider delayDuration={150}>
      <div className="app-shell min-h-screen lg:grid lg:grid-cols-[64px_220px_minmax(0,1fr)]">
        <aside className="app-rail sticky top-0 z-30 hidden h-screen flex-col items-center border-l border-white/10 py-4 lg:flex" aria-label="دسته‌بندی‌های اصلی">
          <button onClick={() => selectPage('dashboard')} className="mb-6 grid h-10 w-10 place-items-center rounded-xl border border-white/10 bg-white/[.035] transition hover:border-primary/40" aria-label="داشبورد">
            <span className="grid h-7 w-7 place-items-center rounded-lg bg-gradient-to-br from-[#dec3ff] to-[#7650cb] text-sm font-black text-[#211135] shadow-glow">D</span>
          </button>
          <div className="mb-3 h-px w-8 bg-white/10" />
          <nav className="flex flex-1 flex-col items-center gap-2" aria-label="انتخاب بخش">
            {allowedGroups.map((group) => {
              const selected = activeGroup?.id === group.id
              return <Tooltip key={group.id}>
                <TooltipTrigger asChild><button onClick={() => selectPage(rootPage(group.id))} aria-label={group.label} aria-current={selected ? 'page' : undefined} className={cn('rail-button relative grid h-10 w-10 place-items-center rounded-xl transition', selected ? 'bg-[#312547] text-[#c2a1ff]' : 'text-[#8d91a7] hover:bg-white/[.06] hover:text-white')}><group.icon size={19} strokeWidth={1.8}/>{selected && <span className="absolute -right-[13px] h-5 w-[3px] rounded-full bg-primary shadow-glow"/>}</button></TooltipTrigger>
                <TooltipContent side="left">{group.label}</TooltipContent>
              </Tooltip>
            })}
          </nav>
          <div className="mt-auto flex flex-col items-center gap-2 border-t border-white/10 pt-3">
            <Tooltip><TooltipTrigger asChild><button onClick={() => selectPage('settings')} className="rail-button grid h-10 w-10 place-items-center rounded-xl text-[#8d91a7] transition hover:bg-white/[.06] hover:text-white" aria-label="تنظیمات"><Settings size={18}/></button></TooltipTrigger><TooltipContent side="left">تنظیمات</TooltipContent></Tooltip>
            <a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="rail-button grid h-10 w-10 place-items-center rounded-xl text-[#8d91a7] transition hover:bg-white/[.06] hover:text-white" aria-label="پشتیبانی"><Headphones size={18}/></a>
          </div>
        </aside>
        <aside className={cn('app-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(82vw,270px)] flex-col border-l border-white/10 px-4 py-6 transition-transform lg:sticky lg:top-0 lg:h-screen lg:w-[220px] lg:translate-x-0', mobileOpen ? 'translate-x-0' : 'translate-x-full')}>
          <div className="flex items-center justify-between px-2"><Brand /><Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(false)} aria-label="بستن منو"><X size={18}/></Button></div>
          <div className="mt-9 px-3 text-[10px] font-bold tracking-[.1em] text-muted-foreground">فضای کار</div>
          <nav className="mt-3 flex-1 space-y-1" aria-label="صفحات اصلی">
            {allowedGroups.map((group) => <button key={group.id} onClick={() => selectPage(rootPage(group.id))} aria-current={activeGroup.id === group.id ? 'page' : undefined} className={cn('flex w-full items-center gap-3 rounded-xl px-3 py-3 text-right text-sm transition', activeGroup.id === group.id ? 'bg-primary/15 font-semibold text-white shadow-[inset_-3px_0_#a987ff]' : 'text-[#a6a6bb] hover:bg-white/[.05] hover:text-white')}><group.icon size={18}/><span>{group.label}</span></button>)}
          </nav>
          <div className="rounded-xl border border-white/10 bg-white/[.03] p-3 text-xs text-muted-foreground"><span className="mb-2 block h-2 w-2 rounded-full bg-emerald-400"/>سرور متصل است<br/><span className="mt-1 block text-[10px]">جزئیات هر بخش داخل صفحهٔ آن قرار دارد.</span></div>
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
            <div className="flex items-center gap-2"><Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setMobileOpen(true)} aria-label="باز کردن منو"><Menu size={19}/></Button><span className="hidden text-[10px] font-bold tracking-[.16em] text-[#c0a2f7] sm:inline" dir="ltr">DONATELLO CONTROL PLANE</span><span className="hidden h-4 w-px bg-white/10 sm:inline-block"/><Badge variant="success" className="hidden sm:inline-flex"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400"/>LIVE</Badge></div>
            <div className="flex items-center gap-1.5"><Button variant="ghost" size="icon" className="h-9 w-9 rounded-lg" onClick={() => document.getElementById('page-options')?.scrollIntoView({ behavior: 'smooth' })} aria-label="جست‌وجو"><Search size={18}/></Button><span className="hidden rounded-lg border border-white/10 px-2 py-1 text-[10px] text-muted-foreground md:inline-flex" dir="ltr"><Command size={12} className="ml-1"/>K</span><a href="https://t.me/mrc0rtex" target="_blank" rel="noopener noreferrer" className="grid h-9 w-9 place-items-center rounded-lg text-muted-foreground transition hover:bg-white/[.06] hover:text-white" aria-label="راهنما و پشتیبانی"><LifeBuoy size={18}/></a><span className="mx-1 h-5 w-px bg-white/10"/><Badge variant="outline" className="hidden sm:inline-flex">FA <span className="mr-1 text-muted-foreground">فارسی</span></Badge><DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" className="h-10 gap-2 rounded-lg px-2"><Avatar className="h-8 w-8"><AvatarFallback>{user.username.slice(0, 1).toUpperCase()}</AvatarFallback></Avatar><span className="hidden text-xs sm:inline">{user.username}</span><ChevronDown size={14} className="text-muted-foreground"/></Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuItem onSelect={() => selectPage('settings')}><Settings size={15}/>تنظیمات</DropdownMenuItem><DropdownMenuItem onSelect={logout} className="text-rose-300"><LogOut size={15}/>خروج</DropdownMenuItem></DropdownMenuContent></DropdownMenu></div>
          </header>
          <main className="mx-auto max-w-[1500px] px-5 pb-16 pt-8 md:px-9 md:pt-10">
            {currentPage === 'dashboard' && <DashboardPage user={user} Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'network' && <NetworkPage Heading={PageHeading} notify={setNotice} />}
            {currentPage === 'firewall' && <FirewallPage Heading={PageHeading} notify={setNotice} user={user} />}
            {currentPage === 'users' && user.role === 'admin' && (
              <UsersPage current={user} Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'audit' && user.role === 'admin' && (
              <AuditPage Heading={PageHeading} notify={setNotice} />
            )}
            {currentPage === 'settings' && <SettingsPage Heading={PageHeading} logout={logout} notify={setNotice} />}
            {active.kind !== 'page' && <ExplorerPage key={currentPage} item={active} user={user} onNavigate={selectPage} notify={setNotice} />}
            {active.kind === 'system' && <SystemControls user={user} notify={setNotice} />}
            {active.kind === 'ports' && <div className="mt-5"><PortProbeControls notify={setNotice}/></div>}
            {activeGroup.id !== 'overview' && <PageOptions group={activeGroup} currentPage={currentPage} onSelect={selectPage}/>}
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
