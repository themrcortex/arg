# نقشهٔ راه پلتفرم ARG — فازهای ۱۸ تا ۴۸

این برنامه ادامهٔ فازهای ۱۳ تا ۱۷ است. فاز ۱۸ یک checkpoint سخت‌سازی و کیفیت است؛ پایان برنامه فاز ۴۸ است. «اختیاری» بودن فازهای ۳۹، ۴۰ و ۴۷ فقط یعنی پیش‌فرض نصب/فعال نیستند؛ هر قابلیت انتخاب‌شده باید معیار پذیرش خودش را بگذراند.

این نسخه متن طراحی فازهای ۱۹ تا ۴۸ را از master plan اصلی نگه می‌دارد. فاز ۱۸ و وضعیت فعلی فازهای ۱۳ تا ۱۷ در [نقشهٔ راه قبلی](app-platform-roadmap-13-18.md) و [جدول وضعیت معماری](app-platform-architecture.md) است. وجود این طراحی به معنی پیاده‌سازی یا قبولی تست نیست.

## دفتر اجرای فازها — ۱۴۰۵/۰۸/۰۵

**نتیجهٔ تست مخزن فازهای ۱۸ تا ۲۲:** ۱۴۱ تست Rust، بررسی TypeScript و build فرانت‌اند، build بهینهٔ Rust و بررسی/build مستندات دوزبانه موفق شدند (۱۹۲ صفحه، ۳۸۴ مقالهٔ محلی). Credentialهای helper مربوط به S3 با AES-256-GCM در `/etc/arg/s3` رمز می‌شوند و کلید محلی فقط برای root قابل دسترسی است؛ JSON قدیمی در نخستین خواندن موفق helper مهاجرت می‌شود. مدیریت عمومی اتصال‌ها، بازیابی/چرخش کلید و scope استفادهٔ مجدد هنوز پیاده نشده‌اند. متادیتا، API فضای‌کار/پروژه/محیط، مهاجرت پیش‌فرض، انتساب اپ و انتخابگرها حاضرند؛ جداسازی runtime/config/secret برای هر محیط و قفل ارتقای Production هنوز باز است. Binding موجود S3 همچنان metadata است و ACL Provider را اعمال نمی‌کند. قالب Laravel، React/Vite و Node فقط پیش‌نویس اپ می‌سازند؛ سورس نمی‌گیرند، نرم‌افزار نصب نمی‌کنند و Deploy نمی‌کنند. تست میزبانی یک/دو/سه VPS در این محیط موجود نیست و Pending می‌ماند.

این‌ها تغییرات محدود مخزن‌اند و به معنی قبولی کامل معیارهای فازهای ۱۸ تا ۲۲ نیستند. تست میزبان جداگانه گزارش خواهد شد.

**بستهٔ جاری: فازهای ۲۳ تا ۲۷، پیاده‌سازی جزئی.** ۱۵۵ تست Rust، `cargo check`، build بهینهٔ Release، بررسی و build فرانت‌اند و بررسی/build مستندات دوزبانه موفق‌اند (۱۹۲ صفحه، ۳۸۴ مقالهٔ محلی). فاز ۲۳ فقط release فعال را تحلیل و Build Plan بدون فرمان اجرایی ذخیره می‌کند. فاز ۲۴ مرور فقط‌خواندنی فایل‌های متنی همان release را با محدودیت مسیر اضافه می‌کند؛ این اتصال Git خصوصی یا ویرایشگر نیست. فاز ۲۵ Snapshot hashدار از متادیتای اپ/محیط بدون مقدار Secret می‌سازد؛ restore، چرخه Preview و promotion هنوز پیاده نشده‌اند. فاز ۲۶ کانتینر Docker موجود را فقط برای نمایش وضعیت به اپ لینک می‌کند و مالکیت آن را تغییر نمی‌دهد. فاز ۲۷ بررسی Zone و CRUD محافظت‌شدهٔ DNS در Cloudflare را با Credential رمز‌شده روی دیسک اضافه می‌کند؛ فعلاً فقط Cloudflare وصل است و propagation/DNS-01 وجود ندارد. تست‌های Cloudflare تست اعتبارسنجی و unit هستند و به Provider زنده وصل نشده‌اند. پذیرش چند Provider و تست میزبان آزمایشی باقی است.

## Phase 19 — Connections Hub و Secret Vault مرکزی

**وابستگی:** 02، 03، 11. **اولویت:** P1.

**ساخت:**
- مدل `connection` با `provider`, `scope`, `server_uuid`, `workspace_id`, `last_verified`, `capabilities`؛ انواع GitHub/GitLab، S3-compatible، DNS، SMTP، cloud؛ credential از متادیتا جدا شود.
- Vault با رمزنگاری authenticated at rest، قابلیت rotation/revoke، masking، audit و مکانیزم امن تحویل به سرور مقصد؛ key material و recovery policy در ADR مشخص شود؛ نه گذاشتن secret در SQLite خام.
- فرم «Test Connection»، expiry/reachability، امکان انتخاب connection موجود در هر wizard و مدیریت سطح دسترسی به‌ازای اپ/سرور/Workspace.
- lifecycle حذف: اگر App وابسته است forbid/confirm detach؛ استفاده از credential در deploy log و terminal sessions کنترل شود.
- UI با `secret reference` به‌جای مقدار Secret؛ export/backup مخفی و recovery دقیق تعریف شود.

