#!/usr/bin/env bash
#
# Converts a source PNG into a macOS .icns file with all required sizes.
# Usage: ./scripts/make-icon.sh [path-to-1024x1024-png]
#
# If no argument is provided, uses assets/icon.png.
# Outputs: assets/AppIcon.icns
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${1:-${ROOT_DIR}/assets/icon.png}"
ICONSET_DIR="${ROOT_DIR}/assets/AppIcon.iconset"
ICNS_PATH="${ROOT_DIR}/assets/AppIcon.icns"

if [[ ! -f "${SOURCE}" ]]; then
  echo "error: source PNG not found at ${SOURCE}" >&2
  echo "usage: $0 [path-to-1024x1024-png]" >&2
  exit 1
fi

echo "Creating iconset from: ${SOURCE}"
mkdir -p "${ICONSET_DIR}"

# Ensure source is a proper PNG (the file might be JPEG with a .png extension)
CONVERTED="${ICONSET_DIR}/_source.png"
sips -s format png "${SOURCE}" --out "${CONVERTED}" > /dev/null
SOURCE="${CONVERTED}"

# Generate all required icon sizes for macOS
sips -z   16   16 "${SOURCE}" --out "${ICONSET_DIR}/icon_16x16.png"      > /dev/null
sips -z   32   32 "${SOURCE}" --out "${ICONSET_DIR}/icon_16x16@2x.png"   > /dev/null
sips -z   32   32 "${SOURCE}" --out "${ICONSET_DIR}/icon_32x32.png"      > /dev/null
sips -z   64   64 "${SOURCE}" --out "${ICONSET_DIR}/icon_32x32@2x.png"   > /dev/null
sips -z  128  128 "${SOURCE}" --out "${ICONSET_DIR}/icon_128x128.png"    > /dev/null
sips -z  256  256 "${SOURCE}" --out "${ICONSET_DIR}/icon_128x128@2x.png" > /dev/null
sips -z  256  256 "${SOURCE}" --out "${ICONSET_DIR}/icon_256x256.png"    > /dev/null
sips -z  512  512 "${SOURCE}" --out "${ICONSET_DIR}/icon_256x256@2x.png" > /dev/null
sips -z  512  512 "${SOURCE}" --out "${ICONSET_DIR}/icon_512x512.png"    > /dev/null
sips -z 1024 1024 "${SOURCE}" --out "${ICONSET_DIR}/icon_512x512@2x.png" > /dev/null

# Convert iconset to icns
iconutil -c icns "${ICONSET_DIR}" -o "${ICNS_PATH}"

# Clean up the temporary iconset directory
rm -rf "${ICONSET_DIR}"

echo "Created: ${ICNS_PATH}"
