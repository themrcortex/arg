import { useEffect, useRef, useState } from 'react'
import { AlertTriangle, CheckCircle2, Download, FileText, LoaderCircle, RefreshCw, RotateCw, X } from 'lucide-react'
import { api } from './lib/api'
import { Button } from './components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './components/ui/dialog'

type NodeTarget = { id: number; name: string; public_name?: string; endpoint: string }
type Branch = 'main' | 'development'
type BuildMode = 'fast' | 'complete'
type UpdateStrategy = 'release' | 'source'
type NodeStatus = {
  server_uuid?: string
  hostname?: string
  os?: string
  architecture?: string
  arg_version?: string
  protocol_version?: number
  capabilities_version?: number
  capabilities?: string[]
}
type UpdateInfo = {
  status: 'available' | 'current' | 'unavailable'
  local?: string
  remote?: string
  local_version?: string
  remote_version?: string
  error?: string
  branch?: string
}
type UpdateProgress = {
  updating?: boolean
  state?: 'idle' | 'running' | 'succeeded' | 'failed' | 'cancelled'
  phase?: string
  previous_phase?: string
  operation?: 'update' | 'rollback'
  started_at?: number | null
  updated_at?: number | null
  job_id?: string | null
  message?: string
  branch?: Branch
  build_mode?: BuildMode
  strategy?: UpdateStrategy
}
type CheckResult = {
  supported: boolean
  reason?: string
  node_status?: NodeStatus
  update?: UpdateInfo | null
  progress?: UpdateProgress | null
}
type ConfirmAction = 'update' | 'cancel' | 'rollback' | null

const phaseTitle: Record<string, string> = {
  preparing: 'آماده‌سازی دریافت و وابستگی‌ها',
  queued: 'در صف اجرای به‌روزرسانی',
  building: 'کامپایل نسخهٔ جدید روی Node',
  installing: 'نصب نسخهٔ ساخته‌شده',
  restarting: 'راه‌اندازی دوبارهٔ سرویس ARG',
  verifying: 'بررسی سلامت نسخه و سرویس',
  succeeded: 'به‌روزرسانی با بررسی سلامت کامل شد',
  failed: 'به‌روزرسانی ناموفق بود؛ بررسی گزارش یا بازگشت نسخه',
  cancelled: 'به‌روزرسانی لغو شد',
  rollback: 'در حال بازگشت نسخهٔ قبلی',
  cancelling: 'درخواست لغو ثبت شد؛ بازگشت امن در حال بررسی است',
  complete: 'به‌روزرسانی با بررسی سلامت کامل شد',
  rolled_back: 'بازگشت نسخهٔ قبلی کامل و بررسی شد',
  unknown: 'خواندن وضعیت واقعی فرایند',
}

function branchLabel(branch: Branch) {
  return branch === 'main' ? 'main · پایدار' : 'development · آزمایشی'
}

function supportsPrebuiltRelease(version?: string) {
  const match = version?.match(/^(\d+)\.(\d+)\.(\d+)/)
  if (!match) return false
  const current = match.slice(1).map(Number)
  const minimum = [0, 7, 7]
  for (let index = 0; index < minimum.length; index += 1) {
    if (current[index] !== minimum[index]) return current[index] > minimum[index]
  }
  return true
}

