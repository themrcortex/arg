# انتشار سایت‌های Static و اجرای SSR

این راهنما رفتار انتشار React، Vue، Vite، Next.js و Nuxt را در صفحهٔ «اپلیکیشن‌ها» توضیح می‌دهد. انتخاب قالب تعیین می‌کند ARG خروجی را با Nginx سرو کند یا برای آن یک پردازش Node.js بسازد.

## انتخاب نوع اجرا

هنگام ساخت یا ویرایش اپ، فیلد «قالب پیشنهادی» را روی «تشخیص خودکار» بگذار یا یک قالب را دستی انتخاب کن. حالت خودکار manifest پروژه را می‌خواند:

| قالب کشف‌شده | رفتار انتشار |
| --- | --- |
| React، Vue یا Vite | اجرای `scripts.build` و انتشار فایل خروجی با Nginx؛ بدون سرویس Node برای درخواست‌های وب |
| Next.js یا Nuxt | نصب وابستگی و build، سپس اجرای سرویس SSR روی loopback با systemd؛ از `scripts.start` یا entrypoint استاندارد استفاده می‌شود |
| Node.js عمومی | نیاز به `scripts.start`؛ سرویس محدودشدهٔ systemd و reverse proxy Nginx |
| فایل Static از پیش ساخته | انتشار پوشه‌ای که `index.html` دارد؛ نصب Node لازم نیست |

انتخاب دستی `React SPA`، `Vue SPA`، `Vite SPA` یا «فایل‌های استاتیک آماده» تشخیص را برای انتشار استاتیک override می‌کند. انتخاب `Next.js SSR`، `Nuxt SSR` یا `Node.js SSR` اجرا را به حالت SSR محدود می‌کند. «تشخیص خودکار» برای dependencyهای React/Vue/Vite حالت Static و برای Next/Nuxt یا پروژهٔ Node دارای `start` حالت SSR را برمی‌گزیند.

## مسیر خروجی و SPA fallback

فیلد «مسیر خروجی سایت استاتیک» مسیر نسبی زیر release است؛ مقدار پیش‌فرض `auto` است. در حالت خودکار، ARG به‌ترتیب خروجی‌های شناخته‌شده را بررسی می‌کند: React معمولاً `build`، Vite/Vue معمولاً `dist`، Next export معمولاً `out` و Nuxt معمولاً `.output/public`. خروجی باید `index.html` عادی داشته باشد. اگر پروژه مسیر دیگری دارد، مثلاً `web/public-site`، همان مسیر را دستی وارد کن. مقدار `.` یعنی `index.html` در ریشهٔ release قرار دارد.

اگر منبع از قبل Static است، ساختار ZIP یا پوشه باید خروجی نهایی را در همان مسیر انتخابی داشته باشد. می‌توانی مسیر سفارشی مانند `customer/site` را هم وارد کنی؛ در حالت تشخیص خودکار، این مسیر برای خروجی آمادهٔ بدون `package.json` بررسی می‌شود. اگر پروژه `package.json`، یک lockfile پشتیبانی‌شده و `scripts.build` داشته باشد، ARG وابستگی‌ها را در محیط build محدود نصب و build را اجرا می‌کند. اگر این سه مورد فراهم نباشند، ARG به‌عنوان فایل آماده دنبال `index.html` می‌گردد؛ هیچ package manager دلخواهی اجرا نمی‌شود.

گزینهٔ SPA fallback درخواست‌هایی مانند `/settings/profile` را به `index.html` می‌فرستد تا Router سمت مرورگر مسیر را باز کند. آن را برای برنامهٔ تک‌صفحه‌ای روشن نگه دار. برای وب‌سایت چندصفحه‌ای معمولی خاموشش کن تا مسیر ناشناخته `404` بگیرد.

## نسخهٔ Node و فرمان‌های build

برای قالب‌های Node-based، نسخهٔ Node نصب‌شده از inventory همان VPS انتخاب می‌شود. مقدار build جدا از مقدار runtime است. React/Vue/Vite فقط به Node زمان build نیاز دارند؛ بعد از انتشار، Node برای پاسخ‌گویی به بازدیدکننده اجرا نمی‌شود. Next/Nuxt و Node عمومی به Node در زمان اجرا نیز نیاز دارند.

