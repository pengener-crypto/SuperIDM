import subprocess
import os
import json
import urllib.request
import urllib.error
import shutil

def load_github_token():
    token_path = os.path.join(os.path.dirname(__file__), '.token')
    if os.path.exists(token_path):
        t = open(token_path, 'r', encoding='utf-8').read().strip()
        try:
            req = urllib.request.Request(
                'https://api.github.com/user',
                headers={'Authorization': f'token {t}', 'User-Agent': 'SuperIDM-Deployer'}
            )
            urllib.request.urlopen(req, timeout=10)
            return t
        except Exception:
            pass
    # Fallback to git's configured credential helper
    res = subprocess.run(
        ['git', 'credential', 'fill'],
        input='protocol=https\nhost=github.com\n\n',
        capture_output=True,
        text=True,
        encoding='utf-8'
    )
    for line in res.stdout.splitlines():
        if line.startswith('password='):
            return line.split('=', 1)[1].strip()
    return ''

TOKEN = load_github_token()

OWNER = 'pengener-crypto'
REPO = 'SuperIDM'
REMOTE_URL = f'https://github.com/{OWNER}/{REPO}.git'

HEADERS = {
    'Authorization': f'token {TOKEN}',
    'Accept': 'application/vnd.github.v3+json',
    'User-Agent': 'SuperIDM-Deployer'
}

def run_cmd(args, cwd=None):
    print(f"Running: {' '.join(args[:4])}...")
    res = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding='utf-8', errors='replace')
    if res.returncode != 0:
        print(f"Command stderr: {res.stderr}")
    else:
        print(f"Command stdout: {res.stdout.strip()}")
    return res

def push_repo():
    cwd = r'C:\Users\Haider Nasrat\Desktop\Desktop-SuperIDM'
    
    # Configure user
    run_cmd(['git', 'config', 'user.name', 'Kutha SoftWorks (pengener-crypto)'], cwd=cwd)
    run_cmd(['git', 'config', 'user.email', 'pengener@gmail.com'], cwd=cwd)
    
    # Ensure remote URL is current
    run_cmd(['git', 'remote', 'set-url', 'origin', REMOTE_URL], cwd=cwd)
    
    # Stage files (honoring .gitignore)
    run_cmd(['git', 'add', '-A'], cwd=cwd)
    
    # Stage files (honoring .gitignore)
    run_cmd(['git', 'add', '-A'], cwd=cwd)
    
    # Commit
    run_cmd(['git', 'commit', '-m', 'feat: release v2.7.6 - Official Mozilla Firefox Store Integration & Stream Capture Enhancements'], cwd=cwd)
    
    # Push to origin main (pass if already up-to-date)
    res = run_cmd(['git', 'push', 'origin', 'main'], cwd=cwd)
    if res.returncode == 0:
        print("Successfully pushed repository to GitHub!")
        return True
    else:
        print("Failed to push repository.")
        return False

def create_release():
    print("Creating GitHub Release v2.7.6...")
    url = f'https://api.github.com/repos/{OWNER}/{REPO}/releases'
    
    # Check if release already exists
    try:
        check_req = urllib.request.Request(f'https://api.github.com/repos/{OWNER}/{REPO}/releases/tags/v2.7.6', headers=HEADERS)
        with urllib.request.urlopen(check_req) as resp:
            existing = json.loads(resp.read().decode())
            print(f"Release v2.7.6 already exists (ID: {existing['id']}), deleting it first...")
            del_req = urllib.request.Request(existing['url'], headers=HEADERS, method='DELETE')
            urllib.request.urlopen(del_req)
    except urllib.error.HTTPError as e:
        if e.code != 404:
            print("Notice checking existing release:", e)

    payload = {
        'tag_name': 'v2.7.6',
        'target_commitish': 'main',
        'name': 'SuperIDM v2.7.6 — Official Windows Release',
        'body': (
            '### SuperIDM v2.7.6 (Official Windows Release)\n'
            '**Published by Kutha SoftWorks**\n\n'
            '#### What’s New in v2.7.6:\n'
            '- **Official Mozilla Firefox Add-ons (AMO) Store Integration:** Direct 1-click integration with the official Firefox Add-ons Store (`https://addons.mozilla.org/en-US/firefox/addon/superidm-download-accelerator/`) alongside Chrome Web Store and Microsoft Edge Add-ons.\n'
            '- **Precision Stream Sniffer & Protected Media Headers:** Filters out HLS/DASH segment noise (`.ts`, `.m4s`, `chunk-*`), prioritizes master manifests (`.m3u8`/`.mpd`), and forwards `Referer`/`Origin`/`Cookie` headers to `yt-dlp` for protected streams.\n'
            '- **Zero First-Run Setup & 100% Offline Engine Bundling:** Bundled `yt-dlp 2026.10.26`, `QuickJS-ng v0.18.0`, and `FFmpeg 6.1.1 Essentials` directly inside `SuperIDM_Setup.exe` and `SuperIDM_2.7.6_x64.msix`.\n'
            '- **MV3 & Gecko Browser Extension Auto-Pairing (v2.7.6):** Automatic connection between Chrome, Edge, Brave, Opera, and Firefox extensions and the desktop engine.\n\n'
            '#### Included Assets:\n'
            '- `SuperIDM_Setup.exe` — Windows Installer (NSIS, Certified for Microsoft Store)\n'
            '- `SuperIDM.exe` — Standalone Portable Windows Executable\n'
            '- `SuperIDM_2.7.6_x64.msix` — Windows MSIX Modern App Package (Microsoft Store Ready, Bundled Extractor Core)\n'
            '- `SuperIDM_Chrome_Store_v2.7.6.zip` — Chrome Web Store & Edge Add-ons Package (Auto-Pairing Enabled)\n'
            '- `SuperIDM_Firefox_v2.7.6.zip` — Mozilla Firefox (AMO) WebExtension Package (Gecko & Auto-Pairing Enabled)\n'
            '- `SuperIDM_Extension_v2.7.6.zip` — Chromium Extension Package'
        ),
        'draft': False,
        'prerelease': False
    }

    req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers=HEADERS, method='POST')
    with urllib.request.urlopen(req) as resp:
        release = json.loads(resp.read().decode())
        print(f"Created release: {release['name']} (ID: {release['id']})")
        return release

