import { useCallback, useEffect, useState } from 'react'
import { Layers3, LoaderCircle, Network, Rocket, Save, Trash2 } from 'lucide-react'
import { apiControlPlane } from './lib/api'
import { Button } from './components/ui/button'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Input } from './components/ui/input'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Node = { id: number; name: string; server_uuid?: string | null; connection_state?: string; last_seen?: number | null }
type App = { id: string; name: string; domain?: string | null; deleted_at?: number | null }
type GroupRow = { tag: string; node_id: number; name: string }
type Policy = { id: string; source_app_id: string; source_app_name: string; destination_server_uuid: string; destination_public_name?: string | null; destination_service: string; port: number; state: string; enforced: boolean }

export function FleetManagementPanels({ nodes, notify }: { nodes: Node[]; notify: (message: string) => void }) {
  const [tags, setTags] = useState<Record<number, string>>({})
  const [selected, setSelected] = useState<number[]>([])
  const [branch, setBranch] = useState<'main' | 'development'>('development')
  const [version, setVersion] = useState('')
  const [canary, setCanary] = useState('1')
  const [preview, setPreview] = useState<{ id: string; nodes: { id: number; name: string; connection_state: string; will_execute: boolean }[] } | null>(null)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [apps, setApps] = useState<App[]>([])
  const [policies, setPolicies] = useState<Policy[]>([])
  const [sourceApp, setSourceApp] = useState('')
  const [destination, setDestination] = useState('')
  const [service, setService] = useState('postgres-primary')
  const [port, setPort] = useState('5432')
  const [confirmed, setConfirmed] = useState(false)
  const [deletePolicy, setDeletePolicy] = useState<Policy | null>(null)

  const refresh = useCallback(async () => {
    setError('')
    try {
      const [groups, appResult, policyResult] = await Promise.all([
        apiControlPlane<{ nodes: GroupRow[] }>('fleet/groups'), apiControlPlane<{ apps: App[] }>('apps'), apiControlPlane<{ policies: Policy[] }>('private-network/policies'),
      ])
      const byNode: Record<number, string[]> = {}
      for (const row of groups.nodes || []) (byNode[row.node_id] ||= []).push(row.tag)
      setTags(Object.fromEntries(nodes.map((node) => [node.id, (byNode[node.id] || []).join(', ')])))
      setApps((appResult.apps || []).filter((app) => !app.deleted_at))
      setPolicies(policyResult.policies || [])
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'اطلاعات مدیریت ناوگان خوانده نشد.') }
  }, [nodes])
  useEffect(() => { void refresh() }, [refresh])

  async function saveTags(node: Node) {
    setBusy(`tags:${node.id}`); setError('')
    try {
      const result = await apiControlPlane<{ tags: string[] }>(`nodes/${node.id}/tags`, { method: 'PUT', body: JSON.stringify({ tags: (tags[node.id] || '').split(',').map((tag) => tag.trim()).filter(Boolean) }) })
      setTags((current) => ({ ...current, [node.id]: result.tags.join(', ') })); notify(`برچسب‌های ${node.name} ذخیره شد؛ هیچ تنظیمی در VPS مقصد تغییر نکرد.`); await refresh()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ذخیرهٔ برچسب‌ها انجام نشد.') }
    finally { setBusy('') }
  }
  async function savePreview() {
    setBusy('rollout'); setError('')
    try {
      const result = await apiControlPlane<{ id: string; nodes: { id: number; name: string; connection_state: string; will_execute: boolean }[] }>('fleet/rollouts/preview', { method: 'POST', body: JSON.stringify({ branch, version: version.trim(), node_ids: selected, canary_count: Number(canary), health_gate: true }) })
      setPreview(result); notify('پیش‌نمایش rollout ثبت شد؛ هیچ به‌روزرسانی روی Nodeها اجرا نشد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'پیش‌نمایش rollout ساخته نشد.') }
    finally { setBusy('') }
  }
  async function savePolicy() {
    setBusy('policy'); setError('')
    try {
      await apiControlPlane('private-network/policies', { method: 'POST', body: JSON.stringify({ source_app_id: sourceApp, destination_node_id: Number(destination), destination_service: service.trim(), port: Number(port), confirmed }) })
      notify('پیش‌نویس سیاست شبکه ثبت شد؛ هنوز هیچ ترافیکی تغییر نکرده است.'); setConfirmed(false); await refresh()
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ثبت پیش‌نویس شبکه انجام نشد.') }
    finally { setBusy('') }
  }
  async function removePolicy() {
    if (!deletePolicy) return
    setBusy('delete-policy'); setError('')
    try { await apiControlPlane(`private-network/policies/${encodeURIComponent(deletePolicy.id)}`, { method: 'DELETE' }); setDeletePolicy(null); notify('پیش‌نویس حذف شد؛ پیکربندی شبکه دست‌نخورده ماند.'); await refresh() }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف پیش‌نویس انجام نشد.') }
    finally { setBusy('') }
  }

  return <div className="mt-5 space-y-4">
    <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Layers3 size={17} className="text-primary"/>گروه‌بندی و انتشار ناوگان</CardTitle><CardDescription>برچسب‌ها و rolloutها در این نسخه فقط metadata و پیش‌نمایش هستند؛ update خودکار یا تغییر روی VPS مقصد انجام نمی‌شود.</CardDescription></CardHeader><CardContent className="space-y-4">
      {error && <p role="alert" className="rounded-md border border-rose-300/20 p-2 text-xs text-rose-200">{error}</p>}
      <div className="space-y-2">{nodes.map((node) => <div key={node.id} className="grid gap-2 rounded-lg border border-border/60 p-3 sm:grid-cols-[auto_minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-center"><input type="checkbox" aria-label={`انتخاب ${node.name} برای پیش‌نمایش`} checked={selected.includes(node.id)} onChange={(event) => setSelected((current) => event.target.checked ? [...current, node.id] : current.filter((id) => id !== node.id))}/><span className="truncate text-sm">{node.name} <Badge variant="outline">{node.connection_state || 'نامشخص'}</Badge></span><Input aria-label={`برچسب‌های ${node.name}`} value={tags[node.id] || ''} onChange={(event) => setTags((current) => ({ ...current, [node.id]: event.target.value }))} placeholder="production, eu-west"/><Button size="sm" variant="outline" onClick={() => void saveTags(node)} disabled={busy === `tags:${node.id}`}><Save size={13}/>ذخیرهٔ برچسب</Button></div>)}</div>
      <div className="grid gap-2 rounded-xl border border-primary/20 bg-primary/[.03] p-3 sm:grid-cols-4"><select value={branch} onChange={(event) => setBranch(event.target.value as 'main' | 'development')} className="h-10 rounded-md border border-input bg-background px-3 text-sm"><option value="development">development</option><option value="main">main</option></select><Input dir="ltr" value={version} onChange={(event) => setVersion(event.target.value)} placeholder="نسخهٔ دقیق، مثل 0.6.9"/><Input type="number" min="1" max={Math.max(1, selected.length)} value={canary} onChange={(event) => setCanary(event.target.value)} aria-label="تعداد Canary"/><Button onClick={() => void savePreview()} disabled={!!busy || !selected.length || !version.trim()}><Rocket size={14}/>ساخت پیش‌نمایش</Button><p className="sm:col-span-4 text-[11px] leading-5 text-muted-foreground">Health gate فقط در طرح ذخیره می‌شود. هیچ branch fetch، build، install یا service restart انجام نمی‌شود.</p></div>
      {preview && <div className="rounded-lg border border-amber-300/20 p-3 text-xs"><p>پیش‌نمایش <code dir="ltr">{preview.id.slice(0, 12)}</code> · به‌روزرسانی اجرا نشده</p><p className="mt-2">{preview.nodes.map((node) => node.name).join('، ')}</p></div>}
    </CardContent></Card>
    <Card><CardHeader><CardTitle className="flex items-center gap-2 text-base"><Network size={17} className="text-primary"/>پیش‌نویس ارتباط خصوصی برنامه‌ها</CardTitle><CardDescription>این فرم فقط قصد اتصال را ذخیره می‌کند؛ WireGuard، route و firewall در این نسخه پیکربندی نمی‌شوند.</CardDescription></CardHeader><CardContent className="space-y-3">
      <div className="grid gap-2 sm:grid-cols-2"><select value={sourceApp} onChange={(event) => setSourceApp(event.target.value)} className="h-10 rounded-md border border-input bg-background px-3 text-sm"><option value="">اپلیکیشن محلی</option>{apps.map((app) => <option key={app.id} value={app.id}>{app.name}</option>)}</select><select value={destination} onChange={(event) => setDestination(event.target.value)} className="h-10 rounded-md border border-input bg-background px-3 text-sm"><option value="">Node مقصد</option>{nodes.map((node) => <option key={node.id} value={node.id} disabled={!node.server_uuid}>{node.name}{node.server_uuid ? '' : ' · pairing ناقص'}</option>)}</select><Input dir="ltr" value={service} onChange={(event) => setService(event.target.value)} placeholder="postgres-primary"/><Input type="number" min="1" max="65535" dir="ltr" value={port} onChange={(event) => setPort(event.target.value)} placeholder="5432"/></div>
      <label className="flex items-start gap-2 text-xs leading-5"><input type="checkbox" checked={confirmed} onChange={(event) => setConfirmed(event.target.checked)}/>می‌دانم این فقط پیش‌نویس است و دسترسی شبکه ایجاد نمی‌کند.</label><Button size="sm" onClick={() => void savePolicy()} disabled={!!busy || !confirmed || !sourceApp || !destination}><Network size={14}/>ذخیرهٔ پیش‌نویس</Button>
      {policies.map((policy) => <div key={policy.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 p-3 text-xs"><span>{policy.source_app_name} → {policy.destination_public_name || policy.destination_server_uuid.slice(0, 8)} · {policy.destination_service}:{policy.port}/tcp</span><Badge variant="outline">{policy.enforced ? 'فعال' : 'منتظر Overlay'}</Badge><Button size="icon" variant="ghost" aria-label="حذف پیش‌نویس" onClick={() => setDeletePolicy(policy)}><Trash2 size={14}/></Button></div>)}
    </CardContent></Card>
    <Dialog open={!!deletePolicy} onOpenChange={(open) => { if (!open && !busy) setDeletePolicy(null) }}><DialogContent className="sm:max-w-md"><DialogHeader><DialogTitle>حذف پیش‌نویس شبکه؟</DialogTitle><DialogDescription>این رکورد حذف می‌شود. هیچ route یا قانون فایروالی تغییر نمی‌کند.</DialogDescription></DialogHeader><div className="flex justify-end gap-2"><Button variant="outline" onClick={() => setDeletePolicy(null)} disabled={!!busy}>انصراف</Button><Button variant="destructive" onClick={() => void removePolicy()} disabled={!!busy}>{busy === 'delete-policy' && <LoaderCircle size={14} className="animate-spin"/>}حذف پیش‌نویس</Button></div></DialogContent></Dialog>
  </div>
}
