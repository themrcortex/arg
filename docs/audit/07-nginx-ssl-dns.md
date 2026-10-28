# 07 — Nginx, SSL, domains and DNS

## Scope and revision

- Baseline: `development` at `eec273cde037fa57edc1b5ab042fda7acc26a32c`, reviewed on the isolated `audit/development-hardening` branch.
- Phase commits: `f423309` (Cloudflare record pagination and CAA presentation escaping), `0fdc55d` (stale DNS page request handling), `40ab081` (Nginx domain-conflict checks before SSL issuance), `df57120` (same-named active `conf.d` fragment coverage).
- Reviewed `src/nginx.rs`, `src/nginx_domains.rs`, `src/dns.rs`, `src/dns_vault.rs`, the related routes in `src/routes.rs`, and `frontend/src/DnsPage.tsx`.
- No production Nginx configuration, certificate, Cloudflare zone, DNS token, or user data was accessed. The tests use in-memory inputs and temporary local files.

## Route and authorization map

| Surface | Read access | Write access | Helper/provider boundary |
|---|---|---|---|
| Nginx sites, site detail, status, domain lookup, site logs/traffic/runtime/repository, maintenance status, firewall setting | Admin session | — | Read-only helper queries where needed; domain lookup no longer relies on a browser `Origin` header |
| Nginx site creation/edit/delete/enable, metadata, quick actions, runtime limits, maintenance, SSL, firewall setting | — | Admin session plus Origin check on mutations | Named Nginx-manager actions; no user-supplied shell string |
| DNS connections and records | Admin session | — | Cloudflare requests go only to the fixed HTTPS API host, use a short timeout, and reject redirects |
| DNS connection create/disconnect and record create/update/delete | — | Admin session, Origin check, and explicit confirmation for destructive record/connection actions | Scoped Cloudflare token is stored in `dns_vault`; API responses do not return it |

## Controls verified

- Nginx site names and domain inputs are validated before they are used in manager paths or Certbot arguments. Site SSL rejects wildcard requests, requires each requested name to be present in that site's `server_name`, and keeps the ARG panel certificate path separate.
- Certbot is invoked using `Command` arguments (not a shell string) through a 600-second timeout. The site file is backed up; `nginx -t` and reload are checked, and the selected file is restored on the represented failure paths.
- Nginx configuration writes use a temporary file and rename, then validate and reload before reporting success. Site enable/disable rolls back its symlink when validation or reload fails.
- Cloudflare DNS names are scoped to the selected zone. Record IDs and supported record kinds are validated, write operations require Origin and confirmation, and credentials are encrypted at rest with AES-256-GCM and reference-bound AAD.
- `dns_vault` restricts the vault directory/key/credential to owner-only permissions, checks file type/owner/size, and opens key/credential files with `O_NOFOLLOW`.
- DNS pagination is now explicit in both API and UI: 100 records per page, bounded to page 10,000, and stale in-flight frontend requests cannot overwrite a newly selected zone/page.
- CAA values are escaped for the quoted presentation string while the structured Cloudflare `data.value` remains unchanged.

## Findings

### DNS-001 — Cloudflare records beyond the first page were unreachable

- **ID:** DNS-001
- **Title:** DNS record listing always requested page 1 and the frontend discarded the available pagination metadata.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the baseline handler hard-coded `per_page=100&page=1`, and the frontend rendered only the returned records.
- **Module:** Cloudflare DNS record listing
- **File:** `src/dns.rs`, `frontend/src/DnsPage.tsx`
- **Function:** `list_records`, `loadRecords`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `f423309`
- **Affected behavior:** Zones with more than 100 records showed only the first 100. Although the backend returned `total_pages`, the browser had no way to request later pages.
- **Technical evidence:** At the baseline, `list_records` formatted `?per_page=100&page=1` unconditionally and returned `page: 1`; `DnsPage` made the records request without a page and ignored `pagination`.
- **Safe reproduction:** The old URL construction is visible in the baseline handler. `bounds_dns_record_pages` covers missing, zero, normal, and excessive page inputs; `cloudflare_record_url_uses_the_requested_bounded_page` asserts both `per_page=100` and the requested page. No Cloudflare request is made in tests.
- **Root cause:** Pagination was included in the response shape but not connected to the request contract or UI state.
- **Security/functional impact:** An administrator could not inspect or operate on records after the first 100; this caused incomplete DNS inventory and management.
- **Proposed fix:** Accept a bounded page query, pass it to Cloudflare, return the effective page metadata, and add accessible previous/next controls with stale-response protection.
- **Implemented change:** Added a default page of 1 and a 10,000-page ceiling, built the provider URL through URL query pairs, returned the effective page and bounded page count, and added Persian page controls. Selecting another zone resets the page; late requests from prior selections are ignored.
- **Tests added:** `bounds_dns_record_pages`; `cloudflare_record_url_uses_the_requested_bounded_page`; TypeScript check and production build.
- **Regression risks:** Cloudflare may lower its page ceilings or impose account-specific limits; provider errors remain surfaced to the admin. The 10,000-page ceiling is an application guard, not a promise that such a zone can be listed in one interactive session.
- **Commit:** `f423309 fix(dns): paginate records and escape CAA values`; `0fdc55d fix(dns): discard stale record page responses`.
- **Remaining risk:** No live Cloudflare zone with more than 100 records was available for integration verification.

