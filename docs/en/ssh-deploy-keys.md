# SSH Keys and Private Repository Deployment

This page explains Git keys managed by ARG. Keys belong to the VPS selected in the workspace switcher. Select the Node that hosts an application before managing its keys. Key management is unavailable in the read-only **All servers** view.

## Deploy keys and account keys

- A **Deploy Key** is restricted to one GitHub repository. Read-only access is enough to fetch source; leave **Allow write access** disabled in GitHub.
- A **GitHub account key** follows the permissions of that account and can reach multiple repositories. Prefer a repository-scoped Deploy Key for site deployments.
- Private SSH fetch currently supports **GitHub**. Public HTTPS Git continues to support GitHub, GitLab, Bitbucket, and Codeberg.

## Create a Deploy Key from an application

1. Open an application in **Applications**, then open **Releases & deployment**.
2. Choose **Deploy source**, then **Private Git · Deploy Key**.
3. Enter the SSH clone URL, for example `git@github.com:acme/storefront.git`.
4. Select **Generate Deploy Key**. ARG creates an Ed25519 key and keeps the private key on the selected VPS.
5. Copy the public key or select its displayed text. Use the link to open that repository's GitHub Deploy Keys settings.
6. In GitHub, choose **Add deploy key**, enter a title, paste the public key, and keep **Allow write access** disabled.
7. Return to ARG, confirm that the key is registered, select a branch or tag, and start the deployment job.

The public key remains available in ARG for copying. The private key is never returned by the API or shown in the interface. If the public key has not been added to GitHub, SSH authentication fails and the deployment job reports an access error.

## Manage keys across the selected VPS

Open **SSH keys** in the sidebar. The page lists keys for the currently selected VPS with their display name, fingerprint, repository restriction, creation time, and last use.

- **Create:** generate a repository-scoped Deploy Key or a personal key for adding to a GitHub account.
- **View/copy:** only public key material can be retrieved.
- **Rename:** changes the ARG label only; the Git key is unchanged.
- **Delete:** removes the private and public files from this VPS. Remove the public key separately from GitHub to revoke the remote authorization.
- **Refresh GitHub host keys:** ARG downloads GitHub's advertised host keys from `api.github.com/meta` over verified HTTPS, validates them with `ssh-keygen`, then stores them for strict SSH host checking.

A key used by an active deployment job cannot be deleted until that job completes or is cancelled. Renaming or deleting a key does not change Deploy Keys already registered at GitHub.

## Security boundaries

- Ed25519 private keys are stored under `/var/lib/arg/ssh-keys/<id>` with mode `0600`; the directory is mode `0700`.
- Git uses batch mode and strict host-key verification. Host keys are fetched from GitHub's official HTTPS metadata endpoint. The `known_hosts` file is owned by the service and access-restricted.
- Git URLs containing tokens or passwords are rejected. Deploy does not run a user-supplied shell string or SSH configuration.
- ARG does not add a key to GitHub. Doing so would require a GitHub OAuth or repository-management token, which this flow does not request or store.
- If a key is deleted, generate a replacement, register its public key in GitHub, and deploy again.

## API

- `GET /api/ssh-keys` — list public key metadata; private key contents are excluded.
- `POST /api/ssh-keys` — create a key with `name`, `purpose`, and `repository` for a Deploy Key; optional `application_id` further scopes it.
- `PUT /api/ssh-keys/{id}` — rename a key in ARG.
- `DELETE /api/ssh-keys/{id}` — delete the local key; an active job blocks deletion.
- `GET|POST /api/ssh-keys/trusted-hosts/github` — read GitHub host-key status or refresh and validate host keys.

Write operations require an administrator, a valid Origin, and an audit event. Private keys are never included in API responses.
