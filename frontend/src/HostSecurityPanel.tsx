import { useState } from 'react'
import { CheckCircle2, CircleHelp, RefreshCw, ShieldCheck } from 'lucide-react'
import { api } from './lib/api'
import { Badge } from './components/ui/badge'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'

type Tool = { id: string; label: string; binary_available: boolean; path: string | null; active_state: string | null; version: string | null }
type Inventory = { tools: Tool[]; measured_at: number; inspection: string; note: string }

export function HostSecurityPanel() {
  const [data, setData] = useState<Inventory | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  async function inspect() {
    setLoading(true); setError('')
    try { setData(await api<Inventory>('security/posture')) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'بررسی ابزارهای امنیتی ناموفق بود.') }
    finally { setLoading(false) }
  }
  return <Card className="mt-5 rounded-2xl border-border/80 bg-card/80">
    <CardHeader className="flex-row flex-wrap items-center justify-between gap-3"><div><CardTitle className="flex items-center gap-2 text-base"><ShieldCheck size={17} className="text-primary"/>موجودی ابزارهای امنیتی</CardTitle><CardDescription className="mt-1.5">فقط وجود فایل اجرایی‌های شناخته‌شده را بررسی می‌کند؛ ابزار نصب یا اجرا نمی‌شود.</CardDescription></div><Button size="sm" variant="outline" onClick={() => void inspect()} disabled={loading}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>{data ? 'بررسی دوباره' : 'بررسی ابزارها'}</Button></CardHeader>
    <CardContent className="space-y-3">{error && <p role="alert" className="rounded-lg border border-rose-300/20 p-2 text-xs text-rose-200">{error}</p>}{!data && !loading && <p className="flex items-center gap-2 text-xs leading-5 text-muted-foreground"><CircleHelp size={14}/>وضعیت ابزارها هنوز بررسی نشده است.</p>}{loading && !data && <div className="h-12 animate-pulse rounded-lg bg-muted/30"/>}{data && <><div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{data.tools.map((tool) => <div key={tool.id} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-border/60 p-3"><span className="truncate text-xs">{tool.label}</span><Badge variant={tool.binary_available ? 'success' : 'outline'}>{tool.binary_available ? <><CheckCircle2 size={12}/>فایل موجود</> : 'یافت نشد'}</Badge></div>)}</div><p className="text-[10px] leading-5 text-muted-foreground">{data.note} زمان بررسی: {new Date(data.measured_at * 1000).toLocaleString('fa-IR')}.</p></>}</CardContent>
  </Card>
}
