#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: this installer only supports macOS" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Pasta"
LEGACY_APP_NAME="Pasta Launcher"
BUNDLE_NAME="${APP_NAME}.app"
LEGACY_BUNDLE_NAME="${LEGACY_APP_NAME}.app"
BIN_NAME="pasta-launcher"
BUNDLE_ID="com.pasta.launcher"
APP_VERSION="${APP_VERSION:-0.1.0}"

INSTALL_DIR="${HOME}/Applications"
if [[ -w "/Applications" ]]; then
  INSTALL_DIR="/Applications"
fi

echo "Building release binary..."
(
  cd "${ROOT_DIR}"
  CARGO_PROFILE_RELEASE_STRIP=none cargo build --release
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

mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}" "${INSTALL_DIR}"
cp "${BIN_PATH}" "${MACOS_DIR}/${BIN_NAME}"
chmod +x "${MACOS_DIR}/${BIN_NAME}"

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
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

DEST_APP="${INSTALL_DIR}/${BUNDLE_NAME}"
LEGACY_DEST_APP="${INSTALL_DIR}/${LEGACY_BUNDLE_NAME}"

if [[ -d "${LEGACY_DEST_APP}" ]]; then
  LEGACY_BACKUP_APP="${LEGACY_DEST_APP}.backup.$(date +%Y%m%d%H%M%S)"
  echo "Legacy app name detected, backing up to: ${LEGACY_BACKUP_APP}"
  mv "${LEGACY_DEST_APP}" "${LEGACY_BACKUP_APP}"
fi

if [[ -d "${DEST_APP}" ]]; then
  BACKUP_APP="${DEST_APP}.backup.$(date +%Y%m%d%H%M%S)"
  echo "Existing app detected, backing up to: ${BACKUP_APP}"
  mv "${DEST_APP}" "${BACKUP_APP}"
fi

cp -R "${STAGE_DIR}" "${DEST_APP}"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "${DEST_APP}" >/dev/null 2>&1 || true
fi

echo "Installed: ${DEST_APP}"
echo "You can launch it from Finder/Spotlight without a terminal."
