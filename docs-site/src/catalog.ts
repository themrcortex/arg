export type Locale = 'fa' | 'en'
export type DocPage = { id: string; group: string; title: Record<Locale, string>; summary: Record<Locale, string>; file: string }
export type NavCategory = { id: string; title: Record<Locale, string>; children: NavSection[] }
export type NavSection = { id: string; title: Record<Locale, string>; pages: DocPage[] }
export const groups: Record<Locale, Record<string, string>> = {
  fa: { gettingStarted: 'شروع کار', dashboard: 'داشبورد و پایش', network: 'شبکه و وب‌سایت‌ها', docker: 'Docker و کانتینرها', data: 'فایل‌ها و داده', system: 'مدیریت سیستم', security: 'امنیت و نگهداری', reference: 'توسعه‌دهندگان و مرجع', applications: 'پلتفرم اپلیکیشن' },
  en: { gettingStarted: 'Getting started', dashboard: 'Dashboard & monitoring', network: 'Networking & websites', docker: 'Docker & containers', data: 'Files & data', system: 'System administration', security: 'Security & maintenance', reference: 'Developers & reference', applications: 'Application platform' },
}
export const pages: DocPage[] = [
  { id: 'overview', group: 'gettingStarted', title: { fa: 'معرفی ARG', en: 'About ARG' }, summary: { fa: 'ARG چیست، برای چه کاری ساخته شده و چه محدودیت‌هایی دارد.', en: 'What ARG does, its architecture, intended use, and current limits.' }, file: 'overview' },
  { id: 'install', group: 'gettingStarted', title: { fa: 'نصب و شروع سریع', en: 'Install & quick start' }, summary: { fa: 'پیش‌نیازها، نصب‌کننده، ساخت مدیر اول و ورود امن.', en: 'Requirements, installer, first administrator, and secure access.' }, file: 'install' },
  { id: 'cli', group: 'gettingStarted', title: { fa: 'دستورها و منوی SSH', en: 'CLI & SSH menu' }, summary: { fa: 'همهٔ فرمان‌های arg، منوی تعاملی، SSL و چرخهٔ سرویس.', en: 'ARG commands, interactive menu, TLS, and service lifecycle.' }, file: 'cli' },
  { id: 'architecture', group: 'reference', title: { fa: 'معماری و مسیرهای فایل', en: 'Architecture & paths' }, summary: { fa: 'اجزای Rust، React، systemd، helper و محل داده‌ها.', en: 'Rust, React, systemd, privileged helper, and storage paths.' }, file: 'architecture' },
  { id: 'dashboard', group: 'dashboard', title: { fa: 'داشبورد و پایش', en: 'Dashboard & monitoring' }, summary: { fa: 'شاخص‌ها، تاریخچه، نمودارها، آستانه‌ها و اعلان‌ها.', en: 'Metrics, history, charts, thresholds, and alerts.' }, file: 'dashboard' },
  { id: 'system-storage', group: 'system', title: { fa: 'سیستم، دیسک و فضای ذخیره‌سازی', en: 'System, disks & storage' }, summary: { fa: 'نمایش منابع میزبان، Mountها و بررسی مصرف دیسک.', en: 'Host resources, mount inventory, and disk usage analysis.' }, file: 'system-storage' },
  { id: 'network-firewall', group: 'network', title: { fa: 'شبکه، پورت و UFW', en: 'Network, ports & UFW' }, summary: { fa: 'رابط‌ها، ترافیک، پورت‌ها و مدیریت امن فایروال.', en: 'Interfaces, traffic, ports, and guarded firewall controls.' }, file: 'network-firewall' },
  { id: 'nginx', group: 'network', title: { fa: 'مدیریت Nginx و سایت‌ها', en: 'Nginx & websites' }, summary: { fa: 'سایت‌ها، ویرایشگر، دامنه، SSL، نگهداری و منابع.', en: 'Sites, config editor, domains, TLS, maintenance, and resources.' }, file: 'nginx' },
  { id: 'cloudflare-dns', group: 'network', title: { fa: 'مدیریت DNS با Cloudflare', en: 'Cloudflare DNS management' }, summary: { fa: 'اتصال امن Zone، مشاهده و تغییر رکوردهای DNS؛ محدودیت‌ها و بررسی انتشار.', en: 'Securely connect a zone, inspect and change DNS records, and understand current limits.' }, file: 'cloudflare-dns' },
  { id: 'docker', group: 'docker', title: { fa: 'مدیریت Docker', en: 'Docker management' }, summary: { fa: 'وضعیت Engine، کانتینرها، Imageها، Volumeها و Compose.', en: 'Engine, containers, images, volumes, networks, and Compose.' }, file: 'docker' },
  { id: 'files', group: 'data', title: { fa: 'مدیریت فایل‌ها', en: 'File manager' }, summary: { fa: 'مرور، انتقال، ویرایش، آرشیو و دانلود فایل‌های VPS.', en: 'Browse, transfer, edit, archive, and download VPS files.' }, file: 'files' },
  { id: 's3', group: 'data', title: { fa: 'فضای ابری S3', en: 'S3 storage' }, summary: { fa: 'اتصال Provider، Bucket، فایل‌ها و انتقال VPS؛ قابلیت‌های واقعی نسخه.', en: 'Providers, buckets, object access, VPS transfers, and release limits.' }, file: 's3' },
  { id: 'mysql', group: 'data', title: { fa: 'مدیریت MySQL و MariaDB', en: 'MySQL & MariaDB' }, summary: { fa: 'نصب، ورود مجدد مدیر، بانک‌ها، SQL و کاربران دیتابیس.', en: 'Install, re-authentication, databases, SQL, and DB users.' }, file: 'mysql' },
  { id: 'packages-services', group: 'system', title: { fa: 'APT و سرویس‌های systemd', en: 'APT & systemd services' }, summary: { fa: 'مدیریت پکیج‌ها و عملیات محدودشدهٔ سرویس‌ها.', en: 'Package operations and guarded system service controls.' }, file: 'packages-services' },
  { id: 'terminal', group: 'system', title: { fa: 'ترمینال سرور', en: 'Server terminal' }, summary: { fa: 'ترمینال چندتب، clipboard، اتصال مجدد و Root Terminal.', en: 'Multiple terminal tabs, clipboard, reconnect, and root terminal.' }, file: 'terminal' },
  { id: 'accounts-settings', group: 'security', title: { fa: 'حساب‌ها، کاربران و تنظیمات', en: 'Accounts, users & settings' }, summary: { fa: 'نقش‌ها، نشست‌ها، گذرواژه، تاریخچه و تنظیمات میزبان.', en: 'Roles, sessions, passwords, audit trail, and host settings.' }, file: 'accounts-settings' },
  { id: 'updates', group: 'security', title: { fa: 'به‌روزرسانی، لغو و بازگشت', en: 'Updates, cancellation & rollback' }, summary: { fa: 'شاخه‌ها، حالت Cache، مراحل، خطاها و بازیابی نسخه.', en: 'Branches, cache modes, progress, failure handling, and rollback.' }, file: 'updates' },
  { id: 'maintenance', group: 'security', title: { fa: 'نگهداری، بکاپ و بازیابی', en: 'Maintenance, backups & recovery' }, summary: { fa: 'وضعیت سلامت، بکاپ دیتابیس ARG، تعمیر، بازراه‌اندازی و حذف امن.', en: 'Readiness checks, ARG database backups, repair, reboot, and safe removal.' }, file: 'maintenance' },
  { id: 'security', group: 'security', title: { fa: 'مدل امنیتی و توصیه‌ها', en: 'Security model & guidance' }, summary: { fa: 'مرز دسترسی‌ها، اسرار، HTTPS و کارهای امن مدیر.', en: 'Privilege boundaries, secrets, HTTPS, and safe operations.' }, file: 'security' },
  { id: 'troubleshooting', group: 'security', title: { fa: 'عیب‌یابی', en: 'Troubleshooting' }, summary: { fa: 'خطاهای پرتکرار و گام‌های تشخیص و بازیابی.', en: 'Common failures and practical diagnosis and recovery steps.' }, file: 'troubleshooting' },
  { id: 'development', group: 'reference', title: { fa: 'توسعه و مشارکت', en: 'Development & contributing' }, summary: { fa: 'ساخت محلی، تست، ساختار کد، نسخه‌بندی و تغییر مستندات.', en: 'Local setup, checks, code structure, versioning, and docs edits.' }, file: 'development' },
  { id: 'public-api', group: 'reference', title: { fa: 'API عمومی فقط‌خواندنی', en: 'Read-only public API' }, summary: { fa: 'ساخت و لغو کلید، Scopeها، HTTPS، pagination، محدودیت درخواست و OpenAPI.', en: 'Create and revoke keys, scopes, HTTPS, pagination, rate limits, and OpenAPI.' }, file: 'public-api' },
  { id: 'app-platform-architecture', group: 'applications', title: { fa: 'معماری پلتفرم اپلیکیشن', en: 'Application platform architecture' }, summary: { fa: 'خط مبنای Phase 00، هویت سرورها، مالکیت منابع و مرزهای امنیتی.', en: 'Phase 00 baseline, server identity, resource ownership, and security boundaries.' }, file: 'app-platform-architecture' },
  { id: 'app-platform-roadmap-13-18', group: 'applications', title: { fa: 'نقشهٔ راه فازهای ۱۳ تا ۱۸ پلتفرم اپ', en: 'Application platform roadmap, phases 13–18' }, summary: { fa: 'پایش، شبکهٔ بین VPSها، ابزار مختص اپ، مهاجرت، Backup/Webhook و دروازهٔ انتشار.', en: 'Monitoring, cross-VPS networking, app-scoped tools, migration, backups/webhooks, and release gates.' }, file: 'app-platform-roadmap-13-18' },
  { id: 'app-platform-roadmap-18-48', group: 'applications', title: { fa: 'نقشهٔ راه کامل فازهای ۱۸ تا ۴۸', en: 'Full application platform roadmap, phases 18–48' }, summary: { fa: 'اتصال‌ها، محیط‌ها، قالب‌ها، ساخت، ناوگان، امنیت، پایش، موبایل، API عمومی و دروازهٔ انتشار.', en: 'Connections, environments, templates, builds, fleet, security, observability, mobile, public APIs, and release gates.' }, file: 'app-platform-roadmap-18-48' },
  { id: 'glossary', group: 'reference', title: { fa: 'واژه‌نامه', en: 'Glossary' }, summary: { fa: 'تعریف اصطلاحات رابط و زیرساخت.', en: 'Definitions for interface and infrastructure terms.' }, file: 'glossary' },
  { id: 'federation', group: 'network', title: { fa: 'مدیریت سرورها و اتصال راه‌دور', en: 'Federation & remote servers' }, summary: { fa: 'نقش‌ها، Pairing، scopeها، امنیت، وضعیت اتصال و فضای‌کار راه‌دور.', en: 'Roles, pairing, scopes, security, connection health, and remote workspaces.' }, file: 'federation' },
  { id: 'application-registry', group: 'applications', title: { fa: 'فهرست اپلیکیشن‌ها', en: 'Application registry' }, summary: { fa: 'ثبت مالکیت، دامنه و پورت؛ فهرست خواندنی VPSها و تفاوت وضعیت هدف با اجرای واقعی.', en: 'Register app ownership, domains, and ports; browse VPS registries and distinguish intent from observed runtime.' }, file: 'application-registry' },
  { id: 'runtime-manager', group: 'applications', title: { fa: 'مدیریت Runtimeها', en: 'Runtime manager' }, summary: { fa: 'Inventory PHP، Node، Python، پیش‌بررسی APT، نصب با پیش‌نمایش و انتخاب نسخهٔ اپ.', en: 'PHP, Node, and Python inventory, APT preflight, install preview, and app version selection.' }, file: 'runtime-manager' },
  { id: 'laravel-runner', group: 'applications', title: { fa: 'مدیریت Laravel', en: 'Laravel runner' }, summary: { fa: 'بررسی release، تنظیم محیط خصوصی، نصب lockfile و عملیات محدود Artisan.', en: 'Release checks, private environment setup, locked Composer install, and allowlisted Artisan actions.' }, file: 'laravel-runner' },
  { id: 'node-runner', group: 'applications', title: { fa: 'اجرای اپلیکیشن Node.js', en: 'Node.js application runner' }, summary: { fa: 'تشخیص lockfile، build محدود، متغیرهای خصوصی، سرویس systemd و reverse proxy مدیریت‌شده.', en: 'Lockfile detection, sandboxed builds, private environment, systemd service, and guarded reverse proxy.' }, file: 'node-runner' },
  { id: 'static-and-ssr', group: 'applications', title: { fa: 'انتشار Static و اجرای SSR', en: 'Static publishing and SSR' }, summary: { fa: 'تشخیص خودکار و دستی React/Vue/Vite، خروجی Nginx، fallback، Next/Nuxt SSR و Rollback.', en: 'Auto/manual React/Vue/Vite detection, Nginx output, fallback, Next/Nuxt SSR, and rollback.' }, file: 'static-and-ssr' },
  { id: 'app-resources', group: 'applications', title: { fa: 'شبکه و منابع اپلیکیشن', en: 'Application networks and resources' }, summary: { fa: 'شبکهٔ داخلی، مسیر دادهٔ پایدار، اتصال S3 و مشاهدهٔ لاگ‌های journald برای هر اپ.', en: 'Per-app internal networks, durable data paths, S3 references, and journald logs.' }, file: 'app-resources' },
  { id: 'deployment-jobs', group: 'applications', title: { fa: 'استقرار منبع و Jobهای پایدار', en: 'Source deployments & durable jobs' }, summary: { fa: 'دریافت امن Git/ZIP/پوشه، پیگیری Job، لغو، retry و بازگشت release.', en: 'Safe Git/ZIP/directory acquisition, durable progress, cancel, retry, and release rollback.' }, file: 'deployment-jobs' },
]


