// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Content Script (Floating Video Grabber)
// ═══════════════════════════════════════════════════════════════

(function() {
  'use strict';

  const BADGE_CLASS = 'superidm-video-badge';
  const MENU_CLASS = 'superidm-quality-menu';
  let showGrabber = true;

  // Listen for settings updates from background
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === 'settings_update') {
      showGrabber = msg.showVideoGrabber;
      document.querySelectorAll('.' + BADGE_CLASS).forEach(b => {
        b.style.display = showGrabber ? '' : 'none';
      });
    }
  });

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

  // ═══════ Send Download Helper ═══════
  async function sendDownload(format, qualityLabel, videoEl) {
    const title = getVideoTitle();
    const filename = `${title}.mp4`;
    let targetUrl = location.href;

    // Check direct video src on element
    let directSrc = videoEl ? (videoEl.currentSrc || videoEl.src) : null;
    if (!directSrc || directSrc.startsWith('blob:')) {
      const sourceEl = videoEl?.querySelector('source[src]');
      if (sourceEl && sourceEl.src) directSrc = sourceEl.src;
    }

    // Check background sniffed media streams for this tab
    let sniffedUrl = null;
    try {
      const tabMediaResp = await new Promise(res => {
        chrome.runtime.sendMessage({ type: 'get_tab_media' }, res);
      });
      if (tabMediaResp && tabMediaResp.media && tabMediaResp.media.length > 0) {
        // Take the latest valid media URL
        const latest = tabMediaResp.media[tabMediaResp.media.length - 1];
        if (latest && latest.url) sniffedUrl = latest.url;
      }
    } catch (e) {}

    const platform = detectPlatform();
    if (platform !== 'youtube' && (sniffedUrl || (directSrc && !directSrc.startsWith('blob:')))) {
      targetUrl = sniffedUrl || directSrc;
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

    // Find or create a positioned container
    let container = videoEl.closest('.html5-video-container, .video-player, .media-container, [class*="player"], [class*="video"]');
    if (!container) container = videoEl.parentElement;
    if (!container) return;

    // Ensure container is positioned
    const containerStyle = getComputedStyle(container);
    if (containerStyle.position === 'static') {
      container.style.position = 'relative';
    }

    // Create badge
    const badge = document.createElement('div');
    badge.className = BADGE_CLASS;
    badge.innerHTML = `${ARROW_SVG}<span class="superidm-btn-text">Download</span>${CHEVRON_SVG}`;
    badge.style.display = showGrabber ? '' : 'none';

    const menu = createQualityMenu(videoEl);
    badge.appendChild(menu);

    badge.addEventListener('click', (e) => {
      e.stopPropagation();
      e.preventDefault();
      // Toggle quality selection dropdown menu
      const isOpen = menu.classList.toggle('superidm-menu-visible');
      badge.classList.toggle('superidm-menu-open', isOpen);
    });

    // Close menu when clicking elsewhere
    document.addEventListener('click', () => {
      menu.classList.remove('superidm-menu-visible');
      badge.classList.remove('superidm-menu-open');
    });

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
        badge.innerHTML = `${ARROW_SVG}<span class="superidm-btn-text">Download</span>${CHEVRON_SVG}`;
        badge.style.display = showGrabber ? '' : 'none';

        const videoEl = player.querySelector('video');
        const menu = createQualityMenu(videoEl);
        badge.appendChild(menu);

        badge.addEventListener('click', (e) => {
          e.stopPropagation();
          e.preventDefault();
          const isOpen = menu.classList.toggle('superidm-menu-visible');
          badge.classList.toggle('superidm-menu-open', isOpen);
        });

        document.addEventListener('click', () => {
          menu.classList.remove('superidm-menu-visible');
          badge.classList.remove('superidm-menu-open');
        });

        container.appendChild(badge);
      }
    }
  }

  // ═══════ MutationObserver for Dynamic Pages ═══════
  const observer = new MutationObserver(() => {
    scanVideos();
    attachPlatformBadge();
  });

  // ═══════ Init ═══════
  function init() {
    chrome.runtime.sendMessage({ type: 'get_status' }, (resp) => {
      if (resp && resp.settings) {
        showGrabber = resp.settings.showVideoGrabber !== false;
      }
    });

    scanVideos();
    attachPlatformBadge();

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