def upload_asset(release, file_path, name):
    if not os.path.exists(file_path):
        print(f"Warning: File not found: {file_path}")
        return None
        
    print(f"Uploading {name} ({os.path.getsize(file_path)} bytes)...")
    upload_url_template = release['upload_url'].split('{')[0]
    upload_url = f"{upload_url_template}?name={name}"

    with open(file_path, 'rb') as f:
        file_data = f.read()

    headers = {
        'Authorization': f'token {TOKEN}',
        'Content-Type': 'application/octet-stream',
        'User-Agent': 'SuperIDM-Deployer',
        'Content-Length': str(len(file_data))
    }

    req = urllib.request.Request(upload_url, data=file_data, headers=headers, method='POST')
    with urllib.request.urlopen(req) as resp:
        asset = json.loads(resp.read().decode())
        print(f"Uploaded {name} successfully! Download URL: {asset['browser_download_url']}")
        return asset['browser_download_url']

def enable_pages():
    print("Checking GitHub Pages...")
    url = f'https://api.github.com/repos/{OWNER}/{REPO}/pages'
    payload = {
        'source': {
            'branch': 'main',
            'path': '/'
        }
    }
    try:
        req = urllib.request.Request(url, data=json.dumps(payload).encode('utf-8'), headers=HEADERS, method='POST')
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read().decode())
            print(f"GitHub Pages enabled at: {data.get('html_url')}")
    except urllib.error.HTTPError as e:
        print("Pages status:", e.code)

if __name__ == '__main__':
    if push_repo():
        rel = create_release()
        
        base_dir = r'C:\Users\Haider Nasrat\Desktop\Desktop-SuperIDM'
        nsis_exe = os.path.join(base_dir, r'src-tauri\target\release\bundle\nsis\SuperIDM_2.7.6_x64-setup.exe')
        portable_exe = os.path.join(base_dir, r'src-tauri\target\release\super-idm.exe')
        msix_file = os.path.join(base_dir, 'SuperIDM_2.7.6_x64.msix')
        ext_zip = os.path.join(base_dir, 'SuperIDM_Extension_v2.7.6.zip')
        
        # Also copy to desktop for quick user access
        desktop_setup = r'C:\Users\Haider Nasrat\Desktop\SuperIDM_Setup.exe'
        desktop_portable = r'C:\Users\Haider Nasrat\Desktop\SuperIDM.exe'
        desktop_msix = r'C:\Users\Haider Nasrat\Desktop\SuperIDM_2.7.6_x64.msix'
        
        if os.path.exists(nsis_exe):
            try:
                shutil.copy2(nsis_exe, desktop_setup)
            except PermissionError:
                print("Notice: Desktop SuperIDM_Setup.exe is in use; uploading from build target.")
        if os.path.exists(portable_exe):
            try:
                shutil.copy2(portable_exe, desktop_portable)
            except PermissionError:
                print("Notice: Desktop SuperIDM.exe is currently running; uploading from build target.")
        if os.path.exists(msix_file):
            try:
                shutil.copy2(msix_file, desktop_msix)
            except PermissionError:
                pass
        
        setup_url = upload_asset(rel, nsis_exe if os.path.exists(nsis_exe) else desktop_setup, 'SuperIDM_Setup.exe')
        portable_url = upload_asset(rel, portable_exe if os.path.exists(portable_exe) else desktop_portable, 'SuperIDM.exe')
        if os.path.exists(msix_file):
            upload_asset(rel, msix_file, 'SuperIDM_2.7.6_x64.msix')
        if os.path.exists(ext_zip):
            upload_asset(rel, ext_zip, 'SuperIDM_Extension_v2.7.6.zip')
        chrome_store_zip = os.path.join(base_dir, 'SuperIDM_Chrome_Store_v2.7.6.zip')
        if os.path.exists(chrome_store_zip):
            upload_asset(rel, chrome_store_zip, 'SuperIDM_Chrome_Store_v2.7.6.zip')
        firefox_zip = os.path.join(base_dir, 'SuperIDM_Firefox_v2.7.6.zip')
        if os.path.exists(firefox_zip):
            upload_asset(rel, firefox_zip, 'SuperIDM_Firefox_v2.7.6.zip')
        
        enable_pages()
        
        print("\n================== DEPLOYMENT SUMMARY ==================")
        print("Direct Setup URL (Package URL for Microsoft Store):")
        print(setup_url)
        print("Portable Executable URL:")
        print(portable_url)
        print(f"Live Privacy Policy URL: https://{OWNER}.github.io/{REPO}/privacy.html")
        print("========================================================\n")
