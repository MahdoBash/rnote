"""Exercise an installed Rnote through stylus input, keyboard and the system document picker."""
import gzip
import json
import re
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path

results = Path("android-test-results")
package = "com.github.flxzt.rnote"
filename = "RnoteAndroidTest.rnote"


def adb(*args, binary=False):
    return subprocess.check_output(["adb", *args], text=not binary)


def key(*keys):
    command = "keycombination" if len(keys) > 1 else "keyevent"
    adb("shell", "input", command, *keys)


def picker_node(predicate):
    for _ in range(10):
        adb("shell", "uiautomator", "dump", "/sdcard/rnote-ui.xml")
        doc = adb("exec-out", "cat", "/sdcard/rnote-ui.xml")
        (results / "document-picker.xml").write_text(doc)
        for node in ET.fromstring(doc).iter("node"):
            if predicate(node.attrib):
                return node.attrib
        time.sleep(0.5)
    raise RuntimeError("Android document picker control did not appear")


def tap(node):
    x0, y0, x1, y1 = map(int, re.findall(r"\d+", node["bounds"]))
    adb("shell", "input", "tap", str((x0 + x1) // 2), str((y0 + y1) // 2))


def stroke(y):
    adb("shell", "input", "stylus", "swipe", "420", str(y), "860", str(y + 90), "320")
    time.sleep(0.6)


def read_notebook(remote, count):
    for _ in range(24):
        try:
            data = adb("exec-out", "cat", remote, binary=True)
            notebook = json.loads(gzip.decompress(data))
            def brush_count(obj):
                if isinstance(obj, dict):
                    return sum((key == "brushstroke") + brush_count(val) for key, val in obj.items())
                if isinstance(obj, list):
                    return sum(map(brush_count, obj))
                return 0
            actual = brush_count(notebook)
            if actual == count:
                return data
        except (OSError, EOFError, ValueError):
            pass
        time.sleep(0.5)
    raise AssertionError(f"Expected {count} persisted brush strokes in {remote}")


stroke(650)
key("KEYCODE_CTRL_LEFT", "KEYCODE_S")
field = picker_node(lambda n: n["class"] == "android.widget.EditText")
tap(field)
key("KEYCODE_CTRL_LEFT", "KEYCODE_A")
adb("shell", "input", "text", filename)
tap(picker_node(lambda n: n["text"].upper() == "SAVE" and n["enabled"] == "true"))
remote = ""
for _ in range(15):
    found = adb("shell", "for base in /sdcard/Download /sdcard/Documents "
                "/sdcard/Android/data/com.github.flxzt.rnote/files/share/Notebooks; do "
                f'if [ -d "$base" ]; then find "$base" -iname {filename}; fi; done').splitlines()
    remote = next((line.strip() for line in found if line.strip().endswith(filename)), "")
    if remote:
        break
    time.sleep(0.5)
if not remote:
    raise RuntimeError("Could not find the note written by DocumentsUI")
first = read_notebook(remote, 1)
(results / "first-save.rnote").write_bytes(first)

adb("shell", "am", "force-stop", package)
adb("shell", "am", "start", "-W", "-n", package + "/org.gtk.android.ToplevelActivity")
time.sleep(2)
key("KEYCODE_CTRL_LEFT", "KEYCODE_O")
tap(picker_node(lambda n: n["text"] == filename))
time.sleep(1)
stroke(920)
key("KEYCODE_CTRL_LEFT", "KEYCODE_S")
second = read_notebook(remote, 2)
(results / "reopened-and-edited.rnote").write_bytes(second)
(results / "reopened-and-edited.png").write_bytes(adb("exec-out", "screencap", "-p", binary=True))
print("PASS: A stylus stroke survived saving, process restart, reopening through DocumentsUI, editing and resaving.")
