# Audit remediation status

Updated through phase 3 at revision `52824d034b0f988785866b89d0f6f7decb6894cc` on `audit/development-hardening`.

| ID | Severity | Status | Implemented fix | Regression test | Commit |
|---|---|---|---|---|---|
| ARG-001 | P2 | Fixed | Read-only domain validation no longer requires a write Origin header; admin auth and validation retained | `domain_check_get_works_with_or_without_origin_and_keeps_admin_auth` | `dfbda3c` |
| ARG-002 | P2 | Fixed | Federation terminal scope follows requested terminal mode; stale root-ticket cookie does not change user scope | `terminal_scope_tracks_requested_mode_not_root_ticket_cookie_presence` | `c9e3ffb` |
| ARG-003 | P2 | Fixed | Separate raw upload/editor caps and derived Base64 request boundaries | `upload_request_limit_accounts_for_base64_and_matches_decoded_file_limit`; editor limit test | `c30dfb8` |
| ARG-004 | P2 | Fixed | Filter hidden paths before visible cap; stable sorting and independent raw-entry ceiling | `tree_listing_filters_hidden_entries_before_visible_limit_and_sorts`; cap tests | `44812e6` |
| ARG-005 | P2 | Fixed | Origin validation plus exclusive disk-scan permit/429 | `disk_scan_requires_same_origin_before_admin_or_helper_work`; `only_one_privileged_disk_scan_can_run_at_a_time` | `cce425a` |
| ARG-006 | P3 | Not an issue | No change; draft network feature accurately says it is not enforced | Source review; no behavior change | — |
| AUTH-001 | P1 | Fixed | API key authorization requires an enabled admin owner; user privilege loss revokes keys | `api_keys_are_revoked_when_owner_loses_administrator_access` | `dbce1df` |
| AUTH-002 | P2 | Fixed | All `/api` responses receive private no-store headers | `cache_policy_covers_api_routes_without_changing_static_assets` | `d10c425` |
| AUTH-003 | P2 | Fixed | Login throttle keys include client IP and forwarded IP is trusted only from loopback | `login_throttles_are_scoped_to_the_client_ip_and_only_trust_forwarded_ip_from_loopback`; legacy lockout regression | `c310ce8` |
| AUTH-004 | P2 | Fixed | App-volume scan checks Origin before DB/helper work | `volume_scan_rejects_missing_and_cross_origin_requests` | `2ee2f4a` |
| PRIV-001 | P1 | Fixed | Helper requires both `arg` UID and membership in `/system.slice/arg.service` | `privileged_helper_accepts_only_the_panel_systemd_service_cgroup` | `d2f5f19` |
| PRIV-002 | P2 | Fixed | Eight-connection admission limit, 10-second request deadline, 4 MiB request cap, 32 MiB response cap | `privileged_socket_reader_rejects_oversized_and_stalled_requests`; `privileged_socket_response_is_bounded_and_must_be_complete` | `d2f5f19`, `52824d0` |
| PRIV-003 | P2 | Fixed | Scan keeps bounded top-K result sets instead of all paths | `disk_scan_keeps_only_bounded_top_entries_in_deterministic_order` | `512ba90` |

## Remaining verification

- Real systemd install/upgrade verification is not available in this workspace. PRIV-001 assumes the installer-generated unit name `arg.service` and should be validated on clean Debian and Ubuntu test VMs before release.
- The helper's full per-action path/TOCTOU review continues alongside phases 5–8.
- Phases 4–15 are not yet marked complete. Results must be appended as each phase is audited and tested.
