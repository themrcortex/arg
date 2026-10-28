import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { AlertTriangle, Archive, CheckCircle2, Clock3, Cpu, Download, HardDrive, MemoryStick, RefreshCw, RotateCcw, Server, ShieldCheck, Trash2, Upload, Wrench } from 'lucide-react'
import { ApiError, api, apiUrl, formatBytes, formatNumber, type Dashboard } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Skeleton } from './components/ui/skeleton'
import { HostSecurityPanel } from './HostSecurityPanel'
import { ExtensionSecurityPanel } from './ExtensionSecurityPanel'
import { ApiKeysPanel, DoctorPanel } from './PlatformReadinessPanels'
import { UnifiedStepper } from './components/ui/stepper'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs'

type Common = { Heading: (props: { title: string; description: string; action?: ReactNode }) => ReactNode; notify: (message: string) => void }
type Backup = { id: string; size_bytes: number; automatic?: boolean; modified_at?: number | null }
type BackupPolicy = { enabled: boolean; interval_hours: number; retention_count: number; last_started_at: number | null; last_success_at: number | null; last_error: string | null; next_run_at: number | null; automatic_count: number }
type SystemInfo = { os: string; kernel: string; architecture: string; cpu: string; hostname: string }
const date = (value: number | null) => value ? new Date(value * 1000).toLocaleString('fa-IR') : '—'

