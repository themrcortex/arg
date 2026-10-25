import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { Activity, ArrowDownToLine, ChevronLeft, ChevronRight, Cloud, Database, ExternalLink, Folder, HardDrive, KeyRound, LoaderCircle, Plus, RefreshCw, ShieldCheck, Trash2, UploadCloud, X } from 'lucide-react'
import { api, formatBytes, formatNumber } from './lib/api'
import { Alert, AlertDescription } from './components/ui/alert'
import { Badge } from './components/ui/badge'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Skeleton } from './components/ui/skeleton'
import { Switch } from './components/ui/switch'

type Provider = { id: string; label: string; rclone_provider: string; endpoint_required: boolean; region_required: boolean; region_default?: string; create_bucket: boolean; docs: string; endpoints?: { id: string; label: string; endpoint: string }[] }
type S3Connection = { id: number; display_name: string; description: string; provider_type: string; endpoint: string; region: string; address_style: string; allow_private_endpoint: boolean; credential_present: boolean; last_rotated_at: number; status: string; last_test_at: number | null; last_error: string | null; created_at: number; updated_at: number }
type BucketRow = { id: number; bucket_name: string; display_name: string; description: string; enabled: boolean; verified_at: number | null }
type S3Object = { key: string; size_bytes: number; last_modified: string; etag?: string | null; storage_class?: string | null }
type BrowserPage = { objects: S3Object[]; prefixes: string[]; is_truncated: boolean; next_token: string | null; key_count: number }
type Summary = { connections: number; healthy_connections: number; buckets: number; mounted_buckets: number; contracts_due_soon: number; usage_bytes: number | null; quota_bytes: number | null; usage_status: string }
type Props = { Heading: (props: { title: string; description: string; action?: React.ReactNode }) => React.ReactNode; notify: (message: string) => void }
type Draft = { display_name: string; description: string; provider_type: string; endpoint: string; region: string; access_key_id: string; secret_access_key: string; session_token: string; address_style: string; allow_private_endpoint: boolean }
const emptyDraft: Draft = { display_name: '', description: '', provider_type: 'aws', endpoint: '', region: 'us-east-1', access_key_id: '', secret_access_key: '', session_token: '', address_style: 'auto', allow_private_endpoint: false }
const statusText: Record<string, string> = { unverified: 'تأییدنشده', connected: 'متصل', degraded: 'محدودیت دسترسی', failed: 'خطای اتصال' }
const statusVariant = (status: string) => status === 'connected' ? 'success' as const : status === 'degraded' ? 'warning' as const : status === 'failed' ? 'destructive' as const : 'outline' as const

