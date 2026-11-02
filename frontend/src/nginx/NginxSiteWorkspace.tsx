import type { KeyboardEvent, ReactNode } from 'react'
import { Globe, Info, Pencil, Power, Trash2 } from 'lucide-react'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { tabsListClassName, tabsTriggerClassName } from '../components/ui/tabs'
import { cn } from '../lib/utils'

type Props = {
  name: string
  description?: string
  enabled: boolean
  maintenanceEnabled?: boolean
  onMetadata: () => void
  onEdit: () => void
  onToggle: () => void
  onDelete?: () => void
  activeTab: NginxSiteTab
  onTabChange: (tab: NginxSiteTab) => void
  children: ReactNode
}

export type NginxSiteTab = 'overview' | 'config' | 'domains' | 'operations' | 'activity'

const tabs: Array<{ id: NginxSiteTab; label: string }> = [
  { id: 'overview', label: 'نمای کلی' },
  { id: 'config', label: 'کانفیگ' },
  { id: 'domains', label: 'دامنه و SSL' },
  { id: 'operations', label: 'عملیات و فایل‌ها' },
  { id: 'activity', label: 'ترافیک و لاگ‌ها' },
]

export function NginxSiteWorkspace({ name, description, enabled, maintenanceEnabled, onMetadata, onEdit, onToggle, onDelete, activeTab, onTabChange, children }: Props) {
  function onTabKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const current = tabs.findIndex((tab) => tab.id === activeTab)
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (current + (event.key === 'ArrowRight' ? -1 : 1) + tabs.length) % tabs.length
    const next = tabs[nextIndex]
    onTabChange(next.id)
    document.getElementById(`nginx-site-tab-${next.id}`)?.focus()
  }

  return <main className="nginx-site-workspace space-y-5" aria-label={`مدیریت سایت ${name}`}>
    <section className="flex flex-wrap items-start justify-between gap-4 rounded-2xl border border-primary/20 bg-primary/[.045] p-5">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2"><Globe size={18} className="text-primary"/><h2 dir="ltr" className="break-all font-mono text-lg font-bold text-white">{name}</h2><Badge variant={enabled ? 'success' : 'outline'}>{enabled ? 'روشن در Nginx' : 'خاموش در Nginx'}</Badge><Badge variant={maintenanceEnabled ? 'warning' : 'outline'}>تعمیرات: {maintenanceEnabled ? 'فعال' : 'خاموش'}</Badge></div>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">{description || 'توضیحی برای این سایت ثبت نشده است.'}</p>
        <p dir="ltr" className="mt-2 break-all font-mono text-[11px] text-muted-foreground">/etc/nginx/sites-available/{name}</p>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={onMetadata}><Info size={14}/>مشخصات</Button>
        <Button size="sm" variant="outline" onClick={onEdit}><Pencil size={14}/>ویرایش کانفیگ</Button>
        <Button size="sm" variant={enabled ? 'destructive' : 'default'} onClick={onToggle}><Power size={14}/>{enabled ? 'خاموش‌کردن' : 'روشن‌کردن'}</Button>
        {onDelete && <Button size="sm" variant="ghost" className="text-destructive hover:text-destructive" onClick={onDelete}><Trash2 size={14}/>حذف سایت</Button>}
      </div>
    </section>
    <nav dir="rtl" aria-label="بخش‌های مدیریت سایت" role="tablist" className={tabsListClassName}>
      {tabs.map(({ id, label }) => <button key={id} id={`nginx-site-tab-${id}`} type="button" role="tab" data-state={activeTab === id ? 'active' : 'inactive'} aria-selected={activeTab === id} aria-controls="nginx-site-tab-panel" tabIndex={activeTab === id ? 0 : -1} onKeyDown={onTabKeyDown} onClick={() => onTabChange(id)} className={cn(tabsTriggerClassName, 'text-xs')}>{label}</button>)}
    </nav>
    <div id="nginx-site-tab-panel" role="tabpanel" tabIndex={0} aria-labelledby={`nginx-site-tab-${activeTab}`} data-active-tab={activeTab} className="nginx-site-tab-panels min-w-0 space-y-5">{children}</div>
  </main>
}
