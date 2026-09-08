// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Popup Script
// ═══════════════════════════════════════════════════════════════

const $ = (sel) => document.querySelector(sel);

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

// ═══════ Settings ═══════
function loadSettings(settings) {
  $('#togIntercept').checked = settings.interceptDownloads !== false;
  $('#togGrabber').checked = settings.showVideoGrabber !== false;
  $('#togSniff').checked = settings.sniffMedia !== false;
  $('#minSize').value = settings.minFileSizeMB || 1;

  chrome.storage.local.get('superidm_rpc_token', (data) => {
    if (data.superidm_rpc_token) {
      $('#tokenInput').value = data.superidm_rpc_token;
    }
  });
}

function gatherSettings() {
  return {
    interceptDownloads: $('#togIntercept').checked,
    showVideoGrabber: $('#togGrabber').checked,
    sniffMedia: $('#togSniff').checked,
    minFileSizeMB: parseInt($('#minSize').value) || 1
  };
}

// Save on any toggle change
['togIntercept', 'togGrabber', 'togSniff', 'minSize'].forEach(id => {
  $(`#${id}`).addEventListener('change', () => {
    chrome.runtime.sendMessage({
      type: 'update_settings',
      settings: gatherSettings()
    });
  });
});

// Save custom token if changed
$('#tokenInput').addEventListener('change', () => {
  const token = $('#tokenInput').value.trim();
  if (token) {
    chrome.storage.local.set({ superidm_rpc_token: token });
    chrome.runtime.sendMessage({ type: 'update_settings', token });
  }
});

// ═══════ Quick Download ═══════
$('#dlBtn').addEventListener('click', () => {
  const url = $('#urlInput').value.trim();
  if (!url) return;
  chrome.runtime.sendMessage({ type: 'download', url }, (resp) => {
    if (resp && resp.ok) {
      $('#urlInput').value = '';
      $('#urlInput').placeholder = '✅ Sent to SuperIDM!';
      setTimeout(() => { $('#urlInput').placeholder = 'Paste URL here...'; }, 2000);
    } else {
      $('#urlInput').placeholder = '❌ SuperIDM is not running';
      setTimeout(() => { $('#urlInput').placeholder = 'Paste URL here...'; }, 2000);
    }
  });
});

$('#urlInput').addEventListener('keydown', (e) => {
  if (e.key === 'Enter') $('#dlBtn').click();
});

// ═══════ Media List ═══════
function renderMedia(mediaList) {
  const container = $('#mediaList');
  const count = $('#mediaCount');
  count.textContent = mediaList.length;

  if (mediaList.length === 0) {
    container.innerHTML = '<div class="media-empty">No media detected on this tab</div>';
    return;
  }

  container.innerHTML = '';
  mediaList.forEach(m => {
    const item = document.createElement('div');
    item.className = 'media-item';

    // Extract short filename from URL
    let shortUrl = m.url;
    try {
      const urlObj = new URL(m.url);
      shortUrl = urlObj.pathname.split('/').pop() || urlObj.hostname;
    } catch (e) {}

    item.innerHTML = `
      <div class="media-item-info">
        <div class="media-item-url" title="${m.url}">${shortUrl}</div>
        <div class="media-item-type">${m.type || 'media'}</div>
      </div>
    `;

    const btn = document.createElement('button');
    btn.className = 'media-dl-btn';
    btn.textContent = '⚡ DL';
    btn.addEventListener('click', () => {
      chrome.runtime.sendMessage({ type: 'download', url: m.url }, (resp) => {
        btn.textContent = (resp && resp.ok) ? '✅' : '❌';
        setTimeout(() => { btn.textContent = '⚡ DL'; }, 1500);
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

// Listen for live status updates
chrome.runtime.onMessage.addListener((msg) => {
  if (msg.type === 'status') updateStatus(msg.connected);
});
