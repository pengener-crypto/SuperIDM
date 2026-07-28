package com.superidm.protocol

import okhttp3.Dns
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.net.InetAddress
import javax.inject.Inject

class CloudflareDohResolver @Inject constructor(
    private val okHttpClient: OkHttpClient = OkHttpClient()
) : Dns {
    override fun lookup(hostname: String): List<InetAddress> {
        return try {
            val url = "${DnsPresets.CLOUDFLARE_DOH}?name=$hostname&type=A"
            val request = Request.Builder()
                .url(url)
                .addHeader("Accept", "application/dns-json")
                .build()

            val response = okHttpClient.newCall(request).execute()
            if (!response.isSuccessful) {
                return SystemDns.lookup(hostname)
            }

            val body = response.body?.string() ?: return SystemDns.lookup(hostname)
            val jsonObject = JSONObject(body)
            val answers = jsonObject.optJSONArray("Answer")
                ?: return SystemDns.lookup(hostname)

            val inetAddresses = mutableListOf<InetAddress>()
            for (i in 0 until answers.length()) {
                val answer = answers.getJSONObject(i)
                val ipString = answer.optString("data")
                if (ipString.isNotEmpty()) {
                    inetAddresses.add(InetAddress.getByName(ipString))
                }
            }

            if (inetAddresses.isEmpty()) SystemDns.lookup(hostname) else inetAddresses
        } catch (e: Exception) {
            e.printStackTrace()
            SystemDns.lookup(hostname)
        }
    }
}

object SystemDns : Dns {
    override fun lookup(hostname: String): List<InetAddress> = InetAddress.getAllByName(hostname).toList()
}

object DnsPresets {
    const val CLOUDFLARE_DOH = "https://cloudflare-dns.com/dns-query"
    const val GOOGLE_DOH = "https://dns.google/dns-query"
}
