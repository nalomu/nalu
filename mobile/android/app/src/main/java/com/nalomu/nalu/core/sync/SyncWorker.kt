package com.nalomu.nalu.core.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import androidx.work.Constraints
import com.nalomu.nalu.NaluApp
import java.util.concurrent.TimeUnit

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
        private const val PERIODIC_WORK = "nalu-sync-periodic"

        fun enqueue(context: Context) {
            val request = OneTimeWorkRequestBuilder<SyncWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()
            runCatching {
                WorkManager.getInstance(context).enqueueUniqueWork(
                    UNIQUE_WORK,
                    ExistingWorkPolicy.REPLACE,
                    request
                )
            }
        }

        fun schedulePeriodic(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()
            runCatching {
                WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                    PERIODIC_WORK,
                    ExistingPeriodicWorkPolicy.KEEP,
                    request
                )
            }
        }
    }
}
