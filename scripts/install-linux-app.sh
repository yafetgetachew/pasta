#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "error: this installer only supports Linux" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Pasta"
BIN_NAME="pasta-launcher"
BUNDLE_ID="com.pasta.launcher"

BIN_DIR="${HOME}/.local/bin"
APPS_DIR="${HOME}/.local/share/applications"
HICOLOR_ROOT="${HOME}/.local/share/icons/hicolor"
POLKIT_USER_DIR="${HOME}/.local/share/polkit-1/actions"
POLKIT_SYS_DIR="/usr/share/polkit-1/actions"

POLICY_SRC="${ROOT_DIR}/packaging/linux/${BUNDLE_ID}.policy"
ICON_SRC="${ROOT_DIR}/assets/pasta.png"

# One PNG per hicolor size so loaders that validate pixel dims against the
# directory name accept the icon at every scale.
ICON_SIZES=(16 22 24 32 48 64 96 128 192 256 512)

echo "Building release binary..."
(
  cd "${ROOT_DIR}"
  cargo build --release
)

BIN_PATH="${ROOT_DIR}/target/release/${BIN_NAME}"
if [[ ! -x "${BIN_PATH}" ]]; then
  echo "error: release binary not found at ${BIN_PATH}" >&2
  exit 1
fi

mkdir -p "${BIN_DIR}" "${APPS_DIR}"
install -m 0755 "${BIN_PATH}" "${BIN_DIR}/${BIN_NAME}"
echo "Installed binary: ${BIN_DIR}/${BIN_NAME}"

if [[ ! -f "${ICON_SRC}" ]]; then
  echo "warning: ${ICON_SRC} not found — desktop entry will have no icon"
else
  RESIZER=""
  if command -v magick >/dev/null 2>&1; then
    RESIZER="magick"
  elif command -v convert >/dev/null 2>&1; then
    RESIZER="convert"
  fi

  if [[ -z "${RESIZER}" ]]; then
    # Fall back to a single 512x512 slot without resampling.
    FALLBACK_DIR="${HICOLOR_ROOT}/512x512/apps"
    mkdir -p "${FALLBACK_DIR}"
    install -m 0644 "${ICON_SRC}" "${FALLBACK_DIR}/${BUNDLE_ID}.png"
    echo "Installed icon:   ${FALLBACK_DIR}/${BUNDLE_ID}.png"
    echo "note: install 'imagemagick' to emit correctly-sized icons per hicolor size directory"
  else
    for size in "${ICON_SIZES[@]}"; do
      dest_dir="${HICOLOR_ROOT}/${size}x${size}/apps"
      mkdir -p "${dest_dir}"
      dest_file="${dest_dir}/${BUNDLE_ID}.png"
      "${RESIZER}" "${ICON_SRC}" -resize "${size}x${size}" -strip +repage \
        "${dest_file}"
      chmod 0644 "${dest_file}"
    done
    # HiDPI menus pick up the pristine 1024x1024 source without resampling.
    SRC_DIMS=$("${RESIZER}" identify -format "%wx%h" "${ICON_SRC}" 2>/dev/null || echo "")
    if [[ "${SRC_DIMS}" == "1024x1024" ]]; then
      dest_dir="${HICOLOR_ROOT}/1024x1024/apps"
      mkdir -p "${dest_dir}"
      install -m 0644 "${ICON_SRC}" "${dest_dir}/${BUNDLE_ID}.png"
    fi
    echo "Installed icons:  ${HICOLOR_ROOT}/{${ICON_SIZES[*]// /,}}x…/apps/${BUNDLE_ID}.png"
  fi
fi

DESKTOP_PATH="${APPS_DIR}/${BUNDLE_ID}.desktop"
DESKTOP_SRC="${ROOT_DIR}/packaging/linux/${BUNDLE_ID}.desktop"
if [[ -f "${DESKTOP_SRC}" ]]; then
  # Rewrite Exec to the absolute user-local path so the .desktop works
  # without requiring ~/.local/bin on PATH.
  sed "s|^Exec=pasta-launcher$|Exec=${BIN_DIR}/${BIN_NAME}|" \
    "${DESKTOP_SRC}" > "${DESKTOP_PATH}"
  chmod 0644 "${DESKTOP_PATH}"
  echo "Installed .desktop:  ${DESKTOP_PATH}"
