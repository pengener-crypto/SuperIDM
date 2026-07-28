package com.superidm.protocol

import okhttp3.Authenticator
import okhttp3.Credentials
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.Route
import java.net.InetSocketAddress
import java.net.Proxy

data class ProxyConfig(
    val type: ProxyType,
    val host: String,
    val port: Int,
    val username: String = "",
    val password: String = ""
)

enum class ProxyType { NONE, HTTP, HTTPS, SOCKS4, SOCKS5 }

object ProxyConfigManager {
    fun buildJavaProxy(config: ProxyConfig): Proxy {
        return when (config.type) {
            ProxyType.NONE -> Proxy.NO_PROXY
            ProxyType.HTTP, ProxyType.HTTPS -> Proxy(Proxy.Type.HTTP, InetSocketAddress(config.host, config.port))
            ProxyType.SOCKS4, ProxyType.SOCKS5 -> Proxy(Proxy.Type.SOCKS, InetSocketAddress(config.host, config.port))
        }
    }
    
    fun applyToOkHttp(builder: OkHttpClient.Builder, config: ProxyConfig): OkHttpClient.Builder {
        if (config.type == ProxyType.NONE) return builder
        builder.proxy(buildJavaProxy(config))
        if (config.username.isNotEmpty()) {
            builder.proxyAuthenticator(buildAuthenticator(config)!!)
        }
        return builder
    }
    
    fun buildAuthenticator(config: ProxyConfig): Authenticator? {
        if (config.username.isEmpty()) return null
        return Authenticator { _: Route?, response: Response ->
            if (response.request.header("Proxy-Authorization") != null) return@Authenticator null
            response.request.newBuilder()
                .header("Proxy-Authorization", Credentials.basic(config.username, config.password))
                .build()
        }
    }
}
