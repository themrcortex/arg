import { useCallback, useEffect, useRef, useState } from 'react'
import {
  Activity, ArrowLeft, ArrowUpRight, Clock3, Info, LockKeyhole,
  Network, RefreshCw, Shield, Users,
} from 'lucide-react'
import { api, formatBytes, formatNumber, type AuditEvent, type Dashboard, type Firewall, type Interface, type Port, type User } from '../lib/api'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/card'
import { Skeleton } from '../components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../components/ui/table'
import type { ExplorerEntry } from './catalog'

type Stat = { number: number; value: string; unit: string; percent?: number; note: string }
type Segment = { label: string; value: number; color?: string }

function toStat(number: number, unit: string, note: string, percent?: number): Stat {
  const value = unit === 'bytes' ? formatBytes(number) : unit === 'bytes/s' ? `${formatBytes(number)}/s` : unit === '%' ? `${formatNumber(Math.round(number * 10) / 10)}٪` :
    unit === 'mbps' ? `${formatNumber(number)} Mbps` : formatNumber(Math.round(number * 10) / 10)
  return { number, value, unit, note, percent }
}

function systemStat(key: string, data: Dashboard, cpu: number | null): Stat {
  const totalMem = (data.memory_kib || 0) * 1024
  const availableMem = (data.available_kib || 0) * 1024
  const swap = (data.swap_total_kib || 0) * 1024
  const swapFree = (data.swap_free_kib || 0) * 1024
  const disk = data.disk_total_bytes || 0
  const diskFree = data.disk_free_bytes || 0
  const detail = data.memory_details || {}
  const load = data.load.split(' ').map((part) => Number(part) || 0)
  const p = (used: number, total: number) => total > 0 ? (used / total) * 100 : 0
  const memory: Record<string, number> = {
    'mem-total': totalMem, 'mem-used': totalMem - availableMem, 'mem-available': availableMem,
    'mem-free': (detail.free_kib || 0) * 1024, 'mem-buffers': (detail.buffers_kib || 0) * 1024,
    'mem-cached': (detail.cached_kib || 0) * 1024, 'mem-active': (detail.active_kib || 0) * 1024,
    'mem-inactive': (detail.inactive_kib || 0) * 1024, 'mem-dirty': (detail.dirty_kib || 0) * 1024,
    'mem-slab': (detail.slab_kib || 0) * 1024,
    'swap-total': swap, 'swap-free': swapFree, 'swap-used': swap - swapFree,
    'disk-total': disk, 'disk-free': diskFree, 'disk-used': disk - diskFree,
    'process-ram': data.process_rss_bytes,
  }
  if (key in memory) {
    const scope = key.startsWith('mem') ? totalMem : key.startsWith('swap') ? swap : key.startsWith('disk') ? disk : totalMem
    return toStat(Math.max(0, memory[key]), 'bytes', 'مقدار فعلی خوانده‌شده از همین سرور', p(memory[key], scope))
  }
  if (key === 'cpu-use') return toStat(cpu ?? 0, '%', cpu === null ? 'پس از دومین نمونه‌برداری محاسبه می‌شود' : 'میانگین مصرف همهٔ هسته‌ها', cpu ?? 0)
  if (key === 'cpu-idle') return toStat(cpu === null ? 0 : 100 - cpu, '%', 'سهم زمان بیکار پردازنده', cpu === null ? 0 : 100 - cpu)
  if (key === 'cpu-cores') return toStat(data.cpu_cores || 1, 'count', 'هسته‌های در دسترس فرایند Donatello')
  if (key.startsWith('load-')) return toStat(load[{ 'load-1': 0, 'load-5': 1, 'load-15': 2 }[key] || 0] || 0, 'count', 'Load average گزارش‌شده توسط لینوکس')
  if (key === 'uptime-days') return toStat(data.uptime_seconds / 86400, 'count', 'از آخرین راه‌اندازی سرور')
  if (key === 'uptime-hours') return toStat(data.uptime_seconds / 3600, 'count', 'از آخرین راه‌اندازی سرور')
  if (key === 'uptime-minutes') return toStat(data.uptime_seconds / 60, 'count', 'از آخرین راه‌اندازی سرور')
  if (key === 'mem-percent') return toStat(p(totalMem - availableMem, totalMem), '%', 'بر اساس MemAvailable', p(totalMem - availableMem, totalMem))
  if (key === 'swap-percent') return toStat(p(swap - swapFree, swap), '%', 'درصد Swap مصرف‌شده', p(swap - swapFree, swap))
  if (key === 'disk-percent') return toStat(p(disk - diskFree, disk), '%', 'فایل‌سیستم ریشه /', p(disk - diskFree, disk))
  return toStat(data.process_cpu_percent, '%', 'مصرف CPU خود Donatello', data.process_cpu_percent)
}

