## اجزای برنامه

```text
مرورگر (React + TypeScript)
        │ HTTPS/HTTP authenticated API + WebSocket terminal
        ▼
Backend ARG (Rust + Axum، کاربر سیستم arg)
        │ درخواست ساختاریافته روی Unix socket خصوصی
        ▼
arg-privileged.service (Helper محدود با دسترسی root)
        │
        ├── systemd / UFW / Nginx / Docker / APT / MySQL
        └── فایل‌ها و عملیات S3 مجاز
```

Backend و Frontend یک محصول واحد هستند. Frontend پس از Build به شکل منابع استاتیک در باینری Rust تعبیه می‌شود. نصب‌کننده جداگانه سرویس و Helper را با systemd نصب می‌کند. Helper ورودی را دوباره اعتبارسنجی می‌کند؛ Browser اجازهٔ اجرای shell آزاد در قالب endpoint مدیریتی ندارد.

## سرویس‌ها

- `arg.service`: پنل/API، با کاربر `arg` و تنظیمات محدودشدهٔ systemd.
- `arg-privileged.service`: عملیات نیازمند root از فهرست محدود. این سرویس یک root shell تعاملی برای پنل فراهم نمی‌کند.
- ترمینال عادی و Root Terminal دو مسیر جدا هستند. ترمینال Root نیازمند مدیر، هشدار صریح و احراز هویت مجدد طبق نسخهٔ جاری است.

## مسیرهای مهم

| مسیر | محتوا و اهمیت |
| --- | --- |
| `/usr/local/bin/arg` | فرمان CLI و کنترل‌سنتر Bash |
| `/usr/local/lib/arg/arg` | باینری Rust نصب‌شده |
| `/usr/local/lib/arg/update-helper` | Helper وضعیت، لغو و rollback Update |
| `/opt/arg/source` | Checkout منبع برای Update و Build |
| `/opt/arg/source/target` | فایل‌های کامپایل Rust؛ حالت Update تعیین می‌کند Cache بماند یا پاک شود |
| `/etc/arg/arg.env` | bind، پورت و مسیر دیتابیس سرویس؛ فایل تنظیمات root-owned |
| `/etc/arg/tls` | نسخهٔ امن‌شدهٔ گواهی و کلید خصوصی پنل |
| `/etc/arg/s3` | اطلاعات محرمانهٔ Provider در فایل‌های محدود دسترسی، نه در API عمومی |
| `/var/lib/arg/arg.db` | SQLite: کاربران، نشست‌ها، تنظیمات و تاریخچهٔ برنامه |
| `/var/lib/arg/rollback` | نسخهٔ قبلی باینری و snapshot مرتبط برای rollback |
| `/run/arg-update*` | قفل، درخواست لغو و وضعیت گذرای Update؛ با reboot پاک می‌شود |

مسیر دیتابیس ممکن است در `/etc/arg/arg.env` تغییر کرده باشد. پیش از کپی یا بازیابی از مسیر واقعی پیکربندی‌شده استفاده کن.

## مسیر یک عملیات مدیریتی

۱. Frontend درخواست را به API احراز هویت‌شده می‌فرستد. ۲. Backend نقش، Origin، اندازه و ساختار داده را بررسی می‌کند. ۳. برای عملیات سیستم، Backend درخواست مشخص و محدود را از Unix socket به Helper می‌دهد. ۴. Helper عملیات allowlist را اجرا می‌کند و نتیجهٔ کنترل‌شده را برمی‌گرداند. ۵. Backend رویداد Audit مناسب را می‌نویسد و UI وضعیت نهایی را نشان می‌دهد.

## پایگاه داده و Backup

SQLite در حالت WAL قابل استفاده است. Backup برنامه با snapshot سازگار انجام می‌شود تا فایل در حین نوشتن به‌صورت خام کپی نشود. Backup خارج از VPS نیز نگهداری کن؛ نسخه‌ای که فقط روی همان دیسک است در خرابی دیسک از دست می‌رود.
