## شاخه‌ها

- **`main`:** پایدار و پیش‌فرض.
- **`development`:** آزمایشی؛ ممکن است ناپایدار یا دارای تغییرات منتشرنشده باشد. روی VPS حساس، بدون snapshot و مسیر SSH جایگزین نصب نکن.

انتخاب شاخه تعیین می‌کند Update از کدام branch GitHub fetch شود و version/API کدام را build کند.

## حالت Cache

- **Fast:** `target` و Cache کامپایل Rust را نگه می‌دارد. Updateهای بعدی عموماً سریع‌تر می‌شوند، اما فضای دیسک مصرف می‌ماند.
- **Complete:** پیش از Build Cache قبلی را پاک می‌کند و پس از موفقیت/پایان Build Cache را دوباره پاک می‌کند. در لحظهٔ compile فضای موقت لازم است؛ این روش بدون دیسک موقت build نیست، اما مصرف پایدار Cache را پایین می‌آورد و زمان بیشتری می‌گیرد.

حجم Cache به Rust target، معماری و dependencyها وابسته است؛ مقدار ثابت برای همهٔ سرورها وجود ندارد. حالت complete، source و باینری نصب‌شده یا rollback را پاک نمی‌کند.

## SSH

از گزینهٔ Update در منو شاخه و حالت را انتخاب کن. دستور غیرتعاملی:

```bash
sudo arg update main complete
sudo arg update development fast
```

ترتیب ورودی‌ها branch سپس mode است. مقادیر مجاز فقط `main|development` و `fast|complete` هستند. اگر فقط `sudo arg update` اجرا شود و TTY داشته باشی، انتخاب‌ها پرسیده می‌شوند.

## بروزرسانی از پنل

دیالوگ Update وضعیت نسخه و commit، شاخه، mode، مرحلهٔ کار و log قابل مشاهده را نشان می‌دهد. یک Update معتبر در لحظه روی سرور قفل می‌گیرد تا عملیات متناقض از کاربر دیگر هم‌زمان اجرا نشود. دیگر نشست‌ها هم وضعیت جاری را می‌بینند.

در حین Update، سرویس وب ممکن است موقتاً restart شود. پس از Build، نصب باینری، restart و health check انجام می‌شود. Overlay جلوی کلیک عملیات متناقض را می‌گیرد. در پایان UI تلاش می‌کند cache مرورگر را دور بزند و نسخهٔ تازه را reload کند.

## لغو و Rollback

`cancel-update` یا دکمهٔ لغو درخواست توقف Build/فرایند امن را می‌دهد. لغو ممکن است زمان ببرد تا مرحلهٔ امن برسد. اگر Binary جدید نصب شده و health check شکست بخورد، نصب‌کننده نسخهٔ قبلی و snapshot دیتابیس را بازمی‌گرداند. اگر سرویس قدیمی به‌علت خرابی محیط بالا نیاید، از SSH log و Backup مستقل استفاده کن.

Rollback بازگرداندن دیتابیس هم دارد؛ تغییرات دیتابیس ایجادشده بعد از snapshot قبلی ممکن است از بین برود. پیش از Update مهم، backup مستقل و کنسول VPS داشته باش.

## عیب‌یابی Update

```bash
sudo arg status
sudo arg logs 200
sudo journalctl -u arg -n 200 --no-pager
sudo arg cancel-update
sudo arg rollback
```

اگر قفل stale ماند، ابتدا process و systemd job واقعی را بررسی کن. هرگز فایل `/run/arg-update*` را هنگام Build واقعی دستی پاک نکن. خطای شبکهٔ GitHub، space exhaustion، Rust toolchain یا `cargo` را از همان log تفکیک کن.
