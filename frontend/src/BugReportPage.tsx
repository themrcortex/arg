import { useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { Bug, Copy, ExternalLink, Github, Info, TriangleAlert } from 'lucide-react'
import { getLastApiFailure, type ApiFailureReport } from './lib/api'
import { Button } from './components/ui/button'
import { Input } from './components/ui/input'
import { Label } from './components/ui/label'
import { Badge } from './components/ui/badge'

type Props = {
  Heading: (props: { title: string; description: string; action?: ReactNode }) => ReactNode
  notify: (message: string) => void
  version?: string
  commit?: string
  page: string
}

function reportBody(input: { description: string; steps: string; expected: string; actual: string; environment: string; failure: ApiFailureReport | null; includeDiagnostic: boolean }) {
  const report = [
    '## توضیح مشکل', input.description.trim() || 'توضیحات وارد نشده است.',
    '## مراحل بازتولید', input.steps.trim() || 'مراحل وارد نشده است.',
    '## نتیجهٔ مورد انتظار', input.expected.trim() || 'وارد نشده است.',
    '## نتیجهٔ فعلی', input.actual.trim() || 'وارد نشده است.',
    '## اطلاعات محیط', input.environment,
  ]
  if (input.failure) {
    report.push('## آخرین خطای ثبت‌شده در این نشست', `- مسیر API: \`${input.failure.path}\``, `- HTTP: ${input.failure.status || 'اتصال برقرار نشد'}`, `- پیام: ${input.failure.message}`)
    if (input.includeDiagnostic && input.failure.diagnostic) {
      report.push('### جزئیات تشخیصی', '```json', JSON.stringify(input.failure.diagnostic, null, 2).slice(0, 5000), '```')
    }
  }
  return report.join('\n\n').slice(0, 9000)
}

export function BugReportPage({ Heading, notify, version, commit, page }: Props) {
  const failure = getLastApiFailure()
  const [kind, setKind] = useState<'bug' | 'improvement'>('bug')
  const [title, setTitle] = useState(failure ? `خطا: ${failure.message.slice(0, 110)}` : '')
  const [description, setDescription] = useState('')
  const [steps, setSteps] = useState('')
  const [expected, setExpected] = useState('')
  const [actual, setActual] = useState(failure?.message || '')
  const [includeDiagnostic, setIncludeDiagnostic] = useState(false)
  const [includeEnvironment, setIncludeEnvironment] = useState(true)

  const environment = useMemo(() => [
    `نسخه ARG: ${version || 'نامشخص'}`,
    `Commit: ${commit || 'نامشخص'}`,
    `صفحه: ${page || 'نامشخص'}`,
    `مرورگر: ${navigator.userAgent.slice(0, 300)}`,
    `زمان: ${new Date().toISOString()}`,
  ].join('\n'), [version, commit, page])

  const body = reportBody({ description, steps, expected, actual, environment: includeEnvironment ? environment : 'اطلاعات محیط ارسال نشد.', failure, includeDiagnostic })

  function validate() {
    if (title.trim().length < 8) { notify('عنوان گزارش باید دست‌کم ۸ نویسه باشد.'); return false }
    if (description.trim().length < 15) { notify('شرح کوتاهی از باگ بنویس تا قابل بررسی باشد.'); return false }
    return true
  }

  function openGitHub(event: FormEvent) {
    event.preventDefault()
    if (!validate()) return
    const prefix = kind === 'bug' ? '[Bug] ' : '[Improvement] '
    const query = new URLSearchParams({ title: `${prefix}${title.trim()}`.slice(0, 180), body })
    window.open(`https://github.com/themrcortex/arg/issues/new?${query.toString()}`, '_blank', 'noopener,noreferrer')
    notify('فرم گزارش در GitHub باز شد؛ متن و اطلاعات حساس را یک‌بار مرور و سپس ارسال کن.')
  }

  async function copyReport() {
    if (!validate()) return
    const prefix = kind === 'bug' ? '[Bug] ' : '[Improvement] '
    try { await navigator.clipboard.writeText(`## ${prefix}${title.trim()}\n\n${body}`); notify('متن گزارش کپی شد؛ می‌توانی آن را در GitHub جای‌گذاری کنی.') }
    catch { notify('کپی خودکار در دسترس نیست؛ از پیش‌نمایش گزارش استفاده کن.') }
  }

  return <>
    <Heading title="خطا و پیشنهاد بهبود" description="باگ یا درخواست قابلیت را آماده کن؛ ARG پیش‌نویس Issue مخزن رسمی را باز می‌کند." action={<a href="https://github.com/themrcortex/arg/issues" target="_blank" rel="noopener noreferrer" className="inline-flex h-9 items-center gap-2 rounded-lg border border-border px-3 text-xs text-foreground hover:bg-accent"><Github size={15}/>گزارش‌های GitHub<ExternalLink size={13}/></a>}/>
    <div className="grid gap-5 xl:grid-cols-[minmax(0,1.1fr)_minmax(280px,.9fr)]">
      <form onSubmit={openGitHub} className="space-y-4 rounded-2xl border border-border/80 bg-card/55 p-4 sm:p-5">
        <div className="flex items-start gap-3 rounded-xl border border-primary/15 bg-primary/[.035] p-3 text-xs leading-5 text-muted-foreground"><Bug size={17} className="mt-0.5 shrink-0 text-primary"/><p>گزارش به شکل پیش‌نویس در GitHub باز می‌شود و تا وقتی خودت آنجا Submit نکنی، برای مخزن ارسال نمی‌شود. ورود به حساب GitHub لازم است.</p></div>
        <div className="space-y-2"><Label htmlFor="report-kind">نوع گزارش</Label><select id="report-kind" value={kind} onChange={(event) => setKind(event.target.value as 'bug' | 'improvement')} className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm text-foreground"><option value="bug">باگ یا خطای فنی</option><option value="improvement">پیشنهاد قابلیت یا بهبود</option></select></div>
        <div className="space-y-2"><Label htmlFor="bug-title">عنوان کوتاه باگ</Label><Input id="bug-title" maxLength={180} required minLength={8} value={title} onChange={(event) => setTitle(event.target.value)} placeholder="مثلاً: خواندن مصرف سرویس خطای parse می‌دهد"/></div>
        <div className="space-y-2"><Label htmlFor="bug-description">شرح مشکل</Label><textarea id="bug-description" required minLength={15} maxLength={4000} value={description} onChange={(event) => setDescription(event.target.value)} placeholder="چه کاری انجام دادی و چه اتفاقی افتاد؟" className="min-h-28 w-full resize-y rounded-lg border border-input bg-background/70 px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"/></div>
        <div className="grid gap-3 sm:grid-cols-2"><div className="space-y-2"><Label htmlFor="bug-steps">مراحل بازتولید</Label><textarea id="bug-steps" maxLength={2500} value={steps} onChange={(event) => setSteps(event.target.value)} placeholder={'۱. وارد صفحه شوید\n۲. روی دکمه بزنید'} className="min-h-24 w-full resize-y rounded-lg border border-input bg-background/70 px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"/></div><div className="space-y-2"><Label htmlFor="bug-actual">نتیجهٔ فعلی یا متن خطا</Label><textarea id="bug-actual" maxLength={2500} value={actual} onChange={(event) => setActual(event.target.value)} placeholder="پیام دقیق یا نتیجه‌ای که دیدی" className="min-h-24 w-full resize-y rounded-lg border border-input bg-background/70 px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"/></div></div>
        <div className="space-y-2"><Label htmlFor="bug-expected">نتیجهٔ مورد انتظار</Label><textarea id="bug-expected" maxLength={1500} value={expected} onChange={(event) => setExpected(event.target.value)} placeholder="انتظار داشتی چه اتفاقی بیفتد؟" className="min-h-20 w-full resize-y rounded-lg border border-input bg-background/70 px-3 py-2 text-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"/></div>
        {failure?.diagnostic && <label className="flex cursor-pointer items-start gap-2 rounded-lg border border-border p-3 text-xs leading-5 text-muted-foreground"><input type="checkbox" checked={includeDiagnostic} onChange={(event) => setIncludeDiagnostic(event.target.checked)} className="mt-1 accent-primary"/><span>جزئیات تشخیصی خطای اخیر هم به پیش‌نویس اضافه شود. ممکن است مسیر فایل، نام سرویس یا مقدار خام دیده شود؛ پیش‌نمایش را بررسی کن.</span></label>}
        <label className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground"><input type="checkbox" checked={includeEnvironment} onChange={(event) => setIncludeEnvironment(event.target.checked)} className="accent-primary"/>نسخه، commit، صفحه و مشخصات مرورگر در گزارش باشند</label>
        <div className="flex flex-wrap gap-2"><Button type="submit"><Github size={15}/>ادامه در GitHub<ExternalLink size={14}/></Button><Button type="button" variant="outline" onClick={() => void copyReport()}><Copy size={14}/>کپی متن گزارش</Button></div>
      </form>

      <section className="min-w-0 space-y-3 rounded-2xl border border-border/80 bg-card/35 p-4 sm:p-5"><div className="flex items-center justify-between gap-2"><h2 className="text-sm font-semibold text-foreground">پیش‌نمایش ارسالی</h2><Badge variant="outline">{body.length} نویسه</Badge></div><div className="flex items-start gap-2 rounded-lg border border-amber-300/15 bg-amber-300/[.035] p-3 text-[11px] leading-5 text-amber-100"><TriangleAlert size={15} className="mt-0.5 shrink-0"/>قبل از ارسال، گزارش را برای رمز، توکن، آدرس خصوصی و اطلاعات مشتری بررسی کن. ARG این متن را خودکار به GitHub ارسال نمی‌کند.</div><div className="rounded-lg border border-border bg-background/60 p-3"><p className="mb-2 text-xs font-semibold text-foreground">{title || 'عنوان گزارش'}</p><pre dir="auto" className="max-h-[560px] overflow-auto whitespace-pre-wrap break-words text-xs leading-6 text-muted-foreground">{body}</pre></div>{failure && <p className="text-[11px] text-muted-foreground">آخرین خطا از <code dir="ltr">{failure.path}</code> · HTTP {failure.status || 'network'}</p>}<div className="flex items-start gap-2 text-[11px] leading-5 text-muted-foreground"><Info size={14} className="mt-0.5 shrink-0"/>پس از بازشدن GitHub، Submit نهایی را خودت انجام می‌دهی. این پنل نیازی به GitHub token یا دسترسی نوشتن مخزن ندارد.</div></section>
    </div>
  </>
}
