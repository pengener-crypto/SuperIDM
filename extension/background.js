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

function isExtensionSupported(url, filename) {
  if (!settings.fileExtensions) return true;
  const raw = (filename || url || '').toLowerCase();
  const clean = raw.split(/[?#]/)[0];
  const dotIdx = clean.lastIndexOf('.');
  if (dotIdx === -1) return false;
  const ext = clean.substring(dotIdx + 1).toUpperCase();
  const allowed = settings.fileExtensions
    .split(/[\s,;]+/)
    .map(e => e.trim().replace(/^\./, '').toUpperCase())
    .filter(Boolean);
  return allowed.includes(ext);
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
    ws = new WebSocket(RPC_WS_URL);

    ws.onopen = async () => {
      try { await authInitPromise; } catch (e) {}
      if (!paired) await pairWithApp();
      wsConnected = paired || Boolean(rpcToken);
      // Send authentication handshake if token available
      if (rpcToken) {
        ws.send(JSON.stringify({
          jsonrpc: '2.0',
          id: 1,
          method: 'superidm.ping',
          params: { token: rpcToken }
        }));
      }
      broadcastStatus();
      syncDesktopBrowserRules(true);
      console.log('[SuperIDM] Secure connection established with desktop app');
    };

    ws.onclose = () => {
      wsConnected = false;
      ws = null;
      broadcastStatus();
      setTimeout(connectWS, RECONNECT_INTERVAL);
    };

    ws.onerror = () => {
      wsConnected = false;
      ws = null;
      broadcastStatus();
    };

    ws.onmessage = (event) => {
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

async function sendToSuperIDM(url, filename, format, headers, cookies) {
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

  const result = await sendToSuperIDMInternal(url, filename, format, headers, cookies);
  if (!result || !result.ok) recentSends.delete(dupKey); // allow an immediate retry after a failure
  return result;
}

async function sendToSuperIDMInternal(url, filename, format, headers, cookies) {
  // If token not loaded yet, try loading it
  if (!rpcToken) {
    try { await authInitPromise; } catch (e) {}
    if (!rpcToken) await initAuthAndSettings();
  }

  // If URL is an Instagram/Facebook CDN chunk, prefer routing active tab's page URL ONLY if it contains an actual post/reel shortcode
  let finalUrl = url;
  if (typeof finalUrl === 'string' && (finalUrl.includes('fbcdn.net') || finalUrl.includes('cdninstagram.com'))) {
    try {
      const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
      if (tabs && tabs[0] && tabs[0].url && /instagram\.com/i.test(tabs[0].url)) {
        const match = tabs[0].url.match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
        if (match) {
          finalUrl = `https://www.instagram.com/reel/${match[2]}/`;
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
      const cookieList = await chrome.cookies.getAll({ url: finalUrl });
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
  if (info.menuItemId === 'superidm-download-link') {
    let url = info.linkUrl || info.srcUrl;
    // If on Instagram, prefer the Instagram reel/post page URL rather than raw audio/video DASH chunk
    if (tab && tab.url && (tab.url.includes('instagram.com/reel') || tab.url.includes('instagram.com/p/'))) {
      if (!info.linkUrl || (!info.linkUrl.includes('/reel/') && !info.linkUrl.includes('/reels/') && !info.linkUrl.includes('/p/'))) {
        url = tab.url;
      }
    }
    if (url) {
      const result = await sendToSuperIDM(url, undefined, undefined, pageHeaders(info.pageUrl || (tab && tab.url)));
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
        const urls = (results && results[0] && results[0].result) || [];
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
          const r = await sendToSuperIDM(u, undefined, undefined, hdrs);
          if (r && r.ok) sent++; else { lastError = (r && r.error) || lastError; if (!sent) break; }
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
    if (!settings.interceptDownloads) {
      suggest();
      return;
    }

    // Skip browser internal downloads, data/blob URLs
    if (!item.url || item.url.startsWith('blob:') || item.url.startsWith('data:') ||
        item.url.startsWith('chrome-extension://') || item.url.startsWith('edge://')) {
      suggest();
      return;
    }

    // Skip excluded domains
    if (isDomainExcluded(item.url) || (item.referrer && isDomainExcluded(item.referrer))) {
      suggest();
      return;
    }

    // Check file extension or size threshold
    const hasMatchingExt = isExtensionSupported(item.url, item.filename);
    const minBytes = (settings.minFileSizeMB || 1) * 1024 * 1024;
    if (!hasMatchingExt && item.totalBytes > 0 && item.totalBytes < minBytes) {
      suggest();
      return;
    }

    let dlUrl = item.url;
    // If downloading an Instagram/FB DASH chunk and referrer points to a reel/post, route the page URL so yt-dlp muxes full video and audio
    if ((dlUrl.includes('fbcdn.net') || dlUrl.includes('cdninstagram.com')) && item.referrer) {
      if (item.referrer.includes('instagram.com/reel') || item.referrer.includes('instagram.com/p/')) {
        dlUrl = item.referrer;
      }
    }

    let handled = false;
    const safeSuggest = () => {
      if (!handled) {
        handled = true;
        suggest();
      }
    };

    // Wrap sendToSuperIDM with a 2500ms timeout
    const timeoutPromise = new Promise((resolve) => setTimeout(() => resolve({ ok: false, timeout: true }), 2500));
    const reqHeaders = item.referrer ? { 'Referer': item.referrer } : undefined;

    Promise.race([sendToSuperIDM(dlUrl, item.filename, null, reqHeaders), timeoutPromise])
      .then(result => {
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
      })
      .catch(() => {
        safeSuggest();
      });

    return true; // Keep suggest channel open asynchronously
  });
} else if (chrome.downloads && chrome.downloads.onCreated) {
  // Firefox (Gecko) fallback download interceptor
  chrome.downloads.onCreated.addListener(async (item) => {
    if (!settings.interceptDownloads) return;
    if (!item.url || item.url.startsWith('blob:') || item.url.startsWith('data:') ||
        item.url.startsWith('moz-extension://') || item.url.startsWith('about:')) return;

    if (isDomainExcluded(item.url) || (item.referrer && isDomainExcluded(item.referrer))) return;

    const hasMatchingExt = isExtensionSupported(item.url, item.filename);
    const minBytes = (settings.minFileSizeMB || 1) * 1024 * 1024;
    if (!hasMatchingExt && item.totalBytes > 0 && item.totalBytes < minBytes) return;

    let dlUrl = item.url;
    if ((dlUrl.includes('fbcdn.net') || dlUrl.includes('cdninstagram.com')) && item.referrer) {
      if (item.referrer.includes('instagram.com/reel') || item.referrer.includes('instagram.com/p/')) {
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
});

if (chrome.tabs.onUpdated) {
  chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
    if (!changeInfo.url) return;
    const pageUrl = changeInfo.url.split('#')[0];
    if (tabUrls[tabId] && tabUrls[tabId] !== pageUrl) {
      delete tabMedia[tabId];
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
  if (msg.type === 'get_status') {
    if (wsConnected && !msg.force) {
      sendResponse({ connected: true, settings });
    } else {
      refreshConnectionState().then((connected) => {
        sendResponse({ connected, settings });
      });
    }
    return true;
  }

  if (msg.type === 'update_settings') {
    if (msg.settings) {
      if (msg.settings.fileExtensions !== undefined || msg.settings.excludedDomains !== undefined) {
        lastDesktopRulesTs = Math.max(lastDesktopRulesTs, Date.now());
      }
      Object.assign(settings, msg.settings);
    }
    if (msg.token) {
      rpcToken = msg.token;
      if (ws) { ws.close(); } // Reconnect with new token
    }
    saveSettings();
    broadcastSettingsUpdate();
    sendResponse({ ok: true, settings });
    return true;
  }

  if (msg.type === 'get_tab_media') {
    // If sent from a tab's content script, sender.tab is populated
    if (sender.tab && sender.tab.id) {
      sendResponse({ media: tabMedia[sender.tab.id] || [] });
      return true;
    } else {
      // If sent from popup, sender.tab is undefined -> query the active tab in current window
      chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
        const activeTabId = tabs && tabs[0] ? tabs[0].id : null;
        sendResponse({ media: (activeTabId && tabMedia[activeTabId]) ? tabMedia[activeTabId] : [] });
      });
      return true; // Keep sendResponse channel open for async query
    }
  }

  if (msg.type === 'intercept_click_download') {
    let headers = undefined;
    if (msg.referrer) {
      headers = { 'Referer': msg.referrer };
    }
    sendToSuperIDM(msg.url, msg.filename, null, headers, msg.cookies).then(sendResponse);
    return true;
  }

  if (msg.type === 'download') {
    sendToSuperIDM(msg.url, msg.filename, msg.format, pageHeaders(msg.referrer)).then(sendResponse);
    return true;
  }

  if (msg.type === 'download_video') {
    // Sniffed CDN streams usually reject requests without the original page as Referer/Origin
    sendToSuperIDM(msg.url, msg.filename, msg.format, pageHeaders(msg.referrer)).then(sendResponse);
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

