<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { Plus, Search, ChevronDown, ChevronRight, Trash2, MoreHorizontal, GripHorizontal, Circle, CheckCircle2, Copy, Settings2, CalendarDays, X } from 'lucide-vue-next'
import type { GroupData, Task, TaskColumn, ColumnWithTasks, TaskSnapshot, ColumnSnapshot } from '$lib/types'
import { useMobile } from '$lib/composables/useMobile'
import { useI18n } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import { useAiRefresh } from '$lib/composables/useAiRefresh'
import { useSettingsStore, type TaskGroupNamingStrategy } from '$lib/stores/settingsStore'
import { legacyBrowserTasks } from '$lib/data/legacyBrowserTasks'
import MobileTasksPage from './MobileTasksPage.vue'

const settings = useSettingsStore()
const { locale, t } = useI18n()
const LOCAL_CALENDAR_TASKS_KEY = 'nalu-dev-calendar-tasks'
const LOCAL_TASK_COLUMNS_KEY = 'nalu-dev-task-columns'

const { isCompactWidth, isMobilePlatform } = useMobile()

// Board data
const groups = ref<GroupData[]>([])
const searchQuery = ref('')
const collapsedGroups = ref<Record<string, boolean>>({})
const savedCollapsedGroups = ref<Record<string, boolean>>({})
const showFutureRecurring = ref(false)
const showCompletedPastDateGroups = ref(false)

// Task editing
const editingTaskId = ref<string | null>(null)
const editTaskTitle = ref('')
const selectedTaskIds = ref<Set<string>>(new Set())
const bulkMoveTargetColumnId = ref('')

// Task adding per column (hover to show)
const addingColumnId = ref<string | null>(null)
const newTaskTitle = ref('')
const savingNewTask = ref(false)

// Group adding
const addingGroup = ref(false)
const newGroupName = ref('')
const savingNewGroup = ref(false)
const editingGroupProject = ref<string | null>(null)
const editGroupName = ref('')
const savingGroupRename = ref(false)
const groupSettingsOpen = ref(false)
const groupMenuProject = ref<string | null>(null)

// Column editing
const editingColumnId = ref<string | null>(null)
const editColumnName = ref('')

