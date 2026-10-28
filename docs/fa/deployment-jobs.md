# استقرار منبع و Jobهای پایدار

این راهنما workflow پایهٔ استقرار را توضیح می‌دهد. در این workflow، ARG سورس را در یک release مجزا دریافت می‌کند، محتوا را بررسی می‌کند و اشاره‌گر `current` را با تغییر اتمی به نسخهٔ جدید می‌برد. Job و تاریخچهٔ مرحله‌ها در SQLite می‌مانند؛ بستن صفحه یا اتصال مرورگر، Job را پاک نمی‌کند.

## محدودهٔ فعلی

از صفحهٔ «اپلیکیشن‌ها»، روی کارت یک اپلیکیشن باز کن و «استقرار منبع» را بزن. سه منبع موجود است:

1. **Git عمومی HTTPS:** مخزن از GitHub، GitLab، Bitbucket یا Codeberg به‌همراه branch، tag یا commit دریافت می‌شود. URL باید بدون نام‌کاربری، رمز یا token باشد.
2. **Git خصوصی SSH:** مخزن خصوصی GitHub را با Deploy Key مخصوص همان مخزن دریافت کن. روند ساخت کلید، ثبت در GitHub و محدودیت‌ها در [مدیریت کلید SSH و Deploy Key](./ssh-deploy-keys.md) آمده است.
3. **پوشهٔ موجود VPS:** پوشه باید زیر `/var/www` یا `/opt/arg/apps` باشد. symlink در هر جای درخت منبع رد می‌شود.
4. **فایل ZIP:** بارگذاری تا ۲۵۶ MiB است. حجم بازشده حداکثر ۲ GiB و تعداد ورودی‌ها حداکثر ۲۰٬۰۰۰ است.

ARG فایل‌ها را در `/var/lib/arg/deployments/apps/<application-id>/releases/<job-id>` نگه می‌دارد. مسیر `current` یک symlink نسبی است که تنها بعد از بررسی release، با rename اتمی عوض می‌شود. release قبلی حفظ می‌شود و از Job موفق می‌توان به release قبلی برگشت.

برای کنترل مصرف دیسک، پنج release تازه به‌علاوهٔ release فعال و قبلی نگه داشته می‌شوند. releaseهای قدیمی‌تر از مسیر مدیریت‌شده پاک و سابقهٔ آن‌ها با وضعیت `expired` در دیتابیس باقی می‌ماند. فایل موقت ZIP ناقص که بیش از یک ساعت مانده باشد هنگام شروع Worker پاک می‌شود؛ ZIPهای Job ناموفق برای Retry تا ۷ روز باقی می‌مانند.

## وضعیت‌ها و مشاهدهٔ پیشرفت

وضعیت‌های اصلی `queued`، `validating`، `running`، `verifying` و یکی از `succeeded`، `failed` یا `cancelled` هستند. هر تغییر مرحله همراه پیام، زمان شروع و پایان در `deployment_job_steps` ثبت می‌شود. API رویداد SSE دارد و رابط فعلی از polling تاریخچه استفاده می‌کند؛ refresh صفحه، دادهٔ مرحله‌ها را از دست نمی‌دهد.

یک اپلیکیشن در هر زمان حداکثر یک Job فعال دارد. Worker در هر VPS یک Job را اجرا می‌کند، lease و heartbeat می‌نویسد و بعد از شروع دوبارهٔ ARG، Job نیمه‌کاره را برای اجرای امن مجدد به صف می‌برد. درخواست‌های تکراری با همان Idempotency key نتیجهٔ همان Job را برمی‌گردانند. برای اجرای دوباره، Retry یک کلید تازه می‌سازد.

## بررسی امنیتی فایل‌ها

- ZIPهای بزرگ، نسبت فشرده‌سازی مشکوک، traversal مثل `../../etc/passwd`، symlink و فایل device رد می‌شوند.
- در کپی منبع، symlink و فایل خاص رد می‌شود؛ فایل‌های `.env*`، `.git` و `node_modules` وارد release نمی‌شوند.
- حجم release سقف ۲ GiB دارد و پیش از کپی/استخراج، فضای آزاد بررسی می‌شود.
- Git با `argv` ثابت اجرا می‌شود؛ hook و script پروژه اجرا نمی‌شود. timeout دریافت ۱۸۰ ثانیه است.
- در HTTPS عمومی هیچ credentialی در URL مجاز نیست. برای SSH خصوصی، کلید Ed25519 با مجوز محدود زیر `/var/lib/arg/ssh-keys` نگهداری می‌شود و Git با `BatchMode` و بررسی سخت‌گیرانهٔ Host Key اجرا می‌شود.
- Deploy Key را در GitHub فقط برای مخزن مربوطه و ترجیحاً فقط‌خواندنی ثبت کن. ARG به GitHub دسترسی API برای افزودن کلید ندارد.