### DNS-002 — CAA presentation content did not escape quotes or backslashes

- **ID:** DNS-002
- **Title:** CAA `content` was interpolated into a quoted presentation value without escaping.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the baseline string interpolation left user-provided `value` unescaped even though it is placed inside double quotes.
- **Module:** DNS record validation and request payload creation
- **File:** `src/dns.rs`
- **Function:** `record_payload`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `f423309`
- **Affected behavior:** A valid CAA value containing `"` or `\` could produce malformed or semantically different `content` than the structured `data.value` sent to Cloudflare.
- **Technical evidence:** Baseline lines in the CAA branch formatted `"{value}"` directly while separately sending the raw value in the structured data object.
- **Safe reproduction:** `escapes_caa_presentation_string_without_changing_structured_value` supplies both a backslash and quote and checks the escaped presentation form plus unchanged structured value. It is an offline payload test.
- **Root cause:** Structured CAA fields and the textual DNS presentation representation were built from the same raw value without presentation-layer escaping.
- **Security/functional impact:** Cloudflare could reject the record or parse a different CAA value than the administrator intended.
- **Proposed fix:** Escape presentation-string metacharacters while retaining the original structured field.
- **Implemented change:** Added an escape helper for `\` and `"` when constructing CAA `content`; `data.value` remains the original string.
- **Tests added:** `escapes_caa_presentation_string_without_changing_structured_value`.
- **Regression risks:** Cloudflare's live normalization for unusual escaped CAA values was not tested against a zone. Common issuer and iodef values are unaffected.
- **Commit:** `f423309 fix(dns): paginate records and escape CAA values`.
- **Remaining risk:** Verify provider normalization with a disposable Cloudflare zone before marketing arbitrary quoted CAA values as supported.

### NGINX-001 — Domain conflict detection skipped included symlinks and silently ignored oversized configurations

- **ID:** NGINX-001
- **Title:** The domain ownership scan could miss a conflicting server name in a symlinked or oversized Nginx fragment.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; the baseline scan accepted only regular directory entries and continued past files larger than 512 KiB.
- **Module:** Nginx domain ownership validation
- **File:** `src/nginx.rs`
- **Function:** `nginx_domain_is_claimed`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `40ab081`
- **Affected behavior:** Node/static site creation could report a domain as available even though an Nginx-included symlink fragment claimed it. A large config was silently treated as if it did not contain a conflict.
- **Technical evidence:** Baseline code skipped entries unless `file_type().is_file()` and used `continue` when a file exceeded 512 KiB. Symlinked `.conf` fragments are valid Nginx include targets.
- **Safe reproduction:** `domain_conflict_check_includes_symlinked_nginx_fragments` uses a temporary symlink to a fixture server block. `oversized_nginx_config_fails_closed_during_domain_conflict_check` uses a temporary file larger than the scan cap and asserts a visible error.
- **Root cause:** The scan treated unsupported or unbounded-to-parse inputs as non-conflicts instead of representing the result as unknown.
- **Security/functional impact:** A domain could be assigned twice, producing ambiguous Nginx routing and certificate/site changes.
- **Proposed fix:** Follow directory symlinks only when their target is a regular file, and fail closed with an actionable error when a candidate config exceeds the scan bound.
- **Implemented change:** The ownership scan includes symlinked regular fragments, ignores the selected site's same-named file only inside `sites-available`, still scans a same-named `conf.d` file, and returns an error instead of silently skipping oversized candidate files.
- **Tests added:** `domain_conflict_check_includes_symlinked_nginx_fragments`; `domain_conflict_check_ignores_own_site_only_in_sites_available`; `oversized_nginx_config_fails_closed_during_domain_conflict_check`.
- **Regression risks:** A single large unrelated config now prevents automated conflict validation and requires the administrator to inspect/split it. This is deliberate fail-closed behavior.
- **Commit:** `40ab081 fix(nginx): reject ambiguous SSL domain ownership`; `df57120 fix(nginx): scan same-named conf fragments`.
- **Remaining risk:** The scanner reads `sites-available` and `/etc/nginx/conf.d`; nonstandard include directories configured outside those paths are not discovered by this application-level scan.

