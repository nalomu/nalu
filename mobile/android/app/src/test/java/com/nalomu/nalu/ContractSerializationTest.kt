package com.nalomu.nalu

import com.nalomu.nalu.core.database.TaskEntity
import com.nalomu.nalu.core.network.ChangelogDto
import com.nalomu.nalu.core.network.SyncPushRequest
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.assertTrue
import org.junit.Test

class ContractSerializationTest {
    private val json = Json { encodeDefaults = true }

    @Test
    fun syncPushRequestUsesSnakeCaseContractFields() {
        val payload = json.encodeToString(
            SyncPushRequest(
                deviceId = "device-1",
                lastServerTs = 12,
                entries = listOf(
                    ChangelogDto(
                        id = 1,
                        tableName = "tasks",
                        rowId = "task-1",
                        operation = "insert",
                        payload = "{}",
                        clientTs = 99,
                        serverTs = null,
                        synced = false
                    )
                )
            )
        )

        assertTrue(payload.contains("\"device_id\""))
        assertTrue(payload.contains("\"last_server_ts\""))
        assertTrue(payload.contains("\"table_name\""))
        assertTrue(payload.contains("\"row_id\""))
        assertTrue(payload.contains("\"client_ts\""))
        assertTrue(payload.contains("\"server_ts\""))
    }

    @Test
    fun taskPayloadIncludesScheduledTaskContractFields() {
        val payload = json.encodeToString(
            TaskEntity(
                id = "task-1",
                title = "Planning",
                createdAt = "2026-06-21T08:00:00Z",
                updatedAt = "2026-06-21T08:00:00Z",
                scheduledStartAt = "2026-06-21T09:00:00",
                scheduledEndAt = "2026-06-21T10:00:00",
                reminderMinutes = 10
            )
        )

        assertTrue(payload.contains("\"scheduled_start_at\""))
        assertTrue(payload.contains("\"scheduled_end_at\""))
        assertTrue(payload.contains("\"reminder_minutes\""))
        assertTrue(payload.contains("\"repeat_type\""))
        assertTrue(payload.contains("\"recurrence_detached\""))
    }
}
