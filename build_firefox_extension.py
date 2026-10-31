import os
import zipfile
import json
import shutil

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_EXT = os.path.join(BASE_DIR, 'extension')
DIST_FIREFOX_ZIP = os.path.join(BASE_DIR, 'SuperIDM_Firefox_v2.7.2.zip')
DESKTOP_FIREFOX_ZIP = os.path.join(r'C:\Users\Haider Nasrat\Desktop', 'SuperIDM_Firefox_v2.7.2.zip')

def build_firefox_package():
    print("Building SuperIDM for Mozilla Firefox (AMO Ready)...")
    
    # Read base manifest
    with open(os.path.join(SRC_EXT, 'manifest.json'), 'r', encoding='utf-8') as f:
        manifest = json.load(f)
        
    # Apply Firefox specific requirements
    manifest['browser_specific_settings'] = {
        'gecko': {
            'id': 'superidm-extension@kutha.com',
            'strict_min_version': '140.0',
            'data_collection_permissions': {
                'required': ['none']
            }
        }
    }
    
    # Firefox MV3 uses background.scripts instead of service_worker
    manifest['background'] = {
        'scripts': ['background.js']
    }
    
    # For content_scripts, ensure world: MAIN is removed for broader Firefox compatibility (<128)
    for cs in manifest.get('content_scripts', []):
        if 'world' in cs:
            del cs['world']

    # Files to include
    include_files = [
        'background.js',
        'content.js',
        'instagram_helper.js',
        'overlay.css',
        'popup.html',
        'popup.css',
        'popup.js',
        'rpc_token.json'
    ]

    with zipfile.ZipFile(DIST_FIREFOX_ZIP, 'w', zipfile.ZIP_DEFLATED) as z:
        # Write tailored manifest
        z.writestr('manifest.json', json.dumps(manifest, indent=2, ensure_ascii=False))
        
        # Write extension files
        for fname in include_files:
            fpath = os.path.join(SRC_EXT, fname)
            if os.path.exists(fpath):
                z.write(fpath, fname)
                
        # Write icons
        icons_dir = os.path.join(SRC_EXT, 'icons')
        if os.path.exists(icons_dir):
            for icon_name in os.listdir(icons_dir):
                icon_path = os.path.join(icons_dir, icon_name)
                if os.path.isfile(icon_path):
                    z.write(icon_path, f'icons/{icon_name}')
                    
    print(f"Successfully created Firefox package: {DIST_FIREFOX_ZIP} ({os.path.getsize(DIST_FIREFOX_ZIP)} bytes)")
    
    # Copy to Desktop
    shutil.copy2(DIST_FIREFOX_ZIP, DESKTOP_FIREFOX_ZIP)
    print(f"Copied to Desktop: {DESKTOP_FIREFOX_ZIP}")

if __name__ == '__main__':
    build_firefox_package()
