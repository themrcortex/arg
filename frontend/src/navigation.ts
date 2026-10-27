import { AppWindow, Bug, Cloud, Container, Cpu, Files, Globe2, LayoutDashboard, Network, Package, PanelsTopLeft, Radio, ScrollText, ServerCog, Settings, Shield, Terminal, UserRound, Users, Wrench, Database, Server, type LucideIcon } from 'lucide-react'

export type PageId = 'dashboard' | 'system' | 'nodes' | 'apps' | 'maintenance' | 'mysql' | 'network' | 'ports' | 'firewall' | 'nginx' | 'docker' | 'files' | 's3' | 'dns' | 'terminal' | 'packages' | 'services' | 'users' | 'audit' | 'settings' | 'account' | 'bug-report'
export type NavigationItem = { id: PageId; label: string; icon: LucideIcon; admin?: boolean }

export const navigation: NavigationItem[] = [
  { id: 'dashboard', label: 'نمای کلی', icon: LayoutDashboard },
  { id: 'system', label: 'سیستم و منابع', icon: Cpu },
  { id: 'nodes', label: 'Nodeهای ARG', icon: Server, admin: true },
  { id: 'apps', label: 'اپلیکیشن‌ها', icon: AppWindow, admin: true },
  { id: 'network', label: 'شبکه', icon: Network },
  { id: 'ports', label: 'پورت‌ها', icon: Radio },
  { id: 'firewall', label: 'فایروال UFW', icon: Shield },
  { id: 'nginx', label: 'سایت‌های Nginx', icon: PanelsTopLeft, admin: true },
  { id: 'docker', label: 'مدیریت Docker', icon: Container, admin: true },
  { id: 'files', label: 'فایل‌های سرور', icon: Files, admin: true },
  { id: 's3', label: 'فضای ابری S3', icon: Cloud, admin: true },
  { id: 'dns', label: 'مدیریت DNS', icon: Globe2, admin: true },
  { id: 'terminal', label: 'ترمینال سرور', icon: Terminal, admin: true },
  { id: 'packages', label: 'مدیریت APT', icon: Package, admin: true },
  { id: 'services', label: 'سرویس‌های سیستم', icon: ServerCog, admin: true },
  { id: 'mysql', label: 'مدیریت MySQL', icon: Database, admin: true },
  { id: 'maintenance', label: 'مدیریت سرور و ARG', icon: Wrench, admin: true },
  { id: 'users', label: 'کاربران', icon: Users, admin: true },
  { id: 'audit', label: 'گزارش عملیات', icon: ScrollText, admin: true },
  { id: 'bug-report', label: 'گزارش باگ', icon: Bug },
  { id: 'settings', label: 'تنظیمات', icon: Settings },
  { id: 'account', label: 'حساب کاربری', icon: UserRound },
]

export function pageFromHash(hash: string): PageId {
  const id = hash.replace(/^#\/?/, '').split('?')[0]
  const direct = navigation.find((item) => item.id === id)
  if (direct) return direct.id
  const legacy = navigation.find((item) => id.startsWith(`${item.id}-`))
  return legacy?.id || 'dashboard'
}
