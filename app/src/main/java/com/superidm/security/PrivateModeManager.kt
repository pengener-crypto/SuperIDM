package com.superidm.security

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class PrivateModeManager @Inject constructor(
    private val dataStore: DataStore<Preferences>
) {
    companion object {
        private val PRIVATE_MODE_KEY = booleanPreferencesKey("private_mode")
    }
    
    val isPrivateModeEnabled: Flow<Boolean> = dataStore.data.map { prefs ->
        prefs[PRIVATE_MODE_KEY] ?: false
    }
    
    suspend fun setPrivateMode(enabled: Boolean) {
        dataStore.edit { prefs -> prefs[PRIVATE_MODE_KEY] = enabled }
    }
}
