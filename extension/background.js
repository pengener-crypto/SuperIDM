// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Background Service Worker
// ═══════════════════════════════════════════════════════════════

const RPC_WS_URL = 'ws://127.0.0.1:6800';
const RPC_HTTP_URL = 'http://127.0.0.1:6801';
const RECONNECT_INTERVAL = 3000;

const DEFAULT_SETTINGS = {
  interceptClicks: true,
  interceptDownloads: true,
  showVideoGrabber: true,
  sniffMedia: true,
  minFileSizeMB: 1,
  fileExtensions: '3GP 7Z AAC ACE AIF ARJ ASF AVI BIN BZ2 DEB DMG DOCX EXE FLAC FLV GZ ISO LZH M4A M4V MKV MOV MP3 MP4 MPG MSI OGG PDF PPTX RAR RPM TAR TORRENT WAV WMA WMV XAPK XLSX ZIP',
  excludedDomains: ''
};

// ───── State ─────
let ws = null;
let wsConnected = false;
let rpcToken = '';
let settings = Object.assign({}, DEFAULT_SETTINGS);
let lastDesktopRulesTs = 0;
let lastRulesSyncAt = 0;
// Per-tab sniffed media
const tabMedia = {};
// Temporarily bypassed URLs (Alt+Click in content.js)
const bypassedUrls = new Map();
const BYPASS_WINDOW_MS = 15000;
let tabMediaSaveTimer = null;

function normalizeBypassUrl(raw) {
  if (!raw || typeof raw !== 'string') return '';
  return raw.trim().split('#')[0];
}

function markUrlBypassed(raw) {
  const key = normalizeBypassUrl(raw);
  if (!key) return;
  const now = Date.now();
  bypassedUrls.set(key, now);
  if (bypassedUrls.size > 100) {
    for (const [k, t] of bypassedUrls) {
      if (now - t > BYPASS_WINDOW_MS) bypassedUrls.delete(k);
    }
  }
}

function isUrlBypassed(raw) {
  const key = normalizeBypassUrl(raw);
  if (!key) return false;
  const ts = bypassedUrls.get(key);
  if (!ts) return false;
  if (Date.now() - ts > BYPASS_WINDOW_MS) {
    bypassedUrls.delete(key);
    return false;
  }
  return true;
}

function persistTabMedia() {
  if (!chrome.storage || !chrome.storage.session) return;
  if (tabMediaSaveTimer) clearTimeout(tabMediaSaveTimer);
  tabMediaSaveTimer = setTimeout(() => {
    tabMediaSaveTimer = null;
    try {
      chrome.storage.session.set({ superidm_tab_media: tabMedia }).catch(() => {});
    } catch (e) { /* ignore */ }
  }, 350);
}

// ───── Helpers ─────
function isDomainExcluded(url) {
  if (!settings.excludedDomains || !url) return false;
  try {
    const host = new URL(url).hostname.toLowerCase();
    const domains = settings.excludedDomains
      .split(/[\s,;\n]+/)
      .map(d => d.trim().toLowerCase())
      .filter(Boolean);
    return domains.some(d => host === d || host.endsWith('.' + d));
  } catch (e) {
    return false;
  }
}

