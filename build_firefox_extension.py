import json
import os
import shutil
import zipfile

from sync_version import read_app_version

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_EXT = os.path.join(BASE_DIR, "extension")
VERSION = read_app_version()
DIST_FIREFOX_ZIP = os.path.join(BASE_DIR, f"SuperIDM_Firefox_v{VERSION}.zip")
DESKTOP_DIR = os.path.join(os.path.expanduser("~"), "Desktop")
DESKTOP_FIREFOX_ZIP = os.path.join(DESKTOP_DIR, f"SuperIDM_Firefox_v{VERSION}.zip")


def build_firefox_package():
    print(f"Building SuperIDM v{VERSION} for Mozilla Firefox (AMO Ready)...")

    with open(os.path.join(SRC_EXT, "manifest.json"), "r", encoding="utf-8") as f:
        manifest = json.load(f)

    manifest["version"] = VERSION
    manifest["browser_specific_settings"] = {
        "gecko": {
            "id": "superidm-extension@kutha.com",
            "strict_min_version": "142.0",
            "data_collection_permissions": {
                "required": ["none"]
            },
        }
    }

    # Firefox MV3 uses background.scripts instead of service_worker.
    # NOTE: Keep `"world": "MAIN"` on instagram_helper.js — supported natively since Firefox 128
    # (and required for page-world fetch/XHR/React Fiber reel detection).
    manifest["background"] = {
        "scripts": ["background.js"]
    }

    include_files = [
        "background.js",
        "content.js",
        "instagram_helper.js",
        "overlay.css",
        "popup.html",
        "popup.css",
        "popup.js",
        "rpc_token.json",
    ]

    with zipfile.ZipFile(DIST_FIREFOX_ZIP, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("manifest.json", json.dumps(manifest, indent=2, ensure_ascii=False))

        for fname in include_files:
            fpath = os.path.join(SRC_EXT, fname)
            if os.path.exists(fpath):
                z.write(fpath, fname)

        icons_dir = os.path.join(SRC_EXT, "icons")
        if os.path.exists(icons_dir):
            for icon_name in os.listdir(icons_dir):
                icon_path = os.path.join(icons_dir, icon_name)
                if os.path.isfile(icon_path):
                    z.write(icon_path, f"icons/{icon_name}")

    print(f"Successfully created Firefox package: {DIST_FIREFOX_ZIP} ({os.path.getsize(DIST_FIREFOX_ZIP)} bytes)")

    if os.path.isdir(DESKTOP_DIR):
        shutil.copy2(DIST_FIREFOX_ZIP, DESKTOP_FIREFOX_ZIP)
        print(f"Copied to Desktop: {DESKTOP_FIREFOX_ZIP}")


if __name__ == "__main__":
    build_firefox_package()
