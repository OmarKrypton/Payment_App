#!/usr/bin/env bash
# CSCEC Payment Voucher — Build & Install (Linux)
# Run: chmod +x build-and-install.sh && ./build-and-install.sh

set -euo pipefail

APP_NAME="cscec-payment"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY_PATH="/usr/local/bin/$APP_NAME"
DESKTOP_FILE="$HOME/.local/share/applications/CSCEC_Payment.desktop"

echo "============================================"
echo " CSCEC Payment Voucher — Build & Install"
echo "============================================"
echo ""

# ── 1. Install Rust ─────────────────────────────────────────
echo ">>> [1/7] Rust..."
if command -v rustc &>/dev/null; then
    echo "  rustc: OK ($(rustc --version))"
else
    echo "  Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "  Rust installed." 
fi

# ── 2. Install Node.js ──────────────────────────────────────
echo ""
echo ">>> [2/7] Node.js..."
if command -v node &>/dev/null; then
    echo "  node: OK ($(node --version))"
else
    echo "  Installing Node.js..."
    if command -v apt-get &>/dev/null; then
        curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
        sudo apt-get install -y -qq nodejs
    elif command -v pacman &>/dev/null; then
        sudo pacman -S --noconfirm nodejs npm
    elif command -v dnf &>/dev/null; then
        sudo dnf install -y nodejs npm
    else
        echo "  ERROR: No supported package manager. Install Node.js manually."
        exit 1
    fi
    echo "  Node.js installed."
fi
echo "  npm:  OK ($(npm --version))"

# ── 3. Install system build deps (WebKit GTK, etc.) ─────────
echo ""
echo ">>> [3/7] System build dependencies (Tauri prerequisites)..."
if command -v apt-get &>/dev/null; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq libwebkit2gtk-4.1-dev build-essential curl wget file \
        libssl-dev libayatana-appindicator3-dev librsvg2-dev
elif command -v pacman &>/dev/null; then
    sudo pacman -S --noconfirm webkit2gtk-4.1 base-devel curl wget file openssl \
        libayatana-appindicator librsvg
elif command -v dnf &>/dev/null; then
    sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file \
        libappindicator-gtk3-devel librsvg2-devel
elif command -v zypper &>/dev/null; then
    sudo zypper --non-interactive install webkit2gtk3-devel gcc gcc-c++ curl wget file \
        libopenssl-devel libappindicator3-devel librsvg-devel
fi
echo "  Build deps installed."

# ── 4. Install poppler + tesseract ──────────────────────────
echo ""
echo ">>> [4/7] Runtime dependencies (poppler + tesseract)..."
if command -v apt-get &>/dev/null; then
    sudo apt-get install -y -qq poppler-utils tesseract-ocr tesseract-ocr-chi-sim
elif command -v pacman &>/dev/null; then
    sudo pacman -S --noconfirm poppler tesseract tesseract-data-chi_sim
elif command -v dnf &>/dev/null; then
    sudo dnf install -y poppler-utils tesseract tesseract-langpack-chi_sim
elif command -v zypper &>/dev/null; then
    sudo zypper --non-interactive install poppler-tools tesseract-ocr tesseract-ocr-traineddata-chinese-simplified
fi

# Verify
for cmd in pdftotext tesseract; do
    if command -v $cmd &>/dev/null; then
        echo "  $cmd: found"
    else
        echo "  WARNING: $cmd not found in PATH"
    fi
done
if tesseract --list-langs 2>&1 | grep -q chi_sim; then
    echo "  chi_sim: OK"
else
    echo "  WARNING: chi_sim language data not installed"
fi

# ── 5. npm install ──────────────────────────────────────────
echo ""
echo ">>> [5/7] npm install..."
cd "$SCRIPT_DIR"
npm install
echo "  npm install: OK"

# ── 6. Build the app ────────────────────────────────────────
echo ""
echo ">>> [6/7] Building CSCEC Payment Voucher (npm run tauri build)..."
echo "  First build compiles Rust + TypeScript. Takes 5-15 minutes."
npm run tauri build
echo "  Build succeeded!"

# ── 7. Install the app ──────────────────────────────────────
echo ""
echo ">>> [7/7] Installing the app..."
sudo cp "$SCRIPT_DIR/src-tauri/target/release/$APP_NAME" "$BINARY_PATH"
sudo chmod +x "$BINARY_PATH"
echo "  Binary: $BINARY_PATH"

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
echo "  Desktop shortcut: $DESKTOP_FILE"

echo ""
echo "============================================"
echo " ALL DONE!"
echo "============================================"
echo ""
echo "Launch: $APP_NAME"
echo "  (from terminal, application menu, or desktop shortcut)"