function extractExtFromStr(str) {
  if (!str || typeof str !== 'string') return '';
  const clean = str.toLowerCase().split(/[?#]/)[0];
  const lastSeg = clean.substring(clean.lastIndexOf('/') + 1);
  const dotIdx = lastSeg.lastIndexOf('.');
  if (dotIdx <= 0 || dotIdx === lastSeg.length - 1) return '';
  const ext = lastSeg.substring(dotIdx + 1).toUpperCase();
  return /^[A-Z0-9]{1,10}$/.test(ext) ? ext : '';
}

function isExtensionSupported(url, filename) {
  if (!settings.fileExtensions) return true;
  const allowed = settings.fileExtensions
    .split(/[\s,;]+/)
    .map(e => e.trim().replace(/^\./, '').toUpperCase())
    .filter(Boolean);
  if (allowed.length === 0) return false;
  // Check filename extension first; fall back to URL path if filename lacks an extension
  const fileExt = extractExtFromStr(filename);
  if (fileExt) return allowed.includes(fileExt);
  const urlExt = extractExtFromStr(url);
  return urlExt ? allowed.includes(urlExt) : false;
}

const NON_DOWNLOAD_EXTS = new Set([
  'HTML', 'HTM', 'XHTML', 'PHP', 'ASP', 'ASPX', 'JSP', 'CSS', 'JS', 'MJS',
  'JSON', 'XML', 'SVG', 'PNG', 'JPG', 'JPEG', 'GIF', 'WEBP', 'ICO', 'BMP', 'TXT'
]);

function shouldInterceptBrowserDownload(item) {
  if (!settings.interceptDownloads) return false;
  if (!item || !item.url) return false;
  const lower = item.url.toLowerCase();
  if (lower.startsWith('blob:') || lower.startsWith('data:') ||
      lower.startsWith('chrome-extension://') || lower.startsWith('edge://') ||
      lower.startsWith('moz-extension://') || lower.startsWith('about:')) {
    return false;
  }
  if (isUrlBypassed(item.url) || (item.finalUrl && isUrlBypassed(item.finalUrl))) {
    return false;
  }
  if (isDomainExcluded(item.url) || (item.finalUrl && isDomainExcluded(item.finalUrl)) ||
      (item.referrer && isDomainExcluded(item.referrer))) {
    return false;
  }

  const hasMatchingExt = isExtensionSupported(item.url, item.filename);
  const minSizeMb = (typeof settings.minFileSizeMB === 'number' && settings.minFileSizeMB >= 0) ? settings.minFileSizeMB : 1;
  const minBytes = minSizeMb * 1024 * 1024;
  const totalBytes = typeof item.totalBytes === 'number' ? item.totalBytes : -1;

  // Always enforce minFileSizeMB when file size is known and > 0
  if (minBytes > 0 && totalBytes > 0 && totalBytes < minBytes) {
    return false;
  }

  if (hasMatchingExt) {
    return true;
  }

  // For extensions NOT in the user's fileExtensions list:
  // skip common web/image assets, and require known size >= minBytes (unless minFileSizeMB === 0).
  const detectedExt = extractExtFromStr(item.filename) || extractExtFromStr(item.url);
  if (detectedExt && NON_DOWNLOAD_EXTS.has(detectedExt)) {
    return false;
  }
  if (minBytes === 0) return true;
  return totalBytes >= minBytes;
}

// ───── Token & Settings Persistence ─────
async function initAuthAndSettings() {
  const data = await chrome.storage.local.get(['superidm_settings', 'superidm_rpc_token', 'superidm_desktop_rules_ts']);
  if (data.superidm_settings) {
    settings = Object.assign({}, DEFAULT_SETTINGS, data.superidm_settings);
  }
  if (typeof data.superidm_desktop_rules_ts === 'number') {
    lastDesktopRulesTs = data.superidm_desktop_rules_ts;
  }

  if (data.superidm_rpc_token) {
    rpcToken = data.superidm_rpc_token;
  } else {
    try {
      const resp = await fetch(chrome.runtime.getURL('rpc_token.json'));
      if (resp.ok) {
        const json = await resp.json();
        if (json && json.token) {
          rpcToken = json.token;
          await chrome.storage.local.set({ superidm_rpc_token: rpcToken });
        }
      }
    } catch (e) { /* ignore */ }
  }

  if (chrome.storage && chrome.storage.session) {
    try {
      const sess = await chrome.storage.session.get(['superidm_tab_media']);
      if (sess && sess.superidm_tab_media && typeof sess.superidm_tab_media === 'object') {
        Object.assign(tabMedia, sess.superidm_tab_media);
      }
    } catch (e) { /* ignore */ }
  }
}
const authInitPromise = initAuthAndSettings();

function saveSettings() {
  chrome.storage.local.set({
    superidm_settings: settings,
    superidm_rpc_token: rpcToken,
    superidm_desktop_rules_ts: lastDesktopRulesTs
  });
}

function broadcastSettingsUpdate() {
  // Notify popup (if open)
  chrome.runtime.sendMessage({
    type: 'settings_update',
    settings: settings
  }).catch(() => {});

  // Notify all content scripts
  chrome.tabs.query({}, (tabs) => {
    if (!tabs) return;
    tabs.forEach(tab => {
      if (tab.id > 0) {
        chrome.tabs.sendMessage(tab.id, {
          type: 'settings_update',
          settings: settings,
          showVideoGrabber: settings.showVideoGrabber
        }).catch(() => {});
      }
    });
  });
}

async function pushRulesToDesktop() {
  try { await authInitPromise; } catch (e) {}
  if (!rpcToken) await pairWithApp();
  if (!rpcToken) return false;

  const postRules = () => fetchWithTimeout(RPC_HTTP_URL + '/api/rules', {
    method: 'POST',
    cache: 'no-store',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${rpcToken}`,
      'X-SuperIDM-Token': rpcToken
    },
    body: JSON.stringify({
      file_extensions: settings.fileExtensions || '',
      excluded_domains: settings.excludedDomains || '',
      updated_at: lastDesktopRulesTs
    })
  }, 2500);

  try {
    let res = await postRules();
    if (res.status === 401) {
      paired = false;
      if (await pairWithApp()) {
        res = await postRules();
      }
    }
    return Boolean(res && res.ok);
  } catch (e) {
    return false;
  }
}

async function syncDesktopBrowserRules(force = false) {
  if (!rpcToken) return;
  const now = Date.now();
  if (!force && (now - lastRulesSyncAt) < 4000) return;
  lastRulesSyncAt = now;
  try {
    const res = await fetchWithTimeout(RPC_HTTP_URL + '/api/rules', {
      method: 'GET',
      cache: 'no-store',
      headers: { 'X-SuperIDM-Token': rpcToken }
    }, 2000);
    if (!res.ok) return;
    const json = await res.json();
    const remoteTs = Number(json && json.updated_at ? json.updated_at : 0);
    if (remoteTs > 0 && remoteTs > lastDesktopRulesTs) {
      lastDesktopRulesTs = remoteTs;
      if (typeof json.file_extensions === 'string' && json.file_extensions.trim()) {
        settings.fileExtensions = json.file_extensions.trim();
      }
      if (typeof json.excluded_domains === 'string') {
        settings.excludedDomains = json.excluded_domains.trim();
      }
      saveSettings();
      broadcastSettingsUpdate();
    }
  } catch (e) { /* desktop app offline or older version without /api/rules */ }
}

// ═══════ Automatic Pairing ═══════
// Each SuperIDM install generates its own secret token, so a store-installed extension
// cannot ship with it. The desktop app hands the token only to browser-extension origins.
let paired = false;
let pairingPromise = null;

async function fetchWithTimeout(url, options = {}, timeoutMs = 2500) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, Object.assign({}, options, { signal: controller.signal }));
  } finally {
    clearTimeout(timer);
  }
}

async function pairWithApp() {
  if (pairingPromise) return pairingPromise;
  pairingPromise = (async () => {
    try {
      // POST always carries the extension Origin header; Chrome omits Origin on simple GETs
      // from a service worker with host_permissions, which older desktop builds rejected.
      let res = null;
      try {
        res = await fetchWithTimeout(RPC_HTTP_URL + '/api/pair', {
          method: 'POST',
          cache: 'no-store',
          headers: { 'Content-Type': 'application/json' },
          body: '{}'
        }, 2500);
      } catch (e) { res = null; }
      if (!res || !res.ok) {
        res = await fetchWithTimeout(RPC_HTTP_URL + '/api/pair', { method: 'GET', cache: 'no-store' }, 2500);
      }
      if (!res.ok) return false;
      const json = await res.json();
      if (json && typeof json.token === 'string' && json.token) {
        if (json.token !== rpcToken) {
          rpcToken = json.token;
          await chrome.storage.local.set({ superidm_rpc_token: rpcToken });
          // Re-authenticate any open WebSocket with the fresh token
          if (ws) { try { ws.close(); } catch (e) {} }
        }
        paired = true;
        return true;
      }
    } catch (e) { /* app offline or older version without pairing */ }
    return false;
  })();
  try {
    return await pairingPromise;
  } finally {
    pairingPromise = null;
  }
}

// ═══════ WebSocket Connection ═══════
async function isServerOnline() {
  try {
    const res = await fetchWithTimeout(RPC_HTTP_URL + '/api/ping', {
      method: 'GET',
      cache: 'no-store'
    }, 2000);
    return res.ok;
  } catch (e) {
    return false;
  }
}

async function connectWS() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

  // Silent pre-check to prevent Chrome from logging ERR_CONNECTION_REFUSED when desktop app is closed
  const online = await isServerOnline();
  if (!online) {
    wsConnected = false;
    broadcastStatus();
    setTimeout(connectWS, RECONNECT_INTERVAL);
    return;
  }

  try {
    const socket = new WebSocket(RPC_WS_URL);
    ws = socket;

    socket.onopen = async () => {
      try { await authInitPromise; } catch (e) {}
      if (!paired) await pairWithApp();
      if (ws !== socket || socket.readyState !== WebSocket.OPEN) return;
      wsConnected = paired || Boolean(rpcToken);
      // Send authentication handshake if token available
      if (rpcToken && socket.readyState === WebSocket.OPEN) {
        try {
          socket.send(JSON.stringify({
            jsonrpc: '2.0',
            id: 1,
            method: 'superidm.ping',
            params: { token: rpcToken }
          }));
        } catch (e) { /* socket closed mid-handshake */ }
      }
      broadcastStatus();
      syncDesktopBrowserRules(true);
      console.log('[SuperIDM] Secure connection established with desktop app');
    };

    socket.onclose = () => {
      if (ws === socket) {
        wsConnected = false;
        ws = null;
        broadcastStatus();
      }
      setTimeout(connectWS, RECONNECT_INTERVAL);
    };

    socket.onerror = () => {
      if (ws === socket) {
        wsConnected = false;
        ws = null;
        broadcastStatus();
      }
    };

    socket.onmessage = (event) => {
      try {
        const resp = JSON.parse(event.data);
        if (resp && resp.error) {
          console.warn('[SuperIDM Security]', resp.error);
        }
      } catch (e) { /* ignore non-JSON */ }
    };
  } catch (e) {
    wsConnected = false;
    setTimeout(connectWS, RECONNECT_INTERVAL);
  }
}

function broadcastStatus() {
  // 1. Notify popup (runtime message)
  chrome.runtime.sendMessage({ type: 'status', connected: wsConnected }).catch(() => {});

  // 2. Notify ALL content scripts in every tab so badges show/hide with app state
  chrome.tabs.query({}, (tabs) => {
    if (!tabs) return;
    tabs.forEach(tab => {
      if (tab.id > 0) {
        chrome.tabs.sendMessage(tab.id, {
          type: 'connection_status',
          connected: wsConnected
        }).catch(() => {});
      }
    });
  });

  // 3. Update extension icon badge to reflect connection state
  try {
    if (wsConnected) {
      chrome.action.setBadgeText({ text: 'ON' });
      chrome.action.setBadgeBackgroundColor({ color: '#00C853' });
    } else {
      chrome.action.setBadgeText({ text: 'OFF' });
      chrome.action.setBadgeBackgroundColor({ color: '#FF1744' });
    }
  } catch (e) { /* ignore if action API unavailable */ }
}

// ═══════ Send Download to SuperIDM ═══════
// Guards against the same URL being queued twice within a few seconds (click interceptor +
// browser download hook, double clicks, ...).
const recentSends = new Map();
const DUPLICATE_WINDOW_MS = 2500;

function pageHeaders(pageUrl) {
  if (!pageUrl || !/^https?:/i.test(pageUrl)) return undefined;
  const headers = { 'Referer': pageUrl };
  try { headers['Origin'] = new URL(pageUrl).origin; } catch (e) { /* ignore */ }
  return headers;
}

async function sendToSuperIDM(url, filename, format, headers, cookies, cookieStoreId) {
  const nowTs = Date.now();
  const dupKey = String(url) + '|' + String(format || '');
  const last = recentSends.get(dupKey);
  if (last && nowTs - last < DUPLICATE_WINDOW_MS) {
    return { ok: true, via: 'duplicate-skipped' };
  }
  recentSends.set(dupKey, nowTs);
  if (recentSends.size > 200) {
    for (const [k, t] of recentSends) {
      if (nowTs - t > DUPLICATE_WINDOW_MS) recentSends.delete(k);
    }
  }

  const result = await sendToSuperIDMInternal(url, filename, format, headers, cookies, cookieStoreId);
  if (!result || !result.ok) recentSends.delete(dupKey); // allow an immediate retry after a failure
  return result;
}

async function sendToSuperIDMInternal(url, filename, format, headers, cookies, cookieStoreId) {
  // If token not loaded yet, try loading it
  if (!rpcToken) {
    try { await authInitPromise; } catch (e) {}
    if (!rpcToken) await initAuthAndSettings();
  }

  // If URL is an Instagram/Facebook CDN chunk, prefer routing active tab's page URL ONLY if it contains an actual post/reel shortcode
  let finalUrl = url;
  let activeStoreId = cookieStoreId;
  if (typeof finalUrl === 'string' && (finalUrl.includes('fbcdn.net') || finalUrl.includes('cdninstagram.com'))) {
    try {
      const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
      if (tabs && tabs[0]) {
        if (!activeStoreId && tabs[0].cookieStoreId) activeStoreId = tabs[0].cookieStoreId;
        if (tabs[0].url && /instagram\.com/i.test(tabs[0].url)) {
          const match = tabs[0].url.match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
          if (match) {
            const kind = match[1].toLowerCase() === 'reels' ? 'reel' : match[1].toLowerCase();
            finalUrl = `https://www.instagram.com/${kind}/${match[2]}/`;
          }
        }
      }
    } catch (e) {}
  }

  // Sanitize filename to avoid "(3) Instagram.mp4" or generic place-holders
  let cleanFilename = filename;
  if (cleanFilename) {
    cleanFilename = cleanFilename
      .replace(/^\(\d+\)\s*/, '')
      .replace(/^\[\d+\]\s*/, '')
      .replace(/\s*•\s*Instagram.*$/i, '')
      .replace(/\s*[-|–—]\s*(Instagram|YouTube|Facebook|TikTok).*$/i, '')
      .trim();
    if (/^(Instagram|Facebook|YouTube|Twitter|TikTok)\.mp4$/i.test(cleanFilename) || cleanFilename === '.mp4' || cleanFilename === 'video.mp4' || cleanFilename === 'download.dat') {
      cleanFilename = undefined;
    }
  }

  // Auto-extract cookies for authenticated downloads if not explicitly provided
  let finalCookies = cookies;
  if (!finalCookies && chrome.cookies && typeof finalUrl === 'string' && (finalUrl.startsWith('http://') || finalUrl.startsWith('https://'))) {
    try {
      const query = { url: finalUrl };
      if (activeStoreId) query.storeId = activeStoreId;
      const cookieList = await chrome.cookies.getAll(query);
      if (cookieList && cookieList.length > 0) {
        finalCookies = cookieList.map(c => `${c.name}=${c.value}`).join('; ');
      }
    } catch (e) { /* ignore cookie fetch errors */ }
  }

  const payload = {
    jsonrpc: '2.0',
    id: Date.now(),
    method: 'superidm.addUri',
    params: {
      token: rpcToken,
      url: finalUrl,
      filename: cleanFilename || undefined,
      format: format || undefined,
      headers: headers || undefined,
      cookies: finalCookies || undefined
    }
  };

  // Use HTTP so we get the app's real answer. (The old WebSocket path returned success
  // immediately, even when the app rejected the request, so downloads silently vanished.)
  const postDownload = async () => {
    payload.params.token = rpcToken;
    return fetch(`${RPC_HTTP_URL}/api/download`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${rpcToken}`,
        'X-SuperIDM-Token': rpcToken
      },
      body: JSON.stringify(payload.params)
    });
  };

  try {
    if (!rpcToken) await pairWithApp();
    let resp = await postDownload();

    // Token missing/stale (e.g. app reinstalled) -> pair automatically and retry once
    if (resp.status === 401) {
      paired = false;
      if (await pairWithApp()) {
        resp = await postDownload();
      }
    }

    if (resp.status === 401) {
      return { ok: false, error: 'Extension is not paired with SuperIDM. Please update the SuperIDM desktop app.' };
    }
    if (!resp.ok) {
      return { ok: false, error: `SuperIDM rejected the request (HTTP ${resp.status})` };
    }

    let json = null;
    try { json = await resp.json(); } catch (e) { /* ignore */ }
    const result = json && json.result;
    if (result && result.status && result.status !== 'ok') {
      return { ok: false, error: result.message || 'SuperIDM could not queue this download' };
    }
    paired = true;
    return { ok: true, via: 'http' };
  } catch (e) {
    return { ok: false, error: 'SuperIDM desktop app is not running' };
  }
}

// ═══════ Context Menus ═══════
function setupContextMenus() {
  try {
    chrome.contextMenus.removeAll(() => {
      if (chrome.runtime.lastError) { /* consume error */ }

      chrome.contextMenus.create({
        id: 'superidm-download-link',
        title: 'Download with SuperIDM',
        contexts: ['link', 'image', 'audio', 'video']
      }, () => {
        if (chrome.runtime.lastError) { /* consume error */ }
      });

      chrome.contextMenus.create({
        id: 'superidm-download-page-links',
        title: 'Download All Links with SuperIDM',
        contexts: ['page']
      }, () => {
        if (chrome.runtime.lastError) { /* consume error */ }
      });
    });
  } catch (e) { /* ignore */ }
}

chrome.runtime.onInstalled.addListener(() => {
  setupContextMenus();
});

chrome.runtime.onStartup.addListener(() => {
  setupContextMenus();
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  try { await authInitPromise; } catch (e) {}
  const storeId = tab && tab.cookieStoreId ? tab.cookieStoreId : undefined;

  if (info.menuItemId === 'superidm-download-link') {
    let url = info.linkUrl || info.srcUrl;
    if (url && (url.startsWith('blob:') || url.startsWith('data:'))) {
      url = (tab && tab.url) || '';
    }
    // If on Instagram, prefer the Instagram reel/post page URL rather than raw audio/video DASH chunk
    if (tab && tab.url && /instagram\.com\/(reel|reels|p|tv)\//i.test(tab.url)) {
      if (!info.linkUrl || !/\/(reel|reels|p|tv)\//i.test(info.linkUrl)) {
        url = tab.url;
      }
    }
    if (url && /^https?:/i.test(url)) {
      const result = await sendToSuperIDM(url, undefined, undefined, pageHeaders(info.pageUrl || (tab && tab.url)), undefined, storeId);
      if (!result.ok) {
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icons/icon128.png',
          title: 'SuperIDM',
          message: result.error || 'Desktop app is not running. Please start SuperIDM first.'
        });
      }
    }
  } else if (info.menuItemId === 'superidm-download-page-links') {
    if (tab && tab.id) {
      if (tab.url && isDomainExcluded(tab.url)) {
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icons/icon128.png',
          title: 'SuperIDM',
          message: 'This domain is in your Excluded Domains list.'
        });
        return;
      }
      const allowed = (settings.fileExtensions || DEFAULT_SETTINGS.fileExtensions)
        .split(/[\s,;]+/)
        .map(e => e.trim().replace(/^\./, '').toLowerCase())
        .filter(Boolean);
      try {
        const results = await chrome.scripting.executeScript({
          target: { tabId: tab.id },
          args: [allowed],
          func: (allowedExts) => {
            const set = new Set(allowedExts);
            const out = [];
            const seen = new Set();
            document.querySelectorAll('a[href]').forEach(a => {
              const href = a.href;
              if (!/^https?:/i.test(href) || seen.has(href)) return;
              let ext = '';
              try {
                const p = new URL(href).pathname;
                const dot = p.lastIndexOf('.');
                if (dot !== -1) ext = p.substring(dot + 1).toLowerCase();
              } catch (e) { return; }
              if (a.hasAttribute('download') || set.has(ext)) {
                seen.add(href);
                out.push(href);
              }
            });
            return out.slice(0, 100);
          }
        });
        const rawUrls = (results && results[0] && results[0].result) || [];
        const urls = rawUrls.filter(u => !isDomainExcluded(u));
        if (urls.length === 0) {
          chrome.notifications.create({
            type: 'basic',
            iconUrl: 'icons/icon128.png',
            title: 'SuperIDM',
            message: 'No downloadable links found on this page.'
          });
          return;
        }
        const hdrs = pageHeaders(tab.url);
        let sent = 0;
        let lastError = '';
        for (const u of urls) {
          const r = await sendToSuperIDM(u, undefined, undefined, hdrs, undefined, storeId);
          if (r && r.ok) {
            sent++;
          } else {
            lastError = (r && r.error) || lastError;
            break; // Stop immediately if desktop app goes offline or rejects
          }
          await new Promise(res => setTimeout(res, 120)); // don't flood the desktop app
        }
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icons/icon128.png',
          title: 'SuperIDM',
          message: sent > 0 ? `Sent ${sent} of ${urls.length} links to SuperIDM.` : (lastError || 'Desktop app is not running. Please start SuperIDM first.')
        });
      } catch (e) {
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icons/icon128.png',
          title: 'SuperIDM',
          message: 'Could not read links from this page.'
        });
      }
    }
  }
});

// ═══════ Download Interceptor (Cross-Browser: Chromium + Firefox) ═══════
const dlApi = typeof chrome !== 'undefined' ? chrome.downloads : null;
const onDeterminingEvent = dlApi ? dlApi['onDetermining' + 'Filename'] : null;
if (onDeterminingEvent && typeof onDeterminingEvent.addListener === 'function') {
  // Chromium-based browsers (Chrome, Edge, Brave, Opera)
  onDeterminingEvent.addListener((item, suggest) => {
    let handled = false;
    const safeSuggest = () => {
      if (!handled) {
        handled = true;
        try { suggest(); } catch (e) {}
      }
    };

    const run = async () => {
      try { await authInitPromise; } catch (e) {}
      if (!shouldInterceptBrowserDownload(item)) {
        safeSuggest();
        return;
      }

      let dlUrl = item.url;
      // If downloading an Instagram/FB DASH chunk and referrer points to a reel/post, route the page URL so yt-dlp muxes full video and audio
      if ((dlUrl.includes('fbcdn.net') || dlUrl.includes('cdninstagram.com')) && item.referrer) {
        if (/instagram\.com\/(reel|reels|p|tv)\//i.test(item.referrer)) {
          dlUrl = item.referrer;
        }
      }

      const timeoutPromise = new Promise((resolve) => setTimeout(() => resolve({ ok: false, timeout: true }), 2500));
      const reqHeaders = item.referrer ? { 'Referer': item.referrer } : undefined;
      const result = await Promise.race([
        sendToSuperIDM(dlUrl, item.filename, null, reqHeaders),
        timeoutPromise
      ]);

      if (result && result.ok) {
        handled = true;
        chrome.downloads.cancel(item.id, () => {
          if (chrome.runtime.lastError) { /* ignore cancel race */ }
          chrome.downloads.erase({ id: item.id }, () => {
            if (chrome.runtime.lastError) { /* ignore */ }
          });
        });
      } else {
        safeSuggest();
      }
    };

    run().catch(() => safeSuggest());
    return true; // Keep suggest channel open asynchronously
  });
} else if (chrome.downloads && chrome.downloads.onCreated) {
  // Firefox (Gecko) fallback download interceptor
  chrome.downloads.onCreated.addListener(async (item) => {
    try { await authInitPromise; } catch (e) {}
    if (!shouldInterceptBrowserDownload(item)) return;

    let dlUrl = item.url;
    if ((dlUrl.includes('fbcdn.net') || dlUrl.includes('cdninstagram.com')) && item.referrer) {
      if (/instagram\.com\/(reel|reels|p|tv)\//i.test(item.referrer)) {
        dlUrl = item.referrer;
      }
    }

    const reqHeaders = item.referrer ? { 'Referer': item.referrer } : undefined;
    const result = await sendToSuperIDM(dlUrl, item.filename, null, reqHeaders);
    if (result && result.ok) {
      try {
        chrome.downloads.cancel(item.id, () => {
          if (chrome.runtime.lastError) {}
          chrome.downloads.erase({ id: item.id }, () => {});
        });
      } catch (e) {}
    }
  });
}

// ═══════ Media Stream Sniffer ═══════
// Only keep things a user could actually want to download: HLS/DASH manifests and full media
// files. Individual stream segments (.ts / .m4s / chunk-123) and ad/analytics noise are dropped,
// otherwise hundreds of segments push the real manifest out of the per-tab list.
const MEDIA_NOISE = /google-analytics|doubleclick|googlesyndication|googleadservices|scorecardresearch|adservice|\/ads?\/|analytics|googlevideo\.com/i;
const MEDIA_SEGMENT = /\.(ts|m4s|cmfv|cmfa|fmp4|vtt|webvtt|srt)(\?|#|$)|[\/_.-](seg(ment)?|frag(ment)?|chunk)[-_]?\d+/i;
const MEDIA_MANIFEST = /\.(m3u8|mpd|f4m|ism)(\?|#|$)|\/manifest(\.[a-z0-9]+)?(\?|$)/i;
const MEDIA_MIN_BYTES = 100 * 1024;
const MEDIA_RANGE_PARAMS = ['range', 'bytestart', 'byteend', 'rn', 'rbuf'];
const MAX_MEDIA_PER_TAB = 40;
const tabUrls = {};

function normalizeMediaKey(raw) {
  try {
    const u = new URL(raw);
    MEDIA_RANGE_PARAMS.forEach(p => u.searchParams.delete(p));
    u.hash = '';
    return u.toString();
  } catch (e) {
    return raw;
  }
}

function getHeader(details, name) {
  const h = details.responseHeaders && details.responseHeaders.find(x => x.name.toLowerCase() === name);
  return h && h.value ? String(h.value) : '';
}

function mediaTotalSize(details) {
  const range = getHeader(details, 'content-range'); // e.g. "bytes 0-1023/52428800"
  const m = range.match(/\/(\d+)\s*$/);
  if (m) return parseInt(m[1], 10);
  const len = parseInt(getHeader(details, 'content-length'), 10);
  return Number.isFinite(len) ? len : 0;
}

function processMediaRequest(details) {
  if (!settings.sniffMedia || !details.url || details.tabId <= 0) return;
  const rawUrl = details.url;
  if (!/^https?:/i.test(rawUrl)) return;
  if (isDomainExcluded(rawUrl) || isDomainExcluded(details.initiator || details.documentUrl || tabUrls[details.tabId] || '')) return;
  if (MEDIA_NOISE.test(rawUrl) || MEDIA_SEGMENT.test(rawUrl)) return;

  const contentType = getHeader(details, 'content-type').toLowerCase();
  const isManifest = MEDIA_MANIFEST.test(rawUrl)
    || contentType.includes('application/vnd.apple.mpegurl')
    || contentType.includes('application/x-mpegurl')
    || contentType.includes('application/dash+xml')
    || contentType.includes('application/vnd.ms-sstr+xml');
  const isMediaExt = /\.(mp4|webm|m4v|flv|avi|mkv|mov|mp3|aac|m4a|ogg|opus|wav|flac)(\?|#|$)/i.test(rawUrl);
  const isMediaMime = /^(video|audio)\//i.test(contentType);
  const isMedia = isManifest || isMediaExt || isMediaMime || details.type === 'media';
  if (!isMedia) return;

  const size = mediaTotalSize(details);
  if (!isManifest && size > 0 && size < MEDIA_MIN_BYTES) return; // tracking clips, UI sounds

  const key = normalizeMediaKey(rawUrl);
  const list = tabMedia[details.tabId] || (tabMedia[details.tabId] = []);
  const existing = list.find(m => m.key === key);
  if (existing) {
    if (size > (existing.size || 0)) existing.size = size;
    return;
  }

  list.push({
    key,
    url: rawUrl,
    type: contentType || (isManifest ? 'manifest' : 'media'),
    kind: isManifest ? 'manifest' : 'file',
    size,
    time: Date.now()
  });
  if (list.length > MAX_MEDIA_PER_TAB) {
    // Drop the oldest non-manifest first so manifests survive
    const idx = list.findIndex(m => m.kind !== 'manifest');
    list.splice(idx === -1 ? 0 : idx, 1);
  }
  persistTabMedia();

  // Notify the tab content script immediately that new media was detected
  chrome.tabs.sendMessage(details.tabId, {
    type: 'media_detected',
    mediaUrl: rawUrl,
    contentType: contentType
  }).catch(() => {});
}

if (chrome.webRequest) {
  // 1. Sniff onHeadersReceived (fires immediately when media starts buffering!)
  if (chrome.webRequest.onHeadersReceived) {
    try {
      chrome.webRequest.onHeadersReceived.addListener(
        processMediaRequest,
        { urls: ['<all_urls>'] },
        ['responseHeaders']
      );
    } catch (e) { /* ignore */ }
  }
}

// Clean up closed tabs, and forget stale media when a tab navigates to a different page
chrome.tabs.onRemoved.addListener((tabId) => {
  delete tabMedia[tabId];
  delete tabUrls[tabId];
  persistTabMedia();
});

if (chrome.tabs.onUpdated) {
  chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
    if (!changeInfo.url) return;
    const pageUrl = changeInfo.url.split('#')[0];
    if (tabUrls[tabId] && tabUrls[tabId] !== pageUrl) {
      delete tabMedia[tabId];
      persistTabMedia();
    }
    tabUrls[tabId] = pageUrl;
  });
}

// Refresh connection + auto-pair immediately (critical when MV3 service worker wakes from idle)
async function refreshConnectionState() {
  try { await authInitPromise; } catch (e) {}
  const online = await isServerOnline();
  if (!online) {
    paired = false;
  } else if (!paired) {
    await pairWithApp();
  }
  const ready = online && (paired || Boolean(rpcToken));
  if (ready !== wsConnected) {
    wsConnected = ready;
    if (!online && ws) {
      try { ws.close(); } catch (e) {}
      ws = null;
    }
    broadcastStatus();
    if (online && (!ws || ws.readyState !== WebSocket.OPEN)) {
      connectWS();
    }
  }
  if (ready) {
    syncDesktopBrowserRules(false);
  }
  return wsConnected;
}

// ═══════ Message Handler (from popup & content scripts) ═══════
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  const storeId = sender && sender.tab && sender.tab.cookieStoreId ? sender.tab.cookieStoreId : undefined;

  if (msg.type === 'bypass_url') {
    markUrlBypassed(msg.url);
    sendResponse({ ok: true });
    return true;
  }

  if (msg.type === 'get_status') {
    (async () => {
      try { await authInitPromise; } catch (e) {}
      if (wsConnected && !msg.force) {
        sendResponse({ connected: true, settings });
      } else {
        const connected = await refreshConnectionState();
        sendResponse({ connected, settings });
      }
    })();
    return true;
  }

  if (msg.type === 'update_settings') {
    (async () => {
      try { await authInitPromise; } catch (e) {}
      let rulesChanged = false;
      if (msg.settings) {
        if (msg.rulesEdited ||
            (msg.settings.fileExtensions !== undefined && msg.settings.fileExtensions !== settings.fileExtensions) ||
            (msg.settings.excludedDomains !== undefined && msg.settings.excludedDomains !== settings.excludedDomains)) {
          rulesChanged = true;
          lastDesktopRulesTs = Math.max(lastDesktopRulesTs + 1, Date.now());
        }
        Object.assign(settings, msg.settings);
      }
      if (msg.tokenChanged) {
        rpcToken = typeof msg.token === 'string' ? msg.token.trim() : '';
        paired = false;
        if (ws) { try { ws.close(); } catch (e) {} ws = null; }
      } else if (typeof msg.token === 'string' && msg.token.trim() && msg.token.trim() !== rpcToken) {
        rpcToken = msg.token.trim();
        paired = false;
        if (ws) { try { ws.close(); } catch (e) {} ws = null; }
      }
      saveSettings();
      broadcastSettingsUpdate();
      if (msg.tokenChanged) {
        refreshConnectionState();
      }
      if (rulesChanged) {
        pushRulesToDesktop().catch(() => {});
      }
      sendResponse({ ok: true, settings });
    })();
    return true;
  }

  if (msg.type === 'get_tab_media') {
    (async () => {
      try { await authInitPromise; } catch (e) {}
      if (sender.tab && sender.tab.id) {
        sendResponse({ media: tabMedia[sender.tab.id] || [] });
      } else {
        chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
          const activeTabId = tabs && tabs[0] ? tabs[0].id : null;
          sendResponse({ media: (activeTabId && tabMedia[activeTabId]) ? tabMedia[activeTabId] : [] });
        });
      }
    })();
    return true;
  }

  if (msg.type === 'intercept_click_download') {
    let headers = undefined;
    if (msg.referrer) {
      headers = { 'Referer': msg.referrer };
    }
    sendToSuperIDM(msg.url, msg.filename, null, headers, msg.cookies, storeId).then(sendResponse);
    return true;
  }

  if (msg.type === 'download') {
    sendToSuperIDM(msg.url, msg.filename, msg.format, pageHeaders(msg.referrer), undefined, storeId).then(sendResponse);
    return true;
  }

  if (msg.type === 'download_video') {
    // Sniffed CDN streams usually reject requests without the original page as Referer/Origin
    sendToSuperIDM(msg.url, msg.filename, msg.format, pageHeaders(msg.referrer), undefined, storeId).then(sendResponse);
    return true;
  }
});

// ═══════ Init & Active Health Poller ═══════
refreshConnectionState();
connectWS();

setInterval(() => {
  refreshConnectionState();
}, 1500);

if (chrome.runtime && chrome.runtime.onStartup) {
  chrome.runtime.onStartup.addListener(() => { refreshConnectionState(); });
}
if (chrome.tabs && chrome.tabs.onActivated) {
  chrome.tabs.onActivated.addListener(() => {
    if (!wsConnected) refreshConnectionState();
  });
}

