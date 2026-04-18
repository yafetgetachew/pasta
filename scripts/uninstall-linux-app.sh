#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "error: this uninstaller only supports Linux" >&2
  exit 1
fi

BIN_NAME="pasta-launcher"
BUNDLE_ID="com.pasta.launcher"
APP_DIR_NAME="pasta-launcher"

PURGE=0
for arg in "$@"; do
  case "$arg" in
    --purge)
      PURGE=1
      ;;
    -h|--help)
      cat <<USAGE
Usage: $(basename "$0") [--purge]

Removes Pasta files installed by scripts/install-linux-app.sh.

  (no flags)   Remove binary, .desktop, icon, and polkit policies.
  --purge      Also delete clipboard history, neural cache, autostart
               entry, and the secret-service keychain entry. DESTRUCTIVE.

RPM-based installs are not touched by this script. Use:
  sudo dnf remove pasta
USAGE
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
HICOLOR_ROOT="${HOME}/.local/share/icons/hicolor"
POLKIT_USER_DIR="${HOME}/.local/share/polkit-1/actions"
POLKIT_SYS_DIR="/usr/share/polkit-1/actions"

# Every size the installer may have dropped an icon into.
ICON_SIZES=(16 22 24 32 48 64 96 128 192 256 512 1024)

remove_quiet() {
  local path="$1"
  if [[ -e "$path" || -L "$path" ]]; then
    rm -f "$path"
    echo "Removed: $path"
  fi
}

echo "Uninstalling Pasta from user-local paths..."
remove_quiet "${BIN_DIR}/${BIN_NAME}"
remove_quiet "${APPS_DIR}/${BUNDLE_ID}.desktop"
for size in "${ICON_SIZES[@]}"; do
  remove_quiet "${HICOLOR_ROOT}/${size}x${size}/apps/${BUNDLE_ID}.png"
done
remove_quiet "${POLKIT_USER_DIR}/${BUNDLE_ID}.policy"

# System polkit policy (requires sudo). Skip silently if not present.
SYS_POLICY="${POLKIT_SYS_DIR}/${BUNDLE_ID}.policy"
if [[ -f "${SYS_POLICY}" ]]; then
  echo ""
  echo "Removing system polkit policy ${SYS_POLICY} (requires sudo)..."
  if sudo rm -f "${SYS_POLICY}"; then
    echo "Removed: ${SYS_POLICY}"
  else
    echo "warning: could not remove ${SYS_POLICY}" >&2
  fi
fi

# Refresh caches so the menu entry disappears immediately.
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APPS_DIR}" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "${HICOLOR_ROOT}" >/dev/null 2>&1 || true
fi
rm -f "${HOME}/.cache/ksycoca6"* 2>/dev/null || true
if command -v kbuildsycoca6 >/dev/null 2>&1; then
  kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
  kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
fi

if [[ "${PURGE}" -eq 1 ]]; then
  echo ""
  echo "--purge: removing user data, cache, autostart, and keychain entry..."

  DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}/${APP_DIR_NAME}"
  CACHE_DIR="${XDG_CACHE_HOME:-${HOME}/.cache}/${APP_DIR_NAME}"
  AUTOSTART="${XDG_CONFIG_HOME:-${HOME}/.config}/autostart/pasta.desktop"

  if [[ -d "${DATA_DIR}" ]]; then
    rm -rf "${DATA_DIR}"
    echo "Removed: ${DATA_DIR}"
  fi
  if [[ -d "${CACHE_DIR}" ]]; then
    rm -rf "${CACHE_DIR}"
    echo "Removed: ${CACHE_DIR}"
  fi
  remove_quiet "${AUTOSTART}"

  # Secret-service entry that stores the AES key for encrypted clipboard
  # secrets. Service + account must match src/storage.rs::KEYCHAIN_{SERVICE,
  # ACCOUNT}.
  if command -v secret-tool >/dev/null 2>&1; then
    if secret-tool clear service "${BUNDLE_ID}" account clipboard_encryption_key_v1 2>/dev/null; then
      echo "Removed keychain entry: service=${BUNDLE_ID} account=clipboard_encryption_key_v1"
    fi
  else
    echo "note: secret-tool not installed; open KWalletManager or Seahorse to"
    echo "      remove the '${BUNDLE_ID}' keychain entry manually."
  fi
fi

echo ""
echo "Done."
if [[ "${PURGE}" -eq 0 ]]; then
  echo "User data preserved. Run with --purge to delete clipboard history and keychain entry."
fi
