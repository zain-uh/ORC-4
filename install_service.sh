#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="orc4.service"
TEMPLATE_FILE="orc4.service"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
USER_NAME="${SUDO_USER:-$USER}"
HOME_DIR="$(getent passwd "$USER_NAME" | cut -d: -f6)"
GENERATED_FILE="$(mktemp)"

if [[ ! -f "$PROJECT_DIR/$TEMPLATE_FILE" ]]; then
  echo "Missing template file: $PROJECT_DIR/$TEMPLATE_FILE"
  exit 1
fi

if [[ ! -x "$PROJECT_DIR/.venv/bin/python" ]]; then
  echo "Missing venv python at: $PROJECT_DIR/.venv/bin/python"
  echo "Create venv first: python3 -m venv .venv && source .venv/bin/activate && pip install -r requirements.txt"
  exit 1
fi

sed \
  -e "s|__USER__|$USER_NAME|g" \
  -e "s|__PROJECT_DIR__|$PROJECT_DIR|g" \
  -e "s|__HOME_DIR__|$HOME_DIR|g" \
  "$PROJECT_DIR/$TEMPLATE_FILE" > "$GENERATED_FILE"

sudo cp "$GENERATED_FILE" "/etc/systemd/system/$SERVICE_NAME"
rm -f "$GENERATED_FILE"

sudo systemctl daemon-reload
sudo systemctl enable "$SERVICE_NAME"
sudo systemctl restart "$SERVICE_NAME"

echo "Installed and started $SERVICE_NAME"
echo "Check status with: systemctl status $SERVICE_NAME"
echo "Follow logs with: journalctl -u $SERVICE_NAME -f"
