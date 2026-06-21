package com.nalomu.nalu.core.settings

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map

private val Context.dataStore by preferencesDataStore(name = "nalu_settings")

data class SyncSettings(
    val serverUrl: String = "",
    val deviceId: String = "",
    val authToken: String = "",
    val lastServerTs: Long = 0,
    val lastSyncAt: String = ""
) {
    val isPaired: Boolean get() = serverUrl.isNotBlank() && deviceId.isNotBlank() && authToken.isNotBlank()
}

class SettingsStore(private val context: Context) {
    val settings: Flow<SyncSettings> = context.dataStore.data.map { preferences ->
        SyncSettings(
            serverUrl = preferences[SERVER_URL].orEmpty(),
            deviceId = preferences[DEVICE_ID].orEmpty(),
            authToken = preferences[AUTH_TOKEN].orEmpty(),
            lastServerTs = preferences[LAST_SERVER_TS] ?: 0,
            lastSyncAt = preferences[LAST_SYNC_AT].orEmpty()
        )
    }

    suspend fun current(): SyncSettings = settings.first()

    suspend fun savePairing(serverUrl: String, deviceId: String, token: String) {
        context.dataStore.edit { preferences ->
            preferences[SERVER_URL] = serverUrl.trim().trimEnd('/')
            preferences[DEVICE_ID] = deviceId
            preferences[AUTH_TOKEN] = token
        }
    }

    suspend fun updateSyncCursor(lastServerTs: Long, lastSyncAt: String) {
        context.dataStore.edit { preferences ->
            preferences[LAST_SERVER_TS] = lastServerTs
            preferences[LAST_SYNC_AT] = lastSyncAt
        }
    }

    suspend fun clearPairing() {
        context.dataStore.edit { preferences ->
            preferences.remove(SERVER_URL)
            preferences.remove(DEVICE_ID)
            preferences.remove(AUTH_TOKEN)
            preferences.remove(LAST_SERVER_TS)
            preferences.remove(LAST_SYNC_AT)
        }
    }

    private companion object {
        val SERVER_URL = stringPreferencesKey("server_url")
        val DEVICE_ID = stringPreferencesKey("device_id")
        val AUTH_TOKEN = stringPreferencesKey("auth_token")
        val LAST_SERVER_TS = longPreferencesKey("last_server_ts")
        val LAST_SYNC_AT = stringPreferencesKey("last_sync_at")
    }
}
