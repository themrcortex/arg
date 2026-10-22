import {
  Cpu,
  LayoutDashboard,
  Network,
  Radio,
  ScrollText,
  Settings,
  Shield,
  Users,
  type LucideIcon,
} from 'lucide-react'

export type InsightKind = 'system' | 'network' | 'ports' | 'firewall' | 'users' | 'audit' | 'settings'
export type BasePage = 'dashboard' | 'network' | 'firewall' | 'users' | 'audit' | 'settings'
export type ExplorerEntry = {
  id: string
  label: string
  description: string
  kind: InsightKind | 'page'
  value: string
  admin?: boolean
}
export type ExplorerCategory = { id: string; label: string; items: ExplorerEntry[] }
export type ExplorerGroup = {
  id: string
  label: string
  icon: LucideIcon
  categories: ExplorerCategory[]
  admin?: boolean
}

const entry = (kind: InsightKind, value: string, label: string, description = ''): ExplorerEntry => ({
  id: `${kind}-${value}`,
  label,
  description,
  kind,
  value,
  admin: kind === 'users' || kind === 'audit',
})
const page = (value: BasePage, label: string, description: string): ExplorerEntry => ({
  id: value,
  label,
  description,
  kind: 'page',
  value,
  admin: value === 'users' || value === 'audit',
})
const category = (id: string, label: string, items: ExplorerEntry[]): ExplorerCategory => ({ id, label, items })

export const commonPorts: [number, string][] = [
  [20, 'FTP data'], [21, 'FTP'], [22, 'SSH'], [23, 'Telnet'], [25, 'SMTP'],
  [53, 'DNS'], [67, 'DHCP server'], [68, 'DHCP client'], [69, 'TFTP'], [80, 'HTTP'],
  [110, 'POP3'], [111, 'RPC'], [123, 'NTP'], [135, 'RPC endpoint'], [137, 'NetBIOS'],
  [139, 'SMB session'], [143, 'IMAP'], [161, 'SNMP'], [389, 'LDAP'], [443, 'HTTPS'],
  [445, 'SMB'], [465, 'SMTPS'], [514, 'Syslog'], [587, 'SMTP submission'], [631, 'IPP'],
  [636, 'LDAPS'], [873, 'Rsync'], [993, 'IMAPS'], [995, 'POP3S'], [1080, 'SOCKS'],
  [1433, 'MS SQL'], [1521, 'Oracle DB'], [1883, 'MQTT'], [2049, 'NFS'], [2375, 'Docker API'],
  [3000, 'App dev'], [3306, 'MySQL'], [3389, 'RDP'], [5432, 'PostgreSQL'], [5672, 'AMQP'],
  [5900, 'VNC'], [6379, 'Redis'], [8000, 'App HTTP'], [8080, 'HTTP alt'], [8443, 'HTTPS alt'],
]

const systemMetrics: [string, string][] = [
  ['cpu-use', 'مصرف پردازنده'], ['cpu-idle', 'زمان بیکاری CPU'], ['cpu-cores', 'هسته‌های در دسترس'],
  ['load-1', 'بار 1 دقیقه'], ['load-5', 'بار 5 دقیقه'], ['load-15', 'بار 15 دقیقه'],
  ['uptime-days', 'روزهای روشن‌بودن'], ['uptime-hours', 'ساعت‌های روشن‌بودن'], ['uptime-minutes', 'دقایق روشن‌بودن'],
  ['mem-total', 'حافظهٔ کل'], ['mem-used', 'حافظهٔ مصرف‌شده'], ['mem-available', 'حافظهٔ در دسترس'],
  ['mem-percent', 'درصد مصرف حافظه'], ['mem-free', 'حافظهٔ آزاد'], ['mem-buffers', 'بافرهای حافظه'],
  ['mem-cached', 'کش حافظه'], ['mem-active', 'حافظهٔ فعال'], ['mem-inactive', 'حافظهٔ غیرفعال'],
  ['mem-dirty', 'صفحات Dirty'], ['mem-slab', 'حافظهٔ Slab'],
  ['swap-total', 'Swap کل'], ['swap-free', 'Swap آزاد'], ['swap-used', 'Swap مصرف‌شده'], ['swap-percent', 'درصد مصرف Swap'],
  ['disk-total', 'ظرفیت دیسک ریشه'], ['disk-free', 'فضای آزاد دیسک'], ['disk-used', 'فضای مصرف‌شدهٔ دیسک'],
  ['disk-percent', 'درصد مصرف دیسک'], ['process-cpu', 'CPU فرایند Donatello'], ['process-ram', 'RAM فرایند Donatello'],
]