**تست:** encrypt/decrypt، wrong key، rotation، scoped access، no secret in HTTP/audit/logs، ارتباط remote offline و revocation.

**قبولی:** اتصال S3/Git یک‌بار ثبت و در چند اپ مجاز reuse شود؛ Secret بین workspaceها نشت نکند.

**Commit:** `feat(connections): introduce scoped connections and encrypted secret vault`



## Phase 20 — Workspace / Project / Environment و Lifecycle محیط‌ها

**وابستگی:** 03، 04، 19. **اولویت:** P1.

**ساخت:**
- سلسله‌مراتب `Workspace → Project → Environment → Application/Service` با Default Workspace برای تمام نصب‌های Single Server؛ migration بدون شکستن appهای موجود.
- محیط‌های `development`, `staging`, `production` با env/config/branch/domain/S3 bucket/database مستقل؛ shared runtime مجاز ولی isolation و secrets جدا.
- UI project overview، environment switch، service dependency graph، quota اولیه و filtered application list.
- Workflow promote: Git ref/تغییرات کد و در صورت انتخاب restore داده با backup، approval و sanitization staging؛ همگام‌سازی prod data با هشدار محرمانگی.
- lock production و confirmation برای عملیات خطرناک؛ role و audit per environment.

**تست:** migrate existing apps to default project، same-name environments، isolation across projects، prod locking، promote failure rollback.

**قبولی:** کاربر بتواند یک Laravel در staging و production با تنظیمات متفاوت deploy کند بدون تغییر مسیر اپ دیگر.

**Commit:** `feat(projects): add workspaces projects and protected environments`



## Phase 21 — Service Bindings و Component Attachment

**وابستگی:** 09، 11، 19، 20. **اولویت:** P1.

**ساخت:**
- مدل `service_binding` و resolver استاندارد برای `fromService` و `fromSecret` با dependency graph و scope checks.
- اتصال دیتابیس، Redis/cache، RabbitMQ/queue، S3/object-storage، SMTP و telemetry به هر app؛ انتخاب «سرویس موجود» یا «ایجاد سرویس» با preflight.
- Env mappings مختص runtime: Laravel (`DB_*`, `REDIS_*`, `FILESYSTEM_DISK`, `AWS_*`, `QUEUE_CONNECTION`) و Node generic (`DATABASE_URL`, `REDIS_URL`, `S3_*`) با preview ماسک‌شده.
- provisioning idempotent کاربران دیتابیس، ACL و tokenهای محدود؛ network/volume و credential rotation وابسته.
- Dependency health و «detach impact preview»؛ قطع اتصال یک سرویس، appهای وابسته را بدون هشدار نشکند.

**تست:** allow/deny cross-project binding، rotation، broken dependency، binding to remote app، secret masking، incompatible runtime.

**قبولی:** اتصال S3 یا Redis با یک عملیات قابل بازگشت، بدون paste دستی credential در چند صفحه.

**Commit:** `feat(bindings): add service attachments and secret references`



## Phase 22 — App Templates و کاتالوگ نصب یک‌کلیکی

**وابستگی:** 04، 20، 21. **اولویت:** P1.

**ساخت:**
- schema declarative نسخه‌دار YAML/JSON برای metadata، runtime، repository/image، commands، healthcheck، domains، networks، volumes، services، env و generated secrets.
- validator با schema test، version compatibility و locked digest؛ magic variables ایمن (`RANDOM_SECRET`, `APP_HOST`, `ALLOCATED_PORT`) با تولید یک‌بار و پایداری هنگام upgrade.
- کاتالوگ اولیه محدود اما واقعی: Laravel skeleton، React/Vite static، Next.js، Nuxt، Node API، Redis، PostgreSQL و WordPress پس از driver مربوط.
- Template gallery با Search/category/details/prerequisites/resources/license/permission/preview و نصب async در Job Engine.
- امکان `custom local template` و export/import بدون credential؛ audit و disable template ناسازگار.

**تست:** schema invalid، unknown token، duplicate port، zip-slip prevention، secrets leakage، replay install، offline catalog.

**قبولی:** حداقل ۳ template از ابتدا تا دامنه/health روی VM آزمایشی نصب و از طریق UI قابل پاک‌سازی باشند.

**Commit:** `feat(templates): add validated one-click application catalog`



## Phase 23 — Buildpacks و تشخیص اتوماتیک پروژه

**وابستگی:** 04، 05، 07، 08، 22. **اولویت:** P1.

**ساخت:**
- preflight source detection: `composer.json`, `artisan`, `package.json`, lockfiles, `vite.config`, `next.config`, `nuxt.config`, `Dockerfile`, `compose.yaml`, `requirements.txt` و PHP ساده.
- نتیجه `BuildPlan` شامل detector/confidence، commands، artifacts، runtime version، env needs، port، working directory، healthchecks و warnings؛ override دستی همیشه در دسترس.
- generate preview برای Dockerfile/Compose یا Host runner plan (بنابر mode)، ذخیره plan immutably کنار revision و diff با نسخه قبل.
- اجرای build به‌صورت محدود: محدودیت CPU/RAM/disk/time، شبکه، uid غیر-root، package install policy، عدم دسترسی به credential پنل و منع bind نامحدود Docker socket.
- Monorepo support با subdirectory، lockfile-respecting install و cache های scoped به پروژه.

