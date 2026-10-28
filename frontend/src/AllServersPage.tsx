import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Activity, ArrowDownToLine, CircleAlert, CircleCheck, LockKeyhole, RefreshCw, Server, ShieldCheck, Waypoints } from 'lucide-react'
import { api, type Dashboard } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Skeleton } from './components/ui/skeleton'
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Node = { id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; endpoint: string; last_seen: number | null; workspace_ready?: boolean }
type Controller = { id: number; name: string; public_name?: string; server_uuid?: string | null; last_seen: number | null }
type Status = { hostname: string; public_name?: string; os: string; architecture: string; arg_version: string; uptime_seconds: number; load: string; memory_total_kib: number; memory_available_kib: number }
type ServerRow = { key: string; name: string; endpoint: string; local: boolean; node?: Node; status?: Status; error?: string; lastSeen?: number | null; responseBytes?: number }
type ProbePulse = { id: number; nodeId: number; bytes: number; at: number; durationMs: number; direction: 'request' | 'response' | 'error'; pending?: boolean }
type BatchState = { nodeId: number; count: number; completed: number; succeeded: number; failed: number }

const CARD_WIDTH = 236
const CARD_GAP = 16
const CARD_TOP = 266
const PARENT_WIDTH = 204
const PULSE_MS = 820
const PROBE_COUNTS = [1, 3, 5, 10, 20]
const LINK_COLOR = '#53c992'

function formatBytes(kib: number) {
  const gib = kib / 1024 / 1024
  return `${gib.toLocaleString('fa-IR', { maximumFractionDigits: 1 })} گیبی‌بایت`
}

function formatPayloadBytes(bytes: number) {
  if (bytes < 1024) return `${bytes.toLocaleString('fa-IR')} بایت`
  return `${(bytes / 1024).toLocaleString('fa-IR', { maximumFractionDigits: 1 })} کیبی‌بایت`
}

