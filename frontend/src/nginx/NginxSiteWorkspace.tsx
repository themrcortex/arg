import type { ReactNode } from 'react'
import { Globe, Info, Pencil, Power, Trash2 } from 'lucide-react'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'

type Props = {
  name: string
  description?: string
  enabled: boolean
  maintenanceEnabled?: boolean
  onMetadata: () => void
  onEdit: () => void
  onToggle: () => void
  onDelete?: () => void
  children: ReactNode
}

const sections = [
  ['nginx-site-runtime', 'وضعیت و منابع'],
  ['nginx-site-domains', 'دامنه و SSL'],
  ['nginx-site-actions', 'عملیات سایت'],
  ['nginx-site-repository', 'Git و گزارش‌ها'],
] as const

export function NginxSiteWorkspace({ name, description, enabled, maintenanceEnabled, onMetadata, onEdit, onToggle, onDelete, children }: Props) {
  return <main className="space-y-5" aria-label={`مدیریت سایت ${name}`}>
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
    <nav aria-label="بخش‌های مدیریت سایت" className="sticky top-2 z-10 flex gap-2 overflow-x-auto rounded-xl border border-white/10 bg-background/90 p-2 backdrop-blur">
      {sections.map(([id, label]) => <button key={id} type="button" onClick={() => document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })} className="shrink-0 rounded-lg px-3 py-2 text-xs text-muted-foreground transition hover:bg-white/[.06] hover:text-white">{label}</button>)}
    </nav>
    {children}
  </main>
}
