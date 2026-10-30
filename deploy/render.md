# Deploying Split Card on Render

`render.yaml` at the repository root describes two services, both built from
Dockerfiles in this directory, in Frankfurt, deploying from `main` once
GitHub checks pass:

- **splitcard-web** (public web service, `web.Dockerfile`): the Next.js app.
  It proxies `/api/*` to the server's `/*`, and `/.well-known/lnurlp/*` and
  `/lnurlp/*` to the same paths, so browsers only ever talk to one origin.
  It finds the server through `SERVER_INTERNAL_URL`, which Render fills in.
- **splitcard-server** (private service, `server.Dockerfile`): the API, with
  a 1 GB disk at `/var/data` holding the SQLite database. Only
  splitcard-web can reach it. Render restarts it after 60 s of failed
  health checks. Private services get TCP health checks; `/health` is
  checked by the smoke test.

Production starts with a **fresh database and a fresh server key**. Never
copy a local database, key or `.env` file into the repository, an image or
a Render disk.

Lightning Address (LNURL) endpoints are not available yet. The proxy paths
for them are in place, and the two LNURL smoke steps report SKIP.

## Single service (alternative)

`app.Dockerfile` builds one image that runs both the website and the API.
Use it to deploy with a single Render web service instead of the Blueprint.
Inside the container, the server listens on `127.0.0.1:4000` only, and the
website on `PORT` proxies to it, so only the website is reachable from
outside. The server runs as its own user, and the website runs as `node`. If
either process exits, the container stops and Render restarts it.

In the Render dashboard, create a **New → Web Service** from the
`safehand-org/safehand` repository, branch `main`, with:

- **Language**: Docker. An existing Node service can't be switched to
  Docker, so create a new one and delete the old one afterwards.
- **Dockerfile Path**: `./deploy/app.Dockerfile`, **Docker Build Context
  Directory**: `.`
- **Health Check Path**: `/api/health`
- **Disk**: mount path `/var/data`, 1 GB. Disks need a paid instance type.
- **Environment**:
  - `PUBLIC_BASE_URL`: `https://<service>.onrender.com/api`, or
    `https://yourdomain/api` once the domain points at this service.
  - `SERVER_SECRET_KEY`: the key from step 1.
  - `RELAYS`: `wss://relay.damus.io,wss://nos.lol,wss://relay.primal.net`
  - `LITESTREAM_*`: optional, see step 4.
  - Don't set `SERVER_INTERNAL_URL`, `PORT` or `SPLITCARD_DEV`.

Steps 1 and 3 to 6 below apply to this service in the same way, with this
service in place of splitcard-web and splitcard-server.

## 1. Generate the production server key

The server signs card events with its own Nostr key. Generate one on your
machine and keep it only in a password manager and in Render:

```sh
key=$(cargo run -q -p splitcard-server --example nip98 -- keygen)
NIP98_SECRET_KEY=$key cargo run -q -p splitcard-server --example nip98 -- pubkey
```

The second command prints the public key, which the server also logs at
startup (`server_pubkey=`). Don't write the key to a file in the repo,
and don't reuse a development key. Changing the key later changes which key
signs future card events, so treat it as permanent.

## 2. Create the Blueprint

1. In the Render dashboard, choose **New → Blueprint**, connect the
   `safehand-org/safehand` repository, and pick the `main` branch.
2. Render reads `render.yaml` and asks for the values marked `sync: false`:
   - `PUBLIC_BASE_URL`: `https://yourdomain/api`, the URL browsers use
     for the API. NIP-98 signatures are checked against this plus the
     request path, so it must match the domain exactly. Until the domain
     has moved (step 3), use `https://splitcard-web.onrender.com/api`.
   - `SERVER_SECRET_KEY`: the key from step 1.
   - `LITESTREAM_REPLICA_URL`, `LITESTREAM_ACCESS_KEY_ID`,
     `LITESTREAM_SECRET_ACCESS_KEY`: see step 4. Leave all three empty to
     start without continuous backups. The server refuses to start if
     only some of them are set.
3. Apply. Both images build, then the server starts on an empty disk and
   creates the database.

