import { useEffect, useState } from 'react'
import { Archive, Download, LoaderCircle, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type ArchiveRow = { id: string; size_bytes: number }
const size = (value: number) => value >= 1024 ** 3 ? `${(value / 1024 ** 3).toFixed(2)} GiB` : `${(value / 1024 ** 2).toFixed(1)} MiB`

export function ApplicationBackupPanel({ appId, notify }: { appId: string; notify: (message: string) => void }) {
  const [archives, setArchives] = useState<ArchiveRow[]>([])
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [confirm, setConfirm] = useState(false)
  const [removeTarget, setRemoveTarget] = useState<ArchiveRow | null>(null)
  const [error, setError] = useState('')
  async function refresh() {
    setLoading(true); setError('')
    try { const result = await api<{ archives: ArchiveRow[] }>(`apps/${encodeURIComponent(appId)}/backups`); setArchives(result.archives || []) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'فهرست آرشیوها خوانده نشد.') }
    finally { setLoading(false) }
  }
  useEffect(() => { void refresh() }, [appId])
  async function create() {
    setBusy(true); setError('')
    try { const result = await api<{ id: string; size_bytes: number }>(`apps/${encodeURIComponent(appId)}/backups`, { method: 'POST', body: JSON.stringify({ confirmed: true }) }); notify(`آرشیو سورس ساخته شد (${size(result.size_bytes)}). این آرشیو قابل بازیابی به‌عنوان بکاپ کامل نیست.`); setConfirm(false); await refresh() }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت آرشیو سورس ناموفق بود.') }
    finally { setBusy(false) }
  }
  async function removeArchive() {
    if (!removeTarget) return
    setBusy(true); setError('')
    try { await api(`apps/${encodeURIComponent(appId)}/backups/${encodeURIComponent(removeTarget.id)}`, { method: 'DELETE', body: JSON.stringify({ confirmed: true }) }); notify('آرشیو محلی حذف شد.'); setRemoveTarget(null); await refresh() }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف آرشیو ناموفق بود.') }
    finally { setBusy(false) }
  }
  return <Card className="mt-3 border-border/70 bg-background/35">
    <CardHeader className="flex-row items-center justify-between gap-2 py-3"><div><CardTitle className="flex items-center gap-2 text-sm"><Archive size={15} className="text-primary"/>آرشیو سورس برنامه</CardTitle><CardDescription className="mt-1 text-[10px]">فقط فایل‌های release فعال؛ مسیرهای حساس شناخته‌شده حذف می‌شوند، اما محتوای کد secret-scan نمی‌شود و این بکاپ قابل‌بازیابی نیست.</CardDescription></div><div className="flex gap-1"><Button size="icon" variant="ghost" aria-label="تازه‌سازی آرشیوها" onClick={() => void refresh()} disabled={loading}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/></Button><Button size="sm" onClick={() => setConfirm(true)} disabled={busy}><Plus size={13}/>ساخت آرشیو</Button></div></CardHeader>
    <CardContent className="space-y-2 pt-0">{error && <p role="alert" className="text-xs text-rose-200">{error}</p>}{loading && <LoaderCircle size={16} className="animate-spin text-primary"/>}{!loading && archives.length === 0 && !error && <p className="text-xs text-muted-foreground">هنوز آرشیوی ساخته نشده است.</p>}{archives.map((item) => <div key={item.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 px-3 py-2 text-xs"><span dir="ltr" className="min-w-0 truncate font-mono">{item.id}</span><div className="flex items-center gap-2"><span className="text-muted-foreground">{size(item.size_bytes)}</span><Badge variant="outline">سورس</Badge><Button asChild size="icon" variant="ghost" aria-label="دانلود آرشیو"><a href={`/api/apps/${encodeURIComponent(appId)}/backups/${encodeURIComponent(item.id)}/download`}><Download size={14}/></a></Button><Button size="icon" variant="ghost" aria-label="حذف آرشیو" onClick={() => setRemoveTarget(item)}><Trash2 size={14}/></Button></div></div>)}</CardContent>
    <Dialog open={confirm} onOpenChange={(open) => { if (!busy) setConfirm(open) }}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>ساخت آرشیو سورس</DialogTitle><DialogDescription>فایل‌های dotfile، dependencyها، فایل‌های دیتابیس و مسیرهای حساس شناخته‌شده حذف می‌شوند. محتوای سورس برای credentialهای hard-coded اسکن نمی‌شود؛ پیش از اشتراک‌گذاری آرشیو را بررسی کن. این فایل بکاپ قابل‌بازیابی نیست.</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setConfirm(false)} disabled={busy}>انصراف</Button><Button onClick={() => void create()} disabled={busy}>{busy && <LoaderCircle size={14} className="animate-spin"/>}تأیید و ساخت</Button></div></DialogContent></Dialog>
    <Dialog open={!!removeTarget} onOpenChange={(open) => { if (!open && !busy) setRemoveTarget(null) }}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>حذف آرشیو محلی؟</DialogTitle><DialogDescription>فایل «{removeTarget?.id}» از دیسک این VPS حذف می‌شود. این کار، release فعال یا فایل‌های برنامه را تغییر نمی‌دهد.</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setRemoveTarget(null)} disabled={busy}>انصراف</Button><Button variant="destructive" onClick={() => void removeArchive()} disabled={busy}>{busy && <LoaderCircle size={14} className="animate-spin"/>}<Trash2 size={14}/>حذف آرشیو</Button></div></DialogContent></Dialog>
  </Card>
}
