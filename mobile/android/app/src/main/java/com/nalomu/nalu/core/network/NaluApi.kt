package com.nalomu.nalu.core.network

import retrofit2.http.Body
import retrofit2.http.POST

interface NaluApi {
    @POST("auth/pair")
    suspend fun pair(@Body request: PairingRequest): PairingResponse

    @POST("sync/push")
    suspend fun push(@Body request: SyncPushRequest): SyncPushResponse

    @POST("sync/pull")
    suspend fun pull(@Body request: SyncPullRequest): SyncPullResponse
}
