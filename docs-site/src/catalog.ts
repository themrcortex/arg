export type Locale = 'fa' | 'en'
export type DocPage = { id: string; group: string; title: Record<Locale, string>; summary: Record<Locale, string>; file: string }
export const groups: Record<Locale, string[]> = {
  fa: ['شروع کار', 'راهنمای پنل', 'مدیریت سرویس‌ها', 'امنیت و نگهداری', 'توسعه'],
  en: ['Getting started', 'Panel guide', 'Service management', 'Security & maintenance', 'Development'],
}
export const pages: DocPage[] = [
  { id: 'overview', group: '0', title: { fa: 'معرفی ARG', en: 'About ARG' }, summary: { fa: 'ARG چیست، برای چه کاری ساخته شده و چه محدودیت‌هایی دارد.', en: 'What ARG does, its architecture, intended use, and current limits.' }, file: 'overview' },
  { id: 'install', group: '0', title: { fa: 'نصب و شروع سریع', en: 'Install & quick start' }, summary: { fa: 'پیش‌نیازها، نصب‌کننده، ساخت مدیر اول و ورود امن.', en: 'Requirements, installer, first administrator, and secure access.' }, file: 'install' },
  { id: 'cli', group: '0', title: { fa: 'دستورها و منوی SSH', en: 'CLI & SSH menu' }, summary: { fa: 'همهٔ فرمان‌های arg، منوی تعاملی، SSL و چرخهٔ سرویس.', en: 'ARG commands, interactive menu, TLS, and service lifecycle.' }, file: 'cli' },
  { id: 'architecture', group: '0', title: { fa: 'معماری و مسیرهای فایل', en: 'Architecture & paths' }, summary: { fa: 'اجزای Rust، React، systemd، helper و محل داده‌ها.', en: 'Rust, React, systemd, privileged helper, and storage paths.' }, file: 'architecture' },
  { id: 'dashboard', group: '1', title: { fa: 'داشبورد و پایش', en: 'Dashboard & monitoring' }, summary: { fa: 'شاخص‌ها، تاریخچه، نمودارها، آستانه‌ها و اعلان‌ها.', en: 'Metrics, history, charts, thresholds, and alerts.' }, file: 'dashboard' },
  { id: 'system-storage', group: '1', title: { fa: 'سیستم، دیسک و فضای ذخیره‌سازی', en: 'System, disks & storage' }, summary: { fa: 'نمایش منابع میزبان، Mountها و بررسی مصرف دیسک.', en: 'Host resources, mount inventory, and disk usage analysis.' }, file: 'system-storage' },
  { id: 'network-firewall', group: '1', title: { fa: 'شبکه، پورت و UFW', en: 'Network, ports & UFW' }, summary: { fa: 'رابط‌ها، ترافیک، پورت‌ها و مدیریت امن فایروال.', en: 'Interfaces, traffic, ports, and guarded firewall controls.' }, file: 'network-firewall' },
  { id: 'nginx', group: '2', title: { fa: 'مدیریت Nginx و سایت‌ها', en: 'Nginx & websites' }, summary: { fa: 'سایت‌ها، ویرایشگر، دامنه، SSL، نگهداری و منابع.', en: 'Sites, config editor, domains, TLS, maintenance, and resources.' }, file: 'nginx' },
  { id: 'docker', group: '2', title: { fa: 'مدیریت Docker', en: 'Docker management' }, summary: { fa: 'وضعیت Engine، کانتینرها، Imageها، Volumeها و Compose.', en: 'Engine, containers, images, volumes, networks, and Compose.' }, file: 'docker' },
  { id: 'files', group: '2', title: { fa: 'مدیریت فایل‌ها', en: 'File manager' }, summary: { fa: 'مرور، انتقال، ویرایش، آرشیو و دانلود فایل‌های VPS.', en: 'Browse, transfer, edit, archive, and download VPS files.' }, file: 'files' },
  { id: 's3', group: '2', title: { fa: 'فضای ابری S3', en: 'S3 storage' }, summary: { fa: 'اتصال Provider، Bucket، فایل‌ها و انتقال VPS؛ قابلیت‌های واقعی نسخه.', en: 'Providers, buckets, object access, VPS transfers, and release limits.' }, file: 's3' },
  { id: 'mysql', group: '2', title: { fa: 'مدیریت MySQL و MariaDB', en: 'MySQL & MariaDB' }, summary: { fa: 'نصب، ورود مجدد مدیر، بانک‌ها، SQL و کاربران دیتابیس.', en: 'Install, re-authentication, databases, SQL, and DB users.' }, file: 'mysql' },
  { id: 'packages-services', group: '2', title: { fa: 'APT و سرویس‌های systemd', en: 'APT & systemd services' }, summary: { fa: 'مدیریت پکیج‌ها و عملیات محدودشدهٔ سرویس‌ها.', en: 'Package operations and guarded system service controls.' }, file: 'packages-services' },
  { id: 'terminal', group: '2', title: { fa: 'ترمینال سرور', en: 'Server terminal' }, summary: { fa: 'ترمینال چندتب، clipboard، اتصال مجدد و Root Terminal.', en: 'Multiple terminal tabs, clipboard, reconnect, and root terminal.' }, file: 'terminal' },
  { id: 'accounts-settings', group: '3', title: { fa: 'حساب‌ها، کاربران و تنظیمات', en: 'Accounts, users & settings' }, summary: { fa: 'نقش‌ها، نشست‌ها، گذرواژه، تاریخچه و تنظیمات میزبان.', en: 'Roles, sessions, passwords, audit trail, and host settings.' }, file: 'accounts-settings' },
  { id: 'updates', group: '3', title: { fa: 'به‌روزرسانی، لغو و بازگشت', en: 'Updates, cancellation & rollback' }, summary: { fa: 'شاخه‌ها، حالت Cache، مراحل، خطاها و بازیابی نسخه.', en: 'Branches, cache modes, progress, failure handling, and rollback.' }, file: 'updates' },
  { id: 'security', group: '3', title: { fa: 'مدل امنیتی و توصیه‌ها', en: 'Security model & guidance' }, summary: { fa: 'مرز دسترسی‌ها، اسرار، HTTPS و کارهای امن مدیر.', en: 'Privilege boundaries, secrets, HTTPS, and safe operations.' }, file: 'security' },
  { id: 'troubleshooting', group: '3', title: { fa: 'عیب‌یابی', en: 'Troubleshooting' }, summary: { fa: 'خطاهای پرتکرار و گام‌های تشخیص و بازیابی.', en: 'Common failures and practical diagnosis and recovery steps.' }, file: 'troubleshooting' },
  { id: 'development', group: '4', title: { fa: 'توسعه و مشارکت', en: 'Development & contributing' }, summary: { fa: 'ساخت محلی، تست، ساختار کد، نسخه‌بندی و تغییر مستندات.', en: 'Local setup, checks, code structure, versioning, and docs edits.' }, file: 'development' },
  { id: 'glossary', group: '4', title: { fa: 'واژه‌نامه', en: 'Glossary' }, summary: { fa: 'تعریف اصطلاحات رابط و زیرساخت.', en: 'Definitions for interface and infrastructure terms.' }, file: 'glossary' },
]
