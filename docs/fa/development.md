## ساختار مخزن

```text
src/                  Rust + Axum، API و منطق سیستم
src/s3/               اتصال S3 و عملیات Bucket/Object/Mount
src/files.rs          عملیات فایل و ZIP
src/nginx/            مدیریت سایت و کانفیگ Nginx
frontend/src/         React + TypeScript + Tailwind + shadcn-style UI
frontend/dist/        خروجی تعبیه‌شده در باینری؛ در مخزن نگهداری می‌شود
docs/fa/, docs/en/    محتوای رسمی Markdown دوزبانه
docs-site/            وب‌سایت مستندات، Vite + React + Tailwind
install.sh            نصب‌کننده و منوی SSH
scripts/              تست‌ها و ابزار نسخه‌بندی/Update
```

## محیط محلی

نیازها: Rust stable، Node.js 22 (یا نسخهٔ سازگار package lock)، npm، و ابزارهای build سیستم SQLite/OpenSSL در صورت نیاز target.

```bash
npm ci --prefix frontend
npm run check --prefix frontend
npm run build --prefix frontend
cargo check --locked
cargo test --locked
cargo build --locked --release
bash -n install.sh scripts/update-helper.sh
```

`frontend/dist` جزو منبع deployment است؛ قبل از build باینری Rust باید با سورس React همان نسخه به‌روز شود.

### تست مستندات

```bash
npm ci --prefix docs-site
npm run check --prefix docs-site
npm run build --prefix docs-site
bash scripts/tests/test-docs-content.sh
```

## قواعد تغییر

- API جدید را در module مناسب اضافه کن، نه در فایل بزرگ route/صفحه اگر جداسازی طبیعی است.
- برای endpoint write، احراز Admin، بررسی Origin، اعتبارسنجی ورودی، محدودیت اندازه/زمان و Audit را در نظر بگیر.
- ورودی کاربر را به shell string الحاق نکن. از `Command::new` با آرگومان ثابت یا Helper پروتکل‌دار استفاده کن.
- اسرار را در DB خام، Frontend، query، process argv، لاگ یا Git ذخیره نکن.
- مهاجرت SQLite باید روی دیتابیس قدیمی هم idempotent و بدون حذف داده باشد.
- قابلیت تشخیص‌ناپذیر را unavailable اعلام کن؛ دادهٔ نمایشی را به جای دادهٔ واقعی نگذار.
- UI فارسی RTL، انگلیسی LTR، موبایل، keyboard، reduced motion و حالت loading/error/empty را تست کن.

## نسخه و انتشار

`main` از SemVer پایدار پیروی می‌کند؛ version در `Cargo.toml`, `frontend/package.json`, lockfile و UI باید همخوان باشد. برای توسعه فقط `development` را استفاده کن. Development باید `main` را در ancestry داشته باشد و نسخهٔ prerelease آن از patch بعدی باشد.

```bash
# روی main، پس از تست و changelog
scripts/bump-version.sh patch

# روی development، پس از merge کردن main
scripts/bump-development-version.sh

scripts/check-versioning.sh
```

هر تغییر عملکردی باید CHANGELOG مناسب داشته باشد. Release با tag GitHub فقط از main و طبق workflow مخزن انجام می‌شود. مستندات مرتبط را در همان PR/commit کد و در هر دو زبان به‌روز کن.

## مشارکت

Issue باید هدف، نسخه/commit، سیستم‌عامل و راه بازتولید داشته باشد. PR کوچک و متمرکز، تست مشخص و توضیح اثر امنیتی/داده‌ای داشته باشد. تغییر destructive بدون rollback یا preflight ارسال نکن. از ارائهٔ مخزن جدا برای مستندات خودداری شده تا docs و کد با هم version شوند.
