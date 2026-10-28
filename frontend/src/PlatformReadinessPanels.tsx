import { useCallback, useEffect, useState } from 'react'
import { Check, Clipboard, KeyRound, RefreshCw, ShieldAlert, ShieldCheck, Trash2, Wrench } from 'lucide-react'
import { ApiError, api } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Input } from './components/ui/input'

type DoctorNodeItem = { name: string; endpoint?: string; state?: string; compatibility?: string; last_seen: number | null; last_check?: number | null; last_error?: string | null }
type DoctorCheck = { id: string; label: string; status: 'pass' | 'warn' | 'info'; value?: string | number | string[] | null; available_bytes?: number | null; detail: string; total?: number; connected?: number; connections?: number; buckets?: number; mounted?: number; applications?: number; deployments?: number; items?: DoctorNodeItem[] }
type DoctorResult = { read_only: true; generated_at: number; summary: { checks: number; warnings: number }; checks: DoctorCheck[] }
type ApiKey = { id: number; username: string; label: string; prefix: string; scopes: string[]; created_at: number; expires_at: number; revoked_at: number | null; last_used_at: number | null; requests_this_minute: number }
type NewApiKey = { id: number; label: string; token: string; prefix: string; scopes: string[]; created_at: number; expires_at: number; notice: string }

const date = (value: number | null) => value ? new Date(value * 1000).toLocaleString('fa-IR') : '—'
const checkText = (check: DoctorCheck) => {
  if (Array.isArray(check.value)) return check.value.length ? check.value.join('، ') : 'چیزی پیدا نشد'
  if (check.available_bytes != null) return `${(check.available_bytes / 1024 ** 3).toFixed(1)} GiB آزاد`
  if (check.id === 's3') return `${check.connected ?? 0} از ${check.connections ?? 0} اتصال · ${check.buckets ?? 0} باکت · ${check.mounted ?? 0} Mount`
  if (check.id === 'nodes' || check.id === 'controllers') return `${check.connected ?? 0} از ${check.total ?? 0} ${check.id === 'nodes' ? 'Node متصل' : 'پنل مجاز'}`
  if (check.id === 'apps') return `${check.applications ?? 0} اپلیکیشن · ${check.deployments ?? 0} استقرار`
  return check.value == null ? 'بررسی شد' : String(check.value)
}
const nodeStateLabel: Record<string, string> = { healthy: 'سالم', connected: 'متصل', degraded: 'نیازمند بررسی', offline: 'قطع', untrusted: 'نامعتبر', incompatible: 'ناسازگار' }
const compatibilityLabel: Record<string, string> = { compatible: 'سازگار', 'legacy-read-only': 'قدیمی · فقط‌خواندنی', incompatible: 'ناسازگار' }

