package com.nalomu.nalu

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
}
