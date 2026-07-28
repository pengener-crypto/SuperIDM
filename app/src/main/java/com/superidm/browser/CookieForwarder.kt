package com.superidm.browser

import android.webkit.CookieManager
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class CookieForwarder @Inject constructor() {
    fun getCookiesForUrl(url: String): String {
        return CookieManager.getInstance().getCookie(url) ?: ""
    }
    
    fun forwardCookiesToHeaders(url: String): Map<String, String> {
        val cookies = getCookiesForUrl(url)
        return if (cookies.isNotEmpty()) {
            mapOf("Cookie" to cookies)
        } else {
            emptyMap()
        }
    }
    
    fun syncCookiesFromWebView() {
        CookieManager.getInstance().flush()
    }
    
    fun clearCookies() {
        CookieManager.getInstance().removeAllCookies(null)
    }
}
