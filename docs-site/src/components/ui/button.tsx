import { forwardRef, type ButtonHTMLAttributes } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'
import { cn } from './utils'
const styles = cva('inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary disabled:pointer-events-none disabled:opacity-45', {
  variants: { variant: { default: 'bg-primary text-[#15191d] hover:bg-[#e8b888]', outline: 'border border-border bg-card/70 hover:border-primary/40 hover:bg-accent', ghost: 'hover:bg-accent hover:text-foreground' }, size: { default: 'h-10 px-4', icon: 'h-10 w-10', sm: 'h-8 px-3 text-xs' } },
  defaultVariants: { variant: 'default', size: 'default' },
})
type Props = ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof styles>
export const Button = forwardRef<HTMLButtonElement, Props>(({ className, variant, size, ...props }, ref) => <button ref={ref} className={cn(styles({ variant, size }), className)} {...props} />)
Button.displayName = 'Button'
