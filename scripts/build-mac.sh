#!/usr/bin/env bash
# Build the macOS bundle: Voxable.app + a drag-to-install .dmg.
# Metal (Apple Silicon GPU) by default; pass --cpu for a portable CPU build.
set -euo pipefail
cd "$(dirname "$0")/.."

# whisper-rs runs bindgen, which needs a libclang. Xcode ships one, so there is no
# need for `brew install llvm`.
export LIBCLANG_PATH="${LIBCLANG_PATH:-$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib}"

FEATURES=(--features metal)
if [[ "${1:-}" == "--cpu" ]]; then
  FEATURES=()
  echo "Building CPU (no Metal)"
else
  echo "Building with Metal"
fi

npm install

# Only the .app — Tauri's own .dmg step runs bundle_dmg.sh, which needs Finder
# (AppleScript) and `hdiutil convert`. We build the disk image below instead.
npm run tauri build -- "${FEATURES[@]}" --bundles app

APP="target/release/bundle/macos/Voxable.app"
[[ -d "$APP" ]] || { echo "Expected $APP — the build did not produce it" >&2; exit 1; }

VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")
ARCH=$([[ "$(uname -m)" == "arm64" ]] && echo aarch64 || echo x64)
DMG="target/release/bundle/dmg/Voxable_${VERSION}_${ARCH}.dmg"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

mkdir -p "$(dirname "$DMG")"
# One-shot compressed image. `hdiutil convert` fails with EAGAIN on some managed
# Macs (Kandji ESF), and `create -format UDZO` avoids that path entirely.
hdiutil create -volname Voxable -srcfolder "$STAGE" -format UDZO -ov "$DMG" >/dev/null

echo
echo "Built:"
ls -lh "$APP/Contents/MacOS/voxable" "$DMG"
