package com.superidm.engine

import android.util.Log
import kotlinx.coroutines.delay
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import javax.inject.Inject

class RetryEngine @Inject constructor() {

    data class RetryConfig(
        val maxAttempts: Int = 5,
        val initialDelayMs: Long = 1000L,
        val maxDelayMs: Long = 30000L,
        val multiplier: Float = 2.0f
    )

    suspend fun <T> withRetry(
        config: RetryConfig = RetryConfig(),
        isTransient: (Exception) -> Boolean = ::defaultTransient,
        block: suspend () -> T
    ): T {
        var attempt = 1
        var currentDelay = config.initialDelayMs

        while (true) {
            try {
                return block()
            } catch (e: Exception) {
                if (!isTransient(e) || attempt >= config.maxAttempts) {
                    throw e
                }
                Log.w("RetryEngine", "Attempt $attempt failed: ${e.message}. Retrying in ${currentDelay}ms...")
                delay(currentDelay)
                attempt++
                currentDelay = (currentDelay * config.multiplier).toLong().coerceAtMost(config.maxDelayMs)
            }
        }
    }

    private fun defaultTransient(e: Exception): Boolean {
        return e is SocketTimeoutException || e is ConnectException || e is UnknownHostException
    }
}