export function DoctorPanel() {
  const [result, setResult] = useState<DoctorResult | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [apiUnavailable, setApiUnavailable] = useState(false)
  const run = useCallback(async () => {
    setBusy(true); setError('')
    try {
      setResult(await api<DoctorResult>('maintenance/doctor'))
      setApiUnavailable(false)
    }
    catch (value) {
      if (value instanceof ApiError && value.status === 404) {
        setResult(null)
        setApiUnavailable(true)
      } else {
        setApiUnavailable(false)
        setError(value instanceof Error ? value.message : 'گزارش Doctor دریافت نشد')
      }
    }
    finally { setBusy(false) }
  }, [])
  useEffect(() => { void run() }, [run])
  return <Card className="mt-5 rounded-2xl border-border/80 bg-card/80">
    <CardHeader className="flex-row flex-wrap items-center justify-between gap-3"><div><CardTitle className="flex items-center gap-2 text-base"><Wrench size={17} className="text-primary"/>ARG Doctor</CardTitle><CardDescription className="mt-1.5">گزارش خواندنی از سازگاری و اتصال‌های ثبت‌شده؛ چیزی نصب یا تغییر داده نمی‌شود.</CardDescription></div><Button variant="outline" size="sm" onClick={() => void run()} disabled={busy}><RefreshCw size={14} className={busy ? 'animate-spin' : ''}/>بررسی دوباره</Button></CardHeader>
    <CardContent>
      {apiUnavailable && <div role="status" className="mb-3 rounded-xl border border-amber-300/20 bg-amber-300/[.04] p-4 text-xs leading-6 text-amber-100/90"><p className="font-medium">API گزارش ARG Doctor در این سرور موجود نیست</p><p className="mt-1 text-amber-100/75">درخواست با HTTP 404 پاسخ گرفت؛ نسخهٔ ARG روی این VPS یا Node انتخاب‌شده احتمالاً قدیمی است یا هنوز این قابلیت را ندارد. ARG همان سرور را به نسخه‌ای که گزارش Doctor را پشتیبانی می‌کند به‌روزرسانی کن و دوباره بررسی کن.</p><Button className="mt-3" variant="outline" size="sm" onClick={() => void run()} disabled={busy}>{busy ? 'در حال بررسی…' : 'تلاش دوباره'}</Button></div>}
      {error && <p role="alert" className="mb-3 rounded-lg border border-rose-400/20 bg-rose-400/5 p-3 text-sm text-rose-200">{error}</p>}
      {busy && !result ? <div className="grid gap-2 sm:grid-cols-2">{Array.from({ length: 6 }, (_, i) => <div key={i} className="h-20 animate-pulse rounded-xl border border-border/70 bg-secondary/30"/>)}</div> : result && <>
        <div className="mb-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground"><span>{result.summary.checks} بررسی</span><span>·</span><span>{result.summary.warnings} مورد نیازمند توجه</span><span>·</span><span>آخرین بررسی: {date(result.generated_at)}</span><span className="rounded-full border border-emerald-400/20 bg-emerald-400/5 px-2 py-0.5 text-emerald-200">فقط خواندنی</span></div>
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{result.checks.map((check) => {
          const Icon = check.status === 'pass' ? ShieldCheck : check.status === 'warn' ? ShieldAlert : Wrench
          const color = check.status === 'pass' ? 'text-emerald-300' : check.status === 'warn' ? 'text-amber-200' : 'text-muted-foreground'
          return <div key={check.id} className="min-w-0 rounded-xl border border-border/70 bg-background/35 p-3"><div className="flex items-center gap-2"><Icon size={15} className={color}/><b className="min-w-0 flex-1 truncate text-sm">{check.label}</b></div><div className="mt-2 break-words text-xs font-medium text-foreground">{checkText(check)}</div><p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">{check.detail}</p>{check.items && check.items.length > 0 && <div className="mt-2 max-h-48 space-y-1.5 overflow-y-auto border-t border-border/60 pt-2">{check.items.map((item, index) => <div key={`${item.name}-${item.endpoint || index}`} className="rounded-lg border border-border/60 bg-background/35 p-2 text-[10px]"><div className="flex flex-wrap items-center justify-between gap-1"><b className="min-w-0 truncate">{item.name}</b>{item.state && <span className={item.state === 'healthy' || item.state === 'connected' ? 'text-emerald-200' : 'text-amber-200'}>{nodeStateLabel[item.state] || item.state}</span>}</div>{item.endpoint && <code dir="ltr" className="mt-1 block truncate text-left text-muted-foreground">{item.endpoint}</code>}{item.compatibility && <p className="mt-1 text-muted-foreground">سازگاری: {compatibilityLabel[item.compatibility] || item.compatibility}</p>}{item.last_seen != null && <p className="mt-1 text-muted-foreground">آخرین ارتباط موفق: {date(item.last_seen)}</p>}{item.last_check != null && <p className="mt-1 text-muted-foreground">آخرین بررسی: {date(item.last_check)}</p>}{item.last_error && <p className="mt-1 break-words text-amber-100/80">{item.last_error}</p>}</div>)}</div>}</div>
        })}</div>
      </>}
    </CardContent>
  </Card>
}