**تست:** ۱۲ fixture پروژه Laravel/React/Vue/Next/Nuxt/Express/TypeScript/Nest/Static/Python/Compose/monorepo؛ ambiguous detection، invalid Dockerfile.

**قبولی:** repo بدون Dockerfile با plan شفاف و امکان اصلاح پیش از اجرا deploy شود.

**Commit:** `feat(buildpacks): detect frameworks and generate auditable build plans`



## Phase 24 — Git Sources، Registry و مدیریت کد تکمیلی

**وابستگی:** 15، 17، 19. **اولویت:** P1.

**ساخت:**
- اتصال GitHub/GitLab و Git self-hosted با deploy key یا OAuth app، branch/tag/commit selector، webhook signed و anti-replay؛ repo provider disconnect.
- upload ZIP، مسیر existing server، repository monorepo و import existing Nginx app با dry-run و ownership boundary.
- Code tab: درخت فایل با search، editor، diff، Git status/history، Commit با confirmation و safety limits؛ تغییر منبع production به‌شکل branch/release نه overwrite کورکورانه.
- Git source mirror/cache با quota، scrub credentials در log، خطای clone/upload/repository permission؛ SSH host-key pinning/known_hosts policy.
- Webhook race control و trigger policy (branch/environment approvals, disable auto-deploy).

**تست:** forged webhook، duplicate commit، wrong branch، private clone rejection، path traversal، symlink escape، non-UTF8/binary file.

**قبولی:** کد App از Git یا ZIP قابل مشاهده و تغییر مجاز است و redeploy با revision pin شده انجام می‌شود.

**Commit:** `feat(sources): integrate git connections and safe application code workflows`



## Phase 25 — Preview Environments و Configuration Snapshots

**وابستگی:** 17، 20، 23، 24. **اولویت:** P1.

**ساخت:**
- PR preview محدود به quota، TTL، branch و ephemeral domain، auto cleanup پس از close/expiry و confirmation روی داده حساس.
- snapshot تغییرناپذیر از config/env references/domains/runtime/network bindings بدون secret plaintext؛ امکان view diff، restore و redeploy.
- Promotion `dev → staging → production` با approval، immutable artifact، health gate و migration safety.
- ربط‌دادن هر preview به Commit SHA و deployment job؛ هشدار اینکه داده preview مستقل است نه کپی خودکار production.
- UI تاریخچه تغییرات، commit origin، compare، expiry countdown و delete protection برای production.

**تست:** abandoned preview cleanup، secret masking، env isolation، wrong environment scope، snapshot restore/redeploy، quota exceed.

**قبولی:** تغییر PR در preview قابل مشاهده است و با حذف preview، volume مشترک production دست‌نخورده می‌ماند.

**Commit:** `feat(environments): add ephemeral previews and immutable config snapshots`



## Phase 26 — Docker/Compose پیشرفته و App Orchestration

**وابستگی:** 09، 10، 13، 21. **اولویت:** P1.

**ساخت:**
- bind container/Compose project به app + service و target server؛ تشخیص containerهای unmanaged و import دستی بدون تصاحب ناخواسته.
- image tags/digest pinning، graceful recreate، restart policies، healthchecks و deploy `A/B` تنها برای workloads سازگار.
- deploy compare روی Compose، env masking، detection image updates و action approval؛ resource limits و auto-restart storm protection.
- autosleep و scale به‌عنوان feature اختیاری (disabled by default) با خطر latency/cost؛ auto-scaling فقط با metrics صحیح و capacity policy.
- event aggregation: `degraded \u003e restarting \u003e unhealthy \u003e starting \u003e healthy \u003e exited \u003e unknown`؛ stop/restart/terminal scope per app.

**تست:** Compose drift، image missing، interrupted pull، restart loop، port conflict، rollback to prior digest، Docker unavailable.

**قبولی:** یک app چندکانتینری وضعیت یکپارچه و گزارش دقیق هر سرویس دارد.

**Commit:** `feat(compose): reconcile managed application stacks and health`



## Phase 27 — DNS Providers و Zone Manager

**وابستگی:** 19، 28 (APIهای DNS می‌توانند مستقل آماده شوند؛ UI deploy ابتدا فاز 28 را به آن متصل کند). **اولویت:** P2.

**ساخت:**
- Provider adapter برای Cloudflare، Route53 و DigitalOcean؛ credential vault و zone ownership checks.
- لیست و CRUD رکوردهای A/AAAA/CNAME/MX/TXT/CAA/SRV با validation، TTL constraints و diff preview؛ mapping دامنه به App.
- عملیات create/update با idempotency و DNS propagation check چند Resolver؛ rollback record changes تا حد امکان.
- DNS-01 برای wildcard فقط با دامنه مجاز، token حداقلی و cleanup رکورد challenge.
- UX قبل از Write نشان دهد نام و zone واقعی و provider چیست؛ خطر حذف MX/TXT مربوط به ایمیل هشدار صریح.

