// ═══════════════════════════════════════════════════════════════
// SuperIDM Browser Extension — Content Script (Floating Video Grabber)
// ═══════════════════════════════════════════════════════════════

(function() {
  'use strict';

  const BADGE_CLASS = 'superidm-video-badge';
  const MENU_CLASS = 'superidm-quality-menu';
  let showGrabber = true;
  let appConnected = false;  // Track whether the desktop app is running
  let extSettings = {
    interceptClicks: true,
    fileExtensions: '3GP 7Z AAC ACE AIF ARJ ASF AVI BIN BZ2 DEB DMG DOCX EXE FLAC FLV GZ ISO LZH M4A M4V MKV MOV MP3 MP4 MPG MSI OGG PDF PPTX RAR RPM TAR TORRENT WAV WMA WMV XAPK XLSX ZIP',
    excludedDomains: ''
  };

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
      if (msg.settings) Object.assign(extSettings, msg.settings);
      showGrabber = msg.settings ? msg.settings.showVideoGrabber !== false : msg.showVideoGrabber !== false;
      updateAllBadgeVisibility();
    }
    if (msg.type === 'connection_status') {
      appConnected = msg.connected;
      updateAllBadgeVisibility();
    }
    if (msg.type === 'media_detected') {
      scanVideos();
      attachPlatformBadge();
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
  const KNOWN_PLATFORMS = [
    // Major Video
    'youtube.com', 'youtu.be', 'youtube-nocookie.com',
    'twitter.com', 'x.com',
    'tiktok.com',
    'instagram.com', 'instagr.am',
    'facebook.com', 'fb.watch', 'fb.com',
    'reddit.com', 'v.redd.it',
    'twitch.tv',
    'vimeo.com',
    'dailymotion.com',
    'kick.com',
    'rumble.com',
    'odysee.com', 'lbry.tv',
    'bitchute.com',
    'rutube.ru',
    'ok.ru', 'odnoklassniki.ru',
    'vk.com', 'vk.video',
    'coub.com',
    'nimo.tv',
    // Audio
    'soundcloud.com',
    'bandcamp.com',
    'audiomack.com',
    'mixcloud.com',
    // News & Media
    'ted.com',
    'cnn.com',
    'bbc.co.uk', 'bbc.com',
    'aljazeera.com', 'aljazeera.net',
    // Asian
    'nicovideo.jp', 'nico.ms',
    'weibo.com',
    'iqiyi.com',
    'youku.com',
    'v.qq.com',
    'mgtv.com',
    // Arab & Regional
    'shahid.mbc.net', 'shahid.net',
    'cinemana.shabakaty.com',
    'anghami.com',
    // Video & File Hosting
    'streamable.com',
    'vidio.com',
    'loom.com',
    'clipchamp.com',
    'streamtape.com', 'streamlare.com',
    'doodstream.com', 'dood.',
    'vidoza.net',
    'mp4upload.com',
    'mixdrop.co', 'mixdrop.to',
    // Social & Misc
    'espn.com',
    'mlb.com',
    'pinterest.com', 'pin.it',
    'tumblr.com',
    'imgur.com',
    '9gag.com',
    'linkedin.com',
    'snapchat.com',
    'threads.net'
  ];

  function detectPlatform() {
    const host = location.hostname.toLowerCase();
    if (host.includes('youtube.com') || host.includes('youtu.be')) return 'youtube';
    if (host.includes('twitter.com') || host.includes('x.com')) return 'twitter';
    if (host.includes('tiktok.com')) return 'tiktok';
    if (host.includes('instagram.com') || host.includes('instagr.am')) return 'instagram';
    if (host.includes('facebook.com') || host.includes('fb.com')) return 'facebook';
    if (host.includes('reddit.com')) return 'reddit';
    if (host.includes('twitch.tv')) return 'twitch';
    if (host.includes('vimeo.com')) return 'vimeo';
    if (host.includes('dailymotion.com')) return 'dailymotion';
    if (host.includes('cinemana.shabakaty.com')) return 'cinemana';
    if (host.includes('shahid.mbc.net') || host.includes('shahid.net')) return 'shahid';
    return KNOWN_PLATFORMS.some(p => host.includes(p)) ? 'known_extractor' : 'generic';
  }

  // ═══════ Clean & Sanitize Video Title ═══════
  function sanitizeTitle(str) {
    if (!str) return 'video';
    let s = str
      .replace(/^\(\d+\)\s*/, '')   // strip "(3) " notification badge
      .replace(/^\[\d+\]\s*/, '')   // strip "[1] "
      .replace(/[\\/:*?"<>|\r\n\t]+/g, ' ')
      .replace(/\s+/g, ' ')
      .trim();
    return s || 'video';
  }

  // ═══════ Get Video Title ═══════
  function getVideoTitle() {
    // 1. YouTube Specific
    const ytTitle = document.querySelector('h1.ytd-watch-metadata, #title h1, h1.title, ytd-watch-metadata h1');
    if (ytTitle && ytTitle.textContent.trim()) {
      return sanitizeTitle(ytTitle.textContent.trim());
    }

    // 2. Instagram Specific
    if (detectPlatform() === 'instagram') {
      let reelId = '';
      const pathMatch = location.pathname.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]+)/i);
      if (pathMatch) reelId = pathMatch[2];

      const ogMeta = document.querySelector('meta[property="og:title"], meta[name="twitter:title"]');
      if (ogMeta && ogMeta.content) {
        const ogText = ogMeta.content.trim().replace(/^\(\d+\)\s*/, '');
        const m = ogText.match(/^(.*?)\s+on Instagram:\s*["“](.*?)["”]/i);
        if (m) {
          const author = m[1].trim();
          const caption = m[2].trim().slice(0, 60);
          const t = sanitizeTitle(caption ? `${author} - ${caption}` : author);
          if (t && t !== 'video') return t;
        }
        const cleanedOg = ogText.replace(/\s*•\s*Instagram.*$/i, '').replace(/\s*[-|–—]\s*Instagram.*$/i, '').trim();
        const t = sanitizeTitle(cleanedOg);
        if (t && t !== 'video' && !/^Instagram$/i.test(t)) return t;
      }

      // Check author / caption in DOM
      const authorEl = document.querySelector('article header h2, article header a, header h2, [role="main"] header a, a[role="link"][tabindex="0"]');
      const author = authorEl ? authorEl.textContent.trim().replace(/^@/, '') : '';
      const captionEl = document.querySelector('article h1, [role="main"] h1, article span[dir="auto"]');
      const caption = captionEl ? captionEl.textContent.trim().slice(0, 50) : '';

      if (author && caption) {
        const t = sanitizeTitle(`${author} - ${caption}`);
        if (t && t !== 'video') return t;
      }
      if (author) return sanitizeTitle(reelId ? `${author}_${reelId}` : `Instagram_${author}`);
      if (reelId) return `Instagram_Reel_${reelId}`;
    }

    // 3. OpenGraph / Twitter Meta Tags (widely used across movie/streaming sites like Cinemana)
    const ogTitle = document.querySelector('meta[property="og:title"], meta[name="twitter:title"], meta[name="title"]');
    if (ogTitle && ogTitle.content && ogTitle.content.trim() && ogTitle.content.trim().length > 2) {
      let og = ogTitle.content.trim()
        .replace(/^\(\d+\)\s*/, '')
        .replace(/\s*[-|–—]\s*(Cinemana|شبكتي|سينمانا|YouTube|EgyBest|ايجي بست|Akwam|اكوام|Shahid|شاهد|FaselHD|فاصل اعلاني|Cima4U|سيما فور يو|عرب سيد|Arabseed).*$/i, '')
        .trim();
      const t = sanitizeTitle(og);
      if (t && t !== 'video' && !/^(Instagram|Facebook|YouTube|Twitter|TikTok)$/i.test(t)) return t;
    }

    // 4. Cinemana & Movie Platform Specific Title Elements
    const streamingTitle = document.querySelector('.video-title, .video_title, .movie-title, .ar_title, .en_title, .video-header h1, .video-details h1, .video-details h2, .details-title, .film-title, h1');
    if (streamingTitle && streamingTitle.textContent.trim()) {
      let title = streamingTitle.textContent.trim();
      const epEl = document.querySelector('.episode-title, .episode-number, .active-episode, [class*="episode"].active, [class*="episode"][class*="selected"]');
      if (epEl && epEl.textContent.trim() && !title.includes(epEl.textContent.trim())) {
        title += ' - ' + epEl.textContent.trim();
      }
      if (title.length > 2) return sanitizeTitle(title);
    }

    // 5. Clean document.title
    let docTitle = document.title || 'video';
    docTitle = docTitle
      .replace(/^\(\d+\)\s*/, '')
      .replace(/^\[\d+\]\s*/, '')
      .replace(/\s*[-|–—]\s*(Cinemana|شبكتي|سينمانا|YouTube|EgyBest|ايجي بست|Akwam|اكوام|Shahid|شاهد|FaselHD|فاصل اعلاني|Cima4U|سيما فور يو|عرب سيد|Arabseed).*$/i, '')
      .replace(/\s*•\s*Instagram.*$/i, '')
      .replace(/\s*[-|–—]\s*Instagram.*$/i, '')
      .replace(/^(Watch|مشاهدة|تحميل|Download)\s+/i, '')
      .trim();

    const finalDocTitle = sanitizeTitle(docTitle || 'video');
    if (!finalDocTitle || /^(Instagram|Facebook|YouTube|Twitter|TikTok|video)$/i.test(finalDocTitle)) {
      if (detectPlatform() === 'instagram') {
        const reelMatch = location.pathname.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]+)/i);
        if (reelMatch) return `Instagram_${reelMatch[2]}`;
        return `Instagram_Video_${Date.now().toString().slice(-4)}`;
      }
    }

    return finalDocTitle;
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

  // ═══════ Extract the best Instagram permalink for the current video ═══════
  async function getInstagramPermalink(videoEl) {
    if (!videoEl) return null;

    // 1. Direct dataset set by instagram_helper.js (runs in MAIN world)
    if (videoEl.dataset && videoEl.dataset.superidmUrl) {
      return videoEl.dataset.superidmUrl;
    }
    if (videoEl.dataset && videoEl.dataset.superidmCode) {
      return `https://www.instagram.com/reel/${videoEl.dataset.superidmCode}/`;
    }

    // 2. Request MAIN world helper with a 250ms timeout
    try {
      const reqId = Date.now().toString();
      videoEl.dataset.superidmReq = reqId;
      const helperPromise = new Promise(resolve => {
        const handler = (e) => {
          if (e.detail?.reqId === reqId) {
            document.removeEventListener('superidm:response_reel_info', handler);
            resolve(e.detail?.url || (e.detail?.code ? `https://www.instagram.com/reel/${e.detail.code}/` : null));
          }
        };
        document.addEventListener('superidm:response_reel_info', handler);
        setTimeout(() => {
          document.removeEventListener('superidm:response_reel_info', handler);
          resolve(null);
        }, 250);
      });

      document.dispatchEvent(new CustomEvent('superidm:request_reel_info', { detail: { reqId } }));
      const helperUrl = await helperPromise;
      if (helperUrl && /\/(reel|p)\/[A-Za-z0-9_-]+/i.test(helperUrl)) {
        return helperUrl;
      }
    } catch (e) {}

    // 3. If currently directly on a reel or post page with ID in address bar
    const pathMatch = location.pathname.match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
    if (pathMatch) {
      return `https://www.instagram.com/reel/${pathMatch[2]}/`;
    }

    // 4. Check modal dialog if open
    const dialog = videoEl.closest('div[role="dialog"]');
    if (dialog) {
      const dLinks = dialog.querySelectorAll('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"]');
      for (const dLink of dLinks) {
        const m = (dLink.getAttribute('href') || '').match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i);
        if (m) return `https://www.instagram.com/reel/${m[2]}/`;
      }
    }

    // 5. Check nearest article or post container
    const article = videoEl.closest('article, [data-blade-container="true"], [role="presentation"], section, div[tabindex]');
    if (article) {
      const aLinks = article.querySelectorAll('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"], a[href*="/tv/"]');
      for (const a of aLinks) {
        const m = (a.getAttribute('href') || '').match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
        if (m) return `https://www.instagram.com/reel/${m[2]}/`;
      }
    }

    // 6. Check parent hierarchy up to 25 levels
    let el = videoEl.parentElement;
    for (let i = 0; i < 25 && el && el !== document.body; i++) {
      const links = el.querySelectorAll('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"]');
      for (const link of links) {
        const m = (link.getAttribute('href') || '').match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
        if (m) return `https://www.instagram.com/reel/${m[2]}/`;
      }
      el = el.parentElement;
    }

    // 7. Vertical proximity search
    try {
      const vRect = videoEl.getBoundingClientRect();
      const allLinks = document.querySelectorAll('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"]');
      let closestCode = null;
      let minDistance = Infinity;
      allLinks.forEach(a => {
        const m = (a.getAttribute('href') || '').match(/\/(reel|reels|p|tv)\/([A-Za-z0-9_-]{9,15})/i);
        if (m) {
          const aRect = a.getBoundingClientRect();
          const dist = Math.abs((aRect.top + aRect.bottom) / 2 - (vRect.top + vRect.bottom) / 2);
          if (dist < minDistance) {
            minDistance = dist;
            closestCode = m[2];
          }
        }
      });
      if (closestCode && minDistance < 1500) {
        return `https://www.instagram.com/reel/${closestCode}/`;
      }
    } catch(e) {}

    // 8. OpenGraph meta tag fallback
    const ogMeta = document.querySelector('meta[property="og:url"]');
    if (ogMeta && ogMeta.content) {
      const m = ogMeta.content.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i);
      if (m) return `https://www.instagram.com/reel/${m[2]}/`;
    }

    return null;
  }

  // ═══════ Send Download Helper ═══════
  async function sendDownload(format, qualityLabel, videoEl) {
    const title = getVideoTitle();
    const platform = detectPlatform();
    const filename = `${title}.${qualityLabel === 'MP3' ? 'mp3' : 'mp4'}`;
    let targetUrl = location.href;

    // 1. Direct video src on element
    let directSrc = videoEl ? (videoEl.currentSrc || videoEl.src) : null;
    if (!directSrc || directSrc.startsWith('blob:')) {
      const sourceEl = videoEl?.querySelector('source[src]');
      if (sourceEl && sourceEl.src) directSrc = sourceEl.src;
    }

    // 2. Background sniffed media streams for this tab
    let sniffedUrl = null;
    try {
      const tabMediaResp = await new Promise(res => {
        chrome.runtime.sendMessage({ type: 'get_tab_media' }, res);
      });
      if (tabMediaResp && tabMediaResp.media && tabMediaResp.media.length > 0) {
        const mList = tabMediaResp.media;
        const best = mList.slice().reverse().find(m => /\.(m3u8|mpd|mp4)(\?|$)/i.test(m.url)) || mList[mList.length - 1];
        if (best && best.url) sniffedUrl = best.url;
      }
    } catch (e) {}

    // 3. Smart platform & URL resolution
    const host = location.hostname.toLowerCase();
    const isKnownYtDlp = KNOWN_PLATFORMS.some(p => host.includes(p));

    if (platform === 'instagram') {
      const permalink = await getInstagramPermalink(videoEl);
      if (permalink && /\/(reel|reels|p|tv)\/[A-Za-z0-9_-]{9,15}/i.test(permalink)) {
        targetUrl = permalink;
      } else if (directSrc && !directSrc.startsWith('blob:')) {
        targetUrl = directSrc;
      } else if (sniffedUrl) {
        targetUrl = sniffedUrl;
      } else {
        const playing = Array.from(document.querySelectorAll('video')).find(v => !v.paused && v.currentSrc && !v.currentSrc.startsWith('blob:'));
        if (playing) {
          targetUrl = playing.currentSrc;
        } else if (location.pathname.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i)) {
          targetUrl = location.href;
        } else if (sniffedUrl) {
          targetUrl = sniffedUrl;
        } else {
          showToast('❌ Could not locate Instagram video stream');
          return;
        }
      }
    } else if (isKnownYtDlp) {
      targetUrl = location.href;
    } else {
      if (sniffedUrl) {
        targetUrl = sniffedUrl;
      } else if (directSrc && !directSrc.startsWith('blob:')) {
        targetUrl = directSrc;
      } else {
        targetUrl = location.href;
      }
    }

    showToast(`Starting download in SuperIDM...`);

    chrome.runtime.sendMessage({
      type: 'download_video',
      url: targetUrl,
      filename: filename,
      format: format || 'bestvideo+bestaudio/best'
    }, (resp) => {
      if (resp && resp.ok) {
        showToast(`Downloading ${qualityLabel || 'Video'}: "${title.substring(0, 25)}..."`);
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

  // ═══════ Attach Badge to Video or Audio ═══════
  function attachBadge(videoEl) {
    if (!videoEl || videoEl.dataset.superidmBadge) return;
    videoEl.dataset.superidmBadge = 'true';

    const platform = detectPlatform();
    const isAudio = videoEl.tagName === 'AUDIO';

    // Find or create a positioned container
    let container = null;
    if (platform === 'instagram') {
      // Find the reel slide/card container that wraps both the video and the gesture overlays
      container = videoEl.closest('article, [data-blade-container="true"], section, div[tabindex="-1"], div[tabindex="0"]');
      if (!container) {
        let el = videoEl.parentElement;
        let best = el;
        const vRect = videoEl.getBoundingClientRect();
        while (el && el !== document.body && el !== document.documentElement) {
          const r = el.getBoundingClientRect();
          if (r.width >= vRect.width * 0.85 && r.height >= vRect.height * 0.85) {
            best = el;
            if (el.querySelector('[role="button"]') || el.tagName === 'ARTICLE' || el.tagName === 'SECTION') {
              break;
            }
          }
          el = el.parentElement;
        }
        container = best || videoEl.parentElement;
      }
    } else {
      container = videoEl.closest('.html5-video-container, .video-player, .media-container, [class*="player"], [class*="video"], [class*="stream"]');
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
    badge.className = BADGE_CLASS + (isAudio ? ' superidm-audio-badge' : '');
    badge.innerHTML = `<div class="superidm-btn-main">${ARROW_SVG}<span class="superidm-btn-text">${isAudio ? 'Download Audio' : 'Download'}</span></div><div class="superidm-btn-chevron">${CHEVRON_SVG}</div>`;
    badge.style.display = shouldShowBadges() ? '' : 'none';
    badge.style.pointerEvents = 'auto';
    badge.style.zIndex = '2147483647';

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
      sendDownload(isAudio ? 'bestaudio/best' : null, isAudio ? 'MP3' : 'Video', videoEl);
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

  // ═══════ Window Capture-Phase Interceptor ═══════
  // Instagram reels feeds place transparent tap-to-play overlays or gesture catchers.
  // By intercepting events at window level during CAPTURE phase (useCapture = true),
  // we catch clicks inside the SuperIDM badge before Instagram's listeners can stop them!
  ['click', 'pointerdown', 'mousedown'].forEach(eventType => {
    window.addEventListener(eventType, (e) => {
      const badges = document.querySelectorAll('.' + BADGE_CLASS);
      for (const badge of badges) {
        if (badge.offsetParent === null || badge.style.display === 'none') continue;
        const rect = badge.getBoundingClientRect();

        const menu = badge.querySelector('.' + MENU_CLASS);
        const isMenuOpen = menu && menu.classList.contains('superidm-menu-visible');
        let inMenu = false;
        if (isMenuOpen) {
          const menuRect = menu.getBoundingClientRect();
          if (e.clientX >= menuRect.left && e.clientX <= menuRect.right &&
              e.clientY >= menuRect.top && e.clientY <= menuRect.bottom) {
            inMenu = true;
          }
        }

        const inBadge = e.clientX >= rect.left && e.clientX <= rect.right &&
                        e.clientY >= rect.top && e.clientY <= rect.bottom;

        if (inBadge || inMenu) {
          e.stopPropagation();
          e.stopImmediatePropagation();
          e.preventDefault();

          if (eventType === 'click') {
            if (inMenu) {
              const items = menu.querySelectorAll('.superidm-menu-item');
              for (const item of items) {
                const iRect = item.getBoundingClientRect();
                if (e.clientY >= iRect.top && e.clientY <= iRect.bottom) {
                  item.click();
                  return;
                }
              }
              return;
            }

            const chevron = badge.querySelector('.superidm-btn-chevron');
            const chevronRect = chevron ? chevron.getBoundingClientRect() : null;
            if (chevronRect && e.clientX >= chevronRect.left && e.clientX <= chevronRect.right) {
              const open = menu.classList.toggle('superidm-menu-visible');
              badge.classList.toggle('superidm-menu-open', open);
            } else {
              const mainBtn = badge.querySelector('.superidm-btn-main');
              if (mainBtn) {
                mainBtn.click();
              }
            }
          }
          return;
        }
      }
    }, true);
  });

  // ═══════ Scan for Videos & Audio ═══════
  function scanVideos() {
    const mediaEls = document.querySelectorAll('video, audio');
    mediaEls.forEach(v => {
      const rect = v.getBoundingClientRect();
      const isVisible = (rect.width > 80 && rect.height > 50) || v.videoWidth > 0 || !v.paused || v.readyState > 0 || v.currentTime > 0;
      if (isVisible) {
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

  // ═══════ Immediate Media Playback Catchers ═══════
  // Whenever any media element plays or is hovered anywhere on the page, immediately attach grabber
  document.addEventListener('play', (e) => {
    if (e.target && (e.target.tagName === 'VIDEO' || e.target.tagName === 'AUDIO')) {
      attachBadge(e.target);
    }
  }, true);

  document.addEventListener('playing', (e) => {
    if (e.target && (e.target.tagName === 'VIDEO' || e.target.tagName === 'AUDIO')) {
      attachBadge(e.target);
    }
  }, true);

  document.addEventListener('mouseenter', (e) => {
    if (e.target && (e.target.tagName === 'VIDEO' || e.target.tagName === 'AUDIO')) {
      attachBadge(e.target);
    }
  }, true);

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

  // ═══════ Instant Click Interceptor (IDM Style) ═══════
  function getCleanFileName(urlStr, explicitFilename) {
    if (explicitFilename && explicitFilename.trim()) {
      return explicitFilename.trim();
    }
    try {
      const u = new URL(urlStr);
      const pathname = u.pathname;
      const lastSeg = pathname.substring(pathname.lastIndexOf('/') + 1);
      if (lastSeg && lastSeg.includes('.')) {
        return decodeURIComponent(lastSeg).replace(/[\\/:*?"<>|\r\n\t]+/g, '_');
      }
    } catch (e) {}
    return null;
  }

  function shouldInterceptUrl(urlStr, downloadAttr) {
    if (!urlStr || typeof urlStr !== 'string') return false;
    const lowerUrl = urlStr.trim().toLowerCase();

    // Skip in-page anchors, javascript, mailto, tel
    if (lowerUrl.startsWith('javascript:') || lowerUrl.startsWith('#') || lowerUrl.startsWith('mailto:') || lowerUrl.startsWith('tel:')) {
      return false;
    }
    // Skip browser internal protocols
    if (lowerUrl.startsWith('chrome:') || lowerUrl.startsWith('edge:') || lowerUrl.startsWith('about:') || lowerUrl.startsWith('chrome-extension:')) {
      return false;
    }
    if ((lowerUrl.startsWith('blob:') || lowerUrl.startsWith('data:')) && !downloadAttr) {
      return false;
    }

    // Check if domain is excluded
    if (extSettings.excludedDomains) {
      try {
        const urlHost = new URL(urlStr, location.href).hostname.toLowerCase();
        const currentHost = location.hostname.toLowerCase();
        const excludedList = extSettings.excludedDomains.split(/[\s,;\n]+/).map(d => d.trim().toLowerCase()).filter(Boolean);
        if (excludedList.some(d => urlHost === d || urlHost.endsWith('.' + d) || currentHost === d || currentHost.endsWith('.' + d))) {
          return false;
        }
      } catch (e) {}
    }

    // If explicit download attribute is present
    if (downloadAttr) {
      return true;
    }

    // Match file extension from URL
    try {
      const u = new URL(urlStr, location.href);
      const cleanPath = u.pathname.split(/[?#]/)[0];
      const dotIndex = cleanPath.lastIndexOf('.');
      if (dotIndex !== -1) {
        const ext = cleanPath.substring(dotIndex + 1).toUpperCase();
        if (ext && ext.length >= 2 && ext.length <= 10) {
          const allowedExts = (extSettings.fileExtensions || '')
            .split(/[\s,;]+/)
            .map(e => e.trim().replace(/^\./, '').toUpperCase())
            .filter(Boolean);
          if (allowedExts.includes(ext)) {
            return true;
          }
        }
      }
    } catch (e) {}

    return false;
  }

  function handleLinkClick(e) {
    // Only intercept when desktop app is connected and click interception is enabled
    if (!appConnected || extSettings.interceptClicks === false) return;

    // 1. Bypass Key: Alt key held down -> BYPASS SuperIDM and let browser download naturally
    if (e.altKey) {
      return; // Standard IDM Alt bypass behavior
    }

    // Find closest anchor tag
    const anchor = e.target.closest('a[href]');
    if (!anchor) return;

    const href = anchor.href;
    if (!href) return;
    const downloadAttr = anchor.getAttribute('download');

    // 2. Force Key: Ctrl / Meta key held down -> FORCE download through SuperIDM for any http(s) link!
    const forceIntercept = (e.ctrlKey || e.metaKey) && (href.startsWith('http://') || href.startsWith('https://'));

    if (!forceIntercept && !shouldInterceptUrl(href, downloadAttr)) {
      return;
    }

    // Intercept: Prevent default navigation and internal browser download
    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();

    const filename = getCleanFileName(href, downloadAttr);
    const displayTitle = filename || anchor.textContent.trim().slice(0, 30) || 'Download';

    // Show stylish instant floating toast
    showToast(`⚡ SuperIDM: جاري التحميل "${displayTitle}" (اضغط Alt للتجاوز)`);

    // Send to background service worker
    chrome.runtime.sendMessage({
      type: 'intercept_click_download',
      url: href,
      filename: filename || undefined,
      referrer: location.href
    }, (resp) => {
      if (resp && resp.ok) {
        // Successfully handed over to desktop app
      } else {
        showToast(`❌ ${resp?.error || 'SuperIDM desktop is not responding'}`);
        // Fallback to native navigation after small delay if desktop engine failed
        setTimeout(() => {
          window.location.href = href;
        }, 800);
      }
    });
  }

  // Attach click & auxclick listeners on document in CAPTURE phase
  document.addEventListener('click', handleLinkClick, true);
  document.addEventListener('auxclick', handleLinkClick, true);

  // ═══════ Init ═══════
  function init() {
    chrome.runtime.sendMessage({ type: 'get_status' }, (resp) => {
      if (resp) {
        if (resp.settings) {
          Object.assign(extSettings, resp.settings);
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
