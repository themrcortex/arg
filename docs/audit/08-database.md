# 08 — SQLite, migrations and data consistency

## Scope and revision

- Reviewed the audit branch after Phase 7 at `ca1273c24eac200173943d8b73fb522422437bc4`; the requested `development` baseline remains `eec273cde037fa57edc1b5ab042fda7acc26a32c`.
- Reviewed connection setup and CLI database paths in `src/main.rs`; migrations and multi-row writes in the application, deployment, federation, monitoring, S3, DNS, maintenance and workspace modules; and the backup API.
- No production database was opened or modified. Reproduction used in-memory SQLite databases and an isolated Python SQLite script.
- Phase source commits: `f8539e39e00d752eeaba6ab7f0828094579b73dc`, `b31c9b7842c836e4beb0c42f2c94097b2c0ed961`, `6a98549`.

## Database architecture and observed controls

- ARG keeps one `rusqlite::Connection` in `Arc<Mutex<Connection>>`. Axum handlers access it synchronously while holding the mutex, so database work is serialized per panel process. This is simple and avoids concurrent use of one SQLite connection, but it can make a slow query delay other database requests. No representative load benchmark was available; performance impact is recorded for Phase 10 rather than treated as a measured defect here.
- Startup enables `PRAGMA foreign_keys=ON` and `PRAGMA journal_mode=WAL` on the main connection. SQLite foreign-key enforcement is connection-local. The root password-reset CLI opens a separate connection and now explicitly enables foreign keys before deleting sessions.
- The locked `rusqlite 0.32.1` implementation configures a 5-second SQLite busy timeout when opening a connection (`inner_connection.rs`); the main panel does not override it. The CLI also sets five seconds explicitly.
- Multi-row deployment enqueue/claim, application cleanup, workspace creation and fleet tag replacement use transactions. The SQLite backup path uses the Online Backup API; `updater_database_backup_preserves_a_consistent_sqlite_snapshot` covers a consistent offline snapshot.
- Migrations are distributed across module `migrate` functions. Most use `CREATE ... IF NOT EXISTS`, guarded additive `ALTER TABLE`, or idempotent backfills. There is no central schema-version table or transaction wrapping the full startup migration sequence. Partial-upgrade recovery is not fully covered by an end-to-end migration failure-injection test; see DB-003.
- SQL inputs inspected in this phase use bound parameters. The dynamic SQL identifiers in additive migrations come from internal fixed tuples, and the network chart chooses its grouping expression from a two-value allowlist.

## Findings

### DB-001 — Root password reset could report failure after changing account state

- **ID:** DB-001
- **Title:** The root CLI changed the password and revoked sessions in separate autocommit statements, then ignored Audit insertion failure.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; reproduced with a failing Audit trigger in isolated SQLite, then covered by transaction regression tests.
- **Module:** Authentication CLI and session cleanup
- **File:** `src/main.rs`
- **Function:** `change_password_cli`, `reset_account_password`
- **Commit SHA:** Pre-fix audit revision `ca1273c24eac200173943d8b73fb522422437bc4`
- **Affected behavior:** If the Audit insert failed, the CLI returned an error even though the password had already changed and sessions had already been deleted. Because this CLI connection did not enable foreign keys, session-dependent rows such as MySQL authorization grants could remain orphaned.
- **Technical evidence:** Before the fix, password update, session deletion and `audit()` were independent operations. `audit()` deliberately discarded its SQLite result. The CLI connection set a busy timeout but did not set `foreign_keys=ON`; the schema declares session grants with `ON DELETE CASCADE`.
- **Safe reproduction:** An in-memory database with a `BEFORE INSERT ON audit` trigger raising `ABORT` left the new password hash and zero sessions after the old sequence returned an error. With foreign keys disabled, the linked MySQL authorization row remained. No real account or database was used.
- **Root cause:** The CLI treated three logically coupled security changes as independent writes and relied on a best-effort Audit helper.
- **Security/functional impact:** A root operator could be told a reset failed after it had actually taken effect; dependent authorization rows were not reliably revoked by cascade.
- **Proposed fix:** Hash before opening the write transaction, enable foreign keys on this connection, and atomically update the password, revoke sessions and insert a mandatory Audit row.
- **Implemented change:** Added `reset_account_password`; it enables foreign keys and performs all three writes in one transaction. Any write failure rolls the entire reset back. The CLI maps a missing/disabled account to its existing safe error.
- **Tests added:** `root_password_reset_updates_password_revokes_session_grants_and_audits_atomically`; `root_password_reset_rolls_back_if_audit_insert_fails`.
- **Regression risks:** Audit storage is now required for this CLI operation. If the Audit table cannot accept a row, reset is refused without changing password or sessions.
- **Commit:** `f8539e39e00d752eeaba6ab7f0828094579b73dc` (`fix(db): make root password reset atomic`).
- **Remaining risk:** None identified for this path in isolated tests. Root password reset was not run against an installed server.

