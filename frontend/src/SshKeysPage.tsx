import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { Check, Copy, ExternalLink, KeyRound, Plus, RefreshCw, ShieldCheck, Trash2, Pencil } from 'lucide-react'
import { ApiError, api, getActiveWorkspaceNode } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Badge } from './components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'
import { useConfirmDialog } from './hooks/useConfirmDialog'

type Heading = (props: { title: string; description: string; action?: ReactNode }) => ReactNode
type SshKeyRecord = {
  id: string
  name: string
  purpose: 'personal' | 'deploy'
  repository: string | null
  application_id: string | null
  public_key: string
  fingerprint: string
  created_at: number
  last_used_at: number | null
}
type HostKeys = { provider: string; updated_at: number | null; fresh_for_seconds: number }
type CreatedKey = Pick<SshKeyRecord, 'id' | 'name' | 'purpose' | 'repository' | 'public_key' | 'fingerprint'> & { application_id?: string | null; private_key_saved_on_server: boolean }

function dateLabel(value: number | null) {
  if (!value) return 'هنوز استفاده نشده'
  return new Date(value * 1000).toLocaleString('fa-IR')
}

export function SshKeysPage({ Heading: PageHeading, notify }: { Heading: Heading; notify: (message: string) => void }) {
  const [keys, setKeys] = useState<SshKeyRecord[]>([])
  const [hostKeys, setHostKeys] = useState<HostKeys | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [createOpen, setCreateOpen] = useState(false)
  const [purpose, setPurpose] = useState<'personal' | 'deploy'>('deploy')
  const [name, setName] = useState('')
  const [repository, setRepository] = useState('')
  const [created, setCreated] = useState<CreatedKey | null>(null)
  const [renameTarget, setRenameTarget] = useState<SshKeyRecord | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const { confirm, confirmDialog } = useConfirmDialog()
  const activeNode = getActiveWorkspaceNode()
  const allServers = activeNode === 'all'

  const load = useCallback(async () => {
    if (allServers) { setLoading(false); return }
    setLoading(true); setError('')
    try {
      const [keyResult, hostResult] = await Promise.all([
        api<{ keys: SshKeyRecord[]; github_host_keys_updated_at: number | null }>('ssh-keys'),
        api<HostKeys>('ssh-keys/trusted-hosts/github'),
      ])
      setKeys(keyResult.keys)
      setHostKeys({ ...hostResult, updated_at: keyResult.github_host_keys_updated_at ?? hostResult.updated_at })
    } catch (cause) {
      setError(cause instanceof ApiError && cause.status === 404 && activeNode
        ? 'این Node نسخه‌ای را اجرا می‌کند که مدیریت کلید SSH را ندارد؛ ابتدا آن Node را از صفحهٔ «Nodeهای ARG» به‌روزرسانی کن.'
        : cause instanceof Error ? cause.message : 'کلیدهای SSH خوانده نشدند.')
    }
    finally { setLoading(false) }
  }, [allServers, activeNode])

  useEffect(() => { void load() }, [load])

  async function createKey(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      const payload = { name: name.trim(), purpose, ...(purpose === 'deploy' ? { repository: repository.trim() } : {}) }
      const result = await api<CreatedKey>('ssh-keys', { method: 'POST', body: JSON.stringify(payload) })
      setCreated(result)
      setName(''); setRepository(''); setPurpose('deploy')
      setCreateOpen(false)
      await load()
      notify('کلید ساخته و به‌صورت خصوصی روی همین VPS ذخیره شد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'ساخت کلید SSH ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function copy(value: string, label: string) {
    try { await navigator.clipboard.writeText(value); notify(`${label} کپی شد.`) }
    catch { setError(`کپی ${label} در این مرورگر مجاز نیست؛ متن را انتخاب و دستی کپی کن.`) }
  }

  async function renameKey(event: React.FormEvent) {
    event.preventDefault()
    if (!renameTarget) return
    setBusy(true); setError('')
    try {
      await api(`ssh-keys/${encodeURIComponent(renameTarget.id)}`, { method: 'PUT', body: JSON.stringify({ name: renameValue.trim() }) })
      setRenameTarget(null); await load(); notify('نام کلید تغییر کرد.')
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'تغییر نام ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function deleteKey(key: SshKeyRecord) {
    if (!await confirm({ title: 'حذف کلید SSH', description: `کلید «${key.name}» و فایل خصوصی آن از این VPS پاک می‌شود. برای لغو دسترسی GitHub، کلید عمومی را جداگانه از تنظیمات مخزن یا حساب GitHub حذف کن.`, confirmLabel: 'حذف کلید', destructive: true })) return
    setBusy(true); setError('')
    try { await api(`ssh-keys/${encodeURIComponent(key.id)}`, { method: 'DELETE' }); await load(); notify('کلید از ARG و این VPS حذف شد.') }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'حذف کلید ناموفق بود.') }
    finally { setBusy(false) }
  }

  async function refreshHostKeys() {
    setBusy(true); setError('')
    try { const result = await api<HostKeys>('ssh-keys/trusted-hosts/github', { method: 'POST' }); setHostKeys(result); notify('کلیدهای میزبان GitHub از HTTPS دریافت و بررسی شدند.') }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'به‌روزرسانی کلید میزبان GitHub ناموفق بود.') }
    finally { setBusy(false) }
  }

  return <>
    <PageHeading title="کلیدهای SSH و Deploy Key" description="کلیدهای دسترسی مخزن را برای فضای‌کار VPS فعال مدیریت کن؛ کلید خصوصی فقط روی همان VPS می‌ماند." action={<div className="flex gap-2"><Button variant="outline" onClick={() => void load()} disabled={loading || busy}><RefreshCw size={14}/>تازه‌سازی</Button><Button onClick={() => { setCreated(null); setError(''); setCreateOpen(true) }} disabled={allServers}><Plus size={15}/>ساخت کلید</Button></div>} />
    {allServers ? <Card><CardContent className="p-5 text-sm leading-7 text-muted-foreground">مدیریت کلیدها به یک VPS مشخص نیاز دارد. از انتخاب‌گر بالای پنل، «این VPS» یا یک Node را انتخاب کن؛ نمای «همهٔ سرورها» فقط برای فهرست‌کردن اطلاعات استفاده می‌شود.</CardContent></Card> : <div className="space-y-4">
      <Card className="border-primary/15 bg-primary/[.025]"><CardHeader className="pb-3"><CardTitle className="flex items-center gap-2 text-sm"><ShieldCheck size={17}/>محدوده و شیوهٔ استفاده</CardTitle><CardDescription className="leading-6">ARG از کلید Ed25519 و اتصال SSH با بررسی Strict Host Key استفاده می‌کند. برای مخزن خصوصی، Deploy Key را در تنظیمات همان مخزن GitHub ثبت کن؛ این پنل به حساب GitHub توکن دسترسی ندارد و نمی‌تواند کلید را از طرفت اضافه کند.</CardDescription></CardHeader><CardContent className="grid gap-3 text-xs leading-6 text-muted-foreground md:grid-cols-2"><p className="rounded-lg border border-border/70 bg-background/35 p-3"><strong className="text-foreground">Deploy Key</strong><br/>به یک مخزن محدود می‌شود و برای استقرار معمولاً دسترسی Read-only کافی است. در GitHub: <span dir="ltr">Settings → Deploy keys → Add deploy key</span>.</p><p className="rounded-lg border border-border/70 bg-background/35 p-3"><strong className="text-foreground">کلید شخصی حساب</strong><br/>در صورت ثبت عمومی در حساب GitHub، دسترسی آن تابع مجوزهای همان حساب است و ممکن است به چند مخزن برسد؛ برای استقرار سایت‌ها Deploy Key محدودشده را ترجیح بده.</p></CardContent></Card>

      <Card><CardHeader className="flex-row items-center justify-between gap-3 space-y-0"><div><CardTitle className="text-sm">اعتماد میزبان GitHub</CardTitle><CardDescription className="mt-1">Host Key از API رسمی GitHub با HTTPS گرفته و با OpenSSH اعتبارسنجی می‌شود.</CardDescription></div><Button variant="outline" size="sm" onClick={() => void refreshHostKeys()} disabled={busy}><RefreshCw size={13}/>دریافت مجدد</Button></CardHeader><CardContent className="text-xs text-muted-foreground">آخرین دریافت: {hostKeys?.updated_at ? dateLabel(hostKeys.updated_at) : 'هنوز ثبت نشده'} {hostKeys?.fresh_for_seconds ? `· اعتبار محلی ${Math.floor(hostKeys.fresh_for_seconds / 86400)} روز` : ''}</CardContent></Card>

      {error && <p role="alert" className="rounded-lg border border-destructive/25 bg-destructive/[.06] p-3 text-xs text-destructive">{error}</p>}
      {loading ? <Card><CardContent className="p-5 text-sm text-muted-foreground">در حال بارگذاری کلیدها…</CardContent></Card> : keys.length === 0 ? <Card><CardContent className="flex flex-col items-center gap-3 p-8 text-center"><KeyRound className="text-muted-foreground"/><p className="text-sm font-medium">هنوز کلید SSH نساخته‌ای</p><p className="max-w-lg text-xs leading-6 text-muted-foreground">از همین صفحه برای یک مخزن Deploy Key بساز، کلید عمومی را در GitHub قرار بده و بعد از بخش اپلیکیشن‌ها استقرار خصوصی را اجرا کن.</p><Button size="sm" onClick={() => setCreateOpen(true)}><Plus size={14}/>ساخت Deploy Key</Button></CardContent></Card> : <div className="grid gap-3">{keys.map((key) => <Card key={key.id} className="overflow-hidden"><CardHeader className="flex-row flex-wrap items-start justify-between gap-3 space-y-0 pb-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><CardTitle className="text-sm">{key.name}</CardTitle><Badge variant="outline">{key.purpose === 'deploy' ? 'Deploy Key' : 'شخصی'}</Badge>{key.application_id && <Badge variant="outline">محدود به اپلیکیشن</Badge>}</div><CardDescription className="mt-1 break-all font-mono text-[11px]" dir="ltr">{key.repository || 'قابل‌استفاده برای مخزن‌های GitHub مجاز'}</CardDescription></div><div className="flex gap-1"><Button size="icon" variant="ghost" title="تغییر نام" aria-label="تغییر نام" onClick={() => { setRenameTarget(key); setRenameValue(key.name); setError('') }}><Pencil size={14}/></Button><Button size="icon" variant="ghost" title="حذف کلید" aria-label="حذف کلید" onClick={() => void deleteKey(key)} disabled={busy}><Trash2 size={14} className="text-destructive"/></Button></div></CardHeader><CardContent className="space-y-3"><div className="grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-2"><p>اثر انگشت <code dir="ltr" className="ml-1 font-mono text-foreground">{key.fingerprint}</code></p><p>آخرین استفاده: {dateLabel(key.last_used_at)}</p><p>ساخته‌شده: {dateLabel(key.created_at)}</p></div><details className="rounded-lg border border-border/70 bg-background/35"><summary className="cursor-pointer px-3 py-2 text-xs">نمایش کلید عمومی</summary><div className="space-y-2 border-t border-border/60 p-3"><code dir="ltr" className="block break-all font-mono text-[11px] leading-5">{key.public_key}</code><Button size="sm" variant="outline" onClick={() => void copy(key.public_key, 'کلید عمومی')}><Copy size={13}/>کپی کلید عمومی</Button></div></details></CardContent></Card>)}</div>}

      <Dialog open={createOpen} onOpenChange={setCreateOpen}><DialogContent><DialogHeader><DialogTitle>ساخت کلید SSH</DialogTitle><DialogDescription>کلید خصوصی به‌صورت Ed25519 روی همین VPS ساخته می‌شود و هرگز از API قابل دریافت نیست.</DialogDescription></DialogHeader><form onSubmit={(event) => void createKey(event)} className="space-y-4"><div className="space-y-2"><Label htmlFor="ssh-purpose">نوع کلید</Label><select id="ssh-purpose" value={purpose} onChange={(event) => setPurpose(event.target.value as 'personal' | 'deploy')} className="h-10 w-full rounded-lg border border-input bg-background px-3 text-sm"><option value="deploy">Deploy Key محدود به یک مخزن</option><option value="personal">کلید شخصی حساب GitHub</option></select></div><div className="space-y-2"><Label htmlFor="ssh-key-name">نام نمایشی</Label><Input id="ssh-key-name" value={name} required maxLength={80} onChange={(event) => setName(event.target.value)} placeholder="مثلاً deploy · فروشگاه"/></div>{purpose === 'deploy' ? <div className="space-y-2"><Label htmlFor="ssh-repository">آدرس SSH مخزن</Label><Input id="ssh-repository" dir="ltr" value={repository} required onChange={(event) => setRepository(event.target.value)} placeholder="git@github.com:owner/repository.git"/><p className="text-[11px] leading-5 text-muted-foreground">این کلید فقط برای همین مخزن قابل انتخاب است. در صفحهٔ GitHub گزینهٔ دسترسی نوشتن را خاموش بگذار، مگر آنکه واقعاً به آن نیاز داشته باشی.</p></div> : <p className="rounded-lg border border-amber-300/20 bg-amber-300/[.05] p-3 text-xs leading-6 text-amber-100">اگر کلید را در تنظیمات SSH حساب GitHub اضافه کنی، دسترسی آن از مجوزهای کل حساب پیروی می‌کند. برای محدودکردن دسترسی، Deploy Key انتخاب کن.</p>}{error && <p role="alert" className="text-xs text-destructive">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setCreateOpen(false)} disabled={busy}>انصراف</Button><Button disabled={busy}>{busy ? 'در حال ساخت…' : 'ساخت کلید روی VPS'}</Button></div></form></DialogContent></Dialog>

      <Dialog open={created !== null} onOpenChange={(open) => { if (!open) setCreated(null) }}><DialogContent className="max-w-2xl"><DialogHeader><DialogTitle>کلید عمومی آمادهٔ ثبت است</DialogTitle><DialogDescription>این متن عمومی است و می‌توانی آن را در GitHub ذخیره کنی. کلید خصوصی فقط در مسیر حفاظت‌شدهٔ همین VPS نگهداری می‌شود.</DialogDescription></DialogHeader>{created && <div className="space-y-4"><div className="rounded-lg border border-border bg-background/55 p-3"><code dir="ltr" className="block break-all font-mono text-xs leading-6">{created.public_key}</code></div><p className="text-xs leading-6 text-muted-foreground">GitHub را باز کن و برو به <span dir="ltr">Settings → Deploy keys → Add deploy key</span>. عنوان کلید را وارد کن، متن بالا را در Key بچسبان و برای دریافت مخزن فقط گزینهٔ دسترسی نوشتن را خاموش بگذار.</p><div className="flex flex-wrap justify-end gap-2"><Button variant="outline" onClick={() => void copy(created.public_key, 'کلید عمومی')}><Copy size={14}/>کپی کلید عمومی</Button>{created.repository && <a href={githubSettingsUrl(created.repository)} target="_blank" rel="noreferrer" className="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground hover:bg-primary/90"><ExternalLink size={14}/>تنظیمات Deploy Keys مخزن</a>}<Button variant="outline" onClick={() => setCreated(null)}><Check size={14}/>پایان</Button></div></div>}</DialogContent></Dialog>

      <Dialog open={renameTarget !== null} onOpenChange={(open) => { if (!open) setRenameTarget(null) }}><DialogContent><DialogHeader><DialogTitle>تغییر نام کلید</DialogTitle></DialogHeader><form onSubmit={(event) => void renameKey(event)} className="space-y-4"><Input autoFocus required maxLength={80} value={renameValue} onChange={(event) => setRenameValue(event.target.value)}/>{error && <p role="alert" className="text-xs text-destructive">{error}</p>}<div className="flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setRenameTarget(null)}>انصراف</Button><Button disabled={busy}>ذخیره</Button></div></form></DialogContent></Dialog>
      {confirmDialog}
    </div>}
  </>
}

function githubSettingsUrl(repository: string) {
  const match = /^(?:git@github\.com:|ssh:\/\/git@github\.com\/)([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+?)(?:\.git)?$/.exec(repository)
  return match ? `https://github.com/${match[1]}/${match[2]}/settings/keys` : 'https://github.com'
}