**تست:** fake provider، API timeout/rate-limit، wrong zone، concurrent edits، secret scope، DNS propagation pending.

**قبولی:** دامنه اپ از DNS provider مجاز به target سرور وصل شود و status شفاف داشته باشد.

**Commit:** `feat(dns): manage provider zones with guarded record changes`



## Phase 28 — Domain/SSL و Nginx Performance پیشرفته

**وابستگی:** 08، 13، 17. **اولویت:** P1.

**ساخت:**
- تکمیل managed vhost با owner/app/server شناسه‌دار، generated config diff، test-before-reload، rollback و عدم دستکاری configهای unmanaged.
- SSL صدور و renew، SAN و DNS-01 wildcard، custom cert validation/expiry alarms؛ enforce TLS secure defaults و HSTS با opt-in دقیق.
- تنظیم cache، gzip/brotli در صورت پشتیبانی واقعی، cache bypass rules، stale-if-error، per-app headers، asset TTL، proxy websocket/SSE/timeouts.
- log request-time/upstream-time/cache-hit و analytics per-app؛ جلوگیری از duplicate directive و safety against unsafe custom Nginx directives.
- per-app rate limit و maintenance mode، کانفیگ Cloudflare-aware با trusted proxy list صحیح.

**تست:** `nginx -t` on fixtures، invalid cert، wrong site ownership، renew failure، duplicate directives، websocket streaming، cache isolation.

**قبولی:** deploy دامنه را بدون قطع تصادفی سایر vhostها فعال می‌کند و خطای Nginx قابل rollback است.

**Commit:** `feat(domains): harden app nginx ssl and performance controls`



## Phase 29 — PostgreSQL، SQL Console و پایگاه‌داده‌های مدیریت‌شده

**وابستگی:** 19، 20، 21. **اولویت:** P2.

**ساخت:**
- حفظ MySQL/MariaDB موجود؛ افزودن PostgreSQL (host/managed container)، inventory، users/roles، bounded queries و connection pools اختیاری.
- DB creation wizard و binding به App با least-privilege؛ credential rotation و backup policy.
- SQL console با role `read-only` پیش‌فرض، query timeout، row/bytes limit، destructive query confirmation و audit (بدون لو رفتن داده حساس).
- database metrics (connections، slow queries/long query، cache hit در صورت قابل اندازه‌گیری) و graceful offline states.
- data migration/restore بین سرورها با transactional/engine-aware procedures؛ سازگاری نسخه DB و data integrity checks.

**تست:** SQL injection isolation، read-only deny، timeout، cross-project access، backup/restore، old MySQL regression.

**قبولی:** یک اپ می‌تواند در محیطی MySQL و در دیگری PostgreSQL را در قالب binding جدا استفاده کند.

**Commit:** `feat(databases): add postgresql and bounded app-scoped sql operations`



## Phase 30 — Backup Platform، Recycle Bin و Recovery Drill

**وابستگی:** 11، 16، 17، 19، 20. **اولویت:** P1.

**ساخت:**
- backup job لایه‌ای برای app config، code revision metadata، persistent volumes، DB dump، Compose manifests و encrypted secrets (با recovery key policy جداگانه).
- مقصد local/S3/B2-compatible، retention by count/age/size، immutable manifests، checksum، encryption option و offsite verification.
- Recycle Bin برای app/DB با soft-delete، retention و dependency-aware restore؛ purge جدا، غیرقابل برگشت و با confirmation چندمرحله‌ای.
- Restore Wizard: choose target server/env، check quotas/runtime/storage، preview conflict، dry-run، isolate and verify before activation.
- disaster recovery runbook؛ automatic scheduled **restore drill** روی staging/test target، نه production.

**تست:** corrupt archive، lost bucket، backup during deploy، low disk، restore to remote server، wrong user ACL، retention cleanup.

**قبولی:** بکاپ صرفاً «ساخته شد» نیست؛ یک restore واقعی صحت فایل و دیتابیس را اثبات کند.

**Commit:** `feat(recovery): implement verified backups recycle bin and restore drills`



## Phase 31 — Fleet Management، Enrollment و Safe Rollouts

**وابستگی:** 01، 02، 13، 17. **اولویت:** P1.

**ساخت:**
- guided join سرورها: preflight OS/capabilities، HTTPS fingerprint verification، short-lived pairing code و approval/revocation.
- Fleet inventory، groups/tags، online/offline/lag، heartbeat، agent version compatibility و pending command queue با TTL/idempotency.
- staged rollouts برای ARG nodes: canary → batch → remaining، health gate و auto rollback در حد امن؛ همزمانی و maintenance windows.
- desired-state templates و Drift Detection برای packages/runtime/nginx/firewall بدون اجرای خودکار اصلاحات خطرناک؛ remediation preview.
- Discovery opt-in محدود به LAN/known scopes، نه اسکن خودکار شبکه‌های غیرمجاز.

**تست:** stale agent، duplicate enrollment، reconnect/replay، offline queue expiration، partial rollout، rollback failure.

**قبولی:** ۳ سرور با نسخه‌های متفاوت وضعیت صادقانه دارند و update failed روی یکی کل ناوگان را خراب نمی‌کند.

