package com.nalomu.nalu.core.database

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Update
import kotlinx.coroutines.flow.Flow

@Dao
interface NaluDao {
    @Query("SELECT * FROM tasks ORDER BY project ASC, position ASC, updated_at DESC")
    fun observeTasks(): Flow<List<TaskEntity>>

    @Query("SELECT * FROM notes ORDER BY updated_at DESC")
    fun observeNotes(): Flow<List<NoteEntity>>

    @Query("SELECT * FROM schedules ORDER BY scheduled_at ASC")
    fun observeSchedules(): Flow<List<ScheduleEntity>>

    @Query("SELECT * FROM sync_state WHERE id = 'default'")
    fun observeSyncState(): Flow<SyncStateEntity?>

    @Query("SELECT * FROM sync_changelog WHERE synced = 0 ORDER BY id ASC")
    suspend fun getPendingChangelog(): List<ChangelogEntity>

    @Query("SELECT * FROM sync_state WHERE id = 'default'")
    suspend fun getSyncState(): SyncStateEntity?

    @Query("SELECT * FROM tasks WHERE id = :id")
    suspend fun getTask(id: String): TaskEntity?

    @Query("SELECT * FROM notes WHERE id = :id")
    suspend fun getNote(id: String): NoteEntity?

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertTask(task: TaskEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertTaskColumn(column: TaskColumnEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertTaskGroup(group: TaskGroupEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertNote(note: NoteEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertSchedule(schedule: ScheduleEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertSyncState(state: SyncStateEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsertDeviceState(state: DeviceStateEntity)

    @Insert
    suspend fun insertChangelog(entry: ChangelogEntity): Long

    @Update
    suspend fun updateTask(task: TaskEntity)

    @Update
    suspend fun updateNote(note: NoteEntity)

    @Update
    suspend fun updateSchedule(schedule: ScheduleEntity)

    @Query("DELETE FROM tasks WHERE id = :id")
    suspend fun deleteTask(id: String)

    @Query("DELETE FROM notes WHERE id = :id")
    suspend fun deleteNote(id: String)

    @Query("DELETE FROM schedules WHERE id = :id")
    suspend fun deleteSchedule(id: String)

    @Query("UPDATE sync_changelog SET synced = 1, server_ts = :serverTs WHERE id = :id")
    suspend fun markChangelogSynced(id: Long, serverTs: Long)

    @Query("UPDATE sync_changelog SET synced = 1 WHERE id = :id")
    suspend fun markChangelogHandled(id: Long)

    @Transaction
    suspend fun recordTaskChange(task: TaskEntity, operation: String, payload: String, rowId: String = task.id) {
        if (operation == SyncOperations.DELETE) {
            deleteTask(rowId)
        } else {
            upsertTask(task)
        }
        insertChangelog(
            ChangelogEntity(
                tableName = SyncTables.TASKS,
                rowId = rowId,
                operation = operation,
                payload = payload,
                clientTs = System.currentTimeMillis()
            )
        )
    }

    @Transaction
    suspend fun recordNoteChange(note: NoteEntity, operation: String, payload: String, rowId: String = note.id) {
        if (operation == SyncOperations.DELETE) {
            deleteNote(rowId)
        } else {
            upsertNote(note)
        }
        insertChangelog(
            ChangelogEntity(
                tableName = SyncTables.NOTES,
                rowId = rowId,
                operation = operation,
                payload = payload,
                clientTs = System.currentTimeMillis()
            )
        )
    }

    @Transaction
    suspend fun recordScheduleChange(schedule: ScheduleEntity, operation: String, payload: String, rowId: String = schedule.id) {
        if (operation == SyncOperations.DELETE) {
            deleteSchedule(rowId)
        } else {
            upsertSchedule(schedule)
        }
        insertChangelog(
            ChangelogEntity(
                tableName = SyncTables.SCHEDULES,
                rowId = rowId,
                operation = operation,
                payload = payload,
                clientTs = System.currentTimeMillis()
            )
        )
    }
}

object SyncOperations {
    const val INSERT = "insert"
    const val UPDATE = "update"
    const val DELETE = "delete"
}

object SyncTables {
    const val TASKS = "tasks"
    const val TASK_COLUMNS = "task_columns"
    const val TASK_GROUPS = "task_groups"
    const val NOTES = "notes"
    const val SCHEDULES = "schedules"
}