function UptimeValue({ seconds }: { seconds: number }) {
  const days = Math.floor(seconds / 86400)
  const hours = Math.floor((seconds % 86400) / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return <span dir="rtl" className="inline-flex flex-wrap items-baseline gap-x-1.5" aria-label={`${formatNumber(days)} روز و ${formatNumber(hours)} ساعت روشن`}>{days > 0 ? <><bdi dir="ltr" className="font-mono tabular-nums">{formatNumber(days)}</bdi><span>روز و</span><bdi dir="ltr" className="font-mono tabular-nums">{formatNumber(hours)}</bdi><span>ساعت</span></> : hours > 0 ? <><bdi dir="ltr" className="font-mono tabular-nums">{formatNumber(hours)}</bdi><span>ساعت و</span><bdi dir="ltr" className="font-mono tabular-nums">{formatNumber(minutes)}</bdi><span>دقیقه</span></> : <><bdi dir="ltr" className="font-mono tabular-nums">{formatNumber(minutes)}</bdi><span>دقیقه</span></>}</span>
}

export function MaintenancePage({ Heading, notify }: Common) {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null)
  const [system, setSystem] = useState<SystemInfo | null>(null)
  const [backups, setBackups] = useState<Backup[]>([])
  const [backupPolicy, setBackupPolicy] = useState<BackupPolicy | null>(null)
  const [backupEnabled, setBackupEnabled] = useState(false)
  const [backupIntervalHours, setBackupIntervalHours] = useState('24')
  const [backupRetentionCount, setBackupRetentionCount] = useState('7')
  const [backupPolicyLoading, setBackupPolicyLoading] = useState(false)
  const [backupPolicySaving, setBackupPolicySaving] = useState(false)
  const [backupPolicyError, setBackupPolicyError] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState('')
  const [actionTarget, setActionTarget] = useState<'repair' | 'reboot' | 'uninstall' | null>(null)
  const [actionPhrase, setActionPhrase] = useState('')
  const [restoreTarget, setRestoreTarget] = useState<Backup | null>(null)
  const [restorePhrase, setRestorePhrase] = useState('')
  const [restoreStep, setRestoreStep] = useState(0)
  const backupUploadRef = useRef<HTMLInputElement>(null)
  const loadBackupPolicy = useCallback(async () => {
    setBackupPolicyLoading(true); setBackupPolicyError('')
    try {
      const policy = await api<BackupPolicy>('maintenance/backup-policy')
      setBackupPolicy(policy); setBackupEnabled(policy.enabled); setBackupIntervalHours(String(policy.interval_hours)); setBackupRetentionCount(String(policy.retention_count))
    } catch (error) {
      setBackupPolicyError(error instanceof ApiError && error.status === 404
        ? 'مدیریت بکاپ خودکار در نسخهٔ این VPS یا Node موجود نیست؛ ARG همان سرور را به‌روزرسانی کن.'
        : error instanceof Error ? error.message : 'تنظیمات بکاپ خودکار دریافت نشد.')
    } finally { setBackupPolicyLoading(false) }
  }, [])
  const refresh = useCallback(async () => {
    setLoading(true)
    void loadBackupPolicy()
    try {
      const [metrics, host, stored] = await Promise.all([
        api<Dashboard>('dashboard'), api<SystemInfo>('maintenance/system'), api<{ backups: Backup[] }>('maintenance/backups'),
      ])
      setDashboard(metrics); setSystem(host); setBackups(stored.backups)
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : 'دریافت اطلاعات سرور ناموفق بود'}`) }
    finally { setLoading(false) }
  }, [loadBackupPolicy, notify])
  useEffect(() => { void refresh() }, [refresh])

  function action(kind: 'repair' | 'reboot' | 'uninstall') {
    setActionPhrase('')
    setActionTarget(kind)
  }

  async function confirmAction() {
    const kind = actionTarget
    if (!kind) return
    const labels = { repair: ['REPAIR ARG', 'بازسازی کامل ARG'], reboot: ['REBOOT SERVER', 'راه‌اندازی مجدد سرور'], uninstall: ['DELETE ARG', 'حذف کامل ARG'] } as const
    const [phrase, title] = labels[kind]
    if (actionPhrase.trim() !== phrase) { notify('عبارت تأیید مطابقت ندارد.'); return }
    setBusy(kind)
    try {
      let branch: 'main' | 'development' = 'main'
      try { branch = window.localStorage.getItem('arg.update-branch') === 'development' ? 'development' : 'main' } catch { /* Stable is the safe default. */ }
      const result = await api<{ message?: string; ok?: boolean }>('maintenance/action', { method: 'POST', body: JSON.stringify({ action: kind, confirmation: phrase, ...(kind === 'repair' ? { branch } : {}) }) })
      notify(result.message || `${title} انجام شد`)
      setActionTarget(null)
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : `${title} ناموفق بود`}`) }
    finally { setBusy('') }
  }

  async function createBackup() {
    setBusy('backup')
    try {
      const backup = await api<Backup & { message: string }>('maintenance/backups', { method: 'POST', body: '{}' })
      setBackups((items) => [backup, ...items.filter((item) => item.id !== backup.id)])
      notify(`بکاپ دیتابیس ARG ساخته شد · ${formatBytes(backup.size_bytes)}`)
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : 'ساخت بکاپ ناموفق بود'}`) }
    finally { setBusy('') }
  }

  async function saveBackupPolicy() {
    const intervalHours = Number(backupIntervalHours)
    const retentionCount = Number(backupRetentionCount)
    if (!Number.isInteger(intervalHours) || intervalHours < 1 || intervalHours > 720 || !Number.isInteger(retentionCount) || retentionCount < 1 || retentionCount > 100) {
      setBackupPolicyError('فاصله باید بین ۱ تا ۷۲۰ ساعت و تعداد نسخه‌ها بین ۱ تا ۱۰۰ باشد.')
      return
    }
    setBackupPolicySaving(true); setBackupPolicyError('')
    try {
      const policy = await api<BackupPolicy>('maintenance/backup-policy', { method: 'PUT', body: JSON.stringify({ enabled: backupEnabled, interval_hours: intervalHours, retention_count: retentionCount }) })
      setBackupPolicy(policy); notify(backupEnabled ? 'زمان‌بندی بکاپ خودکار ذخیره شد.' : 'بکاپ خودکار غیرفعال شد.')
    } catch (error) {
      setBackupPolicyError(error instanceof ApiError && error.status === 404
        ? 'این نسخهٔ VPS یا Node مدیریت بکاپ خودکار را پشتیبانی نمی‌کند؛ ARG همان سرور را به‌روزرسانی کن.'
        : error instanceof Error ? error.message : 'ذخیرهٔ تنظیمات بکاپ خودکار ناموفق بود.')
    } finally { setBackupPolicySaving(false) }
  }

  async function uploadBackup(file?: File) {
    if (!file) return
    if (file.size > 512 * 1024 * 1024) {
      notify('خطا: اندازهٔ فایل بکاپ نباید از 512 MiB بیشتر باشد.')
      if (backupUploadRef.current) backupUploadRef.current.value = ''
      return
    }
    setBusy('upload')
    try {
      const response = await fetch(apiUrl('maintenance/backups/upload'), {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/vnd.sqlite3' }, body: file,
      })
      const result = await response.json().catch(() => null) as (Backup & { message?: string; error?: string }) | null
      if (!response.ok || !result?.id) throw new Error(result?.error || `HTTP ${response.status}`)
      setBackups((items) => [result, ...items.filter((item) => item.id !== result.id)])
      notify(`بکاپ آپلود و بررسی شد · ${formatBytes(result.size_bytes)}`)
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : 'آپلود بکاپ ناموفق بود'}`) }
    finally { setBusy(''); if (backupUploadRef.current) backupUploadRef.current.value = '' }
  }

  async function downloadBackup(id: string) {
    setBusy(`download:${id}`)
    try {
      const response = await fetch(apiUrl(`maintenance/backups/${encodeURIComponent(id)}/download`), { credentials: 'same-origin' })
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { error?: string } | null
        throw new Error(body?.error || `HTTP ${response.status}`)
      }
      const blob = await response.blob()
      if (!blob.size) throw new Error('فایل بکاپ خالی دریافت شد؛ دوباره تلاش کن.')
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a'); link.href = url; link.download = id; link.hidden = true; document.body.appendChild(link); link.click(); link.remove(); window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
      notify('دانلود بکاپ آغاز شد')
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : 'دانلود بکاپ ناموفق بود'}`) }
    finally { setBusy('') }
  }

  function restoreBackup(backup: Backup) {
    setRestorePhrase('')
    setRestoreStep(0)
    setRestoreTarget(backup)
  }

  async function confirmRestore() {
    if (!restoreTarget || restoreStep !== 1) return
    if (restorePhrase.trim() !== 'RESTORE ARG') { notify('عبارت تأیید مطابقت ندارد.'); return }
    const id = restoreTarget.id
    setBusy(`restore:${id}`)
    try {
      const result = await api<{ message: string }>(`maintenance/backups/${encodeURIComponent(id)}/restore`, { method: 'POST', body: JSON.stringify({ confirmation: 'RESTORE ARG' }) })
      notify(result.message)
      setRestoreTarget(null)
      window.setTimeout(() => window.location.reload(), 1800)
    } catch (error) { notify(`خطا: ${error instanceof Error ? error.message : 'بازیابی بکاپ ناموفق بود'}`) }
    finally { setBusy('') }
  }

  const specs = [
    { icon: Server, label: 'نام سرور', value: system?.hostname || dashboard?.hostname || '—', dir: 'ltr' as const },
    { icon: ShieldCheck, label: 'سیستم‌عامل', value: system?.os || '—' },
    { icon: Cpu, label: 'پردازنده', value: system?.cpu && dashboard?.cpu_cores ? `${system.cpu} · ${formatNumber(dashboard.cpu_cores)} هسته` : system?.cpu || '—' },
    { icon: MemoryStick, label: 'حافظه RAM', value: dashboard ? formatBytes(dashboard.memory_kib * 1024) : '—' },
    { icon: HardDrive, label: 'دیسک ریشه', value: dashboard?.disk_total_bytes ? `${formatBytes(dashboard.disk_total_bytes - (dashboard.disk_free_bytes || 0))} / ${formatBytes(dashboard.disk_total_bytes)}` : '—' },
    { icon: Server, label: 'Kernel', value: system?.kernel || '—', dir: 'ltr' as const },
    { icon: Cpu, label: 'Architecture', value: system?.architecture || '—', dir: 'ltr' as const },
    { icon: Clock3, label: 'مدت روشن بودن سرور', value: dashboard ? <UptimeValue seconds={dashboard.uptime_seconds}/> : '—' },
  ]
  const actionTitles = { repair: 'بازسازی کامل ARG', reboot: 'راه‌اندازی مجدد سرور', uninstall: 'حذف کامل ARG' } as const
  const actionPhrases = { repair: 'REPAIR ARG', reboot: 'REBOOT SERVER', uninstall: 'DELETE ARG' } as const
  const actionDetails = {
    repair: 'فایل‌های ARG با سازوکار آپدیت امن بازسازی می‌شوند و در صورت شکست، نسخهٔ قبلی بازمی‌گردد.',
    reboot: 'اتصال پنل و SSH موقتاً قطع می‌شود و سرور دوباره راه‌اندازی خواهد شد.',
    uninstall: 'سرویس، فایل‌ها، تنظیمات و دیتابیس ARG برای همیشه پاک می‌شوند.',
  } as const

  return <>
    <Heading title="مدیریت سرور و ARG" description="مشخصات سرور، تعمیر امن پنل، راه‌اندازی مجدد و پشتیبان‌گیری از داده‌های ARG." action={<Button variant="outline" size="sm" onClick={() => void refresh()} disabled={loading}><RefreshCw size={15} className={loading ? 'animate-spin' : ''}/>تازه‌سازی</Button>}/>
    <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3" aria-label="مشخصات سیستم">
      {loading && !dashboard ? Array.from({ length: 8 }, (_, i) => <Skeleton key={i} className="h-[96px] rounded-2xl"/>) : specs.map(({ icon: Icon, label, value, dir }) => <Card key={label} className="min-w-0 rounded-2xl border-border/80 bg-card/80"><CardContent className="flex items-center gap-3 p-4"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl border border-primary/15 bg-primary/[.08] text-primary"><Icon size={18}/></span><span className="min-w-0"><span className="block text-xs text-muted-foreground">{label}</span><b className="mt-1 block text-sm font-semibold text-foreground" dir={dir || 'rtl'} title={typeof value === 'string' ? value : undefined}>{value}</b></span></CardContent></Card>)}
    </section>

    <Tabs defaultValue="overview" className="mt-5" dir="rtl">
      <TabsList className="h-auto w-full flex-wrap justify-start sm:w-auto" aria-label="بخش‌های مدیریت سرور و ARG">
        <TabsTrigger value="overview">سلامت و دسترسی</TabsTrigger>
        <TabsTrigger value="backups">بکاپ و بازیابی</TabsTrigger>
        <TabsTrigger value="maintenance">تعمیر و عملیات حساس</TabsTrigger>
      </TabsList>
      <TabsContent value="overview" className="space-y-5">
    <HostSecurityPanel/>
    <ExtensionSecurityPanel notify={notify}/>
    <DoctorPanel />
    <ApiKeysPanel />
      </TabsContent>

      <TabsContent value="maintenance" className="space-y-5">
    <div className="grid gap-4 xl:grid-cols-2">
      <Card className="rounded-2xl border-border/80 bg-card/80"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Wrench size={18} className="text-primary"/>تعمیر فایل‌های ARG</CardTitle><CardDescription>نسخهٔ نصب‌شده با آپدیتر امن و ساخت کامل بازسازی می‌شود؛ در صورت خطا، نسخهٔ قبلی برمی‌گردد.</CardDescription></CardHeader><CardContent className="flex flex-wrap items-center justify-between gap-3"><div className="text-xs text-muted-foreground">این عملیات ممکن است چند دقیقه طول بکشد و پنل موقتاً در دسترس نباشد.</div><Button onClick={() => void action('repair')} disabled={!!busy}><Wrench size={15}/>{busy === 'repair' ? 'در حال تعمیر…' : 'تعمیر و بازسازی ARG'}</Button></CardContent></Card>
      <Card className="rounded-2xl border-amber-400/20 bg-amber-400/[.035]"><CardHeader><CardTitle className="flex items-center gap-2 text-base"><RotateCcw size={18} className="text-amber-300"/>راه‌اندازی مجدد سرور</CardTitle><CardDescription>تمام سرویس‌ها برای مدتی قطع می‌شوند؛ پس از بالا آمدن سیستم، پنل دوباره در دسترس خواهد بود.</CardDescription></CardHeader><CardContent className="flex flex-wrap items-center justify-between gap-3"><div className="flex items-center gap-2 text-xs text-amber-100/70"><AlertTriangle size={15}/>اتصال SSH و پنل موقتاً قطع می‌شود</div><Button variant="outline" className="border-amber-300/30 text-amber-100 hover:bg-amber-400/10" onClick={() => void action('reboot')} disabled={!!busy}>{busy === 'reboot' ? 'در حال ارسال…' : 'ریبوت سرور'}</Button></CardContent></Card>
    </div>
<Card className="mt-5 rounded-2xl border-rose-400/20 bg-rose-400/[.025]"><CardHeader><CardTitle className="flex items-center gap-2 text-base text-rose-100"><Trash2 size={17}/>حذف کامل ARG</CardTitle><CardDescription>سرویس، فایل‌های نصب، تنظیمات و دیتابیس ARG پاک می‌شوند. این کار روی سایت‌ها و سرویس‌های دیگر سرور اثری ندارد.</CardDescription></CardHeader><CardContent className="flex flex-wrap items-center justify-between gap-3"><p className="text-xs text-muted-foreground">پیش از حذف، بکاپ بساز و آن را دانلود کن؛ بکاپ‌های محلی هم با حذف کامل پاک می‌شوند.</p><Button variant="destructive" onClick={() => void action('uninstall')} disabled={!!busy}><Trash2 size={15}/>{busy === 'uninstall' ? 'در حال حذف…' : 'حذف کامل ARG'}</Button></CardContent></Card>
      </TabsContent>

      <TabsContent value="backups" className="space-y-5">
    <Card className="mt-5 rounded-2xl border-border/80 bg-card/80"><CardHeader className="flex-row flex-wrap items-center justify-between gap-3"><div><CardTitle className="flex items-center gap-2 text-base"><Archive size={18} className="text-primary"/>بکاپ و بازیابی ARG</CardTitle><CardDescription className="mt-1.5">بکاپ دیتابیس ARG شامل حساب‌ها، تنظیمات پنل، تاریخچه و اطلاعات اتصالات است. فایل بکاپ را خارج از سرور هم نگه دار.</CardDescription></div><div className="flex flex-wrap gap-2"><input ref={backupUploadRef} type="file" accept=".db,application/vnd.sqlite3,application/x-sqlite3" className="hidden" aria-label="انتخاب فایل بکاپ ARG برای آپلود" onChange={(event) => void uploadBackup(event.target.files?.[0])}/><Button type="button" variant="outline" onClick={() => backupUploadRef.current?.click()} disabled={!!busy}><Upload size={15}/>{busy === 'upload' ? 'در حال آپلود و بررسی…' : 'آپلود بکاپ'}</Button><Button onClick={() => void createBackup()} disabled={!!busy}><Archive size={15}/>{busy === 'backup' ? 'در حال ساخت…' : 'ساخت بکاپ'}</Button></div></CardHeader><CardContent>
      <section className="mb-4 space-y-3 rounded-xl border border-border/70 bg-background/25 p-3" aria-labelledby="auto-backup-title">
        <div className="flex flex-wrap items-center justify-between gap-3"><div><h3 id="auto-backup-title" className="text-sm font-semibold">زمان‌بندی بکاپ خودکار</h3><p className="mt-1 text-[11px] leading-5 text-muted-foreground">ARG از دیتابیس خودش نسخهٔ معتبر می‌سازد و فقط نسخه‌های خودکار قدیمی‌تر از سقف نگه‌داری را پاک می‌کند. بکاپ‌های دستی و آپلودشده دست‌نخورده می‌مانند. فایل‌های سایت، گواهی TLS و خود سیستم‌عامل در این بکاپ نیستند.</p></div><label className="flex min-h-11 items-center gap-2 rounded-lg border border-border/70 px-3 text-xs"><input type="checkbox" checked={backupEnabled} onChange={(event) => setBackupEnabled(event.target.checked)} className="size-4 accent-[#dda46f]"/>فعال‌بودن بکاپ خودکار</label></div>
        <div className="grid gap-3 sm:grid-cols-2"><label className="space-y-1.5 text-[11px] text-muted-foreground"><span>فاصلهٔ ساخت بکاپ</span><select value={backupIntervalHours} onChange={(event) => setBackupIntervalHours(event.target.value)} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground"><option value="1">هر ۱ ساعت</option><option value="6">هر ۶ ساعت</option><option value="12">هر ۱۲ ساعت</option><option value="24">روزانه</option><option value="48">هر ۲ روز</option><option value="168">هفتگی</option><option value="720">هر ۳۰ روز</option></select></label><label className="space-y-1.5 text-[11px] text-muted-foreground"><span>تعداد بکاپ‌های خودکار نگه‌داری‌شده</span><select value={backupRetentionCount} onChange={(event) => setBackupRetentionCount(event.target.value)} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground">{[1, 3, 5, 7, 10, 14, 30, 50, 100].map((count) => <option key={count} value={count}>{count} نسخه</option>)}</select></label></div>
        {backupPolicyError && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/5 p-2 text-xs text-rose-200">{backupPolicyError}</p>}
        <div className="flex flex-wrap items-center justify-between gap-3"><div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-muted-foreground"><span>{backupPolicyLoading ? 'در حال خواندن زمان‌بندی…' : `${backupPolicy?.automatic_count ?? 0} بکاپ خودکار موجود`}</span><span>آخرین تلاش: {date(backupPolicy?.last_started_at ?? null)}</span><span>آخرین موفق: {date(backupPolicy?.last_success_at ?? null)}</span><span>اجرای بعدی: {backupPolicy?.enabled ? date(backupPolicy.next_run_at) : 'غیرفعال'}</span>{backupPolicy?.last_error && <span className="text-amber-200">آخرین خطا: {backupPolicy.last_error}</span>}</div><Button variant="outline" onClick={() => void saveBackupPolicy()} disabled={backupPolicySaving || backupPolicyLoading}>{backupPolicySaving ? <><RefreshCw size={14} className="animate-spin"/>در حال ذخیره…</> : 'ذخیرهٔ زمان‌بندی'}</Button></div>
      </section>
      <div className="mb-3 flex items-start gap-2 rounded-xl border border-sky-400/15 bg-sky-400/[.045] p-3 text-xs leading-6 text-muted-foreground"><CheckCircle2 size={16} className="mt-1 shrink-0 text-sky-300"/>بازیابی قبل از جایگزینی، یک بکاپ محافظتی از دیتابیس فعلی می‌سازد. پس از بازیابی صفحه تازه‌سازی می‌شود تا نشست‌ها و داده‌ها از نسخهٔ بازگردانده‌شده خوانده شوند.</div>
      <p className="mb-3 text-[11px] leading-5 text-muted-foreground">فایل‌های نصب و گواهی TLS در این نسخهٔ بکاپ قرار نمی‌گیرند.</p>
      {backups.length ? <div className="overflow-x-auto rounded-xl border border-border/70"><table className="w-full min-w-[520px] text-sm"><thead className="bg-secondary/50 text-xs text-muted-foreground"><tr><th className="px-4 py-3 text-right">نسخهٔ پشتیبان</th><th className="px-4 py-3 text-right">حجم</th><th className="px-4 py-3 text-left">عملیات</th></tr></thead><tbody>{backups.map((backup) => <tr key={backup.id} className="border-t border-border/70"><td className="px-4 py-3 font-mono text-xs" dir="ltr"><span>{backup.id}</span>{backup.automatic && <span className="mr-2 rounded-full border border-sky-300/20 px-2 py-0.5 font-sans text-[10px] text-sky-200">خودکار</span>}</td><td className="px-4 py-3 font-mono text-xs" dir="ltr">{formatBytes(backup.size_bytes)}</td><td className="px-4 py-3"><div className="flex justify-end gap-2"><Button size="sm" variant="outline" onClick={() => void downloadBackup(backup.id)} disabled={!!busy}><Download size={14}/><span className="hidden sm:inline">دانلود</span></Button><Button size="sm" variant="outline" onClick={() => restoreBackup(backup)} disabled={!!busy}><RotateCcw size={14}/><span className="hidden sm:inline">بازیابی</span></Button></div></td></tr>)}</tbody></table></div> : <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">بکاپی ثبت نشده؛ می‌توانی یک بکاپ تازه بسازی یا فایل `.db` قبلی را آپلود کنی.</div>}
    </CardContent></Card>

      </TabsContent>
    </Tabs>
    <Dialog open={!!restoreTarget} onOpenChange={(open) => { if (!open && !busy) { setRestoreTarget(null); setRestorePhrase(''); setRestoreStep(0) } }}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto">
        <DialogHeader><DialogTitle className="flex items-center gap-2"><AlertTriangle size={18} className="text-amber-300"/>بازیابی بکاپ ARG</DialogTitle><DialogDescription>دیتابیس فعلی با نسخهٔ بکاپ جایگزین می‌شود. ARG پیش از جایگزینی یک بکاپ محافظتی خودکار می‌سازد.</DialogDescription></DialogHeader>
        {restoreTarget && <div className="space-y-5">
          <UnifiedStepper currentStep={restoreStep} steps={[
            { id: 'review', title: 'بازبینی بکاپ', description: 'شناسه و اندازه را بررسی کن' },
            { id: 'confirm', title: 'تأیید بازیابی', description: 'عبارت تأیید را وارد کن' },
          ]}/>
          <div className="rounded-xl border border-border bg-background/40 p-3 text-xs"><span className="text-muted-foreground">فایل انتخاب‌شده</span><code dir="ltr" className="mt-1 block break-all font-mono">{restoreTarget.id}</code><span className="mt-2 block text-muted-foreground">حجم: {formatBytes(restoreTarget.size_bytes)}</span></div>
          {restoreStep === 0 ? <div className="space-y-4"><p className="text-sm leading-6 text-muted-foreground">این بکاپ جایگزین پایگاه‌دادهٔ فعال ARG می‌شود. پیش از جایگزینی، سیستم یک بکاپ محافظتی خودکار می‌سازد؛ بعد از موفقیت، صفحه دوباره بارگذاری می‌شود.</p><div className="flex flex-wrap justify-between gap-2"><Button variant="outline" disabled={!!busy} onClick={() => { setRestoreTarget(null); setRestorePhrase('') }}>انصراف</Button><Button disabled={!!busy} onClick={() => setRestoreStep(1)}>ادامه به تأیید<RotateCcw size={14}/></Button></div></div> : <div className="space-y-4"><div className="space-y-2"><Label htmlFor="restore-backup-confirmation">برای تأیید عبارت زیر را دقیق وارد کن</Label><code dir="ltr" className="block font-mono text-sm text-amber-200">RESTORE ARG</code><Input id="restore-backup-confirmation" dir="ltr" autoComplete="off" value={restorePhrase} onChange={(event) => setRestorePhrase(event.target.value)} placeholder="RESTORE ARG"/></div><div className="flex flex-wrap justify-between gap-2"><Button variant="outline" disabled={!!busy} onClick={() => setRestoreStep(0)}>بازگشت</Button><Button variant="destructive" disabled={!!busy || restorePhrase.trim() !== 'RESTORE ARG'} onClick={() => void confirmRestore()}>{busy === `restore:${restoreTarget.id}` ? <><RefreshCw size={14} className="animate-spin"/>در حال بازیابی…</> : <><RotateCcw size={14}/>جایگزینی پایگاه‌داده</>}</Button></div></div>}
        </div>}
      </DialogContent>
    </Dialog>
    <Dialog open={!!actionTarget} onOpenChange={(open) => { if (!open && !busy) { setActionTarget(null); setActionPhrase('') } }}>
      <DialogContent>
        <DialogHeader><DialogTitle className="flex items-center gap-2"><AlertTriangle size={18} className="text-amber-300"/>{actionTarget ? actionTitles[actionTarget] : 'تأیید عملیات'}</DialogTitle><DialogDescription>{actionTarget ? actionDetails[actionTarget] : ''}</DialogDescription></DialogHeader>
        {actionTarget && <div className="space-y-4"><div className="space-y-2"><Label htmlFor="server-action-confirmation">برای تأیید عبارت زیر را وارد کن</Label><code dir="ltr" className="block font-mono text-sm text-amber-200">{actionPhrases[actionTarget]}</code><Input id="server-action-confirmation" dir="ltr" autoComplete="off" value={actionPhrase} onChange={(event) => setActionPhrase(event.target.value)} placeholder={actionPhrases[actionTarget]}/></div><div className="flex justify-end gap-2"><Button variant="outline" disabled={!!busy} onClick={() => setActionTarget(null)}>انصراف</Button><Button variant={actionTarget === 'uninstall' ? 'destructive' : 'default'} disabled={!!busy || actionPhrase.trim() !== actionPhrases[actionTarget]} onClick={() => void confirmAction()}>{busy === actionTarget ? <><RefreshCw size={14} className="animate-spin"/>در حال اجرا…</> : 'تأیید و اجرا'}</Button></div></div>}
      </DialogContent>
    </Dialog>
  </>
}
