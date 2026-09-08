// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Background Service Worker
// ═══════════════════════════════════════════════════════════════

const RPC_WS_URL = 'ws://127.0.0.1:6800';
const RPC_HTTP_URL = 'http://127.0.0.1:6801';
const RECONNECT_INTERVAL = 3000;

// ───── State ─────
let ws = null;
let wsConnected = false;
let rpcToken = '';
let settings = {
  interceptDownloads: true,
  showVideoGrabber: true,
  sniffMedia: true,
  minFileSizeMB: 1
};
// Per-tab sniffed media
const tabMedia = {};

// ───── Token & Settings Persistence ─────
async function initAuthAndSettings() {
  const data = await chrome.storage.local.get(['superidm_settings', 'superidm_rpc_token']);
  if (data.superidm_settings) Object.assign(settings, data.superidm_settings);

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
function connectWS() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;

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
  chrome.runtime.sendMessage({ type: 'status', connected: wsConnected }).catch(() => {});
}

// ═══════ Send Download to SuperIDM ═══════
async function sendToSuperIDM(url, filename, format, headers, cookies) {
  // If token not loaded yet, try loading it
  if (!rpcToken) {
    await initAuthAndSettings();
  }

  const payload = {
    jsonrpc: '2.0',
    id: Date.now(),
    method: 'superidm.addUri',
    params: {
      token: rpcToken,
      url,
      filename: filename || undefined,
      format: format || undefined,
      headers: headers || undefined,
      cookies: cookies || undefined
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
        title: '⚡ Download with SuperIDM',
        contexts: ['link', 'image', 'audio', 'video']
      }, () => {
        if (chrome.runtime.lastError) { /* consume error */ }
      });

      chrome.contextMenus.create({
        id: 'superidm-download-page-links',
        title: '⚡ Download All Links with SuperIDM',
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
    const url = info.linkUrl || info.srcUrl;
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

  // Only intercept files above the minimum size threshold
  const minBytes = (settings.minFileSizeMB || 1) * 1024 * 1024;
  if (item.totalBytes > 0 && item.totalBytes < minBytes) {
    suggest();
    return;
  }

  // Skip small inline resources, data URIs, blob URIs
  if (!item.url || item.url.startsWith('blob:') || item.url.startsWith('data:')) {
    suggest();
    return;
  }

  // Skip browser internal downloads
  if (item.url.startsWith('chrome-extension://') || item.url.startsWith('edge://')) {
    suggest();
    return;
  }

  // Intercept: cancel browser download, send to SuperIDM
  sendToSuperIDM(item.url, item.filename).then(result => {
    if (result.ok) {
      chrome.downloads.cancel(item.id);
    }
  });

  suggest();
});

// ═══════ Media Stream Sniffer ═══════
if (chrome.webRequest && chrome.webRequest.onCompleted) {
  chrome.webRequest.onCompleted.addListener(
    (details) => {
      if (!settings.sniffMedia) return;
      const url = details.url.toLowerCase();
      const isMedia = /\.(mp4|webm|m4v|flv|avi|mkv|m3u8|mpd|ts)(\?|$)/i.test(url)
        || (details.type === 'media')
        || (details.responseHeaders && details.responseHeaders.some(h =>
          h.name.toLowerCase() === 'content-type' && /video|audio/i.test(h.value)));

      if (isMedia && details.tabId > 0) {
        if (!tabMedia[details.tabId]) tabMedia[details.tabId] = [];
        // Deduplicate
        if (!tabMedia[details.tabId].some(m => m.url === details.url)) {
          const contentType = details.responseHeaders?.find(h => h.name.toLowerCase() === 'content-type')?.value || '';
          tabMedia[details.tabId].push({
            url: details.url,
            type: contentType,
            time: Date.now()
          });
          // Keep only last 20 per tab
          if (tabMedia[details.tabId].length > 20) tabMedia[details.tabId].shift();
        }
      }
    },
    { urls: ['<all_urls>'] },
    ['responseHeaders']
  );
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
    // Notify all content scripts about grabber visibility
    chrome.tabs.query({}, (tabs) => {
      tabs.forEach(tab => {
        chrome.tabs.sendMessage(tab.id, {
          type: 'settings_update',
          showVideoGrabber: settings.showVideoGrabber
        }).catch(() => {});
      });
    });
    sendResponse({ ok: true });
    return true;
  }

  if (msg.type === 'get_tab_media') {
    const tabId = sender.tab ? sender.tab.id : null;
    sendResponse({ media: (tabId && tabMedia[tabId]) ? tabMedia[tabId] : [] });
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

// ═══════ Init ═══════
connectWS();
