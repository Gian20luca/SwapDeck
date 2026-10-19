#!/usr/bin/env bash
# Dentro il container: clona l'ultimo commit da /src (sola lettura), costruisce come la CI e copia
# i pacchetti in /out. Cache di cargo e target nei volumi montati su /cache.
set -euo pipefail
TARGET=x86_64-unknown-linux-gnu

rm -rf /work && git clone --quiet /src /work
cd /work
echo "commit: $(git log -1 --format='%h %s')"

export CARGO_TARGET_DIR=/cache/target
pnpm config set store-dir /cache/pnpm-store >/dev/null
pnpm install --frozen-lockfile

cargo build --release --locked -p swapdeck-cli --bin swapdeck-cli --target "$TARGET"
cli="$CARGO_TARGET_DIR/$TARGET/release/swapdeck-cli"
mkdir -p apps/desktop/src-tauri/binaries
cp "$cli" "apps/desktop/src-tauri/binaries/swapdeck-cli-$TARGET"

# Config di rilascio come in CI (updater senza firma: gli artefatti dell'updater non servono qui).
node -e '
  const fs = require("fs");
  const c = JSON.parse(fs.readFileSync("apps/desktop/src-tauri/tauri.release.conf.json", "utf8"));
  fs.writeFileSync("apps/desktop/src-tauri/tauri.ci.conf.json", JSON.stringify(c));
'
pnpm --filter @swapdeck/desktop exec tauri build --ci --target "$TARGET" \
  --bundles appimage,deb --config src-tauri/tauri.ci.conf.json

node apps/vscode/scripts/package.mjs --target linux-x64 --cli "$cli"

b="$CARGO_TARGET_DIR/$TARGET/release/bundle"
rm -rf /out/*
cp "$b"/appimage/*.AppImage "$b"/deb/*.deb /out/
cp "$cli" /out/swapdeck-cli-linux-x64
cp apps/vscode/dist/swapdeck-vscode-linux-x64-*.vsix /out/
chmod -R a+rw /out
ls -la /out
