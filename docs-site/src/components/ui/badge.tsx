import type { HTMLAttributes } from 'react'
import { cn } from './utils'
export function Badge({ className, ...props }: HTMLAttributes<HTMLSpanElement>) { return <span className={cn('inline-flex items-center rounded-full border border-primary/20 bg-primary/[.08] px-2.5 py-1 text-[11px] font-medium text-primary', className)} {...props} /> }
