import { useEffect, useMemo, useState } from 'react'
import { AlertTriangle, CheckCircle2, Globe2, LoaderCircle, RefreshCw, ShieldCheck } from 'lucide-react'
import { api } from '../lib/api'
import { Alert, AlertDescription } from '../components/ui/alert'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { Input } from '../components/ui/input'
import { Label } from '../components/ui/label'
import { Switch } from '../components/ui/switch'
import { UnifiedStepper } from '../components/ui/stepper'

type DomainResult = { host: string; resolved: boolean; addresses: string[]; error?: string }
type Props = {
  domains: string
  onDomainsChange?: (value: string) => void
  email: string
  onEmailChange: (value: string) => void
  redirect: boolean
  onRedirectChange: (value: boolean) => void
  busy: boolean
  onIssue: () => void
  bare?: boolean
}

const normalizeDomains = (value: string) => [...new Set(value.split(/[\s,]+/).map((item) => item.trim().toLowerCase()).filter(Boolean))]
const validHost = (host: string) => host.length <= 253 && host.split('.').length >= 2 && host.split('.').every((label) => label.length > 0 && label.length <= 63 && /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(label))

/** DNS preflight + explicit review around the existing, real site SSL operation. */
export function SslIssueWizard({ domains, onDomainsChange, email, onEmailChange, redirect, onRedirectChange, busy, onIssue, bare = false }: Props) {
  const [step, setStep] = useState(0)
  const [checking, setChecking] = useState(false)
  const [results, setResults] = useState<DomainResult[]>([])
  const [checkedKey, setCheckedKey] = useState('')
  const [error, setError] = useState('')
  const [acknowledged, setAcknowledged] = useState(false)
  const hosts = useMemo(() => normalizeDomains(domains), [domains])
  const key = hosts.join(',')
  const allValid = hosts.length > 0 && hosts.every(validHost)
  const emailValid = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email.trim())
  const current = checkedKey === key && key.length > 0
  const allResolved = current && results.length === hosts.length && results.every((item) => item.resolved)

  useEffect(() => {
    if (checkedKey && checkedKey !== key) { setStep(0); setResults([]); setCheckedKey(''); setAcknowledged(false) }
  }, [checkedKey, key])

  async function checkDomains() {
    if (!allValid || !emailValid) return
    setChecking(true); setError(''); setAcknowledged(false)
    try {
      const response = await api<{ domains: DomainResult[] }>(`nginx/domains/check?hosts=${encodeURIComponent(hosts.join(','))}`)
      setResults(response.domains); setCheckedKey(key); setStep(1)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'بررسی DNS دامنه‌ها ناموفق بود.') }
    finally { setChecking(false) }
  }

  return <div className={bare ? 'space-y-4' : 'space-y-4 rounded-xl border border-emerald-500/20 bg-emerald-500/[.035] p-4'}>
    {!bare && <div><h3 className="text-sm font-semibold text-foreground">دریافت و نصب خودکار گواهی</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">ابتدا دامنه و DNS بررسی می‌شوند؛ سپس تنظیمات نهایی را مرور می‌کنی. در اجرا، ARG سایت را با HTTP می‌سازد و Certbot گواهی را می‌گیرد و HTTPS را روی همان سایت فعال می‌کند.</p></div>}
    <UnifiedStepper currentStep={step} steps={[
      { id: 'preflight', title: 'پیش‌نیازها', description: 'دامنه، ایمیل و DNS' },
      { id: 'review', title: 'بازبینی و اجرا', description: 'تأیید تنظیمات صدور' },
    ]}/>
    {step === 0 ? <>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5"><Label htmlFor="nginx-builder-ssl-email">ایمیل اعلان تمدید</Label><Input id="nginx-builder-ssl-email" type="email" dir="ltr" value={email} onChange={(event) => onEmailChange(event.target.value)} placeholder="admin@example.com" autoComplete="email"/></div>
        <label className="flex items-center gap-2 self-end rounded-lg border border-border p-3 text-xs text-muted-foreground"><Switch checked={redirect} onCheckedChange={onRedirectChange}/>انتقال HTTP به HTTPS</label>
      </div>
      <div className="space-y-2 rounded-lg border border-border bg-background/40 p-3 text-xs"><Label htmlFor="ssl-wizard-domains" className="flex items-center gap-2"><Globe2 size={14} className="text-primary"/>دامنه‌های این سایت</Label>{onDomainsChange ? <Input id="ssl-wizard-domains" dir="ltr" value={domains} onChange={(event) => onDomainsChange(event.target.value)} placeholder="example.com www.example.com"/> : <code dir="ltr" className="block break-all text-left font-mono text-muted-foreground">{hosts.join(', ') || 'ابتدا دامنه‌های سایت را در فرم بالا وارد کن.'}</code>}<p className="text-[10px] leading-5 text-muted-foreground">فقط دامنه‌های موجود در server_name همین سایت را انتخاب کن؛ با فاصله یا کاما جدا کن.</p></div>
      <p className="text-[11px] leading-5 text-muted-foreground">بررسی DNS فقط resolve شدن نام را می‌سنجد و دسترسی challenge را تضمین نمی‌کند. پورت‌های 80 و 443 باید از اینترنت قابل‌دسترسی باشند؛ wildcard پشتیبانی نمی‌شود.</p>
      {error && <Alert className="border-destructive/35 bg-destructive/10"><AlertTriangle size={15}/><AlertDescription>{error}</AlertDescription></Alert>}
      <Button type="button" disabled={checking || busy || !allValid || !emailValid} onClick={() => void checkDomains()}>{checking ? <LoaderCircle size={14} className="animate-spin"/> : <RefreshCw size={14}/>}بررسی دامنه و ادامه</Button>
      {hosts.length > 0 && !allValid && <p className="text-xs text-destructive">دامنه را بدون پروتکل و مسیر وارد کن؛ wildcard برای صدور خودکار پشتیبانی نمی‌شود.</p>}
    </> : <>
      <div className="space-y-2" aria-live="polite">{results.map((result) => <div key={result.host} className="flex min-w-0 flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-background/35 p-3 text-xs"><div className="flex min-w-0 items-center gap-2">{result.resolved ? <CheckCircle2 size={14} className="shrink-0 text-emerald-500"/> : <AlertTriangle size={14} className="shrink-0 text-amber-500"/>}<code dir="ltr" className="truncate font-mono">{result.host}</code></div><Badge variant={result.resolved ? 'success' : 'warning'}>{result.resolved ? 'DNS پاسخ داد' : 'Resolve نشد'}</Badge><p dir="ltr" className="basis-full break-all text-left font-mono text-[10px] text-muted-foreground">{result.addresses.join(', ') || result.error || 'نشانی IP گزارش نشد'}</p></div>)}</div>
      {current && !allResolved && <label className="flex items-start gap-2 rounded-lg border border-amber-500/20 bg-amber-500/[.05] p-3 text-xs leading-5 text-muted-foreground"><input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} className="mt-1 accent-primary"/>می‌دانم DNS برای یک یا چند دامنه resolve نشد؛ پیش از شروع صدور، دامنه و دسترسی بیرونی را دوباره بررسی می‌کنم.</label>}
      <Alert><ShieldCheck size={15}/><AlertDescription className="text-xs leading-5">دامنه‌ها: <bdi dir="ltr" className="font-mono">{hosts.join(', ')}</bdi> · انتقال HTTPS {redirect ? 'روشن' : 'خاموش'}. نتیجهٔ نهایی فقط بعد از پاسخ Certbot نمایش داده می‌شود؛ نوار پیشرفت ساختگی وجود ندارد.</AlertDescription></Alert>
      <div className="flex flex-wrap justify-between gap-2"><Button type="button" variant="outline" disabled={busy} onClick={() => setStep(0)}>بازگشت به پیش‌نیازها</Button><Button type="button" aria-busy={busy} disabled={busy || checking || !current || !allValid || !emailValid || (!allResolved && !acknowledged)} onClick={onIssue}>{busy ? <LoaderCircle size={14} className="animate-spin"/> : <ShieldCheck size={14} />}{busy ? 'در حال ساخت سایت و صدور گواهی…' : 'ساخت سایت و دریافت SSL'}</Button></div>
    </>}
  </div>
}
