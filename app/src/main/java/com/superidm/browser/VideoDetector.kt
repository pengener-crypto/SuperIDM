package com.superidm.browser

import javax.inject.Inject
import javax.inject.Singleton

data class DetectedVideo(
    val url: String,
    val type: String, // "hls", "dash", "mp4", "unknown"
    val quality: String = "",
    val title: String = ""
)

@Singleton
class VideoDetector @Inject constructor() {
    companion object {
        const val JS_INTERFACE_NAME = "SuperIDM"
    }
    
    val JS_INJECTION_SCRIPT: String = """
        (function() {
            function notifyAndroid(url, type) {
                if (window.SuperIDM && window.SuperIDM.onVideoDetected) {
                    window.SuperIDM.onVideoDetected(url, type);
                }
            }

            function scanVideos() {
                var videos = document.querySelectorAll('video, video source');
                for (var i = 0; i < videos.length; i++) {
                    var src = videos[i].src;
                    if (src && src.length > 0 && !src.startsWith('blob:')) {
                        var type = 'unknown';
                        if (src.indexOf('.mp4') !== -1) type = 'mp4';
                        else if (src.indexOf('.m3u8') !== -1) type = 'hls';
                        else if (src.indexOf('.mpd') !== -1) type = 'dash';
                        notifyAndroid(src, type);
                    }
                }
            }

            scanVideos();

            var observer = new MutationObserver(function(mutations) {
                mutations.forEach(function(mutation) {
                    scanVideos();
                });
            });
            observer.observe(document.body, { childList: true, subtree: true });

            var open = window.XMLHttpRequest.prototype.open;
            window.XMLHttpRequest.prototype.open = function(method, url) {
                if (url && (url.indexOf('.m3u8') !== -1 || url.indexOf('.mpd') !== -1)) {
                    var type = url.indexOf('.m3u8') !== -1 ? 'hls' : 'dash';
                    notifyAndroid(url, type);
                }
                return open.apply(this, arguments);
            };
        })();
    """.trimIndent()
    
    fun isVideoUrl(url: String): Boolean {
        val lower = url.lowercase()
        return lower.contains(".mp4") || lower.contains(".m3u8") || lower.contains(".mpd")
    }
}
