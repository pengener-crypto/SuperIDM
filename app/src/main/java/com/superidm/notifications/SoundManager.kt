package com.superidm.notifications

import android.content.Context
import android.media.Ringtone
import android.media.RingtoneManager
import android.net.Uri
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import javax.inject.Singleton

data class NotificationSound(val name: String, val uri: Uri?)

@Singleton
class SoundManager @Inject constructor(
    @ApplicationContext private val context: Context,
    private val dataStore: DataStore<Preferences>
) {
    companion object {
        private val KEY_COMPLETION_SOUND = stringPreferencesKey("completion_sound_uri")
    }

    private var currentPreviewRingtone: Ringtone? = null

    fun getSystemSounds(): List<NotificationSound> {
        val sounds = mutableListOf<NotificationSound>()
        val ringtoneManager = RingtoneManager(context)
        ringtoneManager.setType(RingtoneManager.TYPE_NOTIFICATION)
        
        val cursor = ringtoneManager.cursor
        if (cursor != null && cursor.moveToFirst()) {
            do {
                val title = cursor.getString(RingtoneManager.TITLE_COLUMN_INDEX)
                val uriPrefix = cursor.getString(RingtoneManager.URI_COLUMN_INDEX)
                val id = cursor.getString(RingtoneManager.ID_COLUMN_INDEX)
                val uri = Uri.parse("$uriPrefix/$id")
                sounds.add(NotificationSound(title, uri))
            } while (cursor.moveToNext())
        }
        return sounds
    }

    fun getDefaultSound(): NotificationSound {
        val uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION)
        return NotificationSound("Default", uri)
    }

    fun playPreview(uri: Uri?) {
        stopPreview()
        if (uri != null) {
            try {
                currentPreviewRingtone = RingtoneManager.getRingtone(context, uri)
                currentPreviewRingtone?.play()
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }
    }

    fun stopPreview() {
        currentPreviewRingtone?.takeIf { it.isPlaying }?.stop()
        currentPreviewRingtone = null
    }

    suspend fun saveCompletionSound(uri: Uri?) {
        dataStore.edit { prefs ->
            if (uri != null) {
                prefs[KEY_COMPLETION_SOUND] = uri.toString()
            } else {
                prefs.remove(KEY_COMPLETION_SOUND)
            }
        }
    }

    suspend fun getCompletionSoundUri(): Uri? {
        val uriString = dataStore.data.map { prefs -> prefs[KEY_COMPLETION_SOUND] }.first()
        return uriString?.let { Uri.parse(it) } ?: getDefaultSound().uri
    }
}
