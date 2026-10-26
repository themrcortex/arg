import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { Archive, ArrowDownToLine, ArrowUp, Check, Clipboard, ClipboardPaste, Copy, File, FilePlus2, FileText, Folder, FolderPlus, HardDrive, LoaderCircle, Pencil, RefreshCw, Search, Scissors, Settings2, Trash2, Upload, X } from 'lucide-react'
import { api, type StorageInventory, type StorageVolume } from './lib/api'
import { formatPanelDate, usePanelTimeZone } from './lib/timezone'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'
import { Card, CardContent } from './components/ui/card'
import { Alert, AlertDescription } from './components/ui/alert'
import { Skeleton } from './components/ui/skeleton'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { FileManagerBrowser, type FileManagerEntry } from './components/file-manager/FileManagerBrowser'

type Entry = { name: string; path: string; kind: 'directory' | 'file' | 'symlink' | 'other'; size: number; modified: number | null; permissions: string; read_only: boolean; link_target?: string | null }
type Listing = { path: string; parent: string | null; entries: Entry[]; truncated: boolean }
type DiskScan = { root: string; mode: 'standard' | 'deep'; limits?: { max_entries: number; max_duration_seconds: number; max_depth: number }; total_bytes: number; volume_total: number; volume_used: number; volume_available: number; usage_percent: number; entries_scanned: number; truncated: boolean; directory_children_truncated?: boolean; directory_children?: Array<{ path: string; bytes: number }>; elapsed_ms: number; largest_files: Array<{ path: string; bytes: number }>; largest_directories: Array<{ path: string; bytes: number }>; note: string }
type ClipboardItem = { path: string; name: string; mode: 'copy' | 'move' }
type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode

