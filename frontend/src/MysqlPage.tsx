import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Activity, AlertTriangle, Database, Eye, EyeOff, KeyRound, LoaderCircle, Play, RefreshCw, ShieldCheck, UserRound, Wrench, Save, Trash2, LockKeyhole, UnlockKeyhole, Gauge, Settings2, Plus, TerminalSquare } from 'lucide-react'
import { api } from './lib/api'
import { cachedRequest } from './lib/heavy-cache'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'
import { Alert, AlertDescription } from './components/ui/alert'
import { Skeleton } from './components/ui/skeleton'
import { PostgresInventoryPanel } from './PostgresInventoryPanel'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Common = { Heading: Heading; notify: (message: string) => void }
type MysqlStatus = { installed: boolean; active: boolean; provider: 'mysql' | 'mariadb'; version?: string | null }
type CatalogKind = 'databases' | 'users'
type SqlPreset = { id: number; title: string; database: string | null; sql: string; updated_at: number }
type MysqlRuntimeSetting = { name: string; value: string; nextValue: string }
const mysqlSettingNames = ['max_connections', 'wait_timeout', 'interactive_timeout', 'max_allowed_packet', 'long_query_time', 'slow_query_log', 'innodb_buffer_pool_size']

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
  const [newDatabase, setNewDatabase] = useState('')
  const [presets, setPresets] = useState<SqlPreset[]>([])
  const [presetTitle, setPresetTitle] = useState('')
  const [runtimeMetrics, setRuntimeMetrics] = useState<Record<string, string>>({})
  const [runtimeSettings, setRuntimeSettings] = useState<MysqlRuntimeSetting[]>([])
  const [advancedBusy, setAdvancedBusy] = useState(false)
  const [grantTarget, setGrantTarget] = useState<{ username: string; host: string } | null>(null)
  const [grantDatabase, setGrantDatabase] = useState('')
  const [grantLevel, setGrantLevel] = useState<'read_only' | 'read_write' | 'full'>('read_write')
  const [grantOperation, setGrantOperation] = useState<'grant' | 'revoke'>('grant')
  const [confirmation, setConfirmation] = useState('')
  const confirmationResolver = useRef<((confirmed: boolean) => void) | null>(null)

  function askConfirmation(message: string): Promise<boolean> {
    return new Promise((resolve) => {
      confirmationResolver.current = resolve
      setConfirmation(message)
    })
  }
  function resolveConfirmation(confirmed: boolean) {
    confirmationResolver.current?.(confirmed)
    confirmationResolver.current = null
    setConfirmation('')
  }

  const refresh = useCallback(async (force = true) => {
    setLoading(true)
    try { setStatus(await cachedRequest('mysql-status', () => api<MysqlStatus>('mysql/status'), 15000, force)) }
    catch (cause) { notify(`خطا: ${cause instanceof Error ? cause.message : 'وضعیت MySQL دریافت نشد.'}`) }
    finally { setLoading(false) }
  }, [notify])
  useEffect(() => {
    let alive = true
    void api<{ authorized: boolean }>('mysql/access').then(async (result) => {
      if (!alive) return
      setAuthorized(result.authorized)
      if (result.authorized) await refresh(false)
      else setLoading(false)
    }).catch((cause) => { if (alive) { setLoading(false); notify(`خطا: ${cause instanceof Error ? cause.message : 'بررسی دسترسی MySQL ناموفق بود.'}`) } })
      .finally(() => { if (alive) setCheckingAccess(false) })
    return () => { alive = false }
  }, [notify, refresh])
  useEffect(() => {
    if (!authorized) return
    void api<{ presets: SqlPreset[] }>('mysql/presets').then((result) => setPresets(result.presets)).catch(() => {})
  }, [authorized])

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
    if (!await askConfirmation(`بستهٔ ${provider === 'mysql' ? 'MySQL Server' : 'MariaDB Server'} با APT نصب و سرویس آن فعال می‌شود. ادامه می‌دهی؟`)) return
    setBusy('install'); setError(''); setOutput('')
    try {
      const result = await api<{ message: string; output?: string }>('mysql/install', { method: 'POST', body: JSON.stringify({ provider, confirmation: 'INSTALL MYSQL' }) })
      setOutput(result.output || result.message); notify(result.message); await refresh()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'نصب دیتابیس ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function serviceAction(action: 'start' | 'stop' | 'restart' | 'enable-now' | 'disable') {
    if (['stop', 'restart', 'disable'].includes(action) && !await askConfirmation(`${action} روی سرویس MySQL ممکن است سایت‌ها و برنامه‌های متصل را مختل کند. ادامه می‌دهی؟`)) return
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
  async function loadAdvanced() {
    setAdvancedBusy(true); setError('')
    try {
      const [metrics, settings] = await Promise.all([
        api<{ output: string }>('mysql/metrics'),
        api<{ output: string }>('mysql/settings'),
      ])
      const readTable = (output: string) => Object.fromEntries(output.trim().split('\n').slice(1).filter(Boolean).map((line) => { const [name, value] = line.split('\t'); return [name, value || ''] }))
      setRuntimeMetrics(readTable(metrics.output));
      const values = readTable(settings.output)
      setRuntimeSettings(mysqlSettingNames.map((name) => ({ name, value: values[name] || '—', nextValue: values[name] || '' })))
      notify('آمار و تنظیمات جاری سرور MySQL خوانده شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'خواندن تنظیمات سرور MySQL ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setAdvancedBusy(false) }
  }
  async function createDatabase() {
    const name = newDatabase.trim()
    if (!name) return
    if (!await askConfirmation(`دیتابیس ${name} با charset utf8mb4 ساخته می‌شود.`)) return
    setBusy('database-create'); setError('')
    try {
      const result = await api<{ message: string }>('mysql/databases', { method: 'POST', body: JSON.stringify({ name, confirmation: `CREATE DATABASE ${name}` }) })
      setOutput(result.message); setNewDatabase(''); notify(result.message); await loadCatalog('databases')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'ساخت دیتابیس ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function dropDatabase(name: string) {
    if (!await askConfirmation(`هشدار: دیتابیس ${name} و تمام داده‌های داخل آن برای همیشه حذف می‌شود. ادامه می‌دهی؟`)) return
    setBusy('database-drop'); setError('')
    try {
      const result = await api<{ message: string }>(`mysql/databases/${encodeURIComponent(name)}`, { method: 'DELETE', body: JSON.stringify({ name, confirmation: `DROP DATABASE ${name}` }) })
      setOutput(result.message); notify(result.message); await loadCatalog('databases')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'حذف دیتابیس ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function runUserAction(action: 'grants' | 'lock' | 'unlock' | 'delete', username: string, host: string) {
    const label = action === 'delete' ? 'حذف حساب' : action === 'lock' ? 'قفل حساب' : action === 'unlock' ? 'بازکردن قفل حساب' : 'مشاهدهٔ مجوزها'
    if (action !== 'grants' && !await askConfirmation(`${label} ${username}@${host} را تأیید می‌کنی؟`)) return
    setBusy(`user:${action}`); setError('')
    try {
      const result = await api<{ message: string; output: string }>('mysql/users/action', { method: 'POST', body: JSON.stringify({ action, username, host, confirmed: action === 'grants' ? false : true }) })
      if (action === 'grants') setOutput(result.output || 'مجوزی برای نمایش وجود ندارد.')
      else { setOutput(result.message); await loadCatalog('users') }
      notify(result.message || `${label} انجام شد.`)
    } catch (cause) { const message = cause instanceof Error ? cause.message : `${label} ناموفق بود.`; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function updateUserPrivileges() {
    if (!grantTarget || !grantDatabase.trim()) { notify('کاربر و دیتابیس هدف را انتخاب کن.'); return }
    const verb = grantOperation === 'grant' ? 'اعطا' : 'لغو'
    if (!await askConfirmation(`دسترسی ${grantLevel} کاربر ${grantTarget.username}@${grantTarget.host} روی ${grantDatabase} ${verb} می‌شود.`)) return
    setBusy('privileges'); setError('')
    try {
      const result = await api<{ message: string }>('mysql/users/privileges', { method: 'POST', body: JSON.stringify({ ...grantTarget, database: grantDatabase.trim(), privileges: grantLevel, operation: grantOperation, confirmed: true }) })
      setOutput(result.message); notify(result.message); await loadCatalog('users')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'تغییر مجوز ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function saveSqlPreset() {
    if (!presetTitle.trim() || !sql.trim()) { notify('برای ذخیرهٔ پریست، نام و SQL لازم است.'); return }
    setBusy('preset-save'); setError('')
    try {
      await api('mysql/presets', { method: 'POST', body: JSON.stringify({ title: presetTitle.trim(), database: database.trim() || null, sql }) })
      const result = await api<{ presets: SqlPreset[] }>('mysql/presets')
      setPresets(result.presets); notify('پریست SQL ذخیره شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'ذخیرهٔ پریست ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function deleteSqlPreset(id: number) {
    try { await api(`mysql/presets/${id}`, { method: 'DELETE' }); setPresets((current) => current.filter((preset) => preset.id !== id)); notify('پریست حذف شد.') }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'حذف پریست ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
  }
  async function applyRuntimeSetting(setting: MysqlRuntimeSetting) {
    if (!setting.nextValue.trim() || setting.nextValue === setting.value) return
    if (!await askConfirmation(`مقدار ${setting.name} در سرور جاری MySQL تغییر می‌کند. این تغییر ممکن است پس از restart پایدار نماند. ادامه می‌دهی؟`)) return
    setBusy(`setting:${setting.name}`); setError('')
    try {
      const result = await api<{ message: string }>('mysql/settings', { method: 'PUT', body: JSON.stringify({ name: setting.name, value: setting.nextValue, confirmed: true }) })
      setOutput(result.message); notify(result.message); await loadAdvanced()
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'تغییر تنظیم ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function runSql() {
    setBusy('sql'); setError(''); setOutput('')
    try {
      const command = sql.trim()
      const writesData = /\b(CREATE|ALTER|DROP|TRUNCATE|DELETE|UPDATE|INSERT|REPLACE|GRANT|REVOKE|SET|CALL|RENAME|ANALYZE|OPTIMIZE|REPAIR|FLUSH|RESET|KILL|SHUTDOWN|LOAD|INSTALL|UNINSTALL|START|STOP|CHANGE|PURGE|LOCK|UNLOCK)\b/i.test(command)
      if (writesData && !await askConfirmation('این دستور می‌تواند داده یا دسترسی‌ها را تغییر دهد یا حذف کند. اجرای آن را تأیید می‌کنی؟')) { setBusy(''); return }
      const result = await api<{ output: string }>('mysql/query', { method: 'POST', body: JSON.stringify({ database: database.trim() || null, sql: command, confirm_write: writesData }) })
      setOutput(result.output || 'Query completed successfully (no rows returned).')
      notify('دستور SQL اجرا شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'اجرای SQL ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }
  async function resetUserPassword() {
    try {
      if (!resetUser.trim() || !newPassword) throw new Error('نام کاربر و رمز جدید را وارد کن.')
      if (!await askConfirmation(`رمز حساب ${resetUser}@${resetHost} تغییر می‌کند. رمز قبلی قابل مشاهده یا بازیابی نیست.`)) return
      setBusy('reset')
      const result = await api<{ message: string }>('mysql/users/password', { method: 'POST', body: JSON.stringify({ username: resetUser.trim(), host: resetHost, new_password: newPassword }) })
      setNewPassword(''); setOutput(result.message); notify(result.message); await loadCatalog('users')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'تغییر رمز ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  async function createDatabaseUser() {
    try {
      if (!newDbName.trim() || !newDbUser.trim() || !newDbPassword) throw new Error('دیتابیس، نام کاربر و رمز را وارد کن.')
      if (!await askConfirmation(`کاربر ${newDbUser}@${newDbHost} با دسترسی ${newDbPrivileges} فقط روی دیتابیس ${newDbName} ساخته می‌شود. ادامه می‌دهی؟`)) return
      setBusy('create-user'); setError('')
      const result = await api<{ message: string }>('mysql/users', { method: 'POST', body: JSON.stringify({ database: newDbName.trim(), username: newDbUser.trim(), host: newDbHost, password: newDbPassword, privileges: newDbPrivileges }) })
      setNewDbPassword(''); setOutput(result.message); notify(result.message); await loadCatalog('users')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'ساخت کاربر ناموفق بود.'; setError(message); notify(`خطا: ${message}`) }
    finally { setBusy('') }
  }

  const rows = catalogOutput.trim().split('\n').filter(Boolean).map((row) => row.split('\t'))
  const stateLabel = !status?.installed ? 'نصب نشده' : status.active ? 'فعال' : 'متوقف'
  if (checkingAccess) return <><Heading title="مدیریت MySQL" description="بررسی دسترسی امن به مدیریت دیتابیس."/><PostgresInventoryPanel/><div className="mt-5 grid gap-3 sm:grid-cols-3"><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/><Skeleton className="h-28 rounded-2xl"/></div></>
  if (!authorized) return <><Heading title="مدیریت MySQL" description="برای فعال‌سازی این بخش، رمز پنل را یک‌بار برای نشست فعلی تأیید کن."/><PostgresInventoryPanel/><Card className="mx-auto mt-8 max-w-xl"><CardHeader><CardTitle className="flex items-center gap-2"><KeyRound size={18} className="text-primary"/>تأیید یک‌بارهٔ دسترسی</CardTitle><CardDescription>پس از تأیید، دیگر برای عملیات MySQL رمز پنل پرسیده نمی‌شود. این دسترسی با خروج از حساب باطل خواهد شد.</CardDescription></CardHeader><CardContent className="space-y-3"><Label htmlFor="mysql-unlock-password">رمز پنل</Label><div className="flex gap-2"><Input id="mysql-unlock-password" autoComplete="current-password" type={showPasswords ? 'text' : 'password'} value={adminPassword} onChange={(e) => setAdminPassword(e.target.value)} onKeyDown={(e) => { if (e.key === 'Enter') void unlock() }}/><Button type="button" variant="outline" size="icon" onClick={() => setShowPasswords(v => !v)} aria-label={showPasswords ? 'پنهان‌کردن رمز' : 'نمایش رمز'}>{showPasswords ? <EyeOff size={15}/> : <Eye size={15}/>}</Button><Button onClick={() => void unlock()} disabled={!!busy}>{busy === 'unlock' ? <LoaderCircle size={15} className="animate-spin"/> : <ShieldCheck size={15}/>}تأیید</Button></div>{error && <Alert className="border-rose-400/25"><AlertDescription>{error}</AlertDescription></Alert>}</CardContent></Card></>
  return <>
    <Heading title="مدیریت MySQL" description="سرویس پایگاه‌داده، کاتالوگ‌ها، حساب‌ها و اجرای مستقیم SQL را مدیریت کن." action={<Button size="sm" variant="outline" onClick={() => void refresh()} disabled={loading || !!busy}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>بررسی وضعیت</Button>}/>
    <PostgresInventoryPanel/>
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
        <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Database size={17} className="text-primary"/>کاتالوگ دیتابیس و کاربران</CardTitle><CardDescription>اطلاعات از سرور خوانده می‌شود؛ رمز یا hash کاربران هرگز نمایش داده نمی‌شود.</CardDescription></CardHeader><CardContent><div className="flex flex-wrap gap-2"><Button variant={catalog === 'databases' ? 'default' : 'outline'} onClick={() => void loadCatalog('databases')} disabled={!!busy}><Database size={14}/>دیتابیس‌ها</Button><Button variant={catalog === 'users' ? 'default' : 'outline'} onClick={() => void loadCatalog('users')} disabled={!!busy}><UserRound size={14}/>کاربران</Button></div>{busy.startsWith('catalog:') && <Skeleton className="mt-4 h-28"/>}{catalogOutput && <div className="mt-4 max-h-64 overflow-auto rounded-xl border border-border"><table className="w-full text-xs"><tbody>{rows.map((row, index) => <tr key={`${row.join(':')}-${index}`} className={index ? 'border-t border-border/70' : 'bg-secondary/50 font-semibold'}>{row.map((cell, col) => <td key={col} className="px-3 py-2 font-mono" dir="ltr">{cell}</td>)}{catalog === 'users' && index > 0 && row[0] !== 'arg_manager' && !['root','mysql','mariadb.sys'].includes(row[0] || '') && <td className="px-2 py-1 text-left"><div className="flex flex-wrap justify-end gap-1"><Button size="sm" variant="ghost" onClick={() => { setResetUser(row[0] || ''); setResetHost(row[1] || 'localhost') }} title="بازنشانی رمز">رمز</Button><Button size="icon" variant="ghost" className="h-8 w-8" onClick={() => void runUserAction('grants', row[0] || '', row[1] || 'localhost')} title="نمایش مجوزها"><ShieldCheck size={14}/></Button><Button size="icon" variant="ghost" className="h-8 w-8" onClick={() => { setGrantTarget({ username: row[0] || '', host: row[1] || 'localhost' }); setGrantDatabase('') }} title="مدیریت دسترسی"><Settings2 size={14}/></Button><Button size="icon" variant="ghost" className="h-8 w-8" onClick={() => void runUserAction(row[3] === 'Y' || row[3] === '1' ? 'unlock' : 'lock', row[0] || '', row[1] || 'localhost')} title={row[3] === 'Y' || row[3] === '1' ? 'بازکردن قفل' : 'قفل حساب'}>{row[3] === 'Y' || row[3] === '1' ? <UnlockKeyhole size={14}/> : <LockKeyhole size={14}/>}</Button><Button size="icon" variant="ghost" className="h-8 w-8 text-rose-300" onClick={() => void runUserAction('delete', row[0] || '', row[1] || 'localhost')} title="حذف کاربر"><Trash2 size={14}/></Button></div></td>}{catalog === 'databases' && index > 0 && !['mysql','sys','performance_schema','information_schema'].includes(row[0] || '') && <td className="px-2 py-1 text-left"><Button size="icon" variant="ghost" className="h-8 w-8 text-rose-300" onClick={() => void dropDatabase(row[0] || '')} title="حذف دیتابیس"><Trash2 size={14}/></Button></td>}</tr>)}</tbody></table></div>}
          {grantTarget && <div className="mt-4 space-y-3 rounded-xl border border-primary/20 bg-primary/[.035] p-3"><div className="flex items-center justify-between gap-2"><div className="text-xs font-semibold">مدیریت مجوز <code dir="ltr">{grantTarget.username}@{grantTarget.host}</code></div><Button size="sm" variant="ghost" onClick={() => setGrantTarget(null)}>بستن</Button></div><div className="grid gap-2 sm:grid-cols-2"><Input value={grantDatabase} onChange={(event) => setGrantDatabase(event.target.value)} placeholder="نام دیتابیس هدف" dir="ltr"/><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={grantLevel} onChange={(event) => setGrantLevel(event.target.value as typeof grantLevel)}><option value="read_only">خواندن</option><option value="read_write">خواندن و نوشتن</option><option value="full">دسترسی کامل همین دیتابیس</option></select></div><div className="flex flex-wrap gap-2"><Button size="sm" variant={grantOperation === 'grant' ? 'default' : 'outline'} onClick={() => setGrantOperation('grant')}>اعطای دسترسی</Button><Button size="sm" variant={grantOperation === 'revoke' ? 'destructive' : 'outline'} onClick={() => setGrantOperation('revoke')}>لغو دسترسی</Button><Button size="sm" variant="outline" onClick={() => void updateUserPrivileges()} disabled={!!busy || !grantDatabase.trim()}>{busy === 'privileges' ? <LoaderCircle size={14} className="animate-spin"/> : <ShieldCheck size={14}/>}اعمال روی دیتابیس</Button></div></div>}
          <div className="mt-4 flex flex-wrap gap-2 rounded-xl border border-border/70 p-3"><Input value={newDatabase} onChange={(event) => setNewDatabase(event.target.value)} placeholder="نام دیتابیس جدید" dir="ltr" className="min-w-40 flex-1"/><Button size="sm" variant="outline" onClick={() => void createDatabase()} disabled={!!busy || !newDatabase.trim()}><Plus size={14}/>ساخت دیتابیس</Button></div>
          <div className="mt-5 space-y-3 border-t border-border/70 pt-4"><div className="text-sm font-semibold">ساخت کاربر برای دیتابیس</div><p className="text-xs leading-5 text-muted-foreground">مجوز کاربر فقط به دیتابیس انتخابی محدود می‌شود.</p><div className="grid gap-2 sm:grid-cols-2"><Input value={newDbName} onChange={(e) => setNewDbName(e.target.value)} placeholder="نام دیتابیس" dir="ltr"/><Input value={newDbUser} onChange={(e) => setNewDbUser(e.target.value)} placeholder="نام کاربر" dir="ltr"/><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={newDbHost} onChange={(e) => setNewDbHost(e.target.value)}><option value="localhost">فقط localhost</option><option value="127.0.0.1">فقط 127.0.0.1</option><option value="%">هر میزبان (%)</option></select><select className="h-10 rounded-lg border border-input bg-background px-3 text-sm" value={newDbPrivileges} onChange={(e) => setNewDbPrivileges(e.target.value)}><option value="read_only">فقط خواندن</option><option value="read_write">خواندن و نوشتن</option><option value="full">دسترسی کامل به همین دیتابیس</option></select><div className="flex gap-2 sm:col-span-2"><Input value={newDbPassword} onChange={(e) => setNewDbPassword(e.target.value)} type={showPasswords ? 'text' : 'password'} placeholder="رمز کاربر (حداقل 12 نویسه)" autoComplete="new-password"/><Button type="button" size="icon" variant="outline" onClick={() => setShowPasswords(v => !v)} aria-label={showPasswords ? 'پنهان‌کردن رمزها' : 'نمایش رمزها'}>{showPasswords ? <EyeOff size={15}/> : <Eye size={15}/>}</Button></div></div><Button variant="outline" onClick={() => void createDatabaseUser()} disabled={!!busy}><UserRound size={14}/>{busy === 'create-user' ? 'در حال ساخت…' : 'ساخت کاربر دیتابیس'}</Button></div>
          <div className="mt-5 space-y-3 border-t border-border/70 pt-4"><div className="text-sm font-semibold">بازنشانی رمز حساب MySQL</div><p className="text-xs leading-5 text-muted-foreground">رمز ذخیره‌شده قابل خواندن نیست؛ رمز تازه برای همان user@host تنظیم می‌شود.</p><div className="grid gap-2 sm:grid-cols-2"><Input value={resetUser} onChange={(event) => setResetUser(event.target.value)} placeholder="نام کاربر" dir="ltr"/><Input value={resetHost} onChange={(event) => setResetHost(event.target.value)} placeholder="Host حساب، مثلاً localhost یا 10.%" dir="ltr"/><Input value={newPassword} onChange={(event) => setNewPassword(event.target.value)} type={showPasswords ? 'text' : 'password'} placeholder="رمز جدید (حداقل 12 نویسه)" className="sm:col-span-2" autoComplete="new-password"/></div><Button variant="outline" onClick={() => void resetUserPassword()} disabled={!!busy}><KeyRound size={14}/>{busy === 'reset' ? 'در حال تغییر…' : 'تغییر رمز حساب'}</Button></div>
        </CardContent></Card>

        <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Play size={17} className="text-primary"/>کنسول SQL</CardTitle><CardDescription>دستورها مستقیماً به MySQL محلی فرستاده می‌شوند؛ بدون shell. خروجی حداکثر 1 MiB و زمان اجرا حداکثر 30 ثانیه است.</CardDescription></CardHeader><CardContent><div className="mb-3 grid gap-2 sm:grid-cols-[1fr_220px]"><div className="space-y-1.5"><Label htmlFor="mysql-database">دیتابیس هدف (اختیاری)</Label><Input id="mysql-database" value={database} onChange={(event) => setDatabase(event.target.value)} placeholder="مثلاً app_db" dir="ltr"/></div><div className="flex flex-wrap items-end gap-2"><Button size="sm" variant="outline" onClick={() => setSql('SHOW DATABASES;')}>SHOW DATABASES</Button><Button size="sm" variant="outline" onClick={() => setSql('SHOW TABLES;')}>SHOW TABLES</Button></div></div><Label htmlFor="mysql-sql">دستور SQL</Label><textarea id="mysql-sql" spellCheck={false} value={sql} onChange={(event) => setSql(event.target.value)} className="mt-2 min-h-[220px] w-full resize-y rounded-xl border border-border bg-[#090d12] p-4 text-left font-mono text-xs leading-6 text-foreground outline-none transition focus:border-primary/50" dir="ltr" placeholder="SELECT VERSION();"/><div className="mt-3 flex flex-wrap items-center justify-between gap-2"><span className="text-[11px] text-muted-foreground">دستورات مخرب تأیید جداگانه می‌خواهند؛ خروجی و رمزها در ARG ثبت نمی‌شوند.</span><Button onClick={() => void runSql()} disabled={!!busy || !sql.trim()}>{busy === 'sql' ? <><LoaderCircle size={15} className="animate-spin"/>در حال اجرا…</> : <><Play size={14}/>اجرای SQL</>}</Button></div>{busy === 'sql' && <div className="arg-apt-progress mt-3 h-1.5 w-full rounded-full bg-primary/70"/>}</CardContent></Card>
      </div>
      <div className="mt-5 grid gap-4 xl:grid-cols-2">
        <Card><CardHeader><div className="flex flex-wrap items-center justify-between gap-3"><div><CardTitle className="flex items-center gap-2 text-base"><Gauge size={17} className="text-primary"/>آمار و تنظیمات سرور</CardTitle><CardDescription>خواندن متغیرهای runtime و تغییر allowlist امن؛ اعمال تنظیم‌ها ممکن است فقط تا restart برقرار باشد.</CardDescription></div><Button size="sm" variant="outline" onClick={() => void loadAdvanced()} disabled={advancedBusy || !!busy}><RefreshCw size={14} className={advancedBusy ? 'animate-spin' : ''}/>خواندن وضعیت</Button></div></CardHeader><CardContent>
          {Object.keys(runtimeMetrics).length > 0 && <div className="mb-4 grid grid-cols-2 gap-2 sm:grid-cols-4">{[['Threads_connected','اتصال‌های فعال'],['Max_used_connections','بیشترین اتصال'],['Queries','تعداد Query'],['Slow_queries','Query کند'],['Uptime','Uptime (s)'],['Connections','تلاش اتصال'],['Innodb_buffer_pool_reads','خواندن دیسک InnoDB']].map(([key,label]) => <div key={key} className="rounded-xl border border-border/70 bg-background/40 p-3"><div className="text-[10px] text-muted-foreground">{label}</div><div dir="ltr" className="mt-1 truncate font-mono text-sm">{runtimeMetrics[key] ?? '—'}</div></div>)}</div>}
          {!runtimeSettings.length ? <p className="rounded-xl border border-dashed border-border p-4 text-xs text-muted-foreground">برای مشاهدهٔ وضعیت و مقدار جاری، «خواندن وضعیت» را بزن.</p> : <div className="space-y-2">{runtimeSettings.map((setting) => <div key={setting.name} className="grid gap-2 rounded-xl border border-border/70 p-3 sm:grid-cols-[1fr_120px_auto] sm:items-center"><code dir="ltr" className="text-left font-mono text-xs">{setting.name}</code>{setting.name === 'slow_query_log' ? <select aria-label={`مقدار ${setting.name}`} className="h-9 rounded-lg border border-input bg-background px-2 text-xs" value={setting.nextValue.toUpperCase()} onChange={(event) => setRuntimeSettings((current) => current.map((item) => item.name === setting.name ? { ...item, nextValue: event.target.value } : item))}><option value="ON">ON</option><option value="OFF">OFF</option></select> : <Input aria-label={`مقدار ${setting.name}`} dir="ltr" className="h-9 font-mono text-xs" value={setting.nextValue} onChange={(event) => setRuntimeSettings((current) => current.map((item) => item.name === setting.name ? { ...item, nextValue: event.target.value } : item))}/>}<Button size="sm" variant="outline" onClick={() => void applyRuntimeSetting(setting)} disabled={!!busy || !setting.nextValue || setting.nextValue === setting.value}><Settings2 size={13}/>اعمال</Button><div className="text-[10px] text-muted-foreground sm:col-span-3">مقدار جاری: <code dir="ltr">{setting.value}</code></div></div>)}</div>}
          <p className="mt-3 text-[10px] leading-5 text-muted-foreground">تنظیمات فقط از فهرست allowlist تغییر می‌کنند. مقدارها مستقیم وارد SQL نمی‌شوند مگر پس از اعتبارسنجی نوع و محدوده؛ پایداری بعد از reboot تضمین نمی‌شود.</p>
        </CardContent></Card>
        <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><TerminalSquare size={17} className="text-primary"/>پریست‌ها و دستورهای سفارشی</CardTitle><CardDescription>دستورهای پرکاربردت را ذخیره کن تا بعداً در کنسول بازبینی و اجرا شوند. پریست خودش اجرا نمی‌شود.</CardDescription></CardHeader><CardContent>
          <div className="flex flex-wrap gap-2">{presets.map((preset) => <div key={preset.id} className="flex items-center gap-1 rounded-lg border border-border/70 pl-1"><Button size="sm" variant="ghost" onClick={() => { setSql(preset.sql); setDatabase(preset.database || '') }}>{preset.title}</Button><Button size="icon" variant="ghost" className="h-7 w-7 text-rose-300" onClick={() => void deleteSqlPreset(preset.id)} title="حذف پریست"><Trash2 size={13}/></Button></div>)}{!presets.length && <span className="text-xs text-muted-foreground">هنوز پریست سفارشی نساخته‌ای.</span>}</div>
          <div className="mt-4 flex flex-wrap gap-2"><Input value={presetTitle} onChange={(event) => setPresetTitle(event.target.value)} placeholder="نام پریست برای SQL فعلی" className="min-w-44 flex-1"/><Button size="sm" variant="outline" onClick={() => void saveSqlPreset()} disabled={!!busy || !presetTitle.trim() || !sql.trim()}><Save size={14}/>ذخیره SQL فعلی</Button></div>
          <div className="mt-4 border-t border-border/70 pt-3"><p className="mb-2 text-xs font-medium">نمونه‌های آماده</p><div className="flex flex-wrap gap-2">{[['اتصال‌های فعال','SHOW FULL PROCESSLIST;'],['جدول‌های دیتابیس','SHOW TABLE STATUS;'],['وضعیت InnoDB','SHOW ENGINE INNODB STATUS;'],['تنظیمات جاری','SHOW GLOBAL VARIABLES;'],['نسخه و زمان','SELECT VERSION(), NOW();']].map(([title,statement]) => <Button key={title} size="sm" variant="outline" onClick={() => setSql(statement)}>{title}</Button>)}</div></div>
        </CardContent></Card>
      </div>
    </>}

    {error && <Alert className="mt-4 border-rose-400/25 bg-rose-400/[.06]"><AlertTriangle size={16}/><AlertDescription>{error}</AlertDescription></Alert>}
    {output && <Card className="mt-4"><CardHeader className="pb-2"><CardTitle className="text-sm">نتیجهٔ عملیات</CardTitle></CardHeader><CardContent><pre className="max-h-[420px] overflow-auto whitespace-pre-wrap break-words rounded-xl bg-[#090d12] p-4 text-left font-mono text-xs leading-5 text-muted-foreground" dir="ltr">{output}</pre></CardContent></Card>}
    <p className="mt-5 text-[11px] leading-5 text-muted-foreground">رمز کاربران MySQL قابل بازیابی نیست؛ برای دسترسی مجدد از بخش بازنشانی رمز استفاده کن. رمز حساب داخلی ARG در فایل محافظت‌شدهٔ root ذخیره می‌شود و به رابط یا گزارش‌ها برنمی‌گردد.</p>
    {confirmation && <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/70 p-4 backdrop-blur-sm" onMouseDown={(event) => { if (event.target === event.currentTarget) resolveConfirmation(false) }}>
      <section role="alertdialog" aria-modal="true" aria-labelledby="mysql-confirm-title" aria-describedby="mysql-confirm-description" className="w-full max-w-md rounded-2xl border border-border bg-card p-5 shadow-2xl">
        <div className="flex items-start gap-3"><div className="rounded-xl bg-amber-500/10 p-2 text-amber-400"><AlertTriangle size={18}/></div><div><h2 id="mysql-confirm-title" className="font-semibold">تأیید عملیات MySQL</h2><p id="mysql-confirm-description" className="mt-2 text-sm leading-6 text-muted-foreground">{confirmation}</p></div></div>
        <div className="mt-5 flex justify-end gap-2"><Button variant="outline" onClick={() => resolveConfirmation(false)}>انصراف</Button><Button onClick={() => resolveConfirmation(true)}>تأیید و ادامه</Button></div>
      </section>
    </div>}
  </>
}
