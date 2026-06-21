package com.nalomu.nalu.core.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import androidx.work.Constraints
import com.nalomu.nalu.NaluApp

class SyncWorker(
    appContext: Context,
    params: WorkerParameters
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as NaluApp
        val result = app.container.syncManager.syncNow()
        return result.fold(
            onSuccess = { Result.success() },
            onFailure = { Result.failure(workDataOf("error" to (it.message ?: "sync failed"))) }
        )
    }

    companion object {
        private const val UNIQUE_WORK = "nalu-sync-now"

        fun enqueue(context: Context) {
            val request = OneTimeWorkRequestBuilder<SyncWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork(
                UNIQUE_WORK,
                ExistingWorkPolicy.REPLACE,
                request
            )
        }
    }
}