`SPLITCARD_DEV` must stay `0`. With it off, the server refuses an
`http://` `PUBLIC_BASE_URL`. The server also refuses to start if
`/var/data` is missing or not writable.

Because the server has a disk, Render stops the old instance before
starting the new one. Each deploy therefore has a short downtime. On
SIGTERM the server stops accepting connections, finishes in-flight requests
and background passes (up to 20 s), then closes the database.

## 3. Move the domain from Vercel

1. A day ahead, lower the TTL of the domain's DNS records (for example to
   300 s), so the switch propagates quickly.
2. In Render, open **splitcard-web → Settings → Custom Domains** and add the
   domain (and `www.` if used). Render shows the records to create: a
   `CNAME` to `splitcard-web.onrender.com` for subdomains, or the `A` record
   it lists for an apex domain.
3. At the DNS provider, replace the records that point at Vercel with
   Render's, and delete any Vercel `AAAA` records. If the domain's DNS is
   hosted by Vercel, edit the records in Vercel's DNS settings, or move the
   nameservers first.
4. Wait for Render to verify the domain and issue its certificate.
5. Set the server's `PUBLIC_BASE_URL` to `https://yourdomain/api`. Saving
   it redeploys the server.
6. Run the smoke test (step 5) against the domain. Then remove the domain
   from the Vercel project.

## 4. Continuous backups with Litestream and Cloudflare R2

The server image includes Litestream. When the three `LITESTREAM_*` values
are set, the container:

1. restores the database from the replica only if `/var/data` has no
   database yet (an existing database is never overwritten), then
2. runs the server under `litestream replicate`, which streams every change
   to R2.

To set it up:

1. In Cloudflare, create an R2 bucket, for example `splitcard-backups`.
2. Under **R2 → Manage API tokens**, create a token with **Object Read &
   Write** on that bucket only. Note the access key ID, the secret access key
   and your account ID.
3. Set these on splitcard-server:
   - `LITESTREAM_REPLICA_URL`:
     `s3://splitcard-backups/splitcard?endpoint=<account-id>.r2.cloudflarestorage.com&region=auto`
   - `LITESTREAM_ACCESS_KEY_ID`, `LITESTREAM_SECRET_ACCESS_KEY`: from the token.
4. After the redeploy, the server logs should show Litestream starting
   replication, and objects should appear under `splitcard/` in the bucket.

Litestream keeps a week of snapshots (`deploy/litestream.yml`). Render also
takes a daily snapshot of the disk, kept for 7 days.

## 5. Smoke test

From a checkout of the repository:

```sh
scripts/smoke.sh https://yourdomain [team-slug]
```

It checks that the website is served, that `/api/health` answers through
the proxy, and that a NIP-98 signed `PUT /api/me/profile` is accepted. The
signed request uses a fresh throwaway key and leaves a profile named
"Smoke test". The LNURL steps report SKIP until those endpoints exist. Each
step prints PASS, FAIL or SKIP, and the script exits non-zero if any step
fails.

## 6. Restore from a backup

**From a Render disk snapshot** (daily, last 7 days): open
**splitcard-server → Disks**, pick a snapshot and restore it. Render restarts
the service on the restored disk. Changes made after the snapshot are lost.

**From Litestream** (latest state, or a point in time):

- To inspect a backup without touching production, restore it to your
  machine:

  ```sh
  LITESTREAM_ACCESS_KEY_ID=... LITESTREAM_SECRET_ACCESS_KEY=... \
    litestream restore -o restored.db \
    "s3://splitcard-backups/splitcard?endpoint=<account-id>.r2.cloudflarestorage.com&region=auto"
  ```

  Add `-timestamp 2026-10-29T12:00:00Z` for a point in time. Delete
  `restored.db` when you are done, and never commit it.
- To replace the production database with the latest backup, open
  **splitcard-server → Shell** and move the current files aside:

  ```sh
  cd /var/data && for f in splitcard.db splitcard.db-wal splitcard.db-shm; do
    [ -e "$f" ] && mv "$f" "$f.replaced"; done
  ```

  Then use **Manual Deploy → Restart service** right away. On startup the
  entrypoint finds no database and restores it from R2. Once the restored
  data checks out, delete the `.replaced` files.
