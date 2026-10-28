import { useCallback, useEffect, useRef, useState } from 'react'
import { Activity, ArrowDownToLine, CircleAlert, CircleCheck, LockKeyhole, RefreshCw, Server, ShieldCheck, Waypoints } from 'lucide-react'
import { api, type Dashboard } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Skeleton } from './components/ui/skeleton'

type Node = { id: number; workspace_key: string; name: string; public_name?: string; endpoint: string; last_seen: number | null; workspace_ready?: boolean }
type Controller = { id: number; name: string; public_name?: string; server_uuid?: string | null; last_seen: number | null }
type Registry = { server_uuid: string; public_name: string; nodes: Node[]; controllers?: Controller[] }
type Status = { hostname: string; public_name?: string; arg_version: string; load: string; memory_total_kib: number; memory_available_kib: number }
type ServerRow = { key: string; name: string; endpoint: string; local: boolean; node?: Node; status?: Status; error?: string; responseBytes?: number; latencyMs?: number }
type Pulse = { id: number; nodeId: number; direction: 'request' | 'response' | 'error'; bytes: number; at: number; durationMs: number; pending?: boolean }
type BatchState = { nodeId: number; count: number; completed: number; succeeded: number; failed: number }

const CARD_WIDTH = 204
const CARD_GAP = 12
const CARD_TOP = 266
const PARENT_WIDTH = 204
const PULSE_MS = 820
const LINK_COLOR = '#53c992'
const PROBE_COUNTS = [1, 3, 5, 10, 20]

function payload(bytes: number) {
  return bytes < 1024 ? `${bytes.toLocaleString('fa-IR')} بایت` : `${(bytes / 1024).toLocaleString('fa-IR', { maximumFractionDigits: 1 })} کیبی‌بایت`
}

function wait(ms: number) {
  return new Promise((resolve) => window.setTimeout(resolve, ms))
}

