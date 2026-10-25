## صفحه باز نمی‌شود

1. `sudo arg status` را اجرا کن؛ اگر service فعال نیست، `sudo arg start` و journal را ببین.
2. `sudo ss -lntp` را برای پورت واقعی بررسی کن.
3. با SSH روی خود VPS تست کن: `curl -v http://127.0.0.1:8282/api/state` یا پورت پیکربندی‌شده.
4. اگر loopback جواب می‌دهد اما بیرون نه، Security Group/Firewall ارائه‌دهنده، UFW، bind عمومی و مسیر شبکه را بررسی کن.
5. اگر HTTPS است، domain/IP داخل SAN گواهی، پورت TLS، proxy headerها و زمان VPS را بررسی کن.

## اولین مدیر ساخته نمی‌شود

از آدرس همان origin استفاده کن. اگر درخواست می‌گوید فقط tunnel محلی مجاز است، HTTPS را تنظیم کن یا SSH port-forward را باز نگه‌دار. کد یک‌بارمصرف را از CLI سرور بگیر؛ کد قدیمی یا کد سرور دیگر پذیرفته نمی‌شود.

## UFW خاموش/در دسترس نیست

صفحه وضعیت نصب، بسته و سرویس را جدا می‌کند. اگر نصب نیست، نصب آن فقط ابزار را نصب می‌کند و لزوماً UFW را enable نمی‌کند. برای enable ابتدا SSH و پورت پنل را مجاز کن. اگر فرمان `ufw` وجود ندارد، APT/distro را بررسی کن؛ restart سرویس ARG جای نصب UFW را نمی‌گیرد.

## Nginx یا SSL سایت

```bash
sudo nginx -t
sudo systemctl status nginx
sudo journalctl -u nginx -n 100 --no-pager
sudo ss -lntp | grep -E ':80|:443'
```

گواهی سایت و گواهی پنل دو configuration جدا هستند. اگر Nginx هنگام صدور ACME متوقف شده بود و restore نشد، ابتدا آن را دستی راه‌اندازی و `nginx -t` کن.

## Update طولانی/گیرکرده

Build Rust می‌تواند چند دقیقه طول بکشد؛ spinner/elapsed/log و وضعیت backend را ملاک بگیر، نه زمان ثابت مرورگر. یک Update دوم شروع نکن. وضعیت را از SSH بررسی و در صورت قطع واقعی `cancel-update` را اجرا کن. اگر باینری تازه بالا نیاید، rollback و log را دنبال کن.

## MySQL

خطای option مسیر credential را با `--defaults-extra-file=/...` بررسی کن. `mysql` و `mariadb` cli در option parsing تفاوت نسخه‌ای دارند؛ log را کامل بگیر و اطلاعات رمز را حذف کن. اگر Access gate بسته شد، رمز حساب پنل و نقش Admin لازم است.

## فضای S3

- تست endpoint: DNS، TLS، Endpoint دقیق، region امضا و مجوز کلید را بررسی کن.
- Bucket خالی/لیست نشد: `ListBuckets` ممکن است مجاز نباشد؛ نام Bucket مشخص را به‌طور مستقیم بررسی کن.
- سهمیه نامشخص: از API Provider پشتیبانی نمی‌شود؛ در صورت داشتن قرارداد، سقف را دستی ثبت کن.
- مصرف دیر به‌روز شد: نتیجهٔ scan تا 24 ساعت cache است؛ اسکن دستی انجام بده و وضعیت scanning را صبر کن.
- Mount نشد: Rclone و FUSE، فایل credential، systemd unit، فضای Cache و مسیر امن/خالی را بررسی کن.
- انتقال قطع شد: منبع را نگه دار، log/وضعیت را بخوان، مقصد ناقص را بررسی و انتقال را تکرار کن.

## درخواست پشتیبانی

نسخه، commit، distro، command بی‌خطر برای بازتولید و پیام خطا را ارائه کن. Secret، setup code، Session Cookie، Private Key، دیتابیس مشتری یا log خام را منتشر نکن. ابتدا مخزن را برای Issue و changelog بررسی کن.
