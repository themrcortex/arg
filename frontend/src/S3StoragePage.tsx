import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
import { ArrowDownToLine, ChevronLeft, ChevronRight, Cloud, Database, ExternalLink, Folder, HardDrive, KeyRound, LoaderCircle, Plus, RefreshCw, ShieldCheck, Trash2, UploadCloud, X } from 'lucide-react'
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

type Provider = { id: string; label: string; rclone_provider: string; endpoint_required: boolean; region_required: boolean; region_default?: string; create_bucket: boolean; docs: string; endpoints?: { id: string; label: string; endpoint: string; region?: string }[] }
type S3Connection = { id: number; display_name: string; description: string; provider_type: string; endpoint: string; region: string; address_style: string; allow_private_endpoint: boolean; credential_present: boolean; last_rotated_at: number; status: string; last_test_at: number | null; last_error: string | null; created_at: number; updated_at: number }
type BucketRow = { id: number; bucket_name: string; display_name: string; description: string; enabled: boolean; verified_at: number | null; used_bytes: number | null; quota_bytes: number | null; usage_source: string | null; scan_status: string | null; measured_at: number | null }
type S3Object = { key: string; size_bytes: number; last_modified: string; etag?: string | null; storage_class?: string | null }
type BrowserPage = { objects: S3Object[]; prefixes: string[]; is_truncated: boolean; next_token: string | null; key_count: number }
type MountInfo = { id: number; connection_id: number; bucket_id: number; bucket: string; mount_path: string; method: 'rclone'; read_only: boolean; auto_mount: boolean; status: 'mounted' | 'unmounted' | 'failed' | 'mounting'; last_error: string | null; capacity: 'unknown'; capacity_note: string }
type ServerEntry = { name: string; path: string; kind: 'directory' | 'file' | 'symlink' | 'other'; size: number; permissions: string }
type ServerListing = { path: string; parent: string | null; entries: ServerEntry[]; truncated: boolean }
type TransferFile = { path: string; relative_path: string; size: number }
const MAX_SERVER_TRANSFER_FILES = 10000
const MAX_SERVER_TRANSFER_BYTES = 20 * 1024 * 1024 * 1024
const MAX_SERVER_TRANSFER_FILE_BYTES = 512 * 1024 * 1024
type Props = { Heading: (props: { title: string; description: string; action?: React.ReactNode }) => React.ReactNode; notify: (message: string) => void }
type Draft = { display_name: string; description: string; provider_type: string; endpoint: string; region: string; access_key_id: string; secret_access_key: string; session_token: string; address_style: string; allow_private_endpoint: boolean }
const emptyDraft: Draft = { display_name: '', description: '', provider_type: 'aws', endpoint: '', region: 'us-east-1', access_key_id: '', secret_access_key: '', session_token: '', address_style: 'auto', allow_private_endpoint: false }
const statusText: Record<string, string> = { unverified: 'تأییدنشده', connected: 'متصل', degraded: 'محدودیت دسترسی', failed: 'خطای اتصال' }
const statusVariant = (status: string) => status === 'connected' ? 'success' as const : status === 'degraded' ? 'warning' as const : status === 'failed' ? 'destructive' as const : 'outline' as const

