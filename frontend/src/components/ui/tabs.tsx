import * as React from 'react'
import * as TabsPrimitive from '@radix-ui/react-tabs'
import { cn } from '../../lib/utils'
type TabsProps = React.ComponentPropsWithoutRef<typeof TabsPrimitive.Root>
export const tabsListClassName =
  'arg-tabs-list flex h-12 w-full min-w-0 items-center justify-start gap-1 overflow-x-auto rounded-xl border border-border/80 bg-card/55 p-1.5 text-muted-foreground'
export const tabsTriggerClassName =
  'arg-tabs-trigger inline-flex h-10 min-w-max shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-lg border border-transparent px-3 text-sm font-medium transition-colors hover:bg-white/[.045] hover:text-foreground data-[state=active]:border-primary/20 data-[state=active]:bg-primary/[.12] data-[state=active]:text-foreground data-[state=active]:shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50'
const Tabs = ({ className, ...props }: TabsProps) => (
  <TabsPrimitive.Root className={cn('arg-tabs-root w-full min-w-0 space-y-4', className)} {...props} />
)
const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.List
    ref={ref}
    className={cn(
      tabsListClassName,
      className,
    )}
    {...props}
  />
))
TabsList.displayName = 'TabsList'
const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Trigger
    ref={ref}
    className={cn(
      tabsTriggerClassName,
      className,
    )}
    {...props}
  />
))
TabsTrigger.displayName = 'TabsTrigger'
const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Content ref={ref} className={cn('arg-tabs-content min-w-0 focus-visible:outline-none', className)} {...props} />
))
TabsContent.displayName = 'TabsContent'
export { Tabs, TabsList, TabsTrigger, TabsContent }
