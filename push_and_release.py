import subprocess
import os
import json
import urllib.request
import urllib.error

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
    print(f"Running: {' '.join(args[:3])}...")
    res = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding='utf-8', errors='replace')
    if res.returncode != 0:
        print(f"Command stderr: {res.stderr}")
    else:
        print(f"Command stdout: {res.stdout.strip()}")
    return res

def push_repo():
    cwd = r'C:\Users\Haider Nasrat\Desktop\Desktop-SuperIDM'
    
    # Reset git index completely to remove previous commit with secrets
    run_cmd(['git', 'update-ref', '-d', 'HEAD'], cwd=cwd)
    run_cmd(['git', 'rm', '-r', '--cached', '.'], cwd=cwd)
    
    # Configure user
    run_cmd(['git', 'config', 'user.name', 'Kutha SoftWorks (pengener-crypto)'], cwd=cwd)
    run_cmd(['git', 'config', 'user.email', 'pengener@gmail.com'], cwd=cwd)
    
    # Ensure branch is main
    run_cmd(['git', 'branch', '-M', 'main'], cwd=cwd)
    
    # Set remote
    run_cmd(['git', 'remote', 'set-url', 'origin', REMOTE_URL], cwd=cwd)
    
    # Stage files (honoring .gitignore)
    run_cmd(['git', 'add', '-A'], cwd=cwd)
    
    # Commit
    run_cmd(['git', 'commit', '-m', 'feat: SuperIDM v2.4.0 by Kutha SoftWorks - High-Performance Download Manager & Extension'], cwd=cwd)
    
    # Push force
    res = run_cmd(['git', 'push', '-u', 'origin', 'main', '--force'], cwd=cwd)
    if res.returncode == 0:
        print("Successfully pushed repository to GitHub!")
        return True
    else:
        print("Failed to push repository.")
        return False

def create_release():
    print("Creating GitHub Release v2.4.0...")
    url = f'https://api.github.com/repos/{OWNER}/{REPO}/releases'
    
    # Check if release already exists
    try:
        check_req = urllib.request.Request(f'https://api.github.com/repos/{OWNER}/{REPO}/releases/tags/v2.4.0', headers=HEADERS)
        with urllib.request.urlopen(check_req) as resp:
            existing = json.loads(resp.read().decode())
            print(f"Release v2.4.0 already exists (ID: {existing['id']}), deleting it first...")
            del_req = urllib.request.Request(existing['url'], headers=HEADERS, method='DELETE')
            urllib.request.urlopen(del_req)
    except urllib.error.HTTPError as e:
        if e.code != 404:
            print("Notice checking existing release:", e)

    payload = {
        'tag_name': 'v2.4.0',
        'target_commitish': 'main',
        'name': 'SuperIDM v2.4.0 — Official Windows Release',
        'body': (
            '### SuperIDM v2.4.0 (Official Windows Release)\n'
            '**Published by Kutha SoftWorks**\n\n'
            '#### Included Binaries:\n'
            '- `SuperIDM_Setup.exe` — Windows Silent & Standard Installer (NSIS)\n'
            '- `SuperIDM.exe` — Standalone Portable Windows Executable\n\n'
            '#### Key Features:\n'
            '- Up to 32 concurrent accelerated chunk downloads\n'
            '- Universal HLS / M3U8 and MP4 stream interception\n'
            '- Zero telemetry and strict local loopback RPC (127.0.0.1)\n'
            '- Certified for Microsoft Store & Edge Add-ons'
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
        setup_exe = r'C:\Users\Haider Nasrat\Desktop\SuperIDM_Setup.exe'
        portable_exe = r'C:\Users\Haider Nasrat\Desktop\SuperIDM.exe'
        
        setup_url = upload_asset(rel, setup_exe, 'SuperIDM_Setup.exe')
        portable_url = upload_asset(rel, portable_exe, 'SuperIDM.exe')
        
        enable_pages()
        
        print("\n================== DEPLOYMENT SUMMARY ==================")
        print("Direct Setup URL (Package URL for Microsoft Store):")
        print(setup_url)
        print("Portable Executable URL:")
        print(portable_url)
        print(f"Live Privacy Policy URL: https://{OWNER}.github.io/{REPO}/privacy.html")
        print("========================================================\n")
