# Deploying Kogu (kogu.miro.build)

Three-tier on the VPS, matching the existing `*.miro.build` convention:
- **Rust API** (`kogu.service`) on `127.0.0.1:4100`, reads `data/kogu.sqlite`.
- **Static SPA** (Svelte build) at `/var/www/miro/kogu`, served by nginx.
- **nginx** vhost `kogu.miro.build` -> static + `/api/*` reverse-proxy, TLS via Let's Encrypt.

There is also a loopback-only Japanese TTS sidecar (`tts/kogu-tts.service`); its setup lives in
`tts/README.md`.

These files encode this VPS's conventions (paths, letsencrypt webroot, Cloudflare DNS); adapt
them for your own host.

## One-time, manual (external)
1. **Cloudflare DNS**: add an A record `kogu.miro.build -> <VPS IP>`, **DNS-only (gray cloud)**
   (LE http-01 needs the origin reachable on :80).
2. **TLS cert** once DNS resolves:
   ```
   certbot certonly --webroot -w /var/www/letsencrypt -d kogu.miro.build
   ```

## Build + install (repeatable)
```
sudo deploy/deploy.sh          # release binary + frontend + service + nginx (enables vhost once cert exists)
sudo deploy/deploy.sh --app    # app-only rebuild + restart
```
The script is idempotent. Before the cert exists it stages the vhost but does not enable it, and
prints the exact DNS + certbot steps. After step 2, re-run to go live.

## Rebuilding the dictionary DB
See `pipeline/README.md` for the full build order (fetch, side-channel extractions, build). Short
form:
```
cd pipeline && .venv/bin/python -m kogupipe.fetch && .venv/bin/python -m kogupipe.build
sudo systemctl restart kogu
```

## Files
- `kogu.service` - systemd unit (read-only hardened; mirror of the installed copy).
- `kogu.miro.build.conf` - nginx vhost.
- `kogu-ratelimit.conf` - http-context `limit_req_zone`s (-> `/etc/nginx/conf.d/`).
- `deploy.sh` - the installer.
