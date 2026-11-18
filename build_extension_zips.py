"""Rebuilds the Chromium extension packages (Chrome Web Store / Edge Add-ons / generic) from
./extension and refreshes the unpacked copy on the Desktop. Firefox is built by
build_firefox_extension.py."""
import os
import shutil
import zipfile

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_EXT = os.path.join(BASE_DIR, 'extension')
VERSION = '2.7.5'
UNPACKED_COPY = r'C:\Users\Haider Nasrat\Desktop\SuperIDM-Extension'
ZIP_NAMES = [
    f'SuperIDM_Extension_v{VERSION}.zip',
    f'SuperIDM_Chrome_Store_v{VERSION}.zip',
    f'SuperIDM_Edge_Store_v{VERSION}.zip',
]
CODE_FILES = ['background.js', 'content.js', 'popup.js', 'popup.html', 'popup.css',
              'overlay.css', 'instagram_helper.js', 'manifest.json']


def iter_package_files():
    for root, _dirs, files in os.walk(SRC_EXT):
        for name in files:
            full = os.path.join(root, name)
            rel = os.path.relpath(full, SRC_EXT).replace(os.sep, '/')
            yield full, rel


def main():
    for zip_name in ZIP_NAMES:
        target = os.path.join(BASE_DIR, zip_name)
        with zipfile.ZipFile(target, 'w', zipfile.ZIP_DEFLATED) as z:
            for full, rel in iter_package_files():
                z.write(full, rel)
        print(f'Built {zip_name} ({os.path.getsize(target)} bytes)')

    if os.path.isdir(UNPACKED_COPY):
        for name in CODE_FILES:
            shutil.copy2(os.path.join(SRC_EXT, name), os.path.join(UNPACKED_COPY, name))
        print(f'Refreshed unpacked extension copy: {UNPACKED_COPY} (rpc_token.json left untouched)')


if __name__ == '__main__':
    main()
