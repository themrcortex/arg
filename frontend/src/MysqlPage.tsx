import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Activity, AlertTriangle, Database, Eye, EyeOff, KeyRound, LoaderCircle, Play, RefreshCw, ShieldCheck, UserRound, Wrench } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'
import { Alert, AlertDescription } from './components/ui/alert'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type MysqlStatus = { installed: boolean; active: boolean; provider: 'mysql' | 'mariadb'; version?: string | null }
type CatalogKind = 'databases' | 'users'

export function MysqlPage({ Heading, notify }: Common) {
  const [status, setStatus] = useState<MysqlStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState('')
  const [provider, setProvider] = useState<'mysql' | 'mariadb'>('mysql')
  const [adminPassword, setAdminPassword] = useState('')
  const [showPasswords, setShowPasswords] = useState(false)
  const [authorized, setAuthorized] = useState(false)
  const [checkingAccess, setCheckingAccess] = useState(true)
  const [newDbUser, setNewDbUser] = useState('')
  const [newDbName, setNewDbName] = useState('')
  const [newDbHost, setNewDbHost] = useState('localhost')
  const [newDbPassword, setNewDbPassword] = useState('')
  const [newDbPrivileges, setNewDbPrivileges] = useState('read_write')
  const [sql, setSql] = useState('SHOW DATABASES;')
  const [database, setDatabase] = useState('')
  const [output, setOutput] = useState('')
  const [error, setError] = useState('')
  const [catalog, setCatalog] = useState<CatalogKind>('databases')
  const [catalogOutput, setCatalogOutput] = useState('')
  const [resetUser, setResetUser] = useState('')
  const [resetHost, setResetHost] = useState('localhost')
  const [newPassword, setNewPassword] = useState('')

  const refresh = useCallback(async () => {
    setLoading(true)
    try { setStatus(await api<MysqlStatus>('mysql/status')) }
    catch (cause) { notify(`خطا: ${cause instanceof Error ? cause.message : 'وضعیت MySQL دریافت نشد.'}`) }
    finally { setLoading(false) }
  }, [notify])
  useEffect(() => {
    let alive = true
    void api<{ authorized: boolean }>('mysql/access').then(async (result) => {
      if (!alive) return
      setAuthorized(result.authorized)
      if (result.authorized) await refresh()
      else setLoading(false)
    }).catch((cause) => { if (alive) { setLoading(false); notify(`خطا: ${cause instanceof Error ? cause.message : 'بررسی دسترسی MySQL ناموفق بود.'}`) } })
      .finally(() => { if (alive) setCheckingAccess(false) })
    return () => { alive = false }
  }, [notify, refresh])

  async function unlock() {
    if (!adminPassword) { notify('رمز پنل را وارد کن.'); return }
    setBusy('unlock'); setError('')
    try {
      await api('mysql/access', { method: 'POST', body: JSON.stringify({ password: adminPassword }) })
      setAdminPassword(''); setAuthorized(true); notify('دسترسی MySQL برای نشست فعلی فعال شد.'); await refresh()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'تأیید رمز پنل ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function installServer() {
    if (!window.confirm(`بستهٔ ${provider === 'mysql' ? 'MySQL Server' : 'MariaDB Server'} با APT نصب و سرویس آن فعال می‌شود. ادامه می‌دهی؟`)) return
    setBusy('install'); setError(''); setOutput('')
    try {
      const result = await api<{ message: string; output?: string }>('mysql/install', { method: 'POST', body: JSON.stringify({ provider, confirmation: 'INSTALL MYSQL' }) })
      setOutput(result.output || result.message); notify(result.message); await refresh()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'نصب دیتابیس ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function serviceAction(action: 'start' | 'stop' | 'restart' | 'enable-now' | 'disable') {
    if (['stop', 'restart', 'disable'].includes(action) && !window.confirm(`${action} روی سرویس MySQL ممکن است سایت‌ها و برنامه‌های متصل را مختل کند. ادامه می‌دهی؟`)) return
    setBusy(`service:${action}`); setError('')
    try {
      const result = await api<{ output: string }>('mysql/service', { method: 'POST', body: JSON.stringify({ action, confirmed: ['stop', 'restart', 'disable'].includes(action) }) })
      setOutput(result.output); notify(`عملیات سرویس MySQL (${action}) انجام شد.`); await refresh()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'عملیات سرویس ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function loadCatalog(kind: CatalogKind) {
    setBusy(`catalog:${kind}`); setError('')
    try {
      const result = await api<{ output: string }>('mysql/catalog', { method: 'POST', body: JSON.stringify({ kind }) })
      setCatalog(kind); setCatalogOutput(result.output); notify(kind === 'users' ? 'فهرست کاربران MySQL دریافت شد؛ رمزها در سرور قابل خواندن نیستند.' : 'فهرست دیتابیس‌ها دریافت شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'دریافت اطلاعات ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function runSql() {
    setBusy('sql'); setError(''); setOutput('')
    try {
      const command = sql.trim()
      const writesData = /\b(CREATE|ALTER|DROP|TRUNCATE|DELETE|UPDATE|INSERT|REPLACE|GRANT|REVOKE)\b/i.test(command)
      if (writesData && !window.confirm('این دستور می‌تواند داده یا دسترسی‌ها را تغییر دهد یا حذف کند. اجرای آن را تأیید می‌کنی؟')) { setBusy(''); return }
      const result = await api<{ output: string }>('mysql/query', { method: 'POST', body: JSON.stringify({ database: database.trim() || null, sql: command, confirm_write: writesData }) })
      setOutput(result.output || 'Query completed successfully (no rows returned).')
      notify('دستور SQL اجرا شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'اجرای SQL ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function resetUserPassword() {
    try {
      if (!resetUser.trim() || !newPassword) throw new Error('نام کاربر و رمز جدید را وارد کن.')
      if (!window.confirm(`رمز حساب ${resetUser}@${resetHost} تغییر می‌کند. رمز قبلی قابل مشاهده یا بازیابی نیست.`)) return
      setBusy('reset')
      const result = await api<{ message: string }>('mysql/users/password', { method: 'POST', body: JSON.stringify({ username: resetUser.trim(), host: resetHost, new_password: newPassword }) })
      setNewPassword(''); setOutput(result.message); notify(result.message); await loadCatalog('users')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'تغییر رمز ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function createDatabaseUser() {
    try {
      if (!newDbName.trim() || !newDbUser.trim() || !newDbPassword) throw new Error('دیتابیس، نام کاربر و رمز را وارد کن.')
      if (!window.confirm(`کاربر ${newDbUser}@${newDbHost} با دسترسی ${newDbPrivileges} فقط روی دیتابیس ${newDbName} ساخته می‌شود. ادامه می‌دهی؟`)) return
      setBusy('create-user'); setError('')
      const result = await api<{ message: string }>('mysql/users', { method: 'POST', body: JSON.stringify({ database: newDbName.trim(), username: newDbUser.trim(), host: newDbHost, password: newDbPassword, privileges: newDbPrivileges }) })
      setNewDbPassword(''); setOutput(result.message); notify(result.message); await loadCatalog('users')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'ساخت کاربر ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  const rows = catalogOutput.trim().split('\n').filter(Boolean).map((row) => row.split('\t'))
  const stateLabel = !status?.installed ? 'نصب نشده' : status.active ? 'فعال' : 'متوقف'
  if (checkingAccess) return <><Heading title="مدیریت MySQL" description="بررسی دسترسی امن به مدیریت دیتابیس."/><div className="mt-5 grid gap-3 sm:grid-cols-3"><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/></div></>
  if (!authorized) return <><Heading title="مدیریت MySQL" description="برای فعال‌سازی این بخش، رمز پنل را یک‌بار برای نشست فعلی تأیید کن."/><Card className="mx-auto mt-8 max-w-xl"><CardHeader><CardTitle className="flex items-center gap-2"><KeyRound size={18} className="text-primary"/>تأیید یک‌بارهٔ دسترسی</CardTitle><CardDescription>پس از تأیید، دیگر برای عملیات MySQL رمز پنل پرسیده نمی‌شود. این دسترسی با خروج از حساب باطل خواهد شد.</CardDescription></CardHeader><CardContent className="space-y-3"><Label htmlFor="mysql-unlock-password">رمز پنل</Label><div className="flex gap-2"><Input id="mysql-unlock-password" autoComplete="current-password" type={showPasswords ? 'text' : 'password'} value={adminPassword} onChange={(e) => setAdminPassword(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') void unlock() }}/><Button type="button" variant="outline" size="icon" onClick={() => setShowPasswords(v => !v)} aria-label={showPasswords ? 'پنهان‌کردن رمز' : 'نمایش رمز'}>{showPasswords ? <EyeOff size={15}/> : <Eye size={15}/>}</Button><Button onClick={() => void unlock()} disabled={!!busy}>{busy === 'unlock' ? <LoaderCircle size={15} className="animate-spin"/> : <ShieldCheck size={15}/>}تأیید</Button></div>{error && <Alert className="border-rose-400/25"><AlertDescription>{error}</AlertDescription></Alert>}</CardContent></Card></>
  return <>
    <Heading title="مدیریت MySQL" description="سرویس پایگاه‌داده، کاتالوگ‌ها، حساب‌ها و اجرای مستقیم SQL را مدیریت کن." action={<Button size="sm" variant="outline" onClick={() => void refresh()} disabled={loading || !!busy}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>بررسی وضعیت</Button>}/>
    {loading && !status ? <div className="grid gap-3 sm:grid-cols-3"><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/></div> : <div className="grid gap-3 sm:grid-cols-3">
      <Card><CardContent className="flex items-center gap-3 p-4"><Database className="text-primary"/><div><div className="font-semibold">وضعیت سرویس</div><Badge variant={status?.active ? 'success' : 'outline'} className="mt-2">{stateLabel}</Badge></div></CardContent></Card>
      <Card><CardContent className="flex items-center gap-3 p-4"><Activity className="text-primary"/><div className="min-w-0"><div className="font-semibold">نسخه</div><div className="mt-1 truncate font-mono text-xs text-muted-foreground" dir="ltr">{status?.version || '—'}</div></div></CardContent></Card>
      <Card><CardContent className="flex items-center gap-3 p-4"><ShieldCheck className="text-primary"/><div><div className="font-semibold">نوع سرویس</div><div className="mt-1 text-xs uppercase text-muted-foreground">{status?.provider || '—'}</div></div></CardContent></Card>
    </div>}

    {status && !status.installed && <Card className="mt-5 border-amber-400/25"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Database size={18} className="text-amber-300"/>MySQL هنوز نصب نشده</CardTitle><CardDescription>نصب از مخازن APT رسمی سیستم انجام می‌شود. پس از نصب، حساب مدیریتی محلی ARG به‌صورت خودکار ساخته می‌شود.</CardDescription></CardHeader><CardContent><div className="mb-4 flex flex-wrap gap-2">{(['mysql','mariadb'] as const).map((item) => <Button key={item} variant={provider === item ? 'default' : 'outline'} onClick={() => setProvider(item)}>{item === 'mysql' ? 'MySQL Server' : 'MariaDB Server'}</Button>)}</div><Button onClick={() => void installServer()} disabled={!!busy}>{busy === 'install' ? <><LoaderCircle size={15} className="animate-spin"/>در حال نصب…</> : <><Wrench size={15}/>نصب و فعال‌سازی سرویس</>}</Button></CardContent></Card>}

    {status?.installed && <>
      <Alert className="mt-5 border-emerald-400/20 bg-emerald-400/[.04]"><ShieldCheck size={16}/><AlertDescription>حساب مدیریتی داخلی ARG با رمز تصادفی و دسترسی کامل به دیتابیس‌ها ساخته شده است. رمز فقط در فایل root-only سرور نگه‌داری می‌شود و در پنل یا گزارش‌ها نمایش داده نمی‌شود.</AlertDescription></Alert>

      <Card className="mt-4"><CardHeader className="pb-3"><CardTitle className="text-sm">کنترل سرویس دیتابیس</CardTitle><CardDescription>توقف یا راه‌اندازی مجدد می‌تواند اتصال برنامه‌های روی این VPS به دیتابیس را قطع کند.</CardDescription></CardHeader><CardContent className="flex flex-wrap gap-2"><Button size="sm" onClick={() => void serviceAction(status.active ? 'restart' : 'start')} disabled={!!busy}><Activity size={14}/>{busy.startsWith('service:') ? 'در حال اجرا…' : status.active ? 'راه‌اندازی مجدد' : 'شروع سرویس'}</Button><Button size="sm" variant="outline" onClick={() => void serviceAction('enable-now')} disabled={!!busy}>فعال‌سازی هنگام بوت</Button><Button size="sm" variant="outline" onClick={() => void serviceAction('disable')} disabled={!!busy}>غیرفعال‌سازی خودکار</Button><Button size="sm" variant="destructive" onClick={() => void serviceAction('stop')} disabled={!!busy}>توقف سرویس</Button></CardContent></Card>
      <div className="mt-5 grid gap-4 xl:grid-cols-[.85fr_1.15fr]">
        <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Database size={17} className="text-primary"/>کاتالوگ دیتابیس و کاربران</CardTitle><CardDescription>اطلاعات از سرور خوانده می‌شود؛ رمز یا hash کاربران هرگز نمایش داده نمی‌شود.</CardDescription></CardHeader><CardContent><div className="flex flex-wrap gap-2"><Button variant={catalog === 'databases' ? 'default' : 'outline'} onClick={() => void loadCatalog('databases')} disabled={!!busy}><Database size={14}/>دیتابیس‌ها</Button><Button variant={catalog === 'users' ? 'default' : 'outline'} onClick={() => void loadCatalog('users')} disabled={!!busy}><UserRound size={14}/>کاربران</Button></div>{busy.startsWith('catalog:') && <Skeleton className="mt-4 h-28"/>}{catalogOutput && <div className="mt-4 max-h-64 overflow-auto rounded-xl border border-border"><table className="w-full text-xs"><tbody>{rows.map((row, index) => <tr key={`${row.join(':')}-${index}`} className={index ? 'border-t border-border/70' : 'bg-secondary/50 font-semibold'}>{row.map((cell, col) => <td key={col} className="px-3 py-2 font-mono" dir="ltr">{cell}</td>)}{catalog === 'users' && index > 0 && row[0] !== 'arg_manager' && <td className="px-2 py-1 text-left"><Button size="sm" variant="ghost" onClick={() => { setResetUser(row[0] || ''); setResetHost(['localhost', '127.0.0.1', '%'].includes(row[1] || '') ? row[1] : 'localhost') }}>بازنشانی رمز</Button></td>}</tr>)}</tbody></table></div>}
          <div className="mt-5 space-y-3 border-t border-border/70 pt-4"><div className="text-sm font-semibold">ساخت کاربر برای دیتابیس</div><p className="text-xs leading-5 text-muted-foreground">مجوز کاربر فقط به دیتابیس انتخابی محدود می‌شود.</p><div className="grid gap-2 sm:grid-cols-2"><Input value={newDbName} onChange={(e) => setNewDbName(e.target.value)} placeholder="نام دیتابیس" dir="ltr"/><Input value={newDbUser} onChange={(e) => setNewDbUser(e.target.value)} placeholder="نام کاربر" dir="ltr"/><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={newDbHost} onChange={(e) => setNewDbHost(e.target.value)}><option value="localhost">فقط localhost</option><option value="127.0.0.1">فقط 127.0.0.1</option><option value="%">هر میزبان (%)</option></select><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={newDbPrivileges} onChange={(e) => setNewDbPrivileges(e.target.value)}><option value="read_only">فقط خواندن</option><option value="read_write">خواندن و نوشتن</option><option value="full">دسترسی کامل به همین دیتابیس</option></select><div className="flex gap-2 sm:col-span-2"><Input value={newDbPassword} onChange={(e) => setNewDbPassword(e.target.value)} type={showPasswords ? 'text' : 'password'} placeholder="رمز کاربر (حداقل 12 نویسه)" autoComplete="new-password"/><Button type="button" size="icon" variant="outline" onClick={() => setShowPasswords(v => !v)} aria-label={showPasswords ? 'پنهان‌کردن رمزها' : 'نمایش رمزها'}>{showPasswords ? <EyeOff size={15}/> : <Eye size={15}/>}</Button></div></div><Button variant="outline" onClick={() => void createDatabaseUser()} disabled={!!busy}><UserRound size={14}/>{busy === 'create-user' ? 'در حال ساخت…' : 'ساخت کاربر دیتابیس'}</Button></div>
          <div className="mt-5 space-y-3 border-t border-border/70 pt-4"><div className="text-sm font-semibold">بازنشانی رمز حساب MySQL</div><p className="text-xs leading-5 text-muted-foreground">رمز ذخیره‌شده قابل خواندن نیست؛ رمز تازه برای همان user@host تنظیم می‌شود.</p><div className="grid gap-2 sm:grid-cols-2"><Input value={resetUser} onChange={(event) => setResetUser(event.target.value)} placeholder="نام کاربر" dir="ltr"/><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={resetHost} onChange={(event) => setResetHost(event.target.value)}><option value="localhost">localhost</option><option value="127.0.0.1">127.0.0.1</option><option value="%">% (هر میزبان)</option></select><Input value={newPassword} onChange={(event) => setNewPassword(event.target.value)} type={showPasswords ? 'text' : 'password'} placeholder="رمز جدید (حداقل 12 نویسه)" className="sm:col-span-2" autoComplete="new-password"/></div><Button variant="outline" onClick={() => void resetUserPassword()} disabled={!!busy}><KeyRound size={14}/>{busy === 'reset' ? 'در حال تغییر…' : 'تغییر رمز حساب'}</Button></div>
        </CardContent></Card>

        <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Play size={17} className="text-primary"/>کنسول SQL</CardTitle><CardDescription>دستورها مستقیماً به MySQL محلی فرستاده می‌شوند؛ بدون shell. خروجی حداکثر 1 MiB و زمان اجرا حداکثر 30 ثانیه است.</CardDescription></CardHeader><CardContent><div className="mb-3 grid gap-2 sm:grid-cols-[1fr_220px]"><div className="space-y-1.5"><Label htmlFor="mysql-database">دیتابیس هدف (اختیاری)</Label><Input id="mysql-database" value={database} onChange={(event) => setDatabase(event.target.value)} placeholder="مثلاً app_db" dir="ltr"/></div><div className="flex flex-wrap items-end gap-2"><Button size="sm" variant="outline" onClick={() => setSql('SHOW DATABASES;')}>SHOW DATABASES</Button><Button size="sm" variant="outline" onClick={() => setSql('SHOW TABLES;')}>SHOW TABLES</Button></div></div><Label htmlFor="mysql-sql">دستور SQL</Label><textarea id="mysql-sql" spellCheck={false} value={sql} onChange={(event) => setSql(event.target.value)} className="mt-2 min-h-[220px] w-full resize-y rounded-xl border border-border bg-[#090d12] p-4 text-left font-mono text-xs leading-6 text-foreground outline-none transition focus:border-primary/50" dir="ltr" placeholder="SELECT VERSION();"/><div className="mt-3 flex flex-wrap items-center justify-between gap-2"><span className="text-[11px] text-muted-foreground">دستورات مخرب تأیید جداگانه می‌خواهند؛ خروجی و رمزها در ARG ثبت نمی‌شوند.</span><Button onClick={() => void runSql()} disabled={!!busy || !sql.trim()}>{busy === 'sql' ? <><LoaderCircle size={15} className="animate-spin"/>در حال اجرا…</> : <><Play size={14}/>اجرای SQL</>}</Button></div>{busy === 'sql' && <div className="arg-apt-progress mt-3 h-1.5 w-full rounded-full bg-primary/70"/>}</CardContent></Card>
      </div>
    </>}

    {error && <Alert className="mt-4 border-rose-400/25 bg-rose-400/[.06]"><AlertTriangle size={16}/><AlertDescription>{error}</AlertDescription></Alert>}
    {output && <Card className="mt-4"><CardHeader className="pb-2"><CardTitle className="text-sm">نتیجهٔ عملیات</CardTitle></CardHeader><CardContent><pre className="max-h-[420px] overflow-auto whitespace-pre-wrap break-words rounded-xl bg-[#090d12] p-4 text-left font-mono text-xs leading-5 text-muted-foreground" dir="ltr">{output}</pre></CardContent></Card>}
    <p className="mt-5 text-[11px] leading-5 text-muted-foreground">رمز کاربران MySQL قابل بازیابی نیست؛ برای دسترسی مجدد از بخش بازنشانی رمز استفاده کن. رمز حساب داخلی ARG در فایل محافظت‌شدهٔ root ذخیره می‌شود و به رابط یا گزارش‌ها برنمی‌گردد.</p>
  </>
}