**Commit:** `feat(fleet): add guided pairing fleet health and staged rollouts`



## Phase 32 — Private Networking و Remote Edge Access

**وابستگی:** 14، 21، 31. **اولویت:** P1.

**ساخت:**
- overlay بین سرورهای مجاز با WireGuard/tunnel اتوماتیکِ شفاف و opt-in؛ site-to-site peer metadata، MTU، key rotation و health.
- شبکه خصوصی بین app/services از policy `allow source → destination:port`، DNS/service discovery و reachability matrix؛ هر node مسیر متناسب خودش را می‌بیند.
- edge ingress برای اپ‌های روی سرور پشت NAT با محدودیت دامنه/پورت و authz؛ DNS/SSL از مراحل قبلی reuse شود.
- محافظت از route leaks، duplicate IP/CIDR، firewall conflict، split brain و DNS stale؛ عدم تبلیغ «shared POSIX disk».
- live topology graph: nodes، links، latency فقط اگر واقعاً اندازه‌گیری شده، allowed flows و error details.

**تست:** one peer offline، key rotation، cross-tenant deny، packet route conflict، tunnel reconnect، firewall rollback.

**قبولی:** API سرور A به DB سرور B فقط از مسیر private policy مجاز متصل می‌شود و سایر اپ‌ها دسترسی ندارند.

**Commit:** `feat(edge): add policy driven cross-server networking and ingress`



## Phase 33 — Metrics، APM-lite، Capacity و Log Analytics

**وابستگی:** 12، 13، 26، 28. **اولویت:** P1.

**ساخت:**
- جمع‌آوری metricهای CPU/RAM/disk/network، request rate، HTTP errors، p50/p95/p99، uptime، deployment correlation، DB telemetry و container restarts.
- retention چندسطحی: raw/minute/hour/day با سقف حجم و sample interval مشخص؛ query با server/app/env labels.
- performance panel: slow endpoints و upstream timing، cache-hit ratio، queue depth و health anomalies همراه توضیح مستند و خطایابی.
- fleet heatmaps، capacity estimator با confidence؛ اعداد تخمینی باید از داده واقعی تفکیک شوند.
- OpenTelemetry collector و Grafana/Prometheus/Loki به‌صورت اتصال اختیاری؛ ARG core حتی بدون آن‌ها قابل کار باشد.

**تست:** cardinality explosion، sparse data، missing metrics، retention downsampling، high-load dashboard، remote disconnect.

**قبولی:** پس از restart پنل metric history باقی می‌ماند و گزارش deployment با افزایش خطا مرتبط قابل تشخیص است.

**Commit:** `feat(observability): add app latency analytics and retention-aware metrics`



## Phase 34 — Notifications، Incidents و Public Status Pages

**وابستگی:** 12، 13، 19، 33. **اولویت:** P1.

**ساخت:**
- Notification bus پایدار با delivery retry و dedupe برای in-app، Telegram، Discord، Slack، email و webhook؛ per-user severity و quiet hours.
- alert rules از metrics/health/deploy/backup/cert expiry، با thresholds، cooldown، hysteresis، incident dedup و acknowledgement.
- HTTP/TCP/DNS checks، uptime history و incidents lifecycle؛ public status page قابل opt-in و بدون افشای hostname/internal topology/secrets.
- هشدارهای app error، container restarting، rollback failure، full disk و S3 inaccessible؛ delivery log با redaction.
- subscriptions per workspace/project/server و digest روزانه/هفتگی اختیاری.

**تست:** retry storm، duplicate alert، delivery provider failure، webhook SSRF guard، private resource leaked to status page.

**قبولی:** Alert واقعی در کانال انتخابی ارسال شود، ack/history داشته باشد و در حالت provider offline از بین نرود.

**Commit:** `feat(alerting): unify notifications incidents and public status`



## Phase 35 — Identity، Passkeys/2FA و RBAC عمیق

**وابستگی:** 02، 19، 20. **اولویت:** P1/P2.

**ساخت:**
- policy matrix granular برای server/app/terminal/deploy/secrets/S3/DNS/DB؛ roles `owner/admin/developer/operator/viewer` با least privilege.
- TOTP و recovery codes، WebAuthn/passkeys، session/device management، audit trails و revocation.
- SSO/OIDC و OAuth GitHub/Google اختیاری، SAML فقط در صورت نیاز تیمی؛ account-linking anti-takeover.
- step-up authentication برای destructive purge، root terminal، secret export، cross-server migration و production deploy.
- rate limits، CSRF/origin checks، short-lived API grants و immutable audit correlation id.

**تست:** role matrix به‌ازای هر API، federation authorization، session revocation، 2FA recovery، SSO callback forgery و scope escalation.

**قبولی:** viewer حتی با درخواست مستقیم HTTP نتواند terminal یا secret/deploy write اجرا کند.

**Commit:** `feat(auth): enforce granular access and strong authentication`



## Phase 36 — Host Security Toolkit، WAF و Supply-Chain Checks

**وابستگی:** 19، 26، 28، 35. **اولویت:** P2.