export function S3StoragePage({ Heading, notify }: Props) {
  const [providers, setProviders] = useState<Provider[]>([])
  const [connections, setConnections] = useState<S3Connection[]>([])
  const [summary, setSummary] = useState<Summary | null>(null)
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [bucketData, setBucketData] = useState<{ provider_buckets: string[] | null; connections: BucketRow[] } | null>(null)
  const [remoteError, setRemoteError] = useState('')
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [step, setStep] = useState(0)
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [saving, setSaving] = useState(false)
  const [working, setWorking] = useState('')
  const [bucketName, setBucketName] = useState('')
  const [manualBucket, setManualBucket] = useState('')
  const [prefix, setPrefix] = useState('')
  const [searchPrefix, setSearchPrefix] = useState('')
  const [cursor, setCursor] = useState<string | null>(null)
  const [page, setPage] = useState<BrowserPage | null>(null)
  const [pageLoading, setPageLoading] = useState(false)
  const [pageError, setPageError] = useState('')
  const [privateConfirm, setPrivateConfirm] = useState(false)
  const uploadInput = useRef<HTMLInputElement>(null)

  const selected = connections.find((item) => item.id === selectedId) || null
  const provider = providers.find((item) => item.id === (selected?.provider_type || draft.provider_type))
  const load = useCallback(async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true)
    try {
      const [providerResult, connectionResult, summaryResult] = await Promise.all([
        api<{ providers: Provider[] }>('s3/providers'), api<{ connections: S3Connection[] }>('s3/connections'), api<Summary>('s3/summary'),
      ])
      setProviders(providerResult.providers); setConnections(connectionResult.connections); setSummary(summaryResult)
      setSelectedId((current) => current && connectionResult.connections.some((item) => item.id === current) ? current : connectionResult.connections[0]?.id ?? null)
    } catch (error) { notify(error instanceof Error ? error.message : 'دریافت وضعیت فضای S3 ناموفق بود.') }
    finally { setLoading(false); setRefreshing(false) }
  }, [notify])
  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (!selectedId) { setBucketData(null); setRemoteError(''); return }
    let cancelled = false
    setBucketData(null); setRemoteError(''); setPage(null); setBucketName(''); setPrefix(''); setCursor(null)
    api<{ provider_buckets: string[] | null; connections: BucketRow[] }>(`s3/connections/${selectedId}/buckets?local_only=1`).then((result) => { if (!cancelled) { setBucketData(result); setRemoteError('') } }).catch((error) => { if (!cancelled) { setBucketData({ provider_buckets: null, connections: [] }); setRemoteError(error instanceof Error ? error.message : 'فهرست اتصال‌های محلی خوانده نشد.') } })
    return () => { cancelled = true }
  }, [selectedId])

  const refreshObjects = async (nextPrefix = prefix, nextCursor: string | null = null) => {
    if (!selected || !bucketName) return
    setPageLoading(true); setPageError('')
    try {
      const query = new URLSearchParams({ bucket: bucketName, prefix: nextPrefix, max_keys: '200' })
      if (nextCursor) query.set('continuation_token', nextCursor)
      const result = await api<BrowserPage & { ok?: boolean }>(`s3/connections/${selected.id}/objects?${query}`)
      setPage(result); setPrefix(nextPrefix); setCursor(nextCursor)
    } catch (error) { setPageError(error instanceof Error ? error.message : 'فهرست Objectها دریافت نشد.') }
    finally { setPageLoading(false) }
  }

  const uploadFile = async (file?: File) => {
    if (!file || !selected || !bucketName) return
    const key = `${prefix}${file.name}`
    setWorking('upload')
    try {
      await api(`s3/connections/${selected.id}/objects/upload?${new URLSearchParams({ bucket: bucketName, key, content_type: file.type || 'application/octet-stream' })}`, { method: 'POST', body: file, headers: { 'Content-Type': file.type || 'application/octet-stream' } })
      notify('فایل با موفقیت بارگذاری شد.')
      await refreshObjects(prefix)
    } catch (error) { notify(error instanceof Error ? error.message : 'بارگذاری فایل ناموفق بود.') }
    finally { setWorking(''); if (uploadInput.current) uploadInput.current.value = '' }
  }

  const downloadUrl = (key: string) => selected ? `/api/s3/connections/${selected.id}/objects/download?${new URLSearchParams({ bucket: bucketName, key })}` : '#'
  const selectConnection = (id: number) => { setSelectedId(id); setPage(null) }
  const refreshBucketList = async () => {
    if (!selected) return
    setWorking('list-buckets'); setRemoteError('')
    try { const result = await api<{ provider_buckets: string[]; connections: BucketRow[] }>(`s3/connections/${selected.id}/buckets`); setBucketData(result); notify('فهرست Bucketهای سرویس‌دهنده دریافت شد.') }
    catch (error) { const message = error instanceof Error ? error.message : 'فهرست Bucketها دریافت نشد.'; setRemoteError(message); notify(message) }
    finally { setWorking('') }
  }
  const chooseProvider = (id: string) => {
    const next = { ...emptyDraft, provider_type: id }
    if (id === 'cloudflare_r2') next.region = 'auto'
    if (id === 'arvancloud') next.region = ''
    if (id === 'parspack' || id === 'generic' || id === 'minio') next.region = ''
    setDraft(next); setStep(1)
  }
  const createConnection = async (event: FormEvent) => {
    event.preventDefault()
    if (draft.allow_private_endpoint && !privateConfirm) { notify('برای Endpoint خصوصی، تأیید صریح مقصد لازم است.'); return }
    setSaving(true)
    try {
      const result = await api<{ connection: S3Connection }>('s3/connections', { method: 'POST', body: JSON.stringify(draft) })
      setDialogOpen(false); setDraft(emptyDraft); setStep(0); setPrivateConfirm(false)
      await load(true); setSelectedId(result.connection.id)
      notify('مشخصات اتصال ذخیره شد؛ برای تأیید واقعی دسترسی، اتصال را آزمایش کن.')
    } catch (error) { notify(error instanceof Error ? error.message : 'ذخیرهٔ اتصال ناموفق بود.') }
    finally { setSaving(false) }
  }
  const testConnection = async () => {
    if (!selected) return
    setWorking('test')
    try {
      const result = await api<{ result: { ok: boolean; status?: string; message?: string; error?: string; capabilities?: Record<string, boolean> } }>(`s3/connections/${selected.id}/test`, { method: 'POST', body: JSON.stringify({ bucket_name: bucketName || undefined }) })
      const state = result.result.status || (result.result.ok ? 'connected' : 'failed')
      notify(result.result.message || result.result.error || (state === 'connected' ? 'دسترسی اتصال تأیید شد.' : 'اتصال به بررسی بیشتر نیاز دارد.'))
      await load(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'آزمایش اتصال ناموفق بود.') }
    finally { setWorking('') }
  }
  const attachBucket = async (name: string) => {
    if (!selected || !/^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/.test(name)) { notify('نام Bucket معتبر نیست.'); return }
    setWorking('bucket')
    try {
      await api(`s3/connections/${selected.id}/buckets?local_only=1`, { method: 'POST', body: JSON.stringify({ bucket_name: name, display_name: name }) })
      setManualBucket(''); setBucketName(name); setBucketData(null)
      const result = await api<{ provider_buckets: string[]; connections: BucketRow[] }>(`s3/connections/${selected.id}/buckets?local_only=1`)
      setBucketData(result); notify('Bucket به فهرست ARG اضافه شد؛ دادهٔ سمت ارائه‌دهنده تغییر نکرده است.')
      await load(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'افزودن Bucket ناموفق بود.') }
    finally { setWorking('') }
  }
  const createBucket = async (name: string) => {
    if (!selected || name !== name.trim()) return
    setWorking('create-bucket')
    try {
      await api(`s3/connections/${selected.id}/buckets/create`, { method: 'POST', body: JSON.stringify({ bucket_name: name, confirm_name: name }) })
      setBucketName(name); setManualBucket(''); const result = await api<{ provider_buckets: string[]; connections: BucketRow[] }>(`s3/connections/${selected.id}/buckets?local_only=1`); setBucketData(result); notify('Bucket در سرویس‌دهنده ساخته شد.')
      await load(true)
    } catch (error) { notify(error instanceof Error ? error.message : 'ساخت Bucket ناموفق بود.') }
    finally { setWorking('') }
  }
  const detachBucket = async (item: BucketRow) => {
    if (!window.confirm(`فقط اتصال داخلی ARG به Bucket «${item.bucket_name}» قطع شود؟ فایل‌های ابری حذف نمی‌شوند.`)) return
    setWorking('bucket')
    try { await api(`s3/buckets/${item.id}`, { method: 'DELETE', body: JSON.stringify({ confirm_bucket: item.bucket_name }) }); if (bucketName === item.bucket_name) { setBucketName(''); setPage(null) }; const result = await api<{ provider_buckets: string[]; connections: BucketRow[] }>(`s3/connections/${selectedId}/buckets?local_only=1`); setBucketData(result); await load(true) }
    catch (error) { notify(error instanceof Error ? error.message : 'قطع اتصال Bucket ناموفق بود.') }
    finally { setWorking('') }
  }
  const deleteConnection = async () => {
    if (!selected) return
    const confirmation = window.prompt(`برای حذف اتصال ARG و کلید محلی، نام آن را وارد کن: ${selected.display_name}`)
    if (confirmation !== selected.display_name) return
    setWorking('delete')
    try { await api(`s3/connections/${selected.id}`, { method: 'DELETE', body: JSON.stringify({ confirm_name: confirmation }) }); setSelectedId(null); setBucketName(''); notify('اتصال و کلید محلی حذف شد؛ Bucketها و Objectهای ارائه‌دهنده دست‌نخورده‌اند.'); await load(true) }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف اتصال ناموفق بود.') }
    finally { setWorking('') }
  }
  const deleteObject = async (key: string) => {
    if (!selected || !bucketName) return
    const confirmation = window.prompt(`حذف دائمی این Object را با واردکردن کلید تأیید کن:\n${key}`)
    if (confirmation !== key) return
    setWorking('object-delete')
    try { await api(`s3/connections/${selected.id}/objects/delete`, { method: 'POST', body: JSON.stringify({ bucket: bucketName, key, confirm_key: confirmation }) }); notify('درخواست حذف Object به ارائه‌دهنده ارسال شد.'); await refreshObjects(prefix, cursor) }
    catch (error) { notify(error instanceof Error ? error.message : 'حذف Object ناموفق بود.') }
    finally { setWorking('') }
  }

  const headerAction = <div className="flex flex-wrap gap-2"><Button variant="outline" onClick={() => void load(true)} disabled={refreshing}><RefreshCw size={15} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button><Button onClick={() => { setDraft(emptyDraft); setStep(0); setDialogOpen(true) }}><Plus size={16}/>اتصال جدید</Button></div>
  return <>
    <Heading title="فضای ابری S3" description="اتصال امن به سرویس‌های S3-compatible، مدیریت Bucket و Objectها؛ فضای ابری از دیسک محلی جدا گزارش می‌شود." action={headerAction}/>
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
      <SummaryCard icon={Cloud} label="اتصال‌های سالم" value={summary ? `${formatNumber(summary.healthy_connections)} / ${formatNumber(summary.connections)}` : '—'} detail="وضعیت از آخرین آزمایش واقعی"/>
      <SummaryCard icon={Database} label="Bucketهای ثبت‌شده" value={summary ? formatNumber(summary.buckets) : '—'} detail="اتصال داخلی ARG؛ مستقل از شمار Bucketهای Provider"/>
      <SummaryCard icon={HardDrive} label="مصرف گزارش‌شده" value={summary?.usage_bytes === null || !summary ? 'نامشخص' : formatBytes(summary.usage_bytes)} detail="هنوز اندازه‌گیری نشده؛ عدد تخمینی نمایش داده نمی‌شود"/>
      <SummaryCard icon={Activity} label="تمدید نزدیک" value={summary ? formatNumber(summary.contracts_due_soon) : '—'} detail="قراردادهای 7 روز آینده"/>
    </div>
    {loading ? <div className="mt-4 grid gap-3 xl:grid-cols-[minmax(250px,.8fr)_minmax(0,2fr)]"><Skeleton className="h-64"/><Skeleton className="h-64"/></div> : connections.length === 0 ? <Card className="mt-4 rounded-[14px] border-border"><CardContent className="grid justify-items-center px-5 py-12 text-center"><span className="grid h-14 w-14 place-items-center rounded-2xl border border-primary/20 bg-primary/[.07] text-primary"><Cloud size={27}/></span><h2 className="mt-4 text-lg font-semibold">هنوز فضای S3 اضافه نشده</h2><p className="mt-2 max-w-lg text-sm text-muted-foreground">یک سرویس موجود را متصل کن. ARG حساب تجاری یا Bucket را برایت خریداری نمی‌کند و کلیدهای دسترسی را در دیتابیس ذخیره نمی‌کند.</p><Button className="mt-5" onClick={() => setDialogOpen(true)}><Plus size={16}/>اتصال فضای ابری</Button></CardContent></Card> : <div className="mt-4 grid min-w-0 gap-4 xl:grid-cols-[minmax(250px,.75fr)_minmax(0,2fr)]">
      <section className="min-w-0 space-y-2" aria-label="اتصال‌های S3">{connections.map((connection) => <button key={connection.id} type="button" onClick={() => selectConnection(connection.id)} aria-pressed={selectedId === connection.id} className={`w-full rounded-[14px] border p-4 text-right transition-colors ${selectedId === connection.id ? 'border-primary/40 bg-primary/[.06]' : 'border-border bg-card/70 hover:bg-card'}`}><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="truncate font-semibold text-foreground">{connection.display_name}</p><p className="mt-1 text-[11px] text-muted-foreground">{providers.find((item) => item.id === connection.provider_type)?.label || connection.provider_type}</p></div><Badge variant={statusVariant(connection.status)}>{statusText[connection.status] || statusText.unverified}</Badge></div><p className="mt-3 truncate text-left font-mono text-[10px] text-muted-foreground" dir="ltr">{connection.endpoint || `Amazon S3 · ${connection.region}`}</p><div className="mt-3 flex items-center justify-between text-[10px] text-muted-foreground"><span className="flex items-center gap-1"><KeyRound size={12}/>{connection.credential_present ? 'کلید ذخیره‌شده' : 'کلید موجود نیست'}</span><span>{bucketData?.connections.length || 0} Bucket</span></div></button>)}</section>
      {selected ? <section className="min-w-0 space-y-4">
        <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><CardTitle className="text-lg">{selected.display_name}</CardTitle><Badge variant={statusVariant(selected.status)}>{statusText[selected.status] || statusText.unverified}</Badge></div><CardDescription className="mt-1">{selected.description || providers.find((item) => item.id === selected.provider_type)?.label}</CardDescription></div><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" onClick={() => void testConnection()} disabled={!!working}><RefreshCw size={14} className={working === 'test' ? 'animate-spin' : ''}/>بررسی اتصال</Button><Button size="sm" variant="outline" className="text-destructive" onClick={() => void deleteConnection()} disabled={!!working}><Trash2 size={14}/>حذف اتصال</Button></div></CardHeader><CardContent className="grid gap-3 border-t border-border p-4 sm:grid-cols-2 lg:grid-cols-4"><InfoCell label="Provider" value={providers.find((item) => item.id === selected.provider_type)?.label || selected.provider_type}/><InfoCell label="Endpoint" value={selected.endpoint || 'AWS endpoint پیش‌فرض'} ltr/><InfoCell label="Signing Region" value={selected.region || 'گزارش نشده'} ltr/><InfoCell label="اعتبارنامه" value={`ذخیره‌شده · چرخش ${new Date(selected.last_rotated_at * 1000).toISOString().slice(0,10)}`}/>{selected.last_error && <p className="sm:col-span-2 lg:col-span-4 rounded-lg border border-amber-400/20 bg-amber-400/[.05] p-3 text-xs text-amber-100">{selected.last_error}</p>}</CardContent></Card>
        <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-center justify-between gap-2 space-y-0 p-4"><div><CardTitle className="text-sm">Bucketها</CardTitle><CardDescription className="mt-1">فهرست راه‌دور فقط با درخواست صریح و مجوز ListBuckets خوانده می‌شود.</CardDescription></div><div className="flex items-center gap-3"><Button size="sm" variant="outline" onClick={() => void refreshBucketList()} disabled={!!working}><RefreshCw size={13} className={working === 'list-buckets' ? 'animate-spin' : ''}/>بررسی فهرست Provider</Button><a className="flex items-center gap-1 text-xs text-muted-foreground hover:text-primary" href={providers.find((item) => item.id === selected.provider_type)?.docs} target="_blank" rel="noreferrer">راهنمای Provider <ExternalLink size={12}/></a></div></CardHeader><CardContent className="space-y-3 p-4 pt-0">
          {remoteError && <Alert className="border-amber-400/20 bg-amber-400/[.04]"><AlertDescription>{remoteError} اگر کلید به یک Bucket محدود است، نام را دستی اضافه کن؛ اتصال به‌خاطر نداشتن ListBuckets رد نمی‌شود.</AlertDescription></Alert>}
          {!bucketData ? <Skeleton className="h-16"/> : bucketData.provider_buckets === null ? <p className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground">برای دیدن Bucketهای سرویس‌دهنده، «بررسی فهرست Provider» را بزن. اگر ListBuckets مجاز نباشد می‌توانی نام Bucket را دستی وارد کنی.</p> : <div className="flex flex-wrap gap-2">{(bucketData.provider_buckets || []).map((name) => { const attached = bucketData.connections.some((item) => item.bucket_name === name); return <div key={name} className={`flex items-center gap-1 rounded-lg border px-2 py-1 ${bucketName === name ? 'border-primary/40 bg-primary/[.07]' : 'border-border bg-background/40'}`}><button type="button" className="flex items-center gap-2 px-1 py-1 text-xs" onClick={() => { setBucketName(name); setPrefix(''); setCursor(null); setPage(null) }}><Database size={13} className="text-primary"/><span className="font-mono" dir="ltr">{name}</span></button>{attached ? <Badge variant="outline">متصل در ARG</Badge> : <Button size="sm" variant="ghost" className="h-7 px-2 text-[10px]" onClick={() => void attachBucket(name)} disabled={!!working}>افزودن</Button>}</div>})}</div>}
          {bucketData?.connections.length ? <div className="divide-y divide-border rounded-lg border border-border">{bucketData.connections.map((bucket) => <div key={bucket.id} className="flex items-center gap-2 p-2.5"><button type="button" className="flex min-w-0 flex-1 items-center gap-2 text-right text-xs" onClick={() => { setBucketName(bucket.bucket_name); setPrefix(''); setPage(null) }}><Database size={14} className="shrink-0 text-primary"/><span className="truncate">{bucket.display_name}</span><code className="truncate text-muted-foreground" dir="ltr">{bucket.bucket_name}</code></button><Button size="icon" variant="ghost" aria-label={`قطع اتصال ${bucket.bucket_name} از ARG`} onClick={() => void detachBucket(bucket)} disabled={!!working}><X size={14}/></Button></div>)}</div> : <p className="text-xs text-muted-foreground">Bucketی در ARG متصل نشده است.</p>}
          <form className="flex flex-wrap gap-2" onSubmit={(event) => { event.preventDefault(); void attachBucket(manualBucket) }}><Input aria-label="نام Bucket موجود" dir="ltr" className="min-w-[180px] flex-1" placeholder="نام Bucket موجود را وارد کن" value={manualBucket} onChange={(event) => setManualBucket(event.target.value)}/><Button type="submit" variant="outline" disabled={!manualBucket || !!working}><Plus size={14}/>اتصال موجود</Button>{provider?.create_bucket && <Button type="button" variant="outline" disabled={!manualBucket || !!working} onClick={() => void createBucket(manualBucket)}>ساخت Bucket</Button>}</form>
        </CardContent></Card>
        {bucketName && <Card className="min-w-0 rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-center justify-between gap-3 space-y-0 p-4"><div className="min-w-0"><CardTitle className="text-sm">مدیریت Objectها</CardTitle><CardDescription className="mt-1 truncate font-mono" dir="ltr">{bucketName}{prefix}</CardDescription></div><div className="flex gap-2"><Button size="sm" variant="outline" onClick={() => void refreshObjects(prefix)} disabled={pageLoading}><RefreshCw size={13} className={pageLoading ? 'animate-spin' : ''}/>تازه‌سازی</Button><input ref={uploadInput} type="file" className="hidden" aria-label="انتخاب فایل برای بارگذاری" onChange={(event) => void uploadFile(event.target.files?.[0])}/><Button size="sm" variant="outline" onClick={() => uploadInput.current?.click()} disabled={!!working}><UploadCloud size={14}/>{working === 'upload' ? 'در حال بارگذاری…' : 'آپلود'}</Button></div></CardHeader><CardContent className="space-y-3 p-4 pt-0">
          <form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); void refreshObjects(searchPrefix) }}><Input dir="ltr" value={searchPrefix} onChange={(event) => setSearchPrefix(event.target.value)} placeholder="Prefix / مسیر مجازی"/><Button size="sm" variant="outline" type="submit" disabled={pageLoading}>بازکردن</Button></form>
          {pageError && <Alert className="border-destructive/20"><AlertDescription>{pageError}</AlertDescription></Alert>}
          {!page && !pageLoading ? <div className="rounded-lg border border-dashed border-border p-8 text-center text-sm text-muted-foreground">برای دریافت فهرست این Bucket، دکمهٔ «تازه‌سازی» را بزن.</div> : pageLoading && !page ? <Skeleton className="h-48"/> : page && <><div className="divide-y divide-border overflow-hidden rounded-lg border border-border">{prefix && <button type="button" className="flex w-full items-center gap-2 p-3 text-right text-xs text-primary hover:bg-background/60" onClick={() => { const parent = prefix.replace(/\/$/, '').split('/').slice(0,-1).join('/'); void refreshObjects(parent ? `${parent}/` : '') }}><ChevronRight size={14}/>یک سطح بالاتر</button>}{page.prefixes.map((item) => <button type="button" key={item} className="flex w-full items-center gap-2 p-3 text-right text-xs hover:bg-background/60" onClick={() => void refreshObjects(item)}><Folder size={15} className="text-primary"/><span className="truncate font-mono" dir="ltr">{item.slice(prefix.length)}</span></button>)}{page.objects.map((object) => <div key={object.key} className="flex min-w-0 items-center gap-2 p-3 text-xs"><span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-secondary/70"><Database size={14}/></span><span className="min-w-0 flex-1 truncate font-mono" dir="ltr" title={object.key}>{object.key.slice(prefix.length)}</span><span className="shrink-0 font-mono text-muted-foreground" dir="ltr">{formatBytes(object.size_bytes)}</span><a href={downloadUrl(object.key)} aria-label={`دانلود ${object.key}`} className="grid h-8 w-8 place-items-center rounded-md text-muted-foreground hover:bg-secondary hover:text-foreground"><ArrowDownToLine size={14}/></a><Button size="icon" variant="ghost" aria-label={`حذف ${object.key}`} onClick={() => void deleteObject(object.key)} disabled={!!working}><Trash2 size={14} className="text-destructive"/></Button></div>)}{!page.objects.length && !page.prefixes.length && <p className="p-6 text-center text-xs text-muted-foreground">در این Prefix آبجکتی نیست.</p>}</div><div className="flex items-center justify-between text-xs text-muted-foreground"><span>{formatNumber(page.key_count)} مورد در این صفحه</span><Button size="sm" variant="outline" disabled={!page.is_truncated || !page.next_token || pageLoading} onClick={() => void refreshObjects(prefix,page.next_token)}>صفحهٔ بعدی <ChevronLeft size={14}/></Button></div></>}</CardContent></Card>}
      </section> : <Card className="rounded-[14px] border-border"><CardContent className="p-10 text-center text-sm text-muted-foreground">برای مشاهدهٔ جزئیات، یک اتصال را انتخاب کن.</CardContent></Card>}
    </div>}

    <Dialog open={dialogOpen} onOpenChange={(open) => { setDialogOpen(open); if (!open) { setPrivateConfirm(false); setStep(0) } }}>
      <DialogContent className="max-h-[92vh] max-w-2xl overflow-y-auto">
        <DialogHeader><DialogTitle>اتصال فضای S3</DialogTitle><DialogDescription>مرحلهٔ {step + 1} از 2 · کلیدها فقط در فایل محافظت‌شدهٔ سرور ذخیره می‌شوند.</DialogDescription></DialogHeader>
        {step === 0 ? <div className="grid gap-2 sm:grid-cols-2">{providers.map((item) => <button type="button" key={item.id} onClick={() => chooseProvider(item.id)} className="rounded-xl border border-border bg-background/40 p-4 text-right transition-colors hover:border-primary/40 hover:bg-primary/[.04]"><div className="flex items-center justify-between"><span className="font-semibold">{item.label}</span><Cloud size={17} className="text-primary"/></div><p className="mt-2 text-xs text-muted-foreground">{item.endpoint_required ? 'Endpoint اختصاصی سرویس لازم است' : 'اتصال به Endpoint پیش‌فرض AWS'}</p></button>)}</div> : <form onSubmit={(event) => void createConnection(event)} className="space-y-4">
          <div className="flex items-center justify-between rounded-lg border border-border bg-background/40 p-3 text-xs"><span>ارائه‌دهنده</span><b>{provider?.label || draft.provider_type}</b><a href={provider?.docs} target="_blank" rel="noreferrer" className="flex items-center gap-1 text-primary">راهنما <ExternalLink size={12}/></a></div>
          {draft.provider_type === 'arvancloud' && provider?.endpoints && <div className="space-y-2"><Label htmlFor="arvan-endpoint">Endpoint منطقه</Label><select id="arvan-endpoint" className="h-10 w-full rounded-lg border border-border bg-background px-3 text-sm" value={provider.endpoints.find((item) => item.endpoint === draft.endpoint)?.endpoint || 'custom'} onChange={(event) => { const endpoint = event.target.value === 'custom' ? '' : event.target.value; setDraft((value) => ({ ...value, endpoint })) }}><option value="custom">Endpoint سفارشی</option>{provider.endpoints.map((endpoint) => <option key={endpoint.id} value={endpoint.endpoint}>{endpoint.label} · {endpoint.endpoint}</option>)}</select></div>}
          <div className="grid gap-3 sm:grid-cols-2"><Field label="نام نمایشی اتصال" value={draft.display_name} onChange={(value) => setDraft((old) => ({ ...old, display_name: value }))} placeholder="مثلاً Backup Archive"/><Field label="Bucket موجود (اختیاری)" value={bucketName} onChange={setBucketName} placeholder="my-backup-bucket" ltr/></div>
          {provider?.endpoint_required && <Field label={draft.provider_type === 'parspack' ? 'Endpoint از پنل پارس‌پک' : 'Endpoint URL'} value={draft.endpoint} onChange={(value) => setDraft((old) => ({ ...old, endpoint: value }))} placeholder={draft.provider_type === 'arvancloud' ? 'Endpoint سفارشی HTTPS' : 'https://s3.example.com'} ltr help={draft.provider_type === 'parspack' ? 'Endpoint ثابت فرض نمی‌شود؛ آدرس را از پنل پارس‌پک بردار.' : 'HTTPS origin؛ مسیر، query و credential در URL پذیرفته نمی‌شود.'}/>}
          <div className="grid gap-3 sm:grid-cols-2"><Field label={draft.provider_type === 'arvancloud' ? 'Signing Region (از مستندات سرویس)' : 'Region'} value={draft.region} onChange={(value) => setDraft((old) => ({ ...old, region: value }))} placeholder={draft.provider_type === 'cloudflare_r2' ? 'auto' : 'Region را از Provider بگیر'} ltr/><div className="space-y-2"><Label htmlFor="s3-address-style">Addressing Style</Label><select id="s3-address-style" className="h-10 w-full rounded-lg border border-border bg-background px-3 text-sm" value={draft.address_style} onChange={(event) => setDraft((old) => ({ ...old, address_style: event.target.value }))}><option value="auto">خودکار</option><option value="path">Path-style</option><option value="virtual">Virtual-hosted</option></select></div></div>
          <div className="grid gap-3 sm:grid-cols-2"><Field label="Access Key ID" value={draft.access_key_id} onChange={(value) => setDraft((old) => ({ ...old, access_key_id: value }))} ltr/><Field label="Secret Access Key" value={draft.secret_access_key} onChange={(value) => setDraft((old) => ({ ...old, secret_access_key: value }))} type="password" ltr/></div>
          <Field label="Session Token (اختیاری)" value={draft.session_token} onChange={(value) => setDraft((old) => ({ ...old, session_token: value }))} type="password" ltr/>
          <label className="flex items-start gap-3 rounded-lg border border-border p-3"><Switch checked={draft.allow_private_endpoint} onCheckedChange={(value) => { setDraft((old) => ({ ...old, allow_private_endpoint: value })); setPrivateConfirm(false) }}/><span className="text-xs leading-5"><b className="block">Endpoint خصوصی / شبکهٔ داخلی</b><span className="text-muted-foreground">فقط برای MinIO یا مقصدی که خودت بررسی کرده‌ای. Endpointهای metadata همیشه مسدود می‌مانند.</span></span></label>
          {draft.allow_private_endpoint && <label className="flex items-center gap-2 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs"><input type="checkbox" checked={privateConfirm} onChange={(event) => setPrivateConfirm(event.target.checked)}/>مقصد خصوصی را بررسی کرده‌ام و اجازهٔ اتصال ARG به آن را می‌دهم.</label>}
          <div className="flex justify-between gap-2"><Button type="button" variant="outline" onClick={() => setStep(0)}><ChevronRight size={15}/>بازگشت</Button><Button type="submit" disabled={saving || !draft.display_name || !draft.access_key_id || !draft.secret_access_key || !!provider?.endpoint_required && !draft.endpoint || !!provider?.region_required && !draft.region}>{saving ? <LoaderCircle className="animate-spin" size={15}/> : <ShieldCheck size={15}/>}ذخیرهٔ امن اتصال</Button></div>
        </form>}
      </DialogContent>
    </Dialog>
  </>
}

function SummaryCard({ icon: Icon, label, value, detail }: { icon: typeof Cloud; label: string; value: string; detail: string }) {
  return <Card className="rounded-[13px] border-border bg-card/80"><CardContent className="p-4"><div className="flex items-center justify-between gap-2"><span className="text-xs text-muted-foreground">{label}</span><Icon size={16} className="text-primary" aria-hidden="true"/></div><p className="mt-3 font-mono text-xl font-semibold tabular-nums text-foreground" dir="ltr">{value}</p><p className="mt-1 text-[10px] leading-4 text-muted-foreground">{detail}</p></CardContent></Card>
}
function InfoCell({ label, value, ltr = false }: { label: string; value: string; ltr?: boolean }) {
  return <div className="min-w-0 rounded-lg border border-border/70 bg-background/30 p-3"><p className="text-[10px] text-muted-foreground">{label}</p><p className={`mt-1 truncate text-xs text-foreground ${ltr ? 'text-left font-mono' : ''}`} dir={ltr ? 'ltr' : undefined} title={value}>{value}</p></div>
}
function Field({ label, value, onChange, placeholder, ltr = false, type = 'text', help }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string; ltr?: boolean; type?: string; help?: string }) {
  return <div className="space-y-1.5"><Label>{label}</Label><Input type={type} dir={ltr ? 'ltr' : undefined} autoComplete="off" value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder}/>{help && <p className="text-[10px] leading-4 text-muted-foreground">{help}</p>}</div>
}
