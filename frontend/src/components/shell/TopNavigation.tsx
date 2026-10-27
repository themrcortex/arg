import type { ReactNode } from 'react'

export function TopNavigation({ leading, breadcrumb, actions }: { leading: ReactNode; breadcrumb: ReactNode; actions: ReactNode }) {
  return (
    <header className="utility-bar sticky top-0 z-20 flex min-h-[62px] items-center justify-between gap-3 border-b px-3 sm:px-5 md:px-8">
      <div className="flex min-w-0 items-center gap-2 sm:gap-3">
        {leading}
        <div className="hidden min-w-0 items-center gap-2 border-r border-border/70 pr-3 text-xs sm:flex" aria-label="مسیر صفحه">
          {breadcrumb}
        </div>
      </div>
      <div className="flex min-w-0 max-w-[72vw] items-center gap-1 overflow-x-auto sm:max-w-[66vw] sm:gap-1.5">{actions}</div>
    </header>
  )
}
