#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

APP="target/release/bundle/Unmouse.app"

cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/unmouse "$APP/Contents/MacOS/unmouse"
cp assets/Info.plist "$APP/Contents/Info.plist"

codesign --force --sign "${UNMOUSE_SIGN_IDENTITY:--}" "$APP"

echo "built: $APP"
