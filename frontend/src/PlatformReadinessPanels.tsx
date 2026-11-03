import { useCallback, useEffect, useState } from 'react'
import { Bot, Check, Clipboard, KeyRound, RefreshCw, ShieldAlert, ShieldCheck, Trash2, Wrench } from 'lucide-react'
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
type AiAgentToken = Omit<ApiKey, 'requests_this_minute'> & { requests_this_minute: number }
type AiAgentProposal = { id: string; action_type: string; application_id: string; application_name: string; reason: string; state: 'pending' | 'approving' | 'approved' | 'rejected' | 'expired' | 'failed'; created_at: number; expires_at: number; reviewed_at: number | null; reviewed_by: string | null; result?: string | null; error?: string | null; agent_label: string | null; requested_by: string; deployment?: { source?: { kind?: string; repository?: string; reference?: string; private_key_configured?: boolean; path?: string }; pre_deploy_command_count?: number; healthcheck_file?: string | null; retention_count?: number; error?: string } }
type NewAgentToken = { id: number; label: string; token: string; prefix: string; scopes: string[]; created_at: number; expires_at: number; notice: string }

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

const agentScopeLabels: Record<string, string> = {
  'apps:read': 'خواندن فهرست اپلیکیشن‌ها',
  'deployments:read': 'خواندن خلاصهٔ استقرارها',
  'docs:read': 'خواندن مستندات فارسی و انگلیسی',
  'repository:read': 'خواندن snapshot کد Rust و رابط ARG همراه همین نسخه',
  'deployments:propose': 'پیشنهاد استقرار؛ اجرای آن نیازمند تأیید مدیر است',
}