function prettyBytes(bytes: number) {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / 1024 ** index).toLocaleString('en-US', { maximumFractionDigits: 1 })} ${units[index]}`
}

function diskPathAdvice(path: string) {
  if (path === '/') return { title: 'ریشهٔ سیستم', detail: 'این جمع‌بندی کل فایل‌سیستم ریشه است؛ برای پیدا کردن مصرف، زیرپوشه‌ها را باز کن.' }
  if (path === '/usr' || path.startsWith('/usr/')) return { title: 'فایل‌های سیستم‌عامل', detail: 'بسته‌های سیستم اینجا نصب‌اند؛ فایل‌ها را دستی پاک نکن. از APT برای مدیریت بسته‌ها استفاده کن.' }
  if (path === '/var/lib/containerd' || path.startsWith('/var/lib/containerd/')) return { title: 'داده‌های کانتینر', detail: 'تصویرها و لایه‌های فعال Docker/containerd ممکن است اینجا باشند؛ از صفحهٔ Docker و پس از بررسی کانتینرهای متوقف استفاده کن.' }
  if (path === '/var/log' || path.startsWith('/var/log/')) return { title: 'گزارش سرویس‌ها', detail: 'لاگ‌های جاری را دستی حذف نکن. گزارش‌های systemd را از مدیریت سرور و ابزارهای سرویس پاک‌سازی کن.' }
  if (path === '/var/cache/apt' || path.startsWith('/var/cache/apt/')) return { title: 'کش بسته‌های APT', detail: 'فایل‌های دانلودشدهٔ بسته‌هاست؛ پاک‌سازی امن را از صفحهٔ بسته‌ها با APT انجام بده.' }
  if (path === '/opt/arg/source/target' || path.startsWith('/opt/arg/source/target/')) return { title: 'کش Build زبان Rust', detail: 'فایل‌های میانی کامپایل ARG هستند؛ حالت «آپدیت کامل» کش را پاک می‌کند، اما Build بعدی طولانی‌تر می‌شود.' }
  if (path === '/home' || path.startsWith('/home/')) return { title: 'فایل‌های کاربران و پروژه‌ها', detail: 'ممکن است دادهٔ اصلی سایت‌ها یا برنامه‌ها باشد؛ پیش از حذف، هر زیرپوشه را بررسی کن.' }
  if (path === '/opt' || path.startsWith('/opt/')) return { title: 'برنامه‌ها و نسخه‌های نصب‌شده', detail: 'ممکن است شامل برنامهٔ فعال یا نسخه‌های قبلی باشد؛ سرویس وابسته را قبل از پاک‌سازی بررسی کن.' }
  if (path === '/var/lib' || path.startsWith('/var/lib/')) return { title: 'داده‌های پایدار سرویس‌ها', detail: 'دیتابیس، کانتینر و وضعیت سرویس‌ها ممکن است اینجا باشند؛ پاک‌سازی دستی می‌تواند سرویس را خراب کند.' }
  return { title: 'پوشهٔ سرور', detail: 'برای دیدن محتویات، زیرپوشه را باز کن. فایل ناشناس را پیش از تشخیص مالک و سرویس حذف نکن.' }
}

function encodeBase64(bytes: Uint8Array) {
  let binary = ''
  const step = 0x8000
  for (let start = 0; start < bytes.length; start += step) binary += String.fromCharCode(...bytes.subarray(start, start + step))
  return btoa(binary)
}

export function FileManagerPage({ Heading, notify }: { Heading: Heading; notify: (message: string) => void }) {
  const timeZone = usePanelTimeZone()
  const [path, setPath] = useState('/')
  const [pathInput, setPathInput] = useState('/')
  const [listing, setListing] = useState<Listing | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [clipboard, setClipboard] = useState<ClipboardItem | null>(null)
  const [dialog, setDialog] = useState<'create' | 'rename' | 'permissions' | 'edit' | 'symlink' | null>(null)
  const [createKind, setCreateKind] = useState<'file' | 'directory'>('file')
  const [newName, setNewName] = useState('')
  const [permissions, setPermissions] = useState('0644')
  const [selected, setSelected] = useState<Entry | null>(null)
  const [editor, setEditor] = useState('')
  const [linkTarget, setLinkTarget] = useState('')
  const [reviewLinkTarget, setReviewLinkTarget] = useState(false)
  const [uploading, setUploading] = useState('')
  const [diskScanOpen, setDiskScanOpen] = useState(false)
  const [diskPath, setDiskPath] = useState('/')
  const [diskScanMode, setDiskScanMode] = useState<'standard' | 'deep'>('standard')
  const [diskScan, setDiskScan] = useState<DiskScan | null>(null)
  const [diskScanBusy, setDiskScanBusy] = useState(false)
  const [diskScanError, setDiskScanError] = useState('')
  const [diskVolumes, setDiskVolumes] = useState<StorageVolume[]>([])
  const [diskVolumesLoading, setDiskVolumesLoading] = useState(false)
  const [diskVolumesError, setDiskVolumesError] = useState('')
  const uploadRef = useRef<HTMLInputElement>(null)

  const load = useCallback(async (nextPath = path) => {
    setLoading(true); setError('')
    try {
      const result = await api<Listing>(`files?path=${encodeURIComponent(nextPath)}`)
      setListing(result); setPath(result.path); setPathInput(result.path)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'فهرست فایل‌ها دریافت نشد.') }
    finally { setLoading(false) }
  }, [path])
  useEffect(() => {
    const requested = new URLSearchParams(window.location.hash.split('?')[1] || '').get('path') || '/'
    void load(requested.startsWith('/') ? requested : '/')
  }, [])
  useEffect(() => {
    if (!diskScanOpen) return
    let active = true
    setDiskVolumesLoading(true)
    setDiskVolumesError('')
    api<StorageInventory>('storage/volumes').then((inventory) => {
      if (active) setDiskVolumes(inventory.volumes || [])
    }).catch((cause) => {
      if (active) setDiskVolumesError(cause instanceof Error ? cause.message : 'فهرست دیسک‌ها خوانده نشد.')
    }).finally(() => { if (active) setDiskVolumesLoading(false) })
    return () => { active = false }
  }, [diskScanOpen])

  const date = (seconds: number | null) => formatPanelDate(seconds, timeZone, { dateStyle: 'medium', timeStyle: 'short' }, 'en-GB')
  const entries = listing?.entries || []
  const browserEntries: FileManagerEntry[] = entries.map((entry) => ({ id: entry.path, path: entry.path, name: entry.name, kind: entry.kind, sizeLabel: entry.kind === 'file' ? prettyBytes(entry.size) : undefined, modifiedLabel: date(entry.modified), detail: entry.kind === 'symlink' ? entry.link_target || 'مقصد پیوند قابل خواندن نیست' : entry.permissions }))
  const perform = async (action: string, payload: Record<string, unknown> = {}, success = 'عملیات فایل انجام شد.') => {
    setBusy(action); setError('')
    try {
      await api('files/action', { method: 'POST', body: JSON.stringify({ action, ...payload }) })
      notify(success); await load(path)
      return true
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'عملیات فایل ناموفق بود.'
      setError(message); notify(message); return false
    } finally { setBusy('') }
  }
  const openEditor = async (entry: Entry) => {
    setBusy('read'); setError('')
    try {
      const result = await api<{ content: string }>(`files/content?path=${encodeURIComponent(entry.path)}`)
      setSelected(entry); setEditor(result.content); setDialog('edit')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'فایل برای ویرایش خوانده نشد.') }
    finally { setBusy('') }
  }
  const download = async (entry: Entry) => {
    setBusy(`download:${entry.path}`); setError('')
    try {
      const response = await fetch(`/api/files/download?path=${encodeURIComponent(entry.path)}`, { credentials: 'same-origin' })
      if (!response.ok) { const result = await response.json(); throw new Error(result.error || `HTTP ${response.status}`) }
      const url = URL.createObjectURL(await response.blob())
      const anchor = document.createElement('a'); anchor.href = url; anchor.download = entry.name; anchor.click(); URL.revokeObjectURL(url)
      notify('دانلود فایل شروع شد.')
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'دانلود ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  const uploadFiles = async (files: FileList | null) => {
    if (!files?.length || !listing) return
    const queue = Array.from(files)
    for (const file of queue) {
      if (file.size > 2 * 1024 * 1024) { setError(`${file.name}: حداکثر اندازهٔ هر فایل برای آپلود 2 MiB است.`); continue }
      setUploading(file.name)
      const bytes = new Uint8Array(await file.arrayBuffer())
      const ok = await perform('upload', { path: listing.path, name: file.name, content: encodeBase64(bytes) }, `فایل ${file.name} آپلود شد.`)
      if (!ok) break
    }
    setUploading('')
    if (uploadRef.current) uploadRef.current.value = ''
  }
  const submitCreate = async (event: FormEvent) => {
    event.preventDefault()
    const ok = await perform(createKind === 'directory' ? 'mkdir' : 'create', { path, name: newName }, createKind === 'directory' ? 'پوشه ساخته شد.' : 'فایل ساخته شد.')
    if (ok) { setDialog(null); setNewName('') }
  }
  const submitRename = async (event: FormEvent) => {
    event.preventDefault(); if (!selected) return
    const ok = await perform('rename', { path: selected.path, new_name: newName }, 'نام موردنظر تغییر کرد.')
    if (ok) { setDialog(null); setNewName(''); setSelected(null) }
  }
  const openSymlinkEditor = (entry: Entry) => {
    setSelected(entry); setLinkTarget(entry.link_target || ''); setReviewLinkTarget(false); setDialog('symlink')
  }
  const submitSymlinkTarget = async (event: FormEvent) => {
    event.preventDefault()
    if (!selected || !linkTarget.trim()) return
    if (!reviewLinkTarget) { setReviewLinkTarget(true); return }
    const ok = await perform('symlink-update', { path: selected.path, link_target: linkTarget }, 'مقصد symbolic link تغییر کرد.')
    if (ok) { setDialog(null); setSelected(null); setReviewLinkTarget(false) }
  }
  const submitPermissions = async (event: FormEvent) => {
    event.preventDefault(); if (!selected) return
    const ok = await perform('chmod', { path: selected.path, permissions }, 'دسترسی فایل تغییر کرد.')
    if (ok) setDialog(null)
  }
  const saveEditor = async () => {
    if (!selected) return
    const bytes = new TextEncoder().encode(editor)
    const ok = await perform('save', { path: selected.path, content: encodeBase64(bytes) }, 'تغییرات فایل ذخیره شد.')
    if (ok) setDialog(null)
  }
  const scanDisk = async (root = diskPath) => {
    setDiskPath(root); setDiskScanBusy(true); setDiskScanError(''); setDiskScan(null)
    try { setDiskScan(await api<DiskScan>('files/disk-scan', { method: 'POST', body: JSON.stringify({ path: root, mode: diskScanMode }) })) }
    catch (cause) { setDiskScanError(cause instanceof Error ? cause.message : 'اسکن فضای دیسک انجام نشد.') }
    finally { setDiskScanBusy(false) }
  }
  const openDiskPath = (root: string) => {
    setDiskPath(root)
    void scanDisk(root)
  }
  const parentDiskPath = diskPath === '/' ? null : diskPath.replace(/\/+$/, '').split('/').slice(0, -1).join('/') || '/'
  const paste = async () => {
    if (!clipboard || !listing) return
    if (clipboard.mode === 'move' && !window.confirm(`«${clipboard.name}» به این پوشه منتقل شود؟`)) return
    const ok = await perform('paste', { source: clipboard.path, destination: listing.path, mode: clipboard.mode, confirmed: clipboard.mode === 'move' }, clipboard.mode === 'move' ? 'فایل یا پوشه منتقل شد.' : 'کپی فایل یا پوشه انجام شد.')
    if (ok) setClipboard(null)
  }
  const remove = async (entry: Entry) => {
    if (!window.confirm(`«${entry.name}» و تمام محتوای داخل آن حذف شود؟ این عملیات قابل بازگشت نیست.`)) return
    await perform('delete', { path: entry.path, confirmed: true }, 'فایل یا پوشه حذف شد.')
  }
  const zipFolder = async (entry: Entry) => {
    if (entry.kind !== 'directory') return
    const archiveName = `${entry.name}.zip`
    if (listing?.entries.some((item) => item.name === archiveName)) { setError(`فایلی با نام ${archiveName} از قبل در این پوشه وجود دارد.`); return }
    await perform('zip', { path: entry.path, name: archiveName }, `فشرده‌سازی «${entry.name}» انجام شد؛ فایل ZIP را می‌توانی دانلود کنی.`)
  }
  const unzipFile = async (entry: Entry) => {
    if (entry.kind !== 'file' || !entry.name.toLowerCase().endsWith('.zip')) return
    if (!window.confirm(`محتوای «${entry.name}» در پوشه‌ای تازه در همین مسیر استخراج شود؟ فایل‌های موجود بازنویسی نمی‌شوند.`)) return
    await perform('unzip', { path: entry.path, destination: path, confirmed: true }, `محتوای «${entry.name}» در پوشه‌ای جدا استخراج شد.`)
  }
  const renderFileActions = (item: FileManagerEntry) => {
    const entry = entries.find((candidate) => candidate.path === item.path)
    if (!entry) return null
    return <>
      <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="کپی" title="کپی" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'copy' })}><Copy size={14}/></Button>
      <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="کات" title="کات / انتقال" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'move' })}><Scissors size={14}/></Button>
      <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="تغییر نام" title="تغییر نام" onClick={() => { setSelected(entry); setNewName(entry.name); setDialog('rename') }}><Pencil size={14}/></Button>
      {entry.kind !== 'symlink' && <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="تغییر دسترسی" title="تغییر دسترسی" onClick={() => { setSelected(entry); setPermissions(entry.permissions); setDialog('permissions') }}><Settings2 size={14}/></Button>}
      {entry.kind === 'symlink' && <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="ویرایش مقصد پیوند" title="ویرایش مقصد symbolic link" onClick={() => openSymlinkEditor(entry)}><Pencil size={14}/></Button>}
      {entry.kind === 'directory' ? <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="ساخت ZIP" title="فشرده‌سازی پوشه به ZIP" disabled={!!busy || entry.read_only} onClick={() => void zipFolder(entry)}><Archive size={14}/></Button> : entry.kind === 'file' && entry.name.toLowerCase().endsWith('.zip') ? <Button size="icon" variant="ghost" className="h-8 w-8" aria-label="استخراج ZIP" title="استخراج در پوشه‌ای تازه" disabled={!!busy || entry.read_only} onClick={() => void unzipFile(entry)}><Archive size={14}/></Button> : null}
      {entry.kind === 'file' && <><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="دانلود" title="دانلود" disabled={!!busy} onClick={() => void download(entry)}>{busy === `download:${entry.path}` ? <LoaderCircle size={14} className="animate-spin"/> : <ArrowDownToLine size={14}/>}</Button><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="ویرایش" title="ویرایش متن" disabled={!!busy} onClick={() => void openEditor(entry)}><FileText size={14}/></Button></>}
      <Button size="icon" variant="ghost" className="h-8 w-8 text-rose-300 hover:text-rose-200" aria-label="حذف" title="حذف" disabled={!!busy} onClick={() => void remove(entry)}><Trash2 size={14}/></Button>
    </>
  }
  const toolbar = <div className="grid w-full grid-cols-2 gap-2 sm:w-auto sm:flex sm:flex-wrap"><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => { setDiskPath('/'); setDiskScanOpen(true) }}><HardDrive size={14}/>تحلیل فضای دیسک</Button><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => void load(path)} disabled={loading || !!busy}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>تازه‌سازی</Button><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => { setCreateKind('directory'); setNewName(''); setDialog('create') }} disabled={!!busy}><FolderPlus size={14}/>پوشه</Button><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => { setCreateKind('file'); setNewName(''); setDialog('create') }} disabled={!!busy}><FilePlus2 size={14}/>فایل</Button><Button className="w-full sm:w-auto" size="sm" onClick={() => uploadRef.current?.click()} disabled={!!busy}><Upload size={14}/>{uploading ? `آپلود ${uploading}…` : 'آپلود'}</Button><input ref={uploadRef} type="file" multiple className="hidden" onChange={(event) => void uploadFiles(event.target.files)}/></div>

  return <>
    <Heading title="مدیریت فایل‌ها" description="فایل‌ها و پوشه‌های سرور را مرور کن، ویرایش و جابه‌جا کن یا انتقال بده." action={toolbar}/>
    <Dialog open={diskScanOpen} onOpenChange={setDiskScanOpen}>
      <DialogContent className="max-h-[94vh] max-w-5xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2"><HardDrive size={18} className="text-primary"/>تحلیل فضای دیسک</DialogTitle>
          <DialogDescription>دیسک‌ها را مثل درایوهای سیستم ببین، مسیرهای پرمصرف را باز کن و پیش از هر پاک‌سازی، کاربردشان را بررسی کن.</DialogDescription>
        </DialogHeader>
        <section className="space-y-3 rounded-xl border border-border bg-background/30 p-3 sm:p-4" aria-label="دیسک‌ها و فایل‌سیستم‌ها">
          <div className="flex flex-wrap items-center justify-between gap-2"><div><h3 className="text-sm font-semibold">دیسک‌ها و Mountها</h3><p className="mt-1 text-[10px] text-muted-foreground">نمایش ظرفیت مشابه درایوهای ویندوز · تحلیل فقط فایل‌سیستم محلی را اسکن می‌کند.</p></div><Button size="sm" variant="outline" onClick={() => setDiskPath('/')}><HardDrive size={14}/>انتخاب ریشه</Button></div>
          {diskVolumesLoading ? <div className="grid gap-2 sm:grid-cols-2">{[0,1].map((item) => <Skeleton key={item} className="h-28 rounded-xl"/> )}</div> : diskVolumesError ? <p className="rounded-lg border border-amber-400/20 p-3 text-xs text-amber-100">{diskVolumesError}</p> : <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{diskVolumes.filter((volume) => volume.mount_status === 'mounted' && volume.kind === 'local' && volume.total_bytes !== null).map((volume) => {
            const percent = volume.use_percent === null ? null : Math.max(0, Math.min(100, volume.use_percent))
            const fill = percent !== null && percent >= 90 ? 'bg-rose-400' : percent !== null && percent >= 75 ? 'bg-amber-300' : 'bg-primary'
            const selectedVolume = diskPath === volume.mount_point
            return <button key={volume.id} type="button" onClick={() => setDiskPath(volume.mount_point)} aria-pressed={selectedVolume} className={`min-w-0 rounded-xl border p-3 text-right transition ${selectedVolume ? 'border-primary/45 bg-primary/[.06]' : 'border-border bg-card/55 hover:border-primary/25'}`}>
              <div className="flex items-start justify-between gap-2"><span className="flex min-w-0 items-center gap-2"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg bg-primary/[.08] text-primary"><HardDrive size={16}/></span><span className="min-w-0"><b dir="ltr" className="block truncate text-left font-mono text-xs">{volume.mount_point} <span className="text-muted-foreground">· {volume.device}</span></b><span className="mt-1 block truncate text-[10px] text-muted-foreground">{volume.filesystem}{volume.read_only ? ' · فقط‌خواندنی' : ''}</span></span></span><span dir="ltr" className="shrink-0 font-mono text-xs tabular-nums">{percent === null ? '—' : `${percent.toFixed(1)}%`}</span></div>
              <div className="mt-3 h-2 overflow-hidden rounded-full bg-secondary" role="progressbar" aria-label={`مصرف دیسک ${volume.mount_point}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? 0}><div className={`h-full rounded-full transition-[width] ${fill}`} style={{ width: `${percent ?? 0}%` }}/></div>
              <div className="mt-2 flex justify-between gap-2 text-[10px] text-muted-foreground"><span dir="ltr" className="font-mono">{volume.used_bytes === null ? 'نامشخص' : prettyBytes(volume.used_bytes)} مصرف</span><span dir="ltr" className="font-mono">{volume.available_bytes === null ? 'نامشخص' : prettyBytes(volume.available_bytes)} آزاد · {volume.total_bytes === null ? '—' : prettyBytes(volume.total_bytes)}</span></div>
            </button>
          })}{!diskVolumes.some((volume) => volume.kind === 'local' && volume.mount_status === 'mounted' && volume.total_bytes !== null) && <p className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground sm:col-span-2 xl:col-span-3">اطلاعات درایو محلی در دسترس نیست؛ مسیر را دستی وارد و اسکن کن.</p>}</div>}
          {!!diskVolumes.filter((volume) => volume.kind === 'cloud' || volume.kind === 'remote').length && <p className="text-[10px] leading-5 text-muted-foreground">{diskVolumes.filter((volume) => volume.kind === 'cloud' || volume.kind === 'remote').length} Mount ابری/شبکه‌ای جدا از فضای محلی نمایش داده شده‌اند؛ ظرفیت آن‌ها با اسکن دیسک محلی جمع نمی‌شود.</p>}
        </section>
        <form className="space-y-3" onSubmit={(event) => { event.preventDefault(); void scanDisk() }}>
          <div className="flex flex-col gap-2 sm:flex-row"><Input aria-label="مسیر شروع اسکن" dir="ltr" className="font-mono text-left" value={diskPath} onChange={(event) => setDiskPath(event.target.value)} placeholder="/"/><div className="flex gap-2"><Button type="submit" className="shrink-0" disabled={diskScanBusy || !diskPath.startsWith('/')}>{diskScanBusy ? <LoaderCircle size={15} className="animate-spin"/> : <Search size={15}/>}اسکن این مسیر</Button>{parentDiskPath && <Button type="button" variant="outline" disabled={diskScanBusy} onClick={() => void scanDisk(parentDiskPath)}><ArrowUp size={14}/>پوشهٔ والد</Button>}</div></div>
          <div className="grid gap-2 sm:grid-cols-2"><button type="button" aria-pressed={diskScanMode === 'standard'} onClick={() => setDiskScanMode('standard')} className={`rounded-xl border p-3 text-right transition ${diskScanMode === 'standard' ? 'border-primary/40 bg-primary/[.07]' : 'border-white/10 hover:bg-white/[.03]'}`}><span className="block text-xs font-semibold text-white">اسکن استاندارد</span><span className="mt-1 block text-[10px] leading-4 text-muted-foreground">حداکثر 100,000 مورد یا 12 ثانیه؛ مناسب بررسی روزمره</span></button><button type="button" aria-pressed={diskScanMode === 'deep'} onClick={() => setDiskScanMode('deep')} className={`rounded-xl border p-3 text-right transition ${diskScanMode === 'deep' ? 'border-amber-300/40 bg-amber-300/[.06]' : 'border-white/10 hover:bg-white/[.03]'}`}><span className="block text-xs font-semibold text-white">اسکن عمیق · محدودیت گسترده‌تر</span><span className="mt-1 block text-[10px] leading-4 text-muted-foreground">حداکثر 500,000 مورد یا 90 ثانیه؛ ممکن است CPU و دیسک را موقتاً درگیر کند.</span></button></div>
        </form>
        {diskScanBusy && <div className="space-y-2 rounded-xl border border-primary/20 bg-primary/[.04] p-4"><div className="flex items-center gap-2 text-sm"><LoaderCircle size={15} className="animate-spin text-primary"/>در حال بررسی اندازهٔ فایل‌ها و پوشه‌ها…</div><div className="h-1.5 overflow-hidden rounded-full bg-white/10"><div className="h-full w-1/3 animate-pulse rounded-full bg-primary"/></div></div>}
        {diskScanError && <Alert className="border-rose-400/25 bg-rose-400/[.06]"><AlertDescription>{diskScanError}</AlertDescription></Alert>}
        {diskScan && <div className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4"><Card><CardContent className="p-4"><p className="text-xs text-muted-foreground">اندازهٔ محتوا در این مسیر</p><p dir="ltr" className="mt-2 text-left font-mono text-xl font-semibold">{prettyBytes(diskScan.total_bytes)}</p></CardContent></Card><Card><CardContent className="p-4"><p className="text-xs text-muted-foreground">مصرف فایل‌سیستم</p><p dir="ltr" className="mt-2 text-left font-mono text-xl font-semibold">{diskScan.usage_percent.toFixed(1)}%</p><p dir="ltr" className="mt-1 text-left text-[10px] text-muted-foreground">{prettyBytes(diskScan.volume_used)} / {prettyBytes(diskScan.volume_total)} · آزاد {prettyBytes(diskScan.volume_available)}</p></CardContent></Card><Card><CardContent className="p-4"><p className="text-xs text-muted-foreground">موارد بررسی‌شده</p><p dir="ltr" className="mt-2 text-left font-mono text-xl font-semibold">{diskScan.entries_scanned.toLocaleString('en-US')}</p></CardContent></Card><Card><CardContent className="p-4"><p className="text-xs text-muted-foreground">زمان اسکن</p><p dir="ltr" className="mt-2 text-left font-mono text-xl font-semibold">{(diskScan.elapsed_ms / 1000).toFixed(1)}s {diskScan.truncated && <span className="text-xs text-amber-300">· ناقص</span>}</p></CardContent></Card></div>
          <section className="rounded-xl border border-primary/20 bg-primary/[.035] p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><p className="text-[10px] uppercase tracking-wider text-primary">گزارش این مسیر</p><h3 className="mt-1 text-sm font-semibold">{diskPathAdvice(diskScan.root).title}</h3><p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">{diskPathAdvice(diskScan.root).detail}</p></div><code dir="ltr" className="max-w-full break-all rounded-md border border-border bg-background/50 px-2 py-1 font-mono text-[10px]">{diskScan.root}</code></div>{diskScan.truncated && <p className="mt-3 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-2 text-[10px] leading-5 text-amber-100">اسکن به سقف زمان یا تعداد موارد رسید؛ اعداد این مسیر ممکن است کامل نباشند. برای گزارش دقیق‌تر، همین زیرپوشه را جداگانه باز و اسکن کن.</p>}<p className="mt-3 text-[10px] leading-5 text-muted-foreground">این گزارش چیزی را خودکار حذف نمی‌کند. برای پاک‌سازی، زیرشاخه را باز کن و فایل‌هایش را با توجه به سرویس مالک بررسی کن؛ پوشه‌های سیستمی یا فعال را دستی پاک نکن.</p></section>
          <div className="grid gap-4 lg:grid-cols-2">
            <section className="min-w-0 rounded-xl border border-white/10 p-3"><div className="mb-3 flex items-center justify-between gap-2"><h3 className="flex items-center gap-2 text-sm font-semibold"><Folder size={15} className="text-primary"/>زیرشاخه‌های این مسیر</h3><Badge variant="outline">{(diskScan.directory_children || []).length}{diskScan.directory_children_truncated ? '+' : ''} پوشه</Badge></div><div className="max-h-80 space-y-1 overflow-auto">{(diskScan.directory_children || []).map((item) => { const advice = diskPathAdvice(item.path); return <div key={item.path} className="rounded-lg border border-transparent p-2 transition hover:border-border hover:bg-white/[.025]"><div className="flex items-start gap-2"><button type="button" disabled={diskScanBusy} onClick={() => void scanDisk(item.path)} className="mt-0.5 shrink-0 text-primary hover:text-white" aria-label={`بازکردن ${item.path}`}><Folder size={15}/></button><div className="min-w-0 flex-1"><div className="flex flex-wrap items-start justify-between gap-2"><button type="button" disabled={diskScanBusy} onClick={() => void scanDisk(item.path)} dir="ltr" className="min-w-0 break-all text-left font-mono text-[11px] hover:text-primary">{item.path}</button><span dir="ltr" className="shrink-0 font-mono text-[10px] text-muted-foreground">{prettyBytes(item.bytes)}</span></div><p className="mt-1 text-[10px] leading-4 text-muted-foreground"><b className="text-foreground/80">{advice.title}.</b> {advice.detail}</p><div className="mt-2 h-1 overflow-hidden rounded-full bg-white/10"><div className="h-full rounded-full bg-primary/80" style={{ width: `${Math.max(1, Math.min(100, 100 * item.bytes / Math.max(1, diskScan.total_bytes)))}%` }}/></div></div></div></div>})}{!diskScan.directory_children?.length && <p className="rounded-lg border border-dashed border-border p-5 text-center text-xs text-muted-foreground">زیرپوشه‌ای برای نمایش پیدا نشد.</p>}{diskScan.directory_children_truncated && <p className="p-2 text-[10px] text-amber-200">فقط 2,000 زیرشاخهٔ بزرگ‌تر نمایش داده شده؛ برای دیدن جزئیات مسیرهای دیگر همان مسیر را جداگانه اسکن کن.</p>}</div></section>
            <section className="min-w-0 rounded-xl border border-white/10 p-3"><h3 className="mb-3 flex items-center gap-2 text-sm font-semibold"><FileText size={15} className="text-primary"/>فایل‌های بزرگ</h3><div className="max-h-80 space-y-1 overflow-auto">{diskScan.largest_files.slice(0,40).map((item) => { const advice = diskPathAdvice(item.path.slice(0, item.path.lastIndexOf('/')) || '/'); return <div key={item.path} className="rounded-lg p-2 transition hover:bg-white/[.04]"><div className="flex items-start justify-between gap-2"><button type="button" onClick={() => openDiskPath(item.path.slice(0, item.path.lastIndexOf('/')) || '/')} dir="ltr" className="min-w-0 break-all text-left font-mono text-[11px] hover:text-primary">{item.path}</button><span dir="ltr" className="shrink-0 font-mono text-[10px] text-muted-foreground">{prettyBytes(item.bytes)}</span></div><p className="mt-1 text-[10px] text-muted-foreground">{advice.title}: {advice.detail}</p></div>})}{!diskScan.largest_files.length && <p className="p-5 text-center text-xs text-muted-foreground">فایل بزرگی در این مسیر پیدا نشد.</p>}</div></section>
          </div>
          <section className="rounded-xl border border-amber-400/15 bg-amber-400/[.025] p-3"><h3 className="text-xs font-semibold text-amber-100">پاک‌سازی امن از کجا شروع شود؟</h3><div className="mt-2 grid gap-2 sm:grid-cols-2"><button type="button" onClick={() => { setDiskScanOpen(false); window.location.hash = '/packages' }} className="rounded-lg border border-border bg-background/30 p-3 text-right text-[10px] leading-5 hover:border-primary/30"><b className="block text-xs">کش APT</b>مدیریت بسته‌ها را باز کن تا پاک‌سازی با خود APT انجام شود.</button><button type="button" onClick={() => { setDiskScanOpen(false); window.location.hash = '/docker' }} className="rounded-lg border border-border bg-background/30 p-3 text-right text-[10px] leading-5 hover:border-primary/30"><b className="block text-xs">داده‌های Docker</b>تصویر و Cache را از صفحهٔ Docker بررسی کن؛ لایه‌های فعال را از فایل‌سیستم دستی حذف نکن.</button><button type="button" onClick={() => openDiskPath('/var/log')} className="rounded-lg border border-border bg-background/30 p-3 text-right text-[10px] leading-5 hover:border-primary/30"><b className="block text-xs">لاگ‌ها</b>فایل‌های لاگ و journal را جداگانه ببین؛ فایل لاگ فعال را دستی پاک نکن.</button><button type="button" onClick={() => openDiskPath('/opt/arg/source/target')} className="rounded-lg border border-border bg-background/30 p-3 text-right text-[10px] leading-5 hover:border-primary/30"><b className="block text-xs">کش کامپایل ARG</b>برای آزادکردن آن از «آپدیت کامل» استفاده کن؛ آپدیت بعدی طولانی‌تر می‌شود.</button></div></section>
          <p className="text-[10px] leading-5 text-muted-foreground">{diskScan.note}</p>
        </div>}
      </DialogContent>
    </Dialog>
    {error && <Alert className="mb-4 border-rose-400/25 bg-rose-400/[.06]"><AlertDescription>{error}</AlertDescription></Alert>}
    <Card className="mb-4 overflow-hidden"><CardContent className="space-y-4 p-3 sm:p-4">
      <form className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 sm:flex" onSubmit={(event) => { event.preventDefault(); void load(pathInput) }}><div className="relative col-span-2 min-w-0 sm:col-span-1 sm:flex-1"><HardDrive size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="h-11 w-full pr-9 font-mono text-left" dir="ltr" value={pathInput} onChange={(event) => setPathInput(event.target.value)} aria-label="مسیر پوشه"/></div><Button type="submit" size="sm" variant="outline" disabled={loading}>رفتن</Button><Button type="button" size="sm" variant="outline" disabled={!listing?.parent || loading} onClick={() => listing?.parent && void load(listing.parent)}><ArrowUp size={14}/><span className="hidden xs:inline sm:inline">پوشهٔ بالاتر</span><span className="sm:hidden">بالاتر</span></Button></form>
      <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-center sm:justify-between"><div className="flex min-w-0 flex-wrap items-center gap-2"><Folder size={15} className="shrink-0 text-primary"/><code dir="ltr" className="min-w-0 max-w-full truncate text-xs text-muted-foreground">{listing?.path || path}</code><Badge variant="outline">{(listing?.entries || []).length.toLocaleString('en-US')} مورد</Badge>{listing?.truncated && <Badge variant="warning">نمایش 2000 مورد اول</Badge>}</div></div>
    </CardContent></Card>
    {clipboard && <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/20 bg-primary/[.06] p-3 text-xs"><span className="flex min-w-0 items-center gap-2"><Clipboard size={15} className="shrink-0 text-primary"/><span className="truncate">{clipboard.mode === 'move' ? 'برای انتقال انتخاب شد:' : 'برای کپی انتخاب شد:'} <b dir="ltr">{clipboard.name}</b></span></span><div className="flex gap-2"><Button size="sm" onClick={() => void paste()} disabled={!!busy || loading}><ClipboardPaste size={14}/>چسباندن اینجا</Button><Button size="sm" variant="ghost" onClick={() => setClipboard(null)}><X size={14}/>لغو</Button></div></div>}
    <FileManagerBrowser title="محتویات پوشه" description="فایل‌های مخفی هم نمایش داده می‌شوند؛ symbolic link مقصد خود را نشان می‌دهد و بازکردن پوشه از داخل آن انجام نمی‌شود." entries={browserEntries} loading={loading} onNavigate={(nextPath) => void load(nextPath)} onOpen={(item) => { const entry = entries.find((candidate) => candidate.path === item.path); if (entry?.kind === 'file') void openEditor(entry); else if (entry?.kind === 'symlink') openSymlinkEditor(entry); else if (entry) setSelected(entry) }} renderActions={renderFileActions} emptyMessage="این پوشه خالی است." />
    <Dialog open={dialog === 'create'} onOpenChange={(open) => !busy && setDialog(open ? 'create' : null)}><DialogContent><DialogHeader><DialogTitle>{createKind === 'directory' ? 'ساخت پوشه' : 'ساخت فایل'}</DialogTitle><DialogDescription>مورد جدید در مسیر <code dir="ltr">{path}</code> ساخته می‌شود.</DialogDescription></DialogHeader><form onSubmit={submitCreate} className="space-y-4"><div className="grid grid-cols-2 gap-2"><Button type="button" variant={createKind === 'file' ? 'default' : 'outline'} onClick={() => setCreateKind('file')}><File size={15}/>فایل</Button><Button type="button" variant={createKind === 'directory' ? 'default' : 'outline'} onClick={() => setCreateKind('directory')}><Folder size={15}/>پوشه</Button></div><Input autoFocus value={newName} onChange={(event) => setNewName(event.target.value)} placeholder={createKind === 'directory' ? 'نام پوشه' : 'نام فایل'} dir="auto" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}ساخت</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'rename'} onOpenChange={(open) => !busy && setDialog(open ? 'rename' : null)}><DialogContent><DialogHeader><DialogTitle>تغییر نام</DialogTitle><DialogDescription>نام جدید برای <code dir="ltr">{selected?.name}</code> وارد کن.</DialogDescription></DialogHeader><form onSubmit={submitRename} className="space-y-4"><Input autoFocus value={newName} onChange={(event) => setNewName(event.target.value)} dir="auto" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}ذخیرهٔ نام</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'symlink'} onOpenChange={(open) => { if (!busy) { setDialog(open ? 'symlink' : null); if (!open) setReviewLinkTarget(false) } }}><DialogContent><DialogHeader><DialogTitle>ویرایش مقصد پیوند نمادین</DialogTitle><DialogDescription>فقط خود پیوند <code dir="ltr">{selected?.path}</code> جایگزین می‌شود؛ فایل یا پوشهٔ مقصد بازنویسی نمی‌شود. مقصد می‌تواند مسیر مطلق یا نسبی باشد.</DialogDescription></DialogHeader><form onSubmit={submitSymlinkTarget} className="space-y-4"><div className="space-y-2"><label htmlFor="symlink-target" className="text-sm font-medium">مقصد فعلی / جدید</label><Input id="symlink-target" autoFocus dir="ltr" className="font-mono text-left" value={linkTarget} onChange={(event) => { setLinkTarget(event.target.value); setReviewLinkTarget(false) }} placeholder="مثلاً ../releases/current" required maxLength={4096}/></div>{reviewLinkTarget ? <div className="space-y-2 rounded-xl border border-amber-400/25 bg-amber-400/[.05] p-3 text-xs"><p className="font-semibold text-amber-100">تغییر مقصد را تأیید کن</p><p className="text-muted-foreground">از:</p><code dir="ltr" className="block break-all text-left">{selected?.link_target || 'مقصد قبلی قابل خواندن نیست'}</code><p className="text-muted-foreground">به:</p><code dir="ltr" className="block break-all text-left">{linkTarget}</code></div> : null}<div className="flex gap-2">{reviewLinkTarget && <Button type="button" variant="outline" className="flex-1" onClick={() => setReviewLinkTarget(false)}>بازگشت و ویرایش</Button>}<Button type="submit" className="flex-1" disabled={!!busy || !linkTarget.trim()}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}{reviewLinkTarget ? 'تأیید و ثبت مقصد' : 'بررسی تغییر'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={dialog === 'permissions'} onOpenChange={(open) => !busy && setDialog(open ? 'permissions' : null)}><DialogContent><DialogHeader><DialogTitle>دسترسی فایل</DialogTitle><DialogDescription>مجوزهای Unix را با عدد اکتال وارد کن؛ مثل <code dir="ltr">0644</code> برای فایل معمولی یا <code dir="ltr">0755</code> برای اسکریپت و پوشه.</DialogDescription></DialogHeader><form onSubmit={submitPermissions} className="space-y-4"><Input autoFocus inputMode="numeric" maxLength={4} pattern="[0-7]{3,4}" value={permissions} onChange={(event) => setPermissions(event.target.value)} dir="ltr" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}اعمال دسترسی</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'edit'} onOpenChange={(open) => !busy && setDialog(open ? 'edit' : null)}><DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>ویرایش فایل</DialogTitle><DialogDescription dir="ltr" className="break-all text-left font-mono">{selected?.path}</DialogDescription></DialogHeader><textarea className="min-h-[55vh] w-full resize-y rounded-xl border border-border bg-background p-4 text-left font-mono text-xs leading-6 outline-none focus:ring-2 focus:ring-ring" dir="auto" spellCheck={false} value={editor} onChange={(event) => setEditor(event.target.value)}/><div className="flex flex-wrap justify-between gap-2"><span className="self-center text-[11px] text-muted-foreground">UTF-8 · محدودیت ویرایش 1 MiB</span><div className="flex gap-2"><Button variant="outline" onClick={() => selected && void download(selected)}><ArrowDownToLine size={14}/>دانلود اصلی</Button><Button onClick={() => void saveEditor()} disabled={!!busy}>{busy === 'save' ? <LoaderCircle size={14} className="animate-spin"/> : <Check size={14}/>}ذخیره</Button></div></div></DialogContent></Dialog>
  </>
}
