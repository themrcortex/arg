import { useCallback, useEffect, useState } from 'react'
import { Activity, Database, RefreshCw } from 'lucide-react'
import { ApiError, api } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'

type Status = { installed: boolean; version: string | null; service_state: string; readiness: string; management_mode: string; note: string }
export function PostgresInventoryPanel() {
  const [status, setStatus] = useState<Status | null>(null)
  const [error, setError] = useState('')
  const [unsupported, setUnsupported] = useState(false)
  const [loading, setLoading] = useState(false)
  const refresh = useCallback(async () => {
    setLoading(true); setError(''); setUnsupported(false)
    try { setStatus(await api<Status>('postgres/status')) }
    catch (cause) {
      if (cause instanceof ApiError && cause.status === 404) { setStatus(null); setUnsupported(true) }
      else setError(cause instanceof Error ? cause.message : 'وضعیت PostgreSQL خوانده نشد.')
    }
    finally { setLoading(false) }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  return <Card className="mt-5 border-border/70"><CardHeader className="flex-row items-start justify-between gap-3"><div><CardTitle className="flex items-center gap-2 text-base"><Database size={17} className="text-primary"/>PostgreSQL</CardTitle><CardDescription>وضعیت محلی سرویس و آمادگی اتصال؛ این بخش فعلاً فقط موجودی‌خوان است.</CardDescription></div><Button size="sm" variant="outline" onClick={() => void refresh()} disabled={loading}><RefreshCw size={14} className={loading ? 'animate-spin' : ''}/>بررسی</Button></CardHeader><CardContent>{unsupported ? <div role="status" className="rounded-lg border border-amber-300/20 bg-amber-300/[.04] p-3 text-xs leading-5 text-amber-100">این Node نسخه‌ای از ARG دارد که API موجودی PostgreSQL را ارائه نمی‌کند (HTTP 404). PostgreSQL اختیاری است و این مورد روی مدیریت MySQL اثر ندارد؛ برای نمایش وضعیت PostgreSQL، Node را به‌روزرسانی و دوباره بررسی کن.</div> : error ? <p role="alert" className="text-sm text-rose-200">{error}</p> : status && <><div className="grid gap-2 sm:grid-cols-3"><div className="rounded-lg border border-border/70 p-3 text-sm">نصب: <Badge variant={status.installed ? 'success' : 'outline'}>{status.installed ? 'بله' : 'خیر'}</Badge></div><div className="rounded-lg border border-border/70 p-3 text-sm"><Activity size={14} className="ml-1 inline text-primary"/>سرویس: <span dir="ltr" className="font-mono">{status.service_state}</span></div><div className="rounded-lg border border-border/70 p-3 text-sm">آمادگی: <span dir="ltr" className="font-mono">{status.readiness}</span></div></div><p dir="ltr" className="mt-2 text-xs text-muted-foreground">{status.version || 'psql version unavailable'}</p><p className="mt-3 text-xs leading-6 text-muted-foreground">{status.note}</p></>}</CardContent></Card>
}
