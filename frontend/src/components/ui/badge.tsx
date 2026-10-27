import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '../../lib/utils'
const badgeVariants = cva(
  'inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs font-medium',
  {
    variants: {
      variant: {
        default: 'border-primary/30 bg-primary/10 text-primary',
        success: 'arg-status-success',
        warning: 'arg-status-warning',
        info: 'arg-status-info',
        outline: 'border-border bg-secondary/60 text-muted-foreground',
        destructive: 'arg-status-danger',
      },
      size: {
        default: 'px-2.5 py-1 text-xs',
        sm: 'px-2 py-0.5 text-[10px]',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
)
type Props = React.HTMLAttributes<HTMLDivElement> & VariantProps<typeof badgeVariants>
function Badge({ className, variant, ...props }: Props) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />
}
export { Badge }
