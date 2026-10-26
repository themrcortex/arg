import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Activity, Check, Copy, Link2, LoaderCircle, Plus, RefreshCw, Save, Server, Shield, Trash2, Unplug } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Props = { Heading: Heading; notify: (message: string) => void; workspaceMode: 'single' | 'multi'; onWorkspaceModeChange: (mode: 'single' | 'multi') => void }
type Node = { id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; endpoint: string; node_id: string; protocol_version: number; last_seen: number | null; created_at: number; workspace_ready?: boolean }
type Controller = { id: number; name: string; public_name: string; server_uuid?: string | null; created_at: number; last_seen: number | null }
type NodeStatus = { protocol: string; node_id: string; server_uuid?: string; public_name?: string; hostname: string; os: string; architecture: string; arg_version: string; uptime_seconds: number; load: string; memory_total_kib: number; memory_available_kib: number; capabilities: string[] }
type Service = { unit: string; enabled: string; state: string; sub_state: string; description: string }
type PendingAction = { kind: 'remove-node'; id: number; name: string } | { kind: 'revoke-controller'; id: number; name: string } | { kind: 'service'; id: number; unit: string; action: string }
const bytes = (kib: number) => `${(kib / 1024 / 1024).toLocaleString('en-US', { maximumFractionDigits: 1 })} GiB`
const uptime = (seconds: number) => { const days = Math.floor(seconds / 86400); const hours = Math.floor(seconds % 86400 / 3600); return `${days}d ${hours}h` }

