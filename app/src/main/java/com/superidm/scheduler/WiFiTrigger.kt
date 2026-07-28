package com.superidm.scheduler

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiManager
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class WiFiTrigger @Inject constructor(
    @ApplicationContext private val context: Context,
    private val scheduleDao: ScheduleDao
) {
    private val connectivityManager = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
    private val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    private var networkCallback: ConnectivityManager.NetworkCallback? = null

    fun startMonitoring(onWifiConnected: (ssid: String?) -> Unit, onWifiDisconnected: () -> Unit) {
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .build()
            
        networkCallback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                onWifiConnected(getCurrentSsid())
            }

            override fun onLost(network: Network) {
                onWifiDisconnected()
            }
        }
        
        connectivityManager.registerNetworkCallback(request, networkCallback!!)
    }

    fun stopMonitoring() {
        networkCallback?.let {
            connectivityManager.unregisterNetworkCallback(it)
            networkCallback = null
        }
    }

    fun getCurrentSsid(): String? {
        return try {
            val info = wifiManager.connectionInfo
            if (info != null && info.networkId != -1) {
                info.ssid?.removePrefix("\"")?.removeSuffix("\"")
            } else {
                null
            }
        } catch (e: SecurityException) {
            null
        }
    }
}
