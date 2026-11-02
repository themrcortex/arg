import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Activity, CircleAlert, Database, Network, Plus, RefreshCw, Server, Trash2, Unplug, Waypoints } from 'lucide-react'
import { api } from './lib/api'
import { buildApplicationRoute } from './lib/application-route.mjs'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Badge } from './components/ui/badge'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type AppNode = { id: string; name: string; framework: string; source_type: string; domain: string | null; listen_port: number | null; desired_state: string; observed_state: string; database: { state: string; connection?: string; host?: string; port?: string; database?: string; username?: string; password_configured?: boolean; health?: string } }
type LiveContainer = { id: string; name: string; ipv4: string; ipv6: string; application_id?: string }
type AppNetwork = { id: string; name: string; driver: 'docker-bridge' | 'host-local'; visibility: string; docker_network: string | null; member_count: number; observed: { state: string; internal?: boolean; containers?: LiveContainer[]; error?: string; note?: string } }
type Member = { network_id: string; application_id: string; application_name: string; framework: string; source_type: string; container_name: string; alias: string; internal_port: number | null; observed_state: string }
type Mesh = { server_uuid: string; applications: AppNode[]; networks: AppNetwork[]; members: Member[]; capabilities: Record<string, boolean>; notes: string[] }
type DockerStatus = { installed: boolean; active: boolean; containers: { id: string; names: string; image: string; status: string; state: string; application_id: string; ports: string }[] }

const driverLabels: Record<string, string> = { 'docker-bridge': 'Docker private bridge', 'host-local': 'Loopback · metadata' }
const memberStates: Record<string, string> = { connected: 'متصل واقعی', not_connected: 'در Docker وصل نیست', unknown: 'وضعیت ناشناخته', metadata_only: 'فقط رکورد پورت' }

