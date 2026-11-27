#!/usr/bin/env bash
set -euo pipefail
arch=${1:?expected aarch64 or x86_64}
case "$arch" in aarch64) abi=arm64-v8a;; x86_64) abi=x86_64;; *) exit 2;; esac
cd "$(git rev-parse --show-toplevel)"
root=$PWD
runtime="$root/.android-sdk-$arch"
sysroot="$runtime/sysroot"
project="$runtime/android"
native="$project/app/src/main/jniLibs/$abi"
assets="$project/app/src/main/assets/share"
target="$arch-linux-android"
var=${target//-/_}
upper=${var^^}
toolchain="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64"
cc="$toolchain/bin/${target}31-clang"
# Only target pkg-config is redirected: build scripts still use host GLib.
export "PKG_CONFIG_ALLOW_CROSS_$var=1" "PKG_CONFIG_SYSROOT_DIR_$var=$sysroot"
export "PKG_CONFIG_LIBDIR_$var=$sysroot/lib/$abi/pkgconfig:$sysroot/share/pkgconfig"
export "CARGO_TARGET_${upper}_LINKER=$cc" "CC_$var=$cc" "CXX_$var=${cc}++" "AR_$var=$toolchain/bin/llvm-ar"
export "CARGO_TARGET_${upper}_RUSTFLAGS=-Lnative=$sysroot/lib/$abi -C link-arg=-Wl,-z,max-page-size=16384"
export ANDROID_NDK="$ANDROID_NDK_HOME"
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_DEV_INCREMENTAL=false
export CARGO_BUILD_JOBS=2
java_dir="$project/app/src/main/java/com/github/flxzt/rnote"
mkdir -p "$java_dir"
cp packaging/android/RnoteApplication.java "$java_dir/"
sed -i 's/android:name="org.gtk.android.RuntimeApplication"/android:name="com.github.flxzt.rnote.RnoteApplication"/' "$project/app/src/main/AndroidManifest.xml"
python3 packaging/android/resources.py "$assets" "$sysroot"
cargo rustc --locked --lib -p rnote --target "$target" --crate-type cdylib
so="target/$target/debug/librnote.so"
"$toolchain/bin/llvm-strip" --strip-unneeded -o "$native/librnote_mobile.so" "$so"
cp "$toolchain/sysroot/usr/lib/$target/libc++_shared.so" "$native/"
# Include notices alongside the data, fonts and sounds in the package.
cp LICENSE "$project/app/src/main/assets/COPYING"
(cd "$project" && ./gradlew --no-daemon assembleDebug)
mkdir -p "$root/android-dist"
apk=$(find "$project/app/build/outputs/apk/debug" -name "*-$abi-debug.apk" -print -quit)
test -n "$apk"
cp "$apk" "$root/android-dist/Rnote-$abi.apk"
