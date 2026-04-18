#!/usr/bin/env bash
# Build a staged Pasta.app bundle in target/Pasta.app. Idempotent — safe to
# re-run. Does not install anything; scripts/install-macos-app.sh and the
# release workflow both consume the output of this script.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this builder only supports macOS" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Pasta"
BUNDLE_NAME="${APP_NAME}.app"
BIN_NAME="pasta-launcher"
BUNDLE_ID="com.pasta.launcher"

# Prefer an explicit APP_VERSION (used by the release workflow to tag the
# bundle with the Git tag); otherwise fall back to the version in Cargo.toml.
if [[ -z "${APP_VERSION:-}" ]]; then
  APP_VERSION=$(awk -F '"' '/^version[[:space:]]*=/ { print $2; exit }' "${ROOT_DIR}/Cargo.toml")
fi
APP_VERSION="${APP_VERSION:-0.1.0}"

echo "Building release binary (v${APP_VERSION})..."
(
  cd "${ROOT_DIR}"
  cargo build --release
)

BIN_PATH="${ROOT_DIR}/target/release/${BIN_NAME}"
if [[ ! -x "${BIN_PATH}" ]]; then
  echo "error: release binary not found at ${BIN_PATH}" >&2
  exit 1
fi

STAGE_DIR="${ROOT_DIR}/target/${BUNDLE_NAME}"
CONTENTS_DIR="${STAGE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"
PLIST_PATH="${CONTENTS_DIR}/Info.plist"

rm -rf "${STAGE_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

cp "${BIN_PATH}" "${MACOS_DIR}/${BIN_NAME}"
chmod +x "${MACOS_DIR}/${BIN_NAME}"

ICON_PATH="${ROOT_DIR}/assets/AppIcon.icns"
if [[ -f "${ICON_PATH}" ]]; then
  cp "${ICON_PATH}" "${RESOURCES_DIR}/AppIcon.icns"
else
  echo "warning: AppIcon.icns not found — app will use default icon"
  echo "  Run: ./scripts/make-icon.sh path/to/icon.png"
fi

cat > "${PLIST_PATH}" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>${BIN_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${APP_VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${APP_VERSION}</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

# Ad-hoc sign the staged bundle. The installer re-signs after copying into
# /Applications; the release workflow relies on this signature.
if command -v codesign >/dev/null 2>&1; then
  if codesign --force --deep --sign - "${STAGE_DIR}"; then
    echo "Ad-hoc signed: ${STAGE_DIR}"
  else
    echo "warning: ad-hoc signing failed; the app may be blocked on first launch" >&2
  fi
else
  echo "warning: codesign not found; skipping ad-hoc signing" >&2
fi

echo "Built: ${STAGE_DIR}"
