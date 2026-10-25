## پیش‌نیاز و تشخیص

صفحهٔ Docker Engine و Compose نصب‌شده و در دسترس را تشخیص می‌دهد. بازکردن صفحه نباید Docker را خودکار نصب کند. اگر Docker نصب نیست، وضعیت dependency و مسیر نصب از صفحهٔ مدیریت قابل پیگیری است.

## موجودیت‌ها

- **Container:** process ایزوله بر پایهٔ image؛ وضعیت، زمان اجرا، port mapping و مصرف گزارش‌شده.
- **Image:** قالب read-only ساخت Container. حذف image استفاده‌شده می‌تواند به‌علت reference رد شود.
- **Volume:** دادهٔ پایدار خارج از لایهٔ writable کانتینر. حذف Volume می‌تواند دادهٔ دیتابیس یا فایل کاربر را از بین ببرد.
- **Network:** ارتباط و DNS داخلی بین Containerها؛ حذف آن ممکن است اتصال سرویس‌ها را قطع کند.
- **Compose:** گروه سرویس‌های تعریف‌شده در فایل Compose؛ `down`, `pull`, `up` می‌توانند Containerها را بازسازی کنند.
- **Build cache:** لایه‌های BuildKit که فضای دیسک می‌گیرند و ممکن است سرعت build آینده را بالا ببرند.

## کارهای مدیریتی

صفحه وضعیت و فهرست Docker را از daemon می‌خواند و برای actionهای موجود UI وضعیت در حال انجام و نتیجه را نشان می‌دهد. کنترل رایج Container شامل start/stop/restart/logs و عملیات Image/Volume/Network/Compose مطابق actionهای نسخهٔ جاری است. نام project، service، image و path را قبل از اجرا دوباره بررسی کن؛ مدیریت Docker معمولاً دسترسی تقریباً root به host می‌دهد.

## فضای دیسک

`docker system df -v` برای تفکیک Image، Container، volume و build cache مفید است. `docker system prune` می‌تواند Image یا شبکهٔ بدون استفاده را حذف کند. `docker compose down -v` Volumeهای پروژه را هم حذف می‌کند. از پاک‌کردن دستی `/var/lib/docker` یا `/var/lib/containerd` خودداری کن.

## منابع و محدودیت

مقدار مصرف Container از Docker daemon یا cgroup خوانده می‌شود و به نسخهٔ Engine، وضعیت accounting و runtime وابسته است. مقدار نمایش‌داده‌شده را با محدودیت واقعی مقایسه کن؛ خواندن مصرف معادل Resource Limit نیست. برای محدودکردن CPU/RAM از تنظیم resource خود Container/Compose یا systemd مناسب استفاده کن و اثر روی داده/سرویس را بدان.

## عیب‌یابی

```bash
docker info
docker ps -a
docker compose ls
docker system df -v
journalctl -u docker -n 100 --no-pager
```

اگر Docker Socket در دسترس نیست، وضعیت `docker.service`, permission و group را بررسی کن. اضافه‌کردن کاربر به گروه docker عملاً دسترسی سطح root می‌دهد.