export function FederationTopologyWidget({ onOpen }: { onOpen: (workspaceKey: string) => void }) {
  const [rows, setRows] = useState<ServerRow[]>([])
  const [parents, setParents] = useState<Controller[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState('')
  const [pulses, setPulses] = useState<Pulse[]>([])
  const [totals, setTotals] = useState({ checks: 0, bytes: 0, lastAt: 0 })
  const [probeCount, setProbeCount] = useState(3)
  const [batch, setBatch] = useState<BatchState | null>(null)
  const sequence = useRef(0)
  const busy = useRef(false)
  const manualProbe = useRef(false)

  const load = useCallback(async (quiet = false) => {
    if (busy.current || manualProbe.current) return
    busy.current = true
    if (!quiet) setRefreshing(true)
    setError('')
    try {
      const [registry, local] = await Promise.all([
        api<Registry>('nodes'),
        api<Dashboard>('dashboard'),
      ])
      const nodes = registry.nodes || []
      setParents(registry.controllers || [])
      const remoteRows = new Array<ServerRow>(nodes.length)
      const completed: number[] = []
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(6, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          const requestId = ++sequence.current
          const startedAt = Date.now()
          setPulses((current) => [...current, { id: requestId, nodeId: node.id, direction: 'request' as const, bytes: 0, at: startedAt, durationMs: PULSE_MS, pending: true }].slice(-100))
          try {
            const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(8000) })
            const responseBytes = new TextEncoder().encode(JSON.stringify(status)).byteLength
            const latencyMs = Date.now() - startedAt
            await wait(Math.max(0, PULSE_MS - latencyMs))
            remoteRows[index] = { key: `${node.workspace_key}:${node.id}`, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, local: false, node, status, responseBytes, latencyMs }
            completed.push(responseBytes)
            setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
            setPulses((current) => [...current, { id: ++sequence.current, nodeId: node.id, direction: 'response' as const, bytes: responseBytes, at: Date.now(), durationMs: PULSE_MS }].slice(-100))
          } catch (cause) {
            await wait(Math.max(0, PULSE_MS - (Date.now() - startedAt)))
            remoteRows[index] = { key: `${node.workspace_key}:${node.id}`, name: node.public_name || node.name, endpoint: node.endpoint, local: false, node, error: cause instanceof Error ? cause.message : 'سرور پاسخ نداد.', latencyMs: Date.now() - startedAt }
            setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
            setPulses((current) => [...current, { id: ++sequence.current, nodeId: node.id, direction: 'error' as const, bytes: 0, at: Date.now(), durationMs: PULSE_MS }].slice(-100))
          }
        }
      }))
      const localRow: ServerRow = {
        key: `local:${registry.server_uuid}`,
        name: registry.public_name || local.hostname,
        endpoint: local.hostname,
        local: true,
        status: { hostname: local.hostname, arg_version: '', load: local.load, memory_total_kib: local.memory_kib, memory_available_kib: local.available_kib },
      }
      setRows([localRow, ...remoteRows])
      if (completed.length) setTotals((current) => ({ checks: current.checks + completed.length, bytes: current.bytes + completed.reduce((sum, value) => sum + value, 0), lastAt: Date.now() }))
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'فهرست پنل‌های متصل خوانده نشد.')
    } finally {
      busy.current = false
      setLoading(false)
      if (!quiet) setRefreshing(false)
    }
  }, [])

  const testNode = useCallback(async (node: Node) => {
    if (manualProbe.current || busy.current) return
    manualProbe.current = true
    const count = probeCount
    setBatch({ nodeId: node.id, count, completed: 0, succeeded: 0, failed: 0 })
    let succeeded = 0
    let failed = 0
    try {
      for (let index = 0; index < count; index += 1) {
        const requestId = ++sequence.current
        const startedAt = Date.now()
        setBatch({ nodeId: node.id, count, completed: index, succeeded, failed })
        setPulses((current) => [...current, { id: requestId, nodeId: node.id, direction: 'request' as const, bytes: 0, at: startedAt, durationMs: PULSE_MS, pending: true }].slice(-100))
        try {
          const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(8000) })
          const responseBytes = new TextEncoder().encode(JSON.stringify(status)).byteLength
          const latencyMs = Date.now() - startedAt
          await wait(Math.max(0, PULSE_MS - latencyMs))
          setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
          setPulses((current) => [...current, { id: ++sequence.current, nodeId: node.id, direction: 'response' as const, bytes: responseBytes, at: Date.now(), durationMs: PULSE_MS }].slice(-100))
          succeeded += 1
          setRows((current) => current.map((row) => row.node?.id === node.id ? { ...row, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, status, error: undefined, responseBytes, latencyMs } : row))
          setTotals((current) => ({ checks: current.checks + 1, bytes: current.bytes + responseBytes, lastAt: Date.now() }))
        } catch (cause) {
          await wait(Math.max(0, PULSE_MS - (Date.now() - startedAt)))
          setPulses((current) => current.map((pulse) => pulse.id === requestId ? { ...pulse, pending: false } : pulse))
          setPulses((current) => [...current, { id: ++sequence.current, nodeId: node.id, direction: 'error' as const, bytes: 0, at: Date.now(), durationMs: PULSE_MS }].slice(-100))
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
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void load(true)
    }, 30000)
    return () => window.clearInterval(timer)
  }, [load])
  useEffect(() => {
    const timer = window.setInterval(() => setPulses((current) => current.filter((pulse) => pulse.pending || Date.now() - pulse.at < pulse.durationMs + 180)), 100)
    return () => window.clearInterval(timer)
  }, [])

  const workers = rows.filter((row) => row.node)
  const online = rows.filter((row) => row.status).length
  const cardRowWidth = workers.length * CARD_WIDTH + Math.max(0, workers.length - 1) * CARD_GAP
  const parentRowWidth = parents.length * PARENT_WIDTH + Math.max(0, parents.length - 1) * CARD_GAP
  const graphWidth = Math.max(680, cardRowWidth + 40, parentRowWidth + 40)
  const graphCenter = graphWidth / 2
  const firstCardX = (graphWidth - cardRowWidth) / 2
  const firstParentX = (graphWidth - parentRowWidth) / 2
  const centerOf = (index: number) => firstCardX + index * (CARD_WIDTH + CARD_GAP) + CARD_WIDTH / 2
  const parentCenterOf = (index: number) => firstParentX + index * (PARENT_WIDTH + CARD_GAP) + PARENT_WIDTH / 2

  return <Card className="overflow-hidden border-border/80 bg-card/45">
    <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5">
      <div><CardTitle className="flex items-center gap-2 text-base"><Waypoints size={18} className="text-primary"/><span>ARG Mesh</span><span className="text-sm font-normal text-muted-foreground">· شبکهٔ پنل‌ها</span></CardTitle><CardDescription className="mt-1">درخواست‌های تست واقعی به Node و پاسخ همان درخواست در مسیر برگشت نمایش داده می‌شوند.</CardDescription></div>
      <div className="flex flex-wrap items-center gap-2"><span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2.5 py-1.5 text-[10px] text-emerald-100"><span className="h-1.5 w-1.5 rounded-full bg-emerald-300"/>پاسخ‌گو: {loading ? '—' : `${online}/${rows.length}`}</span><label className="flex items-center gap-2 rounded-lg border border-border bg-background/40 px-2 py-1 text-[10px] text-muted-foreground">تعداد تست<select aria-label="تعداد درخواست تست برای Node" className="h-7 rounded-md bg-background px-2 text-xs text-foreground" value={probeCount} disabled={Boolean(batch)} onChange={(event) => setProbeCount(Number(event.target.value))}>{PROBE_COUNTS.map((count) => <option key={count} value={count}>{count.toLocaleString('fa-IR')}</option>)}</select></label><Button variant="outline" size="sm" onClick={() => void load()} disabled={refreshing || Boolean(batch)}><RefreshCw size={13} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی</Button></div>
    </CardHeader>
    {error && <CardContent className="pt-0"><p role="alert" className="rounded-lg border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs text-rose-200">{error}</p></CardContent>}
    {loading ? <CardContent className="p-4 pt-0"><Skeleton className="h-72 rounded-xl"/></CardContent> : workers.length === 0 && parents.length === 0 ? <CardContent className="p-4 pt-0"><div className="flex min-h-36 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border text-center"><LockKeyhole size={22} className="text-muted-foreground"/><p className="text-sm font-medium">هنوز اتصال Node جفت نشده است</p><p className="text-xs text-muted-foreground">پس از جفت‌سازی، Nodeها و پنل بالادستی این سرور در نقشه نشان داده می‌شوند.</p><a href="#/nodes" className="mt-1 text-xs text-primary hover:underline">مدیریت Nodeها</a></div></CardContent> : <CardContent className="p-0">
      <div className="overflow-x-auto px-3 pb-3 sm:px-4" dir="ltr">
        <div className="relative mx-auto" style={{ width: graphWidth, height: 468 }}>
          <svg className="pointer-events-none absolute inset-0 z-0" width={graphWidth} height="468" viewBox={`0 0 ${graphWidth} 468`} role="img" aria-label="نمودار ارتباط ARG Mesh با پنل‌های بالادستی و Nodeهای مدیریت‌شده">
            <defs><filter id="arg-mesh-pulse-glow"><feGaussianBlur stdDeviation="3" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs>
            {parents.map((parent, index) => {
              const x = parentCenterOf(index)
              return <g key={`parent-link-${parent.id}`}><path d={`M ${x} 62 L ${x} 78 L ${graphCenter} 78 L ${graphCenter} 94`} fill="none" stroke="#7bb8ff" strokeOpacity="0.58" strokeWidth="2"/><circle cx={x} cy="62" r="3.5" fill="#7bb8ff"/></g>
            })}
            {workers.map((row, index) => {
              const nodeId = row.node!.id
              const x = centerOf(index)
              const outbound = `M ${graphCenter} 168 L ${graphCenter} 211 L ${x} 211 L ${x} ${CARD_TOP}`
              const inbound = `M ${x} ${CARD_TOP} L ${x} 211 L ${graphCenter} 211 L ${graphCenter} 168`
              const connected = Boolean(row.status)
              return <g key={nodeId}>
                <path d={outbound} fill="none" stroke={connected ? LINK_COLOR : '#737987'} strokeOpacity={connected ? 0.56 : 0.34} strokeWidth="2" strokeDasharray={connected ? undefined : '5 7'}/>
                {pulses.filter((pulse) => pulse.nodeId === nodeId).map((pulse) => {
                  const path = pulse.direction === 'response' ? inbound : outbound
                  const color = pulse.direction === 'request' ? '#dda46f' : pulse.direction === 'error' ? '#fb7185' : '#8ef0bf'
                  const title = pulse.direction === 'request' ? 'درخواست واقعی بررسی وضعیت Node' : pulse.direction === 'error' ? 'درخواست واقعی ناموفق؛ مسیر برگشت نشان‌دهندهٔ نتیجهٔ خطاست' : `پاسخ واقعی Node · payload تقریبی ${payload(pulse.bytes)}`
                  return <circle key={`packet-${pulse.id}`} className="arg-topology-packet" r={pulse.direction === 'error' ? 4 : 5} fill={color} filter="url(#arg-mesh-pulse-glow)"><title>{title}</title><animateMotion key={`motion-${pulse.id}`} begin="0s" restart="always" dur={`${pulse.durationMs}ms`} calcMode="spline" keyTimes="0;1" keySplines="0.42 0 0.58 1" fill="freeze" path={path}/></circle>
                })}
                <circle cx={x} cy={CARD_TOP} r="4" fill={connected ? LINK_COLOR : '#858b98'}/>
              </g>
            })}
          </svg>
          {parents.map((parent, index) => {
            const age = parent.last_seen ? Math.max(0, Math.floor(Date.now() / 1000 - parent.last_seen)) : null
            return <div key={`parent-${parent.id}`} dir="rtl" className="absolute z-10 flex h-[54px] items-center gap-2 rounded-xl border border-sky-400/25 bg-card/95 px-3 shadow-md shadow-black/10" style={{ top: 6, left: parentCenterOf(index) - PARENT_WIDTH / 2, width: PARENT_WIDTH }}>
              <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-sky-400/20 bg-sky-400/[.07] text-sky-200"><Server size={15}/></span><div className="min-w-0 flex-1"><p className="truncate text-xs font-semibold">{parent.public_name || parent.name}</p><p className="mt-0.5 truncate text-[9px] text-muted-foreground">پنل مادر · {age === null ? 'هنوز درخواستی ثبت نشده' : `آخرین درخواست ${age.toLocaleString('fa-IR')} ثانیه پیش`}</p></div>
            </div>
          })}
          <div className="absolute inset-x-6 top-[94px] z-10"><div dir="rtl" className="flex min-h-[74px] items-center gap-3 rounded-2xl border border-primary/20 bg-card/95 px-4 py-3 shadow-lg shadow-black/10"><span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl border border-primary/25 bg-primary/[.08] text-primary"><ShieldCheck size={20}/></span><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><p className="font-semibold">{rows.find((row) => row.local)?.name || 'سرور فعلی'}</p><span className="rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2 py-0.5 text-[10px] text-emerald-100">این سرور · آنلاین</span></div><p className="mt-1 truncate text-xs text-muted-foreground">{parents.length ? `تحت مدیریت ${parents.length.toLocaleString('fa-IR')} پنل بالادستی · مدیریت ${workers.length.toLocaleString('fa-IR')} Node` : `کنترل‌کنندهٔ ${workers.length.toLocaleString('fa-IR')} Node جفت‌شده`}</p></div><span className="hidden items-center gap-1.5 text-[10px] text-muted-foreground sm:flex"><LockKeyhole size={13}/>TLS</span></div></div>
          <div className="absolute z-10 flex" style={{ top: CARD_TOP, gap: CARD_GAP, paddingLeft: firstCardX }}>
            {workers.map((row) => {
              const node = row.node!
              const age = node.last_seen ? Math.max(0, Math.floor(Date.now() / 1000 - node.last_seen)) : null
              const currentBatch = batch?.nodeId === node.id ? batch : null
              return <div key={row.key} dir="rtl" className="flex h-[190px] w-[204px] shrink-0 flex-col rounded-2xl border border-border/80 bg-card/95 p-3 text-right shadow-lg shadow-black/10">
                <button type="button" className="flex min-h-0 flex-1 flex-col text-right focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary" onClick={() => onOpen(node.workspace_key)} disabled={!node.workspace_ready} aria-label={`${row.name}؛ ${node.workspace_ready ? 'ورود به فضای کار VPS' : 'اتصال کامل آماده نیست'}`}>
                  <span className="flex w-full items-start justify-between gap-2"><span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg border border-primary/20 bg-primary/[.06] text-primary"><Server size={16}/></span><span className={`inline-flex items-center gap-1 rounded-full border px-2 py-1 text-[10px] ${row.status ? 'border-emerald-400/20 bg-emerald-400/[.05] text-emerald-100' : 'border-amber-400/20 bg-amber-400/[.05] text-amber-100'}`}>{row.status ? <CircleCheck size={11}/> : <CircleAlert size={11}/ >}{row.status ? 'متصل' : 'قطع'}</span></span>
                  <span className="mt-2 block w-full truncate text-sm font-semibold">{row.name}</span><span dir="ltr" className="mt-0.5 block w-full truncate text-left font-mono text-[10px] text-muted-foreground">{row.endpoint}</span>
                  <span className="mt-2 flex w-full items-center justify-between gap-1 border-t border-border/70 pt-2 text-[10px] text-muted-foreground"><span>{row.status ? `پاسخ ${row.latencyMs ?? '—'} ms` : age === null ? 'هنوز ارتباط ثبت نشده' : `آخرین پاسخ ${age.toLocaleString('fa-IR')} ثانیه پیش`}</span><span>{row.responseBytes ? `≈ ${payload(row.responseBytes)}` : '—'}</span></span>
                  <span className="mt-auto flex w-full items-center justify-between pt-2 text-[10px] text-primary">{node.workspace_ready ? 'ورود به فضای کار' : 'نیاز به جفت‌سازی'}<span aria-hidden="true">←</span></span>
                </button>
                <div className="mt-2 flex items-center justify-between gap-2 border-t border-border/70 pt-2"><span className="truncate text-[9px] text-muted-foreground">{currentBatch ? `${currentBatch.completed}/${currentBatch.count} · موفق ${currentBatch.succeeded} · ناموفق ${currentBatch.failed}` : 'تست ارتباط واقعی'}</span><Button type="button" size="sm" variant="outline" className="h-7 shrink-0 px-2 text-[10px]" disabled={Boolean(batch) || busy.current} onClick={() => void testNode(node)}>{currentBatch ? <><RefreshCw size={11} className="animate-spin"/>{currentBatch.completed}/{currentBatch.count}</> : `تست ${probeCount.toLocaleString('fa-IR')} بار`}</Button></div>
              </div>
            })}
          </div>
        </div>
      </div>
      <div className="flex flex-wrap items-start gap-2 border-t border-border/70 px-4 py-3 text-[10px] leading-5 text-muted-foreground"><Activity size={13} className="mt-0.5 shrink-0 text-primary"/><p>تست دستی برای هر Node، تعداد درخواست واقعی HTTPS را به‌ترتیب می‌فرستد. دایرهٔ درخواست هنگام شروع فراخوانی حرکت می‌کند و پس از دریافت پاسخ، دایرهٔ پاسخ مسیر برگشت را طی می‌کند؛ مدت نمایش حداقل‌دار است تا نتیجه‌های سریع هم قابل‌دیدن باشند. حجم فقط از JSON پاسخ برآورد می‌شود و کل ترافیک شبکه نیست. سرورهای مادر از اتصال‌های ورودی این پنل نمایش داده می‌شوند.</p><span className="mr-auto inline-flex shrink-0 items-center gap-1"><ArrowDownToLine size={11}/>پاسخ‌ها: {totals.checks.toLocaleString('fa-IR')} · {payload(totals.bytes)}{totals.lastAt ? ` · ${new Date(totals.lastAt).toLocaleTimeString('fa-IR')}` : ''}</span></div>
    </CardContent>}
  </Card>
}
