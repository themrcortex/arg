import { useCallback, useEffect, useState } from 'react'
import { Archive, Cloud, Database, Network, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Badge } from './components/ui/badge'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type Props = { app: { id: string; name: string; framework: string; source_type: string; listen_port: number | null }; notify: (message: string) => void }
type NetworkRecord = { id: string; name: string; driver: 'docker-bridge' | 'host-local'; visibility: string; docker_network: string | null; member_count: number }
type Member = { application_id: string; application_name: string; container_name: string; alias: string; internal_port: number | null }
type Volume = { id: string; name: string; description: string; relative_path: string; quota_bytes: number | null; usage_bytes: number | null; usage_status: string }
type Binding = { id: string; bucket_id: number; bucket_name: string; bucket_display_name: string; connection_name: string; provider: string; prefix: string; purpose: string; access_mode: string; mounted_path: string | null }
type BucketOption = { id: number; bucket_name: string; display_name: string; enabled: boolean; connection_name: string; provider: string; connection_status: string }
type PendingDelete = { id: string; name: string } | null

export function ApplicationResourcesPanel({ app, notify }: Props) {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [networks, setNetworks] = useState<NetworkRecord[]>([])
  const [members, setMembers] = useState<Record<string, Member[]>>({})
  const [volumes, setVolumes] = useState<Volume[]>([])
  const [bindings, setBindings] = useState<Binding[]>([])
  const [bucketOptions, setBucketOptions] = useState<BucketOption[]>([])
  const [networkName, setNetworkName] = useState('')
  const [networkDriver, setNetworkDriver] = useState<'docker-bridge' | 'host-local'>('host-local')
  const [selectedNetwork, setSelectedNetwork] = useState('')
  const [alias, setAlias] = useState('')
  const [containerName, setContainerName] = useState('')
  const [internalPort, setInternalPort] = useState(app.listen_port ? String(app.listen_port) : '')
  const [volumeName, setVolumeName] = useState('')
  const [volumeDescription, setVolumeDescription] = useState('')
  const [bucketId, setBucketId] = useState('')
  const [prefix, setPrefix] = useState('')
  const [purpose, setPurpose] = useState('object-storage')
  const [pendingDelete, setPendingDelete] = useState<PendingDelete>(null)
  const [purgeData, setPurgeData] = useState(false)

  const load = useCallback(async () => {
    setBusy(true); setError('')
    try {
      const volumeRequest = ['node','next','nuxt','auto'].includes(app.framework) ? api<{ volumes: Volume[] }>(`apps/${encodeURIComponent(app.id)}/volumes`) : Promise.resolve({ volumes: [] as Volume[] })
      const [networkResult, volumeResult, bindingResult, bucketCatalog] = await Promise.all([
        api<{ networks: NetworkRecord[] }>('app-networks'),
        volumeRequest,
        api<{ bindings: Binding[] }>(`apps/${encodeURIComponent(app.id)}/s3-bindings`),
        api<{ buckets: BucketOption[] }>('s3/buckets/catalog'),
      ])
      setNetworks(networkResult.networks); setVolumes(volumeResult.volumes); setBindings(bindingResult.bindings)
      const memberRows = await Promise.all(networkResult.networks.map(async (network) => {
        const result = await api<{ members: Member[] }>(`app-networks/${encodeURIComponent(network.id)}/members`)
        return [network.id, result.members] as const
      }))
      setMembers(Object.fromEntries(memberRows))
      const enabledBuckets = bucketCatalog.buckets.filter((bucket) => bucket.enabled)
      setBucketOptions(enabledBuckets)
      setSelectedNetwork((current) => current || networkResult.networks[0]?.id || '')
      setBucketId((current) => current || String(enabledBuckets[0]?.id || ''))
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'منابع اپلیکیشن بارگذاری نشد.') }
    finally { setBusy(false) }
  }, [app.id])

  useEffect(() => { if (open) void load() }, [open, load])
  async function run(action: () => Promise<void>, success: string) {
    setBusy(true); setError('')
    try { await action(); notify(success); await load() }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'عملیات ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <section className="mt-3 rounded-xl border border-border/70 bg-background/25 p-3">
    <div className="flex items-center justify-between gap-2"><div><p className="flex items-center gap-2 text-xs font-medium"><Database size={14} className="text-primary"/>فضای پایدار، شبکه و S3</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">حجم‌های پایدار محلی، endpointهای داخلی و اتصال‌های S3 همین اپ را از اینجا ببین.</p></div><div className="flex gap-1"><Button size="sm" variant="outline" onClick={() => setOpen((value) => !value)}>{open ? 'بستن' : 'مدیریت'}</Button>{open && <Button size="icon" variant="outline" aria-label="تازه‌سازی منابع اپ" disabled={busy} onClick={() => void load()}><RefreshCw size={13} className={busy ? 'animate-spin' : ''}/></Button>}</div></div>
    {open && <div className="mt-3 space-y-4 rounded-lg border border-border/70 bg-background/40 p-3">
      {error && <p role="alert" className="rounded-md border border-rose-300/20 bg-rose-300/5 p-2 text-xs text-rose-100">{error}</p>}
      <section className="space-y-2"><div className="flex items-center gap-2 text-xs font-medium"><Network size={13}/>شبکه و Service Discovery</div><div className="grid gap-2 sm:grid-cols-[1fr_150px_auto]"><Input aria-label="نام شبکه" value={networkName} onChange={(event) => setNetworkName(event.target.value)} placeholder="نام شبکه"/><select className="h-10 rounded-md border border-input bg-background px-2 text-xs" value={networkDriver} onChange={(event) => setNetworkDriver(event.target.value as typeof networkDriver)}><option value="host-local">Loopback داخلی</option><option value="docker-bridge">Docker private bridge</option></select><Button size="sm" disabled={busy || !networkName.trim()} onClick={() => void run(async () => { await api('app-networks', { method: 'POST', body: JSON.stringify({ name: networkName.trim(), driver: networkDriver, visibility: 'private' }) }); setNetworkName('') }, 'شبکه ثبت شد.') }><Plus size={13}/>ساخت</Button></div><p className="text-[10px] leading-5 text-muted-foreground">Docker bridge با `--internal` ساخته می‌شود؛ اتصال کانتینر فقط با برچسب مالکیت ARG ممکن است. Loopback فقط آدرس 127.0.0.1 را ثبت می‌کند و جداسازی شبکه ایجاد نمی‌کند.</p>
        {networks.map((network) => <div key={network.id} className="rounded-md border border-border/60 p-2"><div className="flex flex-wrap items-center justify-between gap-2"><div className="flex items-center gap-2"><span className="text-xs">{network.name}</span><Badge variant="outline">{network.driver === 'docker-bridge' ? 'Docker خصوصی' : 'Loopback'}</Badge><span className="text-[10px] text-muted-foreground">{network.member_count} عضو</span></div><Button size="sm" variant="ghost" disabled={busy || network.member_count > 0} onClick={() => void run(async () => { await api(`app-networks/${encodeURIComponent(network.id)}?confirm=true`, { method: 'DELETE' }) }, 'شبکه حذف شد.') }><Trash2 size={12}/></Button></div>
          <div className="mt-2 flex flex-wrap gap-2">{(members[network.id] || []).map((member) => <span key={member.application_id} className="rounded bg-muted/30 px-2 py-1 text-[10px]">{member.application_name} · {member.alias}{member.internal_port ? ` → 127.0.0.1:${member.internal_port}` : ''}<button className="mr-2 text-rose-200" aria-label={`جداسازی ${member.application_name}`} onClick={() => void run(async () => { await api(`app-networks/${encodeURIComponent(network.id)}/members/${encodeURIComponent(member.application_id)}`, { method: 'DELETE' }) }, 'اپ از شبکه جدا شد.')}>×</button></span>)}</div>
        </div>)}
        {networks.length > 0 && <div className="grid gap-2 sm:grid-cols-4"><select aria-label="شبکهٔ مقصد" className="h-10 rounded-md border border-input bg-background px-2 text-xs" value={selectedNetwork} onChange={(event) => setSelectedNetwork(event.target.value)}>{networks.map((network) => <option key={network.id} value={network.id}>{network.name}</option>)}</select><Input aria-label="نام مستعار سرویس" value={alias} onChange={(event) => setAlias(event.target.value)} placeholder="service-alias"/>{networks.find((item) => item.id === selectedNetwork)?.driver === 'docker-bridge' ? <Input aria-label="نام کانتینر ARG" value={containerName} onChange={(event) => setContainerName(event.target.value)} placeholder="نام کانتینر برچسب‌خورده"/> : <Input aria-label="پورت داخلی" inputMode="numeric" value={internalPort} onChange={(event) => setInternalPort(event.target.value)} placeholder="پورت داخلی"/>}<Button size="sm" disabled={busy || !selectedNetwork || !alias.trim()} onClick={() => void run(async () => { await api(`app-networks/${encodeURIComponent(selectedNetwork)}/members`, { method: 'POST', body: JSON.stringify({ application_id: app.id, alias: alias.trim(), container_name: containerName.trim(), internal_port: internalPort ? Number(internalPort) : null }) }); setAlias(''); setContainerName('') }, 'عضویت شبکه ثبت شد.')}>اتصال اپ</Button></div>}
      </section>

      {['node','next','nuxt','auto'].includes(app.framework) && <section className="space-y-2 border-t border-border/60 pt-3"><div className="flex items-center gap-2 text-xs font-medium"><Archive size={13}/>فضای پایدار محلی</div><p className="text-[10px] leading-5 text-muted-foreground">مسیرها زیر `/shared/data/volumes` می‌مانند و با release عوض نمی‌شوند. در Node از `/shared/data/volumes/نام` قابل دسترسی‌اند. ثبت سقف اختیاری اطلاع‌رسانی است؛ quota سیستم‌فایلی اعمال نمی‌شود.</p><div className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]"><Input aria-label="نام volume" value={volumeName} onChange={(event) => setVolumeName(event.target.value)} placeholder="uploads"/><Input aria-label="توضیح volume" value={volumeDescription} onChange={(event) => setVolumeDescription(event.target.value)} placeholder="فایل‌های آپلودی"/><Button size="sm" disabled={busy || !volumeName.trim()} onClick={() => void run(async () => { await api(`apps/${encodeURIComponent(app.id)}/volumes`, { method: 'POST', body: JSON.stringify({ name: volumeName.trim(), description: volumeDescription }) }); setVolumeName(''); setVolumeDescription('') }, 'فضای پایدار ساخته شد.') }><Plus size={13}/>ساخت</Button></div>
        {volumes.map((volume) => <div key={volume.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/60 p-2 text-[10px]"><div className="min-w-0"><p className="font-medium">{volume.name}{volume.usage_bytes !== null && ` · ${formatBytes(volume.usage_bytes)}`}</p><p dir="ltr" className="truncate text-left font-mono text-muted-foreground">{volume.relative_path}</p></div><div className="flex gap-1"><Button size="sm" variant="outline" disabled={busy} onClick={() => void run(async () => { await api(`app-volumes/${encodeURIComponent(volume.id)}/scan`, { method: 'POST' }) }, 'اندازه‌گیری فضای volume انجام شد.')}>اندازه‌گیری</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => void run(async () => { await api(`app-volumes/${encodeURIComponent(volume.id)}`, { method: 'DELETE' }) }, 'پیوند مدیریتی جدا شد؛ داده باقی ماند.')}>جداکردن</Button><Button size="sm" variant="ghost" className="text-rose-200" disabled={busy} onClick={() => { setPendingDelete({ id: volume.id, name: volume.name }); setPurgeData(false) }}>حذف داده</Button></div></div>)}
      </section>}

      <section className="space-y-2 border-t border-border/60 pt-3"><div className="flex items-center gap-2 text-xs font-medium"><Cloud size={13}/>اتصال S3 برای اپ</div><p className="text-[10px] leading-5 text-amber-100/80">ARG فقط Bucket و prefix را ثبت می‌کند؛ secret به اپ صادر نمی‌شود. دسترسی را با credential محدود به prefix در سرویس‌دهنده تنظیم کن. سطح Read/Write انتخابی در این نسخه metadata است و مجوز Provider را تغییر نمی‌دهد.</p><div className="grid gap-2 sm:grid-cols-4"><select aria-label="Bucket" className="h-10 rounded-md border border-input bg-background px-2 text-xs" value={bucketId} onChange={(event) => setBucketId(event.target.value)}>{bucketOptions.map((bucket) => <option key={bucket.id} value={bucket.id}>{bucket.connection_name} / {bucket.display_name} ({bucket.bucket_name})</option>)}</select><Input aria-label="Prefix" dir="ltr" value={prefix} onChange={(event) => setPrefix(event.target.value)} placeholder="assets/"/><select aria-label="کاربرد S3" className="h-10 rounded-md border border-input bg-background px-2 text-xs" value={purpose} onChange={(event) => setPurpose(event.target.value)}><option value="object-storage">فایل‌های برنامه</option><option value="backup-target">مقصد پشتیبان</option><option value="artifact-cache">Artifact</option><option value="asset-storage">دارایی‌ها</option><option value="rclone-mount">مسیر Mount شده</option></select><Button size="sm" disabled={busy || !bucketId || !bucketOptions.length} onClick={() => void run(async () => { await api(`apps/${encodeURIComponent(app.id)}/s3-bindings`, { method: 'POST', body: JSON.stringify({ bucket_id: Number(bucketId), prefix, purpose, access_mode: 'read-only' }) }); setPrefix('') }, 'اتصال S3 برای اپ ثبت شد.') }><Plus size={13}/>ثبت اتصال</Button></div>
        {bindings.map((binding) => <div key={binding.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/60 p-2 text-[10px]"><span>{binding.connection_name} / {binding.bucket_display_name} · <code dir="ltr">{binding.prefix || '/'}</code> · {binding.purpose}{binding.mounted_path && <code dir="ltr" className="mr-2 text-muted-foreground">{binding.mounted_path}</code>}</span><Button size="sm" variant="ghost" disabled={busy} onClick={() => void run(async () => { await api(`app-s3-bindings/${encodeURIComponent(binding.id)}`, { method: 'DELETE' }) }, 'اتصال S3 از اپ جدا شد؛ Bucket دست‌نخورده ماند.')}>قطع اتصال</Button></div>)}
        {!bucketOptions.length && <p className="text-[10px] text-muted-foreground">Bucket فعالی در S3 ثبت نشده است.</p>}
      </section>
    </div>}
    <Dialog open={!!pendingDelete} onOpenChange={(value) => { if (!value && !busy) setPendingDelete(null) }}><DialogContent className="max-w-md"><DialogHeader><DialogTitle>حذف دادهٔ volume؟</DialogTitle><DialogDescription>«{pendingDelete?.name}» روی دیسک همین سرور قرار دارد. جداکردن فقط رکورد را حذف می‌کند؛ پاک‌سازی محتوا برگشت‌ناپذیر است.</DialogDescription></DialogHeader><label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={purgeData} onChange={(event) => setPurgeData(event.target.checked)} className="mt-0.5 accent-[#dda46f]"/>محتوای این پوشه را هم برای همیشه پاک کن.</label><div className="flex justify-end gap-2"><Button variant="outline" disabled={busy} onClick={() => setPendingDelete(null)}>انصراف</Button><Button variant="destructive" disabled={busy || !purgeData || !pendingDelete} onClick={() => { if (pendingDelete) void run(async () => { await api(`app-volumes/${encodeURIComponent(pendingDelete.id)}/purge`, { method: 'POST', body: JSON.stringify({ confirmed: true }) }); setPendingDelete(null) }, 'دادهٔ volume پاک شد.') }}>{busy ? 'در حال حذف…' : 'حذف دائمی داده'}</Button></div></DialogContent></Dialog>
  </section>
}

function formatBytes(value: number) { if (!Number.isFinite(value) || value < 0) return 'نامشخص'; const units = ['B','KiB','MiB','GiB','TiB']; let amount = value; let unit = 0; while (amount >= 1024 && unit < units.length - 1) { amount /= 1024; unit++ } return `${amount.toFixed(unit ? 1 : 0)} ${units[unit]}` }
