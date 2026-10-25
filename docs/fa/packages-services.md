## APT

مدیریت بسته‌ها برای Debian/Ubuntu عملیات پشتیبانی‌شدهٔ APT مانند update metadata، upgrade، install، remove، autoremove و clean را اجرا می‌کند. هر کارت عملیات نشان می‌دهد کدام فرمان سیستمی را آغاز می‌کند و وضعیت/پیشرفت همان عملیات را نمایش می‌دهد.

- `apt update`: فهرست package repository را تازه می‌کند؛ خود بسته‌ها را ارتقا نمی‌دهد.
- `apt upgrade`: بسته‌های قابل ارتقا را نصب می‌کند؛ تغییرات می‌تواند سرویس را restart کند.
- `apt install`: بسته و dependencyها را نصب می‌کند.
- `apt remove`: بسته را حذف می‌کند؛ config ممکن است باقی بماند.
- `apt purge`: بسته و config package را پاک می‌کند.
- `autoremove`: dependencyهای خودکار و بدون استفاده را حذف می‌کند؛ فهرست حذف را پیش از تأیید بخوان.
- `clean` و `autoclean`: cache دانلودشده را پاک می‌کنند و بستهٔ نصب‌شده را uninstall نمی‌کنند.

Upgrade عمدهٔ توزیع یا تغییر repository در scope مدیریت معمول APT پنل نیست. پیش از ارتقای تولیدی، snapshot و دسترسی کنسول ارائه‌دهنده داشته باش.

## سرویس‌های systemd

فهرست سرویس‌ها و وضعیت active/enabled از systemd می‌آید. عملیات start/stop/restart/reload/enable/disable مطابق موارد مجاز UI قابل اجراست. `active` یعنی اکنون اجرا می‌شود؛ `enabled` یعنی برای boot فعال شده است؛ این دو مستقل‌اند.

ARG سرویس‌های حیاتی مثل SSH، شبکه و خود ARG را از عملیات مختل‌کننده محافظت می‌کند. سرویس‌های دیگر را هم با احتیاط تغییر بده: توقف database، reverse proxy یا firewall ممکن است سایت یا اتصال مدیریتی را قطع کند.

```bash
systemctl status nginx
systemctl is-enabled nginx
journalctl -u nginx -n 100 --no-pager
```

## بسته‌های وابستگی

Docker، Rclone، Certbot و ابزارهای دیگر می‌توانند dependency اختیاری باشند. بازکردن صفحه نباید همه را خودکار نصب کند. نسخهٔ package به مخزن و توزیع وابسته است؛ نسخهٔ نصب‌شده و پیام خطا را بررسی کن.