else
  echo "warning: ${DESKTOP_SRC} not found" >&2
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APPS_DIR}" >/dev/null 2>&1 || true
fi

# Hint the icon lookup that this is a valid hicolor theme root; without an
# index.theme some icon loaders skip the per-user hicolor tree entirely.
if [[ ! -f "${HICOLOR_ROOT}/index.theme" && -f /usr/share/icons/hicolor/index.theme ]]; then
  install -m 0644 /usr/share/icons/hicolor/index.theme "${HICOLOR_ROOT}/index.theme"
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "${HICOLOR_ROOT}" >/dev/null 2>&1 || true
fi
# Force Plasma to re-read our .desktop; incremental rebuilds sometimes keep
# a stale missing-icon entry cached.
rm -f "${HOME}/.cache/ksycoca6"* 2>/dev/null || true
if command -v kbuildsycoca6 >/dev/null 2>&1; then
  kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
elif command -v kbuildsycoca5 >/dev/null 2>&1; then
  kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
fi

# ---------------------------------------------------------------------------
# Polkit policy — required so reveal-secret and clear-history prompt for the
# user's password / howdy face scan instead of silently granting access.
# ---------------------------------------------------------------------------
if [[ ! -f "${POLICY_SRC}" ]]; then
  echo "warning: polkit policy not found at ${POLICY_SRC}; auth prompts will be denied" >&2
else
  INSTALLED_POLICY=""
  # Prefer a per-user install when the modern polkitd picks it up.
  if mkdir -p "${POLKIT_USER_DIR}" 2>/dev/null && install -m 0644 "${POLICY_SRC}" "${POLKIT_USER_DIR}/$(basename "${POLICY_SRC}")" 2>/dev/null; then
    INSTALLED_POLICY="${POLKIT_USER_DIR}/$(basename "${POLICY_SRC}")"
  fi

  # Always also try the system path — older polkitd ignores the per-user one.
  SYS_POLICY="${POLKIT_SYS_DIR}/$(basename "${POLICY_SRC}")"
  if [[ ! -f "${SYS_POLICY}" ]] || ! cmp -s "${POLICY_SRC}" "${SYS_POLICY}"; then
    echo ""
    echo "Polkit policy needs to be installed to ${POLKIT_SYS_DIR} (requires sudo)."
    if sudo install -m 0644 "${POLICY_SRC}" "${SYS_POLICY}"; then
      INSTALLED_POLICY="${SYS_POLICY}"
    else
      echo "warning: could not install system polkit policy; falling back to ${POLKIT_USER_DIR}" >&2
    fi
  else
    INSTALLED_POLICY="${SYS_POLICY}"
  fi

  if [[ -n "${INSTALLED_POLICY}" ]]; then
    echo "Installed polkit:    ${INSTALLED_POLICY}"
    if command -v pkaction >/dev/null 2>&1; then
      pkaction --action-id "${BUNDLE_ID}.reveal-secret" >/dev/null 2>&1 \
        && echo "Verified polkit action ${BUNDLE_ID}.reveal-secret" \
        || echo "note: pkaction could not verify the action — a logout/login may be required"
    fi
  fi
fi

echo ""
echo "Done. Launch Pasta from your app menu or run: ${BIN_DIR}/${BIN_NAME}"
echo ""
echo "If the app-menu icon still renders blank, Plasma is holding a stale"
echo "kickoff thumbnail from the previous install. Restart plasmashell:"
echo "  kquitapp6 plasmashell && kstart plasmashell     # Plasma 6"
echo "  kquitapp5 plasmashell && kstart5 plasmashell    # Plasma 5"
echo "or simply log out and back in."
echo ""
echo "Secret reveal authenticates through polkit → PAM. If you have Howdy"
echo "installed and enrolled (sudo howdy add) and your distro's system-auth"
echo "stack includes pam_howdy.so, face recognition will be tried before the"
echo "password prompt."
