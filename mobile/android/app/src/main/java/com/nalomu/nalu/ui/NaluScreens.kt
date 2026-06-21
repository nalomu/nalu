package com.nalomu.nalu.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Event
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Note
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Sync
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Divider
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.nalomu.nalu.core.database.NoteEntity
import com.nalomu.nalu.core.database.ScheduleEntity
import com.nalomu.nalu.core.database.TaskEntity
import com.nalomu.nalu.core.sync.NotificationHelper
import kotlinx.coroutines.delay

private enum class Destination(val route: String, val label: String) {
    Home("home", "首页"),
    Tasks("tasks", "任务"),
    Notes("notes", "笔记"),
    Schedules("schedules", "日程"),
    Pomodoro("pomodoro", "番茄"),
    Settings("settings", "设置")
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NaluMobileRoot(viewModel: NaluViewModel) {
    val uiState by viewModel.uiState.collectAsState()
    val navController = rememberNavController()
    val backStack by navController.currentBackStackEntryAsState()
    val currentRoute = backStack?.destination?.route ?: Destination.Home.route
    val current = Destination.entries.firstOrNull { it.route == currentRoute } ?: Destination.Home

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Nalu Mobile") },
                actions = {
                    IconButton(onClick = viewModel::syncNow) {
                        Icon(Icons.Outlined.Sync, contentDescription = "同步")
                    }
                }
            )
        },
        bottomBar = {
            NavigationBar {
                Destination.entries.forEach { destination ->
                    NavigationBarItem(
                        selected = current.route == destination.route,
                        onClick = {
                            navController.navigate(destination.route) {
                                popUpTo(navController.graph.findStartDestination().id) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Icon(destination.icon(), contentDescription = destination.label) },
                        label = { Text(destination.label) }
                    )
                }
            }
        }
    ) { padding ->
        if (!uiState.settings.isPaired) {
            PairingScreen(uiState = uiState, viewModel = viewModel, padding = padding)
        } else {
            NavHost(
                navController = navController,
                startDestination = Destination.Home.route,
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
            ) {
                composable(Destination.Home.route) { HomeScreen(uiState = uiState) }
                composable(Destination.Tasks.route) { TasksScreen(uiState = uiState, viewModel = viewModel) }
                composable(Destination.Notes.route) { NotesScreen(uiState = uiState, viewModel = viewModel) }
                composable(Destination.Schedules.route) { SchedulesScreen(uiState = uiState, viewModel = viewModel) }
                composable(Destination.Pomodoro.route) { PomodoroScreen() }
                composable(Destination.Settings.route) { SettingsScreen(uiState = uiState, viewModel = viewModel) }
            }
        }
    }
}

@Composable
private fun Destination.icon() = when (this) {
    Destination.Home -> Icons.Outlined.Home
    Destination.Tasks -> Icons.Outlined.TaskAlt
    Destination.Notes -> Icons.Outlined.Note
    Destination.Schedules -> Icons.Outlined.Event
    Destination.Pomodoro -> Icons.Outlined.PlayArrow
    Destination.Settings -> Icons.Outlined.Settings
}

@Composable
private fun PairingScreen(uiState: NaluUiState, viewModel: NaluViewModel, padding: PaddingValues) {
    var serverUrl by remember { mutableStateOf(uiState.settings.serverUrl) }
    var pairingCode by remember { mutableStateOf("") }
    var deviceName by remember { mutableStateOf("Android") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(padding)
            .padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp)
    ) {
        Text("配对", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold)
        OutlinedTextField(
            value = serverUrl,
            onValueChange = { serverUrl = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("服务器地址") },
            singleLine = true
        )
        OutlinedTextField(
            value = pairingCode,
            onValueChange = { pairingCode = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("配对码") },
            singleLine = true
        )
        OutlinedTextField(
            value = deviceName,
            onValueChange = { deviceName = it },
            modifier = Modifier.fillMaxWidth(),
            label = { Text("设备名") },
            singleLine = true
        )
        Button(
            onClick = { viewModel.pair(serverUrl, pairingCode, deviceName) },
            modifier = Modifier.fillMaxWidth()
        ) {
            Text("配对")
        }
    }
}

