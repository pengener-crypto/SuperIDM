"""Single source of truth for SuperIDM version numbers.

Reads the canonical version from package.json and synchronizes (or verifies)
every manifest, Rust config, MSIX manifest, browser extension file, and UI file.

Usage:
    python sync_version.py                 # Sync all files to package.json version
    python sync_version.py --check         # Fail (exit 1) if any file is out of sync
    python sync_version.py --set 2.7.7     # Update package.json and sync all files
    python sync_version.py --update-date   # Also stamp today's build date in index.html
"""
import argparse
import datetime
import json
import os
import re
import sys

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
PACKAGE_JSON = os.path.join(BASE_DIR, "package.json")
SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+$")


def read_app_version() -> str:
    with open(PACKAGE_JSON, "r", encoding="utf-8") as f:
        data = json.load(f)
    ver = str(data.get("version", "")).strip()
    if not SEMVER_RE.match(ver):
        raise ValueError(f"Invalid version in package.json: {ver!r}")
    return ver


def write_package_version(new_ver: str) -> None:
    if not SEMVER_RE.match(new_ver):
        raise ValueError(f"Version must be MAJOR.MINOR.PATCH (got {new_ver!r})")
    with open(PACKAGE_JSON, "r", encoding="utf-8") as f:
        text = f.read()
    updated = re.sub(
        r'("version"\s*:\s*")\d+\.\d+\.\d+(")',
        rf"\g<1>{new_ver}\g<2>",
        text,
        count=1,
    )
    with open(PACKAGE_JSON, "w", encoding="utf-8", newline="\n") as f:
        f.write(updated)


def _sync_rules(version: str, build_date: str | None = None):
    msix_ver = f"{version}.0"
    html_rules = [
        (r"(<title>SuperIDM &mdash; Download Engine v)\d+\.\d+\.\d+(</title>)", rf"\g<1>{version}\g<2>"),
        (r'(<p id="brandEngineVer"[^>]*>v)\d+\.\d+\.\d+( Engine</p>)', rf"\g<1>{version}\g<2>"),
        (r'(<div id="stAppVersionVal"[^>]*>)\d+\.\d+\.\d+(</div>)', rf"\g<1>{version}\g<2>"),
        (r"(const APP_VERSION\s*=\s*['\"])\d+\.\d+\.\d+(['\"];)", rf"\g<1>{version}\g<2>"),
        (r"(let title = 'SuperIDM — Download Engine v)\d+\.\d+\.\d+(';)", rf"\g<1>{version}\g<2>"),
        (r"(brandEngineVer:\s*'محرك v)\d+\.\d+\.\d+( فائق السرعة',)", rf"\g<1>{version}\g<2>"),
        (r"(brandEngineVer:\s*'v)\d+\.\d+\.\d+( Engine',)", rf"\g<1>{version}\g<2>"),
        (r"(\[INIT\] SuperIDM v)\d+\.\d+\.\d+( Engine started)", rf"\g<1>{version}\g<2>"),
    ]
    if build_date:
        html_rules.append(
            (r'(<div id="stAppBuildDate"[^>]*>)\d{4}\.\d{2}\.\d{2}(</div>)', rf"\g<1>{build_date}\g<2>")
        )

    targets = [
        (
            "src-tauri/Cargo.toml",
            [(r'(?m)^(version\s*=\s*")\d+\.\d+\.\d+(")$', rf"\g<1>{version}\g<2>")],
        ),
        (
            "src-tauri/tauri.conf.json",
            [(r'("version"\s*:\s*")\d+\.\d+\.\d+(")', rf"\g<1>{version}\g<2>")],
        ),
        (
            "msix_build/AppxManifest.xml",
            [(r'(?<![A-Za-z])(Version=")\d+\.\d+\.\d+\.\d+(")', rf"\g<1>{msix_ver}\g<2>")],
        ),
        (
            "extension/manifest.json",
            [(r'("version"\s*:\s*")\d+\.\d+\.\d+(")', rf"\g<1>{version}\g<2>")],
        ),
        (
            "extension/popup.html",
            [(r"(<span>SuperIDM v)\d+\.\d+\.\d+(</span>)", rf"\g<1>{version}\g<2>")],
        ),
        ("index.html", html_rules),
        ("dist/index.html", html_rules),
    ]
    return targets


def apply_or_check(check_only: bool = False, update_date: bool = False) -> list[str]:
    version = read_app_version()
    build_date = datetime.date.today().strftime("%Y.%m.%d") if update_date else None
    issues: list[str] = []

    for rel_path, rules in _sync_rules(version, build_date):
        full_path = os.path.join(BASE_DIR, rel_path)
        if not os.path.exists(full_path):
            issues.append(f"MISSING: {rel_path}")
            continue

        with open(full_path, "r", encoding="utf-8") as f:
            original = f.read()

        updated = original
        for pattern, repl in rules:
            new_text, count = re.subn(pattern, repl, updated)
            if count == 0:
                issues.append(f"PATTERN NOT FOUND in {rel_path}: {pattern}")
            updated = new_text

        if updated != original:
            if check_only:
                issues.append(f"OUT OF SYNC: {rel_path} (expected v{version})")
            else:
                with open(full_path, "w", encoding="utf-8", newline="") as f:
                    f.write(updated)
                print(f"Synced {rel_path} -> v{version}")

    return issues


def check_all_versions() -> list[str]:
    return apply_or_check(check_only=True, update_date=False)


def main() -> int:
    parser = argparse.ArgumentParser(description="Synchronize SuperIDM version across all project files.")
    parser.add_argument("--set", dest="set_ver", help="Set new MAJOR.MINOR.PATCH version in package.json and sync")
    parser.add_argument("--check", action="store_true", help="Verify all files match package.json without writing")
    parser.add_argument("--update-date", action="store_true", help="Also stamp today's build date in index.html")
    args = parser.parse_args()

    if args.set_ver:
        write_package_version(args.set_ver)
        print(f"Updated package.json -> v{args.set_ver}")

    version = read_app_version()
    issues = apply_or_check(check_only=args.check, update_date=args.update_date)
    if issues:
        for item in issues:
            print(f"[ERROR] {item}", file=sys.stderr)
        return 1

    mode = "verified" if args.check else "synchronized"
    print(f"All project files {mode} at v{version}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
