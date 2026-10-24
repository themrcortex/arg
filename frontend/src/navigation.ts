import { Box, Cpu, Files, Globe, LayoutDashboard, Network, Package, Radio, ScrollText, ServerCog, Settings, Shield, Terminal, UserRound, Users, type LucideIcon } from 'lucide-react'

export type PageId = 'dashboard' | 'system' | 'network' | 'ports' | 'firewall' | 'nginx' | 'docker' | 'files' | 'terminal' | 'packages' | 'services' | 'users' | 'audit' | 'settings' | 'account'
export type NavigationItem = { id: PageId; label: string; icon: LucideIcon; admin?: boolean }

export const navigation: NavigationItem[] = [
  { id: 'dashboard', label: 'نمای کلی', icon: LayoutDashboard },
  { id: 'system', label: 'سیستم و منابع', icon: Cpu },
  { id: 'network', label: 'شبکه', icon: Network },
  { id: 'ports', label: 'پورت‌ها', icon: Radio },
  { id: 'firewall', label: 'فایروال UFW', icon: Shield },
  { id: 'nginx', label: 'سایت‌های Nginx', icon: Globe, admin: true },
  { id: 'docker', label: 'مدیریت Docker', icon: Box, admin: true },
  { id: 'files', label: 'فایل‌های سرور', icon: Files, admin: true },
  { id: 'terminal', label: 'ترمینال سرور', icon: Terminal, admin: true },
  { id: 'packages', label: 'مدیریت APT', icon: Package, admin: true },
  { id: 'services', label: 'سرویس‌های سیستم', icon: ServerCog, admin: true },
  { id: 'users', label: 'کاربران', icon: Users, admin: true },
  { id: 'audit', label: 'گزارش عملیات', icon: ScrollText, admin: true },
  { id: 'settings', label: 'تنظیمات', icon: Settings },
  { id: 'account', label: 'حساب کاربری', icon: UserRound },
]

export function pageFromHash(hash: string): PageId {
  const id = hash.replace(/^#\/?/, '')
  const direct = navigation.find((item) => item.id === id)
  if (direct) return direct.id
  const legacy = navigation.find((item) => id.startsWith(`${item.id}-`))
  return legacy?.id || 'dashboard'
}
