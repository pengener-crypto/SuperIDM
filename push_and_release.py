"""Commit, push, and non-destructively create/update the GitHub Release for SuperIDM.

Reads the canonical version from package.json, verifies every file is in sync,
updates existing GitHub Releases in place via PATCH (preserving release ID &
download counters instead of deleting and recreating), and replaces only matching
assets before uploading fresh builds.
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

from sync_version import check_all_versions, read_app_version

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
OWNER = "pengener-crypto"
REPO = "SuperIDM"
REMOTE_URL = f"https://github.com/{OWNER}/{REPO}.git"


def load_github_token() -> str:
    token_path = os.path.join(BASE_DIR, ".token")
    if os.path.exists(token_path):
        t = open(token_path, "r", encoding="utf-8").read().strip()
        try:
            req = urllib.request.Request(
                "https://api.github.com/user",
                headers={"Authorization": f"token {t}", "User-Agent": "SuperIDM-Deployer"},
            )
            urllib.request.urlopen(req, timeout=10)
            return t
        except Exception:
            pass
    res = subprocess.run(
        ["git", "credential", "fill"],
        input="protocol=https\nhost=github.com\n\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    for line in res.stdout.splitlines():
        if line.startswith("password="):
            return line.split("=", 1)[1].strip()
    return ""


def make_headers(token: str) -> dict[str, str]:
    return {
        "Authorization": f"token {token}",
        "Accept": "application/vnd.github.v3+json",
        "User-Agent": "SuperIDM-Deployer",
    }


def run_cmd(args, cwd=None):
    print(f"Running: {' '.join(args[:4])}...")
    res = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if res.returncode != 0:
        if res.stderr.strip():
            print(f"Command stderr: {res.stderr.strip()}")
    elif res.stdout.strip():
        print(f"Command stdout: {res.stdout.strip()}")
    return res


def push_repo(version: str, commit_message: str | None) -> bool:
    run_cmd(["git", "config", "user.name", "Kutha SoftWorks (pengener-crypto)"], cwd=BASE_DIR)
    run_cmd(["git", "config", "user.email", "pengener@gmail.com"], cwd=BASE_DIR)
    run_cmd(["git", "remote", "set-url", "origin", REMOTE_URL], cwd=BASE_DIR)
    run_cmd(["git", "add", "-A"], cwd=BASE_DIR)

    status = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=BASE_DIR,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    changed_lines = [line for line in status.stdout.splitlines() if line.strip()]
    if changed_lines:
        if not commit_message:
            file_count = len(changed_lines)
            commit_message = f"chore: release v{version} ({file_count} files updated)"
        commit_res = run_cmd(["git", "commit", "-m", commit_message], cwd=BASE_DIR)
        if commit_res.returncode != 0:
            print("Failed to commit staged changes.")
            return False
    else:
        print("Working tree clean; skipping git commit.")

    res = run_cmd(["git", "push", "origin", "main"], cwd=BASE_DIR)
    if res.returncode == 0:
        print("Successfully pushed repository to GitHub!")
        return True
    print("Failed to push repository.")
    return False


def build_release_body(version: str) -> str:
    return (
        f"### SuperIDM v{version} (Official Windows Release)\n"
        "**Published by Kutha SoftWorks**\n\n"
        f"#### What's New in v{version}:\n"
        "- **Official Mozilla Firefox Add-ons (AMO) Store Integration:** Direct 1-click integration with the official Firefox Add-ons Store (`https://addons.mozilla.org/en-US/firefox/addon/superidm-download-accelerator/`) alongside Chrome Web Store and Microsoft Edge Add-ons.\n"
        "- **Precision Stream Sniffer & Protected Media Headers:** Filters out HLS/DASH segment noise (`.ts`, `.m4s`, `chunk-*`), prioritizes master manifests (`.m3u8`/`.mpd`), and forwards `Referer`/`Origin`/`Cookie` headers to `yt-dlp` for protected streams.\n"
        "- **Safe Post-Download PC Shutdown:** Real OS shutdown scheduling with a 60-second safety countdown dialog and instant cancel (`shutdown /a`).\n"
        "- **Hardened Multi-Chunk Resume & Engine Integrity:** Verifies remote ETag/Last-Modified consistency before resuming direct downloads, fast-fails expired 401/403/410 tokens via `RecoveryEngine`, and uses unified `SessionConfig` anti-hotlink headers.\n"
        f"- **Zero First-Run Setup & 100% Offline Engine Bundling:** Bundled `yt-dlp`, `QuickJS-ng`, and `FFmpeg` directly inside `SuperIDM_Setup.exe` and `SuperIDM_{version}_x64.msix`.\n\n"
        "#### Included Assets:\n"
        "- `SuperIDM_Setup.exe` — Windows Installer (NSIS, Certified for Microsoft Store)\n"
        "- `SuperIDM.exe` — Standalone Portable Windows Executable\n"
        f"- `SuperIDM_{version}_x64.msix` — Windows MSIX Modern App Package (Microsoft Store Ready, Bundled Extractor Core)\n"
        f"- `SuperIDM_Chrome_Store_v{version}.zip` — Chrome Web Store & Edge Add-ons Package (Auto-Pairing Enabled)\n"
        f"- `SuperIDM_Firefox_v{version}.zip` — Mozilla Firefox (AMO) WebExtension Package (Gecko & Auto-Pairing Enabled)\n"
        f"- `SuperIDM_Extension_v{version}.zip` — Chromium Extension Package"
    )


def get_or_update_release(headers: dict[str, str], version: str):
    tag = f"v{version}"
    payload = {
        "tag_name": tag,
        "target_commitish": "main",
        "name": f"SuperIDM {tag} — Official Windows Release",
        "body": build_release_body(version),
        "draft": False,
        "prerelease": False,
    }

    # Preserve existing release ID and download stats by updating via PATCH when tag already exists.
    try:
        check_req = urllib.request.Request(
            f"https://api.github.com/repos/{OWNER}/{REPO}/releases/tags/{tag}",
            headers=headers,
        )
        with urllib.request.urlopen(check_req) as resp:
            existing = json.loads(resp.read().decode())
            print(f"Updating existing GitHub Release {tag} in place (ID: {existing['id']})...")
            patch_req = urllib.request.Request(
                existing["url"],
                data=json.dumps({
                    "name": payload["name"],
                    "body": payload["body"],
                    "draft": False,
                    "prerelease": False,
                }).encode("utf-8"),
                headers=headers,
                method="PATCH",
            )
            with urllib.request.urlopen(patch_req) as patch_resp:
                updated = json.loads(patch_resp.read().decode())
                return updated, existing.get("assets", [])
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise

    print(f"Creating new GitHub Release {tag}...")
    create_req = urllib.request.Request(
        f"https://api.github.com/repos/{OWNER}/{REPO}/releases",
        data=json.dumps(payload).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    with urllib.request.urlopen(create_req) as resp:
        release = json.loads(resp.read().decode())
        print(f"Created release: {release['name']} (ID: {release['id']})")
        return release, []


def upload_asset(token: str, headers: dict[str, str], release, existing_assets, file_path: str, name: str):
    if not os.path.exists(file_path):
        print(f"Warning: File not found: {file_path}")
        return None

    # Replace only the matching asset, leaving the release object & other assets untouched.
    for old in existing_assets:
        if old.get("name") == name and old.get("url"):
            print(f"Replacing existing asset {name} (ID: {old.get('id')})...")
            del_req = urllib.request.Request(old["url"], headers=headers, method="DELETE")
            urllib.request.urlopen(del_req)
            break

    print(f"Uploading {name} ({os.path.getsize(file_path)} bytes)...")
    upload_url_template = release["upload_url"].split("{")[0]
    upload_url = f"{upload_url_template}?name={name}"

    with open(file_path, "rb") as f:
        file_data = f.read()

    up_headers = {
        "Authorization": f"token {token}",
        "Content-Type": "application/octet-stream",
        "User-Agent": "SuperIDM-Deployer",
        "Content-Length": str(len(file_data)),
    }

    req = urllib.request.Request(upload_url, data=file_data, headers=up_headers, method="POST")
    with urllib.request.urlopen(req) as resp:
        asset = json.loads(resp.read().decode())
        print(f"Uploaded {name} successfully! Download URL: {asset['browser_download_url']}")
        return asset["browser_download_url"]


def cleanup_stale_desktop_backups(desktop_dir: str) -> None:
    for stale in glob.glob(os.path.join(desktop_dir, "SuperIDM.old.*.exe")):
        try:
            os.remove(stale)
            print(f"Cleaned up stale desktop backup: {os.path.basename(stale)}")
        except OSError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser(description="Push and publish SuperIDM GitHub Release.")
    parser.add_argument("-m", "--message", dest="message", help="Custom git commit message")
    parser.add_argument("--skip-push", action="store_true", help="Skip git commit/push and only update release assets")
    args = parser.parse_args()

    issues = check_all_versions()
    if issues:
        print("Version sync check failed! Run `python sync_version.py` before releasing:", file=sys.stderr)
        for item in issues:
            print(f"  - {item}", file=sys.stderr)
        return 1

    version = read_app_version()
    token = load_github_token()
    if not token:
        print("Error: Could not load GitHub token from .token or git credential helper.", file=sys.stderr)
        return 1
    headers = make_headers(token)

    if not args.skip_push:
        if not push_repo(version, args.message):
            return 1

    rel, existing_assets = get_or_update_release(headers, version)

    desktop_dir = r"C:\Users\Haider Nasrat\Desktop"
    cleanup_stale_desktop_backups(desktop_dir)

    nsis_exe = os.path.join(BASE_DIR, rf"src-tauri\target\release\bundle\nsis\SuperIDM_{version}_x64-setup.exe")
    portable_exe = os.path.join(BASE_DIR, r"src-tauri\target\release\super-idm.exe")
    msix_file = os.path.join(BASE_DIR, f"SuperIDM_{version}_x64.msix")
    ext_zip = os.path.join(BASE_DIR, f"SuperIDM_Extension_v{version}.zip")
    chrome_store_zip = os.path.join(BASE_DIR, f"SuperIDM_Chrome_Store_v{version}.zip")
    firefox_zip = os.path.join(BASE_DIR, f"SuperIDM_Firefox_v{version}.zip")

    desktop_setup = os.path.join(desktop_dir, "SuperIDM_Setup.exe")
    desktop_portable = os.path.join(desktop_dir, "SuperIDM.exe")
    desktop_msix = os.path.join(desktop_dir, f"SuperIDM_{version}_x64.msix")

    for src, dst in [(nsis_exe, desktop_setup), (portable_exe, desktop_portable), (msix_file, desktop_msix)]:
        if os.path.exists(src):
            try:
                shutil.copy2(src, dst)
            except PermissionError:
                print(f"Notice: {os.path.basename(dst)} is currently in use; uploading from build target.")

    setup_url = upload_asset(token, headers, rel, existing_assets, nsis_exe if os.path.exists(nsis_exe) else desktop_setup, "SuperIDM_Setup.exe")
    portable_url = upload_asset(token, headers, rel, existing_assets, portable_exe if os.path.exists(portable_exe) else desktop_portable, "SuperIDM.exe")
    if os.path.exists(msix_file):
        upload_asset(token, headers, rel, existing_assets, msix_file, f"SuperIDM_{version}_x64.msix")
    if os.path.exists(ext_zip):
        upload_asset(token, headers, rel, existing_assets, ext_zip, f"SuperIDM_Extension_v{version}.zip")
    if os.path.exists(chrome_store_zip):
        upload_asset(token, headers, rel, existing_assets, chrome_store_zip, f"SuperIDM_Chrome_Store_v{version}.zip")
    if os.path.exists(firefox_zip):
        upload_asset(token, headers, rel, existing_assets, firefox_zip, f"SuperIDM_Firefox_v{version}.zip")

    print("\n================== DEPLOYMENT SUMMARY ==================")
    print(f"Release Version: v{version} (Release ID: {rel.get('id')})")
    print("Direct Setup URL (Package URL for Microsoft Store):")
    print(setup_url)
    print("Portable Executable URL:")
    print(portable_url)
    print(f"Live Privacy Policy URL: https://{OWNER}.github.io/{REPO}/privacy.html")
    print("========================================================\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
