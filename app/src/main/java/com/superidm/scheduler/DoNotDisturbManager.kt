package com.superidm.scheduler

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.*
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.util.Calendar
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DoNotDisturbManager @Inject constructor(
    private val dataStore: DataStore<Preferences>
) {
    companion object {
        private val KEY_ENABLED = booleanPreferencesKey("dnd_enabled")
        private val KEY_START_HOUR = intPreferencesKey("dnd_start_hour")
        private val KEY_START_MIN = intPreferencesKey("dnd_start_min")
        private val KEY_END_HOUR = intPreferencesKey("dnd_end_hour")
        private val KEY_END_MIN = intPreferencesKey("dnd_end_min")
    }

    val isEnabled: Flow<Boolean> = dataStore.data.map { prefs -> prefs[KEY_ENABLED] ?: false }
    val startHour: Flow<Int> = dataStore.data.map { prefs -> prefs[KEY_START_HOUR] ?: 22 }
    val startMinute: Flow<Int> = dataStore.data.map { prefs -> prefs[KEY_START_MIN] ?: 0 }
    val endHour: Flow<Int> = dataStore.data.map { prefs -> prefs[KEY_END_HOUR] ?: 7 }
    val endMinute: Flow<Int> = dataStore.data.map { prefs -> prefs[KEY_END_MIN] ?: 0 }

    suspend fun setEnabled(enabled: Boolean) {
        dataStore.edit { prefs ->
            prefs[KEY_ENABLED] = enabled
        }
    }

    suspend fun setWindow(startHour: Int, startMinute: Int, endHour: Int, endMinute: Int) {
        dataStore.edit { prefs ->
            prefs[KEY_START_HOUR] = startHour
            prefs[KEY_START_MIN] = startMinute
            prefs[KEY_END_HOUR] = endHour
            prefs[KEY_END_MIN] = endMinute
        }
    }

    suspend fun isCurrentlyQuietHour(): Boolean {
        val enabled = isEnabled.first()
        if (!enabled) return false

        val sh = startHour.first()
        val sm = startMinute.first()
        val eh = endHour.first()
        val em = endMinute.first()

        return isInQuietHour(sh, sm, eh, em)
    }

    fun isInQuietHour(startH: Int, startM: Int, endH: Int, endM: Int): Boolean {
        val calendar = Calendar.getInstance()
        val currentHour = calendar.get(Calendar.HOUR_OF_DAY)
        val currentMinute = calendar.get(Calendar.MINUTE)

        val currentTime = currentHour * 60 + currentMinute
        val startTime = startH * 60 + startM
        val endTime = endH * 60 + endM

        return if (startTime < endTime) {
            // Standard window (e.g., 08:00 to 17:00)
            currentTime in startTime until endTime
        } else {
            // Wraps around midnight (e.g., 22:00 to 07:00)
            currentTime >= startTime || currentTime < endTime
        }
    }
}
