import type { RefObject } from 'react'
import { ChevronLeft, ChevronRight, X } from 'lucide-react'
import { cn } from '../../lib/utils'
import { Button } from '../ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../ui/tooltip'
import type { NavigationItem, PageId } from '../../navigation'

type SidebarNavigationProps = {
  items: NavigationItem[]
  currentPage: PageId
  collapsed: boolean
  mobileOpen: boolean
  disabled: boolean
  mobileCloseButton: RefObject<HTMLButtonElement | null>
  onSelectPage: (page: PageId) => void
  onToggleCollapsed: () => void
  onCloseMobile: () => void
}

const sections: Array<{ title: string; ids: PageId[] }> = [
  { title: 'نمای کلی', ids: ['dashboard'] },
  { title: 'سرور و شبکه', ids: ['system', 'network', 'ports', 'firewall', 'nodes'] },
  { title: 'اپلیکیشن‌ها و سرویس‌ها', ids: ['apps', 'nginx', 'docker', 'files', 's3', 'dns'] },
  { title: 'عملیات سرور', ids: ['terminal', 'packages', 'services', 'mysql', 'maintenance'] },
  { title: 'مدیریت ARG', ids: ['users', 'audit', 'settings', 'account', 'bug-report'] },
]

export function SidebarNavigation({ items, currentPage, collapsed, mobileOpen, disabled, mobileCloseButton, onSelectPage, onToggleCollapsed, onCloseMobile }: SidebarNavigationProps) {
  return (
    <aside
      id="arg-sidebar"
      inert={disabled}
      aria-hidden={disabled}
      role={mobileOpen ? 'dialog' : undefined}
      aria-modal={mobileOpen || undefined}
      aria-label="ناوبری اصلی"
      tabIndex={-1}
      className={cn(
        'arg-sidebar fixed inset-y-0 right-0 z-40 flex w-[min(80vw,280px)] max-w-[100vw] flex-col border-l px-3 py-3 transition-transform lg:sticky lg:inset-x-auto lg:top-0 lg:right-auto lg:bottom-auto lg:m-0 lg:h-dvh lg:translate-x-0',
        collapsed ? 'arg-sidebar-collapsed lg:w-[56px]' : 'lg:w-[224px]',
        mobileOpen ? 'translate-x-0' : 'translate-x-[calc(100%+1rem)]',
      )}
    >
      <div className="arg-sidebar-pattern" aria-hidden="true" />
      <div className="arg-sidebar-corner" aria-hidden="true" />
      <div className="arg-brand-row relative z-10 flex min-h-[72px] items-center justify-between gap-1">
        <div className="arg-brand-lockup flex min-w-0 flex-1 items-center" aria-label="ARG">
          <img src="/arg/brand/arg-logo.png" alt="ARG" className="arg-brand-full" />
          <img src="/arg/brand/arg-logo-mark.png" alt="" aria-hidden="true" className="arg-brand-mark" />
        </div>
        <Button variant="ghost" size="icon" className="arg-collapse hidden h-8 w-8 shrink-0 lg:inline-flex" onClick={onToggleCollapsed} aria-label={collapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'} aria-expanded={!collapsed} title={collapsed ? 'باز کردن سایدبار' : 'جمع کردن سایدبار'}>{collapsed ? <ChevronLeft size={18} /> : <ChevronRight size={18} />}</Button>
        <Button ref={mobileCloseButton} variant="ghost" size="icon" className="arg-mobile-close h-8 w-8 shrink-0 lg:hidden" onClick={onCloseMobile} aria-label="بستن منو"><X size={18}/></Button>
      </div>
      <div className="arg-divider relative z-10 mx-2 my-2" aria-hidden="true" />
      <div className="arg-navigation relative z-10 flex-1 space-y-4 overflow-y-auto px-1 pb-3">
        {sections.map((section) => {
          const sectionItems = section.ids.flatMap((id) => {
            const item = items.find((candidate) => candidate.id === id)
            return item ? [item] : []
          })
          if (!sectionItems.length) return null
          return <nav key={section.title} aria-label={section.title}>
            <div className="arg-section-label mb-1.5 px-3">{section.title}</div>
            <TooltipProvider delayDuration={220}>
              <div className="space-y-1">{sectionItems.map((item) => {
                const button = <button key={item.id} onClick={() => onSelectPage(item.id)} aria-current={currentPage === item.id ? 'page' : undefined} aria-label={collapsed ? item.label : undefined} className={cn('arg-nav-item group flex min-h-[42px] w-full items-center gap-3 rounded-xl px-3 text-right text-[13px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring', currentPage === item.id ? 'is-active' : '')}><item.icon className="arg-nav-icon shrink-0" size={20} strokeWidth={1.8}/><span className="arg-nav-label flex-1">{item.label}</span></button>
                return collapsed ? <Tooltip key={item.id}><TooltipTrigger asChild>{button}</TooltipTrigger><TooltipContent side="left" align="center">{item.label}</TooltipContent></Tooltip> : button
              })}</div>
            </TooltipProvider>
          </nav>
        })}
      </div>
      <div className="arg-sidebar-footer relative z-10 mt-auto pt-1">
        <div className="arg-divider mx-2 mb-2" aria-hidden="true" />
        <div className={cn('arg-service-status flex min-h-8 items-center gap-2 px-3 text-[11px]', collapsed && 'justify-center px-0')} title={collapsed ? 'سرویس متصل' : undefined}><span className="h-2 w-2 shrink-0 rounded-full bg-emerald-400"/><span className="arg-nav-label">سرویس متصل</span></div>
      </div>
      <img className="arg-fortress" src="/arg/illustrations/arg-fortress-minimal.svg" alt="" aria-hidden="true" />
    </aside>
  )
}
