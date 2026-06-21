package com.nalomu.nalu.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.nalomu.nalu.core.database.NoteEntity
import com.nalomu.nalu.core.database.ScheduleEntity
import com.nalomu.nalu.core.database.SyncStateEntity
import com.nalomu.nalu.core.database.TaskEntity
import com.nalomu.nalu.core.repository.NaluRepository
import com.nalomu.nalu.core.settings.SettingsStore
import com.nalomu.nalu.core.settings.SyncSettings
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

data class NaluUiState(
    val tasks: List<TaskEntity> = emptyList(),
    val notes: List<NoteEntity> = emptyList(),
    val schedules: List<ScheduleEntity> = emptyList(),
    val settings: SyncSettings = SyncSettings(),
    val syncState: SyncStateEntity? = null,
    val busy: Boolean = false,
    val message: String? = null
)

class NaluViewModel(
    private val repository: NaluRepository,
    settingsStore: SettingsStore
) : ViewModel() {
    private val baseState = combine(
        repository.observeTasks(),
        repository.observeNotes(),
        repository.observeSchedules(),
        settingsStore.settings,
        repository.observeSyncState()
    ) { tasks, notes, schedules, settings, syncState ->
        NaluUiState(
            tasks = tasks,
            notes = notes,
            schedules = schedules,
            settings = settings,
            syncState = syncState
        )
    }

    val uiState: StateFlow<NaluUiState> = baseState.stateIn(
        scope = viewModelScope,
        started = SharingStarted.WhileSubscribed(5_000),
        initialValue = NaluUiState()
    )

    private fun launchWithMessage(block: suspend () -> String) {
        viewModelScope.launch {
            runCatching { block() }
                .onFailure { error -> _transientMessage = error.message ?: "操作失败" }
                .onSuccess { message -> _transientMessage = message }
        }
    }

    private var _transientMessage: String? = null

    fun consumeMessage(): String? {
        val message = _transientMessage
        _transientMessage = null
        return message
    }

    fun pair(serverUrl: String, pairingCode: String, deviceName: String) = launchWithMessage {
        repository.pair(serverUrl, pairingCode, deviceName)
        "已配对"
    }

    fun syncNow() = launchWithMessage {
        repository.syncNow().getOrThrow()
        "同步完成"
    }

    fun addTask(title: String) = launchWithMessage {
        repository.addTask(title)
        "任务已保存"
    }

    fun toggleTask(task: TaskEntity) = launchWithMessage {
        repository.toggleTask(task)
        "任务已更新"
    }

    fun updateTaskProgress(task: TaskEntity, progress: Int) = launchWithMessage {
        repository.updateTaskProgress(task, progress)
        "进度已更新"
    }

    fun deleteTask(task: TaskEntity) = launchWithMessage {
        repository.deleteTask(task)
        "任务已删除"
    }

    fun addNote(title: String, content: String = "") = launchWithMessage {
        repository.addNote(title, content)
        "笔记已保存"
    }

    fun updateNote(note: NoteEntity, title: String, content: String, tags: String) = launchWithMessage {
        repository.updateNote(note, title, content, tags)
        "笔记已更新"
    }

    fun deleteNote(note: NoteEntity) = launchWithMessage {
        repository.deleteNote(note)
        "笔记已删除"
    }

    fun addSchedule(title: String, scheduledAt: String, reminderMinutes: Int) = launchWithMessage {
        repository.addSchedule(title, scheduledAt, reminderMinutes)
        "日程已保存"
    }

    fun toggleSchedule(schedule: ScheduleEntity) = launchWithMessage {
        repository.toggleSchedule(schedule)
        "日程已更新"
    }

    fun deleteSchedule(schedule: ScheduleEntity) = launchWithMessage {
        repository.deleteSchedule(schedule)
        "日程已删除"
    }

    class Factory(
        private val repository: NaluRepository,
        private val settingsStore: SettingsStore
    ) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T {
            return NaluViewModel(repository, settingsStore) as T
        }
    }
}
