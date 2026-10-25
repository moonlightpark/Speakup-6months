#!/bin/sh
# Builds "SpeakUp 6M JP.app" with a microphone usage description, so macOS asks for (and remembers)
# microphone permission for the app itself instead of for the terminal that launched it.
set -e
cd "$(dirname "$0")/.."
cargo build --release
APP="target/release/SpeakUp 6M JP.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/speakup "$APP/Contents/MacOS/speakup"
cp assets/icon/icon.icns "$APP/Contents/Resources/icon.icns"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>SpeakUp 6M JP</string>
  <key>CFBundleDisplayName</key><string>SpeakUp 6M JP</string>
  <key>CFBundleIdentifier</key><string>com.speakup.sixmonths.japanese</string>
  <key>CFBundleExecutable</key><string>speakup</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>따라 말하기 채점과 말하기 연습을 위해 목소리를 녹음합니다. 녹음은 이 컴퓨터에서만 처리됩니다.</string>
</dict>
</plist>
PLIST
# Ad-hoc signature: required on Apple Silicon, and lets macOS remember the permission.
codesign --force --deep --sign - "$APP"
echo "만들어진 앱: $APP"
