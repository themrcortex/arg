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

روی Debian یا Ubuntu دارای systemd، فایل را دریافت و اجرا کنید. نیازی نیست پیش از نصب مخزن را دستی clone کنید؛ اسکریپت پیش‌نیازها را نصب می‌کند، سورس را دریافت و build می‌کند، سرویس Donatello را راه می‌اندازد و IP عمومی سرور را برای تأیید نشان می‌دهد. در همان اجرای اول، IP تأییدشده یا دامنه و ایمیل را وارد کنید تا HTTPS خودکار فعال شود.

```bash
curl --proto '=https' --tlsv1.2 -fsSL \
  https://raw.githubusercontent.com/parsasajjadi2008/donatello/main/install.sh \
  -o donatello.sh
sudo bash donatello.sh install
```

برای بازکردن منوی کامل نصب به‌جای نصب مستقیم، آرگومان `install` را حذف کنید: `sudo bash donatello.sh`.

اگر لینک دانلود بالا را مستقیم باز کردید، فایل `install.sh` دریافت‌شده را این‌طور اجرا کنید: `sudo bash /path/to/install.sh`؛ منوی تعاملی باز می‌شود.

بعد از نصب، برنامه به‌صورت خودکار با systemd فعال می‌شود و با روشن‌شدن سرور دوباره اجرا خواهد شد.

```bash
sudo donatello status
sudo donatello
```

پنل HTTP روی `127.0.0.1:8282` باقی می‌ماند و Donatello با Rustls یک listener امن HTTPS هم اجرا می‌کند. در نصب، با Enter از IP عمومی تأییدشده برای گواهی استفاده کنید یا دامنهٔ متصل به سرور را وارد کنید. گواهی IP در Let's Encrypt کوتاه‌عمر (حدود ۶ روز) است؛ Certbot آن را خودکار و روزی دو بار بررسی و تمدید می‌کند. اگر پورت 80 را یک سرویس systemd گرفته باشد، نصب‌کننده آن سرویس را فقط برای صدور گواهی متوقف می‌کند و بلافاصله پس از صدور یا خطا دوباره راه می‌اندازد؛ همین توقف و راه‌اندازی برای تمدیدهای خودکار هم تنظیم می‌شود. پورت ورودی TCP 80 باید نزد ارائه‌دهندهٔ سرور باز باشد. اگر پورت 443 اشغال باشد، Donatello اولین پورت آزاد از 8443، 9443 یا 10443 را انتخاب می‌کند؛ آن پورت را هم در فایروال باز کنید. ساخت اولین حساب مدیر فقط از اتصال localhost پذیرفته می‌شود؛ یک‌بار SSH tunnel بسازید و از `http://127.0.0.1:8282` حساب را ایجاد کنید، سپس با HTTPS وارد شوید. تنظیم SSL را بعداً هم از منو یا با دستور زیر انجام دهید:

```bash
sudo donatello ssl
```

بدون دامنه، از رایانهٔ خود SSH tunnel بسازید. دستور زیر را روی رایانهٔ خود اجرا کنید، `<SSH_USER>` را با نام کاربری ورود به سرور و `SERVER_IP` را با IP تأییدشده عوض کنید:

```bash
ssh -N -L 8282:127.0.0.1:8282 <SSH_USER>@SERVER_IP
```

سپس [ورود به پنل Donatello](http://127.0.0.1:8282) را باز کنید و حساب مدیر را بسازید. برای بازکردن منوی مدیریت روی سرور، `sudo donatello` را اجرا کنید.

> نصب خودکار فعلاً برای Debian و Ubuntu با systemd است و برای build به اتصال اینترنت نیاز دارد.

## دستورات Donatello

تمام عملیات روزمره از دستور کوتاه `donatello` انجام می‌شود:

منوی ترمینال وضعیت سرویس و آدرس پنل را بالای صفحه نشان می‌دهد. با کلیدهای بالا و پایین حرکت کنید، Enter را برای انتخاب بزنید، یا از میانبرهای عددی استفاده کنید؛ در ترمینال‌های ساده هم منوی متنی در دسترس است.

| دستور | کار |
| --- | --- |
| `sudo donatello` | بازکردن منوی کامل مدیریت |
| `sudo donatello manage` | بازکردن همان منوی مدیریت تعاملی |
| `sudo donatello status` | دیدن وضعیت سرویس |
| `sudo donatello start` | شروع سرویس |
| `sudo donatello stop` | توقف سرویس |
| `sudo donatello restart` | راه‌اندازی دوباره |
| `sudo donatello logs` | دیدن ۱۰۰ خط آخر گزارش |
| `sudo donatello logs 300` | دیدن تعداد دلخواه خط از گزارش |
| `sudo donatello manage port` | تغییر پورت داخلی پنل |
| `sudo donatello ssl` | دریافت گواهی Let's Encrypt و فعال‌کردن HTTPS |
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
- دسترسی بیرونی با HTTPS و Rustls انجام می‌شود؛ پنل HTTP در loopback باقی می‌ماند و ساخت اولین حساب از localhost محدود شده است.
- UFW در نسخهٔ فعلی فقط خوانده می‌شود؛ عملیات تغییردهندهٔ فایروال هنوز پیاده‌سازی نشده است.
- ترمینال وب، تست سرعت و شمارش ترافیک به تفکیک پورت در برنامهٔ فعلی آماده نیستند.

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