export function NodesPage({ Heading, notify, workspaceMode, onWorkspaceModeChange }: Props) {
  const [nodes, setNodes] = useState<Node[]>([])
  const [controllers, setControllers] = useState<Controller[]>([])
  const [publicName, setPublicName] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [selected, setSelected] = useState<number | null>(null)
  const [nodeStatus, setNodeStatus] = useState<NodeStatus | null>(null)
  const [services, setServices] = useState<Service[]>([])
  const [pairName, setPairName] = useState('')
  const [endpoint, setEndpoint] = useState('')
  const [pairingCode, setPairingCode] = useState('')
  const [localCode, setLocalCode] = useState<{ code: string; expires_at: number } | null>(null)
  const [pending, setPending] = useState<PendingAction | null>(null)
  const [controllerNames, setControllerNames] = useState<Record<number, string>>({})

  const load = useCallback(async () => {
    setLoading(true); setError('')
    try { const result = await api<{ nodes: Node[]; controllers: Controller[]; public_name: string }>('nodes'); setNodes(result.nodes); setControllers(result.controllers); setPublicName(result.public_name || '') }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'فهرست Nodeها خوانده نشد.'; setError(message) }
    finally { setLoading(false) }
  }, [])
  useEffect(() => { void load() }, [load])

  async function generateCode() {
    setBusy('pair-code'); setError('')
    try { const result = await api<{ pairing_code: string; expires_at: number }>('nodes/pairing-code', { method: 'POST', body: JSON.stringify({ replace_existing: true }) }); setLocalCode({ code: result.pairing_code, expires_at: result.expires_at }); notify('کد اتصال یک‌بارمصرف ساخته شد.') }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'ساخت کد اتصال ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  async function addNode(event: React.FormEvent) {
    event.preventDefault(); setBusy('add'); setError('')
    try { const result = await api<{ message: string }>('nodes', { method: 'POST', body: JSON.stringify({ name: pairName.trim(), endpoint: endpoint.trim(), pairing_code: pairingCode.trim() }) }); notify(result.message); setPairName(''); setEndpoint(''); setPairingCode(''); setLocalCode(null); await load(); window.dispatchEvent(new Event('arg-nodes-changed')) }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'جفت‌سازی Node ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  async function selectNode(node: Node) {
    setSelected(node.id); setNodeStatus(null); setServices([]); setBusy('inspect'); setError('')
    try {
      const [status, serviceResult] = await Promise.all([api<NodeStatus>(`nodes/${node.id}/status`), api<{ services: Service[] }>(`nodes/${node.id}/services`)])
      setNodeStatus(status); setServices(serviceResult.services || []); setNodes((rows) => rows.map((row) => row.id === node.id ? { ...row, public_name: status.public_name || row.public_name, server_uuid: status.server_uuid || row.server_uuid, workspace_key: status.server_uuid || row.workspace_key } : row)); if (status.server_uuid) window.dispatchEvent(new Event('arg-nodes-changed'))
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'ارتباط با Node برقرار نشد.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  async function runConfirmedAction() {
    if (!pending) return
    setBusy('action'); setError('')
    try {
      if (pending.kind === 'remove-node') {
        await api(`nodes/${pending.id}`, { method: 'DELETE' }); setSelected(null); setNodeStatus(null); setServices([]); notify('اتصال Node از هر دو پنل لغو شد.')
      } else if (pending.kind === 'revoke-controller') {
        await api(`nodes/controllers/${pending.id}`, { method: 'DELETE' }); notify('دسترسی پنل کنترل‌کننده لغو شد.')
      } else {
        await api(`nodes/${pending.id}/services/action`, { method: 'POST', body: JSON.stringify({ action: pending.action, unit: pending.unit, confirmed: true }) }); notify(`${pending.action} برای ${pending.unit} اجرا شد.`)
        const node = nodes.find((item) => item.id === pending.id); if (node) await selectNode(node)
      }
      await load()
      window.dispatchEvent(new Event('arg-nodes-changed'))
    } catch (cause) { const message = cause instanceof Error ? cause.message : 'عملیات Node ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy(''); setPending(null) }
  }
  async function runServiceAction(nodeId: number, service: Service, action: string) {
    if (['stop', 'restart', 'disable', 'disable-now'].includes(action)) { setPending({ kind: 'service', id: nodeId, unit: service.unit, action }); return }
    setBusy(`service:${service.unit}`); setError('')
    try { await api(`nodes/${nodeId}/services/action`, { method: 'POST', body: JSON.stringify({ action, unit: service.unit }) }); notify(`${action} برای ${service.unit} اجرا شد.`); const node = nodes.find((item) => item.id === nodeId); if (node) await selectNode(node) }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'عملیات سرویس ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  async function saveControllerName(controller: Controller) {
    setBusy(`rename:${controller.id}`)
    try { const result = await api<{ name: string; message: string }>(`nodes/controllers/${controller.id}`, { method: 'PUT', body: JSON.stringify({ name: controllerNames[controller.id] ?? controller.name }) }); setControllers((rows) => rows.map((row) => row.id === controller.id ? { ...row, name: result.name } : row)); setControllerNames((rows) => ({ ...rows, [controller.id]: result.name })); notify(result.message) }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'ذخیرهٔ برچسب محلی ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  async function savePublicName() {
    const name = publicName.trim()
    if (!name) return
    setBusy('public-name'); setError('')
    try { const result = await api<{ name: string; message: string }>('nodes/public-name', { method: 'PUT', body: JSON.stringify({ name }) }); setPublicName(result.name); notify(result.message); await load() }
    catch (cause) { const message = cause instanceof Error ? cause.message : 'ذخیره و همگام‌سازی نام عمومی ناموفق بود.'; setError(message); notify(message) }
    finally { setBusy('') }
  }
  const copyCode = async () => { if (!localCode) return; await navigator.clipboard.writeText(localCode.code); notify('کد اتصال کپی شد.') }
  const headerAction = <Button size="sm" variant="outline" onClick={() => void load()} disabled={loading || !!busy}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>تازه‌سازی</Button>

  return <>
    <Heading title="Nodeهای ARG" description="پنل‌های ARG را با پروتکل امن اختصاصی جفت کن و وضعیت سرورها و سرویس‌هایشان را از اینجا مدیریت کن." action={headerAction}/>
    <Card className="mt-4"><CardHeader><CardTitle className="text-base">حالت فضای‌کار</CardTitle><CardDescription>این انتخاب فقط شکل مدیریت را عوض می‌کند و هیچ Node یا داده‌ای را حذف نمی‌کند.</CardDescription></CardHeader><CardContent className="flex flex-wrap gap-2"><Button variant={workspaceMode === 'single' ? 'default' : 'outline'} onClick={() => onWorkspaceModeChange('single')}><Server size={14}/>تک‌سرور</Button><Button variant={workspaceMode === 'multi' ? 'default' : 'outline'} onClick={() => onWorkspaceModeChange('multi')}><Activity size={14}/>چندسرور</Button><span className="basis-full text-xs leading-5 text-muted-foreground">در حالت چندسرور، از نوار بالا سرور محلی، یک Node یا نمای همهٔ سرورها را انتخاب کن.</span></CardContent></Card>
    {error && <p role="alert" className="mt-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p>}
    <Card className="mt-4"><CardHeader><CardTitle className="text-base">نام عمومی این سرور</CardTitle><CardDescription>این نام به پنل‌های جفت‌شده ارسال می‌شود؛ تغییرش hostname سیستم را عوض نمی‌کند. برچسب محلی هر پنل جداگانه تنظیم می‌شود.</CardDescription></CardHeader><CardContent className="flex flex-wrap gap-2"><Input aria-label="نام عمومی این سرور" value={publicName} maxLength={80} onChange={(event) => setPublicName(event.target.value)} placeholder="مثلاً IR-PARSPACK-MAJID"/><Button onClick={() => void savePublicName()} disabled={!!busy || !publicName.trim()}>{busy === 'public-name' ? <LoaderCircle size={14} className="animate-spin"/> : <Save size={14}/>}ذخیره و همگام‌سازی</Button></CardContent></Card>
    <div className="mt-4 grid gap-4 xl:grid-cols-2">
      <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Link2 size={17} className="text-primary"/>اتصال این پنل به پنل دیگر</CardTitle><CardDescription>این پنل، سرور مقصد را به فهرست Nodeهای خودش اضافه می‌کند.</CardDescription></CardHeader><CardContent className="space-y-4">
        <ol className="list-inside list-decimal space-y-1 rounded-lg border border-border/70 bg-background/30 p-3 text-xs leading-5 text-muted-foreground"><li>در پنل ARG سرور مقصد، بخش «Nodeها» را باز کن و در کارت «اجازهٔ مدیریت این سرور» کد اتصال بساز.</li><li>کد 5 دقیقه اعتبار دارد؛ آن را کپی کن و به همین پنل برگرد.</li><li>آدرس HTTPS پنل مقصد و کد را در فرم زیر وارد و جفت‌سازی را اجرا کن.</li></ol>
        <form className="space-y-3" onSubmit={(event) => void addNode(event)}><div className="space-y-1.5"><Label htmlFor="node-name">برچسب محلی برای سرور مقصد</Label><Input id="node-name" value={pairName} onChange={(event) => setPairName(event.target.value)} placeholder="مثلاً VPS اروپا" required maxLength={80}/><p className="text-[10px] text-muted-foreground">این نام فقط در همین پنل دیده می‌شود؛ نام عمومی سرور مقصد جداگانه دریافت می‌شود.</p></div><div className="space-y-1.5"><Label htmlFor="node-endpoint">آدرس پنل مقصد</Label><Input id="node-endpoint" dir="ltr" className="text-left font-mono" value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://node.example.com" required type="url"/><p className="text-[10px] text-muted-foreground">HTTPS اجباری است؛ گواهی معتبر و دسترسی شبکهٔ سرورها لازم است.</p></div><div className="space-y-1.5"><Label htmlFor="node-pairing-code">کد یک‌بارمصرف پنل مقصد</Label><Input id="node-pairing-code" dir="ltr" className="text-left font-mono" value={pairingCode} onChange={(event) => setPairingCode(event.target.value)} placeholder="کد را از صفحهٔ Nodes سرور مقصد کپی کن" required autoComplete="off"/></div><Button className="w-full" disabled={!!busy || !pairName.trim() || !endpoint || !pairingCode}>{busy === 'add' ? <LoaderCircle size={15} className="animate-spin"/> : <Plus size={15}/>}جفت‌سازی امن Node</Button></form>
      </CardContent></Card>
      <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Shield size={17} className="text-primary"/>اجازهٔ مدیریت این سرور</CardTitle><CardDescription>این سرور مقصد اتصال است؛ کد را از اینجا می‌گیری و در پنل کنترل‌کننده وارد می‌کنی.</CardDescription></CardHeader><CardContent className="space-y-3"><ol className="list-inside list-decimal space-y-1 text-xs leading-5 text-muted-foreground"><li>کد بساز و فقط به مدیر پنلی بده که به آن اعتماد داری.</li><li>در پنل دیگر، بخش «Nodeها» و فرم «اتصال این پنل به پنل دیگر» را باز کنند.</li><li>کد 5 دقیقه اعتبار دارد و پس از اتصال موفق فوراً باطل می‌شود.</li></ol><Button onClick={() => void generateCode()} disabled={!!busy}><Plus size={15}/>{busy === 'pair-code' ? 'در حال ساخت…' : 'ساخت کد اتصال'}</Button>{localCode && <div className="space-y-2 rounded-xl border border-amber-400/20 bg-amber-400/[.04] p-3"><p className="text-xs text-amber-100">این کد فقط یک بار قابل‌استفاده است و تا 5 دقیقه اعتبار دارد.</p><div className="flex gap-2"><Input readOnly value={localCode.code} dir="ltr" className="font-mono text-left"/><Button variant="outline" size="icon" onClick={() => void copyCode()} aria-label="کپی کد"><Copy size={14}/></Button></div><p className="text-[10px] text-muted-foreground">با استفادهٔ موفق، کد فوراً باطل می‌شود.</p></div>}<div className="rounded-lg border border-border/70 p-3 text-[10px] leading-5 text-muted-foreground">پس از جفت‌سازی، این پنل می‌تواند فضای کار کامل Node را باز کند؛ درخواست‌های پنل و ترمینال از تونل HTTPS عبور می‌کنند و کلید دسترسی فقط روی دیسک سرور کنترل‌کننده نگهداری می‌شود. دسترسی را فقط به مدیر مورداعتماد بده.</div></CardContent></Card>
    </div>

    <div className="mt-5 grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)]">
      <Card><CardHeader><CardTitle className="text-base">Nodeهای متصل</CardTitle><CardDescription>کلیدهای دسترسی در فایل خصوصی همین سرور نگهداری می‌شوند.</CardDescription></CardHeader><CardContent className="space-y-2">{loading ? Array.from({ length: 2 }, (_, index) => <Skeleton key={index} className="h-20 rounded-xl"/>) : nodes.length ? nodes.map((node) => <div key={node.id} className={`rounded-xl border p-3 transition ${selected === node.id ? 'border-primary/40 bg-primary/[.04]' : 'border-border/80 bg-background/30'}`}><div className="flex items-start justify-between gap-2"><button type="button" className="min-w-0 flex-1 text-right" onClick={() => void selectNode(node)}><span className="flex items-center gap-2 font-medium"><Server size={15} className="text-primary"/>{node.public_name || node.name}{node.public_name && node.name !== node.public_name ? <span className="text-xs text-muted-foreground">· {node.name}</span> : null}</span><span dir="ltr" className="mt-1 block truncate text-left font-mono text-[10px] text-muted-foreground">{node.endpoint}</span><span className="mt-1 block text-[10px] text-muted-foreground">ARG Node v{node.protocol_version} · {node.last_seen ? `آخرین ارتباط ${new Date(node.last_seen * 1000).toLocaleString('en-GB')}` : 'هنوز بررسی نشده'}</span>{!node.workspace_ready && <span className="mt-2 block rounded-md border border-amber-400/20 bg-amber-400/[.04] p-2 text-[10px] leading-5 text-amber-100">برای سوییچ کامل فضای کار، اتصال قدیمی را حذف و Node را دوباره جفت‌سازی کن.</span>}</button><div className="flex shrink-0 gap-1"><Button size="icon" variant="ghost" aria-label="بررسی Node" title="بررسی Node" onClick={() => void selectNode(node)} disabled={!!busy}>{busy === 'inspect' && selected === node.id ? <LoaderCircle size={14} className="animate-spin"/> : <RefreshCw size={14}/>}</Button><Button size="icon" variant="ghost" className="text-destructive" aria-label="لغو اتصال" title="لغو اتصال" onClick={() => setPending({ kind: 'remove-node', id: node.id, name: node.name })} disabled={!!busy}><Unplug size={14}/></Button></div></div></div>) : <div className="rounded-xl border border-dashed border-border p-6 text-center text-xs text-muted-foreground">هنوز Node دیگری به این پنل متصل نشده است.</div>}</CardContent></Card>
      <div className="space-y-4">
        {nodeStatus && <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Activity size={17} className="text-primary"/>{nodeStatus.hostname || 'وضعیت Node'}</CardTitle><CardDescription>{nodeStatus.os} · {nodeStatus.architecture} · ARG v{nodeStatus.arg_version}</CardDescription></CardHeader><CardContent className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">{[['Uptime', uptime(nodeStatus.uptime_seconds)], ['Load average', nodeStatus.load], ['RAM کل', bytes(nodeStatus.memory_total_kib)], ['RAM آزاد', bytes(nodeStatus.memory_available_kib)]].map(([label, value]) => <div key={label} className="rounded-lg border border-border/70 bg-background/30 p-3"><div className="text-[10px] text-muted-foreground">{label}</div><div dir="ltr" className="mt-1 truncate text-left font-mono text-xs">{value}</div></div>)}</CardContent></Card>}
        {selected !== null && <Card><CardHeader><CardTitle className="text-base">سرویس‌های Node</CardTitle><CardDescription>عملیات متوقف‌سازی سرویس‌های حیاتی ARG، SSH و شبکه از پروتکل مسدود است.</CardDescription></CardHeader><CardContent className="space-y-2">{busy === 'inspect' && !services.length ? <Skeleton className="h-24 rounded-xl"/> : services.length ? services.filter((service) => service.state === 'active' || service.enabled !== 'disabled').slice(0, 80).map((service) => <div key={service.unit} className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border/70 bg-background/25 p-3"><div className="min-w-0"><div dir="ltr" className="truncate text-left font-mono text-xs">{service.unit}</div><p className="mt-1 truncate text-[10px] text-muted-foreground">{service.description} · {service.state}/{service.sub_state} · {service.enabled}</p></div><div className="flex gap-1"><Button size="sm" variant="outline" onClick={() => void runServiceAction(selected, service, 'start')} disabled={!!busy}><Check size={12}/>Start</Button><Button size="sm" variant="outline" onClick={() => void runServiceAction(selected, service, 'restart')} disabled={!!busy}><RefreshCw size={12}/>Restart</Button><Button size="sm" variant="ghost" onClick={() => void runServiceAction(selected, service, 'status')} disabled={!!busy}>Status</Button></div></div>) : <p className="rounded-lg border border-dashed border-border p-5 text-center text-xs text-muted-foreground">Node را انتخاب کن تا سرویس‌ها بارگیری شوند.</p>}</CardContent></Card>}
      </div>
    </div>
    <Card className="mt-5"><CardHeader><CardTitle className="text-sm">پنل‌هایی که اجازهٔ مدیریت این سرور را دارند</CardTitle><CardDescription>نام عمومی از پنل مقابل همگام می‌شود؛ برچسب محلی فقط در همین پنل تغییر می‌کند. لغو دسترسی، کلید کنترل‌کننده را باطل می‌کند.</CardDescription></CardHeader><CardContent className="space-y-2">{controllers.length ? controllers.map((controller) => <div key={controller.id} className="flex flex-wrap items-end justify-between gap-3 rounded-lg border border-border/70 p-3"><div className="min-w-0 flex-1 space-y-2"><Label htmlFor={`controller-name-${controller.id}`}>برچسب محلی این پنل</Label><div className="flex gap-2"><Input id={`controller-name-${controller.id}`} value={controllerNames[controller.id] ?? controller.name} maxLength={80} onChange={(event) => setControllerNames((rows) => ({ ...rows, [controller.id]: event.target.value }))}/><Button size="icon" variant="outline" aria-label="ذخیره برچسب محلی" title="ذخیره برچسب محلی" disabled={!!busy || (controllerNames[controller.id] ?? controller.name).trim() === controller.name} onClick={() => void saveControllerName(controller)}>{busy === `rename:${controller.id}` ? <LoaderCircle size={14} className="animate-spin"/> : <Save size={14}/>}</Button></div><p className="text-[10px] text-muted-foreground">نام عمومی پنل مقابل: <strong>{controller.public_name || controller.name}</strong> · آخرین ارتباط: {controller.last_seen ? new Date(controller.last_seen * 1000).toLocaleString('en-GB') : 'هنوز ارتباطی ثبت نشده'}</p></div><Button size="sm" variant="outline" className="text-destructive" onClick={() => setPending({ kind: 'revoke-controller', id: controller.id, name: controller.public_name || controller.name })}><Trash2 size={13}/>لغو دسترسی</Button></div>) : <p className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground">هیچ پنل دیگری به این سرور دسترسی ندارد.</p>}</CardContent></Card>

    <Dialog open={!!pending} onOpenChange={(open) => { if (!open && !busy) setPending(null) }}><DialogContent><DialogHeader><DialogTitle>تأیید عملیات Node</DialogTitle><DialogDescription>{pending?.kind === 'remove-node' ? `اتصال «${pending.name}» از این پنل حذف و دسترسی متناظر در Node مقصد باطل می‌شود.` : pending?.kind === 'revoke-controller' ? `دسترسی پنل «${pending.name}» به این سرور فوراً لغو می‌شود.` : pending ? `عملیات ${pending.action} روی سرویس ${pending.unit} در سرور راه‌دور اجرا می‌شود.` : ''}</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setPending(null)} disabled={!!busy}>انصراف</Button><Button variant="destructive" onClick={() => void runConfirmedAction()} disabled={!!busy}>{busy === 'action' ? <LoaderCircle size={14} className="animate-spin"/> : <Trash2 size={14}/>}تأیید و اجرا</Button></div></DialogContent></Dialog>
  </>
}
