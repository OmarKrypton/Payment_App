#!/usr/bin/env bash
set -euo pipefail

APP_NAME="cscec-payment"
BINARY_PATH="/usr/local/bin/$APP_NAME"
DESKTOP_FILE="$HOME/.local/share/applications/CSCEC_Payment.desktop"

echo "=== CSCEC Payment Voucher — Installer (Linux) ==="

# ── Install system dependencies ──────────────────────────────
echo ""
echo ">>> Installing poppler-utils and tesseract-ocr..."
if command -v apt-get &>/dev/null; then
  sudo apt-get update -qq
  sudo apt-get install -y -qq poppler-utils tesseract-ocr tesseract-ocr-chi-sim
elif command -v pacman &>/dev/null; then
  sudo pacman -S --noconfirm poppler tesseract tesseract-data-chi_sim
elif command -v dnf &>/dev/null; then
  sudo dnf install -y poppler-utils tesseract tesseract-langpack-chi_sim
elif command -v zypper &>/dev/null; then
  sudo zypper --non-interactive install poppler-tools tesseract-ocr tesseract-ocr-traineddata-chinese-simplified
else
  echo "WARNING: No supported package manager found."
  echo "Install these manually:"
  echo "  - poppler-utils  (provides pdftotext, pdftoppm)"
  echo "  - tesseract-ocr  with chi_sim language data"
fi

# Verify
echo ""
echo ">>> Verifying dependencies..."
if command -v pdftotext &>/dev/null; then
  echo "  pdftotext: OK ($(pdftotext --version 2>&1 | head -1))"
else
  echo "  WARNING: pdftotext not found in PATH"
fi
if command -v tesseract &>/dev/null; then
  echo "  tesseract: OK ($(tesseract --version 2>&1 | head -1))"
  if tesseract --list-langs 2>&1 | grep -q chi_sim; then
    echo "  chi_sim:   OK"
  else
    echo "  WARNING: chi_sim language data not found"
  fi
else
  echo "  WARNING: tesseract not found in PATH"
fi

# ── Install the app binary ───────────────────────────────────
echo ""
echo ">>> Installing $APP_NAME..."
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUNDLE="$SCRIPT_DIR/dist/$APP_NAME"
RELEASE="$SCRIPT_DIR/src-tauri/target/release/$APP_NAME"

if [ -f "$BUNDLE" ]; then
  sudo cp "$BUNDLE" "$BINARY_PATH"
  sudo chmod +x "$BINARY_PATH"
  echo "  Installed: $BUNDLE → $BINARY_PATH"
elif [ -f "$RELEASE" ]; then
  sudo cp "$RELEASE" "$BINARY_PATH"
  sudo chmod +x "$BINARY_PATH"
  echo "  Installed: $RELEASE → $BINARY_PATH"
elif command -v "$APP_NAME" &>/dev/null; then
  echo "  $APP_NAME already in PATH, skipping."
else
  echo "  WARNING: $APP_NAME binary not found."
  echo "  Build it first: cd src-tauri && cargo build --release"
  echo "  Then re-run this script."
fi

# ── Desktop shortcut ─────────────────────────────────────────
echo ""
echo ">>> Creating desktop shortcut..."
mkdir -p "$(dirname "$DESKTOP_FILE")"
cat > "$DESKTOP_FILE" << DESKTOP_EOF
[Desktop Entry]
Name=CSCEC Payment Voucher
Comment=Settlement calculation and PDF import tool
Exec=$BINARY_PATH
Type=Application
Terminal=false
Categories=Office;Finance;
DESKTOP_EOF
chmod +x "$DESKTOP_FILE"
echo "  Created: $DESKTOP_FILE"

echo ""
echo "=== Done! ==="
echo "Launch via application menu or run: $APP_NAME"
