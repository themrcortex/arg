import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Archive, CircleAlert, CircleHelp, CirclePlus, Code2, Database, Globe2, Layers3, Pencil, RefreshCw, RotateCcw, Search, Server } from 'lucide-react'
import { api, apiForWorkspace } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'
import { Skeleton } from './components/ui/skeleton'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type AppRecord = {
  id: string
  owner_server_uuid: string
  name: string
  description: string
  framework: string
  source_type: string
  domain: string | null
  listen_port: number | null
  labels: string[]
  desired_state: 'none' | 'running' | 'stopped'
  observed_state: 'unknown' | 'running' | 'stopped' | 'failed' | 'offline'
  observed_at: number | null
  lifecycle_state: string
  reconcile_policy: string
  version: number
  created_by: string
  created_at: number
  updated_at: number
  deleted_at: number | null
}
type Registry = { server_uuid: string; apps: AppRecord[]; features: { deploy: boolean; environment_secrets: boolean; storage_plans: boolean; automatic_reconciliation: boolean } }
type NodeRef = { id: number; workspace_key: string; name: string; public_name?: string; endpoint: string; workspace_ready?: boolean }
type RegistryRow = { app: AppRecord; serverUuid: string; serverName: string; workspaceKey: string; local: boolean }
type ServerError = { key: string; name: string; workspaceKey: string; message: string }
type Draft = { name: string; description: string; framework: string; source_type: string; domain: string; listen_port: string; labels: string; desired_state: string }

const emptyDraft: Draft = { name: '', description: '', framework: 'custom', source_type: 'manual', domain: '', listen_port: '', labels: '', desired_state: 'none' }
const frameworkLabels: Record<string, string> = { laravel: 'Laravel', node: 'Node.js', python: 'Python', wordpress: 'WordPress', static: 'سایت استاتیک', custom: 'سفارشی / تشخیص‌داده‌نشده' }
const sourceLabels: Record<string, string> = { manual: 'ثبت دستی', git: 'مخزن Git', archive: 'فایل آرشیو', docker: 'Docker', 'existing-directory': 'پوشهٔ موجود', image: 'Image' }

function asDraft(app?: AppRecord): Draft {
  return app ? { name: app.name, description: app.description, framework: app.framework, source_type: app.source_type, domain: app.domain || '', listen_port: app.listen_port ? String(app.listen_port) : '', labels: app.labels.join(', '), desired_state: app.desired_state } : { ...emptyDraft }
}
function draftPayload(draft: Draft) {
  return { name: draft.name.trim(), description: draft.description.trim(), framework: draft.framework, source_type: draft.source_type, domain: draft.domain.trim(), listen_port: draft.listen_port.trim() ? Number(draft.listen_port) : null, labels: draft.labels.split(',').map((label) => label.trim()).filter(Boolean), desired_state: draft.desired_state }
}

