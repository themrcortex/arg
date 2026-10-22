<div align="center">

# ◈ DONATELLO

### مدیریت VPS با یک دستور

پنل فارسی و راست‌چین مدیریت سرور، ساخته‌شده با **Rust، React و TypeScript**؛ سبک، یکپارچه و آمادهٔ اجرا به‌صورت سرویس systemd.

![Rust](https://img.shields.io/badge/Rust-backend-8b5cf6?style=for-the-badge&logo=rust&logoColor=white)
![React](https://img.shields.io/badge/React-frontend-8b5cf6?style=for-the-badge&logo=react&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-Debian%20%7C%20Ubuntu-8b5cf6?style=for-the-badge&logo=linux&logoColor=white)

**توسعه‌دهنده: [𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁](https://t.me/mrc0rtex)**

</div>

---

## Quick Install · نصب سریع

**[⬇ دریافت مستقیم فایل Bash نصب Donatello](https://github.com/parsasajjadi2008/donatello/releases/latest/download/install.sh)**

روی Debian یا Ubuntu دارای systemd، فایل را دریافت و اجرا کنید. نیازی نیست پیش از نصب مخزن را دستی clone کنید؛ اسکریپت پیش‌نیازها را نصب می‌کند، سورس را دریافت و build می‌کند، سپس سرویس Donatello را راه می‌اندازد.

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/parsasajjadi2008/donatello/main/install.sh \
  -o donatello.sh
sudo bash donatello.sh install
```

اگر لینک دانلود بالا را مستقیم باز کردید، فایل `install.sh` دریافت‌شده را این‌طور اجرا کنید: `sudo bash /path/to/install.sh install`.

بعد از نصب، برنامه به‌صورت خودکار با systemd فعال می‌شود و با روشن‌شدن سرور دوباره اجرا خواهد شد.

```bash
sudo donatello status
sudo donatello manage
```

پنل به‌صورت پیش‌فرض فقط روی `127.0.0.1:8282` گوش می‌دهد. از سرور به رایانهٔ خود SSH tunnel بسازید:

```bash
ssh -L 8282:127.0.0.1:8282 user@SERVER_IP
```

سپس [http://127.0.0.1:8282](http://127.0.0.1:8282) را باز کنید و حساب مدیر را بسازید.

> نصب خودکار فعلاً برای Debian و Ubuntu با systemd است و برای build به اتصال اینترنت نیاز دارد.

## دستورات Donatello

تمام عملیات روزمره از دستور کوتاه `donatello` انجام می‌شود:

| دستور | کار |
| --- | --- |
| `sudo donatello manage` | بازکردن منوی مدیریت تعاملی |
| `sudo donatello status` | دیدن وضعیت سرویس |
| `sudo donatello start` | شروع سرویس |
| `sudo donatello stop` | توقف سرویس |
| `sudo donatello restart` | راه‌اندازی دوباره |
| `sudo donatello logs` | دیدن ۱۰۰ خط آخر گزارش |
| `sudo donatello logs 300` | دیدن تعداد دلخواه خط از گزارش |
| `sudo donatello manage port` | تغییر پورت داخلی پنل |
| `sudo donatello update` | دریافت نسخهٔ جدید، build و به‌روزرسانی سرویس |
| `sudo donatello delete` | حذف سرویس و برنامه با حفظ داده‌ها |
| `sudo donatello delete --purge` | حذف کامل، شامل تنظیمات و داده‌ها |

حذف برای تأیید، عبارت خواسته‌شده را در ترمینال دریافت می‌کند. در حالت معمول پایگاه‌داده و تنظیمات حفظ می‌شوند؛ گزینهٔ `--purge` همهٔ آن‌ها را حذف می‌کند.

## اجرای سرویس

Donatello یک سرویس systemd به نام `donatello.service` نصب می‌کند. سرویس با کاربر سیستمی محدود اجرا می‌شود، پس از خطا دوباره بالا می‌آید و هنگام boot فعال است.

```bash
systemctl status donatello
journalctl -u donatello -f
```

فایل تنظیمات سرویس در `/etc/donatello/donatello.env` و داده‌های برنامه در `/var/lib/donatello` قرار دارند. پورت پیش‌فرض از طریق دستور `sudo donatello manage port` قابل تغییر است.

## قابلیت‌ها

| بخش | قابلیت |
| --- | --- |
| داشبورد | وضعیت سیستم، CPU، حافظه، دیسک، uptime، بار سیستم و منابع مصرفی خود پنل |
| شبکه | کارت‌های شبکه، نام، MAC، آدرس‌های IP، وضعیت، شمارندهٔ RX/TX و پورت‌های شنونده |
| کاربران | حساب مدیر، اپراتور و ناظر با امکان فعال‌سازی و غیرفعال‌سازی |
| امنیت | هش گذرواژه با Argon2، نشست امن، محدودیت تلاش ورود و گزارش عملیات |
| پنل | React و TypeScript، رابط فارسی RTL و تم بنفش واکنش‌گرا بر اساس الگوهای shadcn/ui |
| نصب | فایل Bash مستقل، دستور `donatello` و سرویس systemd |

## امنیت و دسترسی

- پنل به‌طور پیش‌فرض روی loopback است و پورت آن را مستقیماً روی اینترنت باز نکنید.
- برای دسترسی بیرونی از SSH tunnel یا reverse proxy با HTTPS استفاده کنید.
- UFW در نسخهٔ فعلی فقط خوانده می‌شود؛ عملیات تغییردهندهٔ فایروال هنوز پیاده‌سازی نشده است.
- ترمینال وب، تست سرعت، شمارش ترافیک به تفکیک پورت و صدور خودکار گواهی SSL در برنامهٔ فعلی آماده نیستند.

## توسعه

برای توسعهٔ محلی به Rust stable و Node.js 20 یا جدیدتر نیاز دارید:

```bash
git clone https://github.com/parsasajjadi2008/donatello.git
cd donatello
cd frontend && npm ci && npm run build
cd .. && cargo run
```

برای کار روی فرانت‌اند، در ترمینال جداگانه اجرا کنید:

```bash
cd frontend
npm run dev
```

فایل‌های تولیدشدهٔ `frontend/dist` داخل باینری Rust قرار می‌گیرند؛ بعد از هر تغییر فرانت‌اند، پیش از build بک‌اند `npm run build` را اجرا کنید.

### ساختار

```text
src/main.rs                 API، نشست‌ها و خواندن اطلاعات Linux
install.sh                  نصب، مدیریت systemd، به‌روزرسانی و حذف
frontend/src/App.tsx        پوسته، ورود و نصب اولیه
frontend/src/pages.tsx      داشبورد و صفحات مدیریتی
frontend/src/components/ui  کامپوننت‌های سفارشی shadcn/ui و Radix
frontend/dist               رابط ساخته‌شده و جاسازی‌شده در باینری
PLAN.md                     نقشهٔ ۴۰ قابلیتی پروژه
```

## نقشهٔ راه

وضعیت قابلیت‌ها و برنامهٔ توسعه در [PLAN.md](PLAN.md) آمده است.

---

<div align="center">

ساخته‌شده توسط **[𝓜𝓻.𝓒𝓸𝓻𝓽𝓮𝔁](https://t.me/mrc0rtex)**

</div>
