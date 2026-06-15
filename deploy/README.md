# Deploying Wenbun (wenbun.miro.build)

Three-tier on the VPS, matching the existing `*.miro.build` convention:
- **Rust API** (`wenbun.service`) on `127.0.0.1:4100`, reads `data/kogu.sqlite`.
- **Static SPA** (Svelte build) at `/var/www/miro/wenbun`, served by nginx.
- **nginx** vhost `wenbun.miro.build` → static + `/api/*` reverse-proxy, TLS via Let's Encrypt.

## One-time, manual (needs you - external)
1. **Cloudflare DNS**: add an A record `wenbun.miro.build → 78.46.251.172`, **DNS-only (gray cloud)**
   (LE http-01 needs the origin reachable on :80; matches analytics/codepen).
2. **TLS cert** once DNS resolves:
   ```
   certbot certonly --webroot -w /var/www/letsencrypt -d wenbun.miro.build
   ```
   (LE renewals on this box temp-open :80 via ufw hooks - see other renewal confs.)

## Build + install (repeatable)
```
sudo deploy/deploy.sh          # release binary + frontend + service + nginx (enables vhost once cert exists)
sudo deploy/deploy.sh --app    # app-only rebuild + restart
```
The script is idempotent. Before the cert exists it stages the vhost but does not enable it, and
prints the exact DNS + certbot steps. After step 2, re-run to go live.

## Rebuilding the dictionary DB
```
cd pipeline && .venv/bin/python -m kogupipe.fetch && .venv/bin/python -m kogupipe.build
sudo systemctl restart wenbun
```

## Files
- `wenbun.service` - systemd unit (read-only hardened).
- `wenbun.miro.build.conf` - nginx vhost.
- `wenbun-ratelimit.conf` - http-context `limit_req_zone`s (→ `/etc/nginx/conf.d/`).
- `deploy.sh` - the installer.
