#!/bin/sh
# Builds the iPad trial app for the iOS simulator: target/ios/SpeakUp 6M.app.
# Needs Xcode and `rustup target add aarch64-apple-ios-sim`. Run it with:
#   xcrun simctl install booted "target/ios/SpeakUp 6M.app" && xcrun simctl launch booted com.speakup.sixmonths
set -e
cd "$(dirname "$0")/.."
TARGET=aarch64-apple-ios-sim
APP="target/ios/SpeakUp 6M.app"

# sherpa-onnx ships its simulator libraries as fat archives (x86_64 + arm64), which rustc can't
# read ("Unsupported archive identifier"): keep only the arm64 slice. Returns 0 if it thinned any.
thin_sherpa() {
  thinned=1
  for f in $(find "$HOME/Library/Caches/sherpa-rs/$TARGET" -name '*.a' -path '*simulator*' 2>/dev/null); do
    if lipo -info "$f" | grep -q '^Architectures in the fat file'; then
      lipo "$f" -thin arm64 -output "$f.thin" && mv "$f.thin" "$f" && thinned=0
    fi
  done
  return $thinned
}

# Same minimum iOS for Rust and the C/C++ parts (whisper.cpp, ring, bzip2) as in Info.plist.
export IPHONEOS_DEPLOYMENT_TARGET=15.0
# onnxruntime (in sherpa-onnx) calls ___isPlatformVersionAtLeast, which lives in clang's runtime.
export CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUSTFLAGS="-C link-arg=$(xcrun clang --print-resource-dir)/lib/darwin/libclang_rt.iossim.a"

build() {
  CMAKE_POLICY_VERSION_MINIMUM=3.5 cargo build --release --target $TARGET --bin speakup -j "${JOBS:-4}"
}

thin_sherpa || true
# The first build downloads sherpa-onnx and fails on the fat archives; thin them and go again.
build || { thin_sherpa && build; }

rm -rf "$APP" && mkdir -p "$APP"
cp target/$TARGET/release/speakup "$APP/speakup"
cp assets/icon/icon_256.png "$APP/AppIcon.png"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>SpeakUp 6M</string>
  <key>CFBundleDisplayName</key><string>SpeakUp 6M</string>
  <key>CFBundleIdentifier</key><string>com.speakup.sixmonths</string>
  <key>CFBundleExecutable</key><string>speakup</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleIconFile</key><string>AppIcon.png</string>
  <key>MinimumOSVersion</key><string>15.0</string>
  <key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>
  <!-- iPad only: the desktop layout needs a large screen. -->
  <key>UIDeviceFamily</key><array><integer>2</integer></array>
  <key>UIRequiresFullScreen</key><true/>
  <key>UISupportedInterfaceOrientations~ipad</key>
  <array><string>UIInterfaceOrientationLandscapeLeft</string><string>UIInterfaceOrientationLandscapeRight</string></array>
  <key>UILaunchScreen</key><dict/>
  <key>NSMicrophoneUsageDescription</key><string>따라 말하기 채점과 말하기 연습을 위해 목소리를 녹음합니다. 녹음은 이 기기에서만 처리됩니다.</string>
</dict>
</plist>
PLIST
codesign --force --sign - "$APP"
echo "만들어진 앱: $APP"
