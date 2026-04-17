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

INSTALL_DIR="${HOME}/Applications"
if [[ -w "/Applications" ]]; then
  INSTALL_DIR="/Applications"
fi
mkdir -p "${INSTALL_DIR}"

# Delegate binary build + bundle staging to build-macos-bundle.sh so the
# layout/plist live in exactly one place (also called by release.yml).
"${ROOT_DIR}/scripts/build-macos-bundle.sh"

STAGE_DIR="${ROOT_DIR}/target/${BUNDLE_NAME}"
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

# Strip the quarantine attribute so Gatekeeper doesn't nag on first launch.
# This is safe here because the bundle was just produced locally from a
# repository checkout rather than downloaded from the internet.
if command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "${DEST_APP}" 2>/dev/null || true
fi

# Ad-hoc sign the bundle. This is not notarization — users on machines that
# enforce notarization (e.g. "App Management" restrictions, MDM) will still
# need to right-click → Open on first launch, or run:
#   spctl --add --label 'Pasta' "${DEST_APP}"
if command -v codesign >/dev/null 2>&1; then
  if codesign --force --deep --sign - "${DEST_APP}"; then
    echo "Ad-hoc signed: ${DEST_APP}"
  else
    echo "warning: ad-hoc signing failed; the app may be blocked on first launch" >&2
  fi
else
  echo "warning: codesign not found; skipping ad-hoc signing" >&2
fi

echo "Installed: ${DEST_APP}"
echo "You can launch it from Finder/Spotlight without a terminal."
echo
echo "First launch: if macOS blocks the app with a Gatekeeper warning, either:"
echo "  1) right-click the app in Finder → Open → Open, or"
echo "  2) run:  xattr -dr com.apple.quarantine \"${DEST_APP}\""
