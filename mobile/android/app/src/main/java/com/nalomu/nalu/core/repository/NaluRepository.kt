package com.nalomu.nalu.core.repository

import com.nalomu.nalu.core.database.NaluDatabase
import com.nalomu.nalu.core.database.NoteEntity
import com.nalomu.nalu.core.database.ScheduleEntity
import com.nalomu.nalu.core.database.SyncOperations
import com.nalomu.nalu.core.database.TaskEntity
import com.nalomu.nalu.core.sync.SyncManager
import java.time.Duration
import java.time.Instant
import java.time.LocalDateTime
import java.time.format.DateTimeParseException
import java.util.UUID
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

class NaluRepository(
    database: NaluDatabase,
    private val syncManager: SyncManager,
    private val enqueueSync: (() -> Unit)? = null
) {
    private val dao = database.dao()
    private val json = Json {
        encodeDefaults = true
        ignoreUnknownKeys = true
    }

    fun observeTasks(): Flow<List<TaskEntity>> = dao.observeTasks()
    fun observeCalendarTasks(): Flow<List<TaskEntity>> = dao.observeCalendarTasks()
    fun observeNotes(): Flow<List<NoteEntity>> = dao.observeNotes()
    fun observeLegacySchedules(): Flow<List<ScheduleEntity>> = dao.observeLegacySchedules()
    fun observeSyncState() = dao.observeSyncState()

    suspend fun addTask(title: String, project: String = "default") {
        val now = Instant.now().toString()
        val task = TaskEntity(
            id = UUID.randomUUID().toString(),
            project = project.ifBlank { "default" },
            title = title.trim().ifBlank { "未命名任务" },
            createdAt = now,
            updatedAt = now
        )
        dao.recordTaskChange(task, SyncOperations.INSERT, json.encodeToString(task))
        enqueueSync?.invoke()
    }

    suspend fun updateTaskProgress(task: TaskEntity, progress: Int) {
        val clampedProgress = progress.coerceIn(0, 100)
        val updated = task.copy(
            progress = clampedProgress,
            done = clampedProgress >= 100,
            updatedAt = Instant.now().toString()
        )
        dao.recordTaskChange(updated, SyncOperations.UPDATE, json.encodeToString(updated))
        enqueueSync?.invoke()
    }

    suspend fun toggleTask(task: TaskEntity) {
        val updated = task.copy(
            done = !task.done,
            progress = if (!task.done) 100 else 0,
            updatedAt = Instant.now().toString()
        )
        dao.recordTaskChange(updated, SyncOperations.UPDATE, json.encodeToString(updated))
        enqueueSync?.invoke()
    }

    suspend fun deleteTask(task: TaskEntity) {
        dao.recordTaskChange(task, SyncOperations.DELETE, "{}", task.id)
        enqueueSync?.invoke()
    }

    suspend fun addNote(title: String, content: String = "") {
        val now = Instant.now().toString()
        val note = NoteEntity(
            id = UUID.randomUUID().toString(),
            title = title.trim().ifBlank { "未命名笔记" },
            content = content,
            createdAt = now,
            updatedAt = now
        )
        dao.recordNoteChange(note, SyncOperations.INSERT, json.encodeToString(note))
        enqueueSync?.invoke()
    }

    suspend fun updateNote(note: NoteEntity, title: String, content: String, tags: String) {
        val updated = note.copy(
            title = title.trim().ifBlank { "未命名笔记" },
            content = content,
            tags = tags,
            updatedAt = Instant.now().toString()
        )
        dao.recordNoteChange(updated, SyncOperations.UPDATE, json.encodeToString(updated))
        enqueueSync?.invoke()
    }

    suspend fun deleteNote(note: NoteEntity) {
        dao.recordNoteChange(note, SyncOperations.DELETE, "{}", note.id)
        enqueueSync?.invoke()
    }

    suspend fun addSchedule(title: String, scheduledAt: String, reminderMinutes: Int = 0) {
        val now = Instant.now().toString()
        val scheduledStartAt = scheduledAt.ifBlank { LocalDateTime.now().format(LOCAL_DATE_TIME_FORMATTER) }
        val task = TaskEntity(
            id = UUID.randomUUID().toString(),
            project = projectFromScheduledStart(scheduledStartAt),
            title = title.trim().ifBlank { "未命名日程" },
            scheduledStartAt = scheduledStartAt,
            scheduledEndAt = defaultScheduledEndAt(scheduledStartAt),
            reminderMinutes = reminderMinutes.coerceAtLeast(0),
            createdAt = now,
            updatedAt = now
        )
        dao.recordTaskChange(task, SyncOperations.INSERT, json.encodeToString(task))
        enqueueSync?.invoke()
    }

    suspend fun toggleSchedule(task: TaskEntity) {
        toggleTask(task)
    }

    suspend fun deleteSchedule(task: TaskEntity) {
        deleteTask(task)
    }

    suspend fun pair(serverUrl: String, pairingCode: String, deviceName: String) {
        syncManager.pair(serverUrl, pairingCode, deviceName)
    }

    suspend fun syncNow(): kotlin.Result<Unit> = syncManager.syncNow()

    private fun projectFromScheduledStart(value: String): String {
        return value.take(10).ifBlank { "default" }
    }

    private fun defaultScheduledEndAt(value: String): String {
        return try {
            LocalDateTime.parse(value, LOCAL_DATE_TIME_FORMATTER)
                .plus(Duration.ofHours(1))
                .format(LOCAL_DATE_TIME_FORMATTER)
        } catch (_: DateTimeParseException) {
            value
        }
    }

    private companion object {
        val LOCAL_DATE_TIME_FORMATTER = java.time.format.DateTimeFormatter.ISO_LOCAL_DATE_TIME
    }
}
