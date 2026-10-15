// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Popup Script
// ═══════════════════════════════════════════════════════════════

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

const DEFAULT_EXTENSIONS = '3GP 7Z AAC ACE AIF ARJ ASF AVI BIN BZ2 DEB DMG DOCX EXE FLAC FLV GZ ISO LZH M4A M4V MKV MOV MP3 MP4 MPG MSI OGG PDF PPTX RAR RPM TAR TORRENT WAV WMA WMV XAPK XLSX ZIP';

// ═══════ Tab Switching ═══════
$$('.popup-tab').forEach(tabBtn => {
  tabBtn.addEventListener('click', () => {
    $$('.popup-tab').forEach(b => b.classList.remove('active'));
    $$('.tab-pane').forEach(p => p.classList.remove('active'));

    tabBtn.classList.add('active');
    const targetId = tabBtn.getAttribute('data-target');
    const targetPane = $(`#${targetId}`);
    if (targetPane) targetPane.classList.add('active');
  });
});

// ═══════ Status ═══════
function updateStatus(connected) {
  const badge = $('#statusBadge');
  const text = $('#statusText');
  if (connected) {
    badge.className = 'status-badge connected';
    text.textContent = 'Connected';
  } else {
    badge.className = 'status-badge disconnected';
    text.textContent = 'Offline';
  }
}

// ═══════ Settings Management ═══════
function loadSettings(settings) {
  if (!settings) return;
  if ($('#togClickIntercept')) $('#togClickIntercept').checked = settings.interceptClicks !== false;
  if ($('#togIntercept')) $('#togIntercept').checked = settings.interceptDownloads !== false;
  if ($('#togGrabber')) $('#togGrabber').checked = settings.showVideoGrabber !== false;
  if ($('#togSniff')) $('#togSniff').checked = settings.sniffMedia !== false;
  if ($('#minSize')) $('#minSize').value = settings.minFileSizeMB || 1;

  if ($('#fileExtsInput')) {
    $('#fileExtsInput').value = settings.fileExtensions || DEFAULT_EXTENSIONS;
  }
  if ($('#excludedDomainsInput')) {
    $('#excludedDomainsInput').value = settings.excludedDomains || '';
  }

  chrome.storage.local.get('superidm_rpc_token', (data) => {
    if (data.superidm_rpc_token && $('#tokenInput')) {
      $('#tokenInput').value = data.superidm_rpc_token;
    }
  });
}

function gatherSettings() {
  return {
    interceptClicks: $('#togClickIntercept') ? $('#togClickIntercept').checked : true,
    interceptDownloads: $('#togIntercept') ? $('#togIntercept').checked : true,
    showVideoGrabber: $('#togGrabber') ? $('#togGrabber').checked : true,
    sniffMedia: $('#togSniff') ? $('#togSniff').checked : true,
    minFileSizeMB: parseInt($('#minSize')?.value) || 1,
    fileExtensions: $('#fileExtsInput')?.value.trim() || DEFAULT_EXTENSIONS,
    excludedDomains: $('#excludedDomainsInput')?.value.trim() || ''
  };
}

function saveAllSettings() {
  const currentSettings = gatherSettings();
  const token = $('#tokenInput')?.value.trim() || '';

  if (token) {
    chrome.storage.local.set({ superidm_rpc_token: token });
  }

  chrome.runtime.sendMessage({
    type: 'update_settings',
    settings: currentSettings,
    token: token || undefined
  });
}

// Instant save on toggle switches
['togClickIntercept', 'togIntercept', 'togGrabber', 'togSniff', 'minSize'].forEach(id => {
  const el = $(`#${id}`);
  if (el) {
    el.addEventListener('change', () => {
      saveAllSettings();
    });
  }
});

// Explicit save button on Rules tab
const saveRulesBtn = $('#saveRulesBtn');
if (saveRulesBtn) {
  saveRulesBtn.addEventListener('click', () => {
    saveAllSettings();
    const orig = saveRulesBtn.textContent;
    saveRulesBtn.textContent = 'Saved! ✅';
    saveRulesBtn.style.background = '#00e676';
    setTimeout(() => {
      saveRulesBtn.textContent = orig;
      saveRulesBtn.style.background = '';
    }, 1800);
  });
}