### NGINX-002 — SSL issuance did not reject a domain already claimed by another site

- **ID:** NGINX-002
- **Title:** The SSL preflight checked that a domain appeared in the selected site but did not check for another server block claiming it.
- **Severity:** P2 / Medium
- **Status:** LIKELY
- **Confidence:** Medium; the code-level omission is confirmed, but the exact Certbot Nginx-installer choice in a duplicate-vhost deployment was not reproduced on a real Nginx host.
- **Module:** Site certificate issuance
- **File:** `src/nginx.rs`
- **Function:** `nginx_issue_ssl`
- **Commit SHA:** Baseline `eec273cde037fa57edc1b5ab042fda7acc26a32c`; fix `40ab081`
- **Affected behavior:** Certbot's Nginx plugin could encounter multiple server blocks for a requested domain, making the target configuration ambiguous.
- **Technical evidence:** Baseline `nginx_issue_ssl` verified each requested name against `configured_domains` parsed only from the selected file and proceeded to `certbot --nginx`; it did not call the existing cross-file domain checker.
- **Safe reproduction:** `ssl_domain_conflict_detection_covers_literal_and_wildcard_server_names` checks exact and wildcard collisions using an offline config fixture. The live Certbot selection is not exercised.
- **Root cause:** SSL preflight validated local ownership but omitted the cross-site conflict check used by generated-site creation.
- **Security/functional impact:** A certificate operation could edit an unintended or ambiguous vhost and alter traffic routing.
- **Proposed fix:** Reject any requested domain that overlaps a literal or wildcard name in another scanned site/config before invoking Certbot.
- **Implemented change:** The SSL helper now calls the domain ownership scan for each requested name and refuses the operation with a Persian conflict message before creating a backup or spawning Certbot.
- **Tests added:** `ssl_domain_conflict_detection_covers_literal_and_wildcard_server_names`, plus the symlink and oversize scanner regressions.
- **Regression risks:** An intentionally shared domain across vhosts is now blocked until the operator resolves the overlap. This is safer than allowing the Certbot plugin to choose an ambiguous target.
- **Commit:** `40ab081 fix(nginx): reject ambiguous SSL domain ownership`; `df57120 fix(nginx): scan same-named conf fragments`.
- **Remaining risk:** Validate exact duplicate-vhost Certbot behavior and rollback on disposable Debian/Ubuntu hosts; no live certificate operation was performed.

## Verification

- `cargo test --locked --all-targets`: **242 passed, 0 failed**.
- `npm run check`: **passed**.
- `npm run build`: **passed**. Vite still reports the pre-existing main JS chunk is slightly over 500 kB; this phase did not change application-wide chunking.
- `npm run test:offline`: **2 passed, 0 failed**.
- `git diff --check`: **passed**.
- No database migration was needed. The generated `frontend/dist` output was restored after build verification; this phase changes the frontend source and the application embeds the frontend during its normal build.
- `cargo fmt --all -- --check` is not claimed as passed: the baseline has existing formatting drift across unrelated files. No broad formatting rewrite was included.

## Review limits and follow-up

- No live Cloudflare token/provider call, DNS propagation, Certbot issuance, certificate renewal, external HTTP-01 challenge, or Nginx service reload was run.
- The application-level conflict scan covers the standard sites-available and `conf.d` directories only. A complete effective-config inventory would require parsing `nginx -T` output or a supported Nginx config parser and should be designed with nonstandard include paths in mind.
- Phase 6's open privileged filesystem path race (`FILE-002`) also applies to path-based Nginx config mutation and must be considered in the dedicated helper/TOCTOU remediation; this phase does not claim that race is fixed.
