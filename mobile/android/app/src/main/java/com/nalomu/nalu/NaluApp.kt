package com.nalomu.nalu

import android.app.Application
import androidx.room.Room
import com.nalomu.nalu.core.database.NaluDatabase
import com.nalomu.nalu.core.repository.NaluRepository
import com.nalomu.nalu.core.settings.SettingsStore
import com.nalomu.nalu.core.sync.SyncManager
import com.nalomu.nalu.core.sync.SyncWorker

class NaluApp : Application() {
    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        val database = Room.databaseBuilder(
            this,
            NaluDatabase::class.java,
            "nalu-mobile.db"
        )
            .fallbackToDestructiveMigration(true)
            .build()
        val settingsStore = SettingsStore(this)
        val syncManager = SyncManager(database, settingsStore)
        container = AppContainer(
            database = database,
            settingsStore = settingsStore,
            repository = NaluRepository(database, syncManager) {
                SyncWorker.enqueue(this)
            },
            syncManager = syncManager
        )
        SyncWorker.schedulePeriodic(this)
    }
}

data class AppContainer(
    val database: NaluDatabase,
    val settingsStore: SettingsStore,
    val repository: NaluRepository,
    val syncManager: SyncManager
)