export function NodeUpdateDialog({
  open,
  onOpenChange,
  node,
  notify,
  onRefresh,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  node: NodeTarget | null
  notify: (message: string) => void
  onRefresh: () => void
}) {
  const [branch, setBranch] = useState<Branch>('main')
  const [buildMode, setBuildMode] = useState<BuildMode>('complete')
  const [strategy, setStrategy] = useState<UpdateStrategy>('release')
  const [check, setCheck] = useState<CheckResult | null>(null)
  const [progress, setProgress] = useState<UpdateProgress | null>(null)
  const [checking, setChecking] = useState(false)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [tracking, setTracking] = useState(false)
  const [trackingSince, setTrackingSince] = useState<number | null>(null)
  const [reconnecting, setReconnecting] = useState(false)
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null)
  const [logs, setLogs] = useState('')
  const [logsOpen, setLogsOpen] = useState(false)
  const [logsLoading, setLogsLoading] = useState(false)
  const [logsTruncated, setLogsTruncated] = useState(false)
  const runStarted = useRef<number | null>(null)
  const nodeId = node?.id
  const latest = useRef({ check, node, notify, onRefresh, branch })
  latest.current = { check, node, notify, onRefresh, branch }

  async function refreshCheck(targetId: number, selectedBranch: Branch) {
    setChecking(true)
    setError('')
    setLogs('')
    setLogsOpen(false)
    setLogsTruncated(false)
    try {
      const result = await api<CheckResult>(`nodes/${targetId}/update?branch=${selectedBranch}`)
      setCheck(result)
      if (!supportsPrebuiltRelease(result.node_status?.arg_version)) setStrategy('source')
      if (result.progress) {
        setProgress(result.progress)
        if (result.progress.updating || result.progress.state === 'running') {
          setTracking(true)
          setTrackingSince(Date.now())
          runStarted.current = Date.now()
        }
      }
      if (!result.supported && result.reason) setError(result.reason)
      return result
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : 'وضعیت به‌روزرسانی Node خوانده نشد.'
      setError(message)
      setCheck(null)
      return null
    } finally {
      setChecking(false)
    }
  }

  useEffect(() => {
    if (!open || nodeId == null) return
    setBranch('main')
    setBuildMode('complete')
    setStrategy('release')
    setCheck(null)
    setProgress(null)
    setError('')
    setConfirmAction(null)
    setTracking(false)
    runStarted.current = null
    void refreshCheck(nodeId, 'main')
    // A new dialog target starts a fresh status check. Branch changes are handled by the selector.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, nodeId])

  useEffect(() => {
    if (!open || !tracking || nodeId == null) return
    let stopped = false
    let timer: number | undefined
    const poll = async () => {
      try {
        const current = await api<UpdateProgress>(`nodes/${nodeId}/update/progress`)
        if (stopped) return
        setProgress(current)
        setReconnecting(false)
        setError((previous) => (previous.startsWith('ارتباط هنگام راه‌اندازی مجدد') ? '' : previous))
        const started = runStarted.current || trackingSince || Date.now()
        if (current.state === 'succeeded') {
          const rollbackSucceeded = current.operation === 'rollback' || current.phase === 'rolled_back'
          setBusy('verifying')
          try {
            const verified = await api<NodeStatus>(`nodes/${nodeId}/status`)
            const expected = latest.current.check?.node_status?.server_uuid
            if (
              !verified.server_uuid ||
              (expected && verified.server_uuid.toLowerCase() !== expected.toLowerCase())
            ) {
              setTracking(false)
              setError(
                'نسخهٔ جدید پاسخ داد، اما هویت سرور با Node ثبت‌شده تطبیق ندارد؛ اتصال و کلید محلی حفظ شده و تأیید متوقف شد.',
              )
              setProgress({ ...current, state: 'failed', phase: 'failed' })
            } else if (
              verified.protocol_version !== 2 ||
              verified.capabilities_version !== 1 ||
              !verified.capabilities?.includes('node.update.manage') &&
                !verified.capabilities?.includes('workspace.api.proxy')
            ) {
              setTracking(false)
              setError(
                'سرویس بالا آمده و شناسهٔ سرور حفظ شده، اما نسخهٔ نصب‌شده مسیر مدیریت Node را اعلام نمی‌کند. اتصال و کلید محلی حذف نشده‌اند؛ برای بازیابی، نسخهٔ قبلی را برگردان.',
              )
              setProgress({ ...current, state: 'failed', phase: 'failed' })
            } else {
              setTracking(false)
              if (rollbackSucceeded) await refreshCheck(nodeId, latest.current.branch)
              else setCheck((previous) =>
                previous
                  ? {
                      ...previous,
                      node_status: verified,
                      update: { ...previous.update, status: 'current', local_version: verified.arg_version },
                    }
                  : previous,
              )
              setProgress(current)
              latest.current.notify(rollbackSucceeded
                ? `بازگشت نسخهٔ قبلی ${latest.current.node?.public_name || latest.current.node?.name || 'Node'} انجام شد و هویت و ارتباطش دوباره تأیید شد.`
                : `به‌روزرسانی ${latest.current.node?.public_name || latest.current.node?.name || 'Node'} انجام شد و هویت و ارتباطش دوباره تأیید شد.`)
              latest.current.onRefresh()
            }
          } catch (cause) {
            const message = cause instanceof Error ? cause.message : 'پاسخ وضعیت Node دریافت نشد.'
            if (message.includes('هویت سرور مقصد')) {
              setTracking(false)
              setError(
                `${message} اتصال ذخیره‌شده و کلید محلی حفظ شده‌اند؛ نسخهٔ قبلی را فقط پس از بررسی مقصد برگردان.`,
              )
              setProgress({ ...current, state: 'failed', phase: 'verifying' })
            } else if (Date.now() - started > 60 * 60_000) {
              setTracking(false)
              setError(
                `به‌روزرسانی از سمت Node موفق گزارش شده، اما پس از ۶۰ دقیقه هویت و API قابل‌تأیید نیست: ${message}. اتصال ذخیره‌شده و کلید محلی حذف نشده‌اند.`,
              )
              setProgress({ ...current, state: 'failed', phase: 'verifying' })
            } else {
              setReconnecting(true)
              setError(
                'Update Helper به‌روزرسانی را موفق گزارش کرده؛ برای تأیید نسخه و شناسه منتظر پاسخ سرویس Node هستم. اتصال و کلید حفظ شده‌اند.',
              )
              timer = window.setTimeout(() => void poll(), 5000)
            }
          } finally {
            setBusy('')
          }
          if (!tracking) return
          return
        }
        if (current.state === 'failed' || current.state === 'cancelled') {
          setTracking(false)
          if (current.state === 'failed')
            setError(
              current.message || 'به‌روزرسانی کامل نشد. گزارش Node را بررسی کن یا نسخهٔ قبلی را برگردان.',
            )
          else
            latest.current.notify('به‌روزرسانی Node لغو شد؛ نتیجه و سلامت سرویس را از وضعیت Node بررسی کن.')
          latest.current.onRefresh()
          return
        }
        // The helper may briefly report idle while its scheduled updater starts. Keep polling.
        if (Date.now() - started > 60 * 60_000) {
          setTracking(false)
          setError(
            'پس از ۶۰ دقیقه هنوز نتیجهٔ قطعی از Node دریافت نشد. عملیات آن سرور ممکن است ادامه داشته باشد؛ ارتباط حذف نشده و می‌توانی دوباره وضعیت یا گزارش را بخوانی.',
          )
          return
        }
        timer = window.setTimeout(
          () => void poll(),
          current.state === 'running' || current.updating ? 2500 : 5000,
        )
      } catch (cause) {
        if (stopped) return
        setReconnecting(true)
        setError(
          'ارتباط هنگام راه‌اندازی مجدد Node موقتاً قطع شده؛ در حال تلاش برای اتصال دوباره هستم. کلید و شناسهٔ اتصال حفظ شده‌اند.',
        )
        const started = runStarted.current || trackingSince || Date.now()
        if (Date.now() - started > 60 * 60_000) {
          setTracking(false)
          setError(
            cause instanceof Error
              ? `پس از راه‌اندازی مجدد پاسخ Node دریافت نشد: ${cause.message}. اتصال ذخیره‌شده و کلیدش حفظ شده‌اند؛ وضعیت را دوباره بررسی کن.`
              : 'Node پس از راه‌اندازی مجدد هنوز پاسخ نداده است؛ اتصال ذخیره‌شده و کلیدش حفظ شده‌اند.',
          )
          return
        }
        timer = window.setTimeout(() => void poll(), 5000)
      }
    }
    void poll()
    return () => {
      stopped = true
      if (timer) window.clearTimeout(timer)
    }
  }, [open, tracking, nodeId, trackingSince])

  async function checkSelectedBranch(next: Branch) {
    setConfirmAction(null)
    setBranch(next)
    if (nodeId != null && !tracking) await refreshCheck(nodeId, next)
  }

  async function startUpdate() {
    if (nodeId == null || !check?.supported || check.update?.status !== 'available') return
    setBusy('run')
    setError('')
    try {
      await api(`nodes/${nodeId}/update?mode=${buildMode}&branch=${branch}&strategy=${strategy}`, { method: 'POST' })
      setProgress({
        state: 'running',
        updating: true,
        phase: 'preparing',
        operation: 'update',
        branch,
        build_mode: buildMode,
        strategy,
        started_at: Math.floor(Date.now() / 1000),
      })
      runStarted.current = Date.now()
      setTrackingSince(Date.now())
      setTracking(true)
      setConfirmAction(null)
      notify(`به‌روزرسانی ${node?.public_name || node?.name || 'Node'} روی همان سرور زمان‌بندی شد.`)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'زمان‌بندی به‌روزرسانی Node ناموفق بود.')
    } finally {
      setBusy('')
    }
  }

  async function cancelUpdate() {
    if (nodeId == null) return
    setBusy('cancel')
    try {
      await api(`nodes/${nodeId}/update/cancel`, { method: 'POST' })
      setError('درخواست لغو امن ارسال شد. تا تأیید وضعیت نهایی، Node را از این فهرست حذف نکن.')
      setConfirmAction(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'لغو امن درخواست نشد.')
    } finally {
      setBusy('')
      setTracking(true)
    }
  }

  async function rollbackUpdate() {
    if (nodeId == null) return
    setBusy('rollback')
    try {
      await api(`nodes/${nodeId}/update/rollback`, { method: 'POST' })
      setError('بازگشت نسخهٔ قبلی روی Node زمان‌بندی شد؛ وضعیت سرویس و اتصال در حال بررسی است.')
      setProgress((current) => ({ ...current, operation: 'rollback', state: 'running', phase: 'installing', updating: true, updated_at: Math.floor(Date.now() / 1000) }))
      runStarted.current = Date.now()
      setTrackingSince(Date.now())
      setTracking(true)
      setConfirmAction(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'بازگشت نسخهٔ قبلی درخواست نشد.')
    } finally {
      setBusy('')
    }
  }

  async function refreshLogs() {
    if (nodeId == null || logsLoading) return
    setLogsLoading(true)
    try {
      const result = await api<{ logs: string; truncated?: boolean }>(`nodes/${nodeId}/update/logs`)
      setLogs(result.logs || 'گزارشی از Update Helper دریافت نشد.')
      setLogsTruncated(result.truncated === true)
    } catch (cause) {
      setLogs(cause instanceof Error ? cause.message : 'گزارش به‌روزرسانی از Node دریافت نشد.')
    } finally { setLogsLoading(false) }
  }

  async function toggleLogs() {
    if (logsOpen) { setLogsOpen(false); return }
    setLogsOpen(true)
    if (!logs) await refreshLogs()
  }

  const update = check?.update
  const active = tracking || progress?.updating || progress?.state === 'running'
  const terminal =
    progress?.state === 'succeeded' || progress?.state === 'failed' || progress?.state === 'cancelled'
  const phase = progress?.phase || (progress?.state === 'running' ? 'unknown' : '')
  const stageSequence: [string, string][] = progress?.operation === 'rollback'
    ? [['installing', 'بازگردانی فایل نسخهٔ قبلی'], ['restarting', 'راه‌اندازی سرویس مقصد'], ['verifying', 'بررسی سلامت و هویت Node']]
    : [['preparing', 'دریافت نسخه و آماده‌سازی'], ['building', 'کامپایل نسخه روی Node'], ['installing', 'نصب نسخهٔ بررسی‌شده'], ['restarting', 'راه‌اندازی سرویس مقصد'], ['verifying', 'بررسی سلامت و هویت Node']]
  const stagePhase = ['failed', 'cancelled'].includes(phase) ? progress?.previous_phase || phase : phase
  const activeStageIndex = stageSequence.findIndex(([stage]) => stage === stagePhase)
  const stagesComplete = progress?.state === 'succeeded' && (phase === 'complete' || phase === 'rolled_back')

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next)
      }}
    >
      <DialogContent className="max-h-[88dvh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Download size={18} className="text-primary" />
            به‌روزرسانی Node از این پنل
          </DialogTitle>
          <DialogDescription>
            {node?.public_name || node?.name || 'Node'} ·{' '}
            <span dir="ltr" className="font-mono">
              {node?.endpoint}
            </span>
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="rounded-xl border border-border bg-background/40 p-3 text-xs leading-6 text-muted-foreground">
            به‌روزرسانی روی خود Node اجرا می‌شود. ARG شناسهٔ سرور و کلید Federation را عوض نمی‌کند؛ هنگام
            Restart ممکن است ارتباط چند لحظه قطع شود. بعد از راه‌اندازی، همان هویت و API دوباره بررسی می‌شوند.
            اگر سرویس جدید سالم نشود، Update Helper نسخه و snapshot دیتابیس قبلی را برمی‌گرداند.
          </div>

          {checking && (
            <div
              role="status"
              className="flex items-center gap-2 rounded-xl border border-border p-4 text-sm text-muted-foreground"
            >
              <LoaderCircle size={16} className="animate-spin" />
              در حال بررسی هویت Node، مجوز و نسخهٔ شاخهٔ {branch}…
            </div>
          )}

          {check?.node_status && (
            <div className="grid grid-cols-2 gap-2 rounded-xl border border-border bg-secondary/25 p-3 text-xs sm:grid-cols-3">
              <div>
                <span className="block text-muted-foreground">نام سرور</span>
                <b>{check.node_status.hostname || '—'}</b>
              </div>
              <div>
                <span className="block text-muted-foreground">نسخهٔ ARG</span>
                <b dir="ltr" className="font-mono">
                  {check.node_status.arg_version ? `v${check.node_status.arg_version}` : '—'}
                </b>
              </div>
              <div>
                <span className="block text-muted-foreground">شناسهٔ پایدار</span>
                <b dir="ltr" className="font-mono">
                  {check.node_status.server_uuid?.slice(0, 13) || '—'}
                </b>
              </div>
            </div>
          )}

          {check?.supported && !active && (
            <section
              className="space-y-3 rounded-xl border border-border bg-background/35 p-3"
              aria-label="شاخه و روش ساخت Node"
            >
              <div>
                <p className="mb-2 text-xs font-semibold">شاخهٔ دریافت نسخه</p>
                <div className="grid grid-cols-2 gap-2">
                  {(['main', 'development'] as const).map((item) => (
                    <button
                      key={item}
                      type="button"
                      aria-pressed={branch === item}
                      onClick={() => void checkSelectedBranch(item)}
                      className={`rounded-lg border p-3 text-right transition-colors ${branch === item ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}
                    >
                      <span dir="ltr" className="block text-left font-mono text-xs font-bold">
                        {item}
                      </span>
                      <span className="mt-1 block text-[10px] text-muted-foreground">
                        {item === 'main' ? 'پایدار و پیشنهادی' : 'آزمایشی؛ ممکن است ناپایدار باشد'}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <p className="mb-2 text-xs font-semibold">روش به‌روزرسانی Node</p>
                <div className="grid gap-2 sm:grid-cols-2">
                  {(['release', 'source'] as const).map((item) => (
                    <button
                      key={item}
                      type="button"
                      aria-pressed={strategy === item}
                      disabled={item === 'release' && !supportsPrebuiltRelease(check?.node_status?.arg_version)}
                      onClick={() => setStrategy(item)}
                      className={`rounded-lg border p-3 text-right transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${strategy === item ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}
                    >
                      <span className="block text-xs font-semibold">{item === 'release' ? 'دریافت نسخهٔ آماده · پیشنهادی' : 'کامپایل از سورس'}</span>
                      <span className="mt-1 block text-[10px] leading-5 text-muted-foreground">{item === 'release' ? supportsPrebuiltRelease(check?.node_status?.arg_version) ? 'باینری نسخهٔ انتخاب‌شده را می‌گیرد، checksum را بررسی می‌کند؛ روی Node کامپایل انجام نمی‌شود.' : 'نسخهٔ ARG این Node از روش Release پشتیبانی نمی‌کند؛ برای به‌روزرسانی از سورس استفاده کن.' : 'سورس همان شاخه را روی Node کامپایل می‌کند و CPU، RAM و دیسک مصرف می‌شود.'}</span>
                    </button>
                  ))}
                </div>
              </div>
              {strategy === 'source' && <div>
                <p className="mb-2 text-xs font-semibold">حافظهٔ ساخت از سورس</p>
                <div className="grid grid-cols-2 gap-2">
                  {(['fast', 'complete'] as const).map((item) => (
                    <button
                      key={item}
                      type="button"
                      aria-pressed={buildMode === item}
                      onClick={() => setBuildMode(item)}
                      className={`rounded-lg border p-3 text-right transition-colors ${buildMode === item ? 'border-primary/45 bg-primary/[.07]' : 'border-border hover:bg-secondary/50'}`}
                    >
                      <span className="block text-xs font-semibold">
                        {item === 'fast' ? 'آپدیت سریع' : 'آپدیت کامل'}
                      </span>
                      <span className="mt-1 block text-[10px] leading-5 text-muted-foreground">
                        {item === 'fast'
                          ? 'Cache را نگه می‌دارد و آپدیت‌های بعدی سریع‌تر می‌شوند.'
                          : 'Cache کامپایل را پاک می‌کند؛ زمان ساخت بیشتر می‌شود و فضای دیسک کمتری باقی می‌ماند.'}
                      </span>
                    </button>
                  ))}
                </div>
              </div>}
            </section>
          )}

          {check?.supported && update && (
            <div
              className={`rounded-xl border p-3 text-sm ${update.status === 'available' ? 'border-amber-400/25 bg-amber-400/[.06] text-amber-100' : update.status === 'current' ? 'border-emerald-400/20 bg-emerald-400/[.06] text-emerald-200' : 'border-rose-400/20 bg-rose-400/[.05] text-rose-100'}`}
            >
              <div className="font-semibold">
                {update.status === 'available'
                  ? 'نسخهٔ جدید موجود است'
                  : update.status === 'current'
                    ? 'Node به‌روز است'
                    : 'نسخه بررسی نشد'}
              </div>
              <div className="mt-2 grid grid-cols-2 gap-2 text-xs">
                <span>
                  نصب:{' '}
                  <b dir="ltr" className="font-mono">
                    {update.local_version ? `v${update.local_version}` : update.local || '—'}
                  </b>
                </span>
                <span>
                  شاخهٔ {branch}:{' '}
                  <b dir="ltr" className="font-mono">
                    {update.remote_version ? `v${update.remote_version}` : update.remote || '—'}
                  </b>
                </span>
              </div>
              {update.error && <p className="mt-2 text-xs leading-5">{update.error}</p>}
            </div>
          )}

          {progress && (active || terminal) && (
            <div
              className={`rounded-xl border p-4 ${progress.state === 'failed' ? 'border-rose-400/25 bg-rose-400/[.05]' : progress.state === 'succeeded' ? 'border-emerald-400/25 bg-emerald-400/[.05]' : 'border-primary/25 bg-primary/[.04]'}`}
            >
              <div className="flex items-start gap-2">
                {active ? (
                  <LoaderCircle size={17} className="mt-0.5 animate-spin text-primary" />
                ) : progress.state === 'succeeded' ? (
                  <CheckCircle2 size={17} className="mt-0.5 text-emerald-300" />
                ) : (
                  <AlertTriangle size={17} className="mt-0.5 text-amber-300" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold">
                    {reconnecting
                      ? 'در انتظار اتصال دوباره به Node'
                      : phaseTitle[phase] ||
                        (terminal ? phaseTitle[progress.state || 'unknown'] : 'در حال بررسی وضعیت')}
                  </p>
                  <p
                    role="status"
                    aria-live="polite"
                    className="mt-1 break-words text-xs leading-5 text-muted-foreground"
                  >
                    {reconnecting
                      ? 'سرویس بعد از نصب یا بازگشت در حال بالا آمدن است؛ بررسی خودکار ادامه دارد.'
                      : progress.message ||
                        (active
                          ? 'مرحله و نتیجه از گزارش واقعی Update Helper خوانده می‌شود؛ درصد تخمینی نشان داده نمی‌شود.'
                          : '')}
                  </p>
                  {(progress.branch || progress.strategy || progress.build_mode) && (
                    <p className="mt-2 text-[10px] text-muted-foreground">
                      شاخه:{' '}
                      <b dir="ltr" className="font-mono">
                        {progress.branch || branch}
                      </b>{' '}
                      · روش:{' '}
                      <b dir="ltr" className="font-mono">
                        {progress.strategy === 'release' ? 'release' : progress.strategy === 'source' ? `source/${progress.build_mode || buildMode}` : strategy}
                      </b>
                    </p>
                  )}
                  {progress.job_id && (
                    <p
                      dir="ltr"
                      className="mt-1 truncate text-left font-mono text-[10px] text-muted-foreground"
                    >
                      Job {progress.job_id}
                    </p>
                  )}
                  {progress.updated_at && <p className="mt-1 text-[10px] text-muted-foreground">آخرین گزارش دریافتی از Node: {new Date(progress.updated_at * 1000).toLocaleTimeString('fa-IR')}</p>}
                </div>
              </div>
              {active && (
                <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-secondary">
                  <div className="h-full w-1/3 animate-pulse rounded-full bg-primary" />
                </div>
              )}
              {progress.operation && <ol aria-label="مراحل واقعی به‌روزرسانی Node" className="mt-4 grid gap-2 sm:grid-cols-2">
                {stageSequence.map(([stage, label], index) => {
                  const failedAtStage = progress.state === 'failed' && stage === stagePhase
                  const cancelledAtStage = progress.state === 'cancelled' && stage === stagePhase
                  const completed = stagesComplete || (activeStageIndex >= 0 && index < activeStageIndex) || ((failedAtStage || cancelledAtStage) && index < activeStageIndex)
                  const currentStage = !stagesComplete && index === activeStageIndex && !failedAtStage && !cancelledAtStage
                  return <li key={stage} className={`flex items-center gap-2 rounded-lg border px-2.5 py-2 text-[10px] leading-5 ${failedAtStage ? 'border-destructive/30 bg-destructive/[.06] text-destructive' : cancelledAtStage ? 'border-amber-400/25 bg-amber-400/[.05] text-amber-100' : completed ? 'border-emerald-400/20 bg-emerald-400/[.04] text-emerald-200' : currentStage ? 'border-primary/30 bg-primary/[.06] text-foreground' : 'border-border/60 text-muted-foreground'}`}>
                    {failedAtStage ? <AlertTriangle size={13}/> : completed ? <CheckCircle2 size={13}/> : currentStage ? <LoaderCircle size={13} className="animate-spin"/> : <span className="inline-block h-3 w-3 rounded-full border border-current/40"/>}
                    <span>{label}</span>
                  </li>
                })}
              </ol>}
              <div className="mt-3">
                <Button variant="outline" size="sm" onClick={() => void toggleLogs()} disabled={logsLoading}>
                  {logsLoading ? <LoaderCircle size={13} className="animate-spin"/> : <FileText size={13}/>}
                  {logsOpen ? 'بستن گزارش مراحل' : 'نمایش گزارش واقعی Node'}
                </Button>
                {logsOpen && <div className="mt-2 overflow-hidden rounded-lg border border-border/70 bg-black/35">
                  <div className="flex items-center justify-between gap-2 border-b border-border/60 px-3 py-2 text-[10px] text-muted-foreground"><span>گزارش آخرین عملیات به‌روزرسانی مقصد</span><Button size="sm" variant="ghost" className="h-7 px-2 text-[10px]" onClick={() => void refreshLogs()} disabled={logsLoading}><RefreshCw size={12}/>تازه‌سازی</Button></div>
                  {logsTruncated && <p className="px-3 pt-2 text-[10px] text-amber-200">فقط انتهای گزارش (حداکثر ۶۴ KiB) منتقل شده است.</p>}
                  <pre dir="ltr" className="max-h-64 overflow-auto whitespace-pre-wrap break-words p-3 text-left font-mono text-[10px] leading-5 text-foreground">{logsLoading ? 'در حال دریافت گزارش از Node…' : logs || 'گزارش خالی است.'}</pre>
                </div>}
              </div>
            </div>
          )}

          {error && (
            <p
              role="alert"
              className="rounded-xl border border-rose-400/20 bg-rose-400/[.05] p-3 text-xs leading-5 text-rose-100"
            >
              {error}
            </p>
          )}

          {confirmAction && (
            <section
              className="space-y-3 rounded-xl border border-amber-400/25 bg-amber-400/[.05] p-4"
              aria-label="تأیید عملیات به‌روزرسانی"
            >
              <div className="flex items-start gap-2">
                <AlertTriangle size={17} className="mt-0.5 shrink-0 text-amber-300" />
                <div>
                  <p className="text-sm font-semibold">
                    {confirmAction === 'update'
                      ? 'مرور و تأیید به‌روزرسانی'
                      : confirmAction === 'cancel'
                        ? 'تأیید لغو امن'
                        : 'تأیید بازگشت نسخه'}
                  </p>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">
                    {confirmAction === 'update'
                      ? `روی «${node?.public_name || node?.name}» نسخهٔ ${update?.remote_version || update?.remote || 'جدید'} از شاخهٔ ${branch} با روش ${strategy === 'release' ? 'دریافت Release آماده' : `کامپایل از سورس (${buildMode})`} نصب می‌شود. در زمان Restart ارتباط ممکن است موقتاً قطع شود؛ کلید و شناسهٔ Node حفظ می‌شوند.`
                      : confirmAction === 'cancel'
                        ? 'لغو از Update Helper مقصد درخواست می‌شود. اگر جایگزینی فایل شروع شده باشد، نسخهٔ قبلی و snapshot دیتابیس بازیابی و سلامت سرویس بررسی می‌شوند.'
                        : 'نسخهٔ قبلی و snapshot دیتابیس آن روی مقصد بازیابی می‌شوند؛ داده‌های پایگاه‌داده پس از snapshot ممکن است از بین بروند.'}
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap justify-end gap-2">
                <Button variant="ghost" onClick={() => setConfirmAction(null)} disabled={!!busy}>
                  بازگشت
                </Button>
                <Button
                  variant={confirmAction === 'cancel' ? 'destructive' : 'default'}
                  onClick={() => {
                    if (confirmAction === 'update') void startUpdate()
                    else if (confirmAction === 'cancel') void cancelUpdate()
                    else void rollbackUpdate()
                  }}
                  disabled={!!busy}
                >
                  {busy ? (
                    <LoaderCircle size={14} className="animate-spin" />
                  ) : confirmAction === 'update' ? (
                    <Download size={14} />
                  ) : confirmAction === 'cancel' ? (
                    <X size={14} />
                  ) : (
                    <RotateCw size={14} />
                  )}{' '}
                  {confirmAction === 'update'
                    ? 'شروع به‌روزرسانی'
                    : confirmAction === 'cancel'
                      ? 'تأیید لغو امن'
                      : 'بازگشت و بررسی سلامت'}
                </Button>
              </div>
            </section>
          )}

          <div className="flex flex-wrap justify-end gap-2">
            <Button
              variant="outline"
              onClick={() => {
                if (nodeId != null) void refreshCheck(nodeId, branch)
              }}
              disabled={checking || !!busy}
            >
              <RefreshCw size={14} className={checking ? 'animate-spin' : ''} />
              بررسی وضعیت
            </Button>
            {active && (
              <Button variant="destructive" onClick={() => setConfirmAction('cancel')} disabled={!!busy}>
                <X size={14} />
                لغو امن
              </Button>
            )}
            {progress?.state === 'failed' && check?.supported && (
              <Button variant="outline" onClick={() => setConfirmAction('rollback')} disabled={!!busy}>
                <RotateCw size={14} />
                بازگشت به نسخهٔ قبل
              </Button>
            )}
            {check?.supported && !active && update?.status === 'available' && (
              <Button onClick={() => setConfirmAction('update')} disabled={checking || !!busy}>
                <Download size={14} />
                {`به‌روزرسانی ${branchLabel(branch)}`}
              </Button>
            )}
            <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={!!busy}>
              بستن
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
