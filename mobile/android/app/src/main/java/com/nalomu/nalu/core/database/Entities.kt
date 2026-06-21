package com.nalomu.nalu.core.database

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
@Entity(tableName = "tasks")
data class TaskEntity(
    @PrimaryKey val id: String,
    val project: String = "default",
    val title: String,
    val done: Boolean = false,
    val progress: Int = 0,
    @SerialName("column_id") @ColumnInfo(name = "column_id") val columnId: String = "",
    val position: Long = 0,
    @SerialName("created_at") @ColumnInfo(name = "created_at") val createdAt: String,
    @SerialName("updated_at") @ColumnInfo(name = "updated_at") val updatedAt: String
)

@Serializable
@Entity(tableName = "task_columns")
data class TaskColumnEntity(
    @PrimaryKey val id: String,
    val project: String = "default",
    val name: String = "任务",
    @SerialName("sort_order") @ColumnInfo(name = "sort_order") val sortOrder: Long = 0,
    @SerialName("created_at") @ColumnInfo(name = "created_at") val createdAt: String,
    @SerialName("updated_at") @ColumnInfo(name = "updated_at") val updatedAt: String
)

@Serializable
@Entity(tableName = "task_groups")
data class TaskGroupEntity(
    @PrimaryKey val project: String,
    @SerialName("sort_order") @ColumnInfo(name = "sort_order") val sortOrder: Long = 0,
    @SerialName("created_at") @ColumnInfo(name = "created_at") val createdAt: String,
    @SerialName("updated_at") @ColumnInfo(name = "updated_at") val updatedAt: String
)

@Serializable
@Entity(tableName = "notes")
data class NoteEntity(
    @PrimaryKey val id: String,
    val title: String,
    val content: String = "",
    val tags: String = "",
    @SerialName("note_type") @ColumnInfo(name = "note_type") val noteType: String = "memo",
    @SerialName("created_at") @ColumnInfo(name = "created_at") val createdAt: String,
    @SerialName("updated_at") @ColumnInfo(name = "updated_at") val updatedAt: String
)

@Serializable
@Entity(tableName = "schedules")
data class ScheduleEntity(
    @PrimaryKey val id: String,
    val title: String,
    @SerialName("scheduled_at") @ColumnInfo(name = "scheduled_at") val scheduledAt: String,
    @SerialName("reminder_minutes") @ColumnInfo(name = "reminder_minutes") val reminderMinutes: Int = 5,
    val done: Boolean = false,
    @SerialName("created_at") @ColumnInfo(name = "created_at") val createdAt: String
)

@Entity(tableName = "sync_changelog")
data class ChangelogEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    @ColumnInfo(name = "table_name") val tableName: String,
    @ColumnInfo(name = "row_id") val rowId: String,
    val operation: String,
    val payload: String,
    @ColumnInfo(name = "client_ts") val clientTs: Long,
    @ColumnInfo(name = "server_ts") val serverTs: Long? = null,
    val synced: Boolean = false
)

@Entity(tableName = "sync_state")
data class SyncStateEntity(
    @PrimaryKey val id: String = "default",
    @ColumnInfo(name = "last_server_ts") val lastServerTs: Long = 0,
    @ColumnInfo(name = "last_sync_at") val lastSyncAt: String? = null,
    @ColumnInfo(name = "last_error") val lastError: String? = null
)

@Entity(tableName = "device_state")
data class DeviceStateEntity(
    @PrimaryKey val id: String = "default",
    @ColumnInfo(name = "device_name") val deviceName: String = "",
    @ColumnInfo(name = "created_at") val createdAt: String
)
