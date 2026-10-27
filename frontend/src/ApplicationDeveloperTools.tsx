import { useEffect, useState } from 'react'
import { ChevronDown, ChevronLeft, FileCode2, Folder, FolderOpen, GitBranch, LoaderCircle, Plus, RefreshCw, ShieldCheck, WandSparkles } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'

type Application = { id: string; name: string }
type BuildPlan = { id: string; release_id: string; source_commit?: string | null; created_at: number; plan: { framework: string; confidence: number; ambiguous: boolean; build_mode: string; commands: string[]; warnings: string[]; executes_commands: boolean } }
type FileEntry = { name: string; path: string; kind: 'directory' | 'file'; size_bytes: number }
type Snapshot = { id: string; label: string; sha256: string; created_at: number; source_commit?: string | null; restorable: boolean }

export function ApplicationDeveloperTools({ app, notify }: { app: Application; notify: (message: string) => void }) {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [plans, setPlans] = useState<BuildPlan[]>([])
  const [entries, setEntries] = useState<FileEntry[]>([])
  const [path, setPath] = useState('')
  const [file, setFile] = useState<{ path: string; content: string; source_commit?: string | null } | null>(null)
  const [snapshots, setSnapshots] = useState<Snapshot[]>([])

  async function loadPlans() {
    const result = await api<{ plans: BuildPlan[] }>(`apps/${encodeURIComponent(app.id)}/build-plans`)
    setPlans(result.plans || [])
  }
  async function loadTree(nextPath = '') {
    setBusy('tree'); setError(''); setFile(null)
    try {
      const result = await api<{ entries: FileEntry[] }>(`apps/${encodeURIComponent(app.id)}/source/tree?path=${encodeURIComponent(nextPath)}`)
      setPath(nextPath); setEntries(result.entries || [])
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'فهرست سورس خوانده نشد.') }
    finally { setBusy('') }
  }
  async function loadSnapshots() {
    const result = await api<{ snapshots: Snapshot[] }>(`apps/${encodeURIComponent(app.id)}/snapshots`)
    setSnapshots(result.snapshots || [])
  }
  useEffect(() => {
    if (!open) return
    let active = true
    setError('')
    void Promise.all([loadPlans(), loadTree(), loadSnapshots()]).catch((cause) => {
      if (active) setError(cause instanceof Error ? cause.message : 'اطلاعات ابزار توسعه خوانده نشد.')
    })
    return () => { active = false }
  }, [open, app.id])

  async function analyze() {
    setBusy('plan'); setError('')
    try {
      const result = await api<{ plan: BuildPlan['plan'] }>(`apps/${encodeURIComponent(app.id)}/build-plans`, { method: 'POST', body: JSON.stringify({}) })
      await loadPlans(); notify(`Build Plan آماده شد: ${result.plan.framework} · ${result.plan.confidence}%`)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تحلیل پروژه انجام نشد.') }
    finally { setBusy('') }
  }
  async function createSnapshot() {
    setBusy('snapshot'); setError('')
    try {
      await api(`apps/${encodeURIComponent(app.id)}/snapshots`, { method: 'POST', body: JSON.stringify({ label: 'Snapshot دستی' }) })
      await loadSnapshots(); notify('Snapshot تنظیمات ایجاد شد؛ مقدار Secretها در آن ذخیره نمی‌شود.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Snapshot ایجاد نشد.') }
    finally { setBusy('') }
  }
  async function openFile(entry: FileEntry) {
    if (entry.kind === 'directory') { await loadTree(entry.path); return }
    setBusy('file'); setError('')
    try {
      const result = await api<{ path: string; content: string; source_commit?: string | null }>(`apps/${encodeURIComponent(app.id)}/source/file?path=${encodeURIComponent(entry.path)}`)
      setFile(result)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'پیش‌نمایش این فایل ممکن نیست.') }
    finally { setBusy('') }
  }

  return <Card className="mt-3 border-border/70 bg-background/35">
    <CardHeader className="flex-row items-center justify-between gap-3 py-3">
      <div><CardTitle className="flex items-center gap-2 text-sm"><GitBranch size={15} className="text-primary"/>سورس، Build Plan و Snapshot</CardTitle><CardDescription className="mt-1 text-[10px]">ابزارهای خواندنی روی آخرین release موفق؛ هیچ build یا ویرایش خودکاری اجرا نمی‌شود.</CardDescription></div>
      <Button variant="outline" size="sm" onClick={() => setOpen(!open)} aria-expanded={open}>{open ? 'بستن' : 'بازکردن'}{open ? <ChevronDown size={14}/> : <ChevronLeft size={14}/>}</Button>
    </CardHeader>
    {open && <CardContent className="space-y-4 pt-1">
      {error && <p role="alert" className="rounded-lg border border-rose-300/20 bg-rose-400/5 p-2 text-xs text-rose-200">{error}</p>}
      <div className="grid gap-3 xl:grid-cols-2">
        <section className="space-y-2 rounded-xl border border-border/70 p-3">
          <div className="flex flex-wrap items-center justify-between gap-2"><div><h4 className="text-xs font-semibold">تشخیص و Build Plan</h4><p className="mt-1 text-[10px] text-muted-foreground">فقط manifestها بررسی و پیشنهاد ذخیره می‌شود.</p></div><Button size="sm" onClick={() => void analyze()} disabled={Boolean(busy)}><WandSparkles size={14}/>{busy === 'plan' ? 'در حال تحلیل' : 'تحلیل release'}</Button></div>
          {plans.slice(0, 3).map((item) => <div key={item.id} className="space-y-2 rounded-lg bg-muted/30 p-2.5"><div className="flex flex-wrap items-center gap-2"><Badge variant="outline">{item.plan.framework}</Badge><span className="text-[10px] text-muted-foreground">اطمینان {item.plan.confidence}%</span>{item.plan.ambiguous && <Badge variant="destructive">انتخاب مبهم</Badge>}</div><p dir="ltr" className="text-left font-mono text-[10px] text-muted-foreground">{item.plan.commands.length ? item.plan.commands.join('  →  ') : 'No automatic build command'}</p>{item.plan.warnings.map((warning) => <p key={warning} className="text-[10px] leading-5 text-amber-100">{warning}</p>)}<p className="flex items-center gap-1 text-[9px] text-muted-foreground"><ShieldCheck size={11}/>اجرا نمی‌شود · release {item.release_id.slice(0, 8)}</p></div>)}
          {!plans.length && <p className="rounded-lg border border-dashed border-border p-3 text-center text-[10px] text-muted-foreground">برای این اپ Build Plan ذخیره نشده است.</p>}
        </section>
        <section className="space-y-2 rounded-xl border border-border/70 p-3">
          <div className="flex flex-wrap items-center justify-between gap-2"><div><h4 className="text-xs font-semibold">مرور سورس release</h4><p className="mt-1 text-[10px] text-muted-foreground">فقط‌خواندنی؛ فایل‌های مخفی و مسیرهای بیرون از release مسدودند.</p></div><Button size="icon" variant="outline" aria-label="بارگذاری دوبارهٔ سورس" title="بارگذاری دوباره" onClick={() => void loadTree(path)} disabled={Boolean(busy)}><RefreshCw size={13}/></Button></div>
          <div className="flex min-w-0 items-center gap-1 text-[10px]"><button className="text-primary" onClick={() => void loadTree('')}>release</button>{path.split('/').filter(Boolean).map((segment, index, items) => {const target=items.slice(0,index+1).join('/');return <span key={target} className="flex items-center gap-1"><ChevronLeft size={11}/><button onClick={()=>void loadTree(target)}>{segment}</button></span>})}</div>
          {busy === 'tree' ? <LoaderCircle className="mx-auto animate-spin text-primary" size={18}/> : <div className="max-h-40 space-y-1 overflow-auto">{path && <button className="flex w-full items-center gap-2 rounded px-2 py-1 text-right text-[10px] hover:bg-muted/40" onClick={()=>void loadTree(path.split('/').slice(0,-1).join('/'))}><FolderOpen size={13}/>بازگشت به پوشهٔ بالاتر</button>}{entries.map((entry)=><button key={entry.path} className="flex w-full min-w-0 items-center gap-2 rounded px-2 py-1.5 text-right text-[10px] hover:bg-muted/40" onClick={()=>void openFile(entry)}>{entry.kind==='directory'?<Folder size={13} className="shrink-0 text-primary"/>:<FileCode2 size={13} className="shrink-0 text-muted-foreground"/>}<span className="min-w-0 flex-1 truncate">{entry.name}</span>{entry.kind==='file'&&<span className="shrink-0 text-muted-foreground">{entry.size_bytes.toLocaleString()} B</span>}</button>)}</div>}
          {file && <div className="overflow-hidden rounded-lg border border-border/70"><div className="flex items-center justify-between gap-2 border-b border-border/60 px-2 py-1.5 text-[10px]"><span dir="ltr" className="truncate font-mono">{file.path}</span><Badge variant="outline">فقط‌خواندنی</Badge></div><pre dir="ltr" className="max-h-64 overflow-auto whitespace-pre-wrap break-all p-3 text-left font-mono text-[10px] leading-5">{file.content}</pre></div>}
        </section>
      </div>
      <section className="space-y-2 rounded-xl border border-border/70 p-3"><div className="flex flex-wrap items-center justify-between gap-2"><div><h4 className="text-xs font-semibold">Snapshotهای تنظیمات</h4><p className="mt-1 text-[10px] text-muted-foreground">metadata محیط و تنظیمات اپ؛ Secret، فایل پروژه یا دیتابیس کپی نمی‌شود.</p></div><Button variant="outline" size="sm" onClick={() => void createSnapshot()} disabled={Boolean(busy)}><Plus size={13}/>{busy === 'snapshot' ? 'در حال ذخیره' : 'ثبت Snapshot'}</Button></div><div className="grid gap-2 md:grid-cols-2">{snapshots.slice(0,4).map((snapshot)=><div key={snapshot.id} className="min-w-0 rounded-lg bg-muted/30 p-2 text-[10px]"><p className="font-medium">{snapshot.label || 'Snapshot بدون عنوان'}</p><p className="mt-1 text-muted-foreground">{new Date(snapshot.created_at*1000).toLocaleString('fa-IR')}</p><p dir="ltr" className="mt-1 truncate text-left font-mono text-muted-foreground">SHA-256 {snapshot.sha256}</p><p className="mt-1 text-amber-100">فقط ثبت و مقایسه؛ بازیابی خودکار فعال نیست.</p></div>)}</div>{!snapshots.length && <p className="text-[10px] text-muted-foreground">هنوز Snapshotی ثبت نشده است.</p>}</section>
    </CardContent>}
  </Card>
}