export function S3StoragePage({ Heading, notify }: Props) {
  const [providers, setProviders] = useState<Provider[]>([])
  const [connections, setConnections] = useState<S3Connection[]>([])
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [bucketData, setBucketData] = useState<{ provider_buckets: string[] | null; connections: BucketRow[] } | null>(null)
  const [remoteError, setRemoteError] = useState('')
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [step, setStep] = useState(0)
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [saving, setSaving] = useState(false)
  const [working, setWorking] = useState('')
  const [bucketName, setBucketName] = useState('')
  const [manualBucket, setManualBucket] = useState('')
  const [usageDialogOpen, setUsageDialogOpen] = useState(false)
  const [usedSizeGib, setUsedSizeGib] = useState('')
  const [quotaSizeGib, setQuotaSizeGib] = useState('')
  const [prefix, setPrefix] = useState('')
  const [searchPrefix, setSearchPrefix] = useState('')
  const [cursor, setCursor] = useState<string | null>(null)
  const [page, setPage] = useState<BrowserPage | null>(null)
  const [pageLoading, setPageLoading] = useState(false)
  const [pageError, setPageError] = useState('')
  const [privateConfirm, setPrivateConfirm] = useState(false)
  const [mount, setMount] = useState<MountInfo | null>(null)
  const [mountReadOnly, setMountReadOnly] = useState(true)
  const [mountAutoStart, setMountAutoStart] = useState(false)
  const [transferOpen, setTransferOpen] = useState(false)
  const [transferDirection, setTransferDirection] = useState<'upload' | 'download'>('upload')
  const [transferPath, setTransferPath] = useState('')
  const [transferKey, setTransferKey] = useState('')
  const [folderFiles, setFolderFiles] = useState<TransferFile[]>([])
  const [transferIsDirectory, setTransferIsDirectory] = useState(false)
  const [folderScanBusy, setFolderScanBusy] = useState(false)
  const [transferProgress, setTransferProgress] = useState<{ done: number; total: number; current?: string; error?: string } | null>(null)
  const [moveSource, setMoveSource] = useState(false)
  const [overwriteExisting, setOverwriteExisting] = useState(false)
  const [serverPath, setServerPath] = useState('/')
  const [serverListing, setServerListing] = useState<ServerListing | null>(null)
  const [serverBrowseLoading, setServerBrowseLoading] = useState(false)
  const [serverBrowseError, setServerBrowseError] = useState('')
  const uploadInput = useRef<HTMLInputElement>(null)

  const selected = connections.find((item) => item.id === selectedId) || null
  const selectedBucket = bucketData?.connections.find((item) => item.bucket_name === bucketName) || null
  const provider = providers.find((item) => item.id === (selected?.provider_type || draft.provider_type))
  const load = useCallback(async (quiet = false) => {
    if (quiet) setRefreshing(true); else setLoading(true)
    try {
      const [providerResult, connectionResult] = await Promise.all([
        api<{ providers: Provider[] }>('s3/providers'), api<{ connections: S3Connection[] }>('s3/connections'),
      ])
      setProviders(providerResult.providers); setConnections(connectionResult.connections)
      setSelectedId((current) => current && connectionResult.connections.some((item) => item.id === current) ? current : null)
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
  useEffect(() => {
    let cancelled = false
    setMount(null)
    if (!selectedBucket) return () => { cancelled = true }
    api<{ mount: MountInfo | null }>(`s3/buckets/${selectedBucket.id}/mount`).then((result) => {
      if (!cancelled) {
        setMount(result.mount)
        if (result.mount) { setMountReadOnly(result.mount.read_only); setMountAutoStart(result.mount.auto_mount) }
      }
    }).catch(() => { if (!cancelled) setMount(null) })
    return () => { cancelled = true }
  }, [selectedBucket?.id])

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
    if (draft.allow_private_endpoint && !privateConfirm && (!editingId || !selected?.allow_private_endpoint)) { notify('برای Endpoint خصوصی، تأیید صریح مقصد لازم است.'); return }
    setSaving(true)
    try {
      const result = await api<{ connection: S3Connection }>(editingId ? `s3/connections/${editingId}` : 's3/connections', { method: editingId ? 'PUT' : 'POST', body: JSON.stringify(draft) })
      setDialogOpen(false); setDraft(emptyDraft); setStep(0); setPrivateConfirm(false); setEditingId(null)
      await load(true); setSelectedId(result.connection.id)
      notify(editingId ? 'تنظیمات اتصال ذخیره شد.' : 'مشخصات اتصال ذخیره شد؛ برای تأیید واقعی دسترسی، اتصال را آزمایش کن.')
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
  const openUsageEditor = () => {
    setUsedSizeGib(selectedBucket?.used_bytes != null ? (selectedBucket.used_bytes / 1024 ** 3).toFixed(2) : '')
    setQuotaSizeGib(selectedBucket?.quota_bytes != null ? (selectedBucket.quota_bytes / 1024 ** 3).toFixed(2) : '')
    setUsageDialogOpen(true)
  }
  const saveManualUsage = async (event: FormEvent) => {
    event.preventDefault()
    if (!selectedBucket || !selectedId) return
    const usedGib = Number(usedSizeGib)
    const quotaGib = quotaSizeGib.trim() ? Number(quotaSizeGib) : null
    const maxGib = Number.MAX_SAFE_INTEGER / (1024 ** 3)
    if (!Number.isFinite(usedGib) || usedGib < 0 || usedGib > maxGib || (quotaGib !== null && (!Number.isFinite(quotaGib) || quotaGib <= 0 || quotaGib > maxGib))) {
      notify('حجم مصرفی و سقف را به GiB معتبر وارد کن؛ سقف را اگر نمی‌دانی خالی بگذار.')
      return
    }
    setWorking('usage')
    try {
      await api(`s3/buckets/${selectedBucket.id}/usage`, { method: 'PUT', body: JSON.stringify({ used_bytes: Math.round(usedGib * 1024 ** 3), quota_bytes: quotaGib === null ? null : Math.round(quotaGib * 1024 ** 3) }) })
      const result = await api<{ provider_buckets: string[] | null; connections: BucketRow[] }>(`s3/connections/${selectedId}/buckets?local_only=1`)
      setBucketData(result); setUsageDialogOpen(false)
      notify('اندازهٔ Bucket با برچسب «ثبت دستی» ذخیره شد.')
    } catch (error) { notify(error instanceof Error ? error.message : 'ثبت اندازهٔ Bucket انجام نشد.') }
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
  const editConnection = () => {
    if (!selected) return
    setEditingId(selected.id)
    setDraft({ display_name: selected.display_name, description: selected.description, provider_type: selected.provider_type, endpoint: selected.endpoint, region: selected.region, access_key_id: '', secret_access_key: '', session_token: '', address_style: selected.address_style, allow_private_endpoint: selected.allow_private_endpoint })
    setStep(1); setPrivateConfirm(false); setDialogOpen(true)
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

  const configureMount = async () => {
    if (!selectedBucket) return
    setWorking('mount')
    try {
      const result = await api<{ mount: MountInfo }>(`s3/buckets/${selectedBucket.id}/mount`, { method: 'PUT', body: JSON.stringify({ read_only: mountReadOnly, auto_mount: mountAutoStart }) })
      setMount(result.mount)
      notify(`Bucket روی ${result.mount.mount_path} متصل شد. فضای S3 به ظرفیت فیزیکی دیسک اضافه نمی‌شود.`)
    } catch (error) { notify(error instanceof Error ? error.message : 'Mount فضای ابری انجام نشد.') }
    finally { setWorking('') }
  }
  const stopMount = async () => {
    if (!mount) return
    if (!mount.read_only && !window.confirm('پیش از Unmount مطمئن شو برنامه‌ای در حال نوشتن روی Bucket نیست. ادامه می‌دهی؟')) return
    setWorking('mount')
    try { await api(`s3/mounts/${mount.id}/stop`, { method: 'POST' }); setMount({ ...mount, status: 'unmounted' }); notify('Mount متوقف شد؛ داده‌های Bucket حذف نشدند.') }
    catch (error) { notify(error instanceof Error ? error.message : 'توقف Mount ناموفق بود.') }
    finally { setWorking('') }
  }
  const browseServerPath = async (path: string) => {
    if (!path.startsWith('/')) return
    setServerBrowseLoading(true); setServerBrowseError('')
    try { const result = await api<ServerListing>(`files?path=${encodeURIComponent(path)}`); setServerListing(result); setServerPath(result.path) }
    catch (error) { setServerBrowseError(error instanceof Error ? error.message : 'خواندن پوشه ناموفق بود.') }
    finally { setServerBrowseLoading(false) }
  }
  const openTransfer = (direction: 'upload' | 'download', key = '') => {
    setTransferDirection(direction); setTransferKey(key); setTransferPath(''); setFolderFiles([]); setTransferIsDirectory(false); setTransferProgress(null); setMoveSource(false); setOverwriteExisting(false); setServerPath('/'); setServerListing(null); setTransferOpen(true); void browseServerPath('/')
  }
  const selectCurrentFolder = async () => {
    if (!serverPath || serverPath === '/') { setServerBrowseError('پوشهٔ ریشه را برای انتقال انتخاب نکن؛ یک پوشهٔ مشخص را باز کن.'); return }
    if (!serverListing || serverListing.path !== serverPath) { setServerBrowseError('ابتدا همین مسیر را باز کن تا محتویات واقعی آن بررسی شود.'); return }
    setFolderScanBusy(true); setServerBrowseError(''); setTransferIsDirectory(true); setTransferPath(serverPath); setFolderFiles([]); setMoveSource(false)
    const files: TransferFile[] = []
    let totalBytes = 0
    let visitedDirectories = 0
    const rootName = serverPath.replace(/\/+$/, '').split('/').filter(Boolean).at(-1) || 'folder'
    const visit = async (path: string, relative: string, depth: number): Promise<void> => {
      if (depth > 64) throw new Error('عمق پوشه از سقف مجاز 64 سطح بیشتر است.')
      visitedDirectories += 1
      if (visitedDirectories > MAX_SERVER_TRANSFER_FILES) throw new Error('تعداد پوشه‌ها از سقف بررسی بیشتر است؛ درخت را کوچک‌تر انتخاب کن.')
      const listing = await api<ServerListing>(`files?path=${encodeURIComponent(path)}`)
      if (listing.truncated) throw new Error(`محتویات ${path} بیش از سقف قابل بررسی است؛ پوشه را به بخش‌های کوچک‌تر تقسیم کن.`)
      for (const entry of listing.entries) {
        if (entry.kind === 'symlink' || entry.kind === 'other') continue
        const childRelative = relative ? `${relative}/${entry.name}` : entry.name
        if (entry.kind === 'directory') {
          await visit(entry.path, childRelative, depth + 1)
          continue
        }
        if (entry.kind !== 'file') continue
        if (/[\u0000-\u001f\u007f]/.test(childRelative) || new TextEncoder().encode(`${prefix}${rootName}/${childRelative}`).length > 1024) throw new Error(`مسیر Object برای ${entry.path} معتبر نیست یا از 1024 بایت بیشتر است.`)
        if (entry.size > MAX_SERVER_TRANSFER_FILE_BYTES) throw new Error(`فایل ${entry.path} از سقف 512 MiB بزرگ‌تر است.`)
        totalBytes += entry.size
        if (files.length >= MAX_SERVER_TRANSFER_FILES || totalBytes > MAX_SERVER_TRANSFER_BYTES) throw new Error('سقف پوشه 10000 فایل و 20 GiB است؛ پوشه را به چند بخش تقسیم کن.')
        files.push({ path: entry.path, relative_path: childRelative, size: entry.size })
      }
    }
    try {
      await visit(serverPath, '', 0)
      if (!files.length) throw new Error('این پوشه فایل قابل انتقال ندارد؛ symlinkها نادیده گرفته می‌شوند.')
      setTransferPath(serverPath)
      setTransferKey(`${prefix}${rootName}/`)
      setFolderFiles(files)
      setMoveSource(false)
      setTransferProgress(null)
    } catch (error) {
      setFolderFiles([])
      setServerBrowseError(error instanceof Error ? error.message : 'بررسی پوشه ناموفق بود.')
    } finally { setFolderScanBusy(false) }
  }
  const chooseServerEntry = (entry: ServerEntry) => {
    if (entry.kind === 'directory') {
      void browseServerPath(entry.path)
      if (transferDirection === 'download') {
        const filename = transferKey.split('/').filter(Boolean).at(-1) || 'object'
        setTransferPath(`${entry.path.replace(/\/$/, '')}/${filename}`)
      }
    } else if (transferDirection === 'upload' && entry.kind === 'file') {
      setFolderFiles([]); setTransferIsDirectory(false); setTransferProgress(null)
      setTransferPath(entry.path)
      setTransferKey(`${prefix}${entry.name}`)
    }
  }
  const runServerTransfer = async (event?: { preventDefault: () => void }) => {
    event?.preventDefault()
    if (!selected || !bucketName || !transferPath.startsWith('/')) return
    setWorking('transfer'); setTransferProgress(null)
    let completedFiles = transferIsDirectory && transferProgress?.error && transferProgress.total === folderFiles.length ? transferProgress.done : 0
    try {
      const action = transferDirection === 'upload' ? 'local-to-bucket' : 'bucket-to-local'
      let transferredBytes = 0
      if (transferDirection === 'upload' && transferIsDirectory && folderFiles.length) {
        const resumeFrom = completedFiles
        setTransferProgress({ done: resumeFrom, total: folderFiles.length, current: folderFiles[resumeFrom]?.relative_path })
        for (const [index, file] of folderFiles.entries()) {
          if (index < resumeFrom) { transferredBytes += file.size; continue }
          const key = `${transferKey.replace(/\/+$/, '')}/${file.relative_path}`
          await api<{ size_bytes: number }>(`s3/connections/${selected.id}/transfer/local-to-bucket`, { method: 'POST', body: JSON.stringify({ bucket: bucketName, key, local_path: file.path, overwrite: overwriteExisting }) })
          transferredBytes += file.size
          completedFiles = index + 1
          setTransferProgress({ done: index + 1, total: folderFiles.length, current: folderFiles[index + 1]?.relative_path })
        }
      } else {
        const result = await api<{ size_bytes: number; local_path?: string }>(`s3/connections/${selected.id}/transfer/${action}`, { method: 'POST', body: JSON.stringify({ bucket: bucketName, key: transferKey, local_path: transferPath, overwrite: overwriteExisting }) })
        transferredBytes = result.size_bytes
        if (transferDirection === 'upload' && moveSource) {
          await api('files/action', { method: 'POST', body: JSON.stringify({ action: 'delete', path: transferPath, confirmed: true }) })
        }
        if (result.local_path) setTransferPath(result.local_path)
      }
      setTransferOpen(false)
      setTransferProgress(null)
      notify(`انتقال ${transferIsDirectory ? `${folderFiles.length} فایل` : 'فایل'} · ${formatBytes(transferredBytes)} انجام شد`)
      if (transferDirection === 'upload') await refreshObjects(prefix)
    } catch (error) {
      const message = error instanceof Error ? error.message : 'انتقال فایل ناموفق بود.'
      setTransferProgress((current) => current ? { ...current, done: completedFiles, error: message } : null)
      notify(transferIsDirectory ? `انتقال پوشه متوقف شد؛ ${completedFiles} فایل از ${folderFiles.length} فایل انجام شد. ${message}` : message)
    }
    finally { setWorking('') }
  }

  const headerAction = <div className="flex flex-wrap gap-2">{selected && <Button variant="outline" onClick={() => setSelectedId(null)}><ChevronRight size={15}/>همهٔ اتصال‌ها</Button>}<Button variant="outline" onClick={() => void load(true)} disabled={refreshing}><RefreshCw size={15} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button>{!selected && <Button onClick={() => { setEditingId(null); setDraft(emptyDraft); setStep(0); setDialogOpen(true) }}><Plus size={16}/>اتصال جدید</Button>}</div>
  return <>
    <Heading title={selected ? selected.display_name : "فضای ابری S3"} description={selected ? `${providers.find((item) => item.id === selected.provider_type)?.label || selected.provider_type} · مدیریت Bucket، فایل‌ها و تنظیمات اتصال` : "اتصال‌های فضای ابری را انتخاب کن تا وارد مدیریت فایل‌ها و تنظیمات هر اتصال شوی."} action={headerAction}/>
    {loading ? <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{Array.from({ length: 3 }, (_, index) => <Skeleton key={index} className="h-36"/>)}</div> : connections.length === 0 ? <Card className="mt-4 rounded-[14px] border-border"><CardContent className="grid justify-items-center px-5 py-12 text-center"><span className="grid h-14 w-14 place-items-center rounded-2xl border border-primary/20 bg-primary/[.07] text-primary"><Cloud size={27}/></span><h2 className="mt-4 text-lg font-semibold">هنوز فضای S3 اضافه نشده</h2><p className="mt-2 max-w-lg text-sm text-muted-foreground">یک سرویس موجود را متصل کن. ARG حساب تجاری یا Bucket را برایت خریداری نمی‌کند و کلیدهای دسترسی را در دیتابیس ذخیره نمی‌کند.</p><Button className="mt-5" onClick={() => setDialogOpen(true)}><Plus size={16}/>اتصال فضای ابری</Button></CardContent></Card> : !selected ? <section className="mt-4 grid min-w-0 gap-3 sm:grid-cols-2 xl:grid-cols-3" aria-label="اتصال‌های S3">{connections.map((connection) => <button key={connection.id} type="button" onClick={() => selectConnection(connection.id)} className="group min-w-0 rounded-[14px] border border-border bg-card/75 p-4 text-right transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/40 hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><div className="flex items-start justify-between gap-3"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/[.07] text-primary"><Cloud size={19}/></span><Badge variant={statusVariant(connection.status)}>{statusText[connection.status] || statusText.unverified}</Badge></div><p className="mt-4 truncate font-semibold text-foreground">{connection.display_name}</p><p className="mt-1 truncate text-xs text-muted-foreground">{providers.find((item) => item.id === connection.provider_type)?.label || connection.provider_type}</p><p className="mt-3 truncate text-left font-mono text-[10px] text-muted-foreground" dir="ltr">{connection.endpoint || `Amazon S3 · ${connection.region}`}</p><div className="mt-4 flex items-center justify-between border-t border-border/70 pt-3 text-[11px] text-muted-foreground"><span className="flex items-center gap-1.5"><KeyRound size={13}/>{connection.credential_present ? 'اعتبارنامه ذخیره است' : 'نیازمند کلید'}</span><span className="flex items-center gap-1 text-primary">مدیریت اتصال <ChevronLeft size={14} className="transition-transform group-hover:-translate-x-0.5"/></span></div></button>)}</section> : <div className="mt-4 min-w-0 space-y-4">
      <section className="min-w-0 space-y-4">
        <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><CardTitle className="text-lg">{selected.display_name}</CardTitle><Badge variant={statusVariant(selected.status)}>{statusText[selected.status] || statusText.unverified}</Badge></div><CardDescription className="mt-1">{selected.description || providers.find((item) => item.id === selected.provider_type)?.label}</CardDescription></div><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" onClick={() => void testConnection()} disabled={!!working}><RefreshCw size={14} className={working === 'test' ? 'animate-spin' : ''}/>بررسی اتصال</Button><Button size="sm" variant="outline" onClick={editConnection} disabled={!!working}>تنظیمات اتصال</Button><Button size="sm" variant="outline" className="text-destructive" onClick={() => void deleteConnection()} disabled={!!working}><Trash2 size={14}/>حذف اتصال</Button></div></CardHeader><CardContent className="grid gap-3 border-t border-border p-4 sm:grid-cols-2 lg:grid-cols-4"><InfoCell label="Provider" value={providers.find((item) => item.id === selected.provider_type)?.label || selected.provider_type}/><InfoCell label="Endpoint" value={selected.endpoint || 'AWS endpoint پیش‌فرض'} ltr/><InfoCell label="Signing Region" value={selected.region || 'گزارش نشده'} ltr/><InfoCell label="اعتبارنامه" value={`ذخیره‌شده · چرخش ${new Date(selected.last_rotated_at * 1000).toISOString().slice(0,10)}`}/>{selected.last_error && <p className="sm:col-span-2 lg:col-span-4 rounded-lg border border-amber-400/20 bg-amber-400/[.05] p-3 text-xs text-amber-100">{selected.last_error}</p>}</CardContent></Card>
        <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-center justify-between gap-2 space-y-0 p-4"><div><CardTitle className="text-sm">Bucketها</CardTitle><CardDescription className="mt-1">فهرست راه‌دور فقط با درخواست صریح و مجوز ListBuckets خوانده می‌شود.</CardDescription></div><div className="flex items-center gap-3"><Button size="sm" variant="outline" onClick={() => void refreshBucketList()} disabled={!!working}><RefreshCw size={13} className={working === 'list-buckets' ? 'animate-spin' : ''}/>بررسی فهرست Provider</Button><a className="flex items-center gap-1 text-xs text-muted-foreground hover:text-primary" href={providers.find((item) => item.id === selected.provider_type)?.docs} target="_blank" rel="noreferrer">راهنمای Provider <ExternalLink size={12}/></a></div></CardHeader><CardContent className="space-y-3 p-4 pt-0">
          {remoteError && <Alert className="border-amber-400/20 bg-amber-400/[.04]"><AlertDescription>{remoteError} اگر کلید به یک Bucket محدود است، نام را دستی اضافه کن؛ اتصال به‌خاطر نداشتن ListBuckets رد نمی‌شود.</AlertDescription></Alert>}
          {!bucketData ? <Skeleton className="h-16"/> : bucketData.provider_buckets === null ? <p className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground">برای دیدن Bucketهای سرویس‌دهنده، «بررسی فهرست Provider» را بزن. اگر ListBuckets مجاز نباشد می‌توانی نام Bucket را دستی وارد کنی.</p> : <div className="flex flex-wrap gap-2">{(bucketData.provider_buckets || []).map((name) => { const attached = bucketData.connections.some((item) => item.bucket_name === name); return <div key={name} className={`flex items-center gap-1 rounded-lg border px-2 py-1 ${bucketName === name ? 'border-primary/40 bg-primary/[.07]' : 'border-border bg-background/40'}`}><button type="button" className="flex items-center gap-2 px-1 py-1 text-xs" onClick={() => { setBucketName(name); setPrefix(''); setCursor(null); setPage(null) }}><Database size={13} className="text-primary"/><span className="font-mono" dir="ltr">{name}</span></button>{attached ? <Badge variant="outline">متصل در ARG</Badge> : <Button size="sm" variant="ghost" className="h-7 px-2 text-[10px]" onClick={() => void attachBucket(name)} disabled={!!working}>افزودن</Button>}</div>})}</div>}
          {bucketData?.connections.length ? <div className="divide-y divide-border rounded-lg border border-border">{bucketData.connections.map((bucket) => <div key={bucket.id} className="flex items-center gap-2 p-2.5"><button type="button" className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1 text-right text-xs" onClick={() => { setBucketName(bucket.bucket_name); setPrefix(''); setPage(null) }}><Database size={14} className="shrink-0 text-primary"/><span className="truncate">{bucket.display_name}</span><code className="truncate text-muted-foreground" dir="ltr">{bucket.bucket_name}</code><span className="mr-auto flex items-center gap-1 text-[10px] text-muted-foreground">حجم: {bucket.used_bytes == null ? 'نامشخص' : formatBytes(bucket.used_bytes)}{bucket.usage_source === 'manual' && <Badge variant="outline" className="px-1 py-0 text-[9px]">دستی</Badge>}</span></button><Button size="icon" variant="ghost" aria-label={`قطع اتصال ${bucket.bucket_name} از ARG`} onClick={() => void detachBucket(bucket)} disabled={!!working}><X size={14}/></Button></div>)}</div> : <p className="text-xs text-muted-foreground">Bucketی در ARG متصل نشده است.</p>}
          <form className="flex flex-wrap gap-2" onSubmit={(event) => { event.preventDefault(); void attachBucket(manualBucket) }}><Input aria-label="نام Bucket موجود" dir="ltr" className="min-w-[180px] flex-1" placeholder="نام Bucket موجود را وارد کن" value={manualBucket} onChange={(event) => setManualBucket(event.target.value)}/><Button type="submit" variant="outline" disabled={!manualBucket || !!working}><Plus size={14}/>اتصال موجود</Button>{provider?.create_bucket && <Button type="button" variant="outline" disabled={!manualBucket || !!working} onClick={() => void createBucket(manualBucket)}>ساخت Bucket</Button>}</form>
        </CardContent></Card>
        {selectedBucket && <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-center justify-between gap-3 space-y-0 p-4"><div><CardTitle className="flex items-center gap-2 text-sm"><HardDrive size={15} className="text-primary"/>اندازهٔ Bucket</CardTitle><CardDescription className="mt-1">ARG حجم مصرفی را از API این Provider نگرفته؛ اگر مقدارش را می‌دانی دستی ثبتش کن.</CardDescription></div><Button size="sm" variant="outline" onClick={openUsageEditor}><HardDrive size={14}/>{selectedBucket.used_bytes == null ? 'تکمیل اطلاعات حجم' : 'ویرایش حجم'}</Button></CardHeader><CardContent className="grid gap-3 border-t border-border p-4 sm:grid-cols-3"><InfoCell label="حجم مصرف‌شده" value={selectedBucket.used_bytes == null ? 'ثبت نشده' : formatBytes(selectedBucket.used_bytes)} ltr/><InfoCell label="سقف سرویس / قرارداد" value={selectedBucket.quota_bytes == null ? 'نامشخص' : formatBytes(selectedBucket.quota_bytes)} ltr/><InfoCell label="منبع و زمان ثبت" value={selectedBucket.usage_source === 'manual' && selectedBucket.measured_at ? `ثبت دستی · ${new Date(selectedBucket.measured_at * 1000).toISOString().slice(0, 16).replace('T', ' ')}` : 'هنوز ثبت نشده'}/><p className="text-[10px] leading-5 text-muted-foreground sm:col-span-3">مقدار دستی آمار تأییدشدهٔ سرویس‌دهنده نیست و در گزارش با منبع «ثبت دستی» مشخص می‌شود. سقف را اگر نمی‌دانی خالی بگذار؛ نمودار یا درصد ظرفیت ساختگی نمایش داده نمی‌شود.</p></CardContent></Card>}
        {bucketName && <Card className="min-w-0 rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-center justify-between gap-3 space-y-0 p-4"><div className="min-w-0"><CardTitle className="text-sm">مدیریت Objectها</CardTitle><CardDescription className="mt-1 truncate font-mono" dir="ltr">{bucketName}{prefix}</CardDescription></div><div className="flex flex-wrap gap-2"><Button size="sm" variant="outline" onClick={() => void refreshObjects(prefix)} disabled={pageLoading}><RefreshCw size={13} className={pageLoading ? 'animate-spin' : ''}/>تازه‌سازی</Button><Button size="sm" variant={transferOpen && transferDirection === 'upload' ? 'secondary' : 'outline'} onClick={() => transferOpen && transferDirection === 'upload' ? setTransferOpen(false) : openTransfer('upload', prefix)} disabled={!!working}><HardDrive size={14}/>{transferOpen && transferDirection === 'upload' ? 'بستن انتقال VPS' : 'انتقال از دیسک VPS'}</Button><input ref={uploadInput} type="file" className="hidden" aria-label="انتخاب فایل برای بارگذاری" onChange={(event) => void uploadFile(event.target.files?.[0])}/><Button size="sm" variant="outline" onClick={() => uploadInput.current?.click()} disabled={!!working}><UploadCloud size={14}/>{working === 'upload' ? 'در حال بارگذاری…' : 'آپلود'}</Button></div></CardHeader><CardContent className="space-y-3 p-4 pt-0">
          <form className="flex gap-2" onSubmit={(event) => { event.preventDefault(); void refreshObjects(searchPrefix) }}><Input dir="ltr" value={searchPrefix} onChange={(event) => setSearchPrefix(event.target.value)} placeholder="Prefix / مسیر مجازی"/><Button size="sm" variant="outline" type="submit" disabled={pageLoading}>بازکردن</Button></form>
          {transferOpen && transferDirection === 'upload' && <section className="space-y-3 rounded-xl border border-primary/25 bg-primary/[.025] p-3 sm:p-4" aria-label="انتقال فایل و پوشه از دیسک VPS">
            <div className="flex flex-wrap items-start justify-between gap-2"><div><h3 className="flex items-center gap-2 text-sm font-semibold"><HardDrive size={15} className="text-primary"/>انتقال از دیسک VPS به S3</h3><p className="mt-1 text-[10px] leading-5 text-muted-foreground">فایل یا پوشه را انتخاب کن؛ محتویات پوشه با ساختار زیرپوشه‌ها منتقل و در صورت خطا از آخرین فایل موفق ادامه داده می‌شود.</p></div>{folderFiles.length > 0 && <Badge variant="outline">{formatNumber(folderFiles.length)} فایل · {formatBytes(folderFiles.reduce((sum, file) => sum + file.size, 0))}</Badge>}</div>
            <div className="flex flex-wrap gap-2"><Input aria-label="مسیر پوشهٔ VPS" dir="ltr" className="min-w-[180px] flex-1 font-mono text-left" value={serverPath} onChange={(event) => { setServerPath(event.target.value); setTransferPath(''); setTransferIsDirectory(false); setFolderFiles([]); setTransferKey(''); setTransferProgress(null) }} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void browseServerPath(serverPath) } }}/><Button type="button" size="sm" variant="outline" onClick={() => void browseServerPath(serverPath)} disabled={serverBrowseLoading || folderScanBusy}>{serverBrowseLoading ? <LoaderCircle size={14} className="animate-spin"/> : <RefreshCw size={14}/>}بازکردن مسیر</Button><Button type="button" size="sm" variant="outline" onClick={() => void selectCurrentFolder()} disabled={folderScanBusy || serverPath === '/'}>{folderScanBusy ? <LoaderCircle size={14} className="animate-spin"/> : <Folder size={14}/>} {folderScanBusy ? 'در حال بررسی پوشه…' : 'انتخاب همین پوشه'}</Button></div>
            {serverBrowseError && <Alert className="border-amber-400/20 bg-amber-400/[.04]"><AlertDescription>{serverBrowseError}</AlertDescription></Alert>}
            {serverListing && <div className="max-h-56 overflow-y-auto rounded-lg border border-border bg-background/35" dir="ltr">{serverListing.parent && <button type="button" onClick={() => { setTransferPath(''); setTransferIsDirectory(false); setFolderFiles([]); setTransferKey(''); void browseServerPath(serverListing.parent!) }} className="flex w-full items-center gap-2 border-b border-border p-2.5 text-left text-xs text-primary"><Folder size={13}/>..</button>}{serverListing.entries.filter((entry) => entry.kind === 'directory' || entry.kind === 'file').map((entry) => <button type="button" key={entry.path} onClick={() => { if (entry.kind === 'directory') { setTransferPath(''); setTransferIsDirectory(false); setFolderFiles([]); setTransferKey(''); void browseServerPath(entry.path) } else chooseServerEntry(entry) }} className={`flex w-full items-center justify-between gap-3 border-b border-border/60 p-2.5 text-left text-xs last:border-0 hover:bg-secondary/50 ${transferPath === entry.path ? 'bg-primary/10 text-primary' : ''}`}><span className="flex min-w-0 items-center gap-2">{entry.kind === 'directory' ? <Folder size={14} className="text-primary"/> : <HardDrive size={13} className="text-muted-foreground"/>}<span className="truncate">{entry.name}</span></span><span className="shrink-0 font-mono text-[10px] text-muted-foreground">{entry.kind === 'file' ? formatBytes(entry.size) : 'پوشه · بازکردن'}</span></button>)}{!serverListing.entries.length && <p className="p-3 text-center text-xs text-muted-foreground">این پوشه خالی است.</p>}</div>}
            <div className="grid gap-3 sm:grid-cols-2"><Field label="مبدأ روی VPS" value={transferPath} onChange={(value) => { setTransferPath(value); setFolderFiles([]); setTransferIsDirectory(false); setTransferProgress(null) }} placeholder="یک فایل یا پوشه انتخاب کن" ltr/><Field label={transferIsDirectory ? 'Prefix مقصد برای پوشه' : 'کلید مقصد در Bucket'} value={transferKey} onChange={(value) => { setTransferKey(value); setTransferProgress(null) }} placeholder="backups/site/" ltr/></div>
            {transferProgress && <div className="space-y-1.5 rounded-lg border border-border bg-background/40 p-3" role="status" aria-live="polite"><div className="flex justify-between gap-3 text-xs"><span>{transferProgress.error ? 'انتقال متوقف شد' : 'در حال انتقال فایل‌ها'}</span><span className="font-mono" dir="ltr">{transferProgress.done}/{transferProgress.total}</span></div><div className="h-1.5 overflow-hidden rounded-full bg-secondary"><div className="h-full rounded-full bg-primary transition-[width]" style={{ width: `${transferProgress.total ? transferProgress.done / transferProgress.total * 100 : 0}%` }}/></div>{transferProgress.current && <code className="block truncate text-left text-[10px] text-muted-foreground" dir="ltr">{transferProgress.current}</code>}{transferProgress.error && <p className="text-[10px] text-destructive">{transferProgress.error}</p>}</div>}
            <div className="grid gap-2 sm:grid-cols-2"><label className="flex items-start gap-2 rounded-lg border border-border p-2.5 text-[10px] leading-5"><input type="checkbox" checked={overwriteExisting} onChange={(event) => setOverwriteExisting(event.target.checked)} disabled={!!working} className="mt-1"/><span><b className="block text-xs">جایگزینی Objectهای هم‌نام</b><span className="text-muted-foreground">بدون انتخاب، با رسیدن به فایل تکراری همان‌جا متوقف می‌شود.</span></span></label>{!transferIsDirectory && <label className="flex items-start gap-2 rounded-lg border border-border p-2.5 text-[10px] leading-5"><input type="checkbox" checked={moveSource} onChange={(event) => setMoveSource(event.target.checked)} disabled={!!working} className="mt-1"/><span><b className="block text-xs">پس از انتقال موفق، فایل مبدأ حذف شود</b><span className="text-muted-foreground">برای ایمنی، حذف پوشه‌ای پشتیبانی نمی‌شود.</span></span></label>}</div>
            <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-[10px] leading-4 text-muted-foreground">حداکثر 10000 فایل و 20 GiB در هر پوشه؛ سقف هر فایل 512 MiB است. symlinkها و فایل‌های ویژه منتقل نمی‌شوند. انتقال پوشه روی دیسک مبدأ را حذف نمی‌کند.</p><Button type="button" onClick={() => void runServerTransfer()} disabled={!!working || folderScanBusy || !transferPath.startsWith('/') || !transferKey.trim() || transferIsDirectory && !folderFiles.length}>{working === 'transfer' ? <LoaderCircle size={14} className="animate-spin"/> : <UploadCloud size={14}/>} {transferProgress?.error ? 'ادامهٔ انتقال' : 'شروع انتقال'}</Button></div>
          </section>}
          {pageError && <Alert className="border-destructive/20"><AlertDescription>{pageError}</AlertDescription></Alert>}
          {!page && !pageLoading ? <div className="rounded-lg border border-dashed border-border p-8 text-center text-sm text-muted-foreground">برای دریافت فهرست این Bucket، دکمهٔ «تازه‌سازی» را بزن.</div> : pageLoading && !page ? <Skeleton className="h-48"/> : page && <><div className="divide-y divide-border overflow-hidden rounded-lg border border-border">{prefix && <button type="button" className="flex w-full items-center gap-2 p-3 text-right text-xs text-primary hover:bg-background/60" onClick={() => { const parent = prefix.replace(/\/$/, '').split('/').slice(0,-1).join('/'); void refreshObjects(parent ? `${parent}/` : '') }}><ChevronRight size={14}/>یک سطح بالاتر</button>}{page.prefixes.map((item) => <button type="button" key={item} className="flex w-full items-center gap-2 p-3 text-right text-xs hover:bg-background/60" onClick={() => void refreshObjects(item)}><Folder size={15} className="text-primary"/><span className="truncate font-mono" dir="ltr">{item.slice(prefix.length)}</span></button>)}{page.objects.map((object) => <div key={object.key} className="flex min-w-0 items-center gap-2 p-3 text-xs"><span className="grid h-7 w-7 shrink-0 place-items-center rounded-md bg-secondary/70"><Database size={14}/></span><span className="min-w-0 flex-1 truncate font-mono" dir="ltr" title={object.key}>{object.key.slice(prefix.length)}</span><span className="shrink-0 font-mono text-muted-foreground" dir="ltr">{formatBytes(object.size_bytes)}</span><a href={downloadUrl(object.key)} aria-label={`دانلود ${object.key}`} className="grid h-8 w-8 place-items-center rounded-md text-muted-foreground hover:bg-secondary hover:text-foreground"><ArrowDownToLine size={14}/></a><Button size="icon" variant="ghost" aria-label={`ذخیره ${object.key} روی VPS`} title="ذخیره روی VPS" onClick={() => openTransfer('download', object.key)} disabled={!!working}><HardDrive size={14}/></Button><Button size="icon" variant="ghost" aria-label={`حذف ${object.key}`} onClick={() => void deleteObject(object.key)} disabled={!!working}><Trash2 size={14} className="text-destructive"/></Button></div>)}{!page.objects.length && !page.prefixes.length && <p className="p-6 text-center text-xs text-muted-foreground">در این Prefix آبجکتی نیست.</p>}</div><div className="flex items-center justify-between text-xs text-muted-foreground"><span>{formatNumber(page.key_count)} مورد در این صفحه</span><Button size="sm" variant="outline" disabled={!page.is_truncated || !page.next_token || pageLoading} onClick={() => void refreshObjects(prefix,page.next_token)}>صفحهٔ بعدی <ChevronLeft size={14}/></Button></div></>}</CardContent></Card>}
        {selectedBucket && <Card className="rounded-[14px] border-border"><CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4"><div><CardTitle className="flex items-center gap-2 text-sm"><HardDrive size={16} className="text-primary"/>Mount روی فایل‌سیستم VPS</CardTitle><CardDescription className="mt-1">دسترسی فایل‌مانند از مسیر سرور؛ فضای ابری ظرفیت فیزیکی دیسک را زیاد نمی‌کند و سهمیهٔ S3 ممکن است نامشخص باشد.</CardDescription></div><Badge variant={mount?.status === 'mounted' ? 'success' : mount?.status === 'failed' ? 'destructive' : 'outline'}>{mount?.status === 'mounted' ? 'متصل' : mount?.status === 'failed' ? 'خطا' : 'Unmounted'}</Badge></CardHeader><CardContent className="space-y-3 border-t border-border p-4"><div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-background/30 p-3"><div className="min-w-0"><p className="text-xs font-medium">مسیر Mount روی سرور</p><code dir="ltr" className="mt-1 block truncate text-left font-mono text-[11px] text-muted-foreground">{mount?.mount_path || `/mnt/arg-s3/connection-${selected.id}-bucket-${selectedBucket.id}`}</code></div><div className="flex shrink-0 gap-2"><Button size="sm" variant="outline" onClick={() => { navigator.clipboard.writeText(mount?.mount_path || `/mnt/arg-s3/connection-${selected.id}-bucket-${selectedBucket.id}`); notify('مسیر Mount کپی شد.') }}><HardDrive size={13}/>کپی مسیر</Button><Button size="sm" variant="outline" onClick={() => { window.location.hash = `#/files?path=${encodeURIComponent(mount?.mount_path || `/mnt/arg-s3/connection-${selected.id}-bucket-${selectedBucket.id}`)}` }}><Folder size={13}/>مدیریت فایل‌ها</Button></div></div><div className="grid gap-2 sm:grid-cols-2"><label className="flex items-start gap-3 rounded-lg border border-border p-3"><Switch checked={mountReadOnly} onCheckedChange={setMountReadOnly}/><span className="text-xs leading-5"><b className="block">فقط‌خواندنی</b><span className="text-muted-foreground">برای شروع امن است؛ خاموش‌کردنش دسترسی نوشتن روی Bucket می‌دهد.</span></span></label><label className="flex items-start gap-3 rounded-lg border border-border p-3"><Switch checked={mountAutoStart} onCheckedChange={setMountAutoStart}/><span className="text-xs leading-5"><b className="block">اتصال خودکار پس از راه‌اندازی</b><span className="text-muted-foreground">سرویس systemd برای این Mount فعال می‌شود.</span></span></label></div>{mount?.last_error && <Alert className="border-amber-400/20 bg-amber-400/[.04]"><AlertDescription>{mount.last_error}</AlertDescription></Alert>}<div className="flex flex-wrap gap-2"><Button size="sm" onClick={() => void configureMount()} disabled={!!working}>{working === 'mount' ? <LoaderCircle size={14} className="animate-spin"/> : <HardDrive size={14}/>}اتصال / اعمال تنظیمات</Button>{mount?.status === 'mounted' && <Button size="sm" variant="outline" onClick={() => void stopMount()} disabled={!!working}><X size={14}/>Unmount</Button>}<span className="self-center text-[10px] text-muted-foreground">نیازمند Rclone و FUSE · Cache محلی حداکثر 1 GiB است؛ ظرفیت Bucket همچنان جدا از دیسک محلی گزارش می‌شود.</span></div></CardContent></Card>}
      </section>
    </div>}

    <Dialog open={usageDialogOpen} onOpenChange={(open) => { if (!working) setUsageDialogOpen(open) }}>
      <DialogContent className="max-w-md">
        <DialogHeader><DialogTitle>تکمیل اندازهٔ Bucket</DialogTitle><DialogDescription>برای {selectedBucket?.bucket_name} حجم مصرف‌شده را وارد کن. واحد ورودی GiB است و سقف اختیاری است.</DialogDescription></DialogHeader>
        <form className="space-y-4" onSubmit={(event) => void saveManualUsage(event)}>
          <div className="space-y-1.5"><Label htmlFor="s3-used-size">حجم مصرف‌شده (GiB)</Label><Input id="s3-used-size" type="number" min="0" step="0.01" inputMode="decimal" dir="ltr" className="text-left font-mono" value={usedSizeGib} onChange={(event) => setUsedSizeGib(event.target.value)} placeholder="مثلاً 125.5"/><p className="text-[10px] text-muted-foreground">اندازهٔ داده‌های فعلی داخل Bucket را وارد کن.</p></div>
          <div className="space-y-1.5"><Label htmlFor="s3-quota-size">سقف فضای Bucket (GiB) · اختیاری</Label><Input id="s3-quota-size" type="number" min="0.01" step="0.01" inputMode="decimal" dir="ltr" className="text-left font-mono" value={quotaSizeGib} onChange={(event) => setQuotaSizeGib(event.target.value)} placeholder="اگر Provider یا قرارداد سقف ندارد، خالی بگذار"/><p className="text-[10px] text-muted-foreground">در صورت خالی‌بودن، درصد ظرفیت محاسبه نمی‌شود.</p></div>
          <div className="flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setUsageDialogOpen(false)} disabled={!!working}>انصراف</Button><Button type="submit" disabled={!!working || !usedSizeGib.trim()}>{working === 'usage' ? <LoaderCircle size={14} className="animate-spin"/> : <HardDrive size={14}/>}ذخیرهٔ اندازه</Button></div>
        </form>
      </DialogContent>
    </Dialog>
    <Dialog open={dialogOpen} onOpenChange={(open) => { setDialogOpen(open); if (!open) { setPrivateConfirm(false); setStep(0); setEditingId(null) } }}>
      <DialogContent className="max-h-[92vh] max-w-2xl overflow-y-auto">
        <DialogHeader><DialogTitle>{editingId ? 'تنظیمات اتصال S3' : 'اتصال فضای S3'}</DialogTitle><DialogDescription>{editingId ? 'مشخصات اتصال را ویرایش کن. برای نگه‌داشتن کلید فعلی، فیلدهای کلید را خالی بگذار.' : `مرحلهٔ ${step + 1} از 2 · کلیدها فقط در فایل محافظت‌شدهٔ سرور ذخیره می‌شوند.`}</DialogDescription></DialogHeader>
        {step === 0 ? <div className="grid gap-2 sm:grid-cols-2">{providers.map((item) => <button type="button" key={item.id} onClick={() => chooseProvider(item.id)} className="rounded-xl border border-border bg-background/40 p-4 text-right transition-colors hover:border-primary/40 hover:bg-primary/[.04]"><div className="flex items-center justify-between"><span className="font-semibold">{item.label}</span><Cloud size={17} className="text-primary"/></div><p className="mt-2 text-xs text-muted-foreground">{item.endpoint_required ? 'Endpoint اختصاصی سرویس لازم است' : 'اتصال به Endpoint پیش‌فرض AWS'}</p></button>)}</div> : <form onSubmit={(event) => void createConnection(event)} className="space-y-4">
          <div className="flex items-center justify-between rounded-lg border border-border bg-background/40 p-3 text-xs"><span>ارائه‌دهنده</span><b>{provider?.label || draft.provider_type}</b><a href={provider?.docs} target="_blank" rel="noreferrer" className="flex items-center gap-1 text-primary">راهنما <ExternalLink size={12}/></a></div>
          {draft.provider_type === 'arvancloud' && provider?.endpoints && <div className="space-y-2"><Label htmlFor="arvan-endpoint">Endpoint منطقه</Label><select id="arvan-endpoint" className="h-10 w-full rounded-lg border border-border bg-background px-3 text-sm" value={provider.endpoints.find((item) => item.endpoint === draft.endpoint)?.endpoint || 'custom'} onChange={(event) => { const selectedEndpoint = provider.endpoints?.find((item) => item.endpoint === event.target.value); setDraft((value) => ({ ...value, endpoint: selectedEndpoint?.endpoint || '', region: selectedEndpoint?.region || value.region })) }}><option value="custom">Endpoint سفارشی</option>{provider.endpoints.map((endpoint) => <option key={endpoint.id} value={endpoint.endpoint}>{endpoint.label} · {endpoint.endpoint}</option>)}</select></div>}
          <div className="grid gap-3 sm:grid-cols-2"><Field label="نام نمایشی اتصال" value={draft.display_name} onChange={(value) => setDraft((old) => ({ ...old, display_name: value }))} placeholder="مثلاً Backup Archive"/><Field label="Bucket موجود (اختیاری)" value={bucketName} onChange={setBucketName} placeholder="my-backup-bucket" ltr/></div>
          {provider?.endpoint_required && <Field label={draft.provider_type === 'parspack' ? 'Endpoint از پنل پارس‌پک' : 'Endpoint URL'} value={draft.endpoint} onChange={(value) => setDraft((old) => ({ ...old, endpoint: value }))} placeholder={draft.provider_type === 'arvancloud' ? 'Endpoint سفارشی HTTPS' : 'https://s3.example.com'} ltr help={draft.provider_type === 'parspack' ? 'Endpoint ثابت فرض نمی‌شود؛ آدرس را از پنل پارس‌پک بردار.' : 'HTTPS origin؛ مسیر، query و credential در URL پذیرفته نمی‌شود.'}/>}
          <div className="grid gap-3 sm:grid-cols-2"><Field label={draft.provider_type === 'arvancloud' ? 'Signing Region (از مستندات سرویس)' : 'Region'} value={draft.region} onChange={(value) => setDraft((old) => ({ ...old, region: value }))} placeholder={draft.provider_type === 'cloudflare_r2' ? 'auto' : draft.provider_type === 'arvancloud' && draft.endpoint.includes('ir-central1') ? 'ir-central1' : 'Region را از Provider بگیر'} ltr help={draft.provider_type === 'arvancloud' && draft.endpoint.includes('ir-central1') ? 'طبق Endpoint و منطقهٔ نشان‌داده‌شده در پنل آروان: ir-central1' : undefined}/><div className="space-y-2"><Label htmlFor="s3-address-style">Addressing Style</Label><select id="s3-address-style" className="h-10 w-full rounded-lg border border-border bg-background px-3 text-sm" value={draft.address_style} onChange={(event) => setDraft((old) => ({ ...old, address_style: event.target.value }))}><option value="auto">خودکار</option><option value="path">Path-style</option><option value="virtual">Virtual-hosted</option></select></div></div>
          <div className="grid gap-3 sm:grid-cols-2"><Field label="Access Key ID" value={draft.access_key_id} onChange={(value) => setDraft((old) => ({ ...old, access_key_id: value }))} placeholder={editingId ? 'برای حفظ کلید فعلی خالی بگذار' : undefined} ltr/><Field label="Secret Access Key" value={draft.secret_access_key} onChange={(value) => setDraft((old) => ({ ...old, secret_access_key: value }))} type="password" placeholder={editingId ? 'برای حفظ کلید فعلی خالی بگذار' : undefined} ltr/></div>
          <Field label="Session Token (اختیاری)" value={draft.session_token} onChange={(value) => setDraft((old) => ({ ...old, session_token: value }))} type="password" ltr/>
          <label className="flex items-start gap-3 rounded-lg border border-border p-3"><Switch checked={draft.allow_private_endpoint} onCheckedChange={(value) => { setDraft((old) => ({ ...old, allow_private_endpoint: value })); setPrivateConfirm(false) }}/><span className="text-xs leading-5"><b className="block">Endpoint خصوصی / شبکهٔ داخلی</b><span className="text-muted-foreground">فقط برای MinIO یا مقصدی که خودت بررسی کرده‌ای. Endpointهای metadata همیشه مسدود می‌مانند.</span></span></label>
          {draft.allow_private_endpoint && <label className="flex items-center gap-2 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs"><input type="checkbox" checked={privateConfirm} onChange={(event) => setPrivateConfirm(event.target.checked)}/>مقصد خصوصی را بررسی کرده‌ام و اجازهٔ اتصال ARG به آن را می‌دهم.</label>}
          <div className="flex justify-between gap-2"><Button type="button" variant="outline" onClick={() => setStep(editingId ? 1 : 0)} disabled={!!editingId}><ChevronRight size={15}/>بازگشت</Button><Button type="submit" disabled={saving || !draft.display_name || !editingId && (!draft.access_key_id || !draft.secret_access_key) || !!provider?.endpoint_required && !draft.endpoint || !!provider?.region_required && !draft.region}>{saving ? <LoaderCircle className="animate-spin" size={15}/> : <ShieldCheck size={15}/>}ذخیرهٔ تنظیمات</Button></div>
        </form>}
      </DialogContent>
    </Dialog>
    <Dialog open={transferOpen && transferDirection === 'download'} onOpenChange={(open) => { if (!working) setTransferOpen(open) }}>
      <DialogContent className="max-h-[90vh] max-w-xl overflow-y-auto">
        <DialogHeader><DialogTitle>{transferDirection === 'upload' ? 'انتقال از دیسک VPS به Bucket' : 'انتقال از Bucket به دیسک VPS'}</DialogTitle><DialogDescription>انتقال مستقیماً بین سرور و فضای ابری انجام می‌شود؛ فایل از مرورگر کاربر عبور نمی‌کند. مقصد محلی موجود بازنویسی نمی‌شود.</DialogDescription></DialogHeader>
        <form className="space-y-4" onSubmit={(event) => void runServerTransfer(event)}>
          <div className="rounded-lg border border-border bg-background/40 p-3 text-xs"><span className="text-muted-foreground">Bucket مقصد/مبدأ</span><code className="mt-1 block font-mono" dir="ltr">{bucketName}</code></div>
          <section className="space-y-2 rounded-xl border border-border bg-background/30 p-3" aria-label="مرور فایل‌های VPS"><div className="flex gap-2"><Input aria-label="مسیر پوشهٔ VPS" dir="ltr" className="font-mono text-left" value={serverPath} onChange={(event) => setServerPath(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void browseServerPath(serverPath) } }}/><Button type="button" size="sm" variant="outline" onClick={() => void browseServerPath(serverPath)} disabled={serverBrowseLoading}>{serverBrowseLoading ? <LoaderCircle size={14} className="animate-spin"/> : <RefreshCw size={14}/>}بازکردن</Button></div>{serverBrowseError && <p role="alert" className="text-xs text-rose-200">{serverBrowseError}</p>}{serverListing && <div className="max-h-40 overflow-y-auto rounded-lg border border-border" dir="ltr">{serverListing.parent && <button type="button" onClick={() => void browseServerPath(serverListing.parent!)} className="flex w-full items-center gap-2 border-b border-border p-2 text-left text-xs text-primary"><Folder size={13}/>..</button>}{serverListing.entries.filter((entry) => entry.kind === 'directory' || (transferDirection === 'upload' && entry.kind === 'file')).map((entry) => <button type="button" key={entry.path} onClick={() => chooseServerEntry(entry)} className={`flex w-full items-center justify-between gap-3 border-b border-border/60 p-2 text-left text-xs last:border-0 hover:bg-secondary/60 ${transferPath === entry.path ? 'bg-primary/10 text-primary' : ''}`}><span className="flex min-w-0 items-center gap-2"><Folder size={13} className={entry.kind === 'directory' ? 'text-primary' : 'text-muted-foreground'}/><span className="truncate">{entry.name}</span></span><span className="shrink-0 font-mono text-[10px] text-muted-foreground">{entry.kind === 'file' ? formatBytes(entry.size) : 'folder'}</span></button>)}{!serverListing.entries.length && <p className="p-3 text-center text-xs text-muted-foreground">این پوشه خالی است.</p>}</div>}{transferDirection === 'download' && serverListing && <Button type="button" size="sm" variant="outline" onClick={() => { const filename = transferKey.split('/').filter(Boolean).at(-1) || 'object'; setTransferPath(`${serverListing.path.replace(/\/$/, '')}/${filename}`) }}><Folder size={13}/>انتخاب این پوشه</Button>}<p className="text-[10px] text-muted-foreground">{transferDirection === 'upload' ? 'برای آپلود، یک فایل را انتخاب کن؛ symlink قابل انتقال نیست.' : 'برای دانلود، پوشهٔ مقصد را باز و سپس انتخاب کن.'}</p></section>
          <Field label={transferDirection === 'upload' ? 'مسیر فایل روی VPS' : 'مسیر کامل فایل مقصد روی VPS'} value={transferPath} onChange={setTransferPath} placeholder={transferDirection === 'upload' ? '/var/www/site/backup.tar.gz' : '/var/backups/site.tar.gz'} ltr help={transferDirection === 'download' ? 'پوشهٔ مقصد باید وجود داشته باشد؛ فایل جدید با دسترسی خصوصی ساخته می‌شود.' : 'فقط فایل عادی تا 512 MiB؛ مسیر را از فایل‌منیجر سرور کپی کن.'}/>
          <Field label={transferDirection === 'upload' ? 'کلید مقصد در Bucket' : 'کلید Object در Bucket'} value={transferKey} onChange={setTransferKey} placeholder="backups/site.tar.gz" ltr/>
          {transferDirection === 'upload' && <><label className="flex items-start gap-3 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs leading-5"><input type="checkbox" checked={overwriteExisting} onChange={(event) => setOverwriteExisting(event.target.checked)} className="mt-1"/><span><b className="block">در صورت وجود Object هم‌نام، جایگزینی را اجازه بده</b><span className="text-muted-foreground">به‌صورت پیش‌فرض مقصد بررسی می‌شود و فایل موجود بازنویسی نمی‌شود.</span></span></label><label className="flex items-start gap-3 rounded-lg border border-border p-3 text-xs leading-5"><input type="checkbox" checked={moveSource} onChange={(event) => setMoveSource(event.target.checked)} className="mt-1"/><span><b className="block">پس از انتقال موفق، فایل مبدأ از VPS حذف شود</b><span className="text-muted-foreground">این گزینه فایل را واقعاً جابه‌جا می‌کند؛ حذف فقط پس از موفقیت آپلود انجام می‌شود.</span></span></label></>}
          <div className="flex flex-wrap items-center justify-between gap-2"><Button type="button" variant="outline" onClick={() => { window.location.hash = '#/files' }}><Folder size={14}/>بازکردن فایل‌منیجر VPS</Button><div className="flex gap-2"><Button type="button" variant="outline" onClick={() => setTransferOpen(false)} disabled={!!working}>انصراف</Button><Button type="submit" disabled={!!working || !transferPath.startsWith('/') || !transferKey.trim()}>{working === 'transfer' ? <LoaderCircle size={14} className="animate-spin"/> : transferDirection === 'upload' ? <UploadCloud size={14}/> : <ArrowDownToLine size={14}/>}شروع انتقال</Button></div></div>
          <p className="text-[10px] leading-4 text-muted-foreground">سقف انتقال مستقیم: آپلود 512 MiB، دانلود 2 GiB. عملیات در گزارش مدیریتی ثبت می‌شود.</p>
        </form>
      </DialogContent>
    </Dialog>
  </>
}

function InfoCell({ label, value, ltr = false }: { label: string; value: string; ltr?: boolean }) {
  return <div className="min-w-0 rounded-lg border border-border/70 bg-background/30 p-3"><p className="text-[10px] text-muted-foreground">{label}</p><p className={`mt-1 truncate text-xs text-foreground ${ltr ? 'text-left font-mono' : ''}`} dir={ltr ? 'ltr' : undefined} title={value}>{value}</p></div>
}
function Field({ label, value, onChange, placeholder, ltr = false, type = 'text', help }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string; ltr?: boolean; type?: string; help?: string }) {
  return <div className="space-y-1.5"><Label>{label}</Label><Input type={type} dir={ltr ? 'ltr' : undefined} autoComplete="off" value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder}/>{help && <p className="text-[10px] leading-4 text-muted-foreground">{help}</p>}</div>
}
