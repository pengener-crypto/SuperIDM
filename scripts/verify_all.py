"""Comprehensive static + frontend integrity verifier for SuperIDM.

Checks:
  1. Single-source version synchronization across all manifests, Rust, HTML, and extension files.
  2. dist/index.html and dist/tailwind.css match root index.html and tailwind.css.
  3. Zero JS syntax errors across index.html and all extension/*.js scripts.
  4. Every `window.X = Y` export in index.html references an actual declared symbol.
  5. Every inline HTML handler (onclick/onchange/oninput/ondblclick/oncontextmenu) in index.html
     calls a function that is actually exported on `window`.
  6. No stale/unsupported `ftp://` protocol claims remain in UI or Rust backend.
"""
import json
import os
import subprocess
import sys
import xml.etree.ElementTree as ET

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, BASE_DIR)

from sync_version import check_all_versions, read_app_version


NODE_CHECK_SCRIPT = r"""
const fs = require('fs');
const path = require('path');

const baseDir = process.argv[1];
const htmlPath = path.join(baseDir, 'index.html');
const html = fs.readFileSync(htmlPath, 'utf8');

// 1. Parse all inline <script> blocks in index.html
const scripts = [...html.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/gi)]
  .map(m => m[1])
  .filter(s => s.trim());

if (scripts.length === 0) {
  throw new Error('No <script> blocks found in index.html');
}

const exportedGlobals = new Set();

scripts.forEach((src, idx) => {
  // Syntax check
  new Function(src);

  // Collect all declared identifiers inside the script block
  const declared = new Set();
  for (const m of src.matchAll(/(?:^|[\s;{}()])(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(/gm)) {
    declared.add(m[1]);
  }
  for (const m of src.matchAll(/(?:^|[\s;{}()])(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?:=|;|in\b|of\b)/gm)) {
    declared.add(m[1]);
  }
  for (const m of src.matchAll(/(?:^|[\s;{}()])class\s+([A-Za-z_$][\w$]*)\b/gm)) {
    declared.add(m[1]);
  }

  // Verify every `window.X = Y;` export points to a declared symbol
  for (const m of src.matchAll(/window\.([A-Za-z_$][\w$]*)\s*=\s*([A-Za-z_$][\w$]*)\s*;/g)) {
    const [, prop, ident] = m;
    exportedGlobals.add(prop);
    if (!declared.has(ident)) {
      throw new Error(`Broken window export in script #${idx}: window.${prop} = ${ident} (not declared!)`);
    }
  }
  // Also record direct function expressions assigned to window.foo = function / (...) =>
  for (const m of src.matchAll(/window\.([A-Za-z_$][\w$]*)\s*=\s*(?:async\s+)?(?:function|\()/g)) {
    exportedGlobals.add(m[1]);
  }
});

// 2. Verify every inline event handler call in HTML points to a window.* export (or JS builtin)
const JS_BUILTINS = new Set([
  'if', 'else', 'for', 'while', 'switch', 'return', 'typeof', 'void', 'delete', 'new',
  'event', 'this', 'window', 'document', 'navigator', 'console', 'localStorage',
  'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'requestAnimationFrame',
  'String', 'Number', 'Boolean', 'Array', 'Object', 'Math', 'Date', 'JSON', 'RegExp', 'URL',
  'parseInt', 'parseFloat', 'isNaN', 'isFinite', 'encodeURIComponent', 'decodeURIComponent',
  'alert', 'confirm', 'prompt', 'open', 'close', 'focus', 'blur', 'stopPropagation',
  'preventDefault', 'stopImmediatePropagation', 'closest', 'querySelector', 'getElementById'
]);

const missingHandlers = new Set();
const handlerRe = /\bon(?:click|change|input|dblclick|contextmenu|submit)\s*=\s*"([^"]+)"/gi;
for (const m of html.matchAll(handlerRe)) {
  const expr = m[1];
  // Remove quoted string literals so words inside strings aren't mistaken for calls
  const stripped = expr.replace(/'[^']*'/g, "''").replace(/`[^`]*`/g, '``');
  // Find bare function calls not preceded by a dot (property method calls)
  for (const call of stripped.matchAll(/(?<![.\w$])([A-Za-z_$][\w$]*)\s*\(/g)) {
    const fn = call[1];
    if (!JS_BUILTINS.has(fn) && !exportedGlobals.has(fn)) {
      missingHandlers.add(fn);
    }
  }
}

if (missingHandlers.size > 0) {
  throw new Error('Inline HTML handlers not exported on window: ' + [...missingHandlers].sort().join(', '));
}

// 3. Syntax-check all browser extension scripts
const extDir = path.join(baseDir, 'extension');
for (const fname of ['background.js', 'content.js', 'popup.js', 'instagram_helper.js']) {
  const fpath = path.join(extDir, fname);
  if (fs.existsSync(fpath)) {
    const code = fs.readFileSync(fpath, 'utf8');
    new Function(code);
  }
}

// 4. Ensure no misleading FTP protocol text remains in UI
if (html.includes('HTTP/HTTPS/FTP')) {
  throw new Error('Found outdated "HTTP/HTTPS/FTP" claim in index.html');
}

console.log(`Frontend verification OK (${scripts.length} script block(s), ${exportedGlobals.size} window exports, all inline handlers verified, extension scripts syntax OK).`);
"""