**ساخت:**
- WAF اختیاری per-domain با ModSecurity/OWASP CRS در صورت سازگاری distro و بسته‌ها، detect-before-block، rule audit و emergency rollback.
- Host scanners به‌شکل add-on: Fail2ban/Lynis/ClamAV، file-integrity checks و امنیت SSH؛ بدون ادعای پشتیبانی تمام OSها.
- container image SBOM/CVE scanning با syft/grype یا جایگزین قابل تست؛ نمایش severity، fix version، provenance و false positive review.
- محدودسازی trust build artifacts، image signature verification در صورت انتخاب، digest-pinned downloads و CVE report policy.
- UI centralized Security posture با remediation امن و الزامات دسترسی Root واضح.

**تست:** false positive WAF، rule rollback، scanner missing، malicious artifact fixture، no unapproved config mutation.

**قبولی:** ابزارهای امنیتی نصب نشده باعث شکست پنل نشوند؛ هر remediation کنترل‌شده و قابل audit باشد.

**Commit:** `feat(security): integrate optional waf scanning and host checks`



## Phase 37 — Extension SDK و Marketplace امن

**وابستگی:** 19، 22، 35، 46 (در زمان اجرا نسخه فعلی Update Engine را بررسی کن؛ اگر 46 هنوز اجرا نشده، صرفاً compatibility contract تعریف کن). **اولویت:** P3.

**ساخت:**
- `arg-extension.json` با schema_version، permissions/capabilities، entrypoints، frontend contributions، data migrations، required ARG versions، signed artifact digest.
- lifecycle `install/enable/disable/upgrade/uninstall` با backup/rollback و cleanup؛ deny-by-default API permissions و sandbox/isolated worker برای کد third-party.
- curated registry و offline import؛ verify SHA-256 + ed25519 publisher signature، trust policy و explicit opt-in برای unsigned artifacts.
- frontend contribution points محدود: navigation, tabs, widgets, runtime templates, jobs؛ بدون eval/remote JS نامحدود در سطح پنل.
- log/audit، owner provenance، dependency resolution و migration contracts؛ marketplace باید optional باقی بماند.

**تست:** tampered package، zip traversal، unsigned denial، incompatible version، interrupted upgrade، RBAC bypass، rollback.

**قبولی:** افزونه معتبر فعال/غیرفعال شود، بدون اینکه core ARG برای هر افزونه build مجدد اجباری بخواهد (یا محدودیت runtime loading صریحاً تعریف شده باشد).

**Commit:** `feat(extensions): add signed extension manifests and lifecycle`



## Phase 38 — Visual Automation و Infrastructure Designer

**وابستگی:** 04، 20، 21، 34، 35. **اولویت:** P2.

**ساخت:**
- automation engine مبتنی بر همان job state machine با triggerهای schedule/webhook/event/manual؛ versioned workflows و input schema.
- UI canvas برای app/DB/cache/network/S3/backup/deploy، validation گراف، cycle guard و inspect diff قبل Apply.
- workflow steps دارای retries/backoff/timeout/idempotency/compensation و human approval nodes؛ output secrets redacted.
- dry-run visual stack planner و quota/cost estimates با برچسب estimate؛ Deploy کردن plan روی سرورهای انتخاب‌شده در scope مجاز.
- history per-node، pause/resume، safe disable و failure recovery؛ triggerها پس از reboot از بین نروند.

**تست:** invalid workflow/cycle، duplicate webhook، concurrent runs، failure cleanup، remote offline، privilege escalation prevention.

**قبولی:** یک flow ساده deploy → health check → notify و backup scheduled از UI ساخته و با audit اجرا شود.

**Commit:** `feat(automation): add validated workflows and infrastructure canvas`



## Phase 39 — Cloud Provisioning و Cost Tracking (اختیاری)

**وابستگی:** 19، 31، 35. **اولویت:** P3.

**ساخت:**
- Provider abstraction برای Hetzner، DigitalOcean، Vultr و Linode؛ API tokens فقط در vault و read-only inventory در شروع.
- Provision Wizard با region/image/size/network/SSH keys/cost estimate، quota guard، explicit confirmation و resource ownership tags.
- بعد از creation، نصب ARG agent/pairing با تأیید fingerprint؛ جلوگیری از باقی‌ماندن ماشین orphan پس از failure.
- destroy/resize/snapshot عملیات مجزا با blast-radius preview و non-reversible warning؛ درخواست خرید/مصرف منابع باید واضح باشد.
- tracking هزینه با واحد پول/زمان منبع و برچسب estimate/provider-billed؛ عدم ادعای real-time billing بدون API معتبر.

**تست:** mocked provider APIs، rate-limit، partial creation cleanup، token revocation، mistaken resource ownership، cost discrepancy.

**قبولی:** راه‌اندازی یک VM آزمایشی در محیط تست provider با opt-in و ثبت کامل مالکیت resource انجام شود.

**Commit:** `feat(cloud): add optional provisioning and cost visibility`



## Phase 40 — Mail Server Suite به‌صورت Add-on (اختیاری)

**وابستگی:** 19، 27، 28، 35. **اولویت:** P3.

