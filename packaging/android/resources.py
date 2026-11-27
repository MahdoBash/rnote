"""Install the same Rnote resources as the desktop Meson build into GTK's APK assets."""
import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

assets, sysroot = map(Path, sys.argv[1:])
ui = Path("crates/rnote-ui")
engine = Path("crates/rnote-engine/data")
metadata = tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]
app_id = "com.github.flxzt.rnote"
id_path = "/" + app_id.replace(".", "/") + "/"
config = {
    "APP_NAME": "rnote", "APP_NAME_CAPITALIZED": "Rnote", "APP_ID": app_id,
    "APP_IDPATH": id_path, "APP_VERSION": metadata["version"], "APP_VERSION_SUFFIX": "",
    "APP_AUTHOR_NAME": "Felix Zwettler", "APP_AUTHORS": Path("AUTHORS").read_text().strip(),
    "APP_WEBSITE": metadata["homepage"], "APP_REPO_URL": metadata["repository"],
    "APP_ISSUES_URL": metadata["repository"] + "/issues",
    "APP_SUPPORT_URL": metadata["repository"] + "/discussions",
    "APP_DONATE_URL": "https://rnote.flxzt.net/#support", "PROFILE": "default",
    "GETTEXT_PACKAGE": "rnote", "LOCALEDIR": "/share/locale", "DATADIR": "/share", "LIBDIR": "/lib",
}
template = (ui / "src/config.rs.in").read_text()
(ui / "src/config.rs").write_text(
    re.sub(r"@(\w+)@", lambda m: json.dumps(config[m[1]], ensure_ascii=False), template)
)

schemas = assets / "glib-2.0/schemas"
schemas.mkdir(parents=True, exist_ok=True)
for f in (sysroot / "share/glib-2.0/schemas").glob("*"):
    if f.suffix in (".xml", ".override"):
        shutil.copyfile(f, schemas / f.name)
template = (ui / "data/app.gschema.xml.in").read_text()
(schemas / (app_id + ".gschema.xml")).write_text(
    template.replace("@APP_ID@", app_id).replace("@APP_IDPATH@", id_path)
)
subprocess.run(["glib-compile-schemas", "--strict", schemas], check=True)

for f in (ui / "po").glob("*.po"):
    target = assets / "locale" / f.stem / "LC_MESSAGES/rnote.mo"
    target.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["msgfmt", f, "-o", target], check=True)

files = re.findall(r"'((?:sounds|fonts)/[^']+\.(?:ttf|wav))'", (engine / "meson.build").read_text())
for name in files + ["fonts/LICENSES.md"]:
    source = engine / name
    target = assets / ("fonts/rnote-fonts" if name.startswith("fonts/") else "rnote/sounds")
    target.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target / source.name)
for license_file in (engine / "sounds").rglob("LICENSE*"):
    target = assets / "rnote/sounds" / license_file.relative_to(engine / "sounds")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(license_file, target)

shutil.copy2("packaging/android/fonts.conf", assets / "rnote/fonts.conf")

# GTK's bundled icons only cover its own controls. Include the themed
# symbolic icons the Rnote toolbars reference, with their original index.
theme = Path("/usr/share/icons/Adwaita")
dest = assets / "icons/Adwaita"
dest.mkdir(parents=True, exist_ok=True)
shutil.copy2(theme / "index.theme", dest)
for icon in theme.rglob("*-symbolic.svg"):
    target = dest / icon.relative_to(theme)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(icon, target)