## لغو، Retry و Rollback

- **لغو:** Job صف‌شده فوراً `cancelled` می‌شود. برای Job در حال اجرا، لغو ثبت می‌شود و Worker در مرز مرحلهٔ امن کار را متوقف می‌کند؛ پردازش Git محدود به timeout است.
- **Retry:** Job ناموفق Git یا پوشه دوباره با همان منبع و کلید جدید صف می‌شود. برای ZIP، فایل موقت فقط تا وقتی موجود باشد قابل Retry است؛ در غیر این صورت ZIP را دوباره بارگذاری کن.
- **Rollback:** فقط آخرین release قبلی همان اپ قابل انتخاب است. ARG مسیر و مالکیت release را بررسی و symlink `current` را اتمی جابه‌جا می‌کند. فایل‌ها پاک نمی‌شوند.
- **شکست پیش از فعال‌سازی:** release جاری تغییر نمی‌کند. Job موفق تنها پس از تغییر اشاره‌گر و ثبت نتیجه اعلام می‌شود.

## اتصال Runnerهای Runtime که بعد از موتور پایه اضافه شدند

موتور پایهٔ Deploy هنوز فرمان دلخواه پروژه را اجرا نمی‌کند. اتصال Runtime فقط وقتی فعال می‌شود که برای چارچوب اپ Runner مشخص پیاده شده باشد:

- **Laravel:** [مدیریت Laravel](./laravel-runner.md) نصب Composer از lockfile، فایل محیط خصوصی، عملیات تأییدشدهٔ Artisan، Nginx/PHP-FPM و سرویس‌های اختیاری را توضیح می‌دهد.
- **Node.js:** [اجرای اپلیکیشن Node.js](./node-runner.md) lockfileهای پشتیبانی‌شده، نصب و build محدود، محیط Runtime، سرویس systemd مختص اپ، بررسی loopback، reverse proxy کنترل‌شدهٔ Nginx و بازیابی release را توضیح می‌دهد.
- **چارچوب‌های دیگر:** Python، WordPress، اجرای فرمان سفارشی، انتشار استاتیک React/Vite و SSR مربوط به Next.js/Nuxt هنوز در این Runnerها نیستند. صرف فعال‌شدن release به معنی Running بودن این برنامه‌ها نیست.

فیلد اختیاری `healthcheck_file` وجود یک فایل معمولی در release را بررسی می‌کند. Runnerهای Laravel و Node بررسی فعال‌سازی مخصوص خودشان را دارند؛ بررسی وجود فایل جای بررسی Runtime را نمی‌گیرد. rollback کد، migration دیتابیس یا اثرهای خارجی ایجادشده توسط کد اپ را برنمی‌گرداند.

## API

- `POST /api/apps/{id}/deploy` — منبع Git یا پوشه؛ JSON شامل `source`, `idempotency_key` و `healthcheck_file` اختیاری.
- `POST /api/apps/{id}/deploy/archive` — بدنهٔ خام ZIP با `Content-Type: application/zip` و هدر `Idempotency-Key`.
- `GET /api/apps/{id}/deploy/jobs` — ۵۰ Job اخیر اپلیکیشن.
- `GET /api/deploy/jobs/{id}` — جزئیات و مرحله‌ها.
- `GET /api/deploy/jobs/{id}/events` — رویدادهای Server-Sent Events؛ نشست مدیر در طول stream دوباره بررسی می‌شود.
- `POST /api/deploy/jobs/{id}/cancel` و `POST /api/deploy/jobs/{id}/retry`.
- `POST /api/apps/{id}/deploy/rollback` — بدنهٔ `{"release_id":"…"}`.
- Node Runner: `GET /api/apps/{id}/node`، `GET|PUT /api/apps/{id}/node/service`، `PUT /api/apps/{id}/node/environment` و `GET /api/apps/{id}/node/service/logs`.

همهٔ نوشتن‌ها نیازمند نشست مدیر، Origin معتبر و رویداد audit هستند. مسیر مقصد از UUID اپ تولید می‌شود و از درخواست کاربر گرفته نمی‌شود.