const networkMetrics: [string, string][] = [
  ['interfaces', 'تعداد کارت‌های شبکه'], ['interfaces-up', 'کارت‌های فعال'], ['interfaces-down', 'کارت‌های غیرفعال'],
  ['carrier', 'اتصال فیزیکی فعال'], ['ipv4', 'آدرس‌های IPv4'], ['ipv6', 'آدرس‌های IPv6'],
  ['mac', 'آدرس‌های MAC'], ['mtu-min', 'کمترین MTU'], ['mtu-max', 'بیشترین MTU'],
  ['speed-max', 'بیشترین سرعت لینک'], ['rx-total', 'دریافت تجمعی'], ['tx-total', 'ارسال تجمعی'],
  ['bytes-total', 'کل بایت‌های جابه‌جا شده'], ['rx-rate', 'نرخ دریافت زنده'], ['tx-rate', 'نرخ ارسال زنده'],
  ['rx-packets', 'بسته‌های دریافت‌شده'], ['tx-packets', 'بسته‌های ارسال‌شده'],
  ['rx-errors', 'خطاهای دریافت'], ['tx-errors', 'خطاهای ارسال'],
  ['rx-dropped', 'بسته‌های ورودی حذف‌شده'], ['tx-dropped', 'بسته‌های خروجی حذف‌شده'],
  ['top-interface', 'پرترافیک‌ترین کارت'],
]

const portFilters: [string, string][] = [
  ['all', 'همهٔ پورت‌ها'], ['tcp', 'همهٔ TCP'], ['udp', 'همهٔ UDP'],
  ['ipv4', 'همهٔ IPv4'], ['ipv6', 'همهٔ IPv6'], ['tcp4', 'TCP روی IPv4'], ['tcp6', 'TCP روی IPv6'],
  ['udp4', 'UDP روی IPv4'], ['udp6', 'UDP روی IPv6'], ['low', 'پورت‌های سیستمی'],
  ['high', 'پورت‌های 1024 به بالا'], ['web', 'پورت‌های وب'], ['remote', 'دسترسی راه دور'],
  ['mail', 'پورت‌های ایمیل'], ['database', 'پایگاه‌های داده'], ['dns', 'پورت DNS'],
]

const firewallFilters: [string, string][] = [
  ['all', 'همهٔ قوانین'], ['allow', 'قوانین Allow'], ['deny', 'قوانین Deny'],
  ['reject', 'قوانین Reject'], ['limit', 'قوانین Limit'], ['ipv4', 'قوانین IPv4'],
  ['ipv6', 'قوانین IPv6'], ['tcp', 'قوانین TCP'], ['udp', 'قوانین UDP'],
  ['ssh', 'قوانین SSH'], ['web', 'قوانین وب'], ['status', 'وضعیت UFW'],
]

const userFilters: [string, string][] = [
  ['all', 'همهٔ حساب‌ها'], ['active', 'حساب‌های فعال'], ['disabled', 'حساب‌های غیرفعال'],
  ['admin', 'مدیران'], ['operator', 'اپراتورها'], ['viewer', 'ناظران'],
  ['admin-active', 'مدیران فعال'], ['operator-active', 'اپراتورهای فعال'],
  ['viewer-active', 'ناظران فعال'], ['mine', 'حساب من'],
]

const auditFilters: [string, string][] = [
  ['all', 'همهٔ رویدادها'], ['login', 'ورودها'], ['logout', 'خروج‌ها'], ['setup', 'راه‌اندازی اولیه'],
  ['account.password', 'تغییر گذرواژه'], ['user.create', 'ساخت کاربر'], ['user.update', 'تغییر کاربر'],
  ['hour', 'یک ساعت اخیر'], ['today', 'امروز'], ['week', 'هفت روز اخیر'], ['mine', 'رویدادهای من'],
]