**ساخت:**
- add-on برای Postfix/Dovecot و mailboxes، TLS، SPF/DKIM/DMARC و DNS checks؛ Webmail فقط پس از بررسی پشتیبانی.
- domain ownership و per-mailbox storage/quota، anti-spam integration و delivery queues/log retention بدون محتوای نامه در لاگ مدیریتی.
- preflight پورت‌های SMTP/IMAP، rDNS/PTR و reputational limits؛ هشدار واضح درباره محدودیت ارائه‌دهنده VPS.
- backup/restore mailbox، secure credentials و rate-limited account management.
- امکان نصب/حذف اختیاری به‌طوری که کاربران فقط Web hosting هرگز سرویس Mail نداشته باشند.

**تست:** config validation، DNS mismatch، relay denial، permission boundaries، failed install rollback، TLS expiry.

**قبولی:** suite در محیط آزمایشی تنها با الزامات شبکه معتبر فعال شود و سایر Appها unaffected بمانند.

**Commit:** `feat(mail): add optional guarded mail-server management`



## Phase 41 — Python Runner و WordPress Installer

**وابستگی:** 06، 07، 20، 21، 22، 28، 29. **اولویت:** P2.

**ساخت:**
- Python runtime (venv، Python versions، requirements/uv/poetry detection) برای Flask/Django + Gunicorn/Uvicorn، worker service، collectstatic، health.
- WordPress template installer روی PHP-FPM + MySQL، admin secret generation، site/domain/SSL، scheduled backup و basic health.
- WordPress dev/staging/prod copy workflow با URL/domain rewrite، database backup، media sync و sanitization هشدار؛ production promotion approval.
- dependency isolation، lockfile/version pinning، unsupported plugin detection و import WordPress موجود بدون تصاحب کد کنترل‌نشده.
- runner validation با همان Job/Logs/Terminal/Volume/S3/Network primitives؛ از runtime مستقل موازی پرهیز.

**تست:** smoke Flask، Django static، WordPress install، failed migration، staging promote، rollback، invalid plugin/permission.

**قبولی:** تمام این اپ‌ها در همان Applications UI (نه یک پنل جدا) دیده و مدیریت شوند.

**Commit:** `feat(runtimes): add python apps and wordpress managed templates`



## Phase 42 — Dashboard Customization، Themes و Branding

**وابستگی:** 03، 13، 20. **اولویت:** P2.

**ساخت:**
- dashboard widgets قابل reorder/hide با server scope، dashboard presets (Minimal / DevOps / Hosting / Fleet) و saving per user.
- theme tokens، light/dark، color accent، logo/brand opt-in و RTL typography؛ بدون کپی مستقیم HTML/CSS ServerKit.
- widget SDK/read-only data permissions، accessibility، keyboard navigation و reduced-motion.
- lazy-loading charts و pause polling هنگام hidden tab؛ حفظ responsive در موبایل.
- قابلیت Reset defaults و مهاجرت preference با تغییر نسخه.

**تست:** widget scope leak، theme contrast، RTL mobile snapshots، hidden polling، missing widget version.

**قبولی:** تنظیمات dashboard برای هر کاربر مستقل و بدون اثر روی مدیریت سرور دیگر حفظ می‌شود.

**Commit:** `feat(ui): add customizable dashboards and accessible themes`



## Phase 43 — Mobile/PWA و Offline Read-only

**وابستگی:** 31، 33، 34، 42. **اولویت:** P2.

**ساخت:**
- PWA installable با cache فقط برای UI shell و read-only last-known metrics با watermark `STALE`؛ هیچ credential یا secret حساس در offline cache.
- mobile-first navigation، mini server switcher، alerts inbox، incident acknowledge با reconnect، safe quick actions.
- push notification اختیاری با permission صریح؛ session expiration و logout cache purge.
- offline mutation queue به‌صورت پیش‌فرض ممنوع برای restart/delete/deploy؛ actions باید دوباره روی وضعیت تازه تأیید شوند.

**تست:** lost connection during deploy، wrong cached server, expired session، cache leak، responsive iOS/Android.

**قبولی:** وضعیت قدیمی با وضعیت live اشتباه گرفته نشود؛ عملیات خطرناک offline اجرا نشوند.

**Commit:** `feat(pwa): add mobile-first read-only offline shell`



## Phase 44 — Teams، Quotas و Multi-tenancy کامل

**وابستگی:** 20، 31، 35. **اولویت:** P2.

**ساخت:**
- team invites، membership، roles per workspace/project/environment، server sharing با scoped permissions.
- quotas برای apps/servers/cpu/ram/storage/parallel deployments/API calls، reservation before deploy و policy enforcement مقصد.
- tenant-separated secrets، metrics، logs، jobs، files، S3 و alerts؛ local installation default owner backward-compatible.
- usage dashboard و optional billing-ready metering (بدون درگاه پرداخت مگر درخواست جداگانه)، shareable readonly links با expiry.
- cross-tenant admin view فقط برای super-admin و همه عملیات قابل audit باشند.

**تست:** multi-tenant isolation API/UI/WS، IDOR، quota races، invited user revocation، stale sessions on role change.

**قبولی:** دو تیم روی یک controller بدون دسترسی به داده/ترمینال/سرویس‌های یکدیگر کار کنند.

**Commit:** `feat(teams): isolate workspaces and enforce resource quotas`



## Phase 45 — Public API، API Keys، Webhooks و OpenAPI

