#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This command is macOS only."
  exit 1
fi

TARGET_TRIPLE=""
ARGS=("$@")
for ((i = 0; i < ${#ARGS[@]}; i++)); do
  if [[ "${ARGS[$i]}" == "--target" || "${ARGS[$i]}" == "-t" ]]; then
    if ((i + 1 < ${#ARGS[@]})); then
      TARGET_TRIPLE="${ARGS[$((i + 1))]}"
    fi
  fi
done

echo "[1/3] Building macOS app bundle..."
pnpm tauri build -b app "$@"

PRODUCT_NAME="$(node -e "const c=require('./src-tauri/tauri.conf.json'); console.log(c.productName)")"
VERSION="$(node -e "const c=require('./src-tauri/tauri.conf.json'); console.log(c.version)")"
TARGET_DIR="$(awk -F'=' '/target-dir/ { gsub(/[ "]/, "", $2); print $2; exit }' src-tauri/.cargo/config.toml 2>/dev/null || true)"
if [[ -z "${TARGET_DIR}" ]]; then
  TARGET_DIR="target"
fi

BUNDLE_ROOT="src-tauri/${TARGET_DIR}/release/bundle"
MACOS_DIR="${BUNDLE_ROOT}/macos"
DMG_DIR="${BUNDLE_ROOT}/dmg"
APP_PATH="${MACOS_DIR}/${PRODUCT_NAME}.app"

if [[ ! -d "${APP_PATH}" ]]; then
  APP_PATH="$(find "${MACOS_DIR}" -maxdepth 1 -type d -name '*.app' -print -quit || true)"
fi

if [[ -z "${APP_PATH}" || ! -d "${APP_PATH}" ]]; then
  echo "Unable to find app bundle under ${MACOS_DIR}"
  exit 1
fi

if [[ -n "${TARGET_TRIPLE}" ]]; then
  case "${TARGET_TRIPLE}" in
    aarch64-apple-darwin | arm64-apple-darwin) ARCH_TAG="aarch64" ;;
    x86_64-apple-darwin) ARCH_TAG="x64" ;;
    universal-apple-darwin) ARCH_TAG="universal" ;;
    *) ARCH_TAG="${TARGET_TRIPLE}" ;;
  esac
else
  case "$(uname -m)" in
    arm64 | aarch64) ARCH_TAG="aarch64" ;;
    x86_64) ARCH_TAG="x64" ;;
    *) ARCH_TAG="$(uname -m)" ;;
  esac
fi

mkdir -p "${DMG_DIR}"
DMG_PATH="${DMG_DIR}/${PRODUCT_NAME}_${VERSION}_${ARCH_TAG}.dmg"
TMP_DMG="${DMG_DIR}/.${PRODUCT_NAME// /_}_${ARCH_TAG}.rw.dmg"
MOUNT_POINT=""
DEVICE=""
APP_ITEM_NAME="$(basename "${APP_PATH}")"
BG_NAME="wtga-dmg-bg.png"

generate_dmg_background() {
  local output_path="$1"
  local app_name="$2"
  local swift_file
  swift_file="$(mktemp /tmp/wtga-dmg-bg.XXXXXX.swift)"
  cat > "${swift_file}" <<'SWIFT'
import AppKit
import Foundation

let outPath = CommandLine.arguments[1]
let appName = CommandLine.arguments[2]
let width: CGFloat = 660
let height: CGFloat = 400
let image = NSImage(size: NSSize(width: width, height: height))

image.lockFocus()
let rect = NSRect(x: 0, y: 0, width: width, height: height)
NSColor(calibratedRed: 0.96, green: 0.97, blue: 0.99, alpha: 1.0).setFill()
rect.fill()

if let gradient = NSGradient(colors: [
  NSColor(calibratedRed: 0.83, green: 0.90, blue: 1.00, alpha: 1.0),
  NSColor(calibratedRed: 0.95, green: 0.97, blue: 1.00, alpha: 1.0)
]) {
  gradient.draw(in: NSRect(x: 0, y: 190, width: width, height: 210), angle: 90)
}

let panel = NSBezierPath(roundedRect: NSRect(x: 55, y: 105, width: 550, height: 190), xRadius: 20, yRadius: 20)
NSColor(calibratedWhite: 1.0, alpha: 0.85).setFill()
panel.fill()

let titleAttrs: [NSAttributedString.Key: Any] = [
  .font: NSFont.boldSystemFont(ofSize: 30),
  .foregroundColor: NSColor(calibratedRed: 0.13, green: 0.19, blue: 0.29, alpha: 1.0)
]
let subtitleAttrs: [NSAttributedString.Key: Any] = [
  .font: NSFont.systemFont(ofSize: 19, weight: .semibold),
  .foregroundColor: NSColor(calibratedRed: 0.19, green: 0.27, blue: 0.40, alpha: 1.0)
]
let hintAttrs: [NSAttributedString.Key: Any] = [
  .font: NSFont.systemFont(ofSize: 14, weight: .regular),
  .foregroundColor: NSColor(calibratedRed: 0.28, green: 0.34, blue: 0.44, alpha: 1.0)
]

"Install \(appName)".draw(at: NSPoint(x: 176, y: 258), withAttributes: titleAttrs)
"Drag to Applications".draw(at: NSPoint(x: 224, y: 226), withAttributes: subtitleAttrs)
"拖动左侧应用到右侧 Applications".draw(at: NSPoint(x: 188, y: 202), withAttributes: hintAttrs)

let arrow = NSBezierPath()
arrow.lineWidth = 10
arrow.lineCapStyle = .round
arrow.move(to: NSPoint(x: 260, y: 165))
arrow.line(to: NSPoint(x: 396, y: 165))
NSColor(calibratedRed: 0.23, green: 0.49, blue: 0.95, alpha: 1.0).setStroke()
arrow.stroke()

let head = NSBezierPath()
head.move(to: NSPoint(x: 396, y: 165))
head.line(to: NSPoint(x: 372, y: 180))
head.line(to: NSPoint(x: 372, y: 150))
head.close()
NSColor(calibratedRed: 0.23, green: 0.49, blue: 0.95, alpha: 1.0).setFill()
head.fill()

image.unlockFocus()

guard
  let tiff = image.tiffRepresentation,
  let rep = NSBitmapImageRep(data: tiff),
  let png = rep.representation(using: .png, properties: [:])
else {
  fputs("Failed to generate DMG background image.\n", stderr)
  exit(1)
}

try png.write(to: URL(fileURLWithPath: outPath))
SWIFT
  swift "${swift_file}" "${output_path}" "${app_name}"
  rm -f "${swift_file}"
}

