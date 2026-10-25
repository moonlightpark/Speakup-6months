#!/bin/sh
# Builds the Android APK (phones and tablets): target/android/SpeakUp6M-JP-android-arm64.apk.
# Needs the Android SDK + NDK (ANDROID_HOME), `rustup target add aarch64-linux-android` and
# `cargo install cargo-ndk`. Install on a device/emulator with: adb install -r <apk>
set -e
cd "$(dirname "$0")/.."
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
BT="$SDK/build-tools/$(ls "$SDK/build-tools" | sort -V | tail -1)"
PLATFORM="$SDK/platforms/android-34/android.jar"
[ -f "$PLATFORM" ] || PLATFORM="$(ls -d "$SDK"/platforms/android-*/android.jar | sort -V | tail -1)"
NDK="${ANDROID_NDK_HOME:-$SDK/ndk/$(ls "$SDK/ndk" | sort -V | tail -1)}"
OUT=target/android
APK="$OUT/SpeakUp6M-JP-android-arm64.apk"

# whisper-rs-sys decides "link ggml-blas" by the build machine's OS (macOS), not the target's;
# nothing on Android uses it, so an empty library satisfies the linker.
mkdir -p "$OUT/stub"
[ -f "$OUT/stub/libggml-blas.a" ] || "$NDK/toolchains/llvm/prebuilt/$(ls "$NDK/toolchains/llvm/prebuilt" | head -1)/bin/llvm-ar" rcs "$OUT/stub/libggml-blas.a"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-L native=$PWD/$OUT/stub"

# ANDROID_NDK_ROOT: CMake's own Android support finds the NDK there (whisper.cpp).
CMAKE_POLICY_VERSION_MINIMUM=3.5 ANDROID_NDK_HOME="$NDK" ANDROID_NDK_ROOT="$NDK" \
  cargo ndk -t arm64-v8a -P 28 rustc --lib --release --crate-type cdylib -j "${JOBS:-4}"

rm -rf "$OUT/stage" && mkdir -p "$OUT/stage/lib/arm64-v8a" "$OUT/res/mipmap"
cp target/aarch64-linux-android/release/libspeakup.so "$OUT/stage/lib/arm64-v8a/"
# cpal's Android backend (oboe) is C++ and links the shared C++ runtime.
cp "$(find "$NDK/toolchains/llvm/prebuilt" -path '*aarch64-linux-android/libc++_shared.so' | head -1)" "$OUT/stage/lib/arm64-v8a/"

# WebPane.java (the listening lab's web view) → classes.dex.
rm -rf "$OUT/classes" && mkdir -p "$OUT/classes"
javac -source 8 -target 8 -bootclasspath "$PLATFORM" -nowarn -Xlint:-options -d "$OUT/classes" $(find android/java -name '*.java')
"$BT/d8" --release --min-api 28 --lib "$PLATFORM" --output "$OUT/stage" $(find "$OUT/classes" -name '*.class')

cp assets/icon/icon_256.png "$OUT/res/mipmap/ic_launcher.png"
"$BT/aapt2" compile --dir "$OUT/res" -o "$OUT/res.zip"
"$BT/aapt2" link -o "$OUT/base.apk" --manifest android/AndroidManifest.xml -I "$PLATFORM" \
  --min-sdk-version 28 --target-sdk-version 34 --version-name "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)" \
  "$OUT/res.zip"
(cd "$OUT/stage" && zip -qr ../base.apk lib classes.dex)
"$BT/zipalign" -f -p 4 "$OUT/base.apk" "$OUT/aligned.apk"
# Release key (the same one every time, so an installed app updates): ANDROID_KEYSTORE and
# ANDROID_KEYSTORE_PASS (CI: from secrets); without them the local debug key.
KS="${ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
PASS="${ANDROID_KEYSTORE_PASS:-android}"
"$BT/apksigner" sign --ks "$KS" --ks-pass "pass:$PASS" --key-pass "pass:$PASS" \
  --out "$APK" "$OUT/aligned.apk"
rm -f "$OUT/base.apk" "$OUT/aligned.apk" "$OUT/res.zip"
ls -lh "$APK"