// Drag state
const draggedTask = ref<Task | null>(null)
const draggedColumn = ref<TaskColumn | null>(null)
const draggedGroupProject = ref<string | null>(null)
const dropTargetGroupProject = ref<string | null>(null)
const dropTargetTaskGroupProject = ref<string | null>(null)
const dropTargetColumnId = ref<string | null>(null)
const dropTargetColumnReorderId = ref<string | null>(null)
const dropTargetPosition = ref<number>(-1)
const dropTargetNewColumn = ref<string | null>(null) // project for new column drop zone
const pointerTaskDrag = ref<{
  task: Task
  pointerId: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
const pointerGroupDrag = ref<{
  project: string
  pointerId: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
const pointerColumnDrag = ref<{
  column: TaskColumn
  pointerId: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
let suppressTaskClickUntil = 0
let suppressGroupClickUntil = 0

// Toast / undo
const toastMessage = ref('')
const toastUndoAction = ref<(() => Promise<void>) | null>(null)
const toastVisible = ref(false)
let toastTimer: ReturnType<typeof setTimeout> | null = null

// Column menu
const columnMenuId = ref<string | null>(null)

const isSearching = computed(() => searchQuery.value.trim().length > 0)
const groupNamingOptions: Array<{ id: TaskGroupNamingStrategy; label: string }> = [
  { id: 'date', label: 'tasks.groupNamingDate' },
  { id: 'dateWeekday', label: 'tasks.groupNamingDateWeekday' },
  { id: 'monthDay', label: 'tasks.groupNamingMonthDay' },
  { id: 'defaultName', label: 'tasks.groupNamingDefaultName' },
]

// Column width: fit up to 3 columns; 4+ columns scroll horizontally.
function columnStyle(colCount: number) {
  if (colCount <= 1) return 'flex: 1 1 0%;'
  if (colCount === 2) return 'flex: 1 1 calc(50% - 8px);'
  if (colCount === 3) return 'flex: 1 1 calc((100% - 32px) / 3);'
  return 'flex: 0 0 260px;'
}

// Filtered groups based on search
const filteredGroups = computed(() => {
  if (!isSearching.value) return groups.value.filter(shouldDisplayGroup)
  const q = searchQuery.value.trim().toLowerCase()
  return groups.value
    .map((g: GroupData) => ({
      ...g,
      columns: g.columns
        .map((c: ColumnWithTasks) => ({
          ...c,
          tasks: c.tasks.filter((task: Task) => task.title.toLowerCase().includes(q)),
        }))
        .filter((c: ColumnWithTasks) => c.tasks.length > 0),
    }))
    .filter((g: GroupData) => g.columns.length > 0)
})
const selectedTaskCount = computed(() => selectedTaskIds.value.size)
const visibleSelectedTasks = computed(() => {
  const selected = selectedTaskIds.value
  return groups.value.flatMap((group) => group.columns.flatMap((column) => column.tasks)).filter((task) => selected.has(task.id))
})
const bulkMoveColumnOptions = computed(() =>
  groups.value.flatMap((group) =>
    group.columns.map((column) => ({
      id: column.column.id,
      label: `${displayGroupName(group.project)} / ${column.column.name}`,
    })),
  ),
)
function shouldDisplayGroup(group: GroupData) {
  if (showCompletedPastDateGroups.value) return true
  if (!isDateProject(group.project) || group.project >= todayKey()) return true
  return taskCountByDone(group, false) > 0
}

// Should a group be expanded?
function isGroupExpanded(project: string): boolean {
  if (isSearching.value) return true // expand all during search
  if (collapsedGroups.value[project] === undefined && isDateProject(project)) {
    return project === todayKey()
  }
  return !collapsedGroups.value[project]
}

function toggleGroup(project: string) {
  if (isSearching.value) return
  collapsedGroups.value[project] = !collapsedGroups.value[project]
  saveCollapsedState()
}

function onGroupHeaderClick(project: string) {
  if (Date.now() < suppressGroupClickUntil) return
  toggleGroup(project)
}

function saveCollapsedState() {
  try {
    localStorage.setItem('nalu-collapsed-groups', JSON.stringify(collapsedGroups.value))
  } catch {}
}

function loadCollapsedState() {
  try {
    const saved = localStorage.getItem('nalu-collapsed-groups')
    if (saved) {
      collapsedGroups.value = JSON.parse(saved)
      savedCollapsedGroups.value = { ...collapsedGroups.value }
    }
    showFutureRecurring.value = localStorage.getItem('nalu-show-future-recurring-tasks') === 'true'
    showCompletedPastDateGroups.value = localStorage.getItem('nalu-show-completed-past-date-groups') === 'true'
  } catch {}
}

function saveFutureRecurringState() {
  try {
    localStorage.setItem('nalu-show-future-recurring-tasks', String(showFutureRecurring.value))
  } catch {}
}

function saveCompletedPastDateGroupsState() {
  try {
    localStorage.setItem('nalu-show-completed-past-date-groups', String(showCompletedPastDateGroups.value))
  } catch {}
}

function displayGroupName(project: string): string {
  return project === 'default' ? t('tasks.defaultGroup') : project
}

function incompleteTaskCount(group: GroupData): number {
  return taskCountByDone(group, false)
}

function totalTaskCount(group: GroupData): number {
  return group.columns.reduce((sum, column) => sum + column.tasks.length, 0)
}

function taskCountByDone(group: GroupData, done: boolean): number {
  return group.columns.reduce((sum, column) => sum + column.tasks.filter((task) => task.done === done).length, 0)
}

function columnIncompleteTaskCount(column: ColumnWithTasks): number {
  return column.tasks.filter((task) => !task.done).length
}

function appendTaskSection(lines: string[], group: GroupData, done: boolean) {
  const sectionStart = lines.length
  lines.push('', `## ${done ? t('tasks.completedTasks') : t('tasks.incompleteTasks')}`)

  for (const column of group.columns) {
    const tasks = column.tasks.filter((task) => task.done === done)
    if (tasks.length === 0) continue
    lines.push('', `[${column.column.name}]`)
    for (const task of tasks) {
      lines.push(`- ${task.title}`)
    }
  }

  if (lines.length === sectionStart + 2) {
    lines.splice(sectionStart)
  }
}

function formatGroupTasksForClipboard(group: GroupData): string {
  const lines = [displayGroupName(group.project)]
  appendTaskSection(lines, group, false)
  appendTaskSection(lines, group, true)
  return lines.join('\n')
}

function padDatePart(value: number): string {
  return String(value).padStart(2, '0')
}

function todayKey(): string {
  const now = new Date()
  return formatDateKey(now)
}

function formatDateKey(date: Date): string {
  return `${date.getFullYear()}-${padDatePart(date.getMonth() + 1)}-${padDatePart(date.getDate())}`
}

function isDateProject(project: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(project)
}

function parseTaskDate(value: string) {
  const [datePart, timePart = '00:00:00'] = value.split('T')
  const [year, month, day] = datePart.split('-').map(Number)
  const [hour = 0, minute = 0] = timePart.split(':').map(Number)
  return new Date(year, month - 1, day, hour, minute)
}

function formatTaskScheduleTime(task: Task): string {
  if (!task.scheduled_start_at) return ''
  const start = parseTaskDate(task.scheduled_start_at)
  const end = task.scheduled_end_at ? parseTaskDate(task.scheduled_end_at) : null
  const startText = `${padDatePart(start.getHours())}:${padDatePart(start.getMinutes())}`
  if (!end) return startText
  return `${startText}-${padDatePart(end.getHours())}:${padDatePart(end.getMinutes())}`
}

function selectedCountLabel() {
  return t('tasks.selectedCount').replace('{count}', String(selectedTaskCount.value))
}

function formatCurrentGroupDate(strategy: TaskGroupNamingStrategy): string {
  const now = new Date()
  const year = now.getFullYear()
  const month = padDatePart(now.getMonth() + 1)
  const day = padDatePart(now.getDate())
  if (strategy === 'dateWeekday') {
    const weekday = new Intl.DateTimeFormat(locale.value === 'zh' ? 'zh-CN' : 'en-US', { weekday: 'short' }).format(now)
    return `${year}-${month}-${day} ${weekday}`
  }
  if (strategy === 'monthDay') {
    return locale.value === 'zh' ? `${Number(month)}月${Number(day)}日` : `${month}/${day}`
  }
  if (strategy === 'defaultName') {
    return settings.taskGroupNaming.fallbackName.trim() || t('tasks.groupNamingFallbackDefault')
  }
  return `${year}-${month}-${day}`
}

function nextAvailableGroupName(baseName: string, fallbackName = settings.taskGroupNaming.fallbackName): string {
  const existing = new Set(groups.value.map((group) => group.project))
  if (!existing.has(baseName)) return baseName

  const fallback = fallbackName.trim() || t('tasks.groupNamingFallbackDefault')
  if (!existing.has(fallback)) return fallback

  let index = 2
  let candidate = `${fallback} ${index}`
  while (existing.has(candidate)) {
    index += 1
    candidate = `${fallback} ${index}`
  }
  return candidate
}

function nextDefaultGroupName(): string {
  return nextAvailableGroupName(todayKey())
}

function toggleGroupSettings() {
  groupSettingsOpen.value = !groupSettingsOpen.value
}

function setGroupNamingStrategy(strategy: TaskGroupNamingStrategy) {
  settings.taskGroupNaming.strategy = strategy
  settings.saveTaskGroupNaming()
  if (addingGroup.value) {
    newGroupName.value = nextDefaultGroupName()
  }
}

function saveGroupNamingFallback() {
  settings.saveTaskGroupNaming()
  if (addingGroup.value) {
    newGroupName.value = nextDefaultGroupName()
  }
}

// Data loading
async function loadBoard() {
  try {
    if (!isTauriRuntime()) {
      syncLegacyBrowserTasks()
    }
    groups.value = isTauriRuntime()
      ? await invoke('get_board', { includeFutureRecurring: showFutureRecurring.value })
      : loadLocalBoard()
  } catch (error) {
    console.error('Failed to load board:', error)
  }
}

function isTauriRuntime() {
  return typeof window !== 'undefined' && Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
}

function loadAllLocalTasks(): Task[] {
  try {
    const raw = localStorage.getItem(LOCAL_CALENDAR_TASKS_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function saveAllLocalTasks(items: Task[]) {
  localStorage.setItem(LOCAL_CALENDAR_TASKS_KEY, JSON.stringify(items))
}

function loadAllLocalColumns(): TaskColumn[] {
  try {
    const raw = localStorage.getItem(LOCAL_TASK_COLUMNS_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function saveAllLocalColumns(items: TaskColumn[]) {
  localStorage.setItem(LOCAL_TASK_COLUMNS_KEY, JSON.stringify(items))
}

function makeLocalDefaultColumn(project: string): TaskColumn {
  return {
    id: `${project}__local_default_column`,
    project,
    name: '重要',
    sort_order: 0,
    created_at: '',
    updated_at: '',
  }
}

function ensureLocalDefaultColumn(project: string) {
  const columns = loadAllLocalColumns()
  const existing = columns.find((column) => column.project === project)
  if (existing) return existing
  const column = makeLocalDefaultColumn(project)
  saveAllLocalColumns([...columns, column])
  return column
}

function syncLegacyBrowserTasks() {
  const existing = loadAllLocalTasks()
  const byId = new Map(existing.map((task) => [task.id, task]))
  const knownProjects = new Set(existing.map((task) => task.project || todayKey()))
  const now = new Date().toISOString()
  let changed = false

  for (const row of legacyBrowserTasks) {
    if (byId.has(row.id)) continue
    const project = row.project || 'default'
    byId.set(row.id, {
      id: row.id,
      project,
      title: row.title,
      done: Boolean(row.done),
      progress: Number(row.progress || (row.done ? 100 : 0)),
      column_id: `${project}__local_default_column`,
      position: Number(row.position || Date.now()),
      created_at: row.created_at || now,
      updated_at: row.updated_at || row.created_at || now,
      scheduled_start_at: null,
      scheduled_end_at: null,
      reminder_minutes: 0,
      completed_at: row.done ? (row.updated_at || now) : null,
      repeat_type: 'none',
      recurrence_series_id: null,
      recurrence_sequence: null,
      recurrence_origin_at: null,
      recurrence_detached: false,
    })
    knownProjects.add(project)
    changed = true
  }

  for (const project of knownProjects) {
    ensureLocalDefaultColumn(project)
  }

  if (!changed) return
  saveAllLocalTasks(Array.from(byId.values()))
  localStorage.removeItem('nalu-collapsed-groups')
}

function shouldShowLocalTask(task: Task) {
  if (showFutureRecurring.value || task.done || !task.recurrence_series_id || !task.scheduled_start_at) return true
  return formatDateKey(parseTaskDate(task.scheduled_start_at)) <= todayKey()
}

function localTaskProject(task: Task) {
  if (task.scheduled_start_at) return formatDateKey(parseTaskDate(task.scheduled_start_at))
  return task.project || todayKey()
}

function loadLocalBoard(): GroupData[] {
  const tasks = loadAllLocalTasks().filter(shouldShowLocalTask)
  const savedColumns = loadAllLocalColumns()
  const projects = Array.from(new Set([...tasks.map(localTaskProject), ...savedColumns.map((column) => column.project), todayKey()])).sort()
  const today = todayKey()
  return projects.map((project, sortIndex) => {
    const projectTasks = tasks.filter((task) => localTaskProject(task) === project)
    const columns = savedColumns
      .filter((column) => column.project === project)
      .sort((a, b) => a.sort_order - b.sort_order)
    if (columns.length === 0) columns.push(makeLocalDefaultColumn(project))
    const knownColumnIds = new Set(columns.map((column) => column.id))
    for (const task of projectTasks) {
      if (!knownColumnIds.has(task.column_id)) {
        columns.push({
          id: task.column_id || `${project}__local_default_column`,
          project,
          name: columns.length === 0 ? '重要' : t('tasks.newColumn'),
          sort_order: columns.length,
          created_at: '',
          updated_at: '',
        })
        knownColumnIds.add(task.column_id)
      }
    }
    return {
      project,
      sort_order: sortIndex,
      columns: columns.map((column) => ({
          column,
          tasks: projectTasks
            .filter((task) => task.column_id === column.id || (!task.column_id && column.id === `${project}__local_default_column`))
            .sort((a, b) => Number(a.done) - Number(b.done) || a.position - b.position),
      })),
    }
  }).filter((group) => {
    if (showFutureRecurring.value || !isDateProject(group.project) || group.project <= today) return true
    return group.columns.some((column) => column.tasks.length > 0)
  })
}

function localProjectFromColumn(columnId: string) {
  const column = loadAllLocalColumns().find((item) => item.id === columnId)
  if (column) return column.project
  return columnId.replace(/__local_default_column$/, '')
}

function addLocalTaskToColumn(title: string, columnId: string) {
  const now = new Date().toISOString()
  const project = localProjectFromColumn(columnId)
  const task: Task = {
    id: crypto.randomUUID(),
    project,
    title,
    done: false,
    progress: 0,
    column_id: columnId,
    position: Date.now(),
    created_at: now,
    updated_at: now,
    scheduled_start_at: null,
    scheduled_end_at: null,
    reminder_minutes: 0,
    completed_at: null,
    repeat_type: 'none',
    recurrence_series_id: null,
    recurrence_sequence: null,
    recurrence_origin_at: null,
    recurrence_detached: false,
  }
  saveAllLocalTasks([...loadAllLocalTasks(), task])
}

function normalizeLocalTaskPositions(tasks: Task[], columnId: string) {
  const columnTasks = tasks
    .filter((task) => task.column_id === columnId)
    .sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at))
  columnTasks.forEach((task, index) => {
    task.position = index
  })
}

function moveLocalTask(task: Task, columnId: string, position: number) {
  const tasks = loadAllLocalTasks()
  const columns = loadAllLocalColumns()
  const targetColumn = columns.find((column) => column.id === columnId) ?? ensureLocalDefaultColumn(localProjectFromColumn(columnId))
  const sourceColumnId = task.column_id
  const movingTask = tasks.find((item) => item.id === task.id)
  if (!movingTask) return

  for (const item of tasks) {
    if (item.column_id === columnId && item.id !== task.id && item.position >= position) {
      item.position += 1
    }
  }
  movingTask.project = targetColumn.project
  movingTask.column_id = columnId
  movingTask.position = position
  movingTask.updated_at = new Date().toISOString()
  normalizeLocalTaskPositions(tasks, sourceColumnId)
  normalizeLocalTaskPositions(tasks, columnId)
  saveAllLocalTasks(tasks)
}

function createLocalColumnByDrag(task: Task, project: string): [TaskColumn, Task] {
  const now = new Date().toISOString()
  const columns = loadAllLocalColumns()
  if (!columns.some((column) => column.project === project)) columns.push(makeLocalDefaultColumn(project))
  const sortOrder = Math.max(-1, ...columns.filter((column) => column.project === project).map((column) => column.sort_order)) + 1
  const column: TaskColumn = {
    id: crypto.randomUUID(),
    project,
    name: t('tasks.newColumn'),
    sort_order: sortOrder,
    created_at: now,
    updated_at: now,
  }
  saveAllLocalColumns([...columns, column])
  moveLocalTask(task, column.id, 0)
  const movedTask = loadAllLocalTasks().find((item) => item.id === task.id) ?? task
  return [column, movedTask]
}

function renameLocalColumn(id: string, name: string) {
  saveAllLocalColumns(loadAllLocalColumns().map((column) => (column.id === id ? { ...column, name, updated_at: new Date().toISOString() } : column)))
}

function deleteLocalColumn(id: string): ColumnSnapshot {
  const columns = loadAllLocalColumns()
  const column = columns.find((item) => item.id === id)
  if (!column) throw new Error('COLUMN_NOT_FOUND')
  if (loadAllLocalTasks().some((task) => task.column_id === id)) throw new Error('NON_EMPTY')
  if (columns.filter((item) => item.project === column.project).length <= 1) throw new Error('LAST_COLUMN')
  saveAllLocalColumns(columns.filter((item) => item.id !== id))
  return { column }
}

function restoreLocalColumn(snapshot: ColumnSnapshot) {
  const columns = loadAllLocalColumns()
  if (columns.some((column) => column.id === snapshot.column.id)) return
  saveAllLocalColumns([...columns, snapshot.column].sort((a, b) => a.project.localeCompare(b.project) || a.sort_order - b.sort_order))
}

function reorderLocalColumns(columnIds: string[]) {
  saveAllLocalColumns(
    loadAllLocalColumns().map((column) => {
      const sortOrder = columnIds.indexOf(column.id)
      return sortOrder === -1 ? column : { ...column, sort_order: sortOrder, updated_at: new Date().toISOString() }
    }),
  )
}

function updateLocalTaskTitle(id: string, title: string) {
  saveAllLocalTasks(loadAllLocalTasks().map((task) => (task.id === id ? { ...task, title, updated_at: new Date().toISOString() } : task)))
}

function toggleLocalTaskDone(id: string) {
  saveAllLocalTasks(
    loadAllLocalTasks().map((task) => {
      if (task.id !== id) return task
      const done = !task.done
      return {
        ...task,
        done,
        progress: done ? 100 : 0,
        completed_at: done ? new Date().toISOString() : null,
        updated_at: new Date().toISOString(),
      }
    }),
  )
}

function deleteLocalTaskById(id: string): Task | null {
  const tasks = loadAllLocalTasks()
  const deleted = tasks.find((task) => task.id === id) ?? null
  saveAllLocalTasks(tasks.filter((task) => task.id !== id))
  return deleted
}

function restoreLocalTask(task: Task) {
  saveAllLocalTasks([...loadAllLocalTasks(), task])
}

function bulkUpdateLocalTasksDone(ids: string[], done: boolean) {
  const idSet = new Set(ids)
  saveAllLocalTasks(
    loadAllLocalTasks().map((task) =>
      idSet.has(task.id)
        ? {
            ...task,
            done,
            progress: done ? 100 : 0,
            completed_at: done ? task.completed_at || new Date().toISOString() : null,
            updated_at: new Date().toISOString(),
          }
        : task,
    ),
  )
}

function bulkDeleteLocalTasks(ids: string[]): TaskSnapshot[] {
  const idSet = new Set(ids)
  const tasks = loadAllLocalTasks()
  const snapshots = tasks.filter((task) => idSet.has(task.id)).map((task) => ({ task }))
  saveAllLocalTasks(tasks.filter((task) => !idSet.has(task.id)))
  return snapshots
}

function restoreLocalTasks(snapshots: TaskSnapshot[]) {
  const existing = loadAllLocalTasks()
  const existingIds = new Set(existing.map((task) => task.id))
  saveAllLocalTasks([...existing, ...snapshots.map((snapshot) => snapshot.task).filter((task) => !existingIds.has(task.id))])
}

function bulkMoveLocalTasks(ids: string[], targetColumnId: string) {
  ids.forEach((id, index) => {
    const task = loadAllLocalTasks().find((item) => item.id === id)
    if (task) moveLocalTask(task, targetColumnId, Date.now() + index)
  })
}

async function toggleFutureRecurring() {
  showFutureRecurring.value = !showFutureRecurring.value
  clearTaskSelection()
  saveFutureRecurringState()
  await loadBoard()
}

async function toggleCompletedPastDateGroups() {
  showCompletedPastDateGroups.value = !showCompletedPastDateGroups.value
  clearTaskSelection()
  saveCompletedPastDateGroupsState()
  await loadBoard()
}

// Search watcher: save/restore collapse state
watch(isSearching, (searching: boolean) => {
  clearTaskSelection()
  if (searching) {
    savedCollapsedGroups.value = { ...collapsedGroups.value }
  } else {
    collapsedGroups.value = { ...savedCollapsedGroups.value }
  }
})

function onDocumentKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') clearTaskSelection()
}

// Task operations
async function addTaskToColumn(columnId: string) {
  const title = newTaskTitle.value.trim()
  if (!title || savingNewTask.value) return
  savingNewTask.value = true
  try {
    if (isTauriRuntime()) {
      await invoke('add_task_to_column', { title, columnId })
    } else {
      addLocalTaskToColumn(title, columnId)
    }
    newTaskTitle.value = ''
    addingColumnId.value = null
    await loadBoard()
  } catch (error) {
    console.error('Failed to add task:', error)
  } finally {
    savingNewTask.value = false
  }
}

function toggleTaskSelection(id: string) {
  const next = new Set(selectedTaskIds.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  selectedTaskIds.value = next
}

function clearTaskSelection() {
  if (selectedTaskIds.value.size === 0) return
  selectedTaskIds.value = new Set()
  bulkMoveTargetColumnId.value = ''
}

function onTaskCardClick(event: MouseEvent) {
  if (event.metaKey || event.ctrlKey) {
    event.preventDefault()
    return
  }
}

function onTaskTitleClick(event: MouseEvent, task: Task) {
  if (event.metaKey || event.ctrlKey) {
    event.preventDefault()
    return
  }
  startEditTask(task)
}

function selectedTaskIdsInDisplayOrder() {
  const selected = selectedTaskIds.value
  return groups.value.flatMap((group) => group.columns.flatMap((column) => column.tasks)).filter((task) => selected.has(task.id)).map((task) => task.id)
}

async function bulkSetTasksDone(done: boolean) {
  const ids = selectedTaskIdsInDisplayOrder()
  if (ids.length === 0) return
  try {
    if (isTauriRuntime()) {
      await invoke('bulk_update_tasks_done', { ids, done })
    } else {
      bulkUpdateLocalTasksDone(ids, done)
    }
    showToast(done ? t('tasks.bulkCompleteSuccess') : t('tasks.bulkReopenSuccess'), null)
    clearTaskSelection()
    await loadBoard()
  } catch (error) {
    console.error('Failed to bulk update tasks:', error)
  }
}

async function bulkDeleteSelectedTasks() {
  const ids = selectedTaskIdsInDisplayOrder()
  if (ids.length === 0) return
  if (!window.confirm(t('tasks.bulkDeleteConfirm'))) return
  try {
    const snapshots: TaskSnapshot[] = isTauriRuntime()
      ? await invoke('bulk_delete_tasks_with_snapshot', { ids })
      : bulkDeleteLocalTasks(ids)
    clearTaskSelection()
    await loadBoard()
    showToast(t('tasks.bulkDeleteSuccess'), async () => {
      if (isTauriRuntime()) {
        await invoke('restore_tasks', { snapshots })
      } else {
        restoreLocalTasks(snapshots)
      }
      await loadBoard()
    })
  } catch (error) {
    console.error('Failed to bulk delete tasks:', error)
  }
}

async function bulkMoveSelectedTasks() {
  const ids = selectedTaskIdsInDisplayOrder()
  if (ids.length === 0 || !bulkMoveTargetColumnId.value) return
  try {
    if (isTauriRuntime()) {
      await invoke('bulk_move_tasks', { ids, targetColumnId: bulkMoveTargetColumnId.value })
    } else {
      bulkMoveLocalTasks(ids, bulkMoveTargetColumnId.value)
    }
    showToast(t('tasks.bulkMoveSuccess'), null)
    clearTaskSelection()
    await loadBoard()
  } catch (error) {
    console.error('Failed to bulk move tasks:', error)
  }
}

async function scrollToTodayGroup() {
  const project = todayKey()
  collapsedGroups.value[project] = false
  saveCollapsedState()
  if (!groups.value.some((group) => group.project === project)) {
    try {
      if (isTauriRuntime()) {
        await invoke('create_task_group', { project })
      }
      await loadBoard()
    } catch (error) {
      console.error('Failed to ensure today group:', error)
    }
  }
  await nextTick()
  document.querySelector<HTMLElement>(`[data-task-group-target="${project}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
}

function startAddTaskToColumn(columnId: string) {
  if (isSearching.value) return
  addingColumnId.value = columnId
  newTaskTitle.value = ''
  nextTick(() => {
    const input = document.querySelector(`[data-col-add-input="${columnId}"]`) as HTMLInputElement
    input?.focus()
  })
}

function cancelAddTask() {
  addingColumnId.value = null
  newTaskTitle.value = ''
}

function onAddTaskBlur(columnId: string) {
  if (newTaskTitle.value.trim()) {
    addTaskToColumn(columnId)
  } else {
    cancelAddTask()
  }
}

function startAddGroup() {
  if (isSearching.value) return
  addingGroup.value = true
  newGroupName.value = nextDefaultGroupName()
  nextTick(() => {
    const input = document.querySelector('[data-group-add-input]') as HTMLInputElement
    input?.focus()
    input?.select()
  })
}

function cancelAddGroup() {
  addingGroup.value = false
  newGroupName.value = ''
}

async function saveNewGroup() {
  const project = newGroupName.value.trim()
  if (!project || savingNewGroup.value) {
    if (!project) cancelAddGroup()
    return
  }

  savingNewGroup.value = true
  try {
    await invoke('create_task_group', { project })
    collapsedGroups.value[project] = false
    saveCollapsedState()
    cancelAddGroup()
    await loadBoard()
  } catch (error) {
    if (error === 'GROUP_EXISTS') {
      showToast(t('tasks.groupAlreadyExists'), null)
    } else {
      console.error('Failed to create group:', error)
    }
  } finally {
    savingNewGroup.value = false
  }
}

async function copyGroup(project: string) {
  if (isSearching.value) return
  groupMenuProject.value = null
  try {
    const copiedGroup: GroupData = await invoke('copy_task_group', { project })
    collapsedGroups.value[copiedGroup.project] = false
    saveCollapsedState()
    await loadBoard()
    showToast(t('tasks.copyGroupSuccess'), null)
  } catch (error) {
    console.error('Failed to copy group:', error)
  }
}

async function copyGroupTasksToClipboard(group: GroupData) {
  if (isSearching.value) return
  groupMenuProject.value = null
  const count = taskCountByDone(group, false) + taskCountByDone(group, true)
  if (count === 0) {
    showToast(t('tasks.noTasksToCopy'), null)
    return
  }

  try {
    await writeText(formatGroupTasksForClipboard(group))
    showToast(t('tasks.copyTasksSuccess'), null)
  } catch (error) {
    console.error('Failed to copy group tasks:', error)
  }
}

async function completeGroup(group: GroupData) {
  if (isSearching.value || incompleteTaskCount(group) === 0) return
  groupMenuProject.value = null
  try {
    await invoke('complete_task_group', { project: group.project })
    await loadBoard()
    showToast(t('tasks.completeGroupSuccess'), null)
  } catch (error) {
    console.error('Failed to complete group:', error)
  }
}

async function deleteGroup(project: string) {
  if (isSearching.value || project === 'default') return
  groupMenuProject.value = null
  try {
    await invoke('delete_task_group', { project })
    delete collapsedGroups.value[project]
    saveCollapsedState()
    await loadBoard()
    showToast(t('tasks.deleteGroupSuccess'), null)
  } catch (error: any) {
    if (error === 'HAS_INCOMPLETE_TASKS') {
      showToast(t('tasks.groupHasIncompleteTasks'), null)
    } else if (error === 'DEFAULT_GROUP') {
      showToast(t('tasks.defaultGroupCannotDelete'), null)
    } else {
      console.error('Failed to delete group:', error)
    }
  }
}

function startEditGroup(project: string) {
  if (isSearching.value || project === 'default') return
  editingGroupProject.value = project
  editGroupName.value = project
  nextTick(() => {
    const input = document.querySelector(`[data-group-edit-input="${project}"]`) as HTMLInputElement
    input?.focus()
    input?.select()
  })
}

function cancelEditGroup() {
  editingGroupProject.value = null
  editGroupName.value = ''
}

async function saveEditGroup() {
  const project = editingGroupProject.value
  const name = editGroupName.value.trim()
  if (!project || savingGroupRename.value) return
  if (!name || name === project) {
    cancelEditGroup()
    return
  }

  savingGroupRename.value = true
  try {
    const renamedGroup: GroupData = await invoke('rename_task_group', { project, name })
    collapsedGroups.value[renamedGroup.project] = collapsedGroups.value[project] ?? false
    delete collapsedGroups.value[project]
    saveCollapsedState()
    cancelEditGroup()
    await loadBoard()
  } catch (error: any) {
    if (error === 'GROUP_EXISTS') {
      showToast(t('tasks.groupAlreadyExists'), null)
    } else if (error === 'DEFAULT_GROUP') {
      showToast(t('tasks.defaultGroupCannotRename'), null)
    } else {
      console.error('Failed to rename group:', error)
    }
  } finally {
    savingGroupRename.value = false
  }
}

function startEditTask(task: Task) {
  if (Date.now() < suppressTaskClickUntil) return
  editingTaskId.value = task.id
  editTaskTitle.value = task.title
  nextTick(() => {
    const input = document.querySelector(`[data-edit-input="${task.id}"]`) as HTMLInputElement
    input?.focus()
  })
}

async function saveEditTask() {
  if (!editingTaskId.value) return
  const trimmed = editTaskTitle.value.trim()
  if (!trimmed) {
    // Restore old value
    cancelEditTask()
    return
  }
  try {
    if (isTauriRuntime()) {
      await invoke('update_task_content', { id: editingTaskId.value, title: trimmed })
    } else {
      updateLocalTaskTitle(editingTaskId.value, trimmed)
    }
    editingTaskId.value = null
    await loadBoard()
  } catch (error) {
    console.error('Failed to update task:', error)
  }
}

function cancelEditTask() {
  editingTaskId.value = null
}

async function toggleTaskDone(task: Task) {
  try {
    if (isTauriRuntime()) {
      await invoke('toggle_task', { id: task.id })
    } else {
      toggleLocalTaskDone(task.id)
    }
    await loadBoard()
  } catch (error) {
    console.error('Failed to toggle task:', error)
  }
}

async function deleteTask(task: Task) {
  try {
    const snapshot: TaskSnapshot = isTauriRuntime()
      ? await invoke('delete_task_with_snapshot', { id: task.id })
      : { task: deleteLocalTaskById(task.id) ?? task }
    await loadBoard()
    showToast(t('tasks.deleteTaskUndo'), async () => {
      if (isTauriRuntime()) {
        await invoke('restore_task', { snapshot })
      } else {
        restoreLocalTask(snapshot.task)
      }
      await loadBoard()
    })
  } catch (error) {
    console.error('Failed to delete task:', error)
  }
}

// Column operations
function startEditColumn(col: TaskColumn) {
  editingColumnId.value = col.id
  editColumnName.value = col.name
  columnMenuId.value = null
  nextTick(() => {
    const input = document.querySelector(`[data-col-input="${col.id}"]`) as HTMLInputElement
    input?.focus()
  })
}

async function saveEditColumn() {
  if (!editingColumnId.value) return
  const name = editColumnName.value.trim() || t('tasks.newColumn')
  try {
    if (isTauriRuntime()) {
      await invoke('rename_column', { id: editingColumnId.value, name })
    } else {
      renameLocalColumn(editingColumnId.value, name)
    }
    editingColumnId.value = null
    await loadBoard()
  } catch (error) {
    console.error('Failed to rename column:', error)
  }
}

function cancelEditColumn() {
  editingColumnId.value = null
}

async function deleteColumn(col: TaskColumn) {
  columnMenuId.value = null
  try {
    const snapshot: ColumnSnapshot = isTauriRuntime()
      ? await invoke('delete_column', { id: col.id })
      : deleteLocalColumn(col.id)
    await loadBoard()
    showToast(t('tasks.deleteColumnUndo'), async () => {
      if (isTauriRuntime()) {
        await invoke('restore_column', { snapshot })
      } else {
        restoreLocalColumn(snapshot)
      }
      await loadBoard()
    })
  } catch (error: any) {
    const code = error instanceof Error ? error.message : error
    if (code === 'NON_EMPTY') {
      showToast(t('tasks.nonEmptyColumn'), null)
    } else if (code === 'LAST_COLUMN') {
      showToast(t('tasks.lastColumn'), null)
    } else {
      console.error('Failed to delete column:', error)
    }
  }
}

// Toast management
function showToast(message: string, undoAction: (() => Promise<void>) | null) {
  if (toastTimer) clearTimeout(toastTimer)
  toastMessage.value = message
  toastUndoAction.value = undoAction
  toastVisible.value = true
  toastTimer = setTimeout(() => {
    toastVisible.value = false
    toastUndoAction.value = null
  }, 5000)
}

async function doUndo() {
  if (toastUndoAction.value) {
    await toastUndoAction.value()
    toastVisible.value = false
    toastUndoAction.value = null
  }
}

// Drag and drop - Tasks
function onTaskDragStart(e: DragEvent, task: Task) {
  if (isSearching.value) {
    e.preventDefault()
    return
  }
  draggedTask.value = task
  draggedColumn.value = null
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', task.id)
  }
}

function onTaskDragEnd() {
  draggedTask.value = null
  dropTargetColumnId.value = null
  dropTargetPosition.value = -1
  dropTargetNewColumn.value = null
  dropTargetTaskGroupProject.value = null
}

function findColumnById(columnId: string) {
  for (const group of groups.value) {
    const column = group.columns.find((cwt) => cwt.column.id === columnId)
    if (column) return column
  }
  return null
}

function findTaskLocation(taskId: string) {
  for (const group of groups.value) {
    for (const column of group.columns) {
      const index = column.tasks.findIndex((task) => task.id === taskId)
      if (index !== -1) return { group, column, index }
    }
  }
  return null
}

function clearTaskDropTarget() {
  dropTargetColumnId.value = null
  dropTargetPosition.value = -1
  dropTargetNewColumn.value = null
  dropTargetTaskGroupProject.value = null
}

function clearTextSelection() {
  window.getSelection()?.removeAllRanges()
}

function moveTask(task: Task, columnId: string, position: number) {
  if (!isTauriRuntime()) {
    moveLocalTask(task, columnId, position)
    return loadBoard()
  }
  return invoke('move_task', { id: task.id, targetColumnId: columnId, targetPosition: position })
    .then(() => loadBoard())
    .catch((err: unknown) => console.error('Move failed:', err))
}

function createColumnByDrag(task: Task, project: string) {
  const action = isTauriRuntime()
    ? invoke<[TaskColumn, Task]>('create_column_by_drag', { taskId: task.id, project })
    : Promise.resolve(createLocalColumnByDrag(task, project))
  return action
    .then((result) => {
      return loadBoard().then(() => {
        // Auto-enter edit mode for the new column
        editingColumnId.value = result[0].id
        editColumnName.value = result[0].name
        nextTick(() => {
          const input = document.querySelector(`[data-col-input="${result[0].id}"]`) as HTMLInputElement
          input?.focus()
          input?.select()
        })
      })
    })
    .catch((err: unknown) => console.error('Create column by drag failed:', err))
}

function setTaskDropTargetFromPoint(x: number, y: number) {
  const task = pointerTaskDrag.value?.task ?? draggedTask.value
  if (!task) return

  const element = document.elementFromPoint(x, y) as HTMLElement | null
  if (!element) {
    clearTaskDropTarget()
    return
  }

  const newColumnTarget = element.closest<HTMLElement>('[data-new-column-drop-zone]')
  const newColumnProject = newColumnTarget?.dataset.newColumnDropZone
  if (newColumnProject) {
    dropTargetNewColumn.value = newColumnProject
    dropTargetTaskGroupProject.value = null
    dropTargetColumnId.value = null
    dropTargetPosition.value = -1
    return
  }

  dropTargetNewColumn.value = null
  dropTargetTaskGroupProject.value = null

  const taskCard = element.closest<HTMLElement>('[data-task-card]')
  const targetTaskId = taskCard?.dataset.taskCard
  if (taskCard && targetTaskId) {
    const location = findTaskLocation(targetTaskId)
    if (!location) {
      clearTaskDropTarget()
      return
    }

    const rect = taskCard.getBoundingClientRect()
    const insertAfter = y > rect.top + rect.height / 2
    dropTargetColumnId.value = location.column.column.id
    dropTargetPosition.value = location.index + (insertAfter ? 1 : 0)
    return
  }

  const columnList = element.closest<HTMLElement>('[data-column-task-list]')
  const columnId = columnList?.dataset.columnTaskList
  if (columnId) {
    const column = findColumnById(columnId)
    if (!column) {
      clearTaskDropTarget()
      return
    }
    dropTargetColumnId.value = columnId
    dropTargetPosition.value = column.tasks.length
    return
  }

  const groupTarget = element.closest<HTMLElement>('[data-task-group-target]')
  const targetProject = groupTarget?.dataset.taskGroupTarget
  const targetGroup = targetProject ? groups.value.find((group) => group.project === targetProject) : null
  const firstColumn = targetGroup?.columns[0]
  if (targetProject && firstColumn) {
    dropTargetTaskGroupProject.value = targetProject
    dropTargetColumnId.value = firstColumn.column.id
    dropTargetPosition.value = firstColumn.tasks.length
    return
  }

  clearTaskDropTarget()
}

function stopPointerTaskDrag() {
  window.removeEventListener('pointermove', onTaskPointerMove)
  window.removeEventListener('pointerup', onTaskPointerUp)
  window.removeEventListener('pointercancel', onTaskPointerCancel)
  document.body.style.userSelect = ''
  clearTextSelection()
  pointerTaskDrag.value = null
}

function onTaskPointerDown(e: PointerEvent, task: Task) {
  if (isSearching.value || e.button !== 0 || editingTaskId.value === task.id) return
  if (e.metaKey || e.ctrlKey) {
    e.preventDefault()
    e.stopPropagation()
    toggleTaskSelection(task.id)
    return
  }
  const target = e.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return

  document.body.style.userSelect = 'none'
  clearTextSelection()
  pointerTaskDrag.value = {
    task,
    pointerId: e.pointerId,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onTaskPointerMove)
  window.addEventListener('pointerup', onTaskPointerUp)
  window.addEventListener('pointercancel', onTaskPointerCancel)
}

function onTaskPointerMove(e: PointerEvent) {
  const drag = pointerTaskDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const distance = Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY)
  if (!drag.active) {
    if (distance < 6) return
    drag.active = true
    draggedTask.value = drag.task
    draggedColumn.value = null
    columnMenuId.value = null
  }

  e.preventDefault()
  clearTextSelection()
  setTaskDropTargetFromPoint(e.clientX, e.clientY)
}

function onTaskPointerUp(e: PointerEvent) {
  const drag = pointerTaskDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const wasActive = drag.active
  const task = drag.task
  const targetProject = dropTargetNewColumn.value
  const targetColumnId = dropTargetColumnId.value
  const targetPosition = dropTargetPosition.value

  if (wasActive) {
    suppressTaskClickUntil = Date.now() + 250
    e.preventDefault()
  }

  stopPointerTaskDrag()
  onTaskDragEnd()

  if (!wasActive) return
  if (targetProject) {
    createColumnByDrag(task, targetProject)
  } else if (targetColumnId && targetPosition >= 0) {
    moveTask(task, targetColumnId, targetPosition)
  }
}

function onTaskPointerCancel() {
  stopPointerTaskDrag()
  onTaskDragEnd()
}

function onColumnDragOver(e: DragEvent, columnId: string, position: number) {
  if (!draggedTask.value) return
  e.preventDefault()
  e.dataTransfer!.dropEffect = 'move'
  dropTargetColumnId.value = columnId
  dropTargetPosition.value = position
}

function onColumnListDragOver(e: DragEvent, columnId: string, position: number) {
  if (!draggedTask.value) return
  e.preventDefault()
  e.dataTransfer!.dropEffect = 'move'
  dropTargetColumnId.value = columnId
  dropTargetPosition.value = position
}

function onColumnDrop(e: DragEvent, columnId: string, position: number) {
  e.preventDefault()
  if (!draggedTask.value) return
  const task = draggedTask.value
  onTaskDragEnd()
  moveTask(task, columnId, position)
}

function onColumnListDrop(e: DragEvent, columnId: string, position: number) {
  e.preventDefault()
  if (!draggedTask.value) return
  const task = draggedTask.value
  onTaskDragEnd()
  moveTask(task, columnId, position)
}

// Drop zone (new column)
function onDropZoneDragOver(e: DragEvent, project: string) {
  if (!draggedTask.value || isSearching.value) return
  e.preventDefault()
  e.dataTransfer!.dropEffect = 'copy'
  dropTargetNewColumn.value = project
}

function onDropZoneDrop(e: DragEvent, project: string) {
  e.preventDefault()
  if (!draggedTask.value) return
  const task = draggedTask.value
  onTaskDragEnd()
  createColumnByDrag(task, project)
}

function clearGroupDropTarget() {
  dropTargetGroupProject.value = null
}

function stopPointerGroupDrag() {
  window.removeEventListener('pointermove', onGroupPointerMove)
  window.removeEventListener('pointerup', onGroupPointerUp)
  window.removeEventListener('pointercancel', onGroupPointerCancel)
  document.body.style.userSelect = ''
  clearTextSelection()
  pointerGroupDrag.value = null
  draggedGroupProject.value = null
  clearGroupDropTarget()
}

function setGroupDropTargetFromPoint(x: number, y: number) {
  const project = pointerGroupDrag.value?.project
  if (!project) return

  const element = document.elementFromPoint(x, y) as HTMLElement | null
  const target = element?.closest<HTMLElement>('[data-task-group-target]')
  const targetProject = target?.dataset.taskGroupTarget
  dropTargetGroupProject.value = targetProject && targetProject !== project ? targetProject : null
}

function onGroupPointerDown(e: PointerEvent, project: string) {
  if (isSearching.value || editingGroupProject.value === project || e.button !== 0) return
  const target = e.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return

  document.body.style.userSelect = 'none'
  clearTextSelection()
  pointerGroupDrag.value = {
    project,
    pointerId: e.pointerId,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onGroupPointerMove)
  window.addEventListener('pointerup', onGroupPointerUp)
  window.addEventListener('pointercancel', onGroupPointerCancel)
}

function onGroupPointerMove(e: PointerEvent) {
  const drag = pointerGroupDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const distance = Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY)
  if (!drag.active) {
    if (distance < 6) return
    drag.active = true
    draggedGroupProject.value = drag.project
    draggedTask.value = null
    draggedColumn.value = null
    columnMenuId.value = null
  }

  e.preventDefault()
  clearTextSelection()
  setGroupDropTargetFromPoint(e.clientX, e.clientY)
}

async function onGroupPointerUp(e: PointerEvent) {
  const drag = pointerGroupDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const wasActive = drag.active
  const draggedProject = drag.project
  const targetProject = dropTargetGroupProject.value

  if (wasActive) {
    suppressGroupClickUntil = Date.now() + 250
    e.preventDefault()
  }

  stopPointerGroupDrag()
  if (!wasActive || !targetProject || draggedProject === targetProject) return

  const projects = groups.value.map((group) => group.project)
  const dragIdx = projects.indexOf(draggedProject)
  const dropIdx = projects.indexOf(targetProject)
  if (dragIdx === -1 || dropIdx === -1 || dragIdx === dropIdx) return

  projects.splice(dragIdx, 1)
  projects.splice(dropIdx, 0, draggedProject)

  try {
    await invoke('reorder_task_groups', { projects })
    await loadBoard()
  } catch (error) {
    console.error('Reorder groups failed:', error)
  }
}

function onGroupPointerCancel() {
  stopPointerGroupDrag()
}

// Column reorder
function clearColumnReorderDropTarget() {
  dropTargetColumnReorderId.value = null
}

function stopPointerColumnDrag() {
  window.removeEventListener('pointermove', onColumnPointerMove)
  window.removeEventListener('pointerup', onColumnPointerUp)
  window.removeEventListener('pointercancel', onColumnPointerCancel)
  document.body.style.userSelect = ''
  clearTextSelection()
  pointerColumnDrag.value = null
  draggedColumn.value = null
  clearColumnReorderDropTarget()
}

function setColumnReorderDropTargetFromPoint(x: number, y: number) {
  const column = pointerColumnDrag.value?.column
  if (!column) return

  const element = document.elementFromPoint(x, y) as HTMLElement | null
  const target = element?.closest<HTMLElement>('[data-task-column]')
  const targetColumnId = target?.dataset.taskColumn
  const targetColumn = targetColumnId ? findColumnById(targetColumnId) : null
  dropTargetColumnReorderId.value =
    targetColumn && targetColumn.column.project === column.project && targetColumn.column.id !== column.id
      ? targetColumn.column.id
      : null
}

function onColumnPointerDown(e: PointerEvent, col: TaskColumn) {
  if (isSearching.value || editingColumnId.value === col.id || e.button !== 0) return
  const target = e.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return

  document.body.style.userSelect = 'none'
  clearTextSelection()
  pointerColumnDrag.value = {
    column: col,
    pointerId: e.pointerId,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onColumnPointerMove)
  window.addEventListener('pointerup', onColumnPointerUp)
  window.addEventListener('pointercancel', onColumnPointerCancel)
}

function onColumnPointerMove(e: PointerEvent) {
  const drag = pointerColumnDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const distance = Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY)
  if (!drag.active) {
    if (distance < 6) return
    drag.active = true
    draggedColumn.value = drag.column
    draggedTask.value = null
    columnMenuId.value = null
  }

  e.preventDefault()
  clearTextSelection()
  setColumnReorderDropTargetFromPoint(e.clientX, e.clientY)
}

async function onColumnPointerUp(e: PointerEvent) {
  const drag = pointerColumnDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const wasActive = drag.active
  const dragged = drag.column
  const targetColumnId = dropTargetColumnReorderId.value

  if (wasActive) e.preventDefault()
  stopPointerColumnDrag()
  if (!wasActive || !targetColumnId || dragged.id === targetColumnId) return

  const group = groups.value.find((g) => g.project === dragged.project)
  if (!group) return

  const colIds = group.columns.map((c) => c.column.id)
  const dragIdx = colIds.indexOf(dragged.id)
  const dropIdx = colIds.indexOf(targetColumnId)
  if (dragIdx === -1 || dropIdx === -1 || dragIdx === dropIdx) return

  colIds.splice(dragIdx, 1)
  colIds.splice(dropIdx, 0, dragged.id)

  try {
    if (isTauriRuntime()) {
      await invoke('reorder_columns', { columnIds: colIds })
    } else {
      reorderLocalColumns(colIds)
    }
    await loadBoard()
  } catch (error) {
    console.error('Reorder columns failed:', error)
  }
}

function onColumnPointerCancel() {
  stopPointerColumnDrag()
}

// Column menu
function toggleColumnMenu(colId: string) {
  columnMenuId.value = columnMenuId.value === colId ? null : colId
}

function closeColumnMenu() {
  columnMenuId.value = null
}

function toggleGroupMenu(project: string) {
  groupMenuProject.value = groupMenuProject.value === project ? null : project
}

// Keyboard handlers
function onAddTaskKeydown(e: KeyboardEvent, columnId: string) {
  if (e.key === 'Enter') addTaskToColumn(columnId)
  else if (e.key === 'Escape') cancelAddTask()
}

function onAddGroupKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveNewGroup()
  else if (e.key === 'Escape') cancelAddGroup()
}

function onEditGroupKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveEditGroup()
  else if (e.key === 'Escape') cancelEditGroup()
}

function onEditTaskKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveEditTask()
  else if (e.key === 'Escape') cancelEditTask()
}

function onEditColumnKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveEditColumn()
  else if (e.key === 'Escape') cancelEditColumn()
}

// Click outside handler for floating menus
function onClickOutside(e: MouseEvent) {
  const target = e.target as HTMLElement
  if (columnMenuId.value) {
    if (!target.closest('[data-col-menu]')) {
      closeColumnMenu()
    }
  }
  if (groupSettingsOpen.value && !target.closest('[data-group-settings]')) {
    groupSettingsOpen.value = false
  }
  if (groupMenuProject.value && !target.closest('[data-group-menu]')) {
    groupMenuProject.value = null
  }
}

onMounted(() => {
  loadCollapsedState()
  loadBoard()
  document.addEventListener('click', onClickOutside)
  document.addEventListener('keydown', onDocumentKeydown)
})

onUnmounted(() => {
  document.removeEventListener('click', onClickOutside)
  document.removeEventListener('keydown', onDocumentKeydown)
  stopPointerTaskDrag()
  stopPointerGroupDrag()
  stopPointerColumnDrag()
  if (toastTimer) clearTimeout(toastTimer)
})

useAiRefresh(loadBoard)
</script>

<template>
  <MobileTasksPage v-if="isMobilePlatform" />
  <div v-else class="h-full flex flex-col">
    <!-- Header -->
    <div :class="isCompactWidth ? 'px-3 pt-3 pb-2' : 'px-6 pt-6 pb-3'">
      <div class="mb-4 flex flex-wrap items-center gap-3">
        <h1 class="text-2xl font-bold mr-auto">{{ t('tasks.title') }}</h1>
        <div class="relative" data-group-settings>
          <button
            class="inline-flex items-center justify-center h-8 w-8 rounded-md border transition-colors hover:bg-secondary"
            :title="t('tasks.groupNamingSettings')"
            @click.stop="toggleGroupSettings"
          >
            <Settings2 class="w-4 h-4" />
          </button>
          <div
            v-if="groupSettingsOpen"
            class="absolute right-0 top-10 z-20 w-80 rounded-lg border bg-popover p-3 shadow-lg"
            @click.stop
          >
            <div class="mb-3">
              <h2 class="text-sm font-semibold">{{ t('tasks.groupNamingSettings') }}</h2>
              <p class="mt-1 text-xs text-muted-foreground">{{ t('tasks.groupNamingDesc') }}</p>
            </div>
            <div class="mb-3 space-y-2 border-b pb-3">
              <button
                class="flex h-9 w-full items-center justify-between rounded-md border px-3 text-left text-xs transition-colors hover:bg-secondary"
                :class="showFutureRecurring ? 'border-primary bg-primary/10 text-primary' : ''"
                type="button"
                @click="toggleFutureRecurring"
              >
                <span>{{ t('tasks.showFutureRecurring') }}</span>
                <span class="h-2 w-2 rounded-full" :class="showFutureRecurring ? 'bg-primary' : 'bg-muted-foreground/40'"></span>
              </button>
              <button
                class="flex h-9 w-full items-center justify-between rounded-md border px-3 text-left text-xs transition-colors hover:bg-secondary"
                :class="showCompletedPastDateGroups ? 'border-primary bg-primary/10 text-primary' : ''"
                type="button"
                @click="toggleCompletedPastDateGroups"
              >
                <span>{{ t('tasks.showCompletedPastGroups') }}</span>
                <span class="h-2 w-2 rounded-full" :class="showCompletedPastDateGroups ? 'bg-primary' : 'bg-muted-foreground/40'"></span>
              </button>
            </div>
            <div class="grid grid-cols-2 gap-2">
              <button
                v-for="option in groupNamingOptions"
                :key="option.id"
                class="h-9 rounded-md border px-3 text-left text-xs transition-colors"
                :class="settings.taskGroupNaming.strategy === option.id ? 'border-primary bg-primary/10 text-primary' : 'hover:bg-secondary'"
                @click="setGroupNamingStrategy(option.id)"
              >
                {{ t(option.label) }}
              </button>
            </div>
            <label class="mt-3 block text-xs font-medium text-muted-foreground" for="task-group-fallback-name">
              {{ t('tasks.groupNamingFallback') }}
            </label>
            <Input
              id="task-group-fallback-name"
              v-model="settings.taskGroupNaming.fallbackName"
              class="mt-1 h-8"
              :placeholder="t('tasks.groupNamingFallbackDefault')"
              @blur="saveGroupNamingFallback"
              @keydown.enter.prevent="saveGroupNamingFallback"
            />
          </div>
        </div>
        <div v-if="addingGroup" class="flex items-center gap-2">
          <Input
            v-model="newGroupName"
            data-group-add-input
            class="h-8 w-44"
            :placeholder="t('tasks.groupName')"
            :disabled="savingNewGroup"
            @keydown="onAddGroupKeydown"
            @blur="saveNewGroup"
          />
        </div>
        <button
          v-else
          class="inline-flex items-center gap-1.5 h-8 px-3 rounded-md text-sm border transition-colors hover:bg-secondary disabled:opacity-50"
          :disabled="isSearching"
          @click="startAddGroup"
        >
          <Plus class="w-4 h-4" />
          <span>{{ t('tasks.addGroup') }}</span>
        </button>
      </div>
      <!-- Search bar -->
      <div class="relative max-w-md">
        <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
        <Input
          v-model="searchQuery"
          class="pl-9"
          :placeholder="t('tasks.search')"
        />
      </div>
      <div v-if="isSearching" class="text-xs text-muted-foreground mt-2">
        {{ t('tasks.searchDragDisabled') }}
      </div>
    </div>

    <!-- Board -->
    <div class="flex-1 overflow-y-auto px-6 pb-6" @click="clearTaskSelection">
      <!-- No results -->
      <div v-if="isSearching && filteredGroups.length === 0" class="text-center py-12 text-muted-foreground text-sm">
        {{ t('tasks.searchNoResults') }}
      </div>

      <!-- Groups -->
      <div
        v-for="group in filteredGroups"
        :key="group.project"
        class="mb-6"
        :data-task-group-target="group.project"
        :class="{ 'ring-2 ring-primary/30 rounded-md': dropTargetTaskGroupProject === group.project }"
      >
        <!-- Group header -->
        <div
          class="flex items-center gap-2 py-2 cursor-pointer select-none group"
          :data-task-group="group.project"
          :data-task-group-target="group.project"
          :class="{
            'opacity-50': draggedGroupProject === group.project,
            'ring-2 ring-primary/30 rounded-md': dropTargetGroupProject === group.project,
          }"
          @click="onGroupHeaderClick(group.project)"
          @pointerdown="onGroupPointerDown($event, group.project)"
        >
          <ChevronDown v-if="isGroupExpanded(group.project)" class="w-4 h-4 text-muted-foreground transition-transform" />
          <ChevronRight v-else class="w-4 h-4 text-muted-foreground transition-transform" />
          <input
            v-if="editingGroupProject === group.project"
            v-model="editGroupName"
            :data-group-edit-input="group.project"
            class="h-7 w-44 text-sm font-semibold bg-transparent border-b border-primary outline-none"
            :disabled="savingGroupRename"
            @click.stop
            @keydown="onEditGroupKeydown"
            @blur="saveEditGroup"
          />
          <span
            v-else
            class="font-semibold text-sm"
            :class="group.project === 'default' ? 'cursor-default' : 'cursor-text'"
            @dblclick.stop="startEditGroup(group.project)"
          >{{ displayGroupName(group.project) }}</span>
          <span class="text-xs text-muted-foreground">
            ({{ incompleteTaskCount(group) }}/{{ totalTaskCount(group) }})
          </span>
          <div
            class="relative ml-auto flex items-center gap-1 transition-opacity"
            :class="groupMenuProject === group.project ? 'pointer-events-auto opacity-100' : 'pointer-events-none opacity-0 group-hover:pointer-events-auto group-hover:opacity-100'"
            data-group-menu
          >
            <button
              class="p-1 rounded transition-colors hover:bg-secondary"
              :title="t('tasks.groupActions')"
              @click.stop="toggleGroupMenu(group.project)"
            >
              <MoreHorizontal class="w-3.5 h-3.5 text-muted-foreground" />
            </button>
            <div
              v-if="groupMenuProject === group.project"
              class="absolute right-0 top-full z-30 mt-1 min-w-[180px] overflow-hidden rounded-lg border bg-card shadow-lg"
            >
              <button
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="incompleteTaskCount(group) === 0"
                @click.stop="completeGroup(group)"
              >
                <CheckCircle2 class="w-3.5 h-3.5" />
                {{ t('tasks.completeGroup') }}
              </button>
              <button
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                :disabled="taskCountByDone(group, false) + taskCountByDone(group, true) === 0"
                @click.stop="copyGroupTasksToClipboard(group)"
              >
                <Copy class="w-3.5 h-3.5" />
                {{ t('tasks.copyTasks') }}
              </button>
              <button
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:bg-accent"
                @click.stop="copyGroup(group.project)"
              >
                <Copy class="w-3.5 h-3.5" />
                {{ t('tasks.copyGroup') }}
              </button>
              <button
                v-if="group.project !== 'default'"
                class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-red-400 transition-colors hover:bg-accent"
                @click.stop="deleteGroup(group.project)"
              >
                <Trash2 class="w-3.5 h-3.5" />
                {{ t('tasks.deleteGroup') }}
              </button>
            </div>
          </div>
        </div>

        <!-- Columns (kanban) -->
        <div
          v-if="isGroupExpanded(group.project)"
          class="relative flex gap-4 overflow-x-auto pb-2"
        >
          <div
            v-for="cwt in group.columns"
            :key="cwt.column.id"
            :data-task-column="cwt.column.id"
            class="flex-shrink-0 bg-card border rounded-lg flex flex-col min-h-[300px] max-h-[calc(100vh-280px)] transition-colors"
            :style="columnStyle(group.columns.length)"
            :class="{
              'opacity-50': (draggedTask && draggedTask.column_id === cwt.column.id) || draggedColumn?.id === cwt.column.id,
              'ring-2 ring-primary/30': (dropTargetColumnId === cwt.column.id && draggedTask) || dropTargetColumnReorderId === cwt.column.id,
            }"
          >
            <!-- Column header -->
            <div
              class="flex items-center gap-2 px-3 py-2 border-b"
              :data-column-header="cwt.column.id"
              @pointerdown="onColumnPointerDown($event, cwt.column)"
            >
              <GripHorizontal class="w-3.5 h-3.5 text-muted-foreground/50 cursor-grab flex-shrink-0" />
              <!-- Column name (editable) -->
              <template v-if="editingColumnId === cwt.column.id">
                <input
                  v-model="editColumnName"
                  :data-col-input="cwt.column.id"
                  class="flex-1 text-sm font-medium bg-transparent border-b border-primary outline-none px-0.5"
                  @pointerdown.stop
                  @dragstart.stop
                  @keydown="onEditColumnKeydown"
                  @blur="saveEditColumn"
                />
              </template>
              <template v-else>
                <span
                  class="flex-1 text-sm font-medium cursor-pointer truncate"
                  @dblclick="startEditColumn(cwt.column)"
                >{{ cwt.column.name }}</span>
              </template>
              <span class="text-xs text-muted-foreground flex-shrink-0">{{ columnIncompleteTaskCount(cwt) }}</span>
              <div class="relative flex-shrink-0" data-col-menu>
                <button
                  class="p-1 rounded transition-colors hover:bg-secondary"
                  @click.stop="toggleColumnMenu(cwt.column.id)"
                >
                  <MoreHorizontal class="w-4 h-4 text-muted-foreground" />
                </button>
                <div
                  v-if="columnMenuId === cwt.column.id"
                  class="absolute right-0 top-full mt-1 bg-card border rounded-lg shadow-lg z-30 min-w-[140px] overflow-hidden"
                >
                  <button
                    class="w-full text-left px-3 py-2 text-sm hover:bg-accent transition-colors"
                    @click.stop="startEditColumn(cwt.column)"
                  >{{ t('tasks.renameColumn') }}</button>
                  <button
                    class="w-full text-left px-3 py-2 text-sm hover:bg-accent transition-colors text-red-400"
                    @click.stop="deleteColumn(cwt.column)"
                  >{{ t('tasks.deleteColumn') }}</button>
                </div>
              </div>
            </div>

            <!-- Task list -->
            <div
              class="flex-1 overflow-y-auto p-2 space-y-2 min-h-[40px]"
              :data-column-task-list="cwt.column.id"
              @dragover="onColumnListDragOver($event, cwt.column.id, cwt.tasks.length)"
              @drop="onColumnListDrop($event, cwt.column.id, cwt.tasks.length)"
            >
              <template v-for="(task, idx) in cwt.tasks" :key="task.id">
                <!-- Drop indicator line -->
                <div
                  v-if="dropTargetColumnId === cwt.column.id && dropTargetPosition === idx && draggedTask && draggedTask.id !== task.id"
                  class="h-1 bg-primary rounded-full mx-1 transition-all"
                />
                <!-- Task card -->
                <div
                  class="relative bg-background border rounded-lg p-3 group/card transition-opacity cursor-grab active:cursor-grabbing touch-none"
                  :data-task-card="task.id"
                  :class="{
                    'opacity-40': draggedTask?.id === task.id,
                    'border-primary bg-primary/10 ring-1 ring-primary/40': selectedTaskIds.has(task.id),
                  }"
                  @pointerdown="onTaskPointerDown($event, task)"
                  @click.stop="onTaskCardClick($event)"
                >
                  <!-- Drag handle + Title row -->
                  <div class="flex items-start gap-1.5">
                    <div
                      class="pt-0.5 flex-shrink-0"
                      :data-task-drag-handle="task.id"
                    >
                      <GripHorizontal class="w-3.5 h-3.5 text-muted-foreground/40 group-hover/card:text-muted-foreground/70" />
                    </div>
                    <!-- Task title -->
                    <template v-if="editingTaskId === task.id">
                      <input
                        v-model="editTaskTitle"
                        :data-edit-input="task.id"
                        class="min-w-0 flex-1 pr-12 text-sm bg-transparent border-b border-primary outline-none"
                        @keydown="onEditTaskKeydown"
                        @blur="saveEditTask"
                      />
                    </template>
                    <template v-else>
                      <p
                        class="task-title-clamp min-w-0 flex-1 pr-12 text-sm leading-5 cursor-text"
                        :class="{ 'line-through text-muted-foreground': task.done }"
                        :title="task.title"
                        @click.stop="onTaskTitleClick($event, task)"
                      >{{ task.title }}</p>
                    </template>
                  </div>
                  <span
                    v-if="selectedTaskIds.has(task.id)"
                    class="absolute left-2 top-2 flex h-4 w-4 items-center justify-center rounded-full bg-primary text-[10px] text-primary-foreground"
                  >
                    <CheckCircle2 class="h-3 w-3" />
                  </span>
                  <div v-if="task.scheduled_start_at" class="mt-2 flex items-center gap-2 pl-5 text-xs text-muted-foreground">
                    <span class="rounded-sm bg-secondary px-1.5 py-0.5 tabular-nums">{{ formatTaskScheduleTime(task) }}</span>
                    <span v-if="task.repeat_type && task.repeat_type !== 'none'">{{ t('schedule.repeat') }}</span>
                  </div>

                  <!-- Complete + delete buttons -->
                  <button
                    class="absolute top-2 right-8 opacity-0 group-hover/card:opacity-100 p-1 rounded transition-colors hover:text-primary"
                    :class="{ 'opacity-100 text-primary': task.done }"
                    :title="task.done ? t('tasks.reopenTask') : t('tasks.completeTask')"
                    @pointerdown.stop
                    @click.stop="toggleTaskDone(task)"
                  >
                    <CheckCircle2 v-if="task.done" class="w-3.5 h-3.5" />
                    <Circle v-else class="w-3.5 h-3.5" />
                  </button>
                  <button
                    class="absolute top-2 right-2 opacity-0 group-hover/card:opacity-100 p-1 rounded transition-colors hover:text-red-400"
                    @pointerdown.stop
                    @click.stop="deleteTask(task)"
                  >
                    <Trash2 class="w-3.5 h-3.5" />
                  </button>
                </div>
              </template>

              <!-- Drop indicator at end of column -->
              <div
                v-if="dropTargetColumnId === cwt.column.id && dropTargetPosition === cwt.tasks.length && draggedTask"
                class="h-1 bg-primary rounded-full mx-1 transition-all"
              />
              <div
                class="min-h-6"
                :data-column-drop-tail="cwt.column.id"
                @dragover="onColumnListDragOver($event, cwt.column.id, cwt.tasks.length)"
                @drop="onColumnListDrop($event, cwt.column.id, cwt.tasks.length)"
              />
            </div>

            <!-- Add task input (show on hover at bottom) -->
            <div class="border-t px-2 py-2 flex-shrink-0 group/add-area">
              <template v-if="addingColumnId === cwt.column.id">
                <input
                  v-model="newTaskTitle"
                  :data-col-add-input="cwt.column.id"
                  class="w-full text-sm bg-transparent border-b border-primary outline-none px-1 py-1"
                  :placeholder="t('tasks.placeholder')"
                  :disabled="savingNewTask"
                  @keydown="onAddTaskKeydown($event, cwt.column.id)"
                  @blur="onAddTaskBlur(cwt.column.id)"
                />
              </template>
              <template v-else>
                <button
                  v-if="!isSearching"
                  class="w-full flex items-center gap-1.5 px-1 py-1 text-sm text-muted-foreground/50 hover:text-muted-foreground transition-colors rounded hover:bg-secondary/50"
                  @click="startAddTaskToColumn(cwt.column.id)"
                >
                  <Plus class="w-3.5 h-3.5" />
                  <span>{{ t('tasks.addTask') }}</span>
                </button>
              </template>
            </div>
          </div>

          <!-- New column drop zone (only visible during drag) -->
          <div
            v-if="!isSearching && draggedTask"
            :data-new-column-drop-zone="group.project"
            class="absolute right-0 top-0 bottom-2 z-20 w-[200px] border-2 border-dashed rounded-lg flex flex-col items-center justify-center transition-colors min-h-[200px] shadow-sm backdrop-blur-sm"
            :class="dropTargetNewColumn === group.project ? 'border-primary bg-primary/10 scale-[1.02]' : 'border-muted-foreground/30 hover:border-muted-foreground/50'"
            @dragover="onDropZoneDragOver($event, group.project)"
            @dragleave="dropTargetNewColumn = null"
            @drop="onDropZoneDrop($event, group.project)"
          >
            <Plus class="w-8 h-8 mb-2 text-muted-foreground/60" />
            <span class="text-xs text-muted-foreground text-center px-4">
              {{ t('tasks.dropToCreate') }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <Transition
      enter-active-class="transition-all duration-300 ease-out"
      enter-from-class="opacity-0 translate-y-5"
      leave-active-class="transition-all duration-200 ease-in"
      leave-to-class="opacity-0 translate-y-5"
    >
      <div
        v-if="selectedTaskCount > 0"
        class="fixed bottom-6 left-1/2 z-40 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 flex-wrap items-center gap-2 rounded-lg border bg-card px-4 py-3 shadow-lg"
        @click.stop
      >
        <span class="mr-2 text-sm text-muted-foreground">{{ selectedCountLabel() }}</span>
        <button class="inline-flex h-8 items-center gap-1.5 rounded-md border px-3 text-sm hover:bg-secondary" type="button" @click="bulkSetTasksDone(true)">
          <CheckCircle2 class="h-4 w-4" />
          {{ t('tasks.bulkComplete') }}
        </button>
        <button class="inline-flex h-8 items-center gap-1.5 rounded-md border px-3 text-sm hover:bg-secondary" type="button" @click="bulkSetTasksDone(false)">
          <Circle class="h-4 w-4" />
          {{ t('tasks.bulkReopen') }}
        </button>
        <select v-model="bulkMoveTargetColumnId" class="h-8 min-w-44 rounded-md border bg-background px-2 text-sm">
          <option value="">{{ t('tasks.bulkMovePlaceholder') }}</option>
          <option v-for="column in bulkMoveColumnOptions" :key="column.id" :value="column.id">
            {{ column.label }}
          </option>
        </select>
        <button class="inline-flex h-8 items-center gap-1.5 rounded-md border px-3 text-sm hover:bg-secondary disabled:opacity-50" type="button" :disabled="!bulkMoveTargetColumnId" @click="bulkMoveSelectedTasks">
          {{ t('tasks.bulkMove') }}
        </button>
        <button class="inline-flex h-8 items-center gap-1.5 rounded-md border px-3 text-sm text-red-400 hover:bg-destructive/10" type="button" @click="bulkDeleteSelectedTasks">
          <Trash2 class="h-4 w-4" />
          {{ t('tasks.bulkDelete') }}
        </button>
        <button class="inline-flex h-8 w-8 items-center justify-center rounded-md border text-muted-foreground hover:bg-secondary" type="button" :title="t('tasks.clearSelection')" @click="clearTaskSelection">
          <X class="h-4 w-4" />
        </button>
      </div>
    </Transition>

    <button
      v-if="!isSearching"
      class="fixed bottom-6 right-6 z-30 inline-flex h-11 items-center gap-2 rounded-full border bg-card px-4 text-sm font-medium shadow-lg transition-colors hover:bg-secondary"
      type="button"
      :title="t('tasks.today')"
      @click="scrollToTodayGroup"
    >
      <CalendarDays class="h-4 w-4" />
      {{ t('tasks.today') }}
    </button>

    <!-- Toast -->
    <Transition
      enter-active-class="transition-all duration-300 ease-out"
      enter-from-class="opacity-0 translate-y-5"
      leave-active-class="transition-all duration-200 ease-in"
      leave-to-class="opacity-0 translate-y-5"
    >
      <div
        v-if="toastVisible"
        class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 bg-card border rounded-lg shadow-lg px-4 py-3"
      >
        <span class="text-sm">{{ toastMessage }}</span>
        <button
          v-if="toastUndoAction"
          class="text-sm font-medium text-primary hover:text-primary/80 transition-colors"
          @click="doUndo"
        >{{ t('tasks.undo') }}</button>
      </div>
    </Transition>
  </div>
</template>