export function AiAgentPanel() {
  const [tokens, setTokens] = useState<AiAgentToken[]>([])
  const [proposals, setProposals] = useState<AiAgentProposal[]>([])
  const [label, setLabel] = useState('ARG AI Agent')
  const [days, setDays] = useState('7')
  const [scopes, setScopes] = useState<string[]>(['apps:read', 'deployments:read', 'docs:read'])
  const [created, setCreated] = useState<NewAgentToken | null>(null)
  const [approveId, setApproveId] = useState<string | null>(null)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)
  const refresh = useCallback(async () => {
    try {
      const [tokenResult, proposalResult] = await Promise.all([
        api<{ tokens: AiAgentToken[] }>('admin/ai-agents'),
        api<{ proposals: AiAgentProposal[] }>('admin/ai-agent-proposals'),
      ])
      setTokens(tokenResult.tokens)
      setProposals(proposalResult.proposals)
      setError('')
    } catch (value) { setError(value instanceof Error ? value.message : 'وضعیت اتصال عامل دریافت نشد') }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  const toggleScope = (scope: string) => setScopes((current) => current.includes(scope) ? current.filter((value) => value !== scope) : [...current, scope])
  async function create() {
    const expiry = Number(days)
    if (!label.trim() || !Number.isInteger(expiry) || expiry < 1 || expiry > 30 || scopes.length === 0) { setError('نام، حداقل یک مجوز و اعتبار ۱ تا ۳۰ روز انتخاب کن.'); return }
    setBusy('create'); setError('')
    try { const value = await api<NewAgentToken>('admin/ai-agents', { method: 'POST', body: JSON.stringify({ label: label.trim(), expires_days: expiry, scopes }) }); setCreated(value); await refresh() }
    catch (value) { setError(value instanceof Error ? value.message : 'ساخت اتصال عامل ناموفق بود') }
    finally { setBusy(''); }
  }
  async function revoke(id: number) {
    setBusy(`token-${id}`); setError('')
    try { await api(`admin/ai-agents/${id}`, { method: 'DELETE' }); await refresh() }
    catch (value) { setError(value instanceof Error ? value.message : 'لغو اتصال ناموفق بود') }
    finally { setBusy('') }
  }
  async function decideProposal(id: string, approve: boolean) {
    setBusy(`proposal-${id}`); setError('')
    try {
      if (approve) await api(`admin/ai-agent-proposals/${encodeURIComponent(id)}/approve`, { method: 'POST', body: '{}' })
      else await api(`admin/ai-agent-proposals/${encodeURIComponent(id)}/reject`, { method: 'POST', body: '{}' })
      setApproveId(null); await refresh()
    } catch (value) { setError(value instanceof Error ? value.message : 'بررسی درخواست ناموفق بود'); await refresh() }
    finally { setBusy('') }
  }
  async function copyToken() {
    if (!created) return
    try { await navigator.clipboard.writeText(created.token); setCopied(true) }
    catch { setError('دسترسی Clipboard ممکن نیست؛ توکن را دستی کپی کن.') }
  }
  const mcpUrl = `${window.location.origin}/api/agent/mcp`
  const pending = proposals.filter((proposal) => proposal.state === 'pending')
  const selectedProposal = proposals.find((proposal) => proposal.id === approveId)
  const stateLabel: Record<AiAgentProposal['state'], string> = { pending: 'در انتظار تأیید', approving: 'در حال اجرا', approved: 'تأیید و در صف', rejected: 'ردشده', expired: 'منقضی', failed: 'ناموفق' }
  return <Card className="mt-5 rounded-2xl border-primary/25 bg-card/90">
    <CardHeader><CardTitle className="flex items-center gap-2 text-base"><Bot size={18} className="text-primary"/>اتصال عامل هوش مصنوعی (MCP)</CardTitle><CardDescription>اتصال عامل به ARG برای خواندن وضعیت و مستندات؛ عملیات تغییردهنده باید به‌صورت درخواست ثبت و توسط مدیر تأیید شود.</CardDescription></CardHeader>
    <CardContent className="space-y-5">
      <div className="rounded-xl border border-emerald-400/20 bg-emerald-400/[.035] p-3 text-xs leading-6 text-muted-foreground"><p className="font-medium text-foreground">محدودیت‌های امنیتی فعال</p><ul className="mt-1 list-inside list-disc"><li>توکن مستقل، هش‌شده در دیتابیس، یک‌بارنمایش، حداکثر ۳۰ روز و ۳۰ درخواست در دقیقه است.</li><li>مجوزها به‌صورت جداگانه انتخاب می‌شوند؛ مالک توکن باید مدیر فعال باشد.</li><li>عامل فقط می‌تواند استقرار با تنظیمات ذخیره‌شده را پیشنهاد کند؛ تأیید در این پنل Job واقعی می‌سازد.</li><li>هیچ ابزار shell، root، خواندن Secret یا پراکسی API عمومی ارائه نمی‌شود.</li><li>مستندات و snapshot کد Rust/React همین نسخه به‌صورت resource خواندنی ارائه می‌شوند؛ ARG به GitHub یا URL دلخواه درخواست شبکه‌ای نمی‌فرستد.</li></ul></div>
      {error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/5 p-3 text-sm text-rose-200">{error}</p>}
      <section className="space-y-3 rounded-xl border border-border/70 bg-background/25 p-3" aria-labelledby="ai-agent-create-title">
        <div><h3 id="ai-agent-create-title" className="text-sm font-semibold">ساخت اتصال جدید</h3><p className="mt-1 text-[11px] leading-5 text-muted-foreground">توکن را برای یک عامل و یک نشست کاری جدا بساز؛ توکن مشترک بین چند عامل، دامنهٔ افشا را بزرگ می‌کند.</p></div>
        <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_130px_auto]"><Input value={label} onChange={(event) => setLabel(event.target.value)} maxLength={80} placeholder="نام اتصال" aria-label="نام اتصال عامل"/><label className="flex items-center gap-2"><span className="sr-only">روز اعتبار</span><Input value={days} onChange={(event) => setDays(event.target.value)} type="number" min={1} max={30} aria-label="روز اعتبار عامل"/><span className="shrink-0 text-xs text-muted-foreground">روز</span></label><Button onClick={() => void create()} disabled={!!busy}><KeyRound size={15}/>{busy === 'create' ? 'در حال ساخت…' : 'ساخت توکن'}</Button></div>
        <fieldset className="grid gap-2 sm:grid-cols-2"><legend className="mb-2 text-xs font-medium">مجوزهای این توکن</legend>{Object.entries(agentScopeLabels).map(([scope, description]) => <label key={scope} className="flex min-h-10 items-start gap-2 rounded-lg border border-border/60 p-2 text-xs"><input type="checkbox" checked={scopes.includes(scope)} onChange={() => toggleScope(scope)} className="mt-0.5 size-4 accent-[#dda46f]"/><span><span className="block font-medium">{description}</span><code dir="ltr" className="mt-1 block text-[10px] text-muted-foreground">{scope}</code></span></label>)}</fieldset>
      </section>
      <section className="space-y-2" aria-labelledby="ai-agent-connect-title"><h3 id="ai-agent-connect-title" className="text-sm font-semibold">پیکربندی کلاینت MCP</h3><p className="text-xs leading-5 text-muted-foreground">این نشانی را در کلاینت MCP سازگار وارد کن. توکن را در بخش Secret / Environment کلاینت ذخیره کن، نه داخل فایل پروژه یا prompt.</p><code dir="ltr" className="block select-all break-all rounded-lg border border-border/70 bg-background/55 p-3 text-xs">{mcpUrl}</code><pre dir="ltr" className="overflow-x-auto rounded-lg border border-border/70 bg-background/45 p-3 text-[11px] leading-5 text-muted-foreground">{JSON.stringify({ mcpServers: { arg: { url: mcpUrl, headers: { Authorization: 'Bearer ${ARG_AGENT_TOKEN}' } } } }, null, 2)}</pre></section>
      {tokens.length > 0 ? <div className="overflow-x-auto rounded-xl border border-border/70"><table className="w-full min-w-[760px] text-sm"><thead className="bg-secondary/40 text-xs text-muted-foreground"><tr><th className="px-3 py-2.5 text-right">اتصال</th><th className="px-3 py-2.5 text-right">مجوزها</th><th className="px-3 py-2.5 text-right">انقضا</th><th className="px-3 py-2.5 text-right">آخرین استفاده</th><th className="px-3 py-2.5 text-right">درخواست/دقیقه</th><th className="px-3 py-2.5"/></tr></thead><tbody>{tokens.map((item) => <tr key={item.id} className="border-t border-border/70"><td className="px-3 py-3"><b>{item.label}</b><code dir="ltr" className="mt-1 block text-[11px] text-muted-foreground">{item.prefix}… · {item.username}{item.revoked_at ? ' · لغوشده' : ''}</code></td><td dir="ltr" className="px-3 py-3 text-xs">{item.scopes.join(', ')}</td><td className="px-3 py-3 text-xs">{date(item.expires_at)}</td><td className="px-3 py-3 text-xs">{date(item.last_used_at)}</td><td dir="ltr" className="px-3 py-3 text-xs">{item.requests_this_minute} / 30</td><td className="px-3 py-3 text-left">{!item.revoked_at && <Button variant="ghost" size="icon" onClick={() => void revoke(item.id)} disabled={!!busy} aria-label={`لغو اتصال ${item.label}`}><Trash2 size={15} className="text-rose-300"/></Button>}</td></tr>)}</tbody></table></div> : <div className="rounded-xl border border-dashed border-border p-4 text-center text-sm text-muted-foreground">اتصال فعالی ثبت نشده است.</div>}
      <section className="space-y-3 border-t border-border/70 pt-4" aria-labelledby="ai-agent-proposals-title"><div className="flex flex-wrap items-center justify-between gap-2"><div><h3 id="ai-agent-proposals-title" className="text-sm font-semibold">درخواست‌های عامل</h3><p className="mt-1 text-[11px] text-muted-foreground">درخواست تأییدشده، منبع پیش‌فرض و دستورهای ازقبل‌ذخیره‌شدهٔ همان اپ را اجرا می‌کند.</p></div><Button variant="outline" size="sm" onClick={() => void refresh()} disabled={!!busy}><RefreshCw size={14}/>تازه‌سازی</Button></div>
        {pending.length === 0 ? <p className="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">درخواست معلقی وجود ندارد.</p> : <div className="space-y-2">{pending.map((proposal) => <article key={proposal.id} className="rounded-xl border border-amber-300/20 bg-amber-300/[.035] p-3"><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><p className="font-medium">استقرار {proposal.application_name}</p><p className="mt-1 text-xs text-muted-foreground">درخواست از {proposal.agent_label || 'اتصال لغوشده'} · {proposal.requested_by} · {date(proposal.created_at)}</p>{proposal.reason && <p className="mt-2 break-words text-xs text-foreground/80">دلیل: {proposal.reason}</p>}<div className="mt-2 space-y-1 text-[11px] text-muted-foreground"><span className="block">منبع ثابت‌شده هنگام درخواست:</span>{proposal.deployment?.source?.kind === 'git' ? <code dir="ltr" className="block break-all text-left text-foreground/80">{proposal.deployment.source.repository} · {proposal.deployment.source.reference}{proposal.deployment.source.private_key_configured ? ' · Deploy Key' : ''}</code> : proposal.deployment?.source?.kind === 'directory' ? <code dir="ltr" className="block break-all text-left text-foreground/80">{proposal.deployment.source.path}</code> : <span>منبع در snapshot موجود نیست</span>}<span className="block">دستور پیش از استقرار: {proposal.deployment?.pre_deploy_command_count ?? 0} · حفظ release: {proposal.deployment?.retention_count ?? '—'} · Health Check: {proposal.deployment?.healthcheck_file || 'تنظیم نشده'}</span></div><p className="mt-2 text-[11px] text-amber-100/80">این اقدام ممکن است کد پروژه و دستورهای ذخیره‌شده را اجرا کند. منبع هنگام ثبت پیشنهاد snapshot شده؛ تنظیمات بعدی آن را عوض نمی‌کند.</p></div><div className="flex shrink-0 gap-2"><Button variant="outline" size="sm" onClick={() => void decideProposal(proposal.id, false)} disabled={!!busy}>رد</Button><Button size="sm" onClick={() => setApproveId(proposal.id)} disabled={!!busy || !!proposal.deployment?.error}>بررسی و تأیید</Button></div></div></article>)}</div>}
        {proposals.some((proposal) => proposal.state !== 'pending') && <details className="rounded-lg border border-border/60 p-3"><summary className="cursor-pointer text-xs font-medium">تاریخچهٔ درخواست‌ها</summary><div className="mt-3 max-h-64 space-y-2 overflow-y-auto">{proposals.filter((proposal) => proposal.state !== 'pending').map((proposal) => <div key={proposal.id} className="flex flex-wrap justify-between gap-2 border-t border-border/50 pt-2 text-xs"><span>{proposal.application_name} · {proposal.agent_label || 'اتصال حذف‌شده'}</span><span>{stateLabel[proposal.state]} · {date(proposal.created_at)}</span>{proposal.error && <span role="alert" className="w-full break-words text-rose-200">{proposal.error}</span>}</div>)}</div></details>}
      </section>
    </CardContent>
    <Dialog open={!!created} onOpenChange={(open) => { if (!open) { setCreated(null); setCopied(false) } }}><DialogContent><DialogHeader><DialogTitle>توکن عامل — نمایش یک‌باره</DialogTitle><DialogDescription>توکن خام فقط اکنون در دسترس است. بعد از بستن پنجره قابل بازیابی نیست؛ اگر افشا شد، فوراً اتصال را لغو کن.</DialogDescription></DialogHeader>{created && <div className="space-y-3"><code dir="ltr" className="block max-h-32 overflow-auto break-all rounded-lg border border-border bg-background p-3 text-xs">{created.token}</code><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={() => void copyToken()}>{copied ? <Check size={15}/> : <Clipboard size={15}/>} {copied ? 'کپی شد' : 'کپی توکن'}</Button><Button onClick={() => { setCreated(null); setCopied(false) }}>توکن را امن ذخیره کردم</Button></div></div>}</DialogContent></Dialog>
    <Dialog open={!!approveId} onOpenChange={(open) => { if (!open && !busy) setApproveId(null) }}><DialogContent><DialogHeader><DialogTitle>تأیید استقرار در همین VPS؟</DialogTitle><DialogDescription>درخواست برای {selectedProposal?.application_name} است. منبع و تنظیمات نمایش‌داده‌شده هنگام ثبت snapshot شده‌اند. این کار یک Job واقعی می‌سازد و می‌تواند کد پروژه و دستورهای ذخیره‌شده را اجرا کند؛ بررسی و صف عادی ARG همچنان اعمال می‌شود.</DialogDescription></DialogHeader>{selectedProposal?.deployment?.source?.kind === 'git' && <code dir="ltr" className="block break-all rounded-lg border border-border bg-background p-3 text-xs">{selectedProposal.deployment.source.repository} · {selectedProposal.deployment.source.reference}</code>}<div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setApproveId(null)} disabled={!!busy}>انصراف</Button><Button onClick={() => approveId && void decideProposal(approveId, true)} disabled={!!busy || !!selectedProposal?.deployment?.error}>{busy.startsWith('proposal-') ? 'در حال ثبت…' : 'تأیید و افزودن به صف'}</Button></div></DialogContent></Dialog>
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
