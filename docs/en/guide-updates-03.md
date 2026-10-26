# Choosing fast or full update

This guide walks through **Choosing fast or full update** in ARG’s Operations & administration area. It focuses on a safe, verifiable workflow. Exact controls can depend on your role and the service state; open the named page from the sidebar and inspect its current status before making changes.

## Before you begin

- Sign in with a role allowed to perform the operation. Changes to services, files, or host settings generally require an administrator.
- Open the relevant sidebar page and let its initial load finish. If the page says the feature is unavailable or still being checked, refresh its status before retrying.
- Recheck the exact host, path, bucket, service, or time range. A wrong destination can affect real data in network and storage workflows.

## Step-by-step

1. Open **Operations & administration** from the sidebar and locate **Choosing fast or full update**. Use that page’s search or filters when its list is long.
2. Select the target row or control. Check the destination, current state, and last-checked time. Do not interpret an unknown value as zero.
3. Set the requested option. Follow field hints for empty values, multi-select controls, and read-only states. For deletion, public access, or restart actions, read the confirmation summary before continuing.
4. Run the action and wait for its status or result. Do not close the page mid-operation or submit the same request repeatedly.
5. Verify the final state on the same page: status, error message, last-run time, and any available report or log. Refresh once afterward to confirm the result persisted.

## Options and result states

Interpret controls using the live data on this page. **Enabled/disabled** labels describe the current state; do not rely on color alone. **Refresh** reads current data and does not, by itself, change configuration. A disabled or pending control usually means a prerequisite is missing or an earlier operation has not finished.

For long-running actions, distinguish an in-progress state from success. A sampled metric or report may be old and should be read with its timestamp. A success message confirms only the operation that completed; it does not imply support for related features that are absent from the current interface.

## Details specific to this workflow

Compilation raises CPU and I/O; duration depends on CPU, memory, disk, and cache.

## Example

Suppose you need to use **Choosing fast or full update** for a real target. Select it by its full name and note the current state. Change only the intended target, review the confirmation, and run the action. When it completes, compare the new state with the original. If the result is unexpected, stop and preserve the error details; an immediate retry can duplicate effects.

## Troubleshooting and scope

- If the page cannot load its data, inspect the related service in **System Services** or check ARG logs, then refresh once.
- For access or connection failures, verify the target, DNS, port, and user role. For object storage, compare the endpoint and bucket with the provider’s own settings.
- Keep the full error code and timestamp for an incident. Never share a password, token, access key, or private key in logs or screenshots.
- Do not repeat a mutating action while its previous result is uncertain.

## Related reference

This is a task-specific guide. The page-level reference is [the main guide](?page=updates). Confirm menu names and available capabilities against the running ARG version. This article does not claim support for controls absent from the current interface.
