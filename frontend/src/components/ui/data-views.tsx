import type { ReactNode } from 'react'
import { Search } from 'lucide-react'
import { cn } from '../../lib/utils'
import { Input } from './input'

export function FilterBar({ value, onChange, placeholder, children, className }: { value: string; onChange: (value: string) => void; placeholder: string; children?: ReactNode; className?: string }) {
  return <div className={cn('flex flex-col gap-2 rounded-xl border border-border/80 bg-background/35 p-2.5 sm:flex-row sm:items-center', className)}>
    <label className="relative min-w-0 flex-1">
      <span className="sr-only">جست‌وجو و پالایش</span>
      <Search size={15} aria-hidden="true" className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"/>
      <Input value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} className="h-10 pr-9"/>
    </label>
    {children && <div className="flex flex-wrap gap-2">{children}</div>}
  </div>
}

export function EmptyState({ icon, title, description, action, className }: { icon?: ReactNode; title: string; description?: string; action?: ReactNode; className?: string }) {
  return <div className={cn('grid min-h-40 place-items-center rounded-xl border border-dashed border-border/80 p-6 text-center', className)}>
    <div className="max-w-sm">
      {icon && <div aria-hidden="true" className="mx-auto mb-3 grid h-10 w-10 place-items-center rounded-xl border border-border bg-background/50 text-muted-foreground">{icon}</div>}
      <h3 className="text-sm font-semibold text-foreground">{title}</h3>
      {description && <p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>}
      {action && <div className="mt-3">{action}</div>}
    </div>
  </div>
}
