package com.superidm.engine

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.Socket
import javax.inject.Inject

data class NetworkInterface(
    val network: Network,
    val type: String,
    val isAvailable: Boolean
)

class DualNetworkBonder @Inject constructor(@ApplicationContext private val context: Context) {

    private val connectivityManager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

    suspend fun getAvailableNetworks(): List<NetworkInterface> = withContext(Dispatchers.IO) {
        val result = mutableListOf<NetworkInterface>()
        val activeNetworks = connectivityManager.allNetworks
        for (network in activeNetworks) {
            val caps = connectivityManager.getNetworkCapabilities(network)
            if (caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true) {
                val type = when {
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
                    caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
                    else -> null
                }
                if (type != null) {
                    result.add(NetworkInterface(network, type, true))
                }
            }
        }
        result.distinctBy { it.type }
    }

    fun bindSocketToNetwork(socket: Socket, network: Network) {
        network.bindSocket(socket)
    }

    fun isBondingAvailable(): Boolean {
        var hasWifi = false
        var hasCellular = false
        val activeNetworks = connectivityManager.allNetworks
        for (network in activeNetworks) {
            val caps = connectivityManager.getNetworkCapabilities(network)
            if (caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true) {
                if (caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) hasWifi = true
                if (caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR)) hasCellular = true
            }
        }
        return hasWifi && hasCellular
    }

    fun splitRanges(totalBytes: Long, networks: List<NetworkInterface>): List<Pair<Long, Long>> {
        if (networks.isEmpty() || totalBytes <= 0) return listOf(Pair(0L, totalBytes - 1))
        
        val partSize = totalBytes / networks.size
        return networks.mapIndexed { index, _ ->
            val start = index * partSize
            val end = if (index == networks.size - 1) totalBytes - 1 else start + partSize - 1
            Pair(start, end)
        }
    }
}
