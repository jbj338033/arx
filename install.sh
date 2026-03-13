#!/usr/bin/env bash
set -euo pipefail

ARX_VERSION="${ARX_VERSION:-latest}"
CADDY_VERSION="${CADDY_VERSION:-2.9.1}"
INSTALL_DIR="/usr/local/bin"
DATA_DIR="/var/lib/arx"
CONFIG_DIR="/etc/arx"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'

ok()   { echo -e "        ${GREEN}✓${RESET} $1"; }
fail() { echo -e "        ${RED}✗${RESET} $1"; exit 1; }
step() { echo -e "\n  ${BOLD}[$1]${RESET} $2"; }

echo ""
echo -e "  ${CYAN}╭─────────────────────────────────╮${RESET}"
echo -e "  ${CYAN}│${RESET}        ${BOLD}arx v${ARX_VERSION}${RESET}"
echo -e "  ${CYAN}│${RESET}  self-hosted deployment platform"
echo -e "  ${CYAN}╰─────────────────────────────────╯${RESET}"

step "1/5" "checking prerequisites..."

[ "$(uname -s)" = "Linux" ] && ok "linux $(uname -m)" || fail "arx only supports Linux"
[ "$(uname -m)" = "x86_64" ] || fail "arx only supports x86_64"
command -v systemctl >/dev/null 2>&1 && ok "systemd" || fail "systemd is required"
command -v docker >/dev/null 2>&1 && ok "docker" || fail "docker is required — install it first: https://docs.docker.com/engine/install"
[ "$(id -u)" -eq 0 ] || fail "run as root: curl -fsSL https://get.arx.dev | sudo sh"

step "2/5" "downloading binaries..."

if [ "$ARX_VERSION" = "latest" ]; then
    ARX_URL="https://github.com/arx-deploy/arx/releases/latest/download/arx-linux-x86_64"
else
    ARX_URL="https://github.com/arx-deploy/arx/releases/download/v${ARX_VERSION}/arx-linux-x86_64"
fi

curl -fsSL "$ARX_URL" -o "${INSTALL_DIR}/arx" || fail "failed to download arx binary"
chmod +x "${INSTALL_DIR}/arx"
ok "arx"

if ! command -v caddy >/dev/null 2>&1; then
    CADDY_URL="https://github.com/caddyserver/caddy/releases/download/v${CADDY_VERSION}/caddy_${CADDY_VERSION}_linux_amd64.tar.gz"
    curl -fsSL "$CADDY_URL" | tar xz -C "${INSTALL_DIR}" caddy || fail "failed to download caddy"
    chmod +x "${INSTALL_DIR}/caddy"
fi
ok "caddy v${CADDY_VERSION}"

step "3/5" "configuring services..."

mkdir -p "$DATA_DIR" "$CONFIG_DIR"

if [ ! -f "${CONFIG_DIR}/Caddyfile" ]; then
    cat > "${CONFIG_DIR}/Caddyfile" <<'CADDYFILE'
{
    admin localhost:2019
    auto_https disable_redirects
}
CADDYFILE
fi
ok "/etc/arx/Caddyfile"

if systemctl is-active --quiet arx.service 2>/dev/null; then
    systemctl stop arx.service
fi
if systemctl is-active --quiet arx-caddy.service 2>/dev/null; then
    systemctl stop arx-caddy.service
fi

cat > /etc/systemd/system/arx.service <<EOF
[Unit]
Description=Arx Deployment Platform
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/arx server --host 0.0.0.0 --port 8443 --db ${DATA_DIR}/arx.db
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=arx=info
Environment=CADDY_ADMIN_URL=http://localhost:2019

[Install]
WantedBy=multi-user.target
EOF
ok "arx.service"

cat > /etc/systemd/system/arx-caddy.service <<EOF
[Unit]
Description=Caddy (Arx reverse proxy)
After=network.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/caddy run --config ${CONFIG_DIR}/Caddyfile
ExecReload=${INSTALL_DIR}/caddy reload --config ${CONFIG_DIR}/Caddyfile
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
ok "arx-caddy.service"

step "4/5" "starting services..."

systemctl daemon-reload
systemctl enable --now arx-caddy.service || fail "failed to start arx-caddy.service"
ok "arx-caddy.service"
systemctl enable --now arx.service || fail "failed to start arx.service"
ok "arx.service"

step "5/5" "waiting for arx to start..."

for i in $(seq 1 15); do
    if curl -sf http://127.0.0.1:8443/api/v1/health >/dev/null 2>&1; then
        ok "arx is healthy"
        break
    fi
    if [ "$i" -eq 15 ]; then
        fail "arx did not start in time — check: journalctl -u arx.service"
    fi
    sleep 1
done

SERVER_IP=$(hostname -I | awk '{print $1}')

echo ""
echo -e "  ${GREEN}╭──────────────────────────────────────────╮${RESET}"
echo -e "  ${GREEN}│${RESET}  ${BOLD}arx is running!${RESET}                         ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}                                          ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}  server: ${CYAN}http://${SERVER_IP}:8443${RESET}          ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}                                          ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}  next steps:                             ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}    1. ${DIM}sudo arx admin initial-password${RESET}    ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}    2. ${DIM}arx login --url http://<ip>:8443 \\${RESET} ${GREEN}│${RESET}"
echo -e "  ${GREEN}│${RESET}       ${DIM}            --key <your-key>${RESET}        ${GREEN}│${RESET}"
echo -e "  ${GREEN}╰──────────────────────────────────────────╯${RESET}"
echo ""