export function ApiKeysPanel() {
  const [keys, setKeys] = useState<ApiKey[]>([])
  const [label, setLabel] = useState('')
  const [days, setDays] = useState('90')
  const [created, setCreated] = useState<NewApiKey | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)
  const refresh = useCallback(async () => {
    try { const value = await api<{ keys: ApiKey[] }>('admin/api-keys'); setKeys(value.keys) }
    catch (value) { setError(value instanceof Error ? value.message : 'کلیدهای API دریافت نشد') }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  async function create() {
    const expiry = Number(days)
    if (!label.trim() || !Number.isInteger(expiry) || expiry < 1 || expiry > 365) { setError('نام لازم است و اعتبار باید بین ۱ تا ۳۶۵ روز باشد.'); return }
    setBusy(true); setError('')
    try { const value = await api<NewApiKey>('admin/api-keys', { method: 'POST', body: JSON.stringify({ label: label.trim(), expires_days: expiry }) }); setCreated(value); setLabel(''); await refresh() }
    catch (value) { setError(value instanceof Error ? value.message : 'ساخت کلید ناموفق بود') }
    finally { setBusy(false) }
  }
  async function revoke(id: number) {
    setBusy(true); setError('')
    try { await api(`admin/api-keys/${id}`, { method: 'DELETE' }); await refresh() }
    catch (value) { setError(value instanceof Error ? value.message : 'لغو کلید ناموفق بود') }
    finally { setBusy(false) }
  }
  async function copyToken() {
    if (!created) return
    try { await navigator.clipboard.writeText(created.token); setCopied(true) }
    catch { setError('دسترسی Clipboard ممکن نیست؛ توکن را دستی کپی کن.') }
  }
  return <Card className="mt-5 rounded-2xl border-border/80 bg-card/80">
    <CardHeader><CardTitle className="flex items-center gap-2 text-base"><KeyRound size={17} className="text-primary"/>کلیدهای API عمومی</CardTitle><CardDescription>نسخهٔ اول فقط دسترسی خواندن اپلیکیشن‌ها و استقرارها را می‌دهد؛ هر کلید فقط یک‌بار نمایش داده می‌شود و حداکثر ۶۰ درخواست در دقیقه دارد.</CardDescription></CardHeader>
    <CardContent className="space-y-4">
      {error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/5 p-3 text-sm text-rose-200">{error}</p>}
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_150px_auto]">
        <Input value={label} onChange={(event) => setLabel(event.target.value)} maxLength={80} placeholder="نام کلید، مثلاً ابزار گزارش‌گیری" aria-label="نام کلید API"/>
        <Input value={days} onChange={(event) => setDays(event.target.value)} type="number" min={1} max={365} aria-label="روز اعتبار"/>
        <Button onClick={() => void create()} disabled={busy}><KeyRound size={15}/>{busy ? 'در حال ساخت…' : 'ساخت کلید'}</Button>
      </div>
      <p className="text-[11px] leading-5 text-muted-foreground">Scope ثابت: <code dir="ltr">apps:read</code> و <code dir="ltr">deployments:read</code>. عملیات تغییردهنده و Webhook در این API فعال نیست. صدور و استفاده از توکن فقط روی HTTPS مجاز است؛ سند OpenAPI در <code dir="ltr">/api/v1/openapi.json</code> قرار دارد.</p>
      <pre dir="ltr" className="overflow-x-auto rounded-lg border border-border/70 bg-background/45 p-3 text-[11px] leading-5 text-muted-foreground">curl -H 'Authorization: Bearer &lt;TOKEN&gt;' 'https://your-arg-host/api/v1/apps?limit=50'</pre>
      {keys.length ? <div className="overflow-x-auto rounded-xl border border-border/70"><table className="w-full min-w-[760px] text-sm"><thead className="bg-secondary/40 text-xs text-muted-foreground"><tr><th className="px-3 py-2.5 text-right">کلید</th><th className="px-3 py-2.5 text-right">Scope</th><th className="px-3 py-2.5 text-right">انقضا</th><th className="px-3 py-2.5 text-right">آخرین استفاده</th><th className="px-3 py-2.5 text-right">درخواست/دقیقه</th><th className="px-3 py-2.5"/></tr></thead><tbody>{keys.map((item) => <tr key={item.id} className="border-t border-border/70"><td className="px-3 py-3"><b>{item.label}</b><code dir="ltr" className="mt-1 block text-[11px] text-muted-foreground">{item.prefix}… · {item.username}{item.revoked_at ? ' · لغوشده' : ''}</code></td><td dir="ltr" className="px-3 py-3 text-xs">{item.scopes.join(', ')}</td><td className="px-3 py-3 text-xs">{date(item.expires_at)}</td><td className="px-3 py-3 text-xs">{date(item.last_used_at)}</td><td dir="ltr" className="px-3 py-3 text-xs">{item.requests_this_minute} / 60</td><td className="px-3 py-3 text-left">{!item.revoked_at && <Button variant="ghost" size="icon" onClick={() => void revoke(item.id)} disabled={busy} aria-label={`لغو کلید ${item.label}`}><Trash2 size={15} className="text-rose-300"/></Button>}</td></tr>)}</tbody></table></div> : <div className="rounded-xl border border-dashed border-border p-5 text-center text-sm text-muted-foreground">کلیدی ثبت نشده است.</div>}
    </CardContent>
    <Dialog open={!!created} onOpenChange={(open) => { if (!open) { setCreated(null); setCopied(false) } }}>
      <DialogContent><DialogHeader><DialogTitle>توکن API — نمایش یک‌باره</DialogTitle><DialogDescription>توکن خام فقط اکنون در دسترس است. آن را در Secret Manager نگه‌دار؛ با بستن این پنجره دیگر قابل بازیابی نیست.</DialogDescription></DialogHeader>
        {created && <div className="space-y-3"><code dir="ltr" className="block max-h-32 overflow-auto break-all rounded-lg border border-border bg-background p-3 text-xs">{created.token}</code><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={() => void copyToken()}>{copied ? <Check size={15} /> : <Clipboard size={15} />}{copied ? 'کپی شد' : 'کپی توکن'}</Button><Button onClick={() => { setCreated(null); setCopied(false) }}>ذخیره کردم</Button></div></div>}
      </DialogContent>
    </Dialog>
  </Card>
}
