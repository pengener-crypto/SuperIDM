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
  const data = await chrome.storage.local.get(['superidm_settings', 'superidm_rpc_token']);
  if (data.superidm_settings) {
    settings = Object.assign({}, DEFAULT_SETTINGS, data.superidm_settings);
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
initAuthAndSettings();

function saveSettings() {
  chrome.storage.local.set({ superidm_settings: settings, superidm_rpc_token: rpcToken });
}

// ═══════ WebSocket Connection ═══════
async function isServerOnline() {
  try {
    const res = await fetch(RPC_HTTP_URL + '/api/ping', {
      method: 'GET',
      cache: 'no-store'
    });
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

    ws.onopen = () => {
      wsConnected = true;
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
async function sendToSuperIDM(url, filename, format, headers, cookies) {
  // If token not loaded yet, try loading it
  if (!rpcToken) {
    await initAuthAndSettings();
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

  // Try WebSocket first
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(payload));
    return { ok: true, via: 'ws' };
  }

  // Fallback to HTTP POST with Bearer Token and Custom Header
  try {
    const resp = await fetch(`${RPC_HTTP_URL}/api/download`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${rpcToken}`,
        'X-SuperIDM-Token': rpcToken
      },
      body: JSON.stringify(payload.params)
    });
    if (resp.ok) return { ok: true, via: 'http' };
  } catch (e) { /* fallback failed */ }

  return { ok: false, error: 'SuperIDM desktop app is not running' };
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
      const result = await sendToSuperIDM(url);
      if (!result.ok) {
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icons/icon128.png',
          title: 'SuperIDM',
          message: 'Desktop app is not running. Please start SuperIDM first.'
        });
      }
    }
  } else if (info.menuItemId === 'superidm-download-page-links') {
    if (tab && tab.id) {
      chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => {
          const links = [...document.querySelectorAll('a[href]')]
            .map(a => a.href)
            .filter(h => /\.(zip|rar|7z|exe|msi|iso|tar|gz|pdf|mp4|mkv|avi|mp3|flac|wav|apk|dmg|deb|rpm)(\?.*)?$/i.test(h));
          return [...new Set(links)];
        }
      }).then(results => {
        const urls = results?.[0]?.result || [];
        urls.forEach(u => sendToSuperIDM(u));
      });
    }
  }
});

// ═══════ Download Interceptor ═══════
chrome.downloads.onDeterminingFilename.addListener((item, suggest) => {
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

// ═══════ Media Stream Sniffer ═══════
function processMediaRequest(details) {
  if (!settings.sniffMedia || !details.url || details.tabId <= 0) return;
  const rawUrl = details.url;
  const url = rawUrl.toLowerCase();

  // Exclude ads, analytics, small tracking pixels
  if (url.includes('google-analytics') || url.includes('doubleclick') || url.includes('/ad/') || url.includes('analytics')) {
    return;
  }

  const isMediaExt = /\.(mp4|webm|m4v|flv|avi|mkv|mov|ts|m3u8|mpd|f4m|ism|mp3|aac|m4a|ogg|opus|wav|flac)(\?|$)/i.test(url);
  const contentType = details.responseHeaders?.find(h => h.name.toLowerCase() === 'content-type')?.value?.toLowerCase() || '';
  const isMediaMime = /video|audio/i.test(contentType)
    || contentType.includes('application/vnd.apple.mpegurl')
    || contentType.includes('application/x-mpegurl')
    || contentType.includes('application/dash+xml')
    || contentType.includes('application/vnd.ms-sstr+xml');

  const isMedia = isMediaExt || (details.type === 'media') || isMediaMime;

  if (isMedia) {
    if (!tabMedia[details.tabId]) tabMedia[details.tabId] = [];
    // Deduplicate by URL
    if (!tabMedia[details.tabId].some(m => m.url === rawUrl)) {
      tabMedia[details.tabId].push({
        url: rawUrl,
        type: contentType || (isMediaExt ? 'media-stream' : 'media'),
        time: Date.now()
      });
      // Keep only last 30 per tab
      if (tabMedia[details.tabId].length > 30) tabMedia[details.tabId].shift();

      // Notify the tab content script immediately that new media was detected
      chrome.tabs.sendMessage(details.tabId, {
        type: 'media_detected',
        mediaUrl: rawUrl,
        contentType: contentType
      }).catch(() => {});
    }
  }
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

  // 2. Also listen to onCompleted for completed chunks/segments
  if (chrome.webRequest.onCompleted) {
    try {
      chrome.webRequest.onCompleted.addListener(
        processMediaRequest,
        { urls: ['<all_urls>'] },
        ['responseHeaders']
      );
    } catch (e) { /* ignore */ }
  }
}

// Clean up closed tabs
chrome.tabs.onRemoved.addListener((tabId) => {
  delete tabMedia[tabId];
});

// ═══════ Message Handler (from popup & content scripts) ═══════
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.type === 'get_status') {
    sendResponse({ connected: wsConnected, settings });
    return true;
  }

  if (msg.type === 'update_settings') {
    if (msg.settings) Object.assign(settings, msg.settings);
    if (msg.token) {
      rpcToken = msg.token;
      if (ws) { ws.close(); } // Reconnect with new token
    }
    saveSettings();
    // Notify all content scripts with updated full settings
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
    sendToSuperIDM(msg.url, msg.filename, msg.format).then(sendResponse);
    return true;
  }

  if (msg.type === 'download_video') {
    sendToSuperIDM(msg.url, msg.filename, msg.format).then(sendResponse);
    return true;
  }
});

// ═══════ Init & Active Health Poller ═══════
connectWS();

setInterval(async () => {
  const online = await isServerOnline();
  if (online !== wsConnected) {
    wsConnected = online;
    if (!online && ws) {
      try { ws.close(); } catch(e) {}
      ws = null;
    }
    broadcastStatus();
    if (online && (!ws || ws.readyState !== WebSocket.OPEN)) {
      connectWS();
    }
  }
}, 1500);

