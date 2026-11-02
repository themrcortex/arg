import { useEffect, useState } from 'react'
import { ExternalLink, GitBranch, Plus, RefreshCw, Server } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'

type AppRecord = { id: string; name: string; domain: string | null; listen_port: number | null; framework: string; source_type: string }
type Environment = { id: string; name: string; kind: string; protected: boolean }
type StagingApp = { id: string; name: string; domain: string | null; listen_port: number | null; environment_name: string; environment_kind: string; created_at: number }
type StagingState = { environments: Environment[]; staging_apps: StagingApp[]; copies_secrets: false; copies_release: false }

function defaultStagingDomain(domain: string | null) {
  if (!domain || domain.includes(':') || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(domain)) return ''
  return `staging.${domain}`
}

export function ApplicationStagingPanel({ app, notify, onCreated }: { app: AppRecord; notify: (message: string) => void; onCreated: (appId: string) => Promise<void> }) {
  const [state, setState] = useState<StagingState | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [name, setName] = useState(app.name + ' · Staging')
  const [domain, setDomain] = useState(() => defaultStagingDomain(app.domain))
  const [port, setPort] = useState(app.listen_port && app.listen_port < 65535 ? String(app.listen_port + 1) : '')
  const [environmentId, setEnvironmentId] = useState('')
  const [error, setError] = useState('')

  async function load() {
    setLoading(true); setError('')
    try {
      const result = await api<StagingState>('apps/' + encodeURIComponent(app.id) + '/staging')
      setState(result)
      setEnvironmentId((current) => result.environments.some((environment) => environment.id === current) ? current : result.environments[0]?.id || '')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'محیط‌های Staging بارگذاری نشدند.') }
    finally { setLoading(false) }
  }

  useEffect(() => { void load() }, [app.id])

  async function create(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      const result = await api<{ id: string; message: string }>('apps/' + encodeURIComponent(app.id) + '/staging', {
        method: 'POST',
        body: JSON.stringify({ name: name.trim(), domain: domain.trim(), listen_port: port.trim() ? Number(port) : null, environment_id: environmentId }),
      })
      notify(result.message)
      await onCreated(result.id)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت اپلیکیشن Staging ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <Card className="xl:col-span-2">
    <CardHeader><CardTitle className="flex items-center gap-2 text-sm"><GitBranch size={15}/>محیط Staging مستقل</CardTitle><CardDescription>یک هویت اپلیکیشن جدا با release path، پردازش، دامنه و محیط اختصاصی می‌سازد. Secret، فایل .env، release فعال و دادهٔ پایدار از Production کپی نمی‌شوند.</CardDescription></CardHeader>
    <CardContent className="space-y-3">
      {loading ? <div className="flex items-center gap-2 text-xs text-muted-foreground"><RefreshCw size={13} className="animate-spin"/>در حال خواندن محیط‌های پروژه…</div> : state?.environments.length ? <form onSubmit={(event) => void create(event)} className="space-y-3 rounded-lg border border-border/70 bg-background/35 p-3">
        <div className="grid gap-2 sm:grid-cols-2"><label className="space-y-1 text-[10px] text-muted-foreground">نام اپلیکیشن Staging<Input required maxLength={80} value={name} onChange={(event) => setName(event.target.value)}/></label><label className="space-y-1 text-[10px] text-muted-foreground">محیط پروژه<select required value={environmentId} onChange={(event) => setEnvironmentId(event.target.value)} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs">{state.environments.map((environment) => <option key={environment.id} value={environment.id}>{environment.name} · staging</option>)}</select></label></div>
        <div className="grid gap-2 sm:grid-cols-2"><label className="space-y-1 text-[10px] text-muted-foreground">دامنه یا IP برای Nginx<Input dir="ltr" maxLength={253} value={domain} onChange={(event) => setDomain(event.target.value)} placeholder="staging.example.com یا IP"/><span className="block text-[9px] leading-4">برای hostname، پیش‌فرض subdomain جدا مانند staging.example.com است و DNS باید به همین VPS اشاره کند. رکورد DNS خودکار ساخته نمی‌شود. اگر دامنه خالی باشد، سرویس Node فقط روی loopback می‌ماند و IP:پورت آن از اینترنت مستقیم در دسترس نیست.</span></label><label className="space-y-1 text-[10px] text-muted-foreground">پورت loopback مستقل برای Node/SSR<Input dir="ltr" inputMode="numeric" min={1} max={65535} value={port} onChange={(event) => setPort(event.target.value)} placeholder="برای Static خالی بگذار"/><span className="block text-[9px] leading-4">پورت با Production مشترک نمی‌شود. برای اپ استاتیک پورت لازم نیست.</span></label></div>
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-[9px] leading-4 text-muted-foreground">ساخت این رکورد، کد را اجرا یا deploy نمی‌کند. صفحهٔ بعد source را برای اولین انتشار جداگانه می‌گیرد؛ Nginx و SSL هم بعداً از همان صفحهٔ اپ قابل تنظیم‌اند.</p><Button disabled={busy || !environmentId}><Plus size={13}/>{busy ? 'در حال ساخت…' : 'ساخت محیط مستقل Staging'}</Button></div>
      </form> : <div className="rounded-lg border border-dashed border-border p-3"><p className="text-xs">در این پروژه محیط Staging وجود ندارد.</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">ابتدا در پروژهٔ اپلیکیشن‌ها یک Environment با نوع Staging بساز و سپس این صفحه را تازه کن.</p><Button size="sm" variant="outline" className="mt-2" onClick={() => void load()}><RefreshCw size={12}/>تازه‌سازی</Button></div>}
      {error && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-2 text-xs text-rose-100">{error}</p>}
      {!!state?.staging_apps.length && <div className="space-y-2 border-t border-border/60 pt-3"><p className="text-xs font-medium">محیط‌های Staging موجود</p>{state.staging_apps.map((staging) => <div key={staging.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 p-2.5"><div className="flex min-w-0 items-center gap-2"><Server size={13} className="text-primary"/><div className="min-w-0"><p className="truncate text-xs font-medium">{staging.name}</p><p dir="ltr" className="truncate text-left text-[10px] text-muted-foreground">{staging.domain || 'دامنه ثبت نشده'}{staging.listen_port ? ' · 127.0.0.1:' + staging.listen_port : ''}</p></div><Badge variant="outline">{staging.environment_name}</Badge></div><Button size="sm" variant="outline" onClick={() => void onCreated(staging.id)}><ExternalLink size={12}/>بازکردن</Button></div>)}</div>}
    </CardContent>
  </Card>
}
