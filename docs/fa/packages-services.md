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

## APT: معنی عملیات و روال امن

صفحهٔ بسته‌ها فهرست Packageهای نصب‌شده و وضعیت مخزن را نشان می‌دهد. دکمه‌های بالای صفحه معادل عملیات سیستمی‌اند و Command کنار آن‌ها باید قبل از اجرا خوانده شود:

- `apt update` فهرست نسخه‌های مخازن را تازه می‌کند؛ خودش package نصب نمی‌کند.
- `apt upgrade` بسته‌های قابل ارتقا را نصب می‌کند و ممکن است سرویس‌ها را restart کند.
- `apt install <package>` بسته و dependencyهای لازم را نصب می‌کند.
- `apt remove <package>` برنامه را برمی‌دارد اما ممکن است config باقی بماند.
- `apt purge <package>` package و configهای بسته را حذف می‌کند؛ داده‌های خارج از package لزوماً حذف نمی‌شوند.
- `apt autoremove` dependencyهای دیگر لازم‌ندار را پیشنهاد/حذف می‌کند؛ فهرست را بررسی کن.
- `apt clean` cache دانلودشده را پاک می‌کند، نه بسته‌های نصب‌شده را.

انتخاب کارت/ردیف بسته را با نام و نسخه تطبیق بده. Progress روی کارت یا عملیات نشان‌دهندهٔ مرحله و خروجی جاری است؛ تا وقتی وضعیت نهایی و Exit status نیامده، refresh یا اجرای دوم نزن. Upgrade سیستم عامل با ARG update فرق دارد: اولی بسته‌های Debian/Ubuntu را تغییر می‌دهد و دومی خود ARG را جایگزین می‌کند.

## سرویس‌های systemd

برای unit، نام دقیق مانند `nginx.service`, `docker.service`, `mysql.service` یا `ssh.service` را بخوان. Start سرویس متوقف را اجرا می‌کند؛ Stop دسترسی وابسته را قطع می‌کند؛ Restart downtime کوتاه دارد؛ Reload فقط اگر unit پشتیبانی کند config را دوباره می‌خواند؛ Enable/Disable رفتار boot بعدی را تنظیم می‌کند و الزاماً وضعیت فعلی را عوض نمی‌کند. برای مثال Disable کردن Nginx ممکن است بعد از reboot سایت را از دسترس خارج کند، حتی اگر اکنون Running باشد.

سرویس‌های شبکه/ARG/SSH حیاتی هستند. قبل از Stop/Disable آن‌ها وابستگی و مسیر بازگشت را بررسی کن؛ سرویس ssh را از تنها نشست راه دور متوقف نکن. اگر Action برگشت خطا داد، `systemctl status <unit>` و logهای systemd را بخوان؛ اجرای دوبارهٔ Restart بدون فهم خطا ممکن است اثر را بدتر کند.
