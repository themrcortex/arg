import * as React from 'react'
import { cn } from '../../lib/utils'
type CardProps = React.HTMLAttributes<HTMLDivElement> & { surface?: 'standard' | 'elevated' | 'subtle' | 'flat' }
const Card = React.forwardRef<HTMLDivElement, CardProps>(
  ({ className, surface = 'standard', ...props }, ref) => (
    <div
      ref={ref}
      data-surface={surface}
      className={cn('arg-card-surface rounded-2xl border border-border bg-card text-card-foreground', `arg-card-surface-${surface}`, className)}
      {...props}
    />
  ),
)
Card.displayName = 'Card'
const GlassCard = Card
const CardHeader = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} className={cn('flex flex-col space-y-1.5 p-6 pb-4', className)} {...props} />
  ),
)
CardHeader.displayName = 'CardHeader'
const CardTitle = React.forwardRef<HTMLHeadingElement, React.HTMLAttributes<HTMLHeadingElement>>(
  ({ className, ...props }, ref) => (
    <h3 ref={ref} className={cn('text-base font-bold tracking-tight', className)} {...props} />
  ),
)
CardTitle.displayName = 'CardTitle'
const CardDescription = React.forwardRef<HTMLParagraphElement, React.HTMLAttributes<HTMLParagraphElement>>(
  ({ className, ...props }, ref) => (
    <p ref={ref} className={cn('text-sm text-muted-foreground', className)} {...props} />
  ),
)
CardDescription.displayName = 'CardDescription'
const CardContent = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => <div ref={ref} className={cn('p-6 pt-0', className)} {...props} />,
)
CardContent.displayName = 'CardContent'
export { Card, GlassCard, CardHeader, CardTitle, CardDescription, CardContent }