function wait(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

export function AllServersPage({ Heading: PageHeading, onOpen }: { Heading: Heading; onOpen: (workspaceKey: string) => void }) {
  const [view, setView] = useState('topology')
  const [rows, setRows] = useState<ServerRow[]>([])
  const [parents, setParents] = useState<Controller[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [refreshing, setRefreshing] = useState(false)
  const [pulses, setPulses] = useState<ProbePulse[]>([])
  const [trafficTotals, setTrafficTotals] = useState({ responses: 0, bytes: 0, lastAt: 0 })
  const [probeCount, setProbeCount] = useState(3)
  const [batch, setBatch] = useState<BatchState | null>(null)
  const pulseId = useRef(0)
  const loadInProgress = useRef(false)
  const manualProbe = useRef(false)

  const load = useCallback(async (quiet = false) => {
    if (loadInProgress.current || manualProbe.current) return
    loadInProgress.current = true
    if (!quiet) setRefreshing(true)
    setError('')
    try {
      const [registry, local] = await Promise.all([
        api<{ server_uuid: string; public_name: string; nodes: Node[]; controllers?: Controller[] }>('nodes'),
        api<Dashboard>('dashboard'),
      ])
      const nodes = registry.nodes || []
      setParents(registry.controllers || [])
      const remotes = new Array<ServerRow>(nodes.length)
      const received: number[] = []
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(8, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          const requestPulseId = ++pulseId.current
          const requestStartedAt = Date.now()
          setPulses((current) => [...current, { id: requestPulseId, nodeId: node.id, bytes: 0, at: requestStartedAt, durationMs: PULSE_MS, direction: 'request' as const, pending: true }].slice(-100))
          try {
            const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(7000) })
            const responseBytes = new TextEncoder().encode(JSON.stringify(status)).byteLength
            await wait(Math.max(0, PULSE_MS - (Date.now() - requestStartedAt)))
            remotes[index] = { key: `${node.workspace_key}:${node.id}`, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, local: false, node, status, lastSeen: node.last_seen, responseBytes }
            received.push(responseBytes)
            setPulses((current) => current.map((pulse) => pulse.id === requestPulseId ? { ...pulse, pending: false } : pulse))
            setPulses((current) => [...current, { id: ++pulseId.current, nodeId: node.id, bytes: responseBytes, at: Date.now(), durationMs: PULSE_MS, direction: 'response' as const }].slice(-100))
          } catch (cause) {
            await wait(Math.max(0, PULSE_MS - (Date.now() - requestStartedAt)))
            setPulses((current) => current.map((pulse) => pulse.id === requestPulseId ? { ...pulse, pending: false } : pulse))
            setPulses((current) => [...current, { id: ++pulseId.current, nodeId: node.id, bytes: 0, at: Date.now(), durationMs: PULSE_MS, direction: 'error' as const }].slice(-100))
            remotes[index] = { key: `${node.workspace_key}:${node.id}`, name: node.public_name || node.name, endpoint: node.endpoint, local: false, node, error: cause instanceof Error ? cause.message : 'سرور پاسخ نداد.', lastSeen: node.last_seen }
          }
        }
      }))
      const localRow: ServerRow = {
        key: `local:${registry.server_uuid}`,
        name: registry.public_name || local.hostname,
        endpoint: local.hostname,
        local: true,
        status: { hostname: local.hostname, os: 'سرور فعلی', architecture: '', arg_version: '', uptime_seconds: local.uptime_seconds, load: local.load, memory_total_kib: local.memory_kib, memory_available_kib: local.available_kib },
      }
      setRows([localRow, ...remotes])
      if (received.length) {
        const bytes = received.reduce((sum, responseBytes) => sum + responseBytes, 0)
        setTrafficTotals((current) => ({ responses: current.responses + received.length, bytes: current.bytes + bytes, lastAt: Date.now() }))
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'فهرست سرورها خوانده نشد.')
    } finally {
      loadInProgress.current = false
      setLoading(false)
      if (!quiet) setRefreshing(false)
    }
  }, [])

  const testNode = useCallback(async (node: Node) => {
    if (manualProbe.current || loadInProgress.current) return
    manualProbe.current = true
    const count = probeCount
    let succeeded = 0
    let failed = 0
    setBatch({ nodeId: node.id, count, completed: 0, succeeded, failed })
    try {
      for (let index = 0; index < count; index += 1) {
        const requestId = ++pulseId.current
        const startedAt = Date.now()
        setBatch({ nodeId: node.id, count, completed: index, succeeded, failed })
        setPulses((current) => [...current, { id: requestId, nodeId: node.id, bytes: 0, at: startedAt, durationMs: PULSE_MS, direction: 'request' as const, pending: true }].slice(-100))
        try {
          const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(8000) })
          const responseBytes = new TextEncoder().encode(JSON.stringify(status)).byteLength
          await wait(Math.max(0, PULSE_MS - (Date.now() - startedAt)))
          setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
          setPulses((current) => [...current, { id: ++pulseId.current, nodeId: node.id, bytes: responseBytes, at: Date.now(), durationMs: PULSE_MS, direction: 'response' as const }].slice(-100))
          succeeded += 1
          setRows((current) => current.map((row) => row.node?.id === node.id ? { ...row, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, status, error: undefined, responseBytes } : row))
          setTrafficTotals((current) => ({ responses: current.responses + 1, bytes: current.bytes + responseBytes, lastAt: Date.now() }))
        } catch (cause) {
          await wait(Math.max(0, PULSE_MS - (Date.now() - startedAt)))
          setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
          setPulses((current) => [...current, { id: ++pulseId.current, nodeId: node.id, bytes: 0, at: Date.now(), durationMs: PULSE_MS, direction: 'error' as const }].slice(-100))
          failed += 1
          const message = cause instanceof Error ? cause.message : 'درخواست تست ناموفق بود.'
          setRows((current) => current.map((row) => row.node?.id === node.id ? { ...row, error: message } : row))
        }
        setBatch({ nodeId: node.id, count, completed: index + 1, succeeded, failed })
        if (index < count - 1) await wait(PULSE_MS)
      }
    } finally {
      manualProbe.current = false
      setBatch((current) => current ? { ...current, completed: count, succeeded, failed } : current)
      window.setTimeout(() => setBatch((current) => current?.nodeId === node.id ? null : current), 6000)
    }
  }, [probeCount])

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (!rows.some((row) => row.node)) return
    const timer = window.setInterval(() => { if (document.visibilityState === 'visible') void load(true) }, 15000)
    return () => window.clearInterval(timer)
  }, [load, rows.length])
  useEffect(() => {
    const timer = window.setInterval(() => setPulses((current) => current.filter((pulse) => pulse.pending || Date.now() - pulse.at < pulse.durationMs + 180)), 100)
    return () => window.clearInterval(timer)
  }, [])

  const onlineCount = rows.filter((row) => row.status).length
  const workers = rows.filter((row) => row.node)
  const cardsWidth = workers.length * CARD_WIDTH + Math.max(0, workers.length - 1) * CARD_GAP
  const parentsWidth = parents.length * PARENT_WIDTH + Math.max(0, parents.length - 1) * CARD_GAP
  const graphWidth = Math.max(760, cardsWidth + 48, parentsWidth + 48)
  const graphCenter = graphWidth / 2
  const firstCardX = (graphWidth - cardsWidth) / 2
  const firstParentX = (graphWidth - parentsWidth) / 2
  const getCardCenter = (index: number) => firstCardX + index * (CARD_WIDTH + CARD_GAP) + CARD_WIDTH / 2
  const getParentCenter = (index: number) => firstParentX + index * (PARENT_WIDTH + CARD_GAP) + PARENT_WIDTH / 2
  const action = <Button variant="outline" size="sm" onClick={() => void load()} disabled={refreshing || Boolean(batch)}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی وضعیت</Button>

  return <>
    <PageHeading title="همهٔ سرورها" description="نمای خواندنی وضعیت سرورهای متصل؛ برای انجام عملیات، ابتدا فضای‌کار همان سرور را انتخاب کن." action={action}/>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
    <Tabs value={view} onValueChange={setView} dir="rtl" className="space-y-4">
      <TabsList className="h-auto w-full flex-wrap justify-start sm:w-auto" aria-label="نمایش وضعیت همهٔ سرورها">
        <TabsTrigger value="topology"><Waypoints size={14} className="ml-2"/>نقشهٔ ارتباط</TabsTrigger>
        <TabsTrigger value="inventory"><Server size={14} className="ml-2"/>فهرست و منابع</TabsTrigger>
      </TabsList>
      <TabsContent value="topology" className="min-w-0">
    <Card className="mb-5 overflow-hidden border-border/80 bg-card/45">
      <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5">
        <div><CardTitle className="flex items-center gap-2 text-base"><Waypoints size={18} className="text-primary"/><span>ARG Mesh</span><span className="text-sm font-normal text-muted-foreground">· نقشهٔ ارتباط پنل‌ها</span></CardTitle><CardDescription className="mt-1">تست دستی، تعداد درخواست واقعی HTTPS را به Node می‌فرستد و پاسخ‌ها را در مسیر برگشت نشان می‌دهد.</CardDescription></div>
        <div className="flex flex-wrap items-center gap-2 text-[10px]"><span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2.5 py-1.5 text-emerald-100"><span className="h-1.5 w-1.5 rounded-full bg-emerald-300"/>پاسخ‌گو: {onlineCount}/{rows.length}</span><span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-background/40 px-2.5 py-1.5 text-muted-foreground"><ArrowDownToLine size={12}/>پاسخ دریافت‌شده: {trafficTotals.responses.toLocaleString('fa-IR')} · {formatPayloadBytes(trafficTotals.bytes)}</span><label className="flex items-center gap-2 rounded-lg border border-border bg-background/40 px-2 py-1 text-muted-foreground">تعداد تست<select aria-label="تعداد درخواست تست برای Node" className="h-7 rounded-md bg-background px-2 text-xs text-foreground" value={probeCount} disabled={Boolean(batch)} onChange={(event) => setProbeCount(Number(event.target.value))}>{PROBE_COUNTS.map((count) => <option key={count} value={count}>{count.toLocaleString('fa-IR')}</option>)}</select></label></div>
      </CardHeader>
      {loading ? <CardContent className="p-4 pt-0"><Skeleton className="h-72 rounded-xl"/></CardContent> : workers.length === 0 && parents.length === 0 ? <CardContent className="p-5 pt-0"><div className="flex min-h-36 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border text-center"><LockKeyhole size={22} className="text-muted-foreground"/><p className="text-sm font-medium">هنوز اتصال Node یا پنل مادر ثبت نشده است</p><p className="text-xs text-muted-foreground">پس از جفت‌سازی سرورها، ارتباط‌های ورودی و خروجی در این نقشه نمایش داده می‌شوند.</p></div></CardContent> : <CardContent className="p-0">
        <div className="overflow-x-auto px-3 pb-3 sm:px-4" dir="ltr">
          <div className="relative mx-auto" style={{ width: graphWidth, height: 468 }}>
            <svg className="pointer-events-none absolute inset-0 z-0" width={graphWidth} height="468" viewBox={`0 0 ${graphWidth} 468`} role="img" aria-label="نمودار ارتباط پنل‌های مادر، این سرور و Nodeهای ARG">
              <defs><filter id="topology-glow"><feGaussianBlur stdDeviation="3" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs>
              {parents.map((parent, index) => {
                const x = getParentCenter(index)
                return <g key={`parent-link-${parent.id}`}><path d={`M ${x} 62 L ${x} 78 L ${graphCenter} 78 L ${graphCenter} 94`} fill="none" stroke="#7bb8ff" strokeOpacity="0.58" strokeWidth="2"/><circle cx={x} cy="62" r="3.5" fill="#7bb8ff"/></g>
              })}
              {workers.map((row, index) => {
                const nodeId = row.node!.id
                const x = getCardCenter(index)
                const linkPath = `M ${graphCenter} 168 L ${graphCenter} 211 L ${x} 211 L ${x} ${CARD_TOP}`
                const reversePath = `M ${x} ${CARD_TOP} L ${x} 211 L ${graphCenter} 211 L ${graphCenter} 168`
                const isOnline = Boolean(row.status)
                return <g key={nodeId}>
                  <path d={linkPath} fill="none" stroke={isOnline ? LINK_COLOR : '#707786'} strokeOpacity={isOnline ? 0.55 : 0.38} strokeWidth="2" strokeDasharray={isOnline ? undefined : '5 7'}/>
                  {pulses.filter((pulse) => pulse.nodeId === nodeId).map((pulse) => {
                    const path = pulse.direction === 'response' ? reversePath : linkPath
                    const color = pulse.direction === 'request' ? '#dda46f' : pulse.direction === 'error' ? '#fb7185' : '#8ef0bf'
                    const label = pulse.direction === 'request' ? 'درخواست واقعی بررسی ارتباط' : pulse.direction === 'error' ? 'نتیجهٔ خطای درخواست واقعی' : `پاسخ واقعی Node · ${formatPayloadBytes(pulse.bytes)}`
                    return <circle key={`packet-${pulse.id}`} className="arg-topology-packet" r="5" fill={color} filter="url(#topology-glow)"><title>{label}</title><animateMotion key={`motion-${pulse.id}`} begin="0s" restart="always" dur={`${pulse.durationMs}ms`} calcMode="spline" keyTimes="0;1" keySplines="0.42 0 0.58 1" fill="freeze" path={path}/></circle>
                  })}
                  <circle cx={x} cy={CARD_TOP} r="4" fill={isOnline ? LINK_COLOR : '#858b98'}/>
                </g>
              })}
            </svg>
            {parents.map((parent, index) => {
              const age = parent.last_seen ? Math.max(0, Math.floor(Date.now() / 1000 - parent.last_seen)) : null
              return <div key={`parent-${parent.id}`} dir="rtl" className="absolute z-10 flex h-[54px] items-center gap-2 rounded-xl border border-sky-400/25 bg-card/95 px-3 shadow-md shadow-black/10" style={{ top: 6, left: getParentCenter(index) - PARENT_WIDTH / 2, width: PARENT_WIDTH }}><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-sky-400/20 bg-sky-400/[.07] text-sky-200"><Server size={15}/></span><div className="min-w-0 flex-1"><p className="truncate text-xs font-semibold">{parent.public_name || parent.name}</p><p className="mt-0.5 truncate text-[9px] text-muted-foreground">پنل مادر · {age === null ? 'هنوز درخواستی ثبت نشده' : `آخرین درخواست ${age.toLocaleString('fa-IR')} ثانیه پیش`}</p></div></div>
            })}
            <div className="absolute inset-x-6 top-[94px] z-10">
              <div dir="rtl" className="flex min-h-[78px] items-center gap-3 rounded-2xl border border-primary/20 bg-card/95 px-4 py-3 shadow-lg shadow-black/10">
                <span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl border border-primary/25 bg-primary/[.08] text-primary"><ShieldCheck size={21}/></span>
                <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><p className="font-semibold">{rows.find((row) => row.local)?.name || 'سرور فعلی'}</p><span className="rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2 py-0.5 text-[10px] text-emerald-100">این سرور · آنلاین</span></div><p className="mt-1 truncate text-xs text-muted-foreground">{parents.length ? `تحت مدیریت ${parents.length.toLocaleString('fa-IR')} پنل مادر · مدیریت ${workers.length.toLocaleString('fa-IR')} Node` : `پنل هماهنگ‌کنندهٔ ${workers.length.toLocaleString('fa-IR')} Node جفت‌شده`}</p></div>
                <span className="hidden items-center gap-1.5 text-[10px] text-muted-foreground sm:flex"><LockKeyhole size={13}/>TLS</span>
              </div>
            </div>
            <div className="absolute inset-x-0 z-10 flex" style={{ top: CARD_TOP, gap: CARD_GAP, paddingLeft: firstCardX }}>
              {workers.map((row) => {
                const node = row.node!
                const age = node.last_seen ? Math.max(0, Math.floor(Date.now() / 1000 - node.last_seen)) : null
                const currentBatch = batch?.nodeId === node.id ? batch : null
                return <div key={row.key} dir="rtl" className="flex h-[202px] w-[236px] shrink-0 flex-col rounded-2xl border border-border/80 bg-card/95 p-3 text-right shadow-lg shadow-black/10">
                  <button type="button" className="flex min-h-0 flex-1 flex-col text-right focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary" onClick={() => onOpen(node.workspace_key)} disabled={!node.workspace_ready} aria-label={`${row.name}؛ ${node.workspace_ready ? 'ورود به فضای کار VPS' : 'اتصال کامل آماده نیست'}`}>
                    <span className="flex w-full items-start justify-between gap-2"><span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/[.06] text-primary"><Server size={17}/></span><span className={`inline-flex items-center gap-1 rounded-full border px-2 py-1 text-[10px] ${row.status ? 'border-emerald-400/20 bg-emerald-400/[.05] text-emerald-100' : 'border-amber-400/20 bg-amber-400/[.05] text-amber-100'}`}>{row.status ? <CircleCheck size={11}/> : <CircleAlert size={11}/ >}{row.status ? 'متصل' : 'قطع'}</span></span>
                    <span className="mt-2 block w-full truncate text-sm font-semibold">{row.name}</span><span dir="ltr" className="mt-0.5 block w-full truncate text-left font-mono text-[10px] text-muted-foreground">{row.endpoint}</span>
                    <span className="mt-2 flex w-full items-center justify-between gap-2 border-t border-border/70 pt-2 text-[10px] text-muted-foreground"><span>{row.status ? `ARG v${row.status.arg_version}` : age === null ? 'هنوز ارتباط ثبت نشده' : `آخرین پاسخ ${age.toLocaleString('fa-IR')} ثانیه پیش`}</span><span>{row.responseBytes ? `≈ ${formatPayloadBytes(row.responseBytes)}` : '—'}</span></span>
                    <span className="mt-auto flex w-full items-center justify-between pt-2 text-[10px] text-primary">{node.workspace_ready ? 'ورود به پنل سرور' : 'نیاز به جفت‌سازی'}<span aria-hidden="true">←</span></span>
                  </button>
                  <div className="mt-2 flex items-center justify-between gap-2 border-t border-border/70 pt-2"><span className="truncate text-[9px] text-muted-foreground">{currentBatch ? `${currentBatch.completed}/${currentBatch.count} · موفق ${currentBatch.succeeded} · ناموفق ${currentBatch.failed}` : 'تست ارتباط واقعی'}</span><Button type="button" size="sm" variant="outline" className="h-7 shrink-0 px-2 text-[10px]" disabled={Boolean(batch) || loadInProgress.current} onClick={() => void testNode(node)}>{currentBatch ? <><RefreshCw size={11} className="animate-spin"/>{currentBatch.completed}/{currentBatch.count}</> : `تست ${probeCount.toLocaleString('fa-IR')} بار`}</Button></div>
                </div>
              })}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-start gap-2 border-t border-border/70 px-4 py-3 text-[10px] leading-5 text-muted-foreground"><Activity size={13} className="mt-0.5 shrink-0 text-primary"/><p>هر تست یک درخواست واقعی HTTPS به Node است؛ مسیر برگشت پس از دریافت پاسخ نمایش داده می‌شود. حرکت هر leg با زمان حداقل‌دار پخش می‌شود تا پاسخ‌های سریع هم دیده شوند. حجم فقط از payload JSON برآورد می‌شود و کل ترافیک شبکه نیست. پنل‌های مادر از اتصال‌های ورودی واقعی این سرور نمایش داده شده‌اند.</p></div>
      </CardContent>}
    </Card>
      </TabsContent>
      <TabsContent value="inventory" className="min-w-0 space-y-4">
    <div className="mb-5 grid gap-3 sm:grid-cols-2">
      <Card><CardContent className="flex items-center gap-3 p-4"><Server className="text-primary" size={19}/><div><p className="text-xs text-muted-foreground">سرورهای ثبت‌شده</p><p className="mt-1 font-semibold">{loading ? '—' : rows.length.toLocaleString('fa-IR')}</p></div></CardContent></Card>
      <Card><CardContent className="flex items-center gap-3 p-4"><Activity className="text-emerald-300" size={19}/><div><p className="text-xs text-muted-foreground">پاسخ‌گو در آخرین بررسی</p><p className="mt-1 font-semibold">{loading ? '—' : `${onlineCount.toLocaleString('fa-IR')} از ${rows.length.toLocaleString('fa-IR')}`}</p></div></CardContent></Card>
    </div>
    {loading ? <div className="grid gap-3 lg:grid-cols-2">{[0, 1].map((item) => <Skeleton key={item} className="h-44 rounded-2xl"/>)}</div> : <div className="grid gap-3 lg:grid-cols-2">
      {rows.map((row) => {
        const memoryUsed = row.status ? Math.max(0, row.status.memory_total_kib - row.status.memory_available_kib) : null
        const memoryPercent = row.status?.memory_total_kib ? Math.min(100, Math.round((memoryUsed! / row.status.memory_total_kib) * 100)) : null
        const days = row.lastSeen ? Math.max(0, Math.floor(Date.now() / 1000 - row.lastSeen)) : null
        return <Card key={row.key || 'local'} className="overflow-hidden">
          <CardHeader className="pb-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><CardTitle className="flex items-center gap-2 text-base"><Server size={16} className="shrink-0 text-primary"/><span className="truncate">{row.name}</span></CardTitle><CardDescription dir="ltr" className="mt-2 truncate text-left font-mono text-[11px]">{row.endpoint}</CardDescription></div><span className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2 py-1 text-[10px] ${row.status ? 'border-emerald-400/20 bg-emerald-400/[.06] text-emerald-200' : 'border-amber-400/20 bg-amber-400/[.06] text-amber-100'}`}>{row.status ? <CircleCheck size={12}/> : <CircleAlert size={12}/>} {row.status ? (row.local ? 'سرور فعلی' : 'پاسخ‌گو') : 'در دسترس نیست'}</span></div></CardHeader>
          <CardContent className="space-y-3">
            {row.error && <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">{row.error}{days !== null ? ` · آخرین ارتباط ${days.toLocaleString('fa-IR')} ثانیه پیش` : ''}</p>}
            {row.status && <div className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-4"><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">سیستم‌عامل</p><p className="mt-1 truncate">{row.status.os || '—'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">نسخه ARG</p><p dir="ltr" className="mt-1 truncate text-right font-mono">{row.status.arg_version || 'محلی'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">RAM مصرفی</p><p className="mt-1 truncate">{memoryUsed !== null ? `${formatBytes(memoryUsed)} (${memoryPercent}٪)` : 'نامشخص'}</p></div><div className="rounded-lg border border-border/70 p-2"><p className="text-muted-foreground">Load</p><p dir="ltr" className="mt-1 truncate text-right font-mono">{row.status.load || '—'}</p></div></div>}
            {row.node && <Button variant="outline" size="sm" className="w-full" disabled={!row.node.workspace_ready} onClick={() => onOpen(row.node!.workspace_key)}>{row.node.workspace_ready ? 'رفتن به فضای‌کار این سرور' : 'برای مدیریت، اتصال را دوباره جفت‌سازی کن'}</Button>}
          </CardContent>
        </Card>
      })}
    </div>}
    <p className="mt-4 text-xs leading-6 text-muted-foreground">وضعیت سرور راه‌دور فقط نتیجهٔ آخرین بررسی است و پایش پیوسته محسوب نمی‌شود. اگر ارتباط قطع باشد، ARG عملیات آن سرور را به سرور دیگری منتقل نمی‌کند.</p>
      </TabsContent>
    </Tabs>
  </>
}
