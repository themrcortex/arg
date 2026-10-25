## Overview

The dashboard presents current state and short-term trends. Metrics come from the Rust backend. A value that the host cannot report is displayed as unknown, not as zero or demo data.

## Metrics

- **CPU:** overall use and recorded samples. A short-term value is derived from ARG's sampler and is not necessarily the load on one CPU core.
- **Memory and Swap:** used/available RAM and Swap. OS cache can affect definitions of usage; for OOM diagnosis also inspect `free -h` and `journalctl -k`.
- **Disk:** filesystem capacity, use, and available blocks. The System page has the detailed mount inventory.
- **Uptime and Load:** time since boot and load averages. Load includes runnable or I/O-waiting work; it is not CPU percentage.
- **Network:** receive/send rates and stored traffic history. Exact per-process or per-port accounting is not universally available.
- **ARG process usage:** values reported for the panel process by the operating system.

## History and ranges

The service records resource samples into SQLite in the background; keeping the dashboard open is not required. Sampling interval and retention are configurable. Shorter intervals create more database writes and rows; shorter retention limits storage use.

A selected range only includes retained samples. Data cannot be reconstructed after a fresh install, a long outage, history being disabled, or retention cleanup. Each plotted point should have a time label, including when viewing multi-day ranges.

## Thresholds and alerts

CPU, RAM, and disk thresholds are configured in monitoring settings. In-panel alerts appear when current measurements exceed a configured threshold or enter the red state. The alert reflects a range crossing; clearing or refreshing the measurement updates the state. An old alert is not proof that the condition is still active.

## Fast Actions

Administrators can create supported actions or bounded custom commands, each with a title and target. Custom commands run as the ARG service account, not root, and have execution time/output limits. If selected, the UI can show that action's stdout/stderr and the selected Nginx site's error log as context. Never put passwords or tokens in a command.
