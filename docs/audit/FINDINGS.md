# Audit findings index

This is a running index for completed phases only. It is not a statement that the entire audit is complete. The audit branch is isolated from `development` and `main`.

## P1 / High

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| AUTH-001 | API keys survived owner demotion/disable | CONFIRMED | Fixed in `dbce1df` | [02-auth-security.md](02-auth-security.md) |
| PRIV-001 | Any process with UID `arg`, including Laravel workers, could invoke root helper | CONFIRMED | Fixed in `d2f5f19` | [03-privileged-helper.md](03-privileged-helper.md) |

## P2 / Medium

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| ARG-001 | Nginx domain-check GET incorrectly required Origin | CONFIRMED | Fixed in `dfbda3c` | [01-known-findings.md](01-known-findings.md) |
| ARG-002 | Stale root-ticket cookie changed normal Federation terminal scope | CONFIRMED | Fixed in `c9e3ffb` | [01-known-findings.md](01-known-findings.md) |
| ARG-003 | Upload request length and file-size message disagreed | CONFIRMED | Fixed in `c30dfb8` | [01-known-findings.md](01-known-findings.md) |
| ARG-004 | Hidden files consumed app-tree visible-entry cap | CONFIRMED | Fixed in `44812e6` | [01-known-findings.md](01-known-findings.md) |
| ARG-005 | Disk scan lacked direct Origin guard and its own concurrency gate | CONFIRMED (missing defense-in-depth) | Fixed in `cce425a` | [01-known-findings.md](01-known-findings.md) |
| AUTH-002 | API responses did not uniformly disable caching | CONFIRMED | Fixed in `d10c425` | [02-auth-security.md](02-auth-security.md) |
| AUTH-003 | Username-only login throttle allowed cross-IP account lockout | CONFIRMED | Fixed in `c310ce8` | [02-auth-security.md](02-auth-security.md) |
| AUTH-004 | App-volume scan POST did not verify Origin | CONFIRMED | Fixed in `2ee2f4a` | [02-auth-security.md](02-auth-security.md) |
| PRIV-002 | Helper accepted unbounded connection tasks and unbounded-time reads | CONFIRMED | Fixed in `d2f5f19`; response cap in `52824d0` | [03-privileged-helper.md](03-privileged-helper.md) |
| PRIV-003 | Deep disk scan retained all paths before truncating output | CONFIRMED | Fixed in `512ba90` | [03-privileged-helper.md](03-privileged-helper.md) |
| FED-001 | Private endpoint opt-in allowed cloud metadata targets and CGNAT classification was incomplete | CONFIRMED | Fixed in `6852cdd` | [04-federation.md](04-federation.md) |
| DEPLOY-001 | Cancelling an active deployment did not stop Git/build work | CONFIRMED | Fixed in `14aa314` | [05-deployment.md](05-deployment.md) |
| DEPLOY-002 | Concurrent ZIP uploads could consume system disk reserve | CONFIRMED | Fixed in `d848967` | [05-deployment.md](05-deployment.md) |

## P3 / Low

| ID | Finding | Status | Resolution | Details |
|---|---|---|---|---|
| FED-002 | Repeated workspace-session renewal can create unbounded valid session rows | AUDIT_REQUIRED | Open; retry-safe idempotency design needed | [04-federation.md](04-federation.md) |

## Not an issue

| ID | Observation | Result | Evidence |
|---|---|---|---|
| ARG-006 | Private-network UI/API reports a draft with no active enforcement | NOT_AN_ISSUE | [01-known-findings.md](01-known-findings.md) |

## Open audit coverage

Phases 0–5 have been reviewed and documented. Phases 6–15 remain unreviewed or incomplete and may add findings. In particular, this index must not be interpreted as a clean full-project security assessment.
