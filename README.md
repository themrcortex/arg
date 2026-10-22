<div align="center">

# ◈ Donatello

### مدیریت سرور، ساده و خوش‌ساخت

پنل فارسی مدیریت VPS با **Rust + React + TypeScript** و رابط اختصاصی بنفش بر پایهٔ الگوهای **shadcn/ui**.

![Rust](https://img.shields.io/badge/backend-Rust-9868de?style=for-the-badge&logo=rust&logoColor=white)
![React](https://img.shields.io/badge/frontend-React%20%2B%20TypeScript-9868de?style=for-the-badge&logo=react&logoColor=white)
![Status](https://img.shields.io/badge/status-early%20development-563776?style=for-the-badge)

**توسعه‌دهنده: [𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁](https://t.me/mrc0rtex) · [@mrc0rtex](https://t.me/mrc0rtex)**

</div>

---

## Donatello چیست؟

Donatello یک پنل وب راست‌به‌چپ برای دیدن وضعیت و مدیریت پایهٔ سرور Linux است. سرور API و فایل‌های ساخته‌شدهٔ React را در یک باینری Rust ارائه می‌کند. پنل در نصب اولیه حساب مدیر می‌سازد و به‌صورت پیش‌فرض فقط روی `127.0.0.1` گوش می‌دهد.

### قابلیت‌های آماده

| بخش | قابلیت |
| --- | --- |
| نصب و امنیت | نصب‌گر وب، حساب مدیر اولیه، Argon2، نشست HttpOnly، محافظت Origin، محدودسازی تلاش ورود، تغییر گذرواژه و بستن نشست‌های دیگر |
| کاربران | افزودن حساب، نقش‌های مدیر/اپراتور/ناظر، فعال‌سازی یا غیرفعال‌سازی، جلوگیری از حذف آخرین مدیر |
| مانیتورینگ | مصرف CPU، حافظه، دیسک، uptime، load average و نرخ زندهٔ ترافیک شبکه |
| شبکه | نام، MAC، IPv4/IPv6، وضعیت و بایت‌های هر کارت؛ پورت‌های شنوندهٔ TCP/UDP |
| فایروال | نمایش خروجی واقعی `ufw status numbered` با مهلت اجرای محدود |
| حسابرسی | آخرین ۱۰۰ رویداد ورود و تغییرات مدیریتی |
| رابط | React و TypeScript، تم بنفش، طراحی واکنش‌گرا، فارسی و راست‌به‌چپ، کامپوننت‌های سفارشی shadcn/ui و Radix |

> [!IMPORTANT]
> ترمینال وب، تغییر قوانین UFW، تست سرعت، شمارندهٔ ترافیک هر پورت و صدور/تمدید خودکار SSL هنوز پیاده‌سازی نشده‌اند. صفحات پنل این مرز را مشخص می‌کنند. [نقشهٔ کامل ۴۰ قابلیتی](PLAN.md) در دسترس است.

## شروع سریع

**پیش‌نیاز:** Linux، Rust stable و Node.js 20 یا جدیدتر برای توسعهٔ فرانت.

```bash
# پس از دریافت فایل‌های پروژه
cd donatello
cargo run --release
```

بستهٔ ساخته‌شدهٔ React داخل `frontend/dist` نگه‌داری می‌شود تا اجرای معمول Rust به npm نیاز نداشته باشد. پس از اجرای برنامه، با SSH tunnel به آن وصل شوید:

```bash
ssh -L 8282:127.0.0.1:8282 user@your-server
```

سپس **http://127.0.0.1:8282** را باز کنید و در نصب‌گر حساب مدیر را بسازید.

### تنظیمات اجرا

| متغیر | پیش‌فرض | توضیح |
| --- | --- | --- |
| `DONATELLO_BIND` | `127.0.0.1:8282` | آدرس و پورت پنل |
| `DONATELLO_DB` | `data/donatello.db` | مسیر پایگاه دادهٔ SQLite |

```bash
DONATELLO_BIND=127.0.0.1:9000 \
DONATELLO_DB=/var/lib/donatello/donatello.db \
cargo run --release
```

برای دسترسی عمومی، پنل را پشت reverse proxy دارای HTTPS قرار دهید و پورت داخلی Donatello را عمومی نکنید. صدور خودکار گواهی در خود پنل هنوز آماده نیست.

## توسعه

```bash
# ترمینال اول: API و فایل‌های ساخته‌شده
cargo run

# ترمینال دوم: توسعهٔ React با proxy به API
cd frontend
npm ci
npm run dev
```

برای تولید نسخهٔ نهایی:

```bash
cd frontend && npm ci && npm run build
cd .. && cargo build --release
```

پس از هر تغییر در فرانت، `npm run build` را پیش از ساخت باینری Rust اجرا کنید؛ فایل‌های `frontend/dist` هنگام ساخت داخل باینری قرار می‌گیرند.

### ساختار پروژه

```text
src/main.rs                 API، نشست‌ها و خواندن اطلاعات Linux
frontend/src/App.tsx        نصب‌گر، ورود و پوستهٔ پنل
frontend/src/pages.tsx      داشبورد و صفحه‌های مدیریتی
frontend/src/components/ui  اجزای سفارشی shadcn/ui و Radix
frontend/dist               خروجی React که در Rust قرار می‌گیرد
PLAN.md                     برنامهٔ ۴۰ قابلیتی
```

## اصول امنیتی

- پیش‌فرض شبکه فقط `127.0.0.1:8282` است؛ برای دسترسی راه دور از SSH tunnel یا HTTPS reverse proxy استفاده کنید.
- گذرواژه‌ها با Argon2 هش می‌شوند. نشست‌ها در کوکی `HttpOnly; SameSite=Strict` و با زمان انقضا ذخیره می‌شوند.
- درخواست‌های تغییردهنده باید Origin همان میزبان را داشته باشند.
- تغییر گذرواژه، سایر نشست‌های کاربر را می‌بندد؛ تلاش‌های ورود متعدد برای ده دقیقه محدود می‌شوند.
- فایل پایگاه داده روی Unix با مجوز `0600` تنظیم می‌شود. سرویس را با کاربر سیستم محدود اجرا کنید.
- UFW در نسخهٔ فعلی فقط خوانده می‌شود تا تغییر ناخواسته دسترسی SSH یا پنل را قطع نکند.

## نقشهٔ ادامه

نقشهٔ کامل در [PLAN.md](PLAN.md) آمده است. اولویت‌های بعدی: مجوزهای دقیق برای عملیات، ترمینال وب محدود، مدیریت امن UFW، تست سرعت، ترافیک هر پورت، تنظیم پورت در پنل و ACME با توقف موقت سرویس مجاز و راه‌اندازی قطعی دوبارهٔ آن.

---

<div align="center">

ساخته‌شده با دقت توسط **[𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁](https://t.me/mrc0rtex)** · Telegram: **[@mrc0rtex](https://t.me/mrc0rtex)**

</div>
