#!/usr/bin/env bash
# Bumps the app version in package.json, Cargo.toml and tauri.conf.json.
# Usage: ./scripts/release.sh 0.2.0
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>  e.g. $0 0.2.0"
  exit 1
fi
NEW_VERSION="$1"

cd "$(dirname "$0")/.."

node -e "
const fs = require('fs');
const v = '$NEW_VERSION';

const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
pkg.version = v;
fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');

const conf = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
conf.version = v;
fs.writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(conf, null, 2) + '\n');
"

sed -i "0,/^version = /s//version = \"$NEW_VERSION\"/" src-tauri/Cargo.toml

echo "Version bumped to $NEW_VERSION in package.json, tauri.conf.json, Cargo.toml"
echo "Next: git add -A && git commit -m 'Release v$NEW_VERSION' && git tag v$NEW_VERSION && git push && git push --tags"