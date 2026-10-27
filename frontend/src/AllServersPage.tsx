import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Activity, ArrowDownToLine, CircleAlert, CircleCheck, LockKeyhole, RefreshCw, Server, ShieldCheck, Waypoints } from 'lucide-react'
import { api, type Dashboard } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Node = { id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; endpoint: string; last_seen: number | null; workspace_ready?: boolean }
type Status = { hostname: string; public_name?: string; os: string; architecture: string; arg_version: string; uptime_seconds: number; load: string; memory_total_kib: number; memory_available_kib: number }
type ServerRow = { key: string; name: string; endpoint: string; local: boolean; node?: Node; status?: Status; error?: string; lastSeen?: number | null; responseBytes?: number }
type ProbePulse = { id: number; nodeId: number; bytes: number; at: number; direction: 'request' | 'response' }

const CARD_WIDTH = 236
const CARD_GAP = 16
const CARD_TOP = 236
const LINK_COLOR = '#53c992'

function formatBytes(kib: number) {
  const gib = kib / 1024 / 1024
  return `${gib.toLocaleString('fa-IR', { maximumFractionDigits: 1 })} گیبی‌بایت`
}

function formatPayloadBytes(bytes: number) {
  if (bytes < 1024) return `${bytes.toLocaleString('fa-IR')} بایت`
  return `${(bytes / 1024).toLocaleString('fa-IR', { maximumFractionDigits: 1 })} کیبی‌بایت`
}

