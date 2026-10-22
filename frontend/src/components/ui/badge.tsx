import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from '../../lib/utils'
const badgeVariants = cva(
  'inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium',
  {
    variants: {
      variant: {
        default: 'border-primary/30 bg-primary/15 text-[#c9aaff]',
        success: 'border-emerald-400/25 bg-emerald-400/10 text-emerald-300',
        warning: 'border-amber-400/25 bg-amber-400/10 text-amber-300',
        outline: 'border-border bg-secondary/60 text-muted-foreground',
        destructive: 'border-rose-400/25 bg-rose-400/10 text-rose-300',
      },
    },
    defaultVariants: { variant: 'default' },
  },
)
type Props = React.HTMLAttributes<HTMLDivElement> & VariantProps<typeof badgeVariants>
function Badge({ className, variant, ...props }: Props) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />
}
export { Badge }
