import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { Archive, ArrowDownToLine, ArrowUp, Check, Clipboard, ClipboardPaste, Copy, File, FilePlus2, FileText, Folder, FolderPlus, HardDrive, LoaderCircle, Pencil, RefreshCw, Search, Scissors, Settings2, Trash2, Upload, X } from 'lucide-react'
import { api } from './lib/api'
import { formatPanelDate, usePanelTimeZone } from './lib/timezone'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Alert, AlertDescription } from './components/ui/alert'
import { Skeleton } from './components/ui/skeleton'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Entry = { name: string; path: string; kind: 'directory' | 'file' | 'symlink' | 'other'; size: number; modified: number | null; permissions: string; read_only: boolean }
type Listing = { path: string; parent: string | null; entries: Entry[]; truncated: boolean }
type ClipboardItem = { path: string; name: string; mode: 'copy' | 'move' }
type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode

function prettyBytes(bytes: number) {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / 1024 ** index).toLocaleString('en-US', { maximumFractionDigits: 1 })} ${units[index]}`
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
  const [query, setQuery] = useState('')
  const [clipboard, setClipboard] = useState<ClipboardItem | null>(null)
  const [dialog, setDialog] = useState<'create' | 'rename' | 'permissions' | 'edit' | null>(null)
  const [createKind, setCreateKind] = useState<'file' | 'directory'>('file')
  const [newName, setNewName] = useState('')
  const [permissions, setPermissions] = useState('0644')
  const [selected, setSelected] = useState<Entry | null>(null)
  const [editor, setEditor] = useState('')
  const [uploading, setUploading] = useState('')
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

  const entries = useMemo(() => (listing?.entries || []).filter((entry) => entry.name.toLowerCase().includes(query.trim().toLowerCase())), [listing, query])
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
  const date = (seconds: number | null) => formatPanelDate(seconds, timeZone, { dateStyle: 'medium', timeStyle: 'short' }, 'en-GB')
  const toolbar = <div className="grid w-full grid-cols-2 gap-2 sm:w-auto sm:flex sm:flex-wrap"><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => void load(path)} disabled={loading || !!busy}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>تازه‌سازی</Button><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => { setCreateKind('directory'); setNewName(''); setDialog('create') }} disabled={!!busy}><FolderPlus size={14}/>پوشه</Button><Button className="w-full sm:w-auto" size="sm" variant="outline" onClick={() => { setCreateKind('file'); setNewName(''); setDialog('create') }} disabled={!!busy}><FilePlus2 size={14}/>فایل</Button><Button className="w-full sm:w-auto" size="sm" onClick={() => uploadRef.current?.click()} disabled={!!busy}><Upload size={14}/>{uploading ? `آپلود ${uploading}…` : 'آپلود'}</Button><input ref={uploadRef} type="file" multiple className="hidden" onChange={(event) => void uploadFiles(event.target.files)}/></div>

  return <>
    <Heading title="مدیریت فایل‌ها" description="فایل‌ها و پوشه‌های سرور را مرور کن، ویرایش و جابه‌جا کن یا انتقال بده." action={toolbar}/>
    {error && <Alert className="mb-4 border-rose-400/25 bg-rose-400/[.06]"><AlertDescription>{error}</AlertDescription></Alert>}
    <Card className="mb-4 overflow-hidden"><CardContent className="space-y-4 p-3 sm:p-4">
      <form className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 sm:flex" onSubmit={(event) => { event.preventDefault(); void load(pathInput) }}><div className="relative col-span-2 min-w-0 sm:col-span-1 sm:flex-1"><HardDrive size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="h-11 w-full pr-9 font-mono text-left" dir="ltr" value={pathInput} onChange={(event) => setPathInput(event.target.value)} aria-label="مسیر پوشه"/></div><Button type="submit" size="sm" variant="outline" disabled={loading}>رفتن</Button><Button type="button" size="sm" variant="outline" disabled={!listing?.parent || loading} onClick={() => listing?.parent && void load(listing.parent)}><ArrowUp size={14}/><span className="hidden xs:inline sm:inline">پوشهٔ بالاتر</span><span className="sm:hidden">بالاتر</span></Button></form>
      <div className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-center sm:justify-between"><div className="flex min-w-0 flex-wrap items-center gap-2"><Folder size={15} className="shrink-0 text-primary"/><code dir="ltr" className="min-w-0 max-w-full truncate text-xs text-muted-foreground">{listing?.path || path}</code><Badge variant="outline">{(listing?.entries || []).length.toLocaleString('en-US')} مورد</Badge>{listing?.truncated && <Badge variant="warning">نمایش 2000 مورد اول</Badge>}</div><div className="relative w-full sm:w-56 sm:shrink-0"><Search size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input className="h-10 pr-9" placeholder="جست‌وجو در این پوشه…" value={query} onChange={(event) => setQuery(event.target.value)}/></div></div>
    </CardContent></Card>
    {clipboard && <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/20 bg-primary/[.06] p-3 text-xs"><span className="flex min-w-0 items-center gap-2"><Clipboard size={15} className="shrink-0 text-primary"/><span className="truncate">{clipboard.mode === 'move' ? 'برای انتقال انتخاب شد:' : 'برای کپی انتخاب شد:'} <b dir="ltr">{clipboard.name}</b></span></span><div className="flex gap-2"><Button size="sm" onClick={() => void paste()} disabled={!!busy || loading}><ClipboardPaste size={14}/>چسباندن اینجا</Button><Button size="sm" variant="ghost" onClick={() => setClipboard(null)}><X size={14}/>لغو</Button></div></div>}
    <Card className="overflow-hidden"><CardHeader className="p-4 pb-3 sm:p-6 sm:pb-3"><CardTitle>محتویات پوشه</CardTitle><CardDescription className="leading-6">فایل‌های مخفی هم نمایش داده می‌شوند. پیوندهای نمادین مشخص شده‌اند و بازکردن پوشه از داخل آن‌ها انجام نمی‌شود.</CardDescription></CardHeader><CardContent className="p-3 pt-0 sm:p-6 sm:pt-0">
      {loading ? <div className="space-y-2">{Array.from({ length: 7 }, (_, index) => <Skeleton key={index} className="h-12 rounded-xl"/>)}</div> : <><div className="hidden overflow-x-auto md:block"><table className="w-full min-w-[680px] text-sm"><thead><tr className="border-b border-border text-right text-[11px] text-muted-foreground"><th className="px-3 py-3 font-medium">نام</th><th className="px-3 py-3 font-medium">نوع</th><th className="px-3 py-3 font-medium">اندازه</th><th className="px-3 py-3 font-medium">دسترسی</th><th className="px-3 py-3 font-medium">آخرین تغییر</th><th className="px-3 py-3 font-medium">عملیات</th></tr></thead><tbody>{entries.map((entry) => <tr key={entry.path} className="group border-b border-border/60 transition-colors hover:bg-white/[.025]"><td className="max-w-[320px] px-3 py-2.5"><button className="flex max-w-full items-center gap-2 text-right hover:text-primary" onClick={() => entry.kind === 'directory' ? void load(entry.path) : entry.kind === 'file' ? void openEditor(entry) : setSelected(entry)}><span className="shrink-0 text-primary/90">{entry.kind === 'directory' ? <Folder size={16}/> : entry.kind === 'symlink' ? <ArrowDownToLine size={16}/> : entry.kind === 'file' ? <FileText size={16}/> : <Archive size={16}/>}</span><span dir="ltr" className="truncate text-left font-mono text-xs">{entry.name}</span>{entry.kind === 'symlink' && <Badge variant="warning">symlink</Badge>}</button></td><td className="px-3 py-2.5 text-xs text-muted-foreground">{entry.kind === 'directory' ? 'پوشه' : entry.kind === 'symlink' ? 'پیوند' : entry.kind === 'file' ? 'فایل' : 'سایر'}</td><td dir="ltr" className="px-3 py-2.5 text-left font-mono text-[11px] text-muted-foreground">{entry.kind === 'file' ? prettyBytes(entry.size) : '—'}</td><td dir="ltr" className="px-3 py-2.5 font-mono text-[11px] text-muted-foreground">{entry.permissions}</td><td className="px-3 py-2.5 text-xs text-muted-foreground">{date(entry.modified)}</td><td><div className="flex items-center gap-0.5 px-2 opacity-75 transition group-hover:opacity-100"><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="کپی" title="کپی" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'copy' })}><Copy size={14}/></Button><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="کات" title="کات / انتقال" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'move' })}><Scissors size={14}/></Button><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="تغییر نام" title="تغییر نام" onClick={() => { setSelected(entry); setNewName(entry.name); setDialog('rename') }}><Pencil size={14}/></Button><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="تغییر دسترسی" title="تغییر دسترسی" onClick={() => { setSelected(entry); setPermissions(entry.permissions); setDialog('permissions') }}><Settings2 size={14}/></Button>{entry.kind === 'file' && <><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="دانلود" title="دانلود" disabled={!!busy} onClick={() => void download(entry)}>{busy === `download:${entry.path}` ? <LoaderCircle size={14} className="animate-spin"/> : <ArrowDownToLine size={14}/>}</Button><Button size="icon" variant="ghost" className="h-8 w-8" aria-label="ویرایش" title="ویرایش متن" disabled={!!busy} onClick={() => void openEditor(entry)}><FileText size={14}/></Button></>}<Button size="icon" variant="ghost" className="h-8 w-8 text-rose-300 hover:text-rose-200" aria-label="حذف" title="حذف" disabled={!!busy} onClick={() => void remove(entry)}><Trash2 size={14}/></Button></div></td></tr>)}</tbody></table></div><div className="space-y-2 md:hidden">{entries.map((entry) => <article key={entry.path} className="rounded-xl border border-border/80 bg-background/35 p-3"><div className="flex min-w-0 items-start justify-between gap-2"><button className="flex min-w-0 items-start gap-2 text-right hover:text-primary" onClick={() => entry.kind === 'directory' ? void load(entry.path) : entry.kind === 'file' ? void openEditor(entry) : setSelected(entry)}><span className="mt-0.5 shrink-0 text-primary/90">{entry.kind === 'directory' ? <Folder size={17}/> : entry.kind === 'symlink' ? <ArrowDownToLine size={17}/> : entry.kind === 'file' ? <FileText size={17}/> : <Archive size={17}/>}</span><span className="min-w-0"><span dir="ltr" className="block max-w-full break-all text-left font-mono text-sm">{entry.name}</span><span className="mt-1 block text-xs text-muted-foreground">{entry.kind === 'directory' ? 'پوشه' : entry.kind === 'symlink' ? 'پیوند نمادین' : entry.kind === 'file' ? prettyBytes(entry.size) : 'سایر'} · <span dir="ltr" className="font-mono">{entry.permissions}</span></span></span></button><span className="shrink-0 text-[10px] text-muted-foreground">{date(entry.modified)}</span></div><div className="mt-3 grid grid-cols-4 gap-1 border-t border-border/60 pt-2"><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="کپی" title="کپی" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'copy' })}><Copy size={15}/></Button><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="کات" title="کات / انتقال" onClick={() => setClipboard({ path: entry.path, name: entry.name, mode: 'move' })}><Scissors size={15}/></Button><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="تغییر نام" title="تغییر نام" onClick={() => { setSelected(entry); setNewName(entry.name); setDialog('rename') }}><Pencil size={15}/></Button><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="تغییر دسترسی" title="تغییر دسترسی" onClick={() => { setSelected(entry); setPermissions(entry.permissions); setDialog('permissions') }}><Settings2 size={15}/></Button>{entry.kind === 'file' && <><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="دانلود" title="دانلود" disabled={!!busy} onClick={() => void download(entry)}>{busy === `download:${entry.path}` ? <LoaderCircle size={15} className="animate-spin"/> : <ArrowDownToLine size={15}/>}</Button><Button size="icon" variant="ghost" className="h-9 w-full" aria-label="ویرایش" title="ویرایش متن" disabled={!!busy} onClick={() => void openEditor(entry)}><FileText size={15}/></Button></>}<Button size="icon" variant="ghost" className="h-9 w-full text-rose-300 hover:text-rose-200" aria-label="حذف" title="حذف" disabled={!!busy} onClick={() => void remove(entry)}><Trash2 size={15}/></Button></div></article>)}</div>{!entries.length && <div className="grid min-h-44 place-items-center text-center text-sm text-muted-foreground"><div><Folder size={25} className="mx-auto mb-2 opacity-40"/>{query ? 'موردی با این نام پیدا نشد.' : 'این پوشه خالی است.'}</div></div>}</>}
    </CardContent></Card>
    <Dialog open={dialog === 'create'} onOpenChange={(open) => !busy && setDialog(open ? 'create' : null)}><DialogContent><DialogHeader><DialogTitle>{createKind === 'directory' ? 'ساخت پوشه' : 'ساخت فایل'}</DialogTitle><DialogDescription>مورد جدید در مسیر <code dir="ltr">{path}</code> ساخته می‌شود.</DialogDescription></DialogHeader><form onSubmit={submitCreate} className="space-y-4"><div className="grid grid-cols-2 gap-2"><Button type="button" variant={createKind === 'file' ? 'default' : 'outline'} onClick={() => setCreateKind('file')}><File size={15}/>فایل</Button><Button type="button" variant={createKind === 'directory' ? 'default' : 'outline'} onClick={() => setCreateKind('directory')}><Folder size={15}/>پوشه</Button></div><Input autoFocus value={newName} onChange={(event) => setNewName(event.target.value)} placeholder={createKind === 'directory' ? 'نام پوشه' : 'نام فایل'} dir="auto" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}ساخت</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'rename'} onOpenChange={(open) => !busy && setDialog(open ? 'rename' : null)}><DialogContent><DialogHeader><DialogTitle>تغییر نام</DialogTitle><DialogDescription>نام جدید برای <code dir="ltr">{selected?.name}</code> وارد کن.</DialogDescription></DialogHeader><form onSubmit={submitRename} className="space-y-4"><Input autoFocus value={newName} onChange={(event) => setNewName(event.target.value)} dir="auto" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}ذخیرهٔ نام</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'permissions'} onOpenChange={(open) => !busy && setDialog(open ? 'permissions' : null)}><DialogContent><DialogHeader><DialogTitle>دسترسی فایل</DialogTitle><DialogDescription>مجوزهای Unix را با عدد اکتال وارد کن؛ مثل <code dir="ltr">0644</code> برای فایل معمولی یا <code dir="ltr">0755</code> برای اسکریپت و پوشه.</DialogDescription></DialogHeader><form onSubmit={submitPermissions} className="space-y-4"><Input autoFocus inputMode="numeric" maxLength={4} pattern="[0-7]{3,4}" value={permissions} onChange={(event) => setPermissions(event.target.value)} dir="ltr" required/><Button className="w-full" disabled={!!busy}>{busy ? <LoaderCircle size={15} className="animate-spin"/> : <Check size={15}/>}اعمال دسترسی</Button></form></DialogContent></Dialog>
    <Dialog open={dialog === 'edit'} onOpenChange={(open) => !busy && setDialog(open ? 'edit' : null)}><DialogContent className="max-h-[90vh] max-w-4xl overflow-y-auto"><DialogHeader><DialogTitle>ویرایش فایل</DialogTitle><DialogDescription dir="ltr" className="break-all text-left font-mono">{selected?.path}</DialogDescription></DialogHeader><textarea className="min-h-[55vh] w-full resize-y rounded-xl border border-border bg-background p-4 text-left font-mono text-xs leading-6 outline-none focus:ring-2 focus:ring-ring" dir="auto" spellCheck={false} value={editor} onChange={(event) => setEditor(event.target.value)}/><div className="flex flex-wrap justify-between gap-2"><span className="self-center text-[11px] text-muted-foreground">UTF-8 · محدودیت ویرایش 1 MiB</span><div className="flex gap-2"><Button variant="outline" onClick={() => selected && void download(selected)}><ArrowDownToLine size={14}/>دانلود اصلی</Button><Button onClick={() => void saveEditor()} disabled={!!busy}>{busy === 'save' ? <LoaderCircle size={14} className="animate-spin"/> : <Check size={14}/>}ذخیره</Button></div></div></DialogContent></Dialog>
  </>
}