ARG lockfile یکتا را می‌خواهد: `package-lock.json`، `pnpm-lock.yaml` یا `yarn.lock` مربوط به Yarn Classic. وجود lockfileهای متعارض یا Yarn Berry در این مسیر پذیرفته نمی‌شود. `scripts.build` و `scripts.start` از `package.json` همان منبع خوانده می‌شوند. اگر `scripts.start` وجود نداشته باشد، Next از CLI استاندارد `next start` و Nuxt از `.output/server/index.mjs` استفاده می‌کند؛ این entrypointها به loopback و پورت ثبت‌شده محدود می‌شوند. متغیرهای `HOST`, `PORT`, `NITRO_HOST` و `NITRO_PORT` برای اتصال محلی تنظیم می‌شوند. بخش «متغیرهای عمومی زمان Build» فقط نام‌های دارای پیشوند `VITE_`, `NEXT_PUBLIC_`, `NUXT_PUBLIC_`, `REACT_APP_` یا `PUBLIC_` را می‌پذیرد و مقدارها را تنها به مرحلهٔ build می‌دهد؛ install، Runtime و `.env` خصوصی به آن دسترسی ندارند. چارچوب‌های رایج این مقدارها را در JavaScript تحویلی به مرورگر قرار می‌دهند، پس رمز، Token یا credential در آن نگذار. install lifecycle scriptها فقط داخل unit موقت systemd با DynamicUser، سقف زمان/منبع و فایل‌سیستم محدود اجرا می‌شوند.

## دامنه و Nginx

برای انتشار Static یا ساخت reverse proxy، دامنه را در مشخصات اپ وارد کن. دامنه باید در ARG/Nginx با سایت دیگری تداخل نداشته باشد. در اولین انتشار Static، ARG یک فایل سایت مختص همان Application می‌سازد که root آن به `current/<output>` اشاره می‌کند. بازگشت نسخه، مسیر Nginx را با خروجی release قبلی هماهنگ می‌کند. تنظیم Nginx قبل از فعال‌شدن در دسترس‌بودن فایل‌ها و ساختار خروجی بررسی می‌شود؛ در خطا pointer نسخه برمی‌گردد.

برای Assetهایی که نام hash هشت‌رقمی یا بیشتر دارند، cache طولانی immutable تنظیم می‌شود. فایل‌های مخفی به‌جز مسیر challenge مجاز و پسوندهای حساس مانند `.env`, `.ini`, `.log`, `.sql`, `.zip` مسدود می‌شوند. انتشار اولیه HTTP است؛ پس از بررسی سایت، گواهی را از ابزار SSL صفحهٔ Nginx بگیر. ARG برای Static سرویس systemd وب نمی‌سازد و پورت برنامه لازم نیست.

## Deploy، بررسی و Rollback

از کارت اپ، منبع عمومی Git، ZIP یا پوشهٔ مجاز VPS را انتخاب کن. هر Deploy release جدا می‌سازد، منبع را بررسی می‌کند، در صورت نیاز build را اجرا می‌کند و قبل از تغییر Nginx وجود `index.html` را کنترل می‌کند. مراحل در Job ذخیره می‌شوند؛ Refresh مرورگر، سوابق را پاک نمی‌کند.

برای SSR، unit فقط وقتی restart می‌شود که پیش از Deploy فعال بوده باشد؛ listener و درخواست محلی بررسی می‌شوند. برای Static، خروجی با Nginx منتشر می‌شود و process تازه‌ای ایجاد نمی‌شود. دکمهٔ Rollback release قبلی را فعال می‌کند؛ در Static، مسیر خروجی Nginx نیز دوباره از روی نسخهٔ انتخاب‌شده محاسبه می‌شود. این Rollback تغییرهای بیرونی یا migration دیتابیس را برنمی‌گرداند.

## عیب‌یابی

- **خروجی پیدا نشد:** فایل‌های release را بررسی کن و مسیر خروجی را روی پوشه‌ای بگذار که `index.html` دارد؛ حروف مسیر مهم‌اند.
- **build script پیدا نشد:** قالب را روی فایل Static آماده بگذار یا `scripts.build` را در manifest پروژه تعریف کن.
- **Lockfile متعارض:** فقط lockfile package manager انتخابی را در repository نگه دار.
- **دامنه در Nginx موجود است:** دامنه یا سایت موجود را بررسی کن؛ ARG پیکربندی متعلق به مدیر را overwrite نمی‌کند.
- **SSR اجرا نشد:** `scripts.start` یا entrypoint استاندارد Next/Nuxt، نسخهٔ runtime، `listen_port` آزاد و خطاهای unit را بررسی کن. اگر برنامه خارج از loopback گوش دهد، ARG آن را متوقف می‌کند.
- **SPA route بعد از refresh خطا می‌دهد:** SPA fallback را روشن و سپس Deploy کن.

## مرزهای فعلی

این قابلیت فایل‌های آمادهٔ Static، خروجی SPAهای رایج، entrypoint استاندارد Next/Nuxt و SSR عمومی مبتنی بر `scripts.start` را پوشش می‌دهد. adapterهای سفارشی Next/Nuxt، مدیریت CDN، دامنهٔ wildcard، secret injection امن مخصوص build و اجرای یکپارچهٔ واقعی روی Debian/Ubuntu باید جداگانه اعتبارسنجی شوند. موفقیت تست محلی به‌تنهایی جای آزمون Nginx و systemd روی VPS آزمایشی را نمی‌گیرد.
