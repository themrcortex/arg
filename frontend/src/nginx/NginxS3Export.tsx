import { useEffect, useMemo, useState } from 'react'
import { Archive, CloudUpload, LoaderCircle, RefreshCw, ShieldCheck } from 'lucide-react'
import { api, formatBytes } from '../lib/api'
import { Alert, AlertDescription } from '../components/ui/alert'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../components/ui/dialog'
import { Input } from '../components/ui/input'
import { Label } from '../components/ui/label'

type Connection = { id: number; display_name: string; provider_type: string }
type Bucket = { id: number; bucket_name: string; display_name: string }
type Entry = { name: string; path: string; kind: 'directory' | 'file' | 'symlink' | 'other'; size: number }
type Listing = { path: string; entries: Entry[]; truncated: boolean }
type FileToUpload = { path: string; key: string; size: number }
type Props = { siteName: string; notify: (message: string) => void }

const MAX_FILES = 10_000
const MAX_TOTAL_BYTES = 20 * 1024 * 1024 * 1024
const MAX_FILE_BYTES = 512 * 1024 * 1024

function extractStaticRoot(content: string): string {
  const roots = [...content.matchAll(/^\s*root\s+([^;]+);/gm)]
    .map((match) => match[1].trim().replace(/^(['"])(.*)\1$/, '$2'))
    .filter((root) => root.startsWith('/') && !root.includes('$') && !root.includes('..'))
  const unique = [...new Set(roots)]
  if (unique.length !== 1)
    throw new Error(
      unique.length
        ? 'این سایت چند ریشهٔ فایل متفاوت دارد؛ مسیر ریشه را دستی انتخاب کن.'
        : 'ریشهٔ ثابت و قابل انتقال در کانفیگ پیدا نشد؛ مسیر را دستی وارد کن.',
    )
  return unique[0]
}

function uniqueRunId() {
  const bytes = new Uint8Array(12)
  window.crypto.getRandomValues(bytes)
  return [...bytes].map((value) => value.toString(16).padStart(2, '0')).join('')
}

export function NginxS3Export({ siteName, notify }: Props) {
  const [open, setOpen] = useState(false)
  const [loading, setLoading] = useState(false)
  const [connections, setConnections] = useState<Connection[]>([])
  const [connectionId, setConnectionId] = useState('')
  const [buckets, setBuckets] = useState<Bucket[]>([])
  const [bucket, setBucket] = useState('')
  const [root, setRoot] = useState('')
  const [keyPrefix, setKeyPrefix] = useState('')
  const [files, setFiles] = useState<FileToUpload[]>([])
  const [scanNote, setScanNote] = useState('')
  const [includeSensitive, setIncludeSensitive] = useState(false)
  const [error, setError] = useState('')
  const [progress, setProgress] = useState<{
    done: number
    total: number
    bytes: number
    current?: string
  } | null>(null)
  const running = loading || progress !== null
  const connection = useMemo(
    () => connections.find((item) => String(item.id) === connectionId),
    [connections, connectionId],
  )

  useEffect(() => {
    if (!open || !connectionId) return
    let cancelled = false
    setLoading(true)
    setError('')
    setBucket('')
    setBuckets([])
    api<{ connections: Bucket[] }>(`s3/connections/${connectionId}/buckets?local_only=1`)
      .then((result) => {
        if (!cancelled) setBuckets(result.connections)
      })
      .catch((reason) => {
        if (!cancelled) setError(reason instanceof Error ? reason.message : 'فهرست Bucketها دریافت نشد.')
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [open, connectionId])

  async function begin() {
    setOpen(true)
    setLoading(true)
    setError('')
    setFiles([])
    setProgress(null)
    setScanNote('')
    try {
      const [connectionResult, siteResult] = await Promise.all([
        api<{ connections: Connection[] }>('s3/connections'),
        api<{ site: { content: string } }>(`nginx/sites/${encodeURIComponent(siteName)}`),
      ])
      setConnections(connectionResult.connections)
      setConnectionId(connectionResult.connections[0] ? String(connectionResult.connections[0].id) : '')
      const detectedRoot = extractStaticRoot(siteResult.site.content)
      setRoot(detectedRoot)
      setKeyPrefix(`nginx-sites/${siteName}/snapshots/${Date.now()}-${uniqueRunId()}/`)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'اطلاعات سایت یا اتصال‌های S3 دریافت نشد.')
    } finally {
      setLoading(false)
    }
  }

  async function scan() {
    setLoading(true)
    setError('')
    setFiles([])
    setScanNote('')
    const result: FileToUpload[] = []
    let total = 0
    let visited = 0
    let skippedLinks = 0
    let skippedSensitive = 0
    const base = root.replace(/\/+$/, '')
    const walk = async (path: string, relative: string, depth: number): Promise<void> => {
      if (depth > 64) throw new Error('عمق سایت از 64 سطح بیشتر است؛ برای ایمنی انتقال متوقف شد.')
      visited += 1
      if (visited > MAX_FILES) throw new Error('تعداد فایل‌ها و پوشه‌ها از سقف 10,000 ورودی عبور کرد.')
      const listing = await api<Listing>(`files?path=${encodeURIComponent(path)}`)
      if (listing.truncated) throw new Error(`فهرست ${path} ناقص است؛ پوشه را کوچک‌تر انتخاب کن.`)
      for (const entry of listing.entries) {
        if (entry.kind === 'symlink' || entry.kind === 'other') {
          skippedLinks += 1
          continue
        }
        const child = relative ? `${relative}/${entry.name}` : entry.name
        const sensitive =
          entry.name === '.git' ||
          entry.name === '.ssh' ||
          /^\.env(?:\.|$)/i.test(entry.name) ||
          /\.(?:pem|key)$/i.test(entry.name)
        if (sensitive && !includeSensitive) {
          skippedSensitive += 1
          continue
        }
        if (entry.kind === 'directory') {
          await walk(entry.path, child, depth + 1)
          continue
        }
        if (entry.kind !== 'file') continue
        if (entry.size > MAX_FILE_BYTES) throw new Error(`فایل ${entry.path} از سقف 512 MiB بزرگ‌تر است.`)
        total += entry.size
        if (result.length >= MAX_FILES || total > MAX_TOTAL_BYTES)
          throw new Error('سقف این عملیات 10,000 فایل و 20 GiB است؛ ریشه را به بخش‌های کوچک‌تر تقسیم کن.')
        const objectKey = `${keyPrefix}${child}`
        if (new TextEncoder().encode(objectKey).length > 1024)
          throw new Error(`مسیر Object برای ${entry.path} از حد S3 بیشتر است.`)
        result.push({ path: entry.path, key: objectKey, size: entry.size })
      }
    }
    try {
      if (!base.startsWith('/') || base === '/') throw new Error('ریشهٔ سایت باید یک مسیر مشخص به‌جز / باشد.')
      if (!keyPrefix.trim() || keyPrefix.startsWith('/') || keyPrefix.split('/').includes('..'))
        throw new Error('Prefix مقصد در S3 معتبر نیست.')
      await walk(base, '', 0)
      if (!result.length) throw new Error('در ریشهٔ سایت فایل قابل انتقالی پیدا نشد.')
      setFiles(result)
      setScanNote(
        `${result.length} فایل · ${formatBytes(total)}${skippedLinks ? ` · ${skippedLinks} پیوند/فایل ویژه نادیده گرفته شد` : ''}${skippedSensitive ? ` · ${skippedSensitive} مورد حساس نادیده گرفته شد` : ''}`,
      )
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'بررسی فایل‌های سایت ناموفق بود.')
    } finally {
      setLoading(false)
    }
  }

  async function upload() {
    if (!connection || !bucket || !files.length) return
    setError('')
    setLoading(true)
    setProgress({ done: 0, total: files.length, bytes: 0 })
    let sentBytes = 0
    let uploadedFiles = 0
    try {
      for (const [index, file] of files.entries()) {
        setProgress({ done: index, total: files.length, bytes: sentBytes, current: file.key })
        await api(`s3/connections/${connection.id}/transfer/local-to-bucket`, {
          method: 'POST',
          body: JSON.stringify({ bucket, key: file.key, local_path: file.path, overwrite: true }),
        })
        sentBytes += file.size
        uploadedFiles = index + 1
        setProgress({
          done: index + 1,
          total: files.length,
          bytes: sentBytes,
          current: files[index + 1]?.key,
        })
      }
      notify(`نسخهٔ سایت در S3 ذخیره شد: ${files.length} فایل، ${formatBytes(sentBytes)}`)
      setOpen(false)
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : 'انتقال سایت به S3 ناموفق بود.'
      setError(
        `انتقال پس از ${uploadedFiles} فایل از ${files.length} فایل متوقف شد. فایل‌های محلی دست‌نخورده‌اند؛ اشیای جزئی در Prefix مقصد باقی مانده‌اند. ${message}`,
      )
      notify('انتقال سایت به S3 کامل نشد؛ جزئیات در پنجرهٔ انتقال آمده است.')
    } finally {
      setLoading(false)
      setProgress(null)
    }
  }

  return (
    <>
      <Button
        size="sm"
        variant="outline"
        onClick={() => void begin()}
        disabled={running}
        title="کپی امن محتوای ریشهٔ سایت به فضای S3"
      >
        <CloudUpload size={14} />
        کپی محتوا به S3
      </Button>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!running) setOpen(next)
        }}
      >
        <DialogContent className="max-h-[90dvh] max-w-2xl overflow-y-auto">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Archive size={18} className="text-primary" />
              کپی محتوای سایت به S3
            </DialogTitle>
            <DialogDescription>
              برای سایت {siteName} یک نسخهٔ جدا در Bucket ساخته می‌شود. ریشه و فایل‌های سایت تغییر نمی‌کنند و
              Nginx همچنان از دیسک محلی سرویس می‌دهد.
            </DialogDescription>
          </DialogHeader>
          <Alert>
            <ShieldCheck size={15} className="mb-1 text-primary" />
            <AlertDescription>
              Object Storage جایگزین مستقیم ریشهٔ Nginx نیست؛ این عملیات نسخهٔ پشتیبان فایل‌ها را در S3
              می‌سازد و به سایت دست نمی‌زند.
            </AlertDescription>
          </Alert>
          <label className="flex items-start gap-2 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs leading-5">
            <input
              type="checkbox"
              className="mt-1"
              checked={includeSensitive}
              onChange={(event) => {
                setIncludeSensitive(event.target.checked)
                setFiles([])
                setScanNote('')
              }}
              disabled={running}
            />
            <span>
              <b className="block">شامل‌کردن فایل‌های حساس و مخزن Git</b>
              <span className="text-muted-foreground">
                به‌طور پیش‌فرض `.env`، `.git`، `.ssh` و فایل‌های کلید خصوصی در S3 کپی نمی‌شوند.
              </span>
            </span>
          </label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="space-y-1.5">
              <Label>اتصال S3</Label>
              <select
                className="h-10 w-full rounded-lg border border-input bg-background px-3 text-sm"
                value={connectionId}
                onChange={(event) => setConnectionId(event.target.value)}
                disabled={running}
              >
                <option value="">انتخاب اتصال</option>
                {connections.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.display_name} · {item.provider_type}
                  </option>
                ))}
              </select>
            </label>
            <label className="space-y-1.5">
              <Label>Bucket متصل به ARG</Label>
              <select
                className="h-10 w-full rounded-lg border border-input bg-background px-3 font-mono text-xs"
                value={bucket}
                onChange={(event) => setBucket(event.target.value)}
                disabled={running || !buckets.length}
              >
                <option value="">انتخاب Bucket</option>
                {buckets.map((item) => (
                  <option key={item.id} value={item.bucket_name}>
                    {item.display_name} · {item.bucket_name}
                  </option>
                ))}
              </select>
            </label>
            <label className="space-y-1.5">
              <Label>ریشهٔ سایت روی VPS</Label>
              <Input
                dir="ltr"
                className="font-mono text-xs"
                value={root}
                onChange={(event) => {
                  setRoot(event.target.value)
                  setFiles([])
                }}
                disabled={running}
              />
            </label>
            <label className="space-y-1.5">
              <Label>Prefix نسخه در Bucket</Label>
              <Input
                dir="ltr"
                className="font-mono text-xs"
                value={keyPrefix}
                onChange={(event) => {
                  setKeyPrefix(event.target.value)
                  setFiles([])
                }}
                disabled={running}
              />
            </label>
          </div>
          {!loading && !connections.length && (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs">
              <span>برای انتقال، ابتدا یک اتصال S3 و Bucket در ARG ثبت کن.</span>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setOpen(false)
                  window.location.hash = '#/s3'
                }}
              >
                رفتن به مدیریت S3
              </Button>
            </div>
          )}
          {!loading && connection && !buckets.length && (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-400/20 bg-amber-400/[.04] p-3 text-xs">
              <span>برای این اتصال Bucket ثبت نشده است؛ ابتدا آن را به ARG متصل کن.</span>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setOpen(false)
                  window.location.hash = '#/s3'
                }}
              >
                مدیریت Bucketها
              </Button>
            </div>
          )}
          {error && (
            <p
              role="alert"
              className="rounded-lg border border-rose-400/25 bg-rose-400/[.06] p-3 text-xs leading-5 text-rose-200"
            >
              {error}
            </p>
          )}
          {scanNote && !progress && (
            <div className="flex items-center justify-between gap-3 rounded-lg border border-border p-3 text-xs">
              <span>پیش‌نمایش انتقال</span>
              <Badge variant="outline">{scanNote}</Badge>
            </div>
          )}
          {progress && (
            <div role="status" className="space-y-2 rounded-lg border border-primary/20 bg-primary/[.04] p-3">
              <div className="flex items-center justify-between gap-3 text-xs">
                <span className="flex items-center gap-2">
                  <LoaderCircle size={14} className="animate-spin text-primary" />
                  در حال کپی فایل‌ها به S3
                </span>
                <span dir="ltr" className="font-mono">
                  {progress.done}/{progress.total}
                </span>
              </div>
              <div className="h-2 overflow-hidden rounded-full bg-secondary">
                <div
                  className="h-full rounded-full bg-primary transition-[width]"
                  style={{ width: `${Math.max(2, (progress.done / progress.total) * 100)}%` }}
                />
              </div>
              <p className="text-[10px] text-muted-foreground">
                ارسال‌شده: {formatBytes(progress.bytes)}
                {progress.current ? ` · ${progress.current}` : ''}
              </p>
            </div>
          )}
          <div className="flex flex-wrap justify-between gap-2">
            <p className="self-center text-[10px] text-muted-foreground">
              سقف: 512 MiB برای هر فایل و 20 GiB برای هر بار انتقال · symlinkها نادیده گرفته می‌شوند.
            </p>
            <div className="flex gap-2">
              <Button variant="outline" onClick={() => setOpen(false)} disabled={running}>
                بستن
              </Button>
              {files.length ? (
                <Button onClick={() => void upload()} disabled={running || !bucket}>
                  {loading ? <LoaderCircle size={14} className="animate-spin" /> : <CloudUpload size={14} />}
                  شروع کپی
                </Button>
              ) : (
                <Button
                  onClick={() => void scan()}
                  disabled={running || !bucket || !root.trim() || !keyPrefix.trim()}
                >
                  {loading ? <LoaderCircle size={14} className="animate-spin" /> : <RefreshCw size={14} />}
                  بررسی محتوا
                </Button>
              )}
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}