export function ApplicationMeshPage({ Heading: PageHeading, notify, allServers }: { Heading: Heading; notify: (message: string) => void; allServers: boolean }) {
  const [mesh, setMesh] = useState<Mesh | null>(null)
  const [docker, setDocker] = useState<DockerStatus | null>(null)
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [name, setName] = useState('')
  const [driver, setDriver] = useState<'docker-bridge' | 'host-local'>('docker-bridge')
  const [networkId, setNetworkId] = useState('')
  const [appId, setAppId] = useState('')
  const [containerName, setContainerName] = useState('')
  const [alias, setAlias] = useState('')
  const [internalPort, setInternalPort] = useState('')

  const load = useCallback(async (quiet = false) => {
    if (allServers) { setLoading(false); return }
    if (quiet) setRefreshing(true)
    else setLoading(true)
    setError('')
    try {
      const [topology, dockerState] = await Promise.all([
        api<Mesh>('app-networks/mesh', { signal: AbortSignal.timeout(15000) }),
        api<DockerStatus>('docker', { signal: AbortSignal.timeout(10000) }).catch(() => null),
      ])
      setMesh(topology)
      setDocker(dockerState)
      setNetworkId((current) => current || topology.networks[0]?.id || '')
      setAppId((current) => current || topology.applications[0]?.id || '')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'نقشهٔ ارتباط اپلیکیشن‌ها بارگذاری نشد.') }
    finally { setLoading(false); setRefreshing(false) }
  }, [allServers])

  useEffect(() => { void load(); return () => {} }, [load])

  const appById = useMemo(() => new Map((mesh?.applications || []).map((app) => [app.id, app])), [mesh])
  const compatibleContainers = useMemo(() => (docker?.containers || []).filter((container) => container.application_id === appId), [docker, appId])
  const selectedNetwork = mesh?.networks.find((network) => network.id === networkId)
  const selectedApp = appById.get(appId)

  async function createNetwork(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      await api('app-networks', { method: 'POST', body: JSON.stringify({ name: name.trim(), driver, visibility: 'private' }) })
      setName(''); await load(true); notify('شبکهٔ اپلیکیشن ایجاد شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت شبکه ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function attach(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      if (!selectedNetwork || !selectedApp) throw new Error('شبکه و اپلیکیشن را انتخاب کن.')
      await api(`app-networks/${encodeURIComponent(selectedNetwork.id)}/members`, { method: 'POST', body: JSON.stringify({ application_id: selectedApp.id, alias: alias.trim(), container_name: selectedNetwork.driver === 'docker-bridge' ? containerName : '', internal_port: selectedNetwork.driver === 'host-local' ? Number(internalPort || selectedApp.listen_port) : null }) })
      setAlias(''); setContainerName(''); setInternalPort(''); await load(true); notify('اپلیکیشن به شبکه متصل شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'اتصال اپلیکیشن ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function detach(member: Member) {
    setBusy(true); setError('')
    try {
      await api(`app-networks/${encodeURIComponent(member.network_id)}/members/${encodeURIComponent(member.application_id)}`, { method: 'DELETE' })
      await load(true); notify('عضویت اپلیکیشن از شبکه حذف شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'جداسازی اپلیکیشن ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function removeNetwork(network: AppNetwork) {
    if (network.member_count > 0) { setError('ابتدا اعضای شبکه را جدا کن.'); return }
    setBusy(true); setError('')
    try {
      await api(`app-networks/${encodeURIComponent(network.id)}?confirm=true`, { method: 'DELETE' })
      await load(true); notify('شبکهٔ خالی حذف شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف شبکه ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <div className="space-y-5">
    <PageHeading title="ARG App Mesh" description="توپولوژی و ارتباط‌های واقعی اپلیکیشن‌های این VPS؛ شبکه‌های Docker با inspect زنده بررسی می‌شوند." action={<Button variant="outline" disabled={refreshing || loading || allServers} onClick={() => void load(true)}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button>}/>
    {allServers ? <Card><CardContent className="p-5 text-sm leading-7 text-muted-foreground">برای مشاهده و تغییر توپولوژی زندهٔ Docker، از انتخاب‌گر بالا یک VPS مشخص را انتخاب کن. نمای همهٔ سرورها فقط برای فهرست‌خوانی است و endpointهای mesh از راه دور تجمیع نمی‌شوند.</CardContent></Card> : <>
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4"><Metric icon={Server} label="اپلیکیشن محلی" value={mesh?.applications.length} /><Metric icon={Network} label="شبکهٔ تعریف‌شده" value={mesh?.networks.length} /><Metric icon={Waypoints} label="عضویت ثبت‌شده" value={mesh?.members.length} /><Metric icon={Activity} label="Docker" value={docker?.active ? 'فعال' : docker?.installed ? 'خاموش' : 'نامشخص'} /></div>
      {error && <p role="alert" className="rounded-xl border border-rose-300/20 bg-rose-300/[.05] p-3 text-xs text-rose-100">{error}</p>}
      {loading ? <Skeleton className="h-64 rounded-2xl"/> : <Tabs defaultValue="topology" dir="rtl" className="space-y-4">
        <TabsList className="h-auto max-w-full flex-nowrap justify-start overflow-x-auto"><TabsTrigger value="topology">توپولوژی زنده</TabsTrigger><TabsTrigger value="networks">شبکه‌ها و اتصال</TabsTrigger><TabsTrigger value="databases">اتصال‌های دیتابیس</TabsTrigger></TabsList>
        <TabsContent value="topology" className="space-y-4">
          <Card><CardHeader><CardTitle className="flex items-center gap-2 text-sm"><Waypoints size={16}/>شبکهٔ اپلیکیشن‌ها</CardTitle><CardDescription>خط فقط وقتی اتصال ثبت‌شده دارد نمایش داده می‌شود. وضعیت Docker از daemon خوانده شده و خط شبکهٔ host-local نشان‌دهندهٔ جداسازی واقعی نیست.</CardDescription></CardHeader><CardContent>
            {!mesh?.applications.length ? <Empty>اپلیکیشنی در این VPS ثبت نشده است.</Empty> : <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{mesh.applications.map((app) => { const memberships = (mesh.members || []).filter((member) => member.application_id === app.id); return <article key={app.id} className="rounded-xl border border-border/70 bg-background/35 p-3"><div className="flex items-start justify-between gap-2"><div className="min-w-0"><p className="truncate text-sm font-semibold">{app.name}</p><p className="mt-1 text-[10px] text-muted-foreground">{app.framework} · {app.source_type}</p></div><Badge variant={app.observed_state === 'running' ? 'success' : 'outline'}>{app.observed_state}</Badge></div><div className="mt-3 flex flex-wrap gap-1.5">{memberships.length ? memberships.map((member) => <span key={member.network_id} className="rounded-md border border-primary/20 bg-primary/[.05] px-2 py-1 text-[10px]">{mesh.networks.find((item) => item.id === member.network_id)?.name || 'شبکه'} · {member.alias} · {memberStates[member.observed_state] || member.observed_state}</span>) : <span className="text-[10px] text-muted-foreground">عضو شبکه‌ای نیست</span>}</div></article> })}</div>}
            <div className="mt-4 grid gap-3 lg:grid-cols-2">{mesh?.networks.map((network) => <article key={network.id} className="rounded-xl border border-border/70 bg-background/35 p-3"><div className="flex flex-wrap items-center justify-between gap-2"><div className="flex items-center gap-2"><Network size={14} className="text-primary"/><span className="text-xs font-medium">{network.name}</span><Badge variant={network.observed.state === 'available' ? 'success' : network.driver === 'host-local' ? 'outline' : 'destructive'}>{network.observed.state === 'available' ? 'واقعی و در دسترس' : network.driver === 'host-local' ? 'metadata-only' : 'بررسی ناموفق'}</Badge></div><span className="text-[10px] text-muted-foreground">{driverLabels[network.driver]} · {network.member_count} اپ</span></div>{network.observed.error && <p className="mt-2 flex items-center gap-1 text-[10px] text-amber-100"><CircleAlert size={12}/>{network.observed.error}</p>}{network.observed.containers?.length ? <div className="mt-2 flex flex-wrap gap-2">{network.observed.containers.map((container) => <span key={container.id} dir="ltr" className="rounded bg-black/20 px-2 py-1 font-mono text-[10px]">{container.name}{container.ipv4 ? ` · ${container.ipv4}` : ''}</span>)}</div> : network.driver === 'docker-bridge' && network.observed.state === 'available' ? <p className="mt-2 text-[10px] text-muted-foreground">هیچ کانتینری واقعاً به این شبکه وصل نیست.</p> : null}</article>)}</div>
          </CardContent></Card>
          <div className="grid gap-3 md:grid-cols-2">{(mesh?.notes || []).map((note) => <p key={note} className="rounded-lg border border-border/60 bg-card/40 p-3 text-[10px] leading-5 text-muted-foreground">{note}</p>)}</div>
        </TabsContent>
        <TabsContent value="networks" className="grid items-start gap-4 xl:grid-cols-2">
          <Card><CardHeader><CardTitle className="text-sm">ساخت شبکه</CardTitle><CardDescription>Docker bridge با `--internal` ایجاد می‌شود و فقط روی همین VPS کار می‌کند.</CardDescription></CardHeader><CardContent><form className="space-y-3" onSubmit={(event) => void createNetwork(event)}><label className="block space-y-1 text-[10px] text-muted-foreground">نام شبکه<Input required maxLength={48} dir="ltr" pattern="[A-Za-z0-9_-]+" value={name} onChange={(event) => setName(event.target.value)} placeholder="payments_private"/></label><label className="block space-y-1 text-[10px] text-muted-foreground">Driver<select value={driver} onChange={(event) => setDriver(event.target.value as typeof driver)} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs"><option value="docker-bridge">Docker private bridge · جداسازی واقعی کانتینرها</option><option value="host-local">Host-local · فقط نگاشت loopback</option></select></label>{driver === 'host-local' && <p className="text-[10px] leading-5 text-amber-100/80">این گزینه namespace، firewall یا ایزوله‌سازی ایجاد نمی‌کند؛ فقط پورت loopback ثبت می‌کند.</p>}<Button disabled={busy || !name.trim()}><Plus size={13}/>ساخت شبکه</Button></form></CardContent></Card>
          <Card><CardHeader><CardTitle className="text-sm">اتصال اپلیکیشن</CardTitle><CardDescription>کانتینر باید با برچسب مالکیت ARG به همین اپ مربوط باشد. سرویس‌های host-local در همین صفحه فقط ثبت endpoint می‌شوند.</CardDescription></CardHeader><CardContent><form className="space-y-3" onSubmit={(event) => void attach(event)}><label className="block space-y-1 text-[10px] text-muted-foreground">شبکه<select required value={networkId} onChange={(event) => setNetworkId(event.target.value)} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs"><option value="">انتخاب شبکه</option>{mesh?.networks.map((item) => <option key={item.id} value={item.id}>{item.name} · {driverLabels[item.driver]}</option>)}</select></label><label className="block space-y-1 text-[10px] text-muted-foreground">اپلیکیشن<select required value={appId} onChange={(event) => { setAppId(event.target.value); setContainerName('') }} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs"><option value="">انتخاب اپلیکیشن</option>{mesh?.applications.map((app) => <option key={app.id} value={app.id}>{app.name} · {app.framework}</option>)}</select></label><label className="block space-y-1 text-[10px] text-muted-foreground">Service alias<Input required maxLength={48} dir="ltr" pattern="[A-Za-z0-9_-]+" value={alias} onChange={(event) => setAlias(event.target.value)} placeholder="api"/></label>{selectedNetwork?.driver === 'docker-bridge' ? <label className="block space-y-1 text-[10px] text-muted-foreground">کانتینر متعلق به اپ<select required value={containerName} onChange={(event) => setContainerName(event.target.value)} disabled={!compatibleContainers.length} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-xs"><option value="">{compatibleContainers.length ? 'کانتینر را انتخاب کن' : 'کانتینر برچسب‌خورده‌ای برای این اپ پیدا نشد'}</option>{compatibleContainers.map((container) => <option key={container.id} value={container.names}>{container.names} · {container.state}</option>)}</select></label> : <label className="block space-y-1 text-[10px] text-muted-foreground">پورت داخلی loopback<Input required type="number" min={1} max={65535} value={internalPort || selectedApp?.listen_port || ''} onChange={(event) => setInternalPort(event.target.value)} placeholder="3000"/></label>}<Button disabled={busy || !selectedNetwork || !selectedApp || (selectedNetwork?.driver === 'docker-bridge' && !containerName)}><Network size={13}/>ثبت اتصال</Button></form></CardContent></Card>
          <Card className="xl:col-span-2"><CardHeader><CardTitle className="text-sm">اعضای شبکه</CardTitle><CardDescription>قبل از حذف شبکه، عضویت هر اپ را جدا کن.</CardDescription></CardHeader><CardContent className="space-y-2">{mesh?.members.length ? mesh.members.map((member) => <div key={`${member.network_id}:${member.application_id}`} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 p-2.5"><div className="flex min-w-0 flex-wrap items-center gap-2 text-xs"><b>{member.application_name}</b><span className="text-muted-foreground">{mesh.networks.find((network) => network.id === member.network_id)?.name}</span><code dir="ltr" className="font-mono text-[10px]">{member.alias}{member.internal_port ? ` → 127.0.0.1:${member.internal_port}` : ''}</code><Badge variant={member.observed_state === 'connected' ? 'success' : 'outline'}>{memberStates[member.observed_state] || member.observed_state}</Badge></div><Button size="sm" variant="outline" disabled={busy} onClick={() => void detach(member)}><Unplug size={12}/>جداسازی</Button></div>) : <Empty>عضوی ثبت نشده است.</Empty>}{mesh?.networks.map((network) => <div key={network.id} className="flex items-center justify-between gap-3 border-t border-border/60 pt-2"><span className="text-xs">{network.name} · {network.member_count} عضو</span><Button size="sm" variant="ghost" disabled={busy || network.member_count > 0} onClick={() => void removeNetwork(network)}><Trash2 size={12}/>حذف شبکهٔ خالی</Button></div>)}</CardContent></Card>
        </TabsContent>
        <TabsContent value="databases" className="space-y-3">
          <Card><CardHeader><CardTitle className="flex items-center gap-2 text-sm"><Database size={15}/>اتصال‌های دیتابیس اپلیکیشن‌ها</CardTitle><CardDescription>فعلاً فقط تنظیمات غیرمحرمانهٔ Laravel از فایل محیط خوانده می‌شود. این نمایش تست اتصال نیست؛ مدیریت MySQL از ابزار واقعی همان اپ باز می‌شود.</CardDescription></CardHeader><CardContent className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">{mesh?.applications.map((app) => <article key={app.id} className="rounded-xl border border-border/70 bg-background/35 p-3"><div className="flex items-start justify-between gap-2"><div><p className="text-xs font-semibold">{app.name}</p><p className="mt-1 text-[10px] text-muted-foreground">{app.framework}</p></div><Badge variant={app.database.state === 'configured' ? 'success' : 'outline'}>{app.database.state === 'configured' ? 'تنظیم شده' : app.database.state === 'incomplete' ? 'ناقص' : app.database.state === 'unsupported' ? 'بدون آداپتر' : 'در دسترس نیست'}</Badge></div>{app.database.state === 'configured' && <dl className="mt-3 grid grid-cols-2 gap-2 text-[10px]"><Entry label="Driver" value={app.database.connection}/><Entry label="Host" value={app.database.host}/><Entry label="Port" value={app.database.port}/><Entry label="Database" value={app.database.database}/><Entry label="User" value={app.database.username}/><Entry label="رمز" value={app.database.password_configured ? 'ذخیره شده؛ مخفی' : 'تنظیم نشده'}/></dl>}{app.database.state === 'configured' && <div className="mt-2 flex items-center justify-between gap-2"><p className="text-[9px] text-muted-foreground">وضعیت اتصال: بررسی نشده</p>{app.framework === 'laravel' && mesh && <Button size="sm" variant="outline" onClick={() => { window.location.hash = buildApplicationRoute({ appId: app.id, serverUuid: mesh.server_uuid, section: 'runtime' }) }}>مدیریت MySQL</Button>}</div>}</article>)}</CardContent></Card>
        </TabsContent>
      </Tabs>}
    </>}
  </div>
}

function Metric({ icon: Icon, label, value }: { icon: typeof Server; label: string; value: string | number | undefined }) {
  return <Card><CardContent className="flex items-center gap-3 p-3"><span className="grid h-9 w-9 place-items-center rounded-xl border border-primary/20 bg-primary/[.05] text-primary"><Icon size={16}/></span><div><p className="text-[10px] text-muted-foreground">{label}</p><p className="mt-1 text-sm font-semibold">{value ?? '—'}</p></div></CardContent></Card>
}
function Empty({ children }: { children: ReactNode }) { return <p className="rounded-lg border border-dashed border-border p-4 text-center text-xs text-muted-foreground">{children}</p> }
function Entry({ label, value }: { label: string; value?: string }) { return <div className="min-w-0 rounded-md border border-border/50 p-2"><dt className="text-muted-foreground">{label}</dt><dd dir="ltr" className="mt-1 truncate text-left font-mono">{value || '—'}</dd></div> }