function networkStat(key: string, data: Dashboard, rate: { rx: number; tx: number }): Stat {
  const interfaces = data.interfaces
  const sum = (field: keyof Interface) => interfaces.reduce((total, item) => total + (Number(item[field]) || 0), 0)
  const addresses = interfaces.flatMap((item) => item.addresses || [])
  const numbers: Record<string, [number, string, string]> = {
    interfaces: [interfaces.length, 'count', 'همهٔ رابط‌های ثبت‌شده در /sys/class/net'],
    'interfaces-up': [interfaces.filter((item) => item.state === 'up').length, 'count', 'رابط‌هایی با وضعیت up'],
    'interfaces-down': [interfaces.filter((item) => item.state !== 'up').length, 'count', 'رابط‌هایی که وضعیت up ندارند'],
    carrier: [interfaces.filter((item) => item.carrier).length, 'count', 'رابط‌هایی با carrier فعال'],
    ipv4: [addresses.filter((address) => !address.includes(':')).length, 'count', 'آدرس‌های IPv4 شناسایی‌شده'],
    ipv6: [addresses.filter((address) => address.includes(':')).length, 'count', 'آدرس‌های IPv6 شناسایی‌شده'],
    mac: [interfaces.filter((item) => item.mac && item.mac !== '00:00:00:00:00:00').length, 'count', 'MACهای معتبر گزارش‌شده'],
    'mtu-min': [Math.min(...interfaces.map((item) => item.mtu || Infinity)), 'count', 'کمترین MTU گزارش‌شده'],
    'mtu-max': [Math.max(0, ...interfaces.map((item) => item.mtu || 0)), 'count', 'بیشترین MTU گزارش‌شده'],
    'speed-max': [Math.max(0, ...interfaces.map((item) => item.speed_mbps || 0)), 'mbps', 'فقط رابط‌هایی که سرعت لینک را گزارش می‌کنند'],
    'rx-total': [sum('rx_bytes'), 'bytes', 'شمارندهٔ تجمعی دریافت همهٔ رابط‌ها'],
    'tx-total': [sum('tx_bytes'), 'bytes', 'شمارندهٔ تجمعی ارسال همهٔ رابط‌ها'],
    'bytes-total': [sum('rx_bytes') + sum('tx_bytes'), 'bytes', 'جمع شمارنده‌های RX و TX'],
    'rx-rate': [rate.rx, 'bytes/s', 'بایت دریافت‌شده در هر ثانیه، از اختلاف دو نمونه'],
    'tx-rate': [rate.tx, 'bytes/s', 'بایت ارسال‌شده در هر ثانیه، از اختلاف دو نمونه'],
    'rx-packets': [sum('rx_packets'), 'count', 'شمارندهٔ بسته‌های ورودی'],
    'tx-packets': [sum('tx_packets'), 'count', 'شمارندهٔ بسته‌های خروجی'],
    'rx-errors': [sum('rx_errors'), 'count', 'خطاهای دریافت از زمان راه‌اندازی رابط'],
    'tx-errors': [sum('tx_errors'), 'count', 'خطاهای ارسال از زمان راه‌اندازی رابط'],
    'rx-dropped': [sum('rx_dropped'), 'count', 'بسته‌های ورودی حذف‌شده'],
    'tx-dropped': [sum('tx_dropped'), 'count', 'بسته‌های خروجی حذف‌شده'],
    'top-interface': [Math.max(0, ...interfaces.map((item) => item.rx_bytes + item.tx_bytes)), 'bytes',
      `بیشترین ترافیک: ${[...interfaces].sort((a, b) => (b.rx_bytes + b.tx_bytes) - (a.rx_bytes + a.tx_bytes))[0]?.name || 'نامشخص'}`],
  }
  const [raw, unit, note] = numbers[key] || [0, 'count', 'آمار رابط‌های شبکه']
  const number = Number.isFinite(raw) ? raw : 0
  return toStat(number, unit, note)
}

