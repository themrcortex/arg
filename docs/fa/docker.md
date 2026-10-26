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

## فهرست تب‌ها و عملیات

دادهٔ صفحه با cache کوتاه برای جابه‌جایی سریع برمی‌گردد؛ دکمهٔ تازه‌سازی snapshot را با درخواست تازه جایگزین می‌کند. تغییر وضعیت در `busy` انجام می‌شود؛ تا پایان همان عملیات دکمه‌های متناقض را تکرار نکن. بالا بودن Docker Engine به معنی سالم بودن همهٔ Containerها نیست.

- **کانتینرها:** نام، image، وضعیت، پورت‌ها و فرمان‌های Start/Stop/Restart/Inspect/Logs در دسترس‌اند. Stop ممکن است درخواست‌های فعال را قطع کند. قبل از حذف Container، volumeهای متصل و bind mountها را بخوان؛ دادهٔ مهم معمولاً در volume است، اما حذف volume می‌تواند برگشت‌ناپذیر باشد.
- **Imageها:** pull، فهرست، حذف یا tag کردن Image. Pull از Registry اینترنت و فضای دیسک استفاده می‌کند. Image در حال استفاده حذف نمی‌شود یا با خطای Docker مواجه می‌شود. حذف image به معنی حذف volume دیتابیس نیست.
- **Networkها:** اطلاعات اتصال و شبکه‌های Docker؛ حذف Network متصل به Container ممکن نیست تا اتصال‌ها جدا شوند.
- **Volumeها:** volumeهای پایدار. قبل از حذف، containerهای استفاده‌کننده را Inspect کن و از backup دیتابیس مطمئن شو. حجم volume می‌تواند زمان‌بر و نیازمند اجازهٔ دسترسی باشد.
- **Compose:** مشاهده و اجرای فایل Compose در مسیر مدیریت ARG. قبل از `up` فایل و project name را بررسی کن؛ فرمان ممکن است چند سرویس را بازسازی یا restart کند. متغیرهای محرمانه را در UI یا لاگ share نکن.

### ساخت Container ساده

فرم **نام**، **Image**، **Port mapping**، Environment، Network و Restart policy را دریافت می‌کند. نمونه: نام `web-demo`, image `nginx:alpine`, mapping `127.0.0.1:8081:80`, restart=`unless-stopped`. Bind به `127.0.0.1` دسترسی مستقیم عمومی را نمی‌دهد و برای reverse proxy Nginx مناسب‌تر است. اگر `0.0.0.0:8081:80` بگذاری سرویس روی همهٔ رابط‌ها bind می‌شود؛ قبل از آن UFW و نیاز عمومی را بررسی کن. Environment مثل رمز DB در Compose/metadata ممکن است قابل مشاهده باشد؛ secret را در آن وارد نکن مگر روش محافظت‌شده داشته باشی.

### Build، Run و پاک‌سازی

فرم Build از **Context directory** و tag می‌سازد؛ تمام فایل‌های داخل context ممکن است به Docker daemon ارسال شوند، پس `.dockerignore` تنظیم کن تا `.env`, `.git`, dumps و `node_modules` وارد build نشوند. Tag مثل `local/my-app:latest` باید مشخص باشد. Build cache حجم زیادی اشغال می‌کند؛ قبل از prune صفحهٔ Images/`docker system df -v` را ببین. `prune` می‌تواند لایهٔ قابل‌استفاده برای build بعدی را پاک کند و فقط زمان rebuild را بیشتر کند؛ `--volumes` یا پاک‌سازی volume را هرگز بدون فهم اثر داده‌ای انتخاب نکن.

### عیب‌یابی نمونه

اگر Container `Exited` شد، Logs و Exit code را باز کن، سپس Env، mount path، پورت اشغال‌شده و healthcheck را بررسی کن. Restart متوالی بدون رفع علت فقط loop می‌سازد. اگر اپ از بیرون دیده نمی‌شود، خروجی `docker ps` mapping، Listener host، فایروال و Nginx upstream را کنار هم بررسی کن. آمار CPU/RAM داشبورد کل host است؛ برای یک Container از جزئیات Docker و cgroup همان Container استفاده کن.

## خواندن وضعیت و حفظ داده

**Image** قالب immutable ساخت کانتینر است؛ **Container** نمونهٔ در حال اجرای آن؛ **Volume** دادهٔ پایدار؛ **Bind mount** مسیر واقعی میزبان؛ **Network** مسیر ارتباط Containerها. حذف Container لزوماً Image را حذف نمی‌کند؛ حذف Image هم volume را پاک نمی‌کند مگر عملیات مشخص صریحاً آن را هدف بگیرد. پیش از Upgrade، image tag فعلی را یادداشت و داده را backup کن؛ `latest` به‌تنهایی شناسهٔ نسخهٔ ثابت نیست.

در کارت Container مقدار status و healthcheck را جدا بخوان. `Up` فقط یعنی process اصلی هنوز اجراست؛ `healthy` نتیجهٔ healthcheck است. Restart count پی‌درپی نشانهٔ crash loop است. Logs را با زمان و error context بخوان؛ ممکن است شامل connection string یا توکن باشد، پس در issue عمومی redact کن. Inspect پورت‌های bind، mountها، command و image را نشان می‌دهد؛ Environment را محرمانه فرض کن.

### Compose و تغییر نسخه

پروژه‌های Compose با فایل و project name مدیریت می‌شوند. پیش از Start/Apply، متن compose را بررسی کن: `ports`, `volumes`, `environment`, `restart`, `depends_on` و image tag روی میزبان واقعی اثر دارند. volume نسبی معمولاً نسبت به پوشهٔ فایل Compose resolve می‌شود. تغییر مسیر volume ممکن است دیتابیس خالی تازه‌ای بسازد و به‌اشتباه شبیه پاک‌شدن داده به‌نظر برسد. پیش از update، مسیر Volume فعلی و backup را ثبت کن؛ پس از اجرا healthcheck و اتصال از Nginx را تست کن.

### فضای اشغال‌شده

Docker image layer، writable layer، volume و build cache جدا هستند. `docker system df -v` را برای تشخیص منبع مصرف بخوان. Container size می‌تواند کوچک باشد ولی volume دیتابیس چند گیگابایت باشد. build cache معمولاً از buildهای قبلی استفاده می‌کند؛ پاک‌کردنش دادهٔ runtime نیست اما build بعدی را کندتر می‌کند. prune image فقط image استفاده‌نشده را هدف می‌گیرد؛ prune all و volume cleanup دامنهٔ اثر بزرگ‌تری دارند.
