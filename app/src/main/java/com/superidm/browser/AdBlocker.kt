package com.superidm.browser

import android.webkit.WebResourceResponse
import java.io.ByteArrayInputStream
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class AdBlocker @Inject constructor() {
    private val blockedDomains: Set<String> = setOf(
        "doubleclick.net", "googleadservices.com", "googlesyndication.com",
        "google-analytics.com", "googletagmanager.com", "googletagservices.com",
        "facebook.net", "connect.facebook.net", "graph.facebook.com",
        "ad.doubleclick.net", "ads.yahoo.com", "adserver.yahoo.com",
        "adservices.google.com", "analytics.google.com", "api.facebook.com",
        "beacon.krxd.net", "cdn.taboola.com", "criteo.com", "ib.adnxs.com",
        "media.net", "pixel.facebook.com", "scorecardresearch.com",
        "securepubads.g.doubleclick.net", "syndication.twitter.com",
        "tpc.googlesyndication.com", "www.google-analytics.com",
        "www.googletagmanager.com", "www.googletagservices.com",
        "ad.yieldmanager.com", "ads.pubmatic.com", "adtech.de",
        "advertising.com", "amazon-adsystem.com", "casalemedia.com",
        "contextweb.com", "demdex.net", "dotomi.com", "exelator.com",
        "imrworldwide.com", "indexexchange.com", "moatads.com",
        "openx.net", "outbrain.com", "quantserve.com",
        "rubiconproject.com", "serving-sys.com", "smartadserver.com",
        "spotxchange.com", "taboola.com", "teads.tv", "tremorhub.com",
        "tribalfusion.com", "turn.com", "yieldoptimizer.com",
        "adcolony.com", "admob.com", "applovin.com", "chartboost.com",
        "flurry.com", "inmobi.com", "tapjoy.com", "unityads.unity3d.com",
        "vungle.com", "admarvel.com", "smaato.com", "startapp.com"
    )
    
    fun shouldBlock(url: String): Boolean {
        try {
            val uri = android.net.Uri.parse(url)
            val host = uri.host?.lowercase() ?: return false
            return blockedDomains.any { host == it || host.endsWith(".\$it") }
        } catch (e: Exception) {
            return false
        }
    }
    
    fun getEmptyResponse(): WebResourceResponse {
        return WebResourceResponse(
            "text/plain",
            "UTF-8",
            204,
            "No Content",
            null,
            ByteArrayInputStream(ByteArray(0))
        )
    }
}