export function ApplicationsPage({ Heading: PageHeading, notify, allServers, workspaceKey, onOpenWorkspace }: { Heading: Heading; notify: (message: string) => void; allServers: boolean; workspaceKey: string; onOpenWorkspace: (key: string) => void }) {
  const [rows, setRows] = useState<RegistryRow[]>([])
  const [serverErrors, setServerErrors] = useState<ServerError[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [query, setQuery] = useState('')
  const [frameworkFilter, setFrameworkFilter] = useState('all')
  const [showArchived, setShowArchived] = useState(false)
  const [dialog, setDialog] = useState<'form' | 'delete' | 'restore' | null>(null)
  const [target, setTarget] = useState<AppRecord | null>(null)
  const [draft, setDraft] = useState<Draft>({ ...emptyDraft })
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const load = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true)
    setRefreshing(true)
    setError('')
    try {
      if (!allServers) {
        const registry = await api<Registry>(`apps${showArchived ? '?include_deleted=true' : ''}`)
        setRows(registry.apps.map((app) => ({ app, serverUuid: registry.server_uuid, serverName: 'این VPS', workspaceKey, local: !workspaceKey })))
        setServerErrors([])
        return
      }
      const [registry, local, nodeRegistry] = await Promise.all([
        api<Registry>('apps'),
        api<{ hostname: string }>('dashboard'),
        api<{ public_name?: string; nodes: NodeRef[] }>('nodes'),
      ])
      const result: RegistryRow[] = registry.apps.map((app) => ({ app, serverUuid: registry.server_uuid, serverName: nodeRegistry.public_name || local.hostname || 'این VPS', workspaceKey: '', local: true }))
      const failures: ServerError[] = []
      const nodes = nodeRegistry.nodes || []
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(4, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          const serverName = node.public_name || node.name
          if (!node.workspace_ready) { failures[index] = { key: node.workspace_key, name: serverName, workspaceKey: node.workspace_key, message: 'برای خواندن فهرست اپلیکیشن‌ها، اتصال فضای کار این Node را دوباره جفت کن.' }; continue }
          try {
            const remote = await apiForWorkspace<Registry>(node.workspace_key, `apps${showArchived ? '?include_deleted=true' : ''}`, { signal: AbortSignal.timeout(8000) })
            for (const app of remote.apps) result.push({ app, serverUuid: remote.server_uuid, serverName, workspaceKey: node.workspace_key, local: false })
          } catch (cause) {
            failures[index] = { key: node.workspace_key, name: serverName, workspaceKey: node.workspace_key, message: cause instanceof Error ? cause.message : 'این VPS پاسخ نداد.' }
          }
        }
      }))
      setRows(result)
      setServerErrors(failures.filter(Boolean))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'فهرست اپلیکیشن‌ها خوانده نشد.')
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [allServers, workspaceKey, showArchived])

  useEffect(() => { void load() }, [load])

  const visibleRows = useMemo(() => rows.filter(({ app }) => {
    const search = `${app.name} ${app.description} ${app.domain || ''} ${app.labels.join(' ')} ${frameworkLabels[app.framework] || app.framework}`.toLocaleLowerCase()
    return (frameworkFilter === 'all' || app.framework === frameworkFilter) && search.includes(query.trim().toLocaleLowerCase())
  }), [rows, query, frameworkFilter])

  function beginCreate() { setTarget(null); setDraft({ ...emptyDraft }); setError(''); setDialog('form') }
  function beginEdit(row: RegistryRow) { setTarget(row.app); setDraft(asDraft(row.app)); setError(''); setDialog('form') }

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    setSaving(true); setError('')
    try {
      if (target) {
        await api(`apps/${encodeURIComponent(target.id)}`, { method: 'PUT', body: JSON.stringify({ ...draftPayload(draft), expected_version: target.version }) })
        notify('مشخصات اپلیکیشن به‌روز شد؛ وضعیت اجرای سرویس تغییر نکرده است.')
      } else {
        await api('apps', { method: 'POST', body: JSON.stringify(draftPayload(draft)) })
        notify('رکورد اپلیکیشن ثبت شد؛ هیچ کد یا سرویسی اجرا نشده است.')
      }
      setDialog(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function archiveApp() {
    if (!target) return
    setSaving(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(target.id)}`, { method: 'DELETE', body: JSON.stringify({ expected_version: target.version }) })
      notify('رکورد اپلیکیشن آرشیو شد؛ فایل‌ها و سرویس‌ها دست‌نخورده ماندند.')
      setDialog(null); setTarget(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'آرشیو اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  async function restoreApp() {
    if (!target) return
    setSaving(true); setError('')
    try {
      await api(`apps/${encodeURIComponent(target.id)}/restore`, { method: 'POST', body: JSON.stringify({ expected_version: target.version }) })
      notify('رکورد اپلیکیشن بازیابی شد.')
      setDialog(null); setTarget(null); await load(true)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'بازیابی اپلیکیشن ناموفق بود.') }
    finally { setSaving(false) }
  }

  const action = <div className="flex flex-wrap gap-2">{!allServers && <Button onClick={beginCreate}><CirclePlus size={15}/>ثبت اپلیکیشن</Button>}<Button variant="outline" onClick={() => void load(true)} disabled={refreshing}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button></div>

  return <>
    <PageHeading title="اپلیکیشن‌ها" description={allServers ? 'فهرست خواندنی اپلیکیشن‌های ثبت‌شده روی VPSهای متصل؛ عملیات هر رکورد در فضای کار میزبان انجام می‌شود.' : 'فهرست مستقل اپلیکیشن‌ها و وضعیت مالکیت آن‌ها روی همین VPS.'} action={action}/>
    <Card className="mb-5 border-[#dda46f]/15 bg-card/55"><CardContent className="flex items-start gap-3 p-4"><CircleHelp className="mt-0.5 shrink-0 text-primary" size={18}/><p className="text-xs leading-6 text-muted-foreground">این مرحله دفتر ثبت و مالکیت اپلیکیشن را اضافه می‌کند. ثبت رکورد به‌تنهایی سورس را دریافت نمی‌کند، Build یا Deploy انجام نمی‌دهد و سرویس را روشن یا خاموش نمی‌کند؛ وضعیت واقعی اجرا فقط وقتی یک منبع مشاهدهٔ معتبر وصل شود نمایش داده خواهد شد.</p></CardContent></Card>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
    {allServers && serverErrors.length > 0 && <Card className="mb-4 border-amber-400/20 bg-amber-400/[.04]"><CardContent className="space-y-2 p-4"><p className="flex items-center gap-2 text-sm font-medium text-amber-100"><CircleAlert size={15}/>بعضی VPSها قابل خواندن نبودند</p>{serverErrors.map((item) => <div key={item.key} className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"><span>{item.name}: {item.message}</span><Button size="sm" variant="outline" onClick={() => onOpenWorkspace(item.workspaceKey)}>باز کردن VPS</Button></div>)}</CardContent></Card>}
    <div className="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-border/70 bg-card/35 p-3">
      <div className="relative min-w-[190px] flex-1"><Search size={15} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="جست‌وجوی نام، دامنه یا برچسب" className="pr-9"/></div>
      <select value={frameworkFilter} onChange={(event) => setFrameworkFilter(event.target.value)} aria-label="فیلتر چارچوب" className="h-10 rounded-md border border-input bg-background px-3 text-xs"><option value="all">همهٔ چارچوب‌ها</option>{Object.entries(frameworkLabels).map(([key, label]) => <option value={key} key={key}>{label}</option>)}</select>
      <label className="flex h-10 items-center gap-2 rounded-md border border-input px-3 text-xs text-muted-foreground"><input type="checkbox" checked={showArchived} onChange={(event) => setShowArchived(event.target.checked)} className="accent-[#dda46f]"/>نمایش آرشیوی‌ها</label>
    </div>
    {loading ? <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{[0,1,2].map((item) => <Skeleton key={item} className="h-56 rounded-2xl"/>)}</div> : visibleRows.length === 0 ? <Card className="border-dashed"><CardContent className="flex min-h-48 flex-col items-center justify-center gap-3 p-6 text-center"><Layers3 size={26} className="text-primary"/><p className="font-medium">{showArchived ? 'رکوردی با این فیلتر پیدا نشد' : 'هنوز اپلیکیشنی ثبت نشده است'}</p><p className="max-w-lg text-xs leading-6 text-muted-foreground">اپلیکیشن یک موجودیت مدیریتی جدا از فایل Nginx یا کانتینر Docker است. می‌توانی ابتدا مشخصاتش را ثبت کنی و مراحل اتصال سورس و استقرار در فازهای بعد اضافه می‌شوند.</p>{!allServers && <Button onClick={beginCreate}><CirclePlus size={15}/>ثبت اولین اپلیکیشن</Button>}</CardContent></Card> : <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{visibleRows.map(({ app, serverName, workspaceKey: ownerWorkspace, local }) => <Card key={`${app.id}:${app.owner_server_uuid || ''}`} className={`overflow-hidden ${app.deleted_at ? 'opacity-75' : ''}`}><CardHeader className="pb-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><CardTitle className="flex items-center gap-2 text-base"><Code2 size={17} className="shrink-0 text-primary"/><span className="truncate">{app.name}</span></CardTitle><CardDescription className="mt-2 flex items-center gap-1.5 truncate"><Server size={12}/>{serverName}</CardDescription></div><Badge variant={app.deleted_at ? 'outline' : 'success'}>{app.deleted_at ? 'آرشیو' : 'ثبت‌شده'}</Badge></div></CardHeader><CardContent className="space-y-3"><div className="grid grid-cols-2 gap-2 text-xs"><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">قالب</p><p className="mt-1 truncate">{frameworkLabels[app.framework] || app.framework}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">منبع انتخاب‌شده</p><p className="mt-1 truncate">{sourceLabels[app.source_type] || app.source_type}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت هدف</p><p className="mt-1">{app.desired_state === 'none' ? 'تنظیم نشده' : app.desired_state === 'running' ? 'درخواست اجرا' : 'درخواست توقف'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">وضعیت واقعی</p><p className="mt-1">{app.observed_state === 'unknown' ? 'هنوز مشاهده نشده' : app.observed_state}</p></div></div>{app.domain && <p dir="ltr" className="flex items-center justify-end gap-1.5 truncate text-left font-mono text-xs text-muted-foreground"><Globe2 size={13}/>{app.domain}</p>}{app.listen_port && <p dir="ltr" className="text-left font-mono text-[11px] text-muted-foreground">TCP :{app.listen_port}</p>}{app.description && <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">{app.description}</p>}{app.labels.length > 0 && <div className="flex flex-wrap gap-1">{app.labels.map((label) => <Badge key={label} variant="outline" className="text-[10px]">{label}</Badge>)}</div>}<div className="flex items-center justify-between gap-2 border-t border-border/60 pt-3">{allServers && !local ? <Button size="sm" variant="outline" className="w-full" onClick={() => onOpenWorkspace(ownerWorkspace)}>رفتن به فضای کار میزبان</Button> : <span className="flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground"><Database size={12}/>منابع زنده هنوز متصل نیستند</span>}{!allServers && <div className="flex shrink-0 gap-1">{app.deleted_at ? <Button size="icon" variant="ghost" title="بازیابی رکورد" aria-label="بازیابی رکورد" onClick={() => { setTarget(app); setError(''); setDialog('restore') }}><RotateCcw size={15}/></Button> : <><Button size="icon" variant="ghost" title="ویرایش مشخصات" aria-label="ویرایش مشخصات" onClick={() => beginEdit({ app, serverUuid: '', serverName, workspaceKey: '', local: true })}><Pencil size={15}/></Button><Button size="icon" variant="ghost" className="text-rose-300" title="آرشیو رکورد" aria-label="آرشیو رکورد" onClick={() => { setTarget(app); setError(''); setDialog('delete') }}><Archive size={15}/></Button></>}</div>}</div></CardContent></Card>)}</div>}
    <Dialog open={dialog === 'form'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-h-[92dvh] max-w-2xl overflow-y-auto"><DialogHeader><DialogTitle>{target ? 'ویرایش رکورد اپلیکیشن' : 'ثبت اپلیکیشن'}</DialogTitle><DialogDescription>این فرم فقط کاتالوگ و مالکیت اپ را ثبت می‌کند. فیلدهای استقرار، محیط و فضای ذخیره‌سازی هنوز فعال نیستند.</DialogDescription></DialogHeader><form onSubmit={(event) => void submit(event)} className="space-y-4"><div className="grid gap-4 sm:grid-cols-2"><div className="space-y-2"><Label htmlFor="app-name">نام</Label><Input id="app-name" required maxLength={80} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-framework">قالب پیشنهادی</Label><select id="app-framework" value={draft.framework} onChange={(event) => setDraft({ ...draft, framework: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(frameworkLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div><div className="space-y-2"><Label htmlFor="app-source">منبع برنامه برای استقرار آینده</Label><select id="app-source" value={draft.source_type} onChange={(event) => setDraft({ ...draft, source_type: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm">{Object.entries(sourceLabels).map(([key,label]) => <option value={key} key={key}>{label}</option>)}</select></div><div className="space-y-2"><Label htmlFor="app-domain">دامنهٔ رزروشده</Label><Input id="app-domain" dir="ltr" placeholder="app.example.com" value={draft.domain} onChange={(event) => setDraft({ ...draft, domain: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-port">پورت TCP موردنظر، اختیاری</Label><Input id="app-port" type="number" min={1} max={65535} dir="ltr" placeholder="3000" value={draft.listen_port} onChange={(event) => setDraft({ ...draft, listen_port: event.target.value })}/></div><div className="space-y-2"><Label htmlFor="app-desired">وضعیت هدف (صرفاً یادداشت)</Label><select id="app-desired" value={draft.desired_state} onChange={(event) => setDraft({ ...draft, desired_state: event.target.value })} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm"><option value="none">تعیین‌نشده</option><option value="running">باید اجرا شود</option><option value="stopped">باید متوقف باشد</option></select></div></div><div className="space-y-2"><Label htmlFor="app-labels">برچسب‌ها</Label><Input id="app-labels" value={draft.labels} onChange={(event) => setDraft({ ...draft, labels: event.target.value })} placeholder="مشتری الف، production، وب"/><p className="text-[11px] text-muted-foreground">برچسب‌ها را با ویرگول جدا کن.</p></div><div className="space-y-2"><Label htmlFor="app-description">توضیحات</Label><textarea id="app-description" rows={3} maxLength={1000} value={draft.description} onChange={(event) => setDraft({ ...draft, description: event.target.value })} className="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-[#dda46f]"/></div>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button type="button" variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button type="submit" disabled={saving}>{saving ? 'در حال ذخیره…' : target ? 'ذخیرهٔ تغییرات' : 'ثبت رکورد'}</Button></div></form></DialogContent></Dialog>
    <Dialog open={dialog === 'delete' || dialog === 'restore'} onOpenChange={(open) => { if (!saving && !open) { setDialog(null); setError('') } }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>{dialog === 'delete' ? 'آرشیو اپلیکیشن' : 'بازیابی اپلیکیشن'}</DialogTitle><DialogDescription>{dialog === 'delete' ? `«${target?.name}» از فهرست فعال کنار می‌رود. فایل‌ها، کانتینرها، Nginx و سرویس‌های سیستم تغییر نمی‌کنند.` : `رکورد «${target?.name}» به فهرست فعال برمی‌گردد. اگر نام یا دامنه از آن زمان به مورد دیگری اختصاص یافته باشد، بازیابی انجام نمی‌شود.`}</DialogDescription></DialogHeader>{error && <p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}<div className="flex flex-wrap justify-end gap-2 pt-2"><Button variant="outline" onClick={() => setDialog(null)} disabled={saving}>انصراف</Button><Button variant={dialog === 'delete' ? 'destructive' : 'default'} disabled={saving} onClick={() => void (dialog === 'delete' ? archiveApp() : restoreApp())}>{saving ? 'در حال انجام…' : dialog === 'delete' ? 'آرشیو رکورد' : 'بازیابی رکورد'}</Button></div></DialogContent></Dialog>
  </>
}
