#!/bin/bash
# Build the macOS app bundle for the sidecar. CEF on macOS only runs from a
# bundle: the framework in Contents/Frameworks, and one helper app per
# Chromium process type next to it. Output: target/release/liveboxmix-browser.app
#
#   CEF_PATH=~/.cache/lbx-cef dev/mac-bundle.sh
set -e
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin"
cd "$(dirname "$0")/.."
: "${CEF_PATH:=$HOME/.cache/lbx-cef}"
export CEF_PATH
cargo build --release 2>&1 | grep -E '^(error|\s+-->)' -A6 | head -20 || true
NAME=liveboxmix-browser
REL=target/release
FW="$(find "$CEF_PATH" -maxdepth 3 -type d -name 'Chromium Embedded Framework.framework' | head -1)"
[ -d "$FW" ] || { echo "no CEF framework under $CEF_PATH"; exit 1; }
APP="$REL/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Frameworks" "$APP/Contents/Resources"

plist() { # $1 file, $2 executable, $3 identifier, $4 helper(0/1)
cat > "$1" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>$2</string>
  <key>CFBundleIdentifier</key><string>$3</string>
  <key>CFBundleName</key><string>$2</string>
  <key>CFBundleDisplayName</key><string>$2</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PL
}

cp "$REL/$NAME" "$APP/Contents/MacOS/$NAME"
plist "$APP/Contents/Info.plist" "$NAME" "io.github.psmux.liveboxmix.browser" 0
cp -R "$FW" "$APP/Contents/Frameworks/"
for kind in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
  H="$NAME Helper$kind"
  HAPP="$APP/Contents/Frameworks/$H.app"
  mkdir -p "$HAPP/Contents/MacOS"
  cp "$REL/$NAME-helper" "$HAPP/Contents/MacOS/$H"
  id="io.github.psmux.liveboxmix.browser.helper$(echo "$kind" | tr -d ' ()' | tr 'A-Z' 'a-z')"
  plist "$HAPP/Contents/Info.plist" "$H" "$id" 1
done
# Ad hoc signatures: arm64 refuses to run unsigned code, and the helpers must
# carry a signature for the framework to be loaded into them.
codesign --force --deep --sign - "$APP" 2>&1 | grep -v "replacing existing signature" || true
echo "bundle: $APP ($(du -sh "$APP" | cut -f1))"
