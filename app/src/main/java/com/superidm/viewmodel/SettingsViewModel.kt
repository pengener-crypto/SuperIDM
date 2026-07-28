package com.superidm.viewmodel

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class SettingsViewModel @Inject constructor(
    private val dataStore: DataStore<Preferences>
) : ViewModel() {

    companion object {
        val KEY_MAX_CONCURRENT = intPreferencesKey("max_concurrent")
        val KEY_THEME_MODE = stringPreferencesKey("theme_mode")
        val KEY_DYNAMIC_COLOR = booleanPreferencesKey("dynamic_color")
        val KEY_DEFAULT_SAVE_DIR = stringPreferencesKey("default_save_dir")
        val KEY_CLIPBOARD_WATCHER = booleanPreferencesKey("clipboard_watcher")
        val KEY_AUTO_CATEGORIZE = booleanPreferencesKey("auto_categorize")
    }

    val maxConcurrentDownloads: StateFlow<Int> = dataStore.data.map { prefs ->
        prefs[KEY_MAX_CONCURRENT] ?: 3
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), 3)

    val themeMode: StateFlow<String> = dataStore.data.map { prefs ->
        prefs[KEY_THEME_MODE] ?: "SYSTEM"
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), "SYSTEM")

    val dynamicColorEnabled: StateFlow<Boolean> = dataStore.data.map { prefs ->
        prefs[KEY_DYNAMIC_COLOR] ?: true
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), true)

    val defaultSaveDir: StateFlow<String> = dataStore.data.map { prefs ->
        prefs[KEY_DEFAULT_SAVE_DIR] ?: ""
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), "")

    val clipboardWatcherEnabled: StateFlow<Boolean> = dataStore.data.map { prefs ->
        prefs[KEY_CLIPBOARD_WATCHER] ?: false
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), false)

    val autoCategorize: StateFlow<Boolean> = dataStore.data.map { prefs ->
        prefs[KEY_AUTO_CATEGORIZE] ?: true
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), true)

    fun updateMaxConcurrent(count: Int) {
        viewModelScope.launch { dataStore.edit { it[KEY_MAX_CONCURRENT] = count } }
    }

    fun updateThemeMode(mode: String) {
        viewModelScope.launch { dataStore.edit { it[KEY_THEME_MODE] = mode } }
    }

    fun updateDynamicColor(enabled: Boolean) {
        viewModelScope.launch { dataStore.edit { it[KEY_DYNAMIC_COLOR] = enabled } }
    }

    fun updateSaveDir(dir: String) {
        viewModelScope.launch { dataStore.edit { it[KEY_DEFAULT_SAVE_DIR] = dir } }
    }

    fun updateClipboardWatcher(enabled: Boolean) {
        viewModelScope.launch { dataStore.edit { it[KEY_CLIPBOARD_WATCHER] = enabled } }
    }

    fun updateAutoCategorize(enabled: Boolean) {
        viewModelScope.launch { dataStore.edit { it[KEY_AUTO_CATEGORIZE] = enabled } }
    }
}
