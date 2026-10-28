import { useCallback, useEffect, useState } from 'react'
import { KeyRound, LoaderCircle, ShieldCheck, ShieldX } from 'lucide-react'
import { ApiError, api } from './lib/api'
import { Badge } from './components/ui/badge'
import { Button } from './components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/card'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'

type TrustKey = { id: string; label: string; enabled: boolean; created_at: number; revoked_at: number | null }
type Verification = { schema_valid: boolean; extension_id: string; trusted_signer: boolean; signature_valid: boolean; package_bytes_verified: boolean; installable: boolean; message: string }
const exampleManifest = JSON.stringify({ schema_version: 1, id: 'vendor.extension', name: 'Example Extension', version: '1.0.0', arg_api_compat: '0.6', permissions: ['apps.read'], entrypoints: ['ui/index.html'] }, null, 2)

export function ExtensionSecurityPanel({ notify }: { notify: (message: string) => void }) {
  const [keys, setKeys] = useState<TrustKey[]>([])
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [featureUnavailable, setFeatureUnavailable] = useState(false)
  const [keyId, setKeyId] = useState('')
  const [keyLabel, setKeyLabel] = useState('')
  const [publicKey, setPublicKey] = useState('')
  const [manifest, setManifest] = useState(exampleManifest)
  const [packageDigest, setPackageDigest] = useState('')
  const [signerId, setSignerId] = useState('')
  const [signature, setSignature] = useState('')
  const [verification, setVerification] = useState<Verification | null>(null)

  const refresh = useCallback(async () => {
    setLoading(true); setError('')
    try {
      setKeys((await api<{ keys: TrustKey[] }>('extensions/trust-keys')).keys)
      setFeatureUnavailable(false)
    }
    catch (cause) {
      if (cause instanceof ApiError && cause.status === 404) {
        setFeatureUnavailable(true)
        setKeys([])
      } else { setFeatureUnavailable(false); setError(cause instanceof Error ? cause.message : 'کلیدهای اعتماد دریافت نشدند.') }
    }
    finally { setLoading(false) }
  }, [])
  useEffect(() => { void refresh() }, [refresh])

  async function addKey(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError('')
    try {
      await api('extensions/trust-keys', { method: 'POST', body: JSON.stringify({ id: keyId.trim(), label: keyLabel.trim(), public_key_hex: publicKey.trim() }) })
      setKeyId(''); setKeyLabel(''); setPublicKey(''); notify('کلید عمومی امضاکننده به فهرست اعتماد اضافه شد.'); await refresh()
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 404) { setFeatureUnavailable(true); setError('') }
      else setError(cause instanceof Error ? cause.message : 'ثبت کلید اعتماد ناموفق بود.')
    }
    finally { setBusy(false) }
  }
  async function revoke(id: string) {
    setBusy(true); setError('')
    try { await api(`extensions/trust-keys/${encodeURIComponent(id)}`, { method: 'DELETE' }); notify('کلید اعتماد غیرفعال شد.'); await refresh() }
    catch (cause) {
      if (cause instanceof ApiError && cause.status === 404 && !cause.message.includes('کلید اعتماد فعال پیدا نشد')) { setFeatureUnavailable(true); setError('') }
      else setError(cause instanceof Error ? cause.message : 'لغو کلید اعتماد انجام نشد.')
    }
    finally { setBusy(false) }
  }
  async function validate(event: React.FormEvent) {
    event.preventDefault(); setBusy(true); setError(''); setVerification(null)
    try { setVerification(await api<Verification>('extensions/validate', { method: 'POST', body: JSON.stringify({ manifest_json: manifest, package_sha256: packageDigest.trim(), signer_id: signerId.trim(), signature_hex: signature.trim() }) })) }
    catch (cause) {
      if (cause instanceof ApiError && cause.status === 404) { setFeatureUnavailable(true); setError('') }
      else setError(cause instanceof Error ? cause.message : 'اعتبارسنجی manifest ناموفق بود.')
    }
    finally { setBusy(false) }
  }

  return <Card className="mt-5 rounded-2xl border-border/80 bg-card/80">
    <CardHeader><CardTitle className="flex items-center gap-2 text-base"><KeyRound size={17} className="text-primary"/>اعتماد و بررسی manifest افزونه‌ها</CardTitle><CardDescription>کلیدهای عمومی را صریحاً اعتماد بده و قالب/امضای manifest را بررسی کن. این نسخه هیچ بسته‌ای نصب یا اجرا نمی‌کند.</CardDescription></CardHeader>
    <CardContent className="space-y-5">{error && <p role="alert" className="rounded-lg border border-rose-300/20 p-2 text-xs text-rose-200">{error}</p>}
      {featureUnavailable ? <div role="status" className="rounded-xl border border-amber-300/20 bg-amber-300/[.04] p-4 text-xs leading-6 text-amber-100/90"><p className="font-medium">API اعتماد افزونه‌ها در این سرور موجود نیست</p><p className="mt-1 text-amber-100/75">این درخواست با HTTP 404 پاسخ گرفت؛ نسخهٔ ARG روی این VPS یا Node انتخاب‌شده احتمالاً قدیمی است یا هنوز این قابلیت را ندارد. ARG همان سرور را به نسخه‌ای که این API را پشتیبانی می‌کند به‌روزرسانی کن و دوباره بررسی کن. هیچ کلید یا manifest ثبت/اعتبارسنجی نشده است.</p><Button className="mt-3" size="sm" variant="outline" onClick={() => void refresh()} disabled={loading}>{loading ? <LoaderCircle size={14} className="animate-spin"/> : <KeyRound size={14}/>}تلاش دوباره</Button></div> : <>
      <form onSubmit={(event) => void addKey(event)} className="space-y-3 rounded-xl border border-border/60 p-3"><p className="text-xs font-medium">افزودن کلید اعتماد Ed25519</p><div className="grid gap-3 sm:grid-cols-2"><div className="space-y-1.5"><Label htmlFor="extension-key-id">شناسهٔ امضاکننده</Label><Input id="extension-key-id" dir="ltr" value={keyId} onChange={(event) => setKeyId(event.target.value)} placeholder="vendor.release-key" required maxLength={96}/></div><div className="space-y-1.5"><Label htmlFor="extension-key-label">عنوان قابل‌نمایش</Label><Input id="extension-key-label" value={keyLabel} onChange={(event) => setKeyLabel(event.target.value)} placeholder="کلید انتشار Vendor" required maxLength={100}/></div></div><div className="space-y-1.5"><Label htmlFor="extension-public-key">کلید عمومی hex، ۳۲ بایت</Label><Input id="extension-public-key" dir="ltr" className="font-mono text-xs" value={publicKey} onChange={(event) => setPublicKey(event.target.value)} placeholder="۶۴ نویسهٔ hex" required maxLength={64}/></div><Button size="sm" type="submit" disabled={busy}><KeyRound size={14}/>افزودن کلید</Button></form>
      <div className="space-y-2"><p className="text-xs font-medium">کلیدهای ثبت‌شده</p>{loading ? <LoaderCircle size={16} className="animate-spin text-primary"/> : keys.length ? keys.map((key) => <div key={key.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border/60 p-2 text-xs"><span><bdi dir="ltr" className="font-mono">{key.id}</bdi><span className="mr-2 text-muted-foreground">{key.label}</span></span><span className="flex items-center gap-2"><Badge variant={key.enabled ? 'success' : 'outline'}>{key.enabled ? 'فعال' : 'لغوشده'}</Badge>{key.enabled && <Button size="sm" variant="outline" disabled={busy} onClick={() => void revoke(key.id)}>لغو اعتماد</Button>}</span></div>) : <p className="text-[11px] text-muted-foreground">کلید اعتمادی ثبت نشده است.</p>}</div>
      <form onSubmit={(event) => void validate(event)} className="space-y-3 rounded-xl border border-border/60 p-3"><div><p className="text-xs font-medium">اعتبارسنجی امضای manifest</p><p className="mt-1 text-[10px] leading-5 text-muted-foreground">امضا باید روی `ARG-EXTENSION-MANIFEST-V1`، SHA-256 متن دقیق manifest و digest بسته ساخته شده باشد.</p></div><div className="space-y-1.5"><Label htmlFor="extension-manifest">Manifest JSON</Label><textarea id="extension-manifest" dir="ltr" spellCheck={false} value={manifest} onChange={(event) => setManifest(event.target.value)} className="min-h-36 w-full rounded-lg border border-input bg-background p-3 font-mono text-[11px] leading-5"/></div><div className="grid gap-3 sm:grid-cols-2"><div className="space-y-1.5"><Label htmlFor="extension-digest">SHA-256 اعلام‌شدهٔ بسته</Label><Input id="extension-digest" dir="ltr" value={packageDigest} onChange={(event) => setPackageDigest(event.target.value)} className="font-mono text-xs" minLength={64} maxLength={64} required/></div><div className="space-y-1.5"><Label htmlFor="extension-signer">شناسهٔ کلید اعتماد</Label><Input id="extension-signer" dir="ltr" value={signerId} onChange={(event) => setSignerId(event.target.value)} required/></div></div><div className="space-y-1.5"><Label htmlFor="extension-signature">امضای Ed25519 hex</Label><Input id="extension-signature" dir="ltr" value={signature} onChange={(event) => setSignature(event.target.value)} className="font-mono text-xs" minLength={128} maxLength={128} required/></div><Button size="sm" type="submit" disabled={busy}>{busy ? <LoaderCircle size={14} className="animate-spin"/> : <ShieldCheck size={14}/>}بررسی امضا و سازگاری</Button></form>
      {verification && <div className="space-y-2 rounded-xl border border-border/60 p-3"><p className="flex items-center gap-2 text-xs font-medium">{verification.signature_valid && verification.trusted_signer ? <ShieldCheck size={15} className="text-emerald-300"/> : <ShieldX size={15} className="text-amber-300"/>}نتیجهٔ اعتبارسنجی</p><div className="flex flex-wrap gap-1"><Badge variant={verification.schema_valid ? 'success' : 'destructive'}>Schema {verification.schema_valid ? 'معتبر' : 'نامعتبر'}</Badge><Badge variant={verification.trusted_signer ? 'success' : 'outline'}>امضاکننده {verification.trusted_signer ? 'مورد اعتماد' : 'ناشناخته'}</Badge><Badge variant={verification.signature_valid ? 'success' : 'destructive'}>امضا {verification.signature_valid ? 'معتبر' : 'نامعتبر'}</Badge><Badge variant="outline">نصب غیرفعال</Badge></div><p className="text-[10px] leading-5 text-muted-foreground">{verification.message}</p></div>}
      </>}
    </CardContent>
  </Card>
}
