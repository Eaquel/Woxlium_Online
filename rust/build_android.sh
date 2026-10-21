#!/usr/bin/env bash
# Cross-compiles the Rust core for every Android ABI and copies the .so files
# into <out>/<abi>/libwoxlium.so. Requires: rustup targets + ANDROID_NDK_HOME.
set -euo pipefail
OUT="${1:?usage: build_android.sh <jniLibs-out-dir>}"
NDK="${ANDROID_NDK_HOME:?ANDROID_NDK_HOME not set}"
API=24
HOST_TAG=linux-x86_64
case "$(uname -s)" in Darwin) HOST_TAG=darwin-x86_64 ;; esac
TC="$NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
cd "$(dirname "$0")/woxlium_core"

build() {
  local target="$1" abi="$2" clang="$3"
  local var
  var="CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER"
  export "$var"="$TC/${clang}${API}-clang"
  export "CC_${target//-/_}"="$TC/${clang}${API}-clang"
  export "AR_${target//-/_}"="$TC/llvm-ar"
  # 16 KB page alignment (required for Android 15+ devices / Play)
  RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,--gc-sections" \
    cargo build --release --lib --target "$target"
  mkdir -p "$OUT/$abi"
  cp "target/$target/release/libwoxlium.so" "$OUT/$abi/libwoxlium.so"
}

build aarch64-linux-android   arm64-v8a   aarch64-linux-android
build armv7-linux-androideabi armeabi-v7a armv7a-linux-androideabi
build x86_64-linux-android    x86_64      x86_64-linux-android
ls -la "$OUT"/*/libwoxlium.so