function filterPorts(ports: Port[], key: string): Port[] {
  if (key.startsWith('port-')) return ports.filter((item) => item.port === Number(key.slice(5)))
  const groups: Record<string, number[]> = {
    web: [80, 443, 3000, 8000, 8080, 8443], remote: [22, 23, 3389, 5900],
    mail: [25, 110, 143, 465, 587, 993, 995], database: [1433, 1521, 3306, 5432, 6379], dns: [53],
  }
  return ports.filter((item) => {
    if (key === 'all') return true
    if (key === 'tcp') return item.protocol === 'TCP'
    if (key === 'udp') return item.protocol === 'UDP'
    if (key === 'ipv4') return !item.ipv6
    if (key === 'ipv6') return item.ipv6
    if (key === 'tcp4') return item.protocol === 'TCP' && !item.ipv6
    if (key === 'tcp6') return item.protocol === 'TCP' && item.ipv6
    if (key === 'udp4') return item.protocol === 'UDP' && !item.ipv6
    if (key === 'udp6') return item.protocol === 'UDP' && item.ipv6
    if (key === 'low') return item.port < 1024
    if (key === 'high') return item.port >= 1024
    return groups[key]?.includes(item.port) || false
  })
}

function firewallRules(text: string): string[] {
  return text.split('\n').filter((line) => /^\s*\[\s*\d+\]/.test(line))
}
function filterRules(rules: string[], key: string): string[] {
  if (key === 'all' || key === 'status') return rules
  if (key.startsWith('port-')) {
    const port = key.slice(5)
    return rules.filter((line) => new RegExp(`(^|[^0-9])${port}([^0-9]|$)`).test(line))
  }
  if (key === 'ipv4') return rules.filter((line) => !line.includes('(v6)'))
  if (key === 'ipv6') return rules.filter((line) => line.includes('(v6)'))
  if (key === 'ssh') return rules.filter((line) => /\b22\b|\bssh\b/i.test(line))
  if (key === 'web') return rules.filter((line) => /\b80\b|\b443\b|\b8080\b|\b8443\b|\bhttp\b/i.test(line))
  return rules.filter((line) => new RegExp(`\\b${key}\\b`, 'i').test(line))
}

function filterUsers(users: User[], key: string, actor: User): User[] {
  if (key === 'all') return users
  if (key === 'mine') return users.filter((item) => item.id === actor.id)
  if (key === 'active') return users.filter((item) => item.enabled)
  if (key === 'disabled') return users.filter((item) => !item.enabled)
  if (key.endsWith('-active')) return users.filter((item) => item.role === key.replace('-active', '') && item.enabled)
  return users.filter((item) => item.role === key)
}

function filterAudit(events: AuditEvent[], key: string, actor: User): AuditEvent[] {
  const current = Math.floor(Date.now() / 1000)
  if (key === 'all') return events
  if (key === 'mine') return events.filter((item) => item.actor === actor.username)
  if (key === 'hour') return events.filter((item) => item.at >= current - 3600)
  if (key === 'today') return events.filter((item) => new Date(item.at * 1000).toDateString() === new Date().toDateString())
  if (key === 'week') return events.filter((item) => item.at >= current - 7 * 86400)
  return events.filter((item) => item.action === key)
}

