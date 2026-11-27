#!/usr/bin/env bash
set -euo pipefail
arch=${1:?expected aarch64 or x86_64}
case "$arch" in aarch64) abi=arm64-v8a;; x86_64) abi=x86_64;; *) exit 2;; esac
cd "$(dirname "$0")"
repo=$(git rev-parse --show-toplevel)
sdk_dir="$repo/.android-sdk-$arch"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
: "${ANDROID_NDK_HOME:?Android NDK r28 or later required}"
git clone --quiet https://github.com/sp1ritCS/gtk-android-builder.git "$work/pixiewood"
git -C "$work/pixiewood" checkout --quiet de3719725a8f6e3efe0f28203e803e7ce1904902
pw="$work/pixiewood/pixiewood"
# Use the enum generator from the target GLib release on the host. Older
# generators misread G_GNUC_FLAG_ENUM as a type name in Adwaita headers.
mkdir -p "$work/tools"
curl -fsSL --retry 3 https://raw.githubusercontent.com/GNOME/glib/2.90.1/gobject/glib-mkenums.in \
  -o "$work/tools/glib-mkenums"
sed -i "1c #!$(command -v python3)" "$work/tools/glib-mkenums"
chmod +x "$work/tools/glib-mkenums"
# Meson's GNOME module looks up host tools in the cross file before
# consulting the build-machine GLib's absolute pkg-config tool paths.
sed -i "/^\[binaries\]/a glib-mkenums = '$work/tools/glib-mkenums'" "$work/pixiewood/prepare/android.cross"
sed "s/@ARCH@/$arch/g" app.xml.in > rnote.android.xml
perl "$pw" -v prepare --meson "$(command -v meson)" --sdk "$ANDROID_HOME" \
  --android-studio-dir "$ANDROID_HOME" --toolchain "$ANDROID_NDK_HOME" --release rnote.android.xml
ninja -C ".pixiewood/bin-$arch" subprojects/libadwaita/src/adw-enums.h
grep -q ADW_TYPE_TAB_VIEW_SHORTCUTS ".pixiewood/bin-$arch/subprojects/libadwaita/src/adw-enums.h"
git -C subprojects/gtk apply "$repo/packaging/android/chooser-name.patch"
perl "$pw" -v generate
icon_dir=.pixiewood/android/app/src/main/res/drawable-nodpi
mkdir -p "$icon_dir"
rsvg-convert -w 432 -h 432 "$repo/crates/rnote-ui/data/icons/scalable/apps/rnote.svg" -o "$icon_dir/rnote_icon.png"
perl "$pw" -v build --skip-gradle

# Install development headers and .pc metadata separately from the APK runtime.
mkdir -p "$sdk_dir/sysroot" "$sdk_dir/android"
meson install --no-rebuild -C ".pixiewood/bin-$arch" --destdir "$sdk_dir/sysroot"
cp -aL .pixiewood/android/. "$sdk_dir/android/"
# Meson installs JNI libraries under the ABI-specific directory. Gradle must
# extract them because GTK loads its entry library from nativeLibraryDir.
printf '\nandroid { packaging { jniLibs { useLegacyPackaging = true } } }\n' >> "$sdk_dir/android/app/build.gradle"

# Fontconfig and librsvg can use the same target XML parser and metadata.
test -f "$sdk_dir/sysroot/lib/$abi/pkgconfig/libxml-2.0.pc"
test -x "$sdk_dir/android/gradlew"
