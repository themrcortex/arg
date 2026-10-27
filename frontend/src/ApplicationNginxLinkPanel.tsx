import { useEffect, useState } from 'react'
import { Globe2, Link2, LoaderCircle, Unlink } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Site = { name: string; server_names?: string[]; enabled?: boolean; ssl_enabled?: boolean }
type LinkState = { site: { name: string } | null; domain?: string | null }

export function ApplicationNginxLinkPanel({ app, notify }: { app: { id: string; name: string; domain?: string | null }; notify: (message: string) => void }) {
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [link, setLink] = useState<LinkState | null>(null)
  const [sites, setSites] = useState<Site[]>([])
  const [selected, setSelected] = useState('')
  const [confirm, setConfirm] = useState<'link' | 'unlink' | null>(null)
  const [error, setError] = useState('')
  const matching = sites.filter((site) => app.domain && site.server_names?.includes(app.domain))

  useEffect(() => {
    if (!open) return
    let active = true
    setLoading(true); setError('')
    Promise.all([api<LinkState>(`apps/${encodeURIComponent(app.id)}/nginx-site`), api<{ sites: Site[] }>('nginx/sites')])
      .then(([current, inventory]) => { if (active) { setLink(current); setSites(inventory.sites || []); setSelected(inventory.sites?.find((site) => site.server_names?.includes(app.domain || ''))?.name || '') } })
      .catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : 'اطلاعات اتصال Nginx دریافت نشد.') })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [open, app.id, app.domain])

  async function apply() {
    if (!confirm) return
    setBusy(true); setError('')
    try {
      const endpoint = `apps/${encodeURIComponent(app.id)}/nginx-site`
      if (confirm === 'link') {
        const result = await api<{ site_name: string }>(endpoint, { method: 'PUT', body: JSON.stringify({ site_name: selected, confirmed: true }) })
        setLink({ site: { name: result.site_name }, domain: app.domain }); notify('لینک مالکیت اپ و سایت Nginx ثبت شد؛ کانفیگ Nginx تغییر نکرد.')
      } else {
        await api(endpoint, { method: 'DELETE', body: JSON.stringify({ confirmed: true }) })
        setLink({ site: null, domain: app.domain }); notify('لینک مدیریتی برداشته شد؛ سایت و تنظیمات Nginx دست‌نخورده ماندند.')
      }
      setConfirm(null)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر لینک Nginx انجام نشد.') }
    finally { setBusy(false) }
  }

  return <Card className="mt-3 border-border/70 bg-background/35">
    <CardHeader className="flex-row items-center justify-between gap-3 py-3"><div><CardTitle className="flex items-center gap-2 text-sm"><Globe2 size={15} className="text-primary"/>دامنه و Nginx</CardTitle><CardDescription className="mt-1 text-[10px]">اتصال فقط بعد از تطبیق دامنهٔ اپ با server_name ثبت می‌شود؛ فایل کانفیگ تغییر نمی‌کند.</CardDescription></div><Button variant="outline" size="sm" onClick={() => setOpen((value) => !value)}>{open ? 'بستن' : 'مدیریت لینک'}</Button></CardHeader>
    {open && <CardContent className="space-y-3 pt-1">{error && <p role="alert" className="rounded-lg border border-rose-300/20 p-2 text-xs text-rose-200">{error}</p>}{loading ? <LoaderCircle className="mx-auto animate-spin text-primary" size={18}/> : <>
      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-border/70 p-3 text-xs"><span>دامنهٔ اپ:</span><code dir="ltr" className="font-mono">{app.domain || 'ثبت نشده'}</code>{link?.site && <><Badge variant="success">متصل به {link.site.name}</Badge><Button size="sm" variant="outline" onClick={() => setConfirm('unlink')}><Unlink size={13}/>قطع لینک</Button></>}</div>
      {!link?.site && <div className="flex flex-wrap items-center gap-2"><select value={selected} onChange={(event) => setSelected(event.target.value)} disabled={!app.domain || !matching.length} className="h-10 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-xs"><option value="">{!app.domain ? 'ابتدا دامنهٔ اپ را ثبت کن' : matching.length ? 'سایت هم‌دامنه را انتخاب کن' : 'سایت Nginx با این دامنه پیدا نشد'}</option>{matching.map((site) => <option key={site.name} value={site.name}>{site.name}{site.ssl_enabled ? ' · HTTPS' : ''}</option>)}</select><Button size="sm" onClick={() => setConfirm('link')} disabled={!selected}><Link2 size={13}/>اتصال سایت</Button></div>}
      <p className="text-[10px] leading-5 text-muted-foreground">این لینک مالکیت و ناوبری اپ را ثبت می‌کند. صدور SSL، reload، تغییر فایروال یا ویرایش کانفیگ در این دکمه انجام نمی‌شود.</p>
    </>}</CardContent>}
    <Dialog open={!!confirm} onOpenChange={(value) => { if (!value && !busy) setConfirm(null) }}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>{confirm === 'link' ? 'اتصال سایت Nginx به اپ' : 'قطع لینک مدیریتی'}</DialogTitle><DialogDescription>{confirm === 'link' ? `سایت «${selected}» به اپ «${app.name}» وصل می‌شود. سرور Nginx، فایل‌ها و SSL تغییر نمی‌کنند.` : `فقط لینک سایت «${link?.site?.name}» به اپ برداشته می‌شود. فایل و سرویس سایت باقی می‌مانند.`}</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setConfirm(null)} disabled={busy}>انصراف</Button><Button variant={confirm === 'unlink' ? 'destructive' : 'default'} onClick={() => void apply()} disabled={busy}>{busy && <LoaderCircle size={14} className="animate-spin"/>}{confirm === 'link' ? 'تأیید اتصال' : 'تأیید قطع لینک'}</Button></div></DialogContent></Dialog>
  </Card>
}
