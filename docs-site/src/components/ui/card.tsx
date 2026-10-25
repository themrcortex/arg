import type { HTMLAttributes } from 'react'
import { cn } from './utils'
export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) { return <div className={cn('rounded-2xl border border-border bg-card text-foreground shadow-[0_12px_40px_rgba(0,0,0,.12)]', className)} {...props} /> }
export function CardHeader({ className, ...props }: HTMLAttributes<HTMLDivElement>) { return <div className={cn('flex flex-col gap-1.5 p-5', className)} {...props} /> }
export function CardTitle({ className, ...props }: HTMLAttributes<HTMLHeadingElement>) { return <h3 className={cn('text-base font-semibold tracking-tight', className)} {...props} /> }
export function CardContent({ className, ...props }: HTMLAttributes<HTMLDivElement>) { return <div className={cn('p-5 pt-0', className)} {...props} /> }
