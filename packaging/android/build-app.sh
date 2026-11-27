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
cp packaging/android/{RnoteApplication,NotebookPrintAdapter,NotebooksProvider}.java "$java_dir/"
activity="$project/app/src/main/java/org/gtk/android/ToplevelActivity.java"
# Hardware keyboard events are delivered to the focused Android View. GTK
# creates this surface after the Activity has already chosen its initial focus.
sed -i '/this.view.requestLayout();/i\            this.view.toplevel.requestFocus();' "$activity"
# Retain only folder-tree grants returned by the system chooser.
sed -i '/protected void onActivityResult(int requestCode, int resultCode, Intent data) {/a\        if (resultCode == RESULT_OK) com.github.flxzt.rnote.RnoteApplication.persistWorkspaceGrant(this, data);' "$activity"
sed -i 's/android:name="org.gtk.android.RuntimeApplication"/android:name="com.github.flxzt.rnote.RnoteApplication"/' "$project/app/src/main/AndroidManifest.xml"
manifest="$project/app/src/main/AndroidManifest.xml"
sed -i '/<\/application>/i\    <provider android:name="com.github.flxzt.rnote.NotebooksProvider" android:authorities="com.github.flxzt.rnote.notebooks" android:permission="android.permission.MANAGE_DOCUMENTS" android:grantUriPermissions="true" android:exported="true"><intent-filter><action android:name="android.content.action.DOCUMENTS_PROVIDER"/></intent-filter></provider>' "$manifest"
python3 packaging/android/resources.py "$assets" "$sysroot"
cargo rustc --locked --lib -p rnote --target "$target" --crate-type cdylib
so="target/$target/debug/librnote.so"
"$toolchain/bin/llvm-strip" --strip-unneeded -o "$native/librnote_mobile.so" "$so"
cp "$toolchain/sysroot/usr/lib/$target/libc++_shared.so" "$native/"
# Include notices alongside the data, fonts and sounds in the package.
cp LICENSE "$project/app/src/main/assets/COPYING"
# Sign the test copy with the runner's short-lived key. The distribution
# copy can then be signed separately without sending a private key to CI.
(cd "$project" && ./gradlew --no-daemon assembleRelease)
mkdir -p "$root/android-dist"
apk=$(find "$project/app/build/outputs/apk/release" -name "*-$abi-release-unsigned.apk" -print -quit)
test -n "$apk"
signer="$ANDROID_HOME/build-tools/36.0.0/apksigner"
if [[ "$arch" == x86_64 ]]; then
  keytool -genkeypair -keystore "$runtime/emulator.p12" -storetype PKCS12 \
    -storepass android -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 30 \
    -dname "CN=Rnote emulator test"
  "$signer" sign --ks "$runtime/emulator.p12" --ks-pass pass:android \
    --out "$root/android-dist/Rnote-$abi.apk" "$apk"
  "$signer" verify --verbose "$root/android-dist/Rnote-$abi.apk"
else
  cp "$apk" "$root/android-dist/Rnote-$abi-unsigned.apk"
  cp "$ANDROID_HOME/build-tools/36.0.0/lib/apksigner.jar" "$root/android-dist/"
fi
