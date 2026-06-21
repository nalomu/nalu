package com.nalomu.nalu

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import com.nalomu.nalu.core.database.NaluDatabase
import com.nalomu.nalu.core.database.SyncOperations
import com.nalomu.nalu.core.database.SyncTables
import com.nalomu.nalu.core.repository.NaluRepository
import com.nalomu.nalu.core.settings.SettingsStore
import com.nalomu.nalu.core.sync.SyncManager
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class RepositoryChangelogTest {
    private lateinit var database: NaluDatabase
    private lateinit var repository: NaluRepository

    @Before
    fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        database = Room.inMemoryDatabaseBuilder(context, NaluDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        val settingsStore = SettingsStore(context)
        repository = NaluRepository(database, SyncManager(database, settingsStore))
    }

    @After
    fun tearDown() {
        database.close()
    }

    @Test
    fun addTaskWritesBusinessRowAndPendingChangelog() = runTest {
        repository.addTask("Write Android scaffold")

        val pending = database.dao().getPendingChangelog()

        assertEquals(1, pending.size)
        assertEquals(SyncTables.TASKS, pending.first().tableName)
        assertEquals(SyncOperations.INSERT, pending.first().operation)
        assertFalse(pending.first().synced)
    }

    @Test
    fun addScheduleWritesScheduledTaskChangelogInsteadOfLegacySchedule() = runTest {
        repository.addSchedule("Planning", "2026-06-21T09:00:00", 10)

        val pending = database.dao().getPendingChangelog()
        val task = database.dao().getTask(pending.first().rowId)

        assertEquals(1, pending.size)
        assertEquals(SyncTables.TASKS, pending.first().tableName)
        assertEquals(SyncOperations.INSERT, pending.first().operation)
        assertNotNull(task)
        assertEquals("2026-06-21T09:00:00", task?.scheduledStartAt)
        assertEquals("2026-06-21T10:00:00", task?.scheduledEndAt)
        assertEquals(10, task?.reminderMinutes)
    }
}
