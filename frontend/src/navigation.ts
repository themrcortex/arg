import { Cpu, Globe, LayoutDashboard, Network, Radio, ScrollText, Settings, Shield, Users, type LucideIcon } from 'lucide-react'

export type PageId = 'dashboard' | 'system' | 'network' | 'ports' | 'firewall' | 'nginx' | 'users' | 'audit' | 'settings'
export type NavigationItem = { id: PageId; label: string; icon: LucideIcon; admin?: boolean }

export const navigation: NavigationItem[] = [
  { id: 'dashboard', label: 'نمای کلی', icon: LayoutDashboard },
  { id: 'system', label: 'سیستم و منابع', icon: Cpu },
  { id: 'network', label: 'شبکه', icon: Network },
  { id: 'ports', label: 'پورت‌ها', icon: Radio },
  { id: 'firewall', label: 'فایروال UFW', icon: Shield },
  { id: 'nginx', label: 'سایت‌های Nginx', icon: Globe, admin: true },
  { id: 'users', label: 'کاربران', icon: Users, admin: true },
  { id: 'audit', label: 'گزارش عملیات', icon: ScrollText, admin: true },
  { id: 'settings', label: 'تنظیمات', icon: Settings },
]

export function pageFromHash(hash: string): PageId {
  const id = hash.replace(/^#\/?/, '')
  const direct = navigation.find((item) => item.id === id)
  if (direct) return direct.id
  const legacy = navigation.find((item) => id.startsWith(`${item.id}-`))
  return legacy?.id || 'dashboard'
}
