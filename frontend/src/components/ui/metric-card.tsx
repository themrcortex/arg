import type { ReactNode } from 'react'
import { cn } from '../../lib/utils'

export type MetricTone = 'copper' | 'green' | 'teal' | 'blue' | 'warning'

type MetricCardProps = {
  label: string
  value: string
  detail: ReactNode
  icon?: ReactNode
  progress?: number
  tone?: MetricTone
  direction?: 'ltr' | 'rtl'
  className?: string
}

export function MetricCard({ label, value, detail, icon, progress, tone = 'copper', direction = 'ltr', className }: MetricCardProps) {
  const hasProgress = typeof progress === 'number' && Number.isFinite(progress)
  const boundedProgress = hasProgress ? Math.max(0, Math.min(100, progress)) : 0

  return (
    <section className={cn('arg-metric-card min-w-0 rounded-2xl border p-4 sm:p-5', className)} data-tone={tone} aria-label={label}>
      <div className="flex items-start justify-between gap-3">
        <span className="truncate text-xs font-medium text-muted-foreground">{label}</span>
        {icon && <span className="arg-metric-icon grid h-10 w-10 shrink-0 place-items-center rounded-xl border">{icon}</span>}
      </div>
      <div className="mt-4 truncate text-[26px] font-bold leading-tight tracking-tight text-foreground tabular-nums sm:text-[28px]" dir={direction} title={value}>{value}</div>
      <div className="mt-1.5 min-h-5 truncate text-xs text-muted-foreground" title={typeof detail === 'string' ? detail : undefined}>{detail}</div>
      {hasProgress && <div className="arg-metric-track mt-4 h-1.5 overflow-hidden rounded-full" role="progressbar" aria-label={`مصرف ${label}`} aria-valuenow={Math.round(boundedProgress)} aria-valuemin={0} aria-valuemax={100}><div className="arg-metric-fill h-full rounded-full transition-[width] duration-300" style={{ width: `${boundedProgress}%` }}/></div>}
    </section>
  )
}