function TrendChart({ values, unit }: { values: number[]; unit: string }) {
  if (values.length < 2) return <div className="grid h-44 place-items-center text-sm text-muted-foreground">نمودار پس از دومین نمونه‌برداری نمایش داده می‌شود.</div>
  const max = Math.max(...values, 1)
  const min = Math.min(...values, 0)
  const range = Math.max(max - min, 1)
  const points = values.map((value, index) => `${12 + index * 476 / Math.max(values.length - 1, 1)},${170 - (value - min) / range * 140}`).join(' ')
  return (
    <div className="h-44" dir="ltr">
      <svg viewBox="0 0 500 190" preserveAspectRatio="none" width="100%" height="100%" role="img" aria-label={`روند ${values.length} نمونهٔ اخیر، آخرین مقدار ${Math.round(values[values.length - 1])} ${unit}`}>
        <defs><linearGradient id="insightFill" x1="0" y1="0" x2="0" y2="1"><stop stopColor="#a987ff" stopOpacity=".34"/><stop offset="1" stopColor="#a987ff" stopOpacity="0"/></linearGradient></defs>
        {[30, 75, 120, 170].map((y) => <line key={y} x1="12" x2="488" y1={y} y2={y} stroke="#3a3450" strokeDasharray="2 7" />)}
        <polygon points={`12,170 ${points} 488,170`} fill="url(#insightFill)" />
        <polyline points={points} fill="none" stroke="#a987ff" strokeWidth="2.5" vectorEffect="non-scaling-stroke" />
      </svg>
    </div>
  )
}

function Distribution({ items }: { items: Segment[] }) {
  const max = Math.max(...items.map((item) => item.value), 1)
  return (
    <div className="space-y-4">
      {items.map((item, index) => (
        <div key={`${item.label}-${index}`}>
          <div className="mb-1.5 flex items-center justify-between gap-3 text-xs"><span className="truncate text-muted-foreground">{item.label}</span><b className="font-mono text-foreground">{formatNumber(item.value)}</b></div>
          <div className="h-2 overflow-hidden rounded-full bg-white/5"><div className="h-full rounded-full transition-all duration-500" style={{ width: `${Math.max(item.value ? 3 : 0, item.value / max * 100)}%`, background: item.color || (index % 2 ? '#59d3bc' : '#a987ff') }} /></div>
        </div>
      ))}
    </div>
  )
}

