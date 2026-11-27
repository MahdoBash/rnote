#!/usr/bin/env bash
set -euo pipefail
package=com.github.flxzt.rnote
results=android-test-results
mkdir -p "$results"
trap 'adb logcat -d > "$results/logcat.txt"; adb shell dumpsys activity activities > "$results/activities.txt"; adb exec-out screencap -p > "$results/screen.png"' EXIT
adb install -r android-dist/Rnote-x86_64.apk
adb shell wm size 1280x1800
adb shell wm density 220
adb shell mkdir -p /sdcard/Documents /sdcard/Download
adb logcat -c
adb shell am start -W -n "$package/org.gtk.android.ToplevelActivity"
for attempt in $(seq 1 45); do
  if adb logcat -d -s 'Rnote:I' | grep -q 'Notebook window mapped'; then break; fi
  sleep 2
done
adb logcat -d -s 'Rnote:I' | grep 'Notebook window mapped'
adb shell pidof "$package"
adb exec-out screencap -p > "$results/notebook.png"

python3 packaging/android/test-notebook.py
