package com.nalomu.nalu.core.database

import androidx.room.Database
import androidx.room.RoomDatabase

@Database(
    entities = [
        TaskEntity::class,
        TaskColumnEntity::class,
        TaskGroupEntity::class,
        NoteEntity::class,
        ScheduleEntity::class,
        ChangelogEntity::class,
        SyncStateEntity::class,
        DeviceStateEntity::class
    ],
    version = 2,
    exportSchema = false
)
abstract class NaluDatabase : RoomDatabase() {
    abstract fun dao(): NaluDao
}
