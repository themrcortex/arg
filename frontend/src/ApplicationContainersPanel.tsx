import { useCallback, useEffect, useState } from 'react'
import { Box, Link2, LoaderCircle, RefreshCw, Unlink } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'

type Container = { id: string; names: string; image: string; status: string; ports: string; state: string; stats?: { cpu?: string; memory?: string; memory_percent?: string; network_io?: string; block_io?: string } }
type Link = { container_id: string; service_name: string; state: string; container: Container | null }
type Data = { links: Link[]; unmanaged_containers: Container[]; ownership: string; can_control: boolean }

export function ApplicationContainersPanel({ appId, notify }: { appId: string; notify: (message: string) => void }) {
  const [data, setData] = useState<Data | null>(null)
  const [containerId, setContainerId] = useState('')
  const [serviceName, setServiceName] = useState('web')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const load = useCallback(async () => {
    setError('')
    try { setData(await api<Data>(`apps/${encodeURIComponent(appId)}/containers`)) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'وضعیت کانتینرها خوانده نشد.') }
  }, [appId])
  useEffect(() => { void load() }, [load])
  async function link() {
    if (!containerId) return
    setBusy(true)
    try {
      await api(`apps/${encodeURIComponent(appId)}/containers`, { method: 'POST', body: JSON.stringify({ container_id: containerId, service_name: serviceName, confirmed: true }) })
      setContainerId(''); await load(); notify('کانتینر به‌صورت metadata به اپ وصل شد؛ مالکیت Docker تغییر نکرد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'اتصال کانتینر ثبت نشد.') }
    finally { setBusy(false) }
  }
  async function unlink(id: string) {
    setBusy(true)
    try { await api(`apps/${encodeURIComponent(appId)}/containers/${encodeURIComponent(id)}`, { method: 'DELETE' }); await load(); notify('لینک metadata حذف شد؛ کانتینر Docker دست‌نخورده ماند.') }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'لینک حذف نشد.') }
    finally { setBusy(false) }
  }
  return <Card className="mt-3 border-border/70 bg-background/35"><CardHeader className="flex-row items-center justify-between gap-2 py-3"><div><CardTitle className="flex items-center gap-2 text-sm"><Box size={15} className="text-primary"/>سرویس‌های Docker این اپ</CardTitle><CardDescription className="mt-1 text-[10px]">اتصال فقط برای گزارش وضعیت است؛ ARG کانتینر unmanaged را تصاحب یا کنترل نمی‌کند.</CardDescription></div><Button size="icon" variant="outline" aria-label="تازه‌سازی کانتینرها" onClick={()=>void load()} disabled={busy}><RefreshCw size={13}/></Button></CardHeader><CardContent className="space-y-3 pt-1">{error&&<p role="alert" className="text-xs text-rose-200">{error}</p>}{data?.links.map((item)=><div key={item.container_id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 p-2.5"><div className="min-w-0"><p className="flex items-center gap-2 text-xs"><b>{item.service_name}</b><Badge variant="outline">{item.state}</Badge></p><p dir="ltr" className="mt-1 truncate text-left font-mono text-[10px] text-muted-foreground">{item.container?.names || item.container_id} · {item.container?.image || 'کانتینر پیدا نشد'}</p>{item.container?.stats&&<p dir="ltr" className="mt-1 text-left text-[10px] text-muted-foreground">CPU {item.container.stats.cpu||'—'} · RAM {item.container.stats.memory||'—'} · Net {item.container.stats.network_io||'—'}</p>}</div><Button size="sm" variant="ghost" onClick={()=>void unlink(item.container_id)} disabled={busy}><Unlink size={13}/>برداشتن لینک</Button></div>)}<div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_140px_auto]"><select aria-label="کانتینر موجود" className="h-10 min-w-0 rounded-md border border-input bg-background px-3 text-xs" value={containerId} onChange={(event)=>setContainerId(event.target.value)}><option value="">انتخاب کانتینر unmanaged</option>{data?.unmanaged_containers.map((item)=><option key={item.id} value={item.id}>{item.names} · {item.state}</option>)}</select><Input aria-label="نام سرویس" value={serviceName} onChange={(event)=>setServiceName(event.target.value)} placeholder="web"/><Button size="sm" onClick={()=>void link()} disabled={!containerId||busy}><Link2 size={13}/>{busy?<LoaderCircle size={13} className="animate-spin"/>:'ثبت لینک'}</Button></div><p className="text-[10px] leading-5 text-muted-foreground">این اتصال وضعیت فعلی را به کارت اپ اضافه می‌کند. Start/Stop، تغییر image، volumes، شبکه و rollback خودکار از این لینک انجام نمی‌شود.</p></CardContent></Card>
}
