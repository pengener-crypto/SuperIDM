// ═══════════════════════════════════════════════════════════════
// SuperIDM — Instagram MAIN World Helper Script
// Runs directly in the page context to capture Instagram Reel data,
// shortcodes, and audio/video stream links directly from React Fiber
// and GraphQL/Clips network responses.
// ═══════════════════════════════════════════════════════════════

(function () {
  'use strict';
  if (window.__superidm_helper_installed) return;
  window.__superidm_helper_installed = true;

  const reelsStore = new Map(); // code -> { code, url, author, caption, videoUrl }
  const cdnToCode = new Map();  // cleanCdnUrl -> code

  function cleanUrl(u) {
    if (!u) return '';
    try {
      const parsed = new URL(u);
      return parsed.origin + parsed.pathname;
    } catch (e) {
      return String(u).split('?')[0];
    }
  }

  function processClipItem(item) {
    if (!item || typeof item !== 'object') return;
    const media = item.media || item.node || item.clips || item;
    const code = media.code || media.shortcode || item.code || item.shortcode;
    if (!code || typeof code !== 'string') return;

    const user = media.user || media.owner || {};
    const author = user.username || user.full_name || '';
    const captionObj = media.caption || {};
    const captionText = typeof captionObj === 'string' ? captionObj : (captionObj.text || '');

    let videoUrl = null;
    if (Array.isArray(media.video_versions) && media.video_versions.length > 0) {
      const sorted = media.video_versions.slice().sort((a, b) => (b.width * b.height) - (a.width * a.height));
      videoUrl = sorted[0]?.url;
    } else if (media.video_url) {
      videoUrl = media.video_url;
    }

    const reelUrl = `https://www.instagram.com/reel/${code}/`;
    const data = {
      code,
      reelUrl,
      author,
      caption: captionText.slice(0, 100),
      videoUrl
    };

    reelsStore.set(code, data);
    if (videoUrl) {
      cdnToCode.set(cleanUrl(videoUrl), code);
    }
  }

  function scanObjectForClips(obj, depth = 0) {
    if (!obj || depth > 8 || typeof obj !== 'object') return;
    if (Array.isArray(obj)) {
      for (const el of obj) scanObjectForClips(el, depth + 1);
      return;
    }
    if ((obj.code || obj.shortcode) && (obj.video_versions || obj.video_duration || obj.media_type === 2 || obj.product_type === 'clips' || obj.is_video)) {
      processClipItem(obj);
    }
    for (const k of Object.keys(obj)) {
      if (k === 'items' || k === 'clips' || k === 'edges' || k === 'sections' || k === 'xdt_api__v1__clips__home__connection' || k === 'xdt_shortcode_media' || k === 'node') {
        scanObjectForClips(obj[k], depth + 1);
      }
    }
  }

  // 1. Intercept window.fetch to capture reel clips
  const origFetch = window.fetch;
  window.fetch = async function (...args) {
    const resp = await origFetch.apply(this, args);
    try {
      const url = typeof args[0] === 'string' ? args[0] : (args[0]?.url || '');
      if (url.includes('/graphql/query') || url.includes('/api/v1/clips') || url.includes('/api/v1/feed') || url.includes('/api/graphql')) {
        const clone = resp.clone();
        clone.json().then(data => {
          scanObjectForClips(data, 0);
          tagVideosInDom();
        }).catch(() => {});
      }
    } catch (e) {}
    return resp;
  };

  // 2. Intercept window.XMLHttpRequest (Instagram web heavily uses XHR for GraphQL)
  try {
    const origXhrOpen = XMLHttpRequest.prototype.open;
    const origXhrSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function (method, url, ...rest) {
      this._superidm_url = typeof url === 'string' ? url : '';
      return origXhrOpen.apply(this, [method, url, ...rest]);
    };
    XMLHttpRequest.prototype.send = function (...args) {
      this.addEventListener('load', () => {
        try {
          const u = this._superidm_url || '';
          if (u.includes('/graphql/query') || u.includes('/api/v1/clips') || u.includes('/api/v1/feed') || u.includes('/api/graphql')) {
            const data = JSON.parse(this.responseText);
            scanObjectForClips(data, 0);
            tagVideosInDom();
          }
        } catch (e) {}
      });
      return origXhrSend.apply(this, args);
    };
  } catch (e) {}

  // 3. Scan React Fiber & React Props of elements
  function findCodeInFiber(node, depth = 0) {
    if (!node || depth > 25) return null;
    const props = node.memoizedProps;
    if (props) {
      if (props.code && typeof props.code === 'string' && /^[A-Za-z0-9_-]{9,15}$/.test(props.code)) return props.code;
      if (props.shortcode && typeof props.shortcode === 'string' && /^[A-Za-z0-9_-]{9,15}$/.test(props.shortcode)) return props.shortcode;
      if (props.clip?.code) return props.clip.code;
      if (props.clip?.shortcode) return props.clip.shortcode;
      if (props.media?.code) return props.media.code;
      if (props.media?.shortcode) return props.media.shortcode;
      if (props.item?.code) return props.item.code;
      if (props.post?.code) return props.post.code;
      if (props.videoData?.code) return props.videoData.code;
      if (props.carousel_media && Array.isArray(props.carousel_media) && props.carousel_media[0]?.code) {
        return props.carousel_media[0].code;
      }
    }
    return findCodeInFiber(node.return, depth + 1);
  }

  function getFiberCode(el) {
    if (!el) return null;
    let curr = el;
    for (let d = 0; d < 12 && curr && curr !== document.body; d++) {
      for (const k of Object.keys(curr)) {
        if (k.startsWith('__reactFiber$')) {
          const code = findCodeInFiber(curr[k], 0);
          if (code) return code;
        }
        if (k.startsWith('__reactProps$')) {
          const p = curr[k];
          if (p?.code && typeof p.code === 'string' && /^[A-Za-z0-9_-]{9,15}$/.test(p.code)) return p.code;
          if (p?.shortcode && typeof p.shortcode === 'string' && /^[A-Za-z0-9_-]{9,15}$/.test(p.shortcode)) return p.shortcode;
          if (p?.clip?.code) return p.clip.code;
          if (p?.media?.code) return p.media.code;
          if (p?.item?.code) return p.item.code;
        }
      }
      curr = curr.parentElement;
    }
    return null;
  }

  function findCodeInElementOrCard(videoEl) {
    if (!videoEl) return null;

    // 1. Direct Fiber on video
    let code = getFiberCode(videoEl);
    if (code) return code;

    // 2. Check address bar pathname
    const pathMatch = location.pathname.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i);
    if (pathMatch) return pathMatch[2];

    // 3. Check card/slide container and its buttons/links
    const container = videoEl.closest('article, [data-blade-container="true"], section, div[tabindex="-1"], div[tabindex="0"]')
      || videoEl.parentElement?.parentElement?.parentElement;
    if (container) {
      // Check links inside container
      const links = container.querySelectorAll('a[href*="/reel/"], a[href*="/reels/"], a[href*="/p/"]');
      for (const a of links) {
        const m = (a.getAttribute('href') || '').match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i);
        if (m) return m[2];
      }
      // Check action buttons in reel side-panel (Like, Comment, Share, More)
      const buttons = container.querySelectorAll('[role="button"], button, [aria-label]');
      for (const btn of buttons) {
        code = getFiberCode(btn);
        if (code) return code;
      }
    }

    // 4. Check CDN mapping
    const src = videoEl.currentSrc || videoEl.src;
    if (src) {
      code = cdnToCode.get(cleanUrl(src));
      if (code) return code;
    }

    return null;
  }

  // 4. Tag video elements with resolved reel metadata
  function tagVideosInDom() {
    const videos = document.querySelectorAll('video');
    videos.forEach(v => {
      // Find code
      const code = findCodeInElementOrCard(v);
      if (code) {
        v.dataset.superidmCode = code;
        v.dataset.superidmUrl = `https://www.instagram.com/reel/${code}/`;
        const item = reelsStore.get(code);
        if (item && item.caption) {
          v.dataset.superidmTitle = item.author ? `${item.author} - ${item.caption}` : item.caption;
        }
      }
    });
  }

  // Listen for video events and DOM changes
  document.addEventListener('play', tagVideosInDom, true);
  document.addEventListener('playing', tagVideosInDom, true);
  document.addEventListener('timeupdate', tagVideosInDom, true);
  document.addEventListener('mouseenter', tagVideosInDom, true);

  // 5. Hook window.history to detect reel navigation
  const origPushState = history.pushState;
  history.pushState = function (...args) {
    const res = origPushState.apply(this, args);
    setTimeout(tagVideosInDom, 50);
    return res;
  };
  const origReplaceState = history.replaceState;
  history.replaceState = function (...args) {
    const res = origReplaceState.apply(this, args);
    setTimeout(tagVideosInDom, 50);
    return res;
  };

  // 6. Respond to content script inquiry events
  document.addEventListener('superidm:request_reel_info', (e) => {
    tagVideosInDom();
    const reqId = e.detail?.reqId;
    const targetVideo = (reqId && document.querySelector(`video[data-superidm-req="${reqId}"]`))
      || document.querySelector('video:hover')
      || document.querySelector('video');

    let code = targetVideo?.dataset?.superidmCode || findCodeInElementOrCard(targetVideo);
    if (!code) {
      // Check address bar fallback
      const m = location.pathname.match(/\/(reel|reels|p)\/([A-Za-z0-9_-]{9,15})/i);
      if (m) code = m[2];
    }

    const item = code ? reelsStore.get(code) : null;
    const resolvedUrl = code ? `https://www.instagram.com/reel/${code}/` : null;
    const resolvedTitle = item?.caption ? (item.author ? `${item.author} - ${item.caption}` : item.caption) : null;

    document.dispatchEvent(new CustomEvent('superidm:response_reel_info', {
      detail: {
        reqId,
        code,
        url: resolvedUrl,
        title: resolvedTitle
      }
    }));
  });

  // Initial tag pass
  if (document.readyState !== 'loading') {
    tagVideosInDom();
  } else {
    document.addEventListener('DOMContentLoaded', tagVideosInDom);
  }
})();
