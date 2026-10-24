import { cn } from '../../lib/utils'
function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('arg-skeleton rounded-xl', className)} {...props} />
}
export { Skeleton }
