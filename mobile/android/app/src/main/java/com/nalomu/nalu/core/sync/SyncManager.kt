package com.nalomu.nalu.core.sync

import com.nalomu.nalu.core.database.ChangelogEntity
import com.nalomu.nalu.core.database.NaluDatabase
import com.nalomu.nalu.core.database.NoteEntity
import com.nalomu.nalu.core.database.ScheduleEntity
import com.nalomu.nalu.core.database.SyncOperations
import com.nalomu.nalu.core.database.SyncStateEntity
import com.nalomu.nalu.core.database.SyncTables
import com.nalomu.nalu.core.database.TaskColumnEntity
import com.nalomu.nalu.core.database.TaskEntity
import com.nalomu.nalu.core.database.TaskGroupEntity
import com.nalomu.nalu.core.network.ApiFactory
import com.nalomu.nalu.core.network.ChangelogDto
import com.nalomu.nalu.core.network.PairingRequest
import com.nalomu.nalu.core.network.SyncPullRequest
import com.nalomu.nalu.core.network.SyncPushRequest
import com.nalomu.nalu.core.network.toDto
import com.nalomu.nalu.core.settings.SettingsStore
import java.time.Instant
import kotlinx.serialization.json.Json

class SyncManager(
    private val database: NaluDatabase,
    private val settingsStore: SettingsStore
) {
    private val dao = database.dao()
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    suspend fun pair(serverUrl: String, pairingCode: String, deviceName: String) {
        val api = ApiFactory.create(serverUrl)
        val response = api.pair(PairingRequest(pairingCode = pairingCode, deviceName = deviceName))
        settingsStore.savePairing(serverUrl = serverUrl, deviceId = response.deviceId, token = response.token)
    }

    suspend fun syncNow(): Result<Unit> = runCatching {
        val settings = settingsStore.current()
        require(settings.isPaired) { "Device is not paired" }

        val api = ApiFactory.create(settings.serverUrl, settings.authToken)
        val localState = dao.getSyncState()
        val lastServerTs = localState?.lastServerTs ?: settings.lastServerTs

        val pending = dao.getPendingChangelog()
        if (pending.isNotEmpty()) {
            val pushResponse = api.push(
                SyncPushRequest(
                    deviceId = settings.deviceId,
                    lastServerTs = lastServerTs,
                    entries = pending.map(ChangelogEntity::toDto)
                )
            )
            pushResponse.accepted.forEach { ack ->
                dao.markChangelogSynced(ack.clientEntryId, ack.serverTs)
            }
            pushResponse.conflicts.forEach { conflict ->
                applyRemoteEntry(conflict, createNoteConflictCopy = true)
            }
        }

        val pullResponse = api.pull(
            SyncPullRequest(
                deviceId = settings.deviceId,
                lastServerTs = lastServerTs
            )
        )
        pullResponse.entries.forEach { entry ->
            applyRemoteEntry(entry, createNoteConflictCopy = false)
        }

        val now = Instant.now().toString()
        dao.upsertSyncState(
            SyncStateEntity(
                lastServerTs = pullResponse.latestServerTs,
                lastSyncAt = now,
                lastError = null
            )
        )
        settingsStore.updateSyncCursor(pullResponse.latestServerTs, now)
    }.onFailure { error ->
        val state = dao.getSyncState()
        dao.upsertSyncState(
            SyncStateEntity(
                lastServerTs = state?.lastServerTs ?: 0,
                lastSyncAt = state?.lastSyncAt,
                lastError = error.message
            )
        )
    }

    private suspend fun applyRemoteEntry(entry: ChangelogDto, createNoteConflictCopy: Boolean) {
        when (entry.tableName) {
            SyncTables.TASKS -> applyTask(entry)
            SyncTables.TASK_COLUMNS -> applyTaskColumn(entry)
            SyncTables.TASK_GROUPS -> applyTaskGroup(entry)
            SyncTables.NOTES -> applyNote(entry, createNoteConflictCopy)
            SyncTables.SCHEDULES -> applySchedule(entry)
        }
    }

    private suspend fun applyTask(entry: ChangelogDto) {
        if (entry.operation == SyncOperations.DELETE) {
            dao.deleteTask(entry.rowId)
            return
        }
        dao.upsertTask(json.decodeFromString<TaskEntity>(entry.payload))
    }

    private suspend fun applyTaskColumn(entry: ChangelogDto) {
        if (entry.operation != SyncOperations.DELETE) {
            dao.upsertTaskColumn(json.decodeFromString<TaskColumnEntity>(entry.payload))
        }
    }

    private suspend fun applyTaskGroup(entry: ChangelogDto) {
        if (entry.operation != SyncOperations.DELETE) {
            dao.upsertTaskGroup(json.decodeFromString<TaskGroupEntity>(entry.payload))
        }
    }

    private suspend fun applyNote(entry: ChangelogDto, createNoteConflictCopy: Boolean) {
        if (entry.operation == SyncOperations.DELETE) {
            dao.deleteNote(entry.rowId)
            return
        }
        val serverNote = json.decodeFromString<NoteEntity>(entry.payload)
        if (createNoteConflictCopy) {
            val local = dao.getNote(entry.rowId)
            if (local != null && local.content != serverNote.content) {
                dao.upsertNote(
                    local.copy(
                        id = "${local.id}-conflict-${System.currentTimeMillis()}",
                        title = "${local.title} (conflict copy)",
                        updatedAt = Instant.now().toString()
                    )
                )
            }
        }
        dao.upsertNote(serverNote)
    }

    private suspend fun applySchedule(entry: ChangelogDto) {
        if (entry.operation == SyncOperations.DELETE) {
            dao.deleteSchedule(entry.rowId)
            return
        }
        dao.upsertSchedule(json.decodeFromString<ScheduleEntity>(entry.payload))
    }
}
