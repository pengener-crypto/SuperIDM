"""Rebuilds the Chromium extension packages (Chrome Web Store / Edge Add-ons / generic) from
./extension and refreshes the unpacked copy on the Desktop. Version is read dynamically from
package.json via sync_version.py. Firefox is built by build_firefox_extension.py."""
import json
import os
import shutil
import zipfile

from sync_version import read_app_version

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_EXT = os.path.join(BASE_DIR, "extension")
VERSION = read_app_version()
DESKTOP_DIR = os.path.join(os.path.expanduser("~"), "Desktop")
UNPACKED_COPY = os.path.join(DESKTOP_DIR, "SuperIDM-Extension")
ZIP_NAMES = [
    f"SuperIDM_Extension_v{VERSION}.zip",
    f"SuperIDM_Chrome_Store_v{VERSION}.zip",
    f"SuperIDM_Edge_Store_v{VERSION}.zip",
]
CODE_FILES = [
    "background.js",
    "content.js",
    "popup.js",
    "popup.html",
    "popup.css",
    "overlay.css",
    "instagram_helper.js",
    "manifest.json",
]
STORE_EXCLUDED_FILES = {"طريقة_التثبيت_اليدوي.txt"}


def iter_package_files(is_store_pkg: bool):
    for root, _dirs, files in os.walk(SRC_EXT):
        for name in files:
            if is_store_pkg and name in STORE_EXCLUDED_FILES:
                continue
            full = os.path.join(root, name)
            rel = os.path.relpath(full, SRC_EXT).replace(os.sep, "/")
            yield full, rel


def load_chromium_manifest() -> str:
    with open(os.path.join(SRC_EXT, "manifest.json"), "r", encoding="utf-8") as f:
        manifest = json.load(f)
    manifest["version"] = VERSION
    # Remove Firefox-only gecko block from Chromium store packages to avoid unrecognized-key warnings
    manifest.pop("browser_specific_settings", None)
    return json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"


def main():
    # Safety check: never pack a personal RPC token into store packages
    tok_path = os.path.join(SRC_EXT, "rpc_token.json")
    if os.path.exists(tok_path):
        with open(tok_path, "r", encoding="utf-8") as f:
            tok_data = json.load(f)
        if tok_data.get("token"):
            raise RuntimeError("extension/rpc_token.json contains a non-empty token! Clear it before packaging.")

    chromium_manifest_str = load_chromium_manifest()

    for zip_name in ZIP_NAMES:
        is_store = "Store" in zip_name
        target = os.path.join(BASE_DIR, zip_name)
        with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED) as z:
            for full, rel in iter_package_files(is_store):
                if rel == "manifest.json" and is_store:
                    z.writestr("manifest.json", chromium_manifest_str)
                else:
                    z.write(full, rel)
        print(f"Built {zip_name} ({os.path.getsize(target)} bytes)")
        if os.path.isdir(DESKTOP_DIR):
            shutil.copy2(target, os.path.join(DESKTOP_DIR, zip_name))

    if os.path.isdir(UNPACKED_COPY):
        for name in CODE_FILES:
            shutil.copy2(os.path.join(SRC_EXT, name), os.path.join(UNPACKED_COPY, name))
        print(f"Refreshed unpacked extension copy: {UNPACKED_COPY} (rpc_token.json left untouched)")


if __name__ == "__main__":
    main()
