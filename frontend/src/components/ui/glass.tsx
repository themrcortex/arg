import * as React from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './dialog'
import { cn } from '../../lib/utils'

const panelVariants = cva('arg-glass-panel min-w-0 rounded-2xl border border-border', {
  variants: {
    variant: {
      standard: 'bg-card/70',
      elevated: 'arg-glass-panel-elevated bg-card/85',
      subtle: 'arg-glass-panel-subtle bg-card/45',
      flat: 'bg-transparent shadow-none backdrop-blur-none',
    },
    padding: {
      none: 'p-0',
      sm: 'p-3',
      md: 'p-4 md:p-5',
      lg: 'p-5 md:p-6',
    },
  },
  defaultVariants: { variant: 'standard', padding: 'md' },
})

type GlassPanelProps = React.HTMLAttributes<HTMLDivElement> & VariantProps<typeof panelVariants>
export const GlassPanel = React.forwardRef<HTMLDivElement, GlassPanelProps>(
  ({ className, variant, padding, ...props }, ref) => (
    <div ref={ref} className={cn(panelVariants({ variant, padding }), className)} {...props} />
  ),
)
GlassPanel.displayName = 'GlassPanel'

type GlassDialogProps = React.ComponentPropsWithoutRef<typeof DialogContent>
export const GlassDialog = React.forwardRef<React.ElementRef<typeof DialogContent>, GlassDialogProps>(
  ({ className, ...props }, ref) => <DialogContent ref={ref} className={cn('arg-glass-dialog', className)} {...props} />,
)
GlassDialog.displayName = 'GlassDialog'

type GlassDrawerProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description?: string
  side?: 'right' | 'left' | 'bottom'
  className?: string
  children: React.ReactNode
}

export function GlassDrawer({ open, onOpenChange, title, description, side = 'right', className, children }: GlassDrawerProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={cn('arg-glass-drawer', `arg-glass-drawer-${side}`, className)}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <div className="min-w-0">{children}</div>
      </DialogContent>
    </Dialog>
  )
}
