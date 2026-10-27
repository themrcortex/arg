import { Check, CircleAlert, Minus } from 'lucide-react'
import { cn } from '../../lib/utils'

export type StepStatus = 'pending' | 'active' | 'completed' | 'error' | 'skipped'
export type StepDefinition = { id: string; title: string; description?: string }

type UnifiedStepperProps = {
  steps: StepDefinition[]
  /** Zero-based index of the current step. */
  currentStep: number
  statuses?: StepStatus[]
  orientation?: 'horizontal' | 'vertical'
  compact?: boolean
  className?: string
  ariaLabel?: string
  progressLabel?: string
}

const stateClass: Record<StepStatus, string> = {
  pending: 'border-border bg-background/50 text-muted-foreground',
  active: 'border-primary/55 bg-primary/15 text-primary shadow-[0_0_0_3px_color-mix(in_srgb,var(--arg-accent)_12%,transparent)]',
  completed: 'border-emerald-500/35 bg-emerald-500/10 text-emerald-600 dark:text-emerald-300',
  error: 'border-destructive/40 bg-destructive/10 text-destructive',
  skipped: 'border-border/70 bg-muted/50 text-muted-foreground',
}

/** Shared, theme-aware step indicator for real multi-step and server-reported flows. */
export function UnifiedStepper({ steps, currentStep, statuses, orientation = 'horizontal', compact = false, className, ariaLabel = 'مراحل عملیات', progressLabel }: UnifiedStepperProps) {
  if (!steps.length) return null
  const activeIndex = Math.max(0, Math.min(currentStep, steps.length - 1))
  const statusFor = (index: number): StepStatus => statuses?.[index] || (index < activeIndex ? 'completed' : index === activeIndex ? 'active' : 'pending')
  const completedCount = steps.filter((_, index) => statusFor(index) === 'completed' || statusFor(index) === 'skipped').length
  const activeStep = steps[activeIndex]

  return <div className={cn('w-full', className)}>
    <div
      className={cn('grid gap-x-2 gap-y-3', orientation === 'horizontal' ? 'grid-flow-col auto-cols-fr' : 'grid-cols-1')}
      role={progressLabel ? 'progressbar' : 'list'}
      aria-label={ariaLabel}
      aria-valuemin={progressLabel ? 0 : undefined}
      aria-valuemax={progressLabel ? steps.length : undefined}
      aria-valuenow={progressLabel ? Math.min(steps.length, completedCount + (statusFor(activeIndex) === 'active' ? 1 : 0)) : undefined}
      aria-valuetext={progressLabel ? `${progressLabel}: ${activeStep.title}، مرحلهٔ ${activeIndex + 1} از ${steps.length}` : undefined}
    >
      {steps.map((step, index) => {
        const status = statusFor(index)
        const Icon = status === 'completed' ? Check : status === 'error' ? CircleAlert : status === 'skipped' ? Minus : null
        return <div key={step.id} role={progressLabel ? undefined : 'listitem'} aria-current={status === 'active' ? 'step' : undefined} className={cn('min-w-0', orientation === 'vertical' && 'flex items-start gap-3')}>
          <div className={cn('flex min-w-0 items-center', orientation === 'vertical' ? 'flex-col' : 'flex-col')}>
            <div className="flex w-full items-center">
              {index > 0 && orientation === 'horizontal' && <span aria-hidden="true" className={cn('h-px flex-1', index <= completedCount ? 'bg-emerald-500/45' : 'bg-border/70')}/>}
              <span className={cn('grid h-8 w-8 shrink-0 place-items-center rounded-full border text-xs font-semibold transition-colors duration-200', stateClass[status])}>
                {Icon ? <Icon size={15} aria-hidden="true"/> : <span aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>}
                <span className="sr-only">{status === 'active' ? 'مرحلهٔ جاری' : status === 'completed' ? 'انجام‌شده' : status === 'error' ? 'خطا' : status === 'skipped' ? 'ردشده' : 'در انتظار'}</span>
              </span>
              {index < steps.length - 1 && orientation === 'horizontal' && <span aria-hidden="true" className={cn('h-px flex-1', index < completedCount ? 'bg-emerald-500/45' : 'bg-border/70')}/>}
            </div>
            {!compact && orientation === 'horizontal' && <div className="mt-2 max-w-full text-center">
              <div className={cn('truncate text-[11px] font-medium', status === 'active' ? 'text-foreground' : status === 'completed' ? 'text-emerald-700 dark:text-emerald-300' : 'text-muted-foreground')}>{step.title}</div>
              {step.description && <p className="mt-0.5 hidden text-[10px] leading-4 text-muted-foreground sm:block">{step.description}</p>}
            </div>}
          </div>
          {orientation === 'vertical' && <div className="min-w-0 pb-4 pt-1">
            <div className={cn('text-sm font-medium', status === 'active' ? 'text-foreground' : 'text-muted-foreground')}>{step.title}</div>
            {step.description && <p className="mt-1 text-xs leading-5 text-muted-foreground">{step.description}</p>}
          </div>}
        </div>
      })}
    </div>
    {compact && <p className="mt-2 text-center text-xs font-medium text-foreground" aria-live="polite">{activeStep.title}<span className="mx-1 text-muted-foreground">·</span><span className="text-muted-foreground">{activeIndex + 1} / {steps.length}</span></p>}
  </div>
}
