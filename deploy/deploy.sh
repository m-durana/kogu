#!/usr/bin/env bash
# Kogu deploy — builds the release binary + static frontend and installs the systemd service
# and nginx vhost. Idempotent. Run as root on the VPS from the repo root or anywhere.
#
#   sudo deploy/deploy.sh           # full build + install
#   sudo deploy/deploy.sh --app     # rebuild + restart app only (skip nginx)
#
# DNS + TLS are NOT done here (they need the Cloudflare record + a cert); see deploy/README.md.
set -euo pipefail

ROOT="/srv/miro/kanzi"
DOMAIN="kogu.miro.build"
WEBROOT="/var/www/miro/kogu"
BIN="$ROOT/bin/kogu"
SERVICE="kogu.service"
PORT=4100

cd "$ROOT"
source "$HOME/.cargo/env" 2>/dev/null || true

echo "==> building release binary"
( cd backend && cargo build --release )
mkdir -p "$ROOT/bin"
install -m 0755 backend/target/release/kanzi "$BIN"

echo "==> ensuring database"
[ -f "$ROOT/data/kanzi.sqlite" ] || { echo "!! data/kanzi.sqlite missing — run: pipeline/.venv/bin/python -m kanzipipe.build"; exit 1; }

echo "==> building frontend"
( cd frontend && pnpm install --frozen-lockfile && pnpm run build )
mkdir -p "$WEBROOT"
rsync -a --delete frontend/dist/ "$WEBROOT/"

echo "==> installing systemd service"
# free the port from the old Wenbun service if present (pivot)
systemctl disable --now wenbun.service 2>/dev/null || true
install -m 0644 deploy/$SERVICE /etc/systemd/system/$SERVICE
systemctl daemon-reload
systemctl enable "$SERVICE"
systemctl restart "$SERVICE"
sleep 6
curl -fsS "http://127.0.0.1:$PORT/health" >/dev/null && echo "   backend healthy on :$PORT" || { echo "!! backend not healthy"; journalctl -u "$SERVICE" -n 30 --no-pager; exit 1; }

if [ "${1:-}" = "--app" ]; then echo "done (app only)."; exit 0; fi

echo "==> installing nginx config"
install -m 0644 deploy/kogu-ratelimit.conf /etc/nginx/conf.d/kogu-ratelimit.conf
install -m 0644 deploy/kogu.miro.build.conf /etc/nginx/sites-available/kogu.miro.build.conf

if [ -f "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" ]; then
  ln -sf /etc/nginx/sites-available/kogu.miro.build.conf /etc/nginx/sites-enabled/kogu.miro.build.conf
  nginx -t && systemctl reload nginx
  echo "==> live at https://$DOMAIN"
else
  echo "!! no TLS cert for $DOMAIN yet — vhost staged but NOT enabled."
  echo "   1) ensure Cloudflare A record:  $DOMAIN -> $(curl -s https://api.ipify.org)"
  echo "   2) certbot certonly --webroot -w /var/www/letsencrypt -d $DOMAIN"
  echo "   3) re-run this script (it will enable the vhost)."
fi
