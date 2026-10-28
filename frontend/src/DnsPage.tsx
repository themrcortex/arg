import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { ChevronLeft, ChevronRight, Cloud, Globe2, LoaderCircle, Plus, RefreshCw, ShieldCheck, Trash2 } from 'lucide-react'
import { ApiError, api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Skeleton } from './components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Connection = { id: string; provider: string; name: string; zone_name: string; credential_present: boolean }
type DnsRecord = { id: string; type: string; name: string; content: string; ttl: number; proxied?: boolean; priority?: number }
type ApiData<T> = { connections?: T[]; records?: T[]; pagination?: { page?: number; per_page?: number; total_pages?: number } }
const recordKinds = ['A', 'AAAA', 'CNAME', 'MX', 'TXT']

export function DnsPage({ Heading: PageHeading, notify }: { Heading: Heading; notify: (message: string) => void }) {
  const [connections, setConnections] = useState<Connection[]>([])
  const [selected, setSelected] = useState('')
  const [records, setRecords] = useState<DnsRecord[]>([])
  const [recordPage, setRecordPage] = useState(1)
  const [totalRecordPages, setTotalRecordPages] = useState(1)
  const [recordsLoading, setRecordsLoading] = useState(false)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [apiUnavailable, setApiUnavailable] = useState(false)
  const [connectOpen, setConnectOpen] = useState(false)
  const [recordOpen, setRecordOpen] = useState(false)
  const [disconnectOpen, setDisconnectOpen] = useState(false)
  const [deleteTarget, setDeleteTarget] = useState<DnsRecord | null>(null)
  const [draft, setDraft] = useState({ name: '', zone: '', api_token: '' })
  const [recordDraft, setRecordDraft] = useState({ kind: 'A', name: '@', content: '', ttl: '1', proxied: false, priority: '' })
  const selectedRef = useRef('')
  const recordsRequestRef = useRef(0)

  function chooseConnection(id: string) {
    if (selectedRef.current !== id) {
      recordsRequestRef.current += 1
      setRecords([])
      setTotalRecordPages(1)
      setRecordsLoading(false)
    }
    selectedRef.current = id
    setRecordPage(1)
    setSelected(id)
  }

  const loadConnections = useCallback(async () => {
    const result = await api<ApiData<Connection>>('dns/connections')
    const items = result.connections || []
    setConnections(items)
    const next = items.some((item) => item.id === selectedRef.current) ? selectedRef.current : items[0]?.id || ''
    if (next !== selectedRef.current) {
      recordsRequestRef.current += 1
      setRecords([])
      setTotalRecordPages(1)
      setRecordsLoading(false)
      setRecordPage(1)
    }
    selectedRef.current = next
    setSelected(next)
  }, [])
  const loadRecords = useCallback(async (id = selected, page = recordPage) => {
    const requestId = ++recordsRequestRef.current
    if (!id) { setRecords([]); setTotalRecordPages(1); setRecordsLoading(false); return }
    setRecordsLoading(true)
    try {
      const result = await api<ApiData<DnsRecord>>(`dns/connections/${encodeURIComponent(id)}/records?page=${page}`)
      if (requestId !== recordsRequestRef.current) return
      const totalPages = Math.max(1, Math.min(10_000, result.pagination?.total_pages || 1))
      setTotalRecordPages(totalPages)
      if (page > totalPages) { setRecordPage(totalPages); return }
      setRecords(result.records || [])
    } catch (cause) {
      if (requestId === recordsRequestRef.current) throw cause
    } finally {
      if (requestId === recordsRequestRef.current) setRecordsLoading(false)
    }
  }, [selected, recordPage])
  const refresh = useCallback(async () => {
    setLoading(true); setError(''); setApiUnavailable(false)
    try { await loadConnections() } catch (cause) {
      if (cause instanceof ApiError && cause.status === 404) {
        setConnections([]); setSelected(''); setRecords([]); setApiUnavailable(true)
      } else setError(cause instanceof Error ? cause.message : 'اتصال‌های DNS بارگذاری نشدند.')
    }
    finally { setLoading(false) }
  }, [loadConnections])
  useEffect(() => { void refresh() }, [refresh])
  useEffect(() => {
    void loadRecords().catch((cause) => setError(cause instanceof Error ? cause.message : 'رکوردهای DNS خوانده نشدند.'))
    return () => { recordsRequestRef.current += 1 }
  }, [selected, recordPage, loadRecords])

  async function connect() {
    setBusy(true); setError('')
    try {
      const result = await api<Connection & { tested: boolean }>('dns/connections', { method: 'POST', body: JSON.stringify(draft) })
      setConnectOpen(false); setDraft({ name: '', zone: '', api_token: '' }); await loadConnections(); chooseConnection(result.id); notify(`اتصال Cloudflare برای ${result.zone_name} بررسی و ذخیره شد.`)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'اتصال Cloudflare برقرار نشد.') }
    finally { setBusy(false) }
  }
  async function addRecord() {
    if (!selected) return
    setBusy(true); setError('')
    try {
      await api(`dns/connections/${encodeURIComponent(selected)}/records`, { method: 'POST', body: JSON.stringify({ ...recordDraft, ttl: Number(recordDraft.ttl), priority: recordDraft.priority ? Number(recordDraft.priority) : null, confirmed: true }) })
      setRecordOpen(false); setRecordDraft({ kind: 'A', name: '@', content: '', ttl: '1', proxied: false, priority: '' }); await loadRecords(); notify('رکورد DNS در Cloudflare ثبت شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'رکورد ساخته نشد.') }
    finally { setBusy(false) }
  }
  async function removeRecord() {
    if (!selected || !deleteTarget) return
    setBusy(true); setError('')
    try {
      await api(`dns/connections/${encodeURIComponent(selected)}/records/${encodeURIComponent(deleteTarget.id)}`, { method: 'DELETE', body: JSON.stringify({ confirmed: true }) })
      setDeleteTarget(null); await loadRecords(); notify(`رکورد ${deleteTarget.name} از zone حذف شد.`)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف رکورد انجام نشد.') }
    finally { setBusy(false) }
  }
  async function disconnect() {
    if (!selected) return
    setBusy(true); setError('')
    try {
      await api(`dns/connections/${encodeURIComponent(selected)}`, { method: 'DELETE', body: JSON.stringify({ confirmed: true }) })
      setDisconnectOpen(false); await loadConnections(); setRecords([]); notify('Credential از ARG حذف شد؛ رکوردهای Cloudflare دست‌نخورده ماندند.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'قطع اتصال انجام نشد.') }
    finally { setBusy(false) }
  }

  const selectedConnection = connections.find((item) => item.id === selected)
  return <>
    <PageHeading title="مدیریت DNS" description="Zoneهای Cloudflare را با اتصال آزمایش‌شده مدیریت کن؛ عملیات هر رکورد مستقیم روی ارائه‌دهنده اعمال می‌شود." action={<div className="flex gap-2"><Button variant="outline" onClick={() => void refresh()} disabled={loading}><RefreshCw size={14}/>تازه‌سازی</Button><Button onClick={() => setConnectOpen(true)} disabled={apiUnavailable}><Plus size={15}/>اتصال Cloudflare</Button></div>} />
    {apiUnavailable && <div role="status" className="mb-4 rounded-xl border border-amber-300/25 bg-amber-300/[.05] p-4 text-xs leading-6 text-amber-100"><p className="font-medium">مدیریت DNS در VPS انتخاب‌شده در دسترس نیست</p><p className="text-amber-100/80">API اتصال‌های Cloudflare در این Node پیدا نشد (HTTP 404). این معمولاً یعنی نسخهٔ ARG روی Node قدیمی است. Node را به نسخهٔ جدید به‌روزرسانی کن، سپس «تازه‌سازی» را بزن. برای جلوگیری از ذخیرهٔ Token روی VPS اشتباه، اتصال تا آن زمان غیرفعال است.</p></div>}
    {error && <div role="alert" className="mb-4 rounded-xl border border-rose-300/20 bg-rose-400/5 p-3 text-sm text-rose-100">{error}</div>}
    <Tabs defaultValue="zones" className="mt-4" dir="rtl">
      <TabsList className="h-auto w-full flex-wrap justify-start sm:w-auto" aria-label="بخش‌های مدیریت DNS">
        <TabsTrigger value="zones">Zoneهای متصل</TabsTrigger>
        <TabsTrigger value="records">رکوردهای DNS</TabsTrigger>
      </TabsList>
      <TabsContent value="zones" className="space-y-4">
      <Card><CardHeader><CardTitle className="flex items-center gap-2"><Cloud size={17} className="text-primary"/>Zoneهای متصل</CardTitle><CardDescription>Token فقط بعد از تست معتبر ذخیره و در API مخفی می‌شود.</CardDescription></CardHeader><CardContent className="space-y-2">{loading ? [0,1].map((item)=><Skeleton key={item} className="h-20 rounded-xl"/>) : connections.map((item)=><button key={item.id} onClick={()=>chooseConnection(item.id)} className={`w-full rounded-xl border p-3 text-right transition-colors ${selected===item.id?'border-primary/50 bg-primary/[.06]':'border-border/70 hover:bg-muted/30'}`}><span className="flex items-center justify-between gap-2"><b className="truncate text-sm">{item.name}</b><Badge variant="outline">Cloudflare</Badge></span><span dir="ltr" className="mt-2 block truncate text-left font-mono text-xs text-muted-foreground">{item.zone_name}</span><span className="mt-2 flex items-center gap-1 text-[10px] text-emerald-200"><ShieldCheck size={12}/>Credential رمز‌شده · مخفی</span></button>)}{!loading&&!connections.length&&<div className="rounded-xl border border-dashed border-border p-5 text-center text-xs text-muted-foreground">هنوز Zoneای وصل نشده است.</div>}</CardContent></Card>
      </TabsContent>
      <TabsContent value="records" className="space-y-4">
      <Card><CardHeader className="flex-row items-center justify-between gap-3"><div><CardTitle className="flex items-center gap-2"><Globe2 size={17} className="text-primary"/>{selectedConnection?.zone_name || 'رکوردهای DNS'}</CardTitle><CardDescription className="mt-1">تغییرات این بخش روی zone واقعی Cloudflare اعمال می‌شود؛ پیش از حذف، وابستگی دامنه و ایمیل را بررسی کن.</CardDescription></div><div className="flex gap-2">{selected&&<Button variant="outline" size="sm" onClick={()=>setDisconnectOpen(true)}><Trash2 size={13}/>قطع اتصال</Button>}<Button onClick={()=>setRecordOpen(true)} disabled={!selected}><Plus size={14}/>رکورد جدید</Button></div></CardHeader><CardContent>{!selected ? <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">برای مدیریت رکوردها ابتدا یک Zone وصل کن.</div> : recordsLoading ? <Skeleton className="h-48 rounded-xl"/> : <div className="overflow-x-auto"><table className="w-full min-w-[600px] text-right text-xs"><thead><tr className="border-b border-border text-muted-foreground"><th className="p-3">نوع</th><th className="p-3">نام</th><th className="p-3">مقدار</th><th className="p-3">TTL</th><th className="p-3">Proxy</th><th className="p-3">عملیات</th></tr></thead><tbody>{records.map((record)=><tr key={record.id} className="border-b border-border/60"><td className="p-3"><Badge variant="outline">{record.type}</Badge></td><td dir="ltr" className="p-3 text-left font-mono">{record.name}</td><td dir="ltr" className="max-w-80 truncate p-3 text-left font-mono">{record.content}</td><td className="p-3">{record.ttl===1?'خودکار':`${record.ttl}s`}</td><td className="p-3">{record.proxied?'فعال':'DNS only'}</td><td className="p-3"><Button size="icon" variant="ghost" aria-label={`حذف ${record.name}`} onClick={()=>setDeleteTarget(record)}><Trash2 size={14} className="text-rose-200"/></Button></td></tr>)}</tbody></table>{!records.length&&<p className="p-8 text-center text-sm text-muted-foreground">این Zone رکوردی ندارد.</p>}<div className="flex items-center justify-between gap-3 border-t border-border/60 px-3 py-3 text-xs"><span className="text-muted-foreground">صفحه {recordPage} از {totalRecordPages} · هر صفحه ۱۰۰ رکورد</span><div className="flex gap-2"><Button variant="outline" size="sm" aria-label="صفحهٔ قبلی" disabled={recordsLoading||recordPage<=1} onClick={()=>setRecordPage((page)=>Math.max(1,page-1))}><ChevronRight size={14}/>قبلی</Button><Button variant="outline" size="sm" aria-label="صفحهٔ بعدی" disabled={recordsLoading||recordPage>=totalRecordPages} onClick={()=>setRecordPage((page)=>Math.min(totalRecordPages,page+1))}>بعدی<ChevronLeft size={14}/></Button></div></div></div>}</CardContent></Card>
      </TabsContent>
    </Tabs>
    <Dialog open={connectOpen} onOpenChange={(open)=>{setConnectOpen(open);if(!open)setDraft({name:'',zone:'',api_token:''})}}><DialogContent className="max-h-[92dvh] overflow-auto sm:max-w-lg"><DialogHeader><DialogTitle>اتصال Cloudflare</DialogTitle><DialogDescription>ARG دسترسی Token و وجود Zone را بررسی می‌کند. Token در پاسخ یا log نمایش داده نمی‌شود و روی دیسک رمز می‌شود.</DialogDescription></DialogHeader><div className="space-y-3"><div className="space-y-1"><Label>نام اتصال</Label><Input value={draft.name} onChange={(event)=>setDraft({...draft,name:event.target.value})} placeholder="وب‌سایت اصلی"/></div><div className="space-y-1"><Label>Zone</Label><Input dir="ltr" value={draft.zone} onChange={(event)=>setDraft({...draft,zone:event.target.value})} placeholder="example.com"/></div><div className="space-y-1"><Label>API Token</Label><Input dir="ltr" type="password" autoComplete="new-password" value={draft.api_token} onChange={(event)=>setDraft({...draft,api_token:event.target.value})} placeholder="Token با دسترسی Zone:DNS:Edit"/></div><p className="text-[11px] leading-5 text-muted-foreground">برای Token فقط دسترسی Zone:Read و DNS:Edit همان Zone را بده. رکوردهای DNS می‌توانند سایت یا ایمیل را از دسترس خارج کنند.</p></div><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setConnectOpen(false)}>انصراف</Button><Button onClick={()=>void connect()} disabled={busy||!draft.api_token||!draft.zone||!draft.name}>{busy&&<LoaderCircle className="animate-spin" size={14}/>}بررسی و اتصال</Button></div></DialogContent></Dialog>
    <Dialog open={recordOpen} onOpenChange={setRecordOpen}><DialogContent className="max-h-[92dvh] overflow-auto sm:max-w-lg"><DialogHeader><DialogTitle>افزودن DNS Record</DialogTitle><DialogDescription>تغییر مستقیم روی zone {selectedConnection?.zone_name} نوشته می‌شود.</DialogDescription></DialogHeader><div className="grid gap-3 sm:grid-cols-2"><div className="space-y-1"><Label>نوع رکورد</Label><select className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm" value={recordDraft.kind} onChange={(event)=>setRecordDraft({...recordDraft,kind:event.target.value})}>{recordKinds.map((kind)=><option key={kind}>{kind}</option>)}</select></div><div className="space-y-1"><Label>TTL (ثانیه)</Label><Input type="number" min="1" max="86400" value={recordDraft.ttl} onChange={(event)=>setRecordDraft({...recordDraft,ttl:event.target.value})}/></div><div className="space-y-1 sm:col-span-2"><Label>نام</Label><Input dir="ltr" value={recordDraft.name} onChange={(event)=>setRecordDraft({...recordDraft,name:event.target.value})} placeholder="@ یا app.example.com"/></div><div className="space-y-1 sm:col-span-2"><Label>مقدار / مقصد</Label><Input dir="ltr" value={recordDraft.content} onChange={(event)=>setRecordDraft({...recordDraft,content:event.target.value})} placeholder={recordDraft.kind==='A'?'203.0.113.10':recordDraft.kind==='AAAA'?'2001:db8::10':'target.example.com'}/></div>{recordDraft.kind==='MX'&&<div className="space-y-1 sm:col-span-2"><Label>Priority</Label><Input type="number" min="0" max="65535" value={recordDraft.priority} onChange={(event)=>setRecordDraft({...recordDraft,priority:event.target.value})}/></div>}{['A','AAAA','CNAME'].includes(recordDraft.kind)&&<label className="flex items-center gap-2 text-xs sm:col-span-2"><input type="checkbox" checked={recordDraft.proxied} onChange={(event)=>setRecordDraft({...recordDraft,proxied:event.target.checked})}/>پروکسی Cloudflare فعال شود</label>}</div><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setRecordOpen(false)}>انصراف</Button><Button onClick={()=>void addRecord()} disabled={busy||!recordDraft.content||!recordDraft.name}>{busy&&<LoaderCircle className="animate-spin" size={14}/>}ایجاد رکورد</Button></div></DialogContent></Dialog>
    <Dialog open={Boolean(deleteTarget)} onOpenChange={(open)=>{if(!open)setDeleteTarget(null)}}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>حذف رکورد DNS</DialogTitle><DialogDescription>رکورد {deleteTarget?.type} برای {deleteTarget?.name} از Cloudflare حذف می‌شود. اگر رکورد MX یا TXT مربوط به ایمیل است، ادامه نده.</DialogDescription></DialogHeader><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setDeleteTarget(null)}>بازگشت</Button><Button variant="destructive" onClick={()=>void removeRecord()} disabled={busy}>{busy&&<LoaderCircle className="animate-spin" size={14}/>}حذف از Cloudflare</Button></div></DialogContent></Dialog>
    <Dialog open={disconnectOpen} onOpenChange={setDisconnectOpen}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>قطع اتصال Cloudflare</DialogTitle><DialogDescription>Token رمز‌شده از ARG حذف می‌شود؛ zone و همهٔ رکوردهای آن در Cloudflare باقی می‌مانند.</DialogDescription></DialogHeader><div className="mt-5 flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={()=>setDisconnectOpen(false)}>بازگشت</Button><Button variant="destructive" onClick={()=>void disconnect()} disabled={busy}>{busy&&<LoaderCircle className="animate-spin" size={14}/>}حذف Credential از ARG</Button></div></DialogContent></Dialog>
  </>
}