### DB-002 — SQLite row and query errors were silently converted to incomplete or zero-valued API data

- **ID:** DB-002
- **Title:** Several database-backed lists discarded row-decoding errors; other read paths returned empty/zero summaries when a query failed.
- **Severity:** P2 / Medium
- **Status:** CONFIRMED
- **Confidence:** High; source showed `filter_map(Result::ok)` and `unwrap_or_default`/zero fallbacks; malformed SQLite values reproduced the skipped-row case.
- **Module:** Database-backed API read models
- **File:** `src/app_workspaces.rs`, `src/app_networks.rs`, `src/app_storage.rs`, `src/app_s3.rs`, `src/app_monitoring.rs`, `src/extensions.rs`, `src/federation.rs`, `src/doctor.rs`, `src/main.rs`, `src/s3/mod.rs`
- **Function:** Workspace/project/environment, application network/member, volume, S3 binding/connection/bucket, monitoring, trust-key, Federation, doctor, session/history/traffic list and S3 summary readers
- **Commit SHA:** Pre-fix audit revision `ca1273c24eac200173943d8b73fb522422437bc4`
- **Affected behavior:** A malformed or schema-incompatible row could disappear from a successful response. Some S3 summary query failures could look like a healthy empty installation, and S3 connection read errors were reported as “not found.”
- **Technical evidence:** The pre-fix call sites used `.filter_map(Result::ok)` on `query_map` results, or swallowed query errors into default empty vectors/zero counts. SQLite’s dynamic value types allow a stored value to fail a requested `FromSql` conversion even if a table has no rigid column type.
- **Safe reproduction:** The regression fixture inserts text into workspace boolean/integer fields and asserts the list readers return an error rather than omit the row. The S3 fixture inserts malformed `allow_private_endpoint` data and verifies that a present-but-undecodable connection yields HTTP 500 while a genuinely absent connection remains HTTP 404.
- **Root cause:** Result iterators were treated as best-effort even though these endpoints represent authoritative management state; selected summaries also hid SQL failures as numeric defaults.
- **Security/functional impact:** Administrators could see incomplete resource inventories, understated storage/traffic counts, or a false “not found” diagnosis while underlying records remained in the database.
- **Proposed fix:** Collect row iterators as `Result<Vec<_>, _>` and propagate errors through the common database error response. Keep defaults only where the domain explicitly defines unavailable values as unknown.
- **Implemented change:** Updated the affected production query paths to return database errors rather than partial rows. S3 summary queries no longer turn SQL errors into zero; S3 connection reads distinguish missing rows from other database errors. Remaining `filter_map(Result::ok)` uses in `src/maintenance.rs` are filesystem directory enumeration, not SQL result handling.
- **Tests added:** `workspace_project_and_environment_lists_propagate_malformed_row_errors`; `s3_connection_reads_distinguish_missing_rows_from_database_decode_errors`. Existing module tests cover the changed API modules’ compilation and migration behavior.
- **Regression risks:** A damaged database now produces a visible API error instead of a partial response. The change may reveal pre-existing malformed rows that were previously hidden.
- **Commit:** `b31c9b7842c836e4beb0c42f2c94097b2c0ed961` and `6a98549` (`fix(db): propagate SQLite read failures`).
- **Remaining risk:** The regression fixtures use isolated SQLite and do not simulate physical database-page corruption. Filesystem backup enumeration remains best-effort and is outside this SQL finding.

