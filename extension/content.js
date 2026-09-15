// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Content Script (Floating Video Grabber)
// ═══════════════════════════════════════════════════════════════

(function() {
  'use strict';

  const BADGE_CLASS = 'superidm-video-badge';
  const MENU_CLASS = 'superidm-quality-menu';
  let showGrabber = true;
  let appConnected = false;  // Track whether the desktop app is running

  // Helper: should badges be visible?
  function shouldShowBadges() {
    return showGrabber && appConnected;
  }

  // Helper: update all badge visibility based on current state
  function updateAllBadgeVisibility() {
    const show = shouldShowBadges();
    document.querySelectorAll('.' + BADGE_CLASS).forEach(b => {
      b.style.display = show ? '' : 'none';
      if (!show) {
        b.classList.remove('superidm-menu-open');
        const m = b.querySelector('.' + MENU_CLASS);
        if (m) m.classList.remove('superidm-menu-visible');
      }
    });
  }

  // Listen for messages from background script
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === 'settings_update') {
      showGrabber = msg.showVideoGrabber;
      updateAllBadgeVisibility();
    }
    if (msg.type === 'connection_status') {
      appConnected = msg.connected;
      updateAllBadgeVisibility();
    }
  });

  // Active status poller: query background every 1500ms so badges vanish instantly if app is closed
  setInterval(() => {
    try {
      chrome.runtime.sendMessage({ type: 'get_status' }, (resp) => {
        if (chrome.runtime.lastError || !resp) {
          if (appConnected) {
            appConnected = false;
            updateAllBadgeVisibility();
          }
        } else {
          const isConn = !!resp.connected;
          if (appConnected !== isConn) {
            appConnected = isConn;
            updateAllBadgeVisibility();
          }
        }
      });
    } catch (e) {
      if (appConnected) {
        appConnected = false;
        updateAllBadgeVisibility();
      }
    }
  }, 1500);

  // ═══════ Detect Platform ═══════
  function detectPlatform() {
    const host = location.hostname;
    if (host.includes('youtube.com') || host.includes('youtu.be')) return 'youtube';
    if (host.includes('twitter.com') || host.includes('x.com')) return 'twitter';
    if (host.includes('tiktok.com')) return 'tiktok';
    if (host.includes('instagram.com')) return 'instagram';
    if (host.includes('facebook.com') || host.includes('fb.com')) return 'facebook';
    if (host.includes('reddit.com')) return 'reddit';
    if (host.includes('twitch.tv')) return 'twitch';
    if (host.includes('vimeo.com')) return 'vimeo';
    if (host.includes('dailymotion.com')) return 'dailymotion';
    return 'generic';
  }

  // ═══════ Clean & Sanitize Video Title ═══════
  function sanitizeTitle(str) {
    if (!str) return 'video';
    return str
      .replace(/[\\/:*?"<>|\r\n\t]+/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
  }

  // ═══════ Get Video Title ═══════
  function getVideoTitle() {
    // 1. YouTube Specific
    const ytTitle = document.querySelector('h1.ytd-watch-metadata, #title h1, h1.title, ytd-watch-metadata h1');
    if (ytTitle && ytTitle.textContent.trim()) {
      return sanitizeTitle(ytTitle.textContent.trim());
    }

    // 2. OpenGraph / Twitter Meta Tags (widely used across movie/streaming sites like Cinemana)
    const ogTitle = document.querySelector('meta[property="og:title"], meta[name="twitter:title"], meta[name="title"]');
    if (ogTitle && ogTitle.content && ogTitle.content.trim() && ogTitle.content.trim().length > 2) {
      let og = ogTitle.content.trim()
        .replace(/\s*[-|–—]\s*(Cinemana|شبكتي|سينمانا|YouTube|EgyBest|ايجي بست|Akwam|اكوام|Shahid|شاهد|FaselHD|فاصل اعلاني|Cima4U|سيما فور يو|عرب سيد|Arabseed).*$/i, '')
        .trim();
      if (og.length > 2) return sanitizeTitle(og);
    }

    // 3. Cinemana & Movie Platform Specific Title Elements
    const streamingTitle = document.querySelector('.video-title, .video_title, .movie-title, .ar_title, .en_title, .video-header h1, .video-details h1, .video-details h2, .details-title, .film-title, h1');
    if (streamingTitle && streamingTitle.textContent.trim()) {
      let title = streamingTitle.textContent.trim();
      const epEl = document.querySelector('.episode-title, .episode-number, .active-episode, [class*="episode"].active, [class*="episode"][class*="selected"]');
      if (epEl && epEl.textContent.trim() && !title.includes(epEl.textContent.trim())) {
        title += ' - ' + epEl.textContent.trim();
      }
      if (title.length > 2) return sanitizeTitle(title);
    }

    // 4. Clean document.title
    let docTitle = document.title || 'video';
    docTitle = docTitle
      .replace(/\s*[-|–—]\s*(Cinemana|شبكتي|سينمانا|YouTube|EgyBest|ايجي بست|Akwam|اكوام|Shahid|شاهد|FaselHD|فاصل اعلاني|Cima4U|سيما فور يو|عرب سيد|Arabseed).*$/i, '')
      .replace(/^(Watch|مشاهدة|تحميل|Download)\s+/i, '')
      .trim();

    return sanitizeTitle(docTitle || 'video');
  }

  // SVG Clean Cyan Download Arrow Icon
  const ARROW_SVG = `
    <svg class="superidm-icon-svg" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M12 3V14.5M12 14.5L7.5 10M12 14.5L16.5 10" stroke="#00f5ff" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"/>
      <path d="M5 19H19" stroke="#00f5ff" stroke-width="2.4" stroke-linecap="round"/>
    </svg>
  `;

  // SVG Dropdown Chevron Icon
  const CHEVRON_SVG = `
    <svg class="superidm-chevron-svg" viewBox="0 0 24 24" fill="none" stroke="#00f5ff" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="6 9 12 15 18 9"></polyline>
    </svg>
  `;

  // ═══════ Platforms that yt-dlp handles natively via page URL ═══════
  const YTDLP_PAGE_PLATFORMS = ['youtube', 'instagram', 'tiktok', 'twitter', 'facebook', 'reddit', 'vimeo', 'dailymotion', 'twitch'];

  // ═══════ Extract the best Instagram permalink for the current video ═══════
  function getInstagramPermalink(videoEl) {
    if (!videoEl) return location.href;
    // 1. Check parent hierarchy for any link to reel or post
    let el = videoEl.parentElement;
    for (let i = 0; i < 15 && el && el !== document.body; i++) {
      const link = el.querySelector('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"]');
      if (link && link.href) return link.href;
      el = el.parentElement;
    }
    // 2. Check nearest article
    const article = videoEl.closest('article');
    if (article) {
      const timeLink = article.querySelector('a[href*="/reel/"], a[href*="/p/"], a[href*="/tv/"]');
      if (timeLink) return new URL(timeLink.href, location.origin).href;
    }
    // 3. Check current page pathname
    if (/\/(reel|reels|p|tv)\/[A-Za-z0-9_-]+/.test(location.pathname)) {
      return location.href;
    }
    return location.href;
  }

  // ═══════ Send Download Helper ═══════
  async function sendDownload(format, qualityLabel, videoEl) {
    const title = getVideoTitle();
    const platform = detectPlatform();
    const filename = `${title}.${qualityLabel === 'MP3' ? 'mp3' : 'mp4'}`;
    let targetUrl = location.href;

    // Direct video src on element
    let directSrc = videoEl ? (videoEl.currentSrc || videoEl.src) : null;
    if (!directSrc || directSrc.startsWith('blob:')) {
      const sourceEl = videoEl?.querySelector('source[src]');
      if (sourceEl && sourceEl.src) directSrc = sourceEl.src;
    }

    // Background sniffed media streams for this tab
    let sniffedUrl = null;
    try {
      const tabMediaResp = await new Promise(res => {
        chrome.runtime.sendMessage({ type: 'get_tab_media' }, res);
      });
      if (tabMediaResp && tabMediaResp.media && tabMediaResp.media.length > 0) {
        const latest = tabMediaResp.media[tabMediaResp.media.length - 1];
        if (latest && latest.url) sniffedUrl = latest.url;
      }
    } catch (e) {}

    // Special handling for Instagram: prefer direct MP4 CDN url or sniffed stream to avoid login barriers
    if (platform === 'instagram') {
      if (directSrc && !directSrc.startsWith('blob:')) {
        targetUrl = directSrc;
      } else if (sniffedUrl) {
        targetUrl = sniffedUrl;
      } else {
        targetUrl = getInstagramPermalink(videoEl);
      }
    } else if (YTDLP_PAGE_PLATFORMS.includes(platform)) {
      // For YouTube, TikTok, Twitter, etc., yt-dlp natively handles page URL
      targetUrl = location.href;
    } else {
      // Generic direct URL
      if (sniffedUrl || (directSrc && !directSrc.startsWith('blob:'))) {
        targetUrl = sniffedUrl || directSrc;
      }
    }

    showToast(`⚡ Starting download in SuperIDM...`);

    chrome.runtime.sendMessage({
      type: 'download_video',
      url: targetUrl,
      filename: filename,
      format: format || 'bestvideo+bestaudio/best'
    }, (resp) => {
      if (resp && resp.ok) {
        showToast(`⚡ Downloading ${qualityLabel || 'Video'}: "${title.substring(0, 25)}..."`);
      } else {
        showToast(`❌ ${resp?.error || 'SuperIDM is not running'}`);
      }
    });
  }

  // ═══════ Create Quality Menu ═══════
  function createQualityMenu(videoEl) {
    const menu = document.createElement('div');
    menu.className = MENU_CLASS;

    const header = document.createElement('div');
    header.className = 'superidm-menu-header';
    header.innerHTML = '<span>Select Quality</span><span>SuperIDM</span>';
    menu.appendChild(header);

    const formats = [
      { name: '4K Ultra HD', tag: '2160p', format: 'bestvideo[height<=2160]+bestaudio/best' },
      { name: 'Full HD Quality', tag: '1080p', format: 'bestvideo[height<=1080]+bestaudio/best' },
      { name: 'High Definition', tag: '720p', format: 'bestvideo[height<=720]+bestaudio/best' },
      { name: 'Standard Quality', tag: '480p', format: 'bestvideo[height<=480]+bestaudio/best' },
      { name: 'Audio Stream', tag: 'MP3', format: 'bestaudio/best' },
    ];

    formats.forEach(({ name, tag, format }) => {
      const item = document.createElement('div');
      item.className = 'superidm-menu-item';
      item.innerHTML = `<span>${name}</span><span class="superidm-item-tag">${tag}</span>`;
      item.addEventListener('click', (e) => {
        e.stopPropagation();
        e.stopImmediatePropagation();
        e.preventDefault();
        sendDownload(format, tag, videoEl);
        menu.classList.remove('superidm-menu-visible');
        const badge = menu.closest('.' + BADGE_CLASS);
        if (badge) badge.classList.remove('superidm-menu-open');
      });
      menu.appendChild(item);
    });

    return menu;
  }

  // ═══════ Toast Notification ═══════
  function showToast(msg) {
    let toast = document.getElementById('superidm-toast');
    if (!toast) {
      toast = document.createElement('div');
      toast.id = 'superidm-toast';
      document.body.appendChild(toast);
    }
    toast.innerHTML = `${ARROW_SVG} <span>${msg}</span>`;
    toast.classList.add('superidm-toast-show');
    setTimeout(() => toast.classList.remove('superidm-toast-show'), 3500);
  }

  // ═══════ Attach Badge to Video ═══════
  function attachBadge(videoEl) {
    if (videoEl.dataset.superidmBadge) return;
    videoEl.dataset.superidmBadge = 'true';

    const platform = detectPlatform();

    // Find or create a positioned container
    let container;
    if (platform === 'instagram') {
      // Instagram wraps videos in deeply nested divs with overflow:hidden.
      // Walk up to the nearest <article> or a large enough ancestor that is
      // not clipping its children, so our badge stays visible and clickable.
      container = videoEl.closest('article');
      if (!container) {
        // Reels page may not use <article>; pick the first ancestor whose
        // bounding rect fully covers the video.
        let el = videoEl.parentElement;
        const vRect = videoEl.getBoundingClientRect();
        while (el && el !== document.body) {
          const r = el.getBoundingClientRect();
          if (r.width >= vRect.width && r.height >= vRect.height * 0.9) {
            container = el;
            break;
          }
          el = el.parentElement;
        }
      }
      if (!container) container = videoEl.parentElement;
    } else {
      container = videoEl.closest('.html5-video-container, .video-player, .media-container, [class*="player"], [class*="video"]');
      if (!container) container = videoEl.parentElement;
    }
    if (!container) return;

    // Ensure container is positioned
    const containerStyle = getComputedStyle(container);
    if (containerStyle.position === 'static') {
      container.style.position = 'relative';
    }

    // Create badge
    const badge = document.createElement('div');
    badge.className = BADGE_CLASS;
    badge.innerHTML = `<div class="superidm-btn-main">${ARROW_SVG}<span class="superidm-btn-text">Download</span></div><div class="superidm-btn-chevron">${CHEVRON_SVG}</div>`;
    badge.style.display = shouldShowBadges() ? '' : 'none';

    const menu = createQualityMenu(videoEl);
    badge.appendChild(menu);

    const mainBtn = badge.querySelector('.superidm-btn-main');
    const chevronBtn = badge.querySelector('.superidm-btn-chevron');

    // Direct Download Click (Immediate download)
    mainBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      e.stopImmediatePropagation();
      e.preventDefault();
      menu.classList.remove('superidm-menu-visible');
      badge.classList.remove('superidm-menu-open');
      sendDownload(null, 'Video', videoEl);
    }, true);

    // Chevron Click (Toggle Quality Menu)
    chevronBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      e.stopImmediatePropagation();
      e.preventDefault();
      const isOpen = menu.classList.toggle('superidm-menu-visible');
      badge.classList.toggle('superidm-menu-open', isOpen);
    }, true);

    container.appendChild(badge);
  }

  // ═══════ Scan for Videos ═══════
  function scanVideos() {
    const videos = document.querySelectorAll('video');
    videos.forEach(v => {
      const rect = v.getBoundingClientRect();
      if (rect.width > 200 && rect.height > 120) {
        attachBadge(v);
      }
    });
  }

  // ═══════ Platform-Specific Badge (YouTube, etc.) ═══════
  function attachPlatformBadge() {
    const platform = detectPlatform();

    if (platform === 'youtube') {
      const player = document.querySelector('#movie_player, ytd-player');
      if (player && !player.dataset.superidmBadge) {
        player.dataset.superidmBadge = 'true';
        const container = player.querySelector('.html5-video-container') || player;
        const containerStyle = getComputedStyle(container);
        if (containerStyle.position === 'static') container.style.position = 'relative';

        const badge = document.createElement('div');
        badge.className = BADGE_CLASS + ' superidm-yt-badge';
        badge.innerHTML = `<div class="superidm-btn-main">${ARROW_SVG}<span class="superidm-btn-text">Download</span></div><div class="superidm-btn-chevron">${CHEVRON_SVG}</div>`;
        badge.style.display = shouldShowBadges() ? '' : 'none';

        const videoEl = player.querySelector('video');
        const menu = createQualityMenu(videoEl);
        badge.appendChild(menu);

        const mainBtn = badge.querySelector('.superidm-btn-main');
        const chevronBtn = badge.querySelector('.superidm-btn-chevron');

        mainBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.preventDefault();
          menu.classList.remove('superidm-menu-visible');
          badge.classList.remove('superidm-menu-open');
          sendDownload(null, 'Video', videoEl);
        });

        chevronBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          e.preventDefault();
          const isOpen = menu.classList.toggle('superidm-menu-visible');
          badge.classList.toggle('superidm-menu-open', isOpen);
        });

        container.appendChild(badge);
      }
    }
  }

  // ═══════ Delegated Click Listener (Dismiss Active Menus) ═══════
  document.addEventListener('click', (e) => {
    if (!e.target.closest('.' + BADGE_CLASS)) {
      document.querySelectorAll('.' + MENU_CLASS + '.superidm-menu-visible').forEach(m => {
        m.classList.remove('superidm-menu-visible');
      });
      document.querySelectorAll('.' + BADGE_CLASS + '.superidm-menu-open').forEach(b => {
        b.classList.remove('superidm-menu-open');
      });
    }
  });

  // ═══════ MutationObserver for Dynamic Pages (Debounced with rAF) ═══════
  let observerRaf = null;
  const observer = new MutationObserver(() => {
    if (observerRaf) return;
    observerRaf = requestAnimationFrame(() => {
      scanVideos();
      attachPlatformBadge();
      observerRaf = null;
    });
  });

  // ═══════ Init ═══════
  function init() {
    chrome.runtime.sendMessage({ type: 'get_status' }, (resp) => {
      if (resp) {
        if (resp.settings) {
          showGrabber = resp.settings.showVideoGrabber !== false;
        }
        appConnected = !!resp.connected;
      }
      // Only scan/attach after we know the connection state
      scanVideos();
      attachPlatformBadge();
      updateAllBadgeVisibility();
    });

    observer.observe(document.body || document.documentElement, {
      childList: true,
      subtree: true
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