@Composable
private fun HomeScreen(uiState: NaluUiState) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        item {
            SectionHeader("今日")
            StatRow(
                tasks = uiState.tasks.count { !it.done },
                schedules = uiState.schedules.count { !it.done },
                notes = uiState.notes.size
            )
        }
        item {
            SectionHeader("同步")
            SyncCard(uiState)
        }
        item {
            SectionHeader("最近任务")
        }
        items(uiState.tasks.take(5), key = { it.id }) { task ->
            TaskRow(task = task, onToggle = {}, onDelete = {}, onProgress = {})
        }
    }
}

@Composable
private fun StatRow(tasks: Int, schedules: Int, notes: Int) {
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
        StatCard("任务", tasks.toString(), Modifier.weight(1f))
        StatCard("日程", schedules.toString(), Modifier.weight(1f))
        StatCard("笔记", notes.toString(), Modifier.weight(1f))
    }
}

@Composable
private fun StatCard(label: String, value: String, modifier: Modifier = Modifier) {
    Card(modifier = modifier, colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)) {
        Column(modifier = Modifier.padding(14.dp)) {
            Text(value, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold)
            Text(label, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun TasksScreen(uiState: NaluUiState, viewModel: NaluViewModel) {
    var title by remember { mutableStateOf("") }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    modifier = Modifier.weight(1f),
                    label = { Text("任务标题") },
                    singleLine = true
                )
                Button(onClick = {
                    viewModel.addTask(title)
                    title = ""
                }) {
                    Text("保存")
                }
            }
        }
        items(uiState.tasks, key = { it.id }) { task ->
            TaskRow(
                task = task,
                onToggle = { viewModel.toggleTask(task) },
                onDelete = { viewModel.deleteTask(task) },
                onProgress = { viewModel.updateTaskProgress(task, it) }
            )
        }
    }
}

@Composable
private fun TaskRow(
    task: TaskEntity,
    onToggle: () -> Unit,
    onDelete: () -> Unit,
    onProgress: (Int) -> Unit
) {
    Card {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = task.done, onCheckedChange = { onToggle() })
                Column(modifier = Modifier.weight(1f)) {
                    Text(task.title, maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.Medium)
                    Text(task.project, style = MaterialTheme.typography.bodySmall)
                }
                IconButton(onClick = onDelete) {
                    Icon(Icons.Outlined.Delete, contentDescription = "删除")
                }
            }
            LinearProgressIndicator(progress = { task.progress / 100f }, modifier = Modifier.fillMaxWidth())
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(0, 25, 50, 75, 100).forEach { value ->
                    AssistChip(onClick = { onProgress(value) }, label = { Text("$value%") })
                }
            }
        }
    }
}

@Composable
private fun NotesScreen(uiState: NaluUiState, viewModel: NaluViewModel) {
    val clipboard = LocalClipboardManager.current
    var title by remember { mutableStateOf("") }
    var content by remember { mutableStateOf("") }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = title, onValueChange = { title = it }, label = { Text("标题") }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = content, onValueChange = { content = it }, label = { Text("内容") }, modifier = Modifier.fillMaxWidth(), minLines = 3)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = {
                        viewModel.addNote(title, content)
                        title = ""
                        content = ""
                    }) { Text("保存") }
                    OutlinedButton(onClick = {
                        val text = clipboard.getText()?.text.orEmpty()
                        if (text.isNotBlank()) viewModel.addNote("剪贴板", text)
                    }) { Text("剪贴板") }
                }
            }
        }
        items(uiState.notes, key = { it.id }) { note ->
            NoteRow(note = note, onDelete = { viewModel.deleteNote(note) })
        }
    }
}

@Composable
private fun NoteRow(note: NoteEntity, onDelete: () -> Unit) {
    Card {
        Row(modifier = Modifier.padding(14.dp), verticalAlignment = Alignment.Top) {
            Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(note.title, fontWeight = FontWeight.Medium)
                Text(note.content, maxLines = 3, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
                if (note.tags.isNotBlank()) Text(note.tags, style = MaterialTheme.typography.bodySmall)
            }
            IconButton(onClick = onDelete) {
                Icon(Icons.Outlined.Delete, contentDescription = "删除")
            }
        }
    }
}

