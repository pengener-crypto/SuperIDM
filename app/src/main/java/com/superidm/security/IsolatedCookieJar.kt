package com.superidm.security

import okhttp3.Cookie
import okhttp3.CookieJar
import okhttp3.HttpUrl

class IsolatedCookieJar(val downloadId: String) : CookieJar {
    private val cookieStore = mutableMapOf<String, MutableList<Cookie>>()
    
    override fun saveFromResponse(url: HttpUrl, cookies: List<Cookie>) {
        val host = url.host
        cookieStore.getOrPut(host) { mutableListOf() }.apply {
            removeAll { existing -> cookies.any { new -> new.name == existing.name } }
            addAll(cookies)
        }
    }
    
    override fun loadForRequest(url: HttpUrl): List<Cookie> {
        val host = url.host
        return cookieStore.entries
            .filter { (storedHost, _) -> host == storedHost || host.endsWith(".${storedHost}") }
            .flatMap { it.value }
            .filter { cookie -> cookie.matches(url) }
    }
    
    fun clearAll() { cookieStore.clear() }
    
    fun addCookiesFromString(url: HttpUrl, cookieHeader: String) {
        val cookies = cookieHeader.split(";").mapNotNull { part ->
            Cookie.parse(url, part.trim())
        }
        saveFromResponse(url, cookies)
    }
}