// Reset file extensions to defaults
const resetExtsBtn = $('#resetExtsBtn');
if (resetExtsBtn) {
  resetExtsBtn.addEventListener('click', () => {
    if ($('#fileExtsInput')) {
      $('#fileExtsInput').value = DEFAULT_EXTENSIONS;
      saveAllSettings();
    }
  });
}

// ═══════ Quick Download ═══════
const dlBtn = $('#dlBtn');
const urlInput = $('#urlInput');

if (dlBtn && urlInput) {
  dlBtn.addEventListener('click', () => {
    const url = urlInput.value.trim();
    if (!url) return;
    chrome.runtime.sendMessage({ type: 'download', url }, (resp) => {
      if (resp && resp.ok) {
        urlInput.value = '';
        urlInput.placeholder = '✅ Sent to SuperIDM!';
        setTimeout(() => { urlInput.placeholder = 'Paste URL here...'; }, 2000);
      } else {
        urlInput.placeholder = '❌ SuperIDM is not running';
        setTimeout(() => { urlInput.placeholder = 'Paste URL here...'; }, 2000);
      }
    });
  });

  urlInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') dlBtn.click();
  });
}

// ═══════ Detected Media List ═══════
function renderMedia(mediaList) {
  const container = $('#mediaList');
  const count = $('#mediaCount');
  if (!container || !count) return;

  count.textContent = mediaList.length;

  if (mediaList.length === 0) {
    container.innerHTML = '<div class="media-empty">No media detected on this tab</div>';
    return;
  }

  container.innerHTML = '';
  mediaList.forEach(m => {
    const item = document.createElement('div');
    item.className = 'media-item';

    let shortUrl = m.url;
    try {
      const urlObj = new URL(m.url);
      shortUrl = urlObj.pathname.split('/').pop() || urlObj.hostname;
    } catch (e) {}

    item.innerHTML = `
      <div class="media-info">
        <div class="media-name" title="${m.url}">${shortUrl}</div>
        <div class="media-type">${m.type || 'media'}</div>
      </div>
    `;

    const btn = document.createElement('button');
    btn.className = 'media-dl-btn';
    btn.textContent = 'DL';
    btn.addEventListener('click', () => {
      btn.textContent = '⏳';
      chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
        const activeTab = tabs && tabs[0];
        const tabUrl = activeTab?.url || '';
        const rawTitle = (activeTab?.title || '')
          .replace(/^\(\d+\)\s*/, '')
          .replace(/^\[\d+\]\s*/, '')
          .replace(/\s*•\s*Instagram.*$/i, '')
          .replace(/\s*[-|–—]\s*(Instagram|YouTube|Facebook|TikTok).*$/i, '')
          .trim();

        const isPlatform = /instagram\.com|instagr\.am|tiktok\.com|youtube\.com|youtu\.be|facebook\.com|fb\.watch|twitter\.com|x\.com|vimeo\.com/i.test(tabUrl);
        const isInstagramCdn = /fbcdn\.net|cdninstagram\.com/i.test(m.url);

        let sendPayload;
        if (isPlatform || isInstagramCdn) {
          const targetUrl = (tabUrl && tabUrl.startsWith('http')) ? tabUrl : m.url;
          const cleanTitle = (rawTitle && !/^(Instagram|YouTube|Facebook|TikTok|video)$/i.test(rawTitle)) ? `${rawTitle}.mp4` : undefined;
          sendPayload = {
            type: 'download_video',
            url: targetUrl,
            filename: cleanTitle,
            format: 'bestvideo+bestaudio/best'
          };
        } else {
          sendPayload = {
            type: 'download',
            url: m.url,
            filename: shortUrl
          };
        }

        chrome.runtime.sendMessage(sendPayload, (resp) => {
          btn.textContent = (resp && resp.ok) ? '✅' : '❌';
          setTimeout(() => { btn.textContent = 'DL'; }, 1500);
        });
      });
    });

    item.appendChild(btn);
    container.appendChild(item);
  });
}

// ═══════ Init ═══════
chrome.runtime.sendMessage({ type: 'get_status' }, (resp) => {
  if (resp) {
    updateStatus(resp.connected);
    if (resp.settings) loadSettings(resp.settings);
  } else {
    updateStatus(false);
  }
});

chrome.runtime.sendMessage({ type: 'get_tab_media' }, (resp) => {
  if (resp && resp.media) renderMedia(resp.media);
});

chrome.runtime.onMessage.addListener((msg) => {
  if (msg.type === 'status') updateStatus(msg.connected);
});
