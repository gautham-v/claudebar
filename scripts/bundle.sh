#!/usr/bin/env bash
# Build a release binary and assemble target/Claudebar.app around it.
#
# The .app is not cosmetic: LSUIElement is what keeps claudebar out of the Dock.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Honour CARGO_TARGET_DIR / .cargo/config.toml rather than assuming ./target.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" \
  | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
TARGET_DIR="${TARGET_DIR:-$ROOT/target}"

APP="$TARGET_DIR/Claudebar.app"
BIN="$TARGET_DIR/release/claudebar"

cargo build --release --manifest-path "$ROOT/Cargo.toml"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/claudebar"
cp "$ROOT/assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Claudebar</string>
	<key>CFBundleDisplayName</key>
	<string>Claudebar</string>
	<key>CFBundleExecutable</key>
	<string>claudebar</string>
	<key>CFBundleIdentifier</key>
	<string>com.gauthamv.claudebar</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>LSUIElement</key>
	<true/>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
PLIST

# Sign with a stable identity when the machine has one. Ad-hoc signatures get a
# fresh code identity on every rebuild, which makes macOS treat each build as a
# different app: TCC re-prompts for calendar access (and claudebar re-prompts for
# its Keychain item) every single launch. Override with CODESIGN_IDENTITY.
IDENTITY="${CODESIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/.*"\(.*\)"/\1/p' | head -n 1)"
fi
if [ -n "$IDENTITY" ]; then
  codesign --force --options runtime --sign "$IDENTITY" "$APP" \
    || echo "warning: codesign with '$IDENTITY' failed; the Keychain item may not stick"
else
  echo "note: no codesigning identity found; signing ad-hoc, so macOS will re-prompt on every rebuild"
  codesign --force --sign - "$APP" 2>/dev/null \
    || echo "warning: ad-hoc codesign failed; the Keychain item may not stick"
fi

echo "built $APP"

# When CARGO_TARGET_DIR points elsewhere, keep the documented ./target/Claudebar.app
# path working via a symlink.
if [ "$TARGET_DIR" != "$ROOT/target" ]; then
  # A copy, not a symlink: Launch Services refuses to `open` a symlinked .app.
  mkdir -p "$ROOT/target"
  rm -rf "$ROOT/target/Claudebar.app"
  cp -R "$APP" "$ROOT/target/Claudebar.app"
  echo "copied to $ROOT/target/Claudebar.app"
fi
