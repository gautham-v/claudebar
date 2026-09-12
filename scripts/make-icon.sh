#!/usr/bin/env bash
# Regenerate assets/AppIcon.icns from scripts/make-icon.swift.
#
# The .icns is committed, so this only needs running when the icon changes —
# the bundle script just copies the result.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

swift "$ROOT/scripts/make-icon.swift" "$WORK/master.png"

SET="$WORK/AppIcon.iconset"
mkdir -p "$SET"
for size in 16 32 128 256 512; do
  sips -z $size $size "$WORK/master.png" --out "$SET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size*2)) $((size*2)) "$WORK/master.png" --out "$SET/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$SET" -o "$ROOT/assets/AppIcon.icns"
echo "wrote $ROOT/assets/AppIcon.icns"