const legacyPageTargets: Record<string, string> = {
  'guide-account-': 'accounts-settings', 'guide-users-': 'accounts-settings', 'guide-audit-': 'accounts-settings', 'guide-settings-': 'accounts-settings',
  'guide-dashboard-': 'dashboard', 'guide-system-': 'system-storage',
  'guide-network-': 'network-firewall', 'guide-ports-': 'network-firewall', 'guide-firewall-': 'network-firewall',
  'guide-nginx-': 'nginx', 'guide-docker-': 'docker', 'guide-files-': 'files', 'guide-s3-': 's3', 'guide-mysql-': 'mysql',
  'guide-packages-': 'packages-services', 'guide-services-': 'packages-services', 'guide-terminal-': 'terminal',
  'guide-updates-': 'updates', 'guide-maintenance-': 'maintenance', 'guide-nodes-': 'federation',
}
export const resolvePageId = (id: string) => {
  if (pages.some((page) => page.id === id)) return id
  const legacyPrefix = Object.keys(legacyPageTargets).find((prefix) => id.startsWith(prefix))
  return legacyPrefix ? legacyPageTargets[legacyPrefix] : undefined
}

const sectionFor = (id: string): string => {
  if (['overview', 'install', 'cli'].includes(id)) return 'basics'
  if (['architecture', 'development', 'glossary', 'public-api'].includes(id)) return 'reference'
  if (id === 'dashboard') return 'monitoring'
  if (id === 'federation') return 'federation'
  if (id.startsWith('app-platform-')) return 'platformOverview'
  if (id === 'application-registry') return 'registry'
  if (id === 'runtime-manager') return 'runtime'
  if (['laravel-runner', 'node-runner', 'static-and-ssr'].includes(id)) return 'runtimes'
  if (['app-resources', 'deployment-jobs'].includes(id)) return 'deployments'
  if (id === 'system-storage') return 'host'
  if (id === 'network-firewall') return 'network'
  if (id === 'cloudflare-dns') return 'dnsProviders'
  if (id === 'nginx') return 'nginx'
  if (id === 'docker') return 'docker'
  if (id === 'files') return 'files'
  if (id === 's3') return 's3'
  if (id === 'mysql') return 'databases'
  if (id === 'packages-services') return 'services'
  if (id === 'terminal') return 'terminal'
  if (id === 'accounts-settings') return 'accounts'
  if (id === 'updates') return 'updates'
  if (id === 'maintenance') return 'maintenance'
  if (id === 'security') return 'security'
  if (id === 'troubleshooting') return 'troubleshooting'
  return 'reference'
}
const sectionTitles: Record<string, Record<Locale, string>> = {
  platformOverview: { fa: 'مروری بر پلتفرم', en: 'Platform overview' }, registry: { fa: 'فهرست اپلیکیشن‌ها', en: 'Application registry' }, runtime: { fa: 'Runtimeها', en: 'Runtimes' }, runtimes: { fa: 'اجرای اپلیکیشن‌ها', en: 'Application runners' }, deployments: { fa: 'استقرار و منابع', en: 'Deployments & resources' }, federation: { fa: 'اتصال سرورها', en: 'Server federation' },
  basics: { fa: 'آشنایی و نصب', en: 'Introduction & installation' }, reference: { fa: 'توسعه و مرجع', en: 'Development & reference' }, dnsProviders: { fa: 'DNS و دامنه‌ها', en: 'DNS & domains' },
  monitoring: { fa: 'پایش منابع', en: 'Resource monitoring' }, host: { fa: 'سیستم و دیسک', en: 'System & disks' }, network: { fa: 'شبکه', en: 'Network' }, ports: { fa: 'پورت‌ها', en: 'Ports' }, firewall: { fa: 'فایروال', en: 'Firewall' }, nginx: { fa: 'Nginx و وب‌سایت‌ها', en: 'Nginx & websites' }, docker: { fa: 'Docker', en: 'Docker' }, files: { fa: 'مدیریت فایل', en: 'File manager' }, s3: { fa: 'فضای ذخیره‌سازی S3', en: 'S3 storage' }, databases: { fa: 'پایگاه داده', en: 'Databases' }, services: { fa: 'بسته‌ها و سرویس‌ها', en: 'Packages & services' }, terminal: { fa: 'ترمینال', en: 'Terminal' }, accounts: { fa: 'حساب‌ها و کاربران', en: 'Accounts & users' }, audit: { fa: 'گزارش ممیزی', en: 'Audit log' }, settings: { fa: 'تنظیمات', en: 'Settings' }, updates: { fa: 'به‌روزرسانی', en: 'Updates' }, maintenance: { fa: 'نگهداری', en: 'Maintenance' }, security: { fa: 'امنیت', en: 'Security' }, troubleshooting: { fa: 'عیب‌یابی', en: 'Troubleshooting' },
}
const categoryOrder = ['gettingStarted','dashboard','network','docker','data','applications','system','security','reference']
export const navigation: NavCategory[] = categoryOrder.map((id) => ({
  id, title: { fa: groups.fa[id], en: groups.en[id] },
  children: [...new Set(pages.filter((page) => page.group === id).map((page) => sectionFor(page.id)))].map((sectionId) => ({
    id: sectionId, title: sectionTitles[sectionId], pages: pages.filter((page) => page.group === id && sectionFor(page.id) === sectionId),
  })),
}))
export const readingOrder = navigation.flatMap((category) => category.children.flatMap((section) => section.pages))
export const pageNeighbors = (id: string) => {
  const sequence = readingOrder
  const index = sequence.findIndex((page) => page.id === id)
  if (index < 0) return { previous: undefined, next: undefined }
  return { previous: sequence[index - 1], next: sequence[index + 1] }
}
export const isMarkdownPageLink = (href: string) => /\.md(?:[?#]|$)/i.test(href.split(/[?#]/, 1)[0])
export const resolveMarkdownPageLink = (href: string, fallbackLocale: Locale) => {
  const [path = '', fragment = ''] = href.split('#', 2)
  const pathname = path.split('?', 1)[0]
  if (!/\.md$/i.test(pathname)) return undefined
  const segments = pathname.split('/').filter(Boolean)
  let file = segments.at(-1) || ''
  try { file = decodeURIComponent(file) } catch { return undefined }
  const basename = file.replace(/\.md$/i, '')
  const target = pages.find((page) => page.file === basename)
  if (!target) return undefined
  const explicitLocale = segments.find((segment) => segment === 'fa' || segment === 'en') as Locale | undefined
  let anchor = fragment
  try { anchor = decodeURIComponent(anchor) } catch { /* Keep malformed fragments inert. */ }
  return { id: target.id, locale: explicitLocale || fallbackLocale, anchor }
}
export const findPage = (id: string) => pages.find((page) => page.id === id)
export const pageTrail = (id: string) => navigation.flatMap((category) => category.children.filter((section) => section.pages.some((page) => page.id === id)).map((section) => [category, section]))[0] ?? []
export const validateCatalog = (availableFiles: string[] = pages.map((page) => page.file)) => {
  const ids = new Set(pages.map((page) => page.id))
  return { duplicateIds: pages.filter((page, index) => pages.findIndex((candidate) => candidate.id === page.id) !== index).map((page) => page.id), missingFiles: pages.filter((page) => !availableFiles.includes(page.file)).map((page) => page.file), unreferencedIds: readingOrder.filter((page) => !ids.has(page.id)).map((page) => page.id) }
}