export function AllServersPage({ Heading: PageHeading, onOpen }: { Heading: Heading; onOpen: (workspaceKey: string) => void }) {
  const [rows, setRows] = useState<ServerRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [refreshing, setRefreshing] = useState(false)
  const [pulses, setPulses] = useState<ProbePulse[]>([])
  const [trafficTotals, setTrafficTotals] = useState({ responses: 0, bytes: 0, lastAt: 0 })
  const pulseId = useRef(0)
  const loadInProgress = useRef(false)

  const load = useCallback(async (quiet = false) => {
    if (loadInProgress.current) return
    loadInProgress.current = true
    if (!quiet) setRefreshing(true)
    setError('')
    try {
      const [registry, local] = await Promise.all([
        api<{ server_uuid: string; public_name: string; nodes: Node[] }>('nodes'),
        api<Dashboard>('dashboard'),
      ])
      const nodes = registry.nodes || []
      const remotes = new Array<ServerRow>(nodes.length)
      const received: number[] = []
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(8, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          const requestPulseId = ++pulseId.current
          setPulses((current) => [...current, { id: requestPulseId, nodeId: node.id, bytes: 0, at: Date.now(), direction: 'request' as const }].slice(-80))
          try {
            const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(7000) })
            const responseBytes = new TextEncoder().encode(JSON.stringify(status)).byteLength
            remotes[index] = { key: `${node.workspace_key}:${node.id}`, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, local: false, node, status, lastSeen: node.last_seen, responseBytes }
            received.push(responseBytes)
            setPulses((current) => current.map((pulse) => pulse.id === requestPulseId ? { ...pulse, bytes: responseBytes, at: Date.now(), direction: 'response' } : pulse))
          } catch (cause) {
            setPulses((current) => current.filter((pulse) => pulse.id !== requestPulseId))
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

  useEffect(() => { void load() }, [load])
  useEffect(() => {
    if (!rows.some((row) => row.node)) return
    const timer = window.setInterval(() => { if (document.visibilityState === 'visible') void load(true) }, 15000)
    return () => window.clearInterval(timer)
  }, [load, rows.length])
  useEffect(() => {
    const timer = window.setInterval(() => setPulses((current) => current.filter((pulse) => Date.now() - pulse.at < (pulse.direction === 'request' ? 8000 : 2200))), 500)
    return () => window.clearInterval(timer)
  }, [])

  const onlineCount = rows.filter((row) => row.status).length
  const workers = rows.filter((row) => row.node)
  const graphWidth = Math.max(760, workers.length * CARD_WIDTH + Math.max(0, workers.length - 1) * CARD_GAP + 48)
  const graphCenter = graphWidth / 2
  const cardsWidth = workers.length * CARD_WIDTH + Math.max(0, workers.length - 1) * CARD_GAP
  const firstCardX = (graphWidth - cardsWidth) / 2
  const getCardCenter = (index: number) => firstCardX + index * (CARD_WIDTH + CARD_GAP) + CARD_WIDTH / 2
  const action = <Button variant="outline" size="sm" onClick={() => void load()} disabled={refreshing}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی وضعیت</Button>

  return <>
    <PageHeading title="همهٔ سرورها" description="نمای خواندنی وضعیت سرورهای متصل؛ برای انجام عملیات، ابتدا فضای‌کار همان سرور را انتخاب کن." action={action}/>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
    <Card className="mb-5 overflow-hidden border-border/80 bg-card/45">
      <CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 p-4 sm:p-5">
        <div><CardTitle className="flex items-center gap-2 text-base"><Waypoints size={18} className="text-primary"/>نقشهٔ ارتباط پنل‌ها</CardTitle><CardDescription className="mt-1">هر ۱۵ ثانیه ارتباط امن ARG با Nodeها بررسی می‌شود؛ حرکت هر نقطه پس از دریافت پاسخ واقعی همان سرور آغاز می‌شود.</CardDescription></div>
        <div className="flex flex-wrap gap-2 text-[10px]"><span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2.5 py-1.5 text-emerald-100"><span className="h-1.5 w-1.5 rounded-full bg-emerald-300"/>پاسخ‌گو: {onlineCount}/{rows.length}</span><span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-background/40 px-2.5 py-1.5 text-muted-foreground"><ArrowDownToLine size={12}/>پاسخ دریافت‌شده: {trafficTotals.responses.toLocaleString('fa-IR')} · {formatPayloadBytes(trafficTotals.bytes)}</span></div>
      </CardHeader>
      {loading ? <CardContent className="p-4 pt-0"><Skeleton className="h-72 rounded-xl"/></CardContent> : workers.length === 0 ? <CardContent className="p-5 pt-0"><div className="flex min-h-36 flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border text-center"><LockKeyhole size={22} className="text-muted-foreground"/><p className="text-sm font-medium">هنوز Node راه‌دوری متصل نیست</p><p className="text-xs text-muted-foreground">پس از جفت‌سازی سرور، ارتباط آن در این نقشه نمایش داده می‌شود.</p></div></CardContent> : <CardContent className="p-0">
        <div className="overflow-x-auto px-3 pb-3 sm:px-4" dir="ltr">
          <div className="relative mx-auto" style={{ width: graphWidth, height: 430 }}>
            <svg className="pointer-events-none absolute inset-0 z-0" width={graphWidth} height="430" viewBox={`0 0 ${graphWidth} 430`} role="img" aria-label="نمودار ارتباط رمزگذاری‌شده بین پنل کنترل و Nodeهای ARG">
              <defs><filter id="topology-glow"><feGaussianBlur stdDeviation="3" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter></defs>
              {workers.map((row, index) => {
                const nodeId = row.node!.id
                const x = getCardCenter(index)
                const linkPath = `M ${graphCenter} 100 L ${graphCenter} 163 L ${x} 163 L ${x} ${CARD_TOP}`
                const reversePath = `M ${x} ${CARD_TOP} L ${x} 163 L ${graphCenter} 163 L ${graphCenter} 100`
                const isOnline = Boolean(row.status)
                return <g key={nodeId}>
                  <path d={linkPath} fill="none" stroke={isOnline ? LINK_COLOR : '#707786'} strokeOpacity={isOnline ? 0.55 : 0.38} strokeWidth="2" strokeDasharray={isOnline ? undefined : '5 7'}/>
                  {pulses.filter((pulse) => pulse.nodeId === nodeId).map((pulse) => <circle key={`${pulse.id}:${pulse.direction}`} className="arg-topology-packet" r="5" fill={pulse.direction === 'request' ? '#dda46f' : '#8ef0bf'} filter="url(#topology-glow)"><title>{pulse.direction === 'request' ? 'درخواست بررسی ارتباط' : `پاسخ Node · ${formatPayloadBytes(pulse.bytes)}`}</title><animateMotion dur="1.1s" fill="freeze" path={pulse.direction === 'request' ? linkPath : reversePath}/></circle>)}
                  <circle cx={x} cy={CARD_TOP} r="4" fill={isOnline ? LINK_COLOR : '#858b98'}/>
                </g>
              })}
            </svg>
            <div className="absolute inset-x-6 top-3 z-10">
              <div dir="rtl" className="flex min-h-[78px] items-center gap-3 rounded-2xl border border-primary/20 bg-card/95 px-4 py-3 shadow-lg shadow-black/10">
                <span className="grid h-11 w-11 shrink-0 place-items-center rounded-xl border border-primary/25 bg-primary/[.08] text-primary"><ShieldCheck size={21}/></span>
                <div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><p className="font-semibold">ARG Control Plane</p><span className="rounded-full border border-emerald-400/20 bg-emerald-400/[.06] px-2 py-0.5 text-[10px] text-emerald-100">این پنل · آنلاین</span></div><p className="mt-1 truncate text-xs text-muted-foreground">{rows.find((row) => row.local)?.name || 'سرور فعلی'} · هماهنگی امن Nodeها و مدیریت فضای‌کار</p></div>
                <span className="hidden items-center gap-1.5 text-[10px] text-muted-foreground sm:flex"><LockKeyhole size={13}/>TLS</span>
              </div>
            </div>
            <div className="absolute inset-x-0 z-10 flex" style={{ top: CARD_TOP, gap: CARD_GAP, paddingLeft: firstCardX }}>
              {workers.map((row) => {
                const node = row.node!
                const age = node.last_seen ? Math.max(0, Math.floor(Date.now() / 1000 - node.last_seen)) : null
                return <button key={row.key} type="button" dir="rtl" className="group flex h-[172px] w-[236px] shrink-0 flex-col rounded-2xl border border-border/80 bg-card/95 p-3 text-right shadow-lg shadow-black/10 transition hover:-translate-y-0.5 hover:border-primary/40 hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary" onClick={() => onOpen(node.workspace_key)} disabled={!node.workspace_ready} aria-label={`${row.name}؛ ${node.workspace_ready ? 'ورود به فضای کار VPS' : 'اتصال کامل آماده نیست'}`}>
                  <div className="flex w-full items-start justify-between gap-2"><span className="grid h-9 w-9 shrink-0 place-items-center rounded-xl border border-primary/20 bg-primary/[.06] text-primary"><Server size={17}/></span><span className={`inline-flex items-center gap-1 rounded-full border px-2 py-1 text-[10px] ${row.status ? 'border-emerald-400/20 bg-emerald-400/[.05] text-emerald-100' : 'border-amber-400/20 bg-amber-400/[.05] text-amber-100'}`}>{row.status ? <CircleCheck size={11} /> : <CircleAlert size={11} />}{row.status ? 'متصل' : 'قطع'}</span></div>
                  <span className="mt-2 block w-full truncate text-sm font-semibold">{row.name}</span>
                  <span dir="ltr" className="mt-0.5 block w-full truncate text-left font-mono text-[10px] text-muted-foreground">{row.endpoint}</span>
                  <span className="mt-2 flex w-full items-center justify-between gap-2 border-t border-border/70 pt-2 text-[10px] text-muted-foreground"><span>{row.status ? `ARG v${row.status.arg_version}` : age === null ? 'هنوز ارتباط ثبت نشده' : `آخرین پاسخ ${age.toLocaleString('fa-IR')} ثانیه پیش`}</span><span>{row.responseBytes ? `≈ ${formatPayloadBytes(row.responseBytes)}` : '—'}</span></span>
                  <span className="mt-auto flex w-full items-center justify-between pt-2 text-[10px] text-primary">{node.workspace_ready ? 'ورود به پنل سرور' : 'نیاز به جفت‌سازی'}<span aria-hidden="true">←</span></span>
                </button>
              })}
            </div>
          </div>
        </div>
        <div className="flex flex-wrap items-start gap-2 border-t border-border/70 px-4 py-3 text-[10px] leading-5 text-muted-foreground"><Activity size={13} className="mt-0.5 shrink-0 text-primary"/><p>نقطهٔ متحرک، پاسخ واقعی بررسی وضعیت Node را از سرور راه‌دور به این Control Plane نشان می‌دهد؛ اندازهٔ تقریبی از payload JSON پاسخ محاسبه می‌شود و سربار TLS/HTTP را شامل نمی‌شود. این نما ترافیک عمومی برنامه‌ها یا تمام بایت‌های شبکهٔ VPS را گزارش نمی‌کند.</p></div>
      </CardContent>}
    </Card>
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
  </>
}
