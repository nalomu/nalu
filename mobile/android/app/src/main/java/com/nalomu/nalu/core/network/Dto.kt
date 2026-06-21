package com.nalomu.nalu.core.network

import com.nalomu.nalu.core.database.ChangelogEntity
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class PairingRequest(
    @SerialName("pairing_code") val pairingCode: String,
    @SerialName("device_name") val deviceName: String
)

@Serializable
data class PairingResponse(
    @SerialName("device_id") val deviceId: String,
    val token: String
)

@Serializable
data class ChangelogDto(
    val id: Long?,
    @SerialName("table_name") val tableName: String,
    @SerialName("row_id") val rowId: String,
    val operation: String,
    val payload: String,
    @SerialName("client_ts") val clientTs: Long,
    @SerialName("server_ts") val serverTs: Long?,
    val synced: Boolean
)

@Serializable
data class SyncPushRequest(
    @SerialName("device_id") val deviceId: String,
    @SerialName("last_server_ts") val lastServerTs: Long,
    val entries: List<ChangelogDto>
)

@Serializable
data class SyncAck(
    @SerialName("client_entry_id") val clientEntryId: Long,
    @SerialName("server_ts") val serverTs: Long
)

@Serializable
data class SyncPushResponse(
    val accepted: List<SyncAck>,
    val conflicts: List<ChangelogDto>
)

@Serializable
data class SyncPullRequest(
    @SerialName("device_id") val deviceId: String,
    @SerialName("last_server_ts") val lastServerTs: Long
)

@Serializable
data class SyncPullResponse(
    val entries: List<ChangelogDto>,
    @SerialName("latest_server_ts") val latestServerTs: Long
)

fun ChangelogEntity.toDto(): ChangelogDto = ChangelogDto(
    id = id,
    tableName = tableName,
    rowId = rowId,
    operation = operation,
    payload = payload,
    clientTs = clientTs,
    serverTs = serverTs,
    synced = synced
)