apply_finder_layout() {
  local volume_name="$1"
  local item_name="$2"
  local applescript_file
  applescript_file="$(mktemp /tmp/wtga-dmg-layout.XXXXXX.applescript)"

  cat > "${applescript_file}" <<OSA
on run (volumeName)
  tell application "Finder"
    tell disk (volumeName as string)
      open
      set theXOrigin to 120
      set theYOrigin to 120
      set theWidth to 660
      set theHeight to 400
      set theBottomRightX to (theXOrigin + theWidth)
      set theBottomRightY to (theYOrigin + theHeight)
      tell container window
        set current view to icon view
        set toolbar visible to false
        set statusbar visible to false
        set the bounds to {theXOrigin, theYOrigin, theBottomRightX, theBottomRightY}
      end tell
      set opts to the icon view options of container window
      tell opts
        set icon size to 128
        set text size to 14
        set arrangement to not arranged
      end tell
      try
        set background picture of opts to file ".background:${BG_NAME}"
      end try
      set position of item "${item_name}" to {180, 190}
      set position of item "Applications" to {480, 190}
      close
      open
      delay 1
    end tell
    delay 1
  end tell
end run
OSA

  sleep 2
  /usr/bin/osascript "${applescript_file}" "${volume_name}"
  rm -f "${applescript_file}"
}

cleanup() {
  if [[ -n "${DEVICE}" ]]; then
    hdiutil detach "${DEVICE}" -quiet >/dev/null 2>&1 || true
  fi
  rm -f "${TMP_DMG}"
}
trap cleanup EXIT

rm -f "${DMG_PATH}" "${TMP_DMG}"

echo "[2/3] Creating writable DMG..."
hdiutil create -srcfolder "${APP_PATH}" -volname "${PRODUCT_NAME}" -fs HFS+ -format UDRW -ov "${TMP_DMG}" >/dev/null

ATTACH_OUTPUT="$(hdiutil attach -mountrandom /Volumes -readwrite -noverify -noautoopen -nobrowse "${TMP_DMG}")"
DEVICE="$(echo "${ATTACH_OUTPUT}" | awk '/^\/dev\// { print $1; exit }')"
MOUNT_POINT="$(echo "${ATTACH_OUTPUT}" | awk '/\/Volumes\// { print $3; exit }')"

if [[ -z "${DEVICE}" ]]; then
  echo "Failed to mount temporary DMG."
  exit 1
fi
if [[ -z "${MOUNT_POINT}" || ! -d "${MOUNT_POINT}" ]]; then
  echo "Failed to detect mounted DMG path."
  exit 1
fi

if [[ ! -L "${MOUNT_POINT}/Applications" ]]; then
  ln -s /Applications "${MOUNT_POINT}/Applications"
fi

mkdir -p "${MOUNT_POINT}/.background"
generate_dmg_background "${MOUNT_POINT}/.background/${BG_NAME}" "${PRODUCT_NAME}"

if ! apply_finder_layout "$(basename "${MOUNT_POINT}")" "${APP_ITEM_NAME}"; then
  echo "Warning: Failed to apply Finder layout. DMG will still be usable."
fi

sync
hdiutil detach "${DEVICE}" -quiet >/dev/null
DEVICE=""

echo "[3/3] Compressing final DMG..."
hdiutil convert "${TMP_DMG}" -format UDZO -imagekey zlib-level=9 -ov -o "${DMG_PATH}" >/dev/null

echo "DMG created: ${DMG_PATH}"