export function ExplorerPage({ item, user, onNavigate, notify }: {
  item: ExplorerEntry
  user: User
  onNavigate: (id: string) => void
  notify: (message: string) => void
}) {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null)
  const [firewall, setFirewall] = useState<Firewall | null>(null)
  const [users, setUsers] = useState<User[]>([])
  const [events, setEvents] = useState<AuditEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [cpu, setCpu] = useState<number | null>(null)
  const [rate, setRate] = useState({ rx: 0, tx: 0 })
  const [trend, setTrend] = useState<number[]>([])
  const previous = useRef<{ total: number; idle: number; rx: number; tx: number; at: number } | null>(null)
  const valueRef = useRef(item.value)
  valueRef.current = item.value

  const refresh = useCallback(async () => {
    try {
      if (['system', 'network', 'ports'].includes(item.kind)) {
        const next = await api<Dashboard>('dashboard')
        const now = Date.now()
        const rx = next.interfaces.reduce((sum, device) => sum + device.rx_bytes, 0)
        const tx = next.interfaces.reduce((sum, device) => sum + device.tx_bytes, 0)
        const old = previous.current
        if (old) {
          const total = (next.cpu_total || 0) - old.total
          const idle = (next.cpu_idle || 0) - old.idle
          const nextCpu = total > 0 ? Math.max(0, Math.min(100, (1 - idle / total) * 100)) : 0
          const seconds = Math.max((now - old.at) / 1000, 0.1)
          const nextRate = { rx: Math.max(0, (rx - old.rx) / seconds), tx: Math.max(0, (tx - old.tx) / seconds) }
          setCpu(nextCpu)
          setRate(nextRate)
          const number = item.kind === 'system' ? systemStat(valueRef.current, next, nextCpu).number :
            item.kind === 'network' ? networkStat(valueRef.current, next, nextRate).number :
            filterPorts(next.ports, valueRef.current).length
          setTrend((values) => [...values, number].slice(-24))
        } else {
          const number = item.kind === 'system' ? systemStat(valueRef.current, next, null).number :
            item.kind === 'network' ? networkStat(valueRef.current, next, { rx: 0, tx: 0 }).number :
            filterPorts(next.ports, valueRef.current).length
          setTrend([number])
        }
        previous.current = { total: next.cpu_total || 0, idle: next.cpu_idle || 0, rx, tx, at: now }
        setDashboard(next)
      } else if (item.kind === 'firewall') setFirewall(await api<Firewall>('firewall'))
      else if (item.kind === 'users') setUsers((await api<{ users: User[] }>('users')).users)
      else if (item.kind === 'audit') setEvents((await api<{ events: AuditEvent[] }>('audit')).events)
    } catch (error) {
      notify(error instanceof Error ? error.message : 'دریافت اطلاعات ناموفق بود')
    } finally {
      setLoading(false)
    }
  }, [item.kind, notify])

  useEffect(() => {
    void refresh()
    if (!['system', 'network', 'ports'].includes(item.kind)) return
    const timer = window.setInterval(refresh, 5000)
    return () => window.clearInterval(timer)
  }, [item.kind, refresh])

  const stat = dashboard && (item.kind === 'system' ? systemStat(item.value, dashboard, cpu) : item.kind === 'network' ? networkStat(item.value, dashboard, rate) : null)
  const matchingPorts = dashboard && item.kind === 'ports' ? filterPorts(dashboard.ports, item.value) : []
  const allRules = firewallRules(firewall?.status || '')
  const matchingRules = item.kind === 'firewall' ? filterRules(allRules, item.value) : []
  const matchingUsers = item.kind === 'users' ? filterUsers(users, item.value, user) : []
  const matchingEvents = item.kind === 'audit' ? filterAudit(events, item.value, user) : []

  const related = item.kind === 'system' ? 'dashboard' : item.kind === 'ports' || item.kind === 'network' ? 'network' : item.kind
  const relatedLabel = item.kind === 'system' ? 'داشبورد' : item.kind === 'ports' || item.kind === 'network' ? 'نمای شبکه' : item.kind === 'firewall' ? 'فایروال' : item.kind === 'users' ? 'مدیریت کاربران' : item.kind === 'audit' ? 'گزارش کامل' : 'تنظیمات'
  return (
    <div className="space-y-5">
      <section className="insight-hero dot-grid relative overflow-hidden rounded-[28px] border border-white/10 p-6 md:p-8">
        <div className="relative z-10 flex flex-wrap items-start justify-between gap-5">
          <div>
            <div className="mb-5 flex items-center gap-2 text-[11px] font-bold tracking-[.12em] text-[#a6eddf]" dir="ltr"><span className="h-1.5 w-1.5 rounded-full bg-[#71e4cf] shadow-[0_0_12px_#71e4cf]" /> LIVE INSIGHT / {item.kind.toUpperCase()}</div>
            <h1 className="text-3xl font-extrabold text-white md:text-4xl">{item.label}</h1>
            <p className="mt-3 max-w-2xl text-sm leading-7 text-[#aaa8bd]">{item.description || 'نمای تخصصی بر اساس داده‌های واقعی سرور؛ برای تازه‌سازی از دکمهٔ به‌روزرسانی استفاده کنید.'}</p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" onClick={() => void refresh()}><RefreshCw size={15} /> به‌روزرسانی</Button>
            <Button variant="outline" size="sm" onClick={() => onNavigate(related)}><ArrowLeft size={15} /> {relatedLabel}</Button>
          </div>
        </div>
      </section>

      {loading ? <div className="grid gap-4 md:grid-cols-2"><Skeleton className="h-60"/><Skeleton className="h-60"/></div> : null}

      {!loading && stat && (
        <div className="grid gap-5 xl:grid-cols-[.75fr_1.25fr]">
          <Card className="metric-card"><CardContent className="p-6 md:p-8">
            <div className="flex items-center justify-between text-xs text-muted-foreground"><span>مقدار فعلی</span><Activity size={18} className="text-[#a6eddf]"/></div>
            <div className="mt-7 break-all text-4xl font-extrabold text-white md:text-5xl" dir={stat.unit === 'bytes' || stat.unit === 'bytes/s' || stat.unit === 'mbps' ? 'ltr' : 'rtl'}>{stat.value}</div>
            <p className="mt-4 text-sm leading-7 text-muted-foreground">{stat.note}</p>
            {stat.percent !== undefined && <div className="mt-6 h-2 overflow-hidden rounded-full bg-white/5"><div className="h-full rounded-full bg-gradient-to-r from-[#5adfc9] to-[#af83ff]" style={{ width: `${Math.min(100, Math.max(0, stat.percent))}%` }}/></div>}
          </CardContent></Card>
          <Card><CardHeader><CardTitle className="flex items-center gap-2"><Activity size={18} className="text-primary"/> روند زنده</CardTitle><p className="text-xs text-muted-foreground">نمونه‌های این صفحه هر ۵ ثانیه ثبت می‌شوند؛ تاریخچه با بستن صفحه پاک می‌شود.</p></CardHeader><CardContent><TrendChart values={trend} unit={stat.unit}/></CardContent></Card>
        </div>
      )}

      {!loading && item.kind === 'network' && dashboard && (
        <Card><CardHeader><CardTitle className="flex items-center gap-2"><Network size={18} className="text-primary"/> رابط‌های شبکه</CardTitle></CardHeader><CardContent><Distribution items={dashboard.interfaces.map((device) => ({ label: `${device.name} · RX ${formatBytes(device.rx_bytes)} · TX ${formatBytes(device.tx_bytes)}`, value: device.rx_bytes + device.tx_bytes }))}/><div className="mt-6 grid gap-3 md:grid-cols-2 xl:grid-cols-3">{dashboard.interfaces.map((device) => <div key={device.name} className="rounded-xl border border-white/10 bg-white/[.025] p-4"><div className="flex justify-between gap-2"><b className="font-mono text-sm">{device.name}</b><Badge variant={device.state === 'up' ? 'success' : 'outline'}>{device.state}</Badge></div><p className="mt-2 break-all font-mono text-xs text-muted-foreground">{device.addresses?.join(' · ') || device.mac}</p></div>)}</div></CardContent></Card>
      )}

      {!loading && item.kind === 'ports' && dashboard && (
        <div className="grid gap-5 xl:grid-cols-[.75fr_1.25fr]">
          <Card className="metric-card"><CardContent className="p-7"><div className="flex items-center gap-2 text-xs text-muted-foreground"><Activity size={16} className="text-primary"/> نتیجهٔ بررسی محلی</div><div className="mt-6 text-5xl font-extrabold text-white">{formatNumber(matchingPorts.length)}</div><p className="mt-3 text-sm text-muted-foreground">{item.value.startsWith('port-') ? matchingPorts.length ? 'در حال شنود روی این سرور' : 'شنونده‌ای برای این پورت پیدا نشد' : 'پورت مطابق فیلتر'}</p><p className="mt-6 text-xs leading-6 text-muted-foreground">شنود محلی به‌معنی قابل‌دسترس بودن از اینترنت نیست؛ فایروال و شبکهٔ ارائه‌دهنده را جداگانه بررسی کنید.</p></CardContent></Card>
          <Card><CardHeader><CardTitle>ترکیب پروتکل و IP</CardTitle></CardHeader><CardContent><Distribution items={[
            { label: 'TCP / IPv4', value: matchingPorts.filter((port) => port.protocol === 'TCP' && !port.ipv6).length },
            { label: 'TCP / IPv6', value: matchingPorts.filter((port) => port.protocol === 'TCP' && port.ipv6).length },
            { label: 'UDP / IPv4', value: matchingPorts.filter((port) => port.protocol === 'UDP' && !port.ipv6).length },
            { label: 'UDP / IPv6', value: matchingPorts.filter((port) => port.protocol === 'UDP' && port.ipv6).length },
          ]}/></CardContent></Card>
          <Card className="xl:col-span-2"><CardHeader><CardTitle>پورت‌های مطابق فیلتر</CardTitle></CardHeader><CardContent>{matchingPorts.length ? <Table><TableHeader><TableRow><TableHead>پورت</TableHead><TableHead>پروتکل</TableHead><TableHead>نسخهٔ IP</TableHead></TableRow></TableHeader><TableBody>{matchingPorts.map((port, index) => <TableRow key={`${port.port}-${port.protocol}-${index}`}><TableCell className="font-mono text-primary">{port.port}</TableCell><TableCell>{port.protocol}</TableCell><TableCell>IPv{port.ipv6 ? '6' : '4'}</TableCell></TableRow>)}</TableBody></Table> : <p className="py-8 text-center text-sm text-muted-foreground">موردی پیدا نشد.</p>}</CardContent></Card>
        </div>
      )}

      {!loading && item.kind === 'firewall' && (
        <div className="grid gap-5 xl:grid-cols-[.7fr_1.3fr]">
          <Card className="metric-card"><CardContent className="p-7"><Shield className="text-primary"/><div className="mt-6 text-5xl font-extrabold">{formatNumber(matchingRules.length)}</div><p className="mt-3 text-sm text-muted-foreground">قانون مطابق فیلتر · از {formatNumber(allRules.length)} قانون</p><Badge className="mt-6" variant={firewall?.available ? 'success' : 'warning'}>{firewall?.available ? 'UFW در دسترس' : 'UFW در دسترس نیست'}</Badge></CardContent></Card>
          <Card><CardHeader><CardTitle>ترکیب قوانین UFW</CardTitle></CardHeader><CardContent><Distribution items={['ALLOW', 'DENY', 'REJECT', 'LIMIT'].map((action) => ({ label: action, value: allRules.filter((line) => line.includes(action)).length }))}/></CardContent></Card>
          <Card className="xl:col-span-2"><CardHeader><CardTitle>خروجی قوانین مطابق فیلتر</CardTitle></CardHeader><CardContent><pre className="max-h-[480px] overflow-auto whitespace-pre-wrap break-words rounded-xl border border-white/10 bg-[#090b13] p-5 text-left font-mono text-xs leading-7 text-[#cbc9dc]" dir="ltr">{item.value === 'status' ? firewall?.status : matchingRules.join('\n') || 'No matching UFW rule found.'}{firewall?.error && `\n${firewall.error}`}</pre><p className="mt-4 text-xs text-muted-foreground">این نما فقط وضعیت UFW را می‌خواند و قانون جدید اعمال نمی‌کند.</p></CardContent></Card>
        </div>
      )}

      {!loading && item.kind === 'users' && (
        <div className="grid gap-5 xl:grid-cols-[.7fr_1.3fr]">
          <Card className="metric-card"><CardContent className="p-7"><Users className="text-primary"/><div className="mt-6 text-5xl font-extrabold">{formatNumber(matchingUsers.length)}</div><p className="mt-3 text-sm text-muted-foreground">حساب مطابق فیلتر · از {formatNumber(users.length)} حساب</p></CardContent></Card>
          <Card><CardHeader><CardTitle>ترکیب نقش‌ها</CardTitle></CardHeader><CardContent><Distribution items={['admin', 'operator', 'viewer'].map((role) => ({ label: role, value: matchingUsers.filter((account) => account.role === role).length }))}/></CardContent></Card>
          <Card className="xl:col-span-2"><CardHeader><CardTitle>حساب‌های مطابق فیلتر</CardTitle></CardHeader><CardContent>{matchingUsers.length ? <Table><TableHeader><TableRow><TableHead>نام کاربری</TableHead><TableHead>نقش</TableHead><TableHead>وضعیت</TableHead></TableRow></TableHeader><TableBody>{matchingUsers.map((account) => <TableRow key={account.id}><TableCell className="font-semibold">{account.username}</TableCell><TableCell>{account.role}</TableCell><TableCell><Badge variant={account.enabled ? 'success' : 'warning'}>{account.enabled ? 'فعال' : 'غیرفعال'}</Badge></TableCell></TableRow>)}</TableBody></Table> : <p className="py-8 text-center text-sm text-muted-foreground">حسابی پیدا نشد.</p>}</CardContent></Card>
        </div>
      )}

      {!loading && item.kind === 'audit' && (
        <div className="grid gap-5 xl:grid-cols-[.7fr_1.3fr]">
          <Card className="metric-card"><CardContent className="p-7"><Activity className="text-primary"/><div className="mt-6 text-5xl font-extrabold">{formatNumber(matchingEvents.length)}</div><p className="mt-3 text-sm text-muted-foreground">رویداد مطابق فیلتر · فقط در ۱۰۰ رویداد اخیر</p></CardContent></Card>
          <Card><CardHeader><CardTitle>ترکیب عملیات</CardTitle></CardHeader><CardContent><Distribution items={['login', 'logout', 'account.password', 'user.create', 'user.update'].map((action) => ({ label: action, value: matchingEvents.filter((event) => event.action === action).length }))}/></CardContent></Card>
          <Card className="xl:col-span-2"><CardHeader><CardTitle>رویدادهای مطابق فیلتر</CardTitle></CardHeader><CardContent>{matchingEvents.length ? <Table><TableHeader><TableRow><TableHead>زمان</TableHead><TableHead>کاربر</TableHead><TableHead>عملیات</TableHead><TableHead>جزئیات</TableHead></TableRow></TableHeader><TableBody>{matchingEvents.map((event, index) => <TableRow key={`${event.at}-${index}`}><TableCell className="whitespace-nowrap">{new Date(event.at * 1000).toLocaleString('fa-IR')}</TableCell><TableCell>{event.actor}</TableCell><TableCell className="font-mono text-xs">{event.action}</TableCell><TableCell>{event.detail}</TableCell></TableRow>)}</TableBody></Table> : <p className="py-8 text-center text-sm text-muted-foreground">رویدادی پیدا نشد.</p>}</CardContent></Card>
        </div>
      )}

      {!loading && item.kind === 'settings' && (
        <Card><CardContent className="p-7 md:p-9">
          <div className="grid gap-6 md:grid-cols-[68px_1fr]"><div className="grid h-16 w-16 place-items-center rounded-2xl border border-primary/30 bg-primary/10 text-primary">{item.value === 'password' || item.value === 'session' ? <LockKeyhole size={27}/> : item.value === 'monitoring' ? <Clock3 size={27}/> : <Network size={27}/>}</div><div><h2 className="text-xl font-bold">{item.label}</h2><p className="mt-3 text-sm leading-8 text-muted-foreground">{item.value === 'endpoint' ? `آدرس فعلی پنل ${location.origin} است. برای تغییر پورت سرویس، از منوی ترمینال با دستور sudo donatello استفاده کنید.` : item.value === 'session' ? 'نشست ورود با کوکی HttpOnly و SameSite=Strict محافظت می‌شود. مدت اعتبار نشست ۲۴ ساعت است.' : item.value === 'monitoring' ? 'داشبورد و نماهای زنده هر ۵ ثانیه دادهٔ تازه را از همین سرور دریافت می‌کنند.' : 'تغییر گذرواژه از صفحهٔ تنظیمات انجام می‌شود. برای بازنشانی رمز بدون رمز فعلی، از sudo donatello password در ترمینال سرور استفاده کنید.'}</p><Button className="mt-6" onClick={() => onNavigate('settings')}>رفتن به تنظیمات <ArrowUpRight size={16}/></Button></div></div>
        </CardContent></Card>
      )}

      <div className="flex items-start gap-3 rounded-2xl border border-white/10 bg-white/[.025] p-4 text-xs leading-6 text-muted-foreground"><Info size={17} className="mt-0.5 shrink-0 text-[#a6eddf]"/> داده‌ها از API همین سرور خوانده می‌شوند. نمودارهای زندهٔ این صفحه فقط نمونه‌های همین نشست مرورگر را نشان می‌دهند.</div>
    </div>
  )
}