### DB-003 — Startup migrations are not one atomic, versioned upgrade

- **ID:** DB-003
- **Title:** The startup migration sequence has no central schema version and no transaction spanning module migrations.
- **Severity:** P2 / Medium
- **Status:** AUDIT_REQUIRED
- **Confidence:** High for the architecture observation; unknown for a data-corruption consequence because module migrations are mostly additive and idempotent.
- **Module:** Startup schema initialization
- **File:** `src/main.rs` and module `migrate` functions
- **Function:** `main`, `ensure_sessions_metadata`, `ensure_metric_storage_columns`, `ensure_federation_identity_columns`, per-module `migrate`
- **Commit SHA:** `ca1273c24eac200173943d8b73fb522422437bc4`
- **Affected behavior:** A process or disk failure between module migrations can leave earlier schema/data changes committed while later migrations have not run. An update rollback restores the previous binary but does not restore the previous database schema.
- **Technical evidence:** Main executes the base schema and then calls migration functions sequentially without a shared transaction or migration-version ledger. Existing module tests cover selected migrations on fresh/legacy schemas and idempotent reruns, but not an interrupted full startup migration followed by binary rollback.
- **Safe reproduction:** Static control-flow inspection confirms sequential autocommit migrations. No production database was used; a complete failure-injection test of the startup sequence is still outstanding.
- **Root cause:** Schema evolution is distributed across modules without a recorded migration checkpoint or global rollback boundary.
- **Security/functional impact:** If a migration fails mid-upgrade, startup stops with a mixed schema. Restart may recover where each migration is safely repeatable, but old-binary compatibility is not established.
- **Proposed fix:** Before introducing a schema ledger or wrapping all migrations, test the exact module sequence on fresh and representative legacy fixtures, inject failures at each migration boundary, and define rollback compatibility. Then introduce a versioned, transaction-safe migration runner only if those tests show a concrete recovery gap.
- **Implemented change:** No architectural change in this phase; the issue is explicitly retained for migration failure-injection and installed-version testing.
- **Tests added:** None for full-sequence failure injection; existing targeted migration idempotency tests passed in the full suite.
- **Regression risks:** A central transaction may be incompatible with module migrations that require operations outside transactions; this must be validated before implementation.
- **Commit:** —
- **Remaining risk:** Upgrade interruption and rollback across historical production schemas remain unverified. This must be resolved before claiming migration rollback safety.

## Not an issue / verified controls

- `rusqlite 0.32.1` configures a five-second busy timeout at connection open; this was confirmed in the locked crate source, not assumed from application code.
- Main DB startup enables WAL and foreign keys. The in-memory regression tests validate session-grant cascade behavior for the separate root password CLI path.
- Existing tests exercise idempotent additive migrations in workspaces, app schemas, monitoring, dashboard preferences, workflow drafts, S3, deployment and other modules; they do not replace full upgrade failure-injection.
- SQLite Online Backup is used instead of copying the live database file; the existing consistent-snapshot test passed.

## Phase 8 verification

- `cargo test --locked --all-targets`: **246 passed, 0 failed**.
- `cargo check --locked --all-targets`: passed.
- `git diff --check`: passed.
- Full startup migration interruption, production backup/restore, and workload contention tests were not run. No database was taken from an installed ARG server.