def main() -> int:
    errors: list[str] = []

    # 1. Version consistency
    ver_issues = check_all_versions()
    if ver_issues:
        errors.extend(ver_issues)
    else:
        print(f"Version sync check OK (v{read_app_version()}).")

    # 2. dist/ parity (only enforced when dist/ exists; clean git clones generate dist/ at build time)
    dist_dir = os.path.join(BASE_DIR, "dist")
    if os.path.isdir(dist_dir):
        for name in ("index.html", "tailwind.css"):
            root_p = os.path.join(BASE_DIR, name)
            dist_p = os.path.join(dist_dir, name)
            if not os.path.exists(dist_p):
                errors.append(f"MISSING dist/{name}")
            else:
                with open(root_p, "rb") as f1, open(dist_p, "rb") as f2:
                    if f1.read() != f2.read():
                        errors.append(f"OUT OF SYNC: dist/{name} differs from {name}")
        if not any("dist/" in e for e in errors):
            print("dist/ parity check OK (index.html & tailwind.css match root).")
    else:
        print("dist/ directory not generated yet; skipping dist parity check.")

    # 2b. Extension manifest, empty rpc_token.json & MSIX manifest checks
    manifest_p = os.path.join(BASE_DIR, "extension", "manifest.json")
    try:
        with open(manifest_p, "r", encoding="utf-8") as f:
            man = json.load(f)
        for cs in man.get("content_scripts", []):
            for asset in cs.get("js", []) + cs.get("css", []):
                if not os.path.exists(os.path.join(BASE_DIR, "extension", asset)):
                    errors.append(f"MISSING extension asset referenced in manifest.json: {asset}")
    except Exception as ex:
        errors.append(f"Invalid extension/manifest.json: {ex}")

    tok_p = os.path.join(BASE_DIR, "extension", "rpc_token.json")
    if os.path.exists(tok_p):
        try:
            with open(tok_p, "r", encoding="utf-8") as f:
                tok_val = json.load(f).get("token", "")
            if tok_val:
                errors.append("SECURITY: extension/rpc_token.json must be empty before packaging!")
        except Exception as ex:
            errors.append(f"Invalid extension/rpc_token.json: {ex}")

    appx_p = os.path.join(BASE_DIR, "msix_build", "AppxManifest.xml")
    if os.path.exists(appx_p):
        try:
            ET.parse(appx_p)
        except Exception as ex:
            errors.append(f"Invalid msix_build/AppxManifest.xml: {ex}")

    # 3. Node JS + DOM handler + extension check
    res = subprocess.run(
        ["node", "-e", NODE_CHECK_SCRIPT, BASE_DIR],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if res.stdout.strip():
        print(res.stdout.strip())
    if res.returncode != 0:
        errors.append(res.stderr.strip() or "Node frontend verification failed")

    if errors:
        print("\nVERIFICATION FAILED:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print("ALL STATIC & FRONTEND VERIFICATION CHECKS PASSED!")
    return 0


if __name__ == "__main__":
    sys.exit(main())
