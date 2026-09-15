import subprocess
import os
import json
import urllib.request
import urllib.error
import shutil

token_path = os.path.join(os.path.dirname(__file__), '.token')
with open(token_path, 'r', encoding='utf-8') as f:
    TOKEN = f.read().strip()

OWNER = 'pengener-crypto'
REPO = 'SuperIDM'
REMOTE_URL = f'https://{TOKEN}@github.com/{OWNER}/{REPO}.git'

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
    
    # Commit
    run_cmd(['git', 'commit', '-m', 'feat: release v2.5.0 - Monotonic Progress, Instant Extension Exit Sync, Security Handshake'], cwd=cwd)
    
    # Push to origin main
    res = run_cmd(['git', 'push', 'origin', 'main'], cwd=cwd)
    if res.returncode == 0:
        print("Successfully pushed repository to GitHub!")
        return True
    else:
        print("Failed to push repository.")
        return False

def create_release():
    print("Creating GitHub Release v2.5.0...")
    url = f'https://api.github.com/repos/{OWNER}/{REPO}/releases'
    
    # Check if release already exists
    try:
        check_req = urllib.request.Request(f'https://api.github.com/repos/{OWNER}/{REPO}/releases/tags/v2.5.0', headers=HEADERS)
        with urllib.request.urlopen(check_req) as resp:
            existing = json.loads(resp.read().decode())
            print(f"Release v2.5.0 already exists (ID: {existing['id']}), deleting it first...")
            del_req = urllib.request.Request(existing['url'], headers=HEADERS, method='DELETE')
            urllib.request.urlopen(del_req)
    except urllib.error.HTTPError as e:
        if e.code != 404:
            print("Notice checking existing release:", e)

    payload = {
        'tag_name': 'v2.5.0',
        'target_commitish': 'main',
        'name': 'SuperIDM v2.5.0 — Official Windows Release',
        'body': (
            '### SuperIDM v2.5.0 (Official Windows Release)\n'
            '**Published by Kutha SoftWorks**\n\n'
            '#### What’s New in v2.5.0:\n'
            '- **Monotonic Download Progress:** Completely eliminated progress jumps and percentage regressions during segmented and multi-pass stream downloads.\n'
            '- **Instant Browser Extension Sync:** Floating video grabber badges now disappear immediately (`display: none`) when the desktop app is closed and reappear when launched.\n'
            '- **Clean Process Termination:** Improved exit and system tray behavior with complete teardown of local loopback RPC servers.\n'
            '- **Full Security Handshake:** Origin lockdown and authenticated bearer token handshakes across all extension-to-desktop IPC.\n\n'
            '#### Included Assets:\n'
            '- `SuperIDM_Setup.exe` — Windows Installer (NSIS, Certified for Microsoft Store)\n'
            '- `SuperIDM.exe` — Standalone Portable Windows Executable\n'
            '- `SuperIDM_2.5.0_x64_en-US.msi` — Windows MSI Installer Package\n'
            '- `SuperIDM_Extension_v2.5.0.zip` — Chromium Extension Package (Chrome / Edge Web Store ready)'
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
        nsis_exe = os.path.join(base_dir, r'src-tauri\target\release\bundle\nsis\SuperIDM_2.5.0_x64-setup.exe')
        portable_exe = os.path.join(base_dir, r'src-tauri\target\release\super-idm.exe')
        msi_file = os.path.join(base_dir, r'src-tauri\target\release\bundle\msi\SuperIDM_2.5.0_x64_en-US.msi')
        ext_zip = os.path.join(base_dir, 'SuperIDM_Extension_v2.5.0.zip')
        
        # Also copy to desktop for quick user access
        desktop_setup = r'C:\Users\Haider Nasrat\Desktop\SuperIDM_Setup.exe'
        desktop_portable = r'C:\Users\Haider Nasrat\Desktop\SuperIDM.exe'
        
        if os.path.exists(nsis_exe):
            shutil.copy2(nsis_exe, desktop_setup)
        if os.path.exists(portable_exe):
            shutil.copy2(portable_exe, desktop_portable)
        
        setup_url = upload_asset(rel, desktop_setup, 'SuperIDM_Setup.exe')
        portable_url = upload_asset(rel, desktop_portable, 'SuperIDM.exe')
        if os.path.exists(msi_file):
            upload_asset(rel, msi_file, 'SuperIDM_2.5.0_x64_en-US.msi')
        if os.path.exists(ext_zip):
            upload_asset(rel, ext_zip, 'SuperIDM_Extension_v2.5.0.zip')
        
        enable_pages()
        
        print("\n================== DEPLOYMENT SUMMARY ==================")
        print("Direct Setup URL (Package URL for Microsoft Store):")
        print(setup_url)
        print("Portable Executable URL:")
        print(portable_url)
        print(f"Live Privacy Policy URL: https://{OWNER}.github.io/{REPO}/privacy.html")
        print("========================================================\n")