export const explorerGroups: ExplorerGroup[] = [
  {
    id: 'overview', label: 'مرکز کنترل', icon: LayoutDashboard,
    categories: [category('overview-main', 'نمای اصلی', [page('dashboard', 'داشبورد زنده', 'وضعیت کلی سرور و نمودارهای زنده')])],
  },
  {
    id: 'system', label: 'سیستم و منابع', icon: Cpu,
    categories: [
      category('system-cpu', 'پردازنده و بار', systemMetrics.slice(0, 6).map(([key, label]) => entry('system', key, label))),
      category('system-uptime', 'زمان روشن‌بودن', systemMetrics.slice(6, 9).map(([key, label]) => entry('system', key, label))),
      category('system-memory', 'حافظه', systemMetrics.slice(9, 20).map(([key, label]) => entry('system', key, label))),
      category('system-swap', 'Swap', systemMetrics.slice(20, 24).map(([key, label]) => entry('system', key, label))),
      category('system-disk', 'دیسک ریشه', systemMetrics.slice(24, 28).map(([key, label]) => entry('system', key, label))),
      category('system-process', 'خود Donatello', systemMetrics.slice(28).map(([key, label]) => entry('system', key, label))),
    ],
  },
  {
    id: 'network', label: 'شبکه', icon: Network,
    categories: [
      category('network-main', 'نمای اصلی', [page('network', 'کارت‌ها و پورت‌ها', 'فهرست کامل رابط‌ها و پورت‌های شنونده')]),
      category('network-links', 'رابط‌ها و آدرس‌ها', networkMetrics.slice(0, 10).map(([key, label]) => entry('network', key, label))),
      category('network-traffic', 'ترافیک و خطاها', networkMetrics.slice(10).map(([key, label]) => entry('network', key, label))),
    ],
  },
  {
    id: 'ports', label: 'پورت‌های شنونده', icon: Radio,
    categories: [
      category('ports-filters', 'فیلترهای زنده', portFilters.map(([key, label]) => entry('ports', key, label))),
      category('ports-common', 'بررسی پورت شناخته‌شده', commonPorts.map(([port, label]) => entry('ports', `port-${port}`, `${port} · ${label}`))),
    ],
  },
  {
    id: 'firewall', label: 'فایروال UFW', icon: Shield,
    categories: [
      category('firewall-main', 'وضعیت', [page('firewall', 'نمای کلی UFW', 'خروجی واقعی دستور ufw status numbered')]),
      category('firewall-filters', 'فیلتر قوانین', firewallFilters.map(([key, label]) => entry('firewall', key, label))),
      category('firewall-common', 'قانون هر پورت', commonPorts.map(([port, label]) => entry('firewall', `port-${port}`, `${port} · ${label}`))),
    ],
  },
  {
    id: 'users', label: 'کاربران و دسترسی', icon: Users, admin: true,
    categories: [
      category('users-main', 'مدیریت', [page('users', 'مدیریت کاربران', 'ساخت حساب، نقش و وضعیت دسترسی')]),
      category('users-filters', 'نمای حساب‌ها', userFilters.map(([key, label]) => entry('users', key, label))),
    ],
  },
  {
    id: 'audit', label: 'گزارش عملیات', icon: ScrollText, admin: true,
    categories: [
      category('audit-main', 'گزارش کامل', [page('audit', 'رویدادهای اخیر', '100 رویداد آخر پنل')]),
      category('audit-filters', 'ردیابی رویداد', auditFilters.map(([key, label]) => entry('audit', key, label))),
    ],
  },
  {
    id: 'settings', label: 'تنظیمات پنل', icon: Settings,
    categories: [
      category('settings-main', 'مدیریت', [page('settings', 'تنظیمات حساب', 'گذرواژه و اطلاعات پنل')]),
      category('settings-details', 'جزئیات اجرا', [
        entry('settings', 'endpoint', 'نشانی و پورت پنل'),
        entry('settings', 'session', 'امنیت نشست'),
        entry('settings', 'monitoring', 'دورهٔ مانیتورینگ'),
        entry('settings', 'password', 'تغییر گذرواژه'),
      ]),
    ],
  },
]

export const explorerEntries = explorerGroups.flatMap((group) => group.categories.flatMap((section) => section.items))
export const entryById = new Map(explorerEntries.map((item) => [item.id, item]))