@Composable
private fun SchedulesScreen(uiState: NaluUiState, viewModel: NaluViewModel) {
    var title by remember { mutableStateOf("") }
    var scheduledAt by remember { mutableStateOf("") }
    var reminder by remember { mutableStateOf("5") }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)
    ) {
        item {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value = title, onValueChange = { title = it }, label = { Text("日程标题") }, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(value = scheduledAt, onValueChange = { scheduledAt = it }, label = { Text("时间") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
                OutlinedTextField(value = reminder, onValueChange = { reminder = it.filter(Char::isDigit) }, label = { Text("提前分钟") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
                Button(onClick = {
                    viewModel.addSchedule(title, scheduledAt, reminder.toIntOrNull() ?: 5)
                    title = ""
                    scheduledAt = ""
                }) { Text("保存") }
            }
        }
        items(uiState.schedules, key = { it.id }) { schedule ->
            ScheduleRow(schedule = schedule, onToggle = { viewModel.toggleSchedule(schedule) }, onDelete = { viewModel.deleteSchedule(schedule) })
        }
    }
}

@Composable
private fun ScheduleRow(schedule: ScheduleEntity, onToggle: () -> Unit, onDelete: () -> Unit) {
    Card {
        Row(modifier = Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = onToggle) {
                Icon(Icons.Outlined.CheckCircle, contentDescription = "完成")
            }
            Column(modifier = Modifier.weight(1f)) {
                Text(schedule.title, fontWeight = FontWeight.Medium)
                Text(schedule.scheduledAt, style = MaterialTheme.typography.bodySmall)
            }
            IconButton(onClick = onDelete) {
                Icon(Icons.Outlined.Delete, contentDescription = "删除")
            }
        }
    }
}

@Composable
private fun PomodoroScreen() {
    val context = LocalContext.current
    var running by remember { mutableStateOf(false) }
    var workMinutes by remember { mutableIntStateOf(25) }
    var remainingSeconds by remember { mutableIntStateOf(workMinutes * 60) }
    var completed by remember { mutableIntStateOf(0) }

    LaunchedEffect(running, workMinutes) {
        if (!running) return@LaunchedEffect
        while (running && remainingSeconds > 0) {
            delay(1_000)
            remainingSeconds -= 1
        }
        if (remainingSeconds == 0) {
            completed += 1
            running = false
            remainingSeconds = workMinutes * 60
            NotificationHelper.notify(context, "番茄钟完成", "完成 $workMinutes 分钟专注")
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(20.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(18.dp)
    ) {
        Text("${remainingSeconds / 60}:${(remainingSeconds % 60).toString().padStart(2, '0')}", style = MaterialTheme.typography.displayMedium)
        Slider(
            value = workMinutes.toFloat(),
            onValueChange = {
                workMinutes = it.toInt().coerceIn(5, 60)
                if (!running) remainingSeconds = workMinutes * 60
            },
            valueRange = 5f..60f,
            steps = 10
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Button(onClick = { running = !running }) { Text(if (running) "暂停" else "开始") }
            OutlinedButton(onClick = {
                running = false
                remainingSeconds = workMinutes * 60
            }) { Text("重置") }
        }
        Text("完成 $completed")
    }
}

@Composable
private fun SettingsScreen(uiState: NaluUiState, viewModel: NaluViewModel) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        SectionHeader("同步")
        SyncCard(uiState)
        Button(onClick = viewModel::syncNow, modifier = Modifier.fillMaxWidth()) {
            Text("立即同步")
        }
    }
}

@Composable
private fun SyncCard(uiState: NaluUiState) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(uiState.settings.serverUrl, fontWeight = FontWeight.Medium)
            Text("device: ${uiState.settings.deviceId}", style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("cursor: ${uiState.syncState?.lastServerTs ?: uiState.settings.lastServerTs}", style = MaterialTheme.typography.bodySmall)
            uiState.syncState?.lastError?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
        }
    }
}

@Composable
private fun SectionHeader(text: String) {
    Column {
        Text(text, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Spacer(Modifier.height(6.dp))
        Divider()
    }
}