**وابستگی:** 19، 34، 35، 44. **اولویت:** P2.

**ساخت:**
- API keys با scopeهای دقیق per workspace/app/server، expiry، key hashing، rotation، usage counter و rate limiting.
- OpenAPI schema نسخه‌دار و client error format مشترک شامل `correlation_id`، structured errors و pagination.
- outbound webhooks event subscriptions با signed payload، dedupe/retry/backoff/dead-letter، secret rotation و SSRF-protected destinations.
- CLI client افزوده بر مدیریت فعلی ARG برای list apps/deploy status/logs/rollback در scope مجاز.
- backwards-compatibility API و deprecation policy؛ آمار درخواست بدون ذخیره توکن خام.

**تست:** wrong scope، expired key، replay event، webhook timeout، OpenAPI schema contract و backward compat.

**قبولی:** یک CI خارجی بتواند deploy مشخص را امن trigger و از API status آن را پیگیری کند.

**Commit:** `feat(api): publish scoped tokens webhooks and openapi`



## Phase 46 — Installer/Updater، Profiles و ARG Doctor

**وابستگی:** 18، 31، 35. **اولویت:** P1.

**ساخت:**
- نصب با profiles واقعی و opt-in (Minimal بدون Docker، Standard با Docker optional، Full با security tools)؛ capability probe بعد از نصب و در هر reboot.
- نصب/repair/update از مسیر موجود ARG، با preflight space/service/permissions، backup DB، migration rollback notes، staged release، health gate و crash recovery.
- ARG Doctor در UI/CLI: تشخیص دامنه/SSL، Docker، S3 mount، node pairing، runtimes، permission، systemd، log disk pressure.
- compatibility matrix Ubuntu/Debian و architecture؛ OS unsupported نباید نصب موفق کاذب نمایش دهد.
- feature update log و changelog بعد از بارگذاری نسخه جدید، نه قبل از refresh؛ جلوگیری از پرش مراحل و race.

**تست:** upgrade from old ARG with apps/S3/federation، interrupted update، low-disk preflight، bad rollback، unavailable Docker.

**قبولی:** نصب موجود بعد از update تمام اپ‌ها/سرویس‌ها/secret refs را نگه دارد و Doctor خطاهای واقعی را نشان دهد.

**Commit:** `feat(maintenance): add install profiles and recovery-aware arg doctor`



## Phase 47 — AI Operations Assistant با تأیید انسانی (کاملاً اختیاری)

**وابستگی:** 35، 45، 46. **اولویت:** P3.

**ساخت:**
- دستیار read-only برای توضیح deploy failure، log error و metric anomaly با citation به log event/job/metrics واقعی؛ پاسخ حدسی را از fact جدا کند.
- provider adapter اختیاری BYOK و کنترل هزینه/ارسال داده؛ redaction secrets و امکان غیرفعال‌سازی کامل.
- هیچ اجرای Shell/Root/Firewall/DB mutation از خروجی مدل بدون preflight و approval صریح کاربر؛ ابزارهای `allowlisted` و مجوز server/app.
- history و tool actions در audit؛ prompt injection در logs/repo content باید به‌عنوان داده untrusted تلقی شود.
- graceful fallback when model API unavailable؛ هسته پنل بی‌نیاز از AI باقی بماند.

**تست:** prompt injection via log/repo، unauthorized tool calls، secrets redaction، provider timeout، misleading diagnosis.

**قبولی:** دستیار صرفاً با شواهد قابل بررسی پیشنهاد می‌دهد و بدون تأیید عملیاتی انجام نمی‌دهد.

**Commit:** `feat(ai): add opt-in evidence-linked read-only ops assistant`



## Phase 48 — ممیزی نهایی ServerKit-Parity، تست انتهابه‌انتها و Release

**وابستگی:** همه فازهای **انتخاب‌شده** برای release target، نه الزاماً P3های اختیاری. **اولویت:** Release Gate.

**ساخت:**
- Feature inventory براساس ماتریس 4A.3 با وضعیت `implemented/tested/experimental/not planned` و شواهد test/commit؛ هیچ UI fake button مجاز نیست.
- threat modeling کامل federation، vault، marketplace، WebSocket، terminal، uploads، Git webhook، DNS، app migrations و tenant isolation.
- load/soak testing شامل ده‌ها app، ۳ server، stream logs، offline agent، network partition و storage failure؛ resource budget اندازه‌گیری‌شده.
- E2E upgrade/rollback از نسخه قدیمی ARG، restore drill، quota/retention validation، API/backward compatibility، docs دو زبانه و release notes.
- security review مستقل برای privilege boundaries، leak prevention، dependency license compliance و artifact signatures؛ گزارش limitations واضح.

**تست:** test matrix کامل در بخش 4C؛ هیچ pass ساختگی. مشکلات blocker باید پیش از release حل یا feature flag خاموش شوند.

**قبولی:** ARG همچنان روی یک VPS سبک و مفید است و روی شبکه چندسروری هم App Platform قابل اطمینان دارد؛ همه ادعاها با شواهد سنجش‌پذیر پشتیبانی می‌شوند.

**Commit:** `chore(release): validate serverkit-inspired ARG platform parity`

---
