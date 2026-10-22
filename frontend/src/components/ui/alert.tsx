import * as React from 'react'
import { cn } from '../../lib/utils'
function Alert({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      role="alert"
      className={cn(
        'relative w-full rounded-xl border border-border bg-secondary/65 p-4 text-sm leading-7',
        className,
      )}
      {...props}
    />
  )
}
function AlertTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return <h5 className={cn('mb-1 font-semibold', className)} {...props} />
}
function AlertDescription({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('text-muted-foreground', className)} {...props} />
}
export { Alert, AlertTitle, AlertDescription }
