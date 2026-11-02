import { useCallback, useEffect, useState, type FormEvent } from 'react'
import { Check, ExternalLink, Globe2, LoaderCircle, Plus, RefreshCw, Save, ToggleLeft } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'

type Site = { name: string; content: string; content_hash: string; enabled: boolean; ssl_enabled?: boolean; application_id?: string }
type SiteState = { configured: boolean; site: Site | null; domain: string | null; domains: string[]; framework?: string; warning?: string | null }

export function ApplicationNginxLinkPanel({ app, notify }: { app: { id: string; name: string; domain?: string | null }; notify: (message: string) => void }) {
  const [state, setState] = useState<SiteState | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [primary, setPrimary] = useState(app.domain || '')
  const [aliases, setAliases] = useState('')
  const [config, setConfig] = useState('')
  const [configDirty, setConfigDirty] = useState(false)

  const refresh = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const result = await api<SiteState>(`apps/${encodeURIComponent(app.id)}/nginx-site`)
      setState(result)
      setPrimary(result.domain || '')
      setAliases((result.domains || []).filter((domain) => domain !== result.domain).join(', '))
      setConfig(result.site?.content || '')
      setConfigDirty(false)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'وضعیت تنظیمات Nginx خوانده نشد.')
    } finally {
      setLoading(false)
    }
  }, [app.id])

  useEffect(() => { void refresh() }, [refresh])

  async function ensureSite() {
    setBusy('create'); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/nginx-site`, { method: 'POST' })
      await refresh()
      notify('کانفیگ Nginx ساخته و اعتبارسنجی شد؛ تا آماده‌شدن release غیرفعال می‌ماند.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت تنظیمات Nginx ناموفق بود.') }
    finally { setBusy('') }
  }

  async function saveDomains(event: FormEvent) {
    event.preventDefault()
    const all = [primary, ...aliases.split(/[\n,]+/).map((value) => value.trim()).filter(Boolean)].map((value) => value.trim()).filter(Boolean)
    setBusy('domains'); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/nginx-site/domains`, { method: 'PUT', body: JSON.stringify({ domains: all }) })
      await refresh()
      notify('دامنه‌ها و server_name همگام شدند؛ Nginx کانفیگ را اعتبارسنجی و در صورت نیاز امن reload کرد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ دامنه‌ها ناموفق بود.') }
    finally { setBusy('') }
  }

  async function saveConfig() {
    if (!state?.site) return
    setBusy('config'); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/nginx-site/config`, { method: 'PUT', body: JSON.stringify({ content: config, expected_hash: state.site.content_hash }) })
      setConfigDirty(false)
      await refresh()
      notify('کانفیگ سفارشی ذخیره شد؛ nginx -t موفق بود و Nginx امن reload شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ کانفیگ Nginx ناموفق بود.') }
    finally { setBusy('') }
  }

  async function toggleSite() {
    if (!state?.site) return
    setBusy('toggle'); setError('')
    try {
      await api(`nginx/sites/${encodeURIComponent(state.site.name)}/enabled`, { method: 'PUT', body: JSON.stringify({ enabled: !state.site.enabled }) })
      await refresh()
      notify(state.site.enabled ? 'سایت Nginx غیرفعال شد.' : 'سایت Nginx فعال شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر وضعیت سایت Nginx ناموفق بود.') }
    finally { setBusy('') }
  }

  const openFullManager = (tab: 'overview' | 'domains' | 'config' = 'overview') => {
    if (!state?.site) return
    const query = new URLSearchParams({ site: state.site.name, tab })
    window.location.hash = `#/nginx?${query.toString()}`
  }

  return <Card className="border-border/70 bg-background/35">
    <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 py-4">
      <div><CardTitle className="flex items-center gap-2 text-sm"><Globe2 size={16} className="text-primary"/>دامنه و Nginx</CardTitle><CardDescription className="mt-1 max-w-3xl text-xs leading-5">کانفیگ مستقل همین اپ، دامنهٔ اصلی و Aliasها را مدیریت کن. هر ذخیره با nginx -t بررسی می‌شود و فایل سایت در شکست اعتبارسنجی دست‌نخورده می‌ماند.</CardDescription></div>
      <div className="flex gap-2"><Button size="sm" variant="outline" onClick={() => void refresh()} disabled={loading || !!busy}><RefreshCw size={13} className={loading ? 'animate-spin' : ''}/>تازه‌سازی</Button>{state?.site && <Button size="sm" variant="outline" onClick={() => openFullManager()}><ExternalLink size={13}/>صفحهٔ کامل Nginx</Button>}</div>
    </CardHeader>
    <CardContent className="space-y-4 pt-0">
      {error && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-300/[.04] p-3 text-xs leading-5 text-rose-100">{error}</p>}
      {loading ? <div className="flex min-h-28 items-center justify-center"><LoaderCircle className="animate-spin text-primary" size={20}/></div> : <>
        {state?.warning && <p role="status" className="rounded-lg border border-amber-300/20 bg-amber-300/[.04] p-3 text-xs leading-5 text-amber-100">{state.warning}</p>}
        {!state?.site && <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-dashed border-primary/30 bg-primary/[.035] p-4"><div><p className="text-sm font-medium">{app.domain ? 'کانفیگ Nginx برای این اپ پیدا نشد' : 'برای ساخت سایت، دامنه لازم است'}</p><p className="mt-1 text-xs leading-5 text-muted-foreground">{app.domain ? 'ساخت کانفیگ امن و غیرفعال انجام می‌شود؛ پس از اولین انتشار، تنظیمات runtime تکمیل و سایت فعال می‌شود.' : 'دامنه را در فرم زیر اضافه کن. سپس کانفیگ سایت به‌صورت خودکار ساخته می‌شود.'}</p></div>{app.domain && <Button size="sm" onClick={() => void ensureSite()} disabled={!!busy}><Plus size={14}/>{busy === 'create' ? 'در حال ساخت…' : 'ساخت کانفیگ Nginx'}</Button>}</div>}
        {state?.site && <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border/70 bg-background/40 p-3"><div className="flex min-w-0 flex-wrap items-center gap-2"><Badge variant={state.site.enabled ? 'success' : 'outline'}>{state.site.enabled ? 'فعال' : 'غیرفعال'}</Badge><Badge variant={state.site.ssl_enabled ? 'success' : 'outline'}>{state.site.ssl_enabled ? 'HTTPS تنظیم شده' : 'SSL تنظیم نشده'}</Badge><code dir="ltr" className="font-mono text-xs">{state.site.name}</code></div><Button size="sm" variant="outline" onClick={() => void toggleSite()} disabled={!!busy}><ToggleLeft size={14}/>{state.site.enabled ? 'غیرفعال‌کردن' : 'فعال‌کردن'}</Button></div>}
        <form onSubmit={(event) => void saveDomains(event)} className="space-y-3 rounded-xl border border-border/70 p-4">
          <div><h3 className="text-sm font-semibold">دامنه‌ها</h3><p className="mt-1 text-[11px] leading-5 text-muted-foreground">اولین مقدار دامنهٔ اصلی است. Aliasها را با ویرگول یا خط جدید جدا کن؛ برای حذف همهٔ دامنه‌ها هر دو فیلد را خالی و ذخیره کن.</p></div>
          <div className="grid gap-3 md:grid-cols-2"><label className="space-y-1.5 text-xs"><span>دامنهٔ اصلی</span><input dir="ltr" className="h-10 w-full rounded-md border border-input bg-background px-3 text-left font-mono text-xs" value={primary} onChange={(event) => setPrimary(event.target.value)} placeholder="app.example.com" /></label><label className="space-y-1.5 text-xs"><span>دامنه‌های جایگزین</span><input dir="ltr" className="h-10 w-full rounded-md border border-input bg-background px-3 text-left font-mono text-xs" value={aliases} onChange={(event) => setAliases(event.target.value)} placeholder="www.example.com, api.example.com" /></label></div>
          <div className="flex flex-wrap gap-2"><Button size="sm" type="submit" disabled={!!busy}><Save size={14}/>{busy === 'domains' ? 'در حال ذخیره…' : 'ذخیرهٔ دامنه‌ها'}</Button>{state?.site && <Button size="sm" type="button" variant="outline" onClick={() => openFullManager('domains')}><ExternalLink size={13}/>DNS و SSL</Button>}</div>
        </form>
        {state?.site && <section className="space-y-3 rounded-xl border border-border/70 p-4"><div className="flex flex-wrap items-start justify-between gap-2"><div><h3 className="text-sm font-semibold">کانفیگ Nginx</h3><p className="mt-1 text-[11px] leading-5 text-muted-foreground">برای ویرایش کامل، متن کانفیگ را تغییر بده. نام دامنه‌ها را از فرم بالا نگه دار تا تنظیمات اپ و Nginx هماهنگ بماند.</p></div><Button size="sm" variant="outline" onClick={() => openFullManager('config')}><ExternalLink size={13}/>ویرایشگر کامل</Button></div><textarea dir="ltr" spellCheck={false} value={config} onChange={(event) => { setConfig(event.target.value); setConfigDirty(true) }} className="min-h-72 w-full resize-y rounded-lg border border-input bg-background p-3 text-left font-mono text-[11px] leading-5 outline-none focus-visible:ring-2 focus-visible:ring-primary" aria-label="کانفیگ Nginx اپلیکیشن"/><div className="flex flex-wrap items-center justify-between gap-2"><p className="text-[10px] text-muted-foreground">سایت‌های مدیریت‌شده با اپ ممکن است هنگام استقرار runtime دوباره همگام شوند.</p><Button size="sm" onClick={() => void saveConfig()} disabled={!configDirty || !!busy}><Check size={14}/>{busy === 'config' ? 'در حال اعتبارسنجی…' : 'ذخیره و اعتبارسنجی'}</Button></div></section>}
      </>}
    </CardContent>
  </Card>
}
