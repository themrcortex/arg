import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Activity, CircleAlert, CircleCheck, RefreshCw, Server } from 'lucide-react'
import { api, type Dashboard } from './lib/api'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Skeleton } from './components/ui/skeleton'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type Node = { id: number; workspace_key: string; name: string; public_name?: string; server_uuid?: string | null; endpoint: string; last_seen: number | null; workspace_ready?: boolean }
type Status = { hostname: string; public_name?: string; os: string; architecture: string; arg_version: string; uptime_seconds: number; load: string; memory_total_kib: number; memory_available_kib: number }
type ServerRow = { key: string; name: string; endpoint: string; local: boolean; node?: Node; status?: Status; error?: string; lastSeen?: number | null }

function formatBytes(kib: number) {
  const gib = kib / 1024 / 1024
  return `${gib.toLocaleString('fa-IR', { maximumFractionDigits: 1 })} گیبی‌بایت`
}

export function AllServersPage({ Heading: PageHeading, onOpen }: { Heading: Heading; onOpen: (workspaceKey: string) => void }) {
  const [rows, setRows] = useState<ServerRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [refreshing, setRefreshing] = useState(false)

  const load = useCallback(async () => {
    setRefreshing(true)
    setError('')
    try {
      const [registry, local] = await Promise.all([
        api<{ server_uuid: string; public_name: string; nodes: Node[] }>('nodes'),
        api<Dashboard>('dashboard'),
      ])
      const nodes = registry.nodes || []
      const remotes = new Array<ServerRow>(nodes.length)
      let cursor = 0
      await Promise.all(Array.from({ length: Math.min(8, nodes.length) }, async () => {
        while (cursor < nodes.length) {
          const index = cursor++
          const node = nodes[index]
          try {
            const status = await api<Status>(`nodes/${node.id}/status`, { signal: AbortSignal.timeout(7000) })
            remotes[index] = { key: `${node.workspace_key}:${node.id}`, name: status.public_name || node.public_name || node.name, endpoint: status.hostname || node.endpoint, local: false, node, status, lastSeen: node.last_seen }
          } catch (cause) {
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
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'فهرست سرورها خوانده نشد.')
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [])

  useEffect(() => { void load() }, [load])

  const onlineCount = rows.filter((row) => row.status).length
  const action = <Button variant="outline" size="sm" onClick={() => void load()} disabled={refreshing}><RefreshCw size={14} className={refreshing ? 'animate-spin' : ''}/>تازه‌سازی وضعیت</Button>

  return <>
    <PageHeading title="همهٔ سرورها" description="نمای خواندنی وضعیت سرورهای متصل؛ برای انجام عملیات، ابتدا فضای‌کار همان سرور را انتخاب کن." action={action}/>
    {error && <p role="alert" className="mb-4 rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-sm text-rose-200">{error}</p>}
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
