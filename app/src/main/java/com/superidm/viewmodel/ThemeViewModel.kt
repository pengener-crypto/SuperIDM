package com.superidm.viewmodel

import androidx.compose.ui.graphics.Color
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.*
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

enum class ColorMode { SYSTEM, LIGHT, DARK, AMOLED }

@HiltViewModel
class ThemeViewModel @Inject constructor(
    private val dataStore: DataStore<Preferences>
) : ViewModel() {
    companion object {
        private val KEY_COLOR_MODE = stringPreferencesKey("color_mode")
        private val KEY_DYNAMIC_COLOR = booleanPreferencesKey("dynamic_color")
        private val KEY_ACCENT_COLOR = longPreferencesKey("accent_color") // stored as ARGB long
        private val KEY_FONT_SIZE = floatPreferencesKey("font_size_multiplier")
        
        val PRESET_COLORS = listOf(
            0xFF6C63FF, 0xFF2196F3, 0xFF00BCD4, 0xFF009688,
            0xFF4CAF50, 0xFFFFEB3B, 0xFFFF9800, 0xFFF44336,
            0xFFE91E63, 0xFF3F51B5, 0xFF673AB7, 0xFF607D8B
        ).map { Color(it) }
    }
    
    val colorMode: StateFlow<ColorMode> = dataStore.data.map { prefs ->
        ColorMode.valueOf(prefs[KEY_COLOR_MODE] ?: ColorMode.SYSTEM.name)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), ColorMode.SYSTEM)
    
    val isDynamicColor: StateFlow<Boolean> = dataStore.data.map { prefs ->
        prefs[KEY_DYNAMIC_COLOR] ?: true
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), true)
    
    val accentColor: StateFlow<Color> = dataStore.data.map { prefs ->
        Color(prefs[KEY_ACCENT_COLOR] ?: 0xFF6C63FF)
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), Color(0xFF6C63FF))
    
    val fontSizeMultiplier: StateFlow<Float> = dataStore.data.map { prefs ->
        prefs[KEY_FONT_SIZE] ?: 1.0f
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 1.0f)
    
    fun setColorMode(mode: ColorMode) {
        viewModelScope.launch { dataStore.edit { it[KEY_COLOR_MODE] = mode.name } }
    }
    fun setDynamicColor(enabled: Boolean) {
        viewModelScope.launch { dataStore.edit { it[KEY_DYNAMIC_COLOR] = enabled } }
    }
    fun setAccentColor(color: Color) {
        viewModelScope.launch { dataStore.edit { it[KEY_ACCENT_COLOR] = color.value.toLong() } }
    }
    fun setFontSize(multiplier: Float) {
        viewModelScope.launch { dataStore.edit { it[KEY_FONT_SIZE] = multiplier } }
    }
}
