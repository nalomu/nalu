<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  CalendarDays,
  Check,
  ChevronLeft,
  ChevronRight,
  Circle,
  Pencil,
  Plus,
  Trash2,
  X,
} from 'lucide-vue-next'
import type { GroupData, RepeatType, Task } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import { useAiRefresh } from '$lib/composables/useAiRefresh'

type ViewMode = 'day' | 'week' | 'month'
type CalendarTaskInput = {
  title: string
  project?: string
  column_id?: string
  scheduled_start_at: string
  scheduled_end_at: string
  reminder_minutes: number
  repeat_type: RepeatType
  done: boolean
}

interface CalendarCell {
  date: Date
  key: string
  day: number
  isCurrentMonth: boolean
  isToday: boolean
  tasks: Task[]
}

interface DraftRange {
  dayIndex: number
  minute: number
}

interface PositionedTask {
  task: Task
  top: number
  height: number
  left: number
  width: number
  startsBeforeDay: boolean
  endsAfterDay: boolean
}

type DragEditMode = 'move' | 'resize-start' | 'resize-end'

interface TaskSegment {
  task: Task
  startTime: number
  endTime: number
  top: number
  height: number
  startsBeforeDay: boolean
  endsAfterDay: boolean
}

interface PositionedTaskSegment extends TaskSegment {
  laneIndex: number
  laneCount: number
}

interface PanelAnchor {
  x: number
  y: number
}

interface TaskColumnOption {
  columnId: string
  project: string
  label: string
}

const HOUR_ROW_HEIGHT = 64
const SELECTION_STEP_MINUTES = 10
const WEEK_HEADER_HEIGHT = 68
const EVENT_AREA_PERCENT = 90
const EVENT_CELL_INSET_X = 4
const EVENT_CELL_INSET_Y = 4
const EDITOR_PANEL_WIDTH = 520
const EDITOR_PANEL_MARGIN = 16
const MAX_MONTH_EVENTS = 3
const LOCAL_CALENDAR_TASKS_KEY = 'nalu-dev-calendar-tasks'
const LOCAL_TASK_COLUMNS_KEY = 'nalu-dev-task-columns'
const DRAG_THRESHOLD_PX = 6
const MIN_TASK_DURATION_MINUTES = 10

const { t, locale } = useI18n()
const tasks = ref<Task[]>([])
const taskColumnOptions = ref<TaskColumnOption[]>([])
const viewMode = ref<ViewMode>('week')
const anchorDate = ref(startOfDay(new Date()))
const currentTime = ref(new Date())
const dialogOpen = ref(false)
const editingTask = ref<Task | null>(null)
const editorAnchor = ref<PanelAnchor | null>(null)
const editorPanelRef = ref<HTMLElement | null>(null)
const dayScrollRef = ref<HTMLElement | null>(null)
const weekScrollRef = ref<HTMLElement | null>(null)
const selecting = ref<{ start: DraftRange; current: DraftRange } | null>(null)
const selectedCalendarTaskIds = ref<Set<string>>(new Set())
const dragEditingTask = ref<{
  task: Task
  mode: DragEditMode
  pointerId: number
  startX: number
  startY: number
  originStart: Date
  originEnd: Date
  previewStart: Date
  previewEnd: Date
  active: boolean
} | null>(null)
let interval: ReturnType<typeof setInterval>
let suppressCalendarClickUntil = 0

const taskForm = ref({
  title: '',
  project: formatDateKey(new Date()),
  columnId: '',
  startDate: formatDateKey(new Date()),
  startTime: '09:00',
  endDate: formatDateKey(new Date()),
  endTime: '10:00',
  reminderMinutes: 0,
  repeatType: 'none' as RepeatType,
  done: false,
})

const hours = Array.from({ length: 24 }, (_, index) => index)
const repeatOptions = computed<Array<{ value: RepeatType; label: string }>>(() => [
  { value: 'none', label: t('schedule.repeatNone') },
  { value: 'daily', label: t('schedule.repeatDaily') },
  { value: 'weekly', label: t('schedule.repeatWeekly') },
  { value: 'monthly', label: t('schedule.repeatMonthly') },
  { value: 'yearly', label: t('schedule.repeatYearly') },
])
const weekDayLabels = computed(() => [
  t('schedule.weekdayMon'),
  t('schedule.weekdayTue'),
  t('schedule.weekdayWed'),
  t('schedule.weekdayThu'),
  t('schedule.weekdayFri'),
  t('schedule.weekdaySat'),
  t('schedule.weekdaySun'),
])
const viewModes = computed<{ value: ViewMode; label: string }[]>(() => [
  { value: 'day', label: t('schedule.day') },
  { value: 'week', label: t('schedule.week') },
  { value: 'month', label: t('schedule.month') },
])
const selectedCalendarTaskCount = computed(() => selectedCalendarTaskIds.value.size)
const editorTaskColumns = computed(() => {
  const currentProject = taskForm.value.project || taskForm.value.startDate || formatDateKey(anchorDate.value)
  const options = new Map(taskColumnOptions.value.map((option) => [option.columnId, option]))
  if (taskForm.value.columnId && !options.has(taskForm.value.columnId)) {
    options.set(taskForm.value.columnId, {
      columnId: taskForm.value.columnId,
      project: currentProject,
      label: `${displayGroupName(currentProject)} / ${t('tasks.newColumn')}`,
    })
  }
  if (options.size === 0) {
    const fallbackColumnId = taskForm.value.columnId || `${currentProject}__local_default_column`
    options.set(fallbackColumnId, {
      columnId: fallbackColumnId,
      project: currentProject,
      label: `${displayGroupName(currentProject)} / ${t('tasks.newColumn')}`,
    })
  }
  return Array.from(options.values()).sort((a, b) => {
    const aIsDate = isDateProject(a.project)
    const bIsDate = isDateProject(b.project)
    if (aIsDate && bIsDate) return a.project.localeCompare(b.project)
    if (aIsDate) return -1
    if (bIsDate) return 1
    return a.label.localeCompare(b.label)
  })
})

const tasksByDate = computed(() => {
  const groups = new Map<string, Task[]>()
  for (const task of tasks.value) {
    if (!task.scheduled_start_at) continue
    const key = formatDateKey(parseTaskDate(task.scheduled_start_at))
    const items = groups.get(key) ?? []
    items.push(task)
    groups.set(key, items)
  }
  for (const items of groups.values()) {
    items.sort((a, b) => parseTaskDate(a.scheduled_start_at || '').getTime() - parseTaskDate(b.scheduled_start_at || '').getTime())
  }
  return groups
})

const selectedDateKey = computed(() => formatDateKey(anchorDate.value))
const selectedDayTasks = computed(() => tasksByDate.value.get(selectedDateKey.value) ?? [])
const weekStart = computed(() => startOfWeek(anchorDate.value))
const weekDays = computed(() =>
  Array.from({ length: 7 }, (_, index) => {
    const date = addDays(weekStart.value, index)
    return {
      date,
      key: formatDateKey(date),
      day: date.getDate(),
      label: weekDayLabels.value[index],
      isToday: isSameDate(date, currentTime.value),
      tasks: tasksByDate.value.get(formatDateKey(date)) ?? [],
    }
  }),
)
const weekHasTasks = computed(() => weekDays.value.some((day) => getPositionedTasksForDate(day.date).length > 0))
const monthCells = computed<CalendarCell[]>(() => {
  const monthStart = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth(), 1)
  const gridStart = startOfWeek(monthStart)
  const nextMonthStart = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth() + 1, 1)
  const gridEnd = addDays(startOfWeek(nextMonthStart), 7)
  const cells: CalendarCell[] = []

  for (let date = new Date(gridStart); date < gridEnd || cells.length < 35; date = addDays(date, 1)) {
    const key = formatDateKey(date)
    cells.push({
      date: new Date(date),
      key,
      day: date.getDate(),
      isCurrentMonth: date.getMonth() === anchorDate.value.getMonth(),
      isToday: isSameDate(date, currentTime.value),
      tasks: tasksByDate.value.get(key) ?? [],
    })
    if (cells.length >= 42) break
  }

  return cells
})
const visibleTitle = computed(() => {
  if (viewMode.value === 'day') return formatFullDate(anchorDate.value)
  if (viewMode.value === 'week') {
    const start = weekStart.value
    const end = addDays(start, 6)
    if (start.getFullYear() === end.getFullYear()) {
      return `${formatMonthDay(start)} - ${formatMonthDay(end)}, ${start.getFullYear()}`
    }
    return `${formatMonthDay(start)}, ${start.getFullYear()} - ${formatMonthDay(end)}, ${end.getFullYear()}`
  }
  return formatMonthTitle(anchorDate.value)
})
const currentHour = computed(() => currentTime.value.getHours())
const currentMinuteOffset = computed(() => `${(currentTime.value.getMinutes() / 60) * 100}%`)
const currentTimeTop = computed(() => `${((currentTime.value.getHours() * 60 + currentTime.value.getMinutes()) / 60) * HOUR_ROW_HEIGHT}px`)
const editorPanelStyle = computed(() => {
  const viewportWidth = typeof window === 'undefined' ? 1280 : window.innerWidth
  const viewportHeight = typeof window === 'undefined' ? 800 : window.innerHeight
  const anchor = editorAnchor.value ?? { x: viewportWidth / 2, y: 160 }
  const width = Math.min(EDITOR_PANEL_WIDTH, viewportWidth - EDITOR_PANEL_MARGIN * 2)
  const placeRight = anchor.x + width + EDITOR_PANEL_MARGIN <= viewportWidth
  const left = placeRight
    ? anchor.x + EDITOR_PANEL_MARGIN
    : Math.max(EDITOR_PANEL_MARGIN, anchor.x - width - EDITOR_PANEL_MARGIN)
  const top = Math.min(Math.max(EDITOR_PANEL_MARGIN, anchor.y - 96), Math.max(EDITOR_PANEL_MARGIN, viewportHeight - 560))
  return {
    left: `${left}px`,
    top: `${top}px`,
    width: `${width}px`,
  }
})

function defaultTaskTitle() {
  return t('schedule.defaultTaskTitle')
}

function setEditorAnchorFromEvent(event?: MouseEvent | PointerEvent) {
  if (!event) {
    editorAnchor.value = null
    return
  }
  editorAnchor.value = { x: event.clientX, y: event.clientY }
}

async function loadTasks() {
  try {
    if (isTauriRuntime()) {
      tasks.value = await invoke('get_calendar_tasks', visibleRange())
      await invoke('ensure_recurring_task_instances')
    } else {
      tasks.value = loadLocalCalendarTasks(visibleRange())
    }
    await loadTaskGroups()
  } catch (error) {
    console.error(error)
  }
}

async function loadTaskGroups() {
  try {
    const options: TaskColumnOption[] = []
    if (isTauriRuntime()) {
      const board = await invoke<GroupData[]>('get_board', { includeFutureRecurring: true })
      for (const group of board) {
        for (const column of group.columns) {
          options.push({
            columnId: column.column.id,
            project: group.project,
            label: `${displayGroupName(group.project)} / ${column.column.name}`,
          })
        }
      }
    } else {
      const columns = loadAllLocalTaskColumns()
      for (const column of columns) {
        options.push({
          columnId: column.id,
          project: column.project,
          label: `${displayGroupName(column.project)} / ${column.name}`,
        })
      }
    }
    if (options.length === 0 && !isTauriRuntime()) {
      const column = ensureLocalDefaultColumn(taskForm.value.project || formatDateKey(anchorDate.value))
      options.push({
        columnId: column.id,
        project: column.project,
        label: `${displayGroupName(column.project)} / ${column.name}`,
      })
    }
    taskColumnOptions.value = options.sort((a, b) => {
      const aIsDate = isDateProject(a.project)
      const bIsDate = isDateProject(b.project)
      if (aIsDate && bIsDate) return a.project.localeCompare(b.project) || a.label.localeCompare(b.label)
      if (aIsDate) return -1
      if (bIsDate) return 1
      return a.label.localeCompare(b.label)
    })
  } catch (error) {
    console.error(error)
    ensureTaskColumnOption(taskForm.value.project || formatDateKey(anchorDate.value), taskForm.value.columnId)
  }
}

function visibleRange() {
  if (viewMode.value === 'day') {
    return {
      startAt: `${formatDateKey(anchorDate.value)}T00:00:00`,
      endAt: `${formatDateKey(anchorDate.value)}T23:59:59`,
    }
  }
  if (viewMode.value === 'week') {
    const start = weekStart.value
    const end = addDays(start, 6)
    return {
      startAt: `${formatDateKey(start)}T00:00:00`,
      endAt: `${formatDateKey(end)}T23:59:59`,
    }
  }
  const start = startOfWeek(new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth(), 1))
  const end = addDays(start, 42)
  return {
    startAt: `${formatDateKey(start)}T00:00:00`,
    endAt: `${formatDateKey(end)}T23:59:59`,
  }
}

function setViewMode(mode: ViewMode) {
  viewMode.value = mode
  loadTasks()
  scrollToCurrentTimeCell('auto')
}

function goToday() {
  anchorDate.value = startOfDay(new Date())
  loadTasks()
}

function goPrevious() {
  if (viewMode.value === 'day') anchorDate.value = addDays(anchorDate.value, -1)
  if (viewMode.value === 'week') anchorDate.value = addDays(anchorDate.value, -7)
  if (viewMode.value === 'month') anchorDate.value = addMonths(anchorDate.value, -1)
  loadTasks()
}

function goNext() {
  if (viewMode.value === 'day') anchorDate.value = addDays(anchorDate.value, 1)
  if (viewMode.value === 'week') anchorDate.value = addDays(anchorDate.value, 7)
  if (viewMode.value === 'month') anchorDate.value = addMonths(anchorDate.value, 1)
  loadTasks()
}

function openCreateDialog(start: Date, end: Date, event?: MouseEvent | PointerEvent) {
  editingTask.value = null
  setEditorAnchorFromEvent(event)
  const project = formatDateKey(start)
  const column = ensureTaskColumnOption(project)
  taskForm.value = {
    title: defaultTaskTitle(),
    project: column.project,
    columnId: column.columnId,
    startDate: formatDateKey(start),
    startTime: formatTime(start),
    endDate: formatDateKey(end),
    endTime: formatTime(end),
    reminderMinutes: 0,
    repeatType: 'none',
    done: false,
  }
  dialogOpen.value = true
}

async function createTaskFromRange(start: Date, end: Date, event?: PointerEvent) {
  const project = formatDateKey(start)
  const column = ensureTaskColumnOption(project)
  const input: CalendarTaskInput = {
    title: defaultTaskTitle(),
    project: column.project,
    column_id: column.columnId || undefined,
    scheduled_start_at: `${formatDateKey(start)}T${formatTime(start)}:00`,
    scheduled_end_at: `${formatDateKey(end)}T${formatTime(end)}:00`,
    reminder_minutes: 0,
    repeat_type: 'none',
    done: false,
  }
  try {
    const createdTask = await createCalendarTask(input)
    anchorDate.value = parseTaskDate(input.scheduled_start_at)
    await loadTasks()
    openEditDialog(createdTask, event)
  } catch (error) {
    console.error(error)
  }
}

function openHeaderCreateDialog(event?: MouseEvent) {
  const start = new Date(anchorDate.value.getFullYear(), anchorDate.value.getMonth(), anchorDate.value.getDate(), 9)
  openCreateDialog(start, new Date(start.getTime() + 60 * 60000), event)
}

function openMonthCreateDialog(date: Date, event?: MouseEvent) {
  openCreateDialog(
    new Date(date.getFullYear(), date.getMonth(), date.getDate(), 0, 0),
    new Date(date.getFullYear(), date.getMonth(), date.getDate(), 23, 59),
    event,
  )
}

function openEditDialog(task: Task, event?: MouseEvent | PointerEvent) {
  if (event && ('metaKey' in event) && (event.metaKey || event.ctrlKey)) return
  if (Date.now() < suppressCalendarClickUntil) return
  if (!task.scheduled_start_at) return
  setEditorAnchorFromEvent(event)
  const start = parseTaskDate(task.scheduled_start_at)
  const end = task.scheduled_end_at ? parseTaskDate(task.scheduled_end_at) : new Date(start.getTime() + 60 * 60000)
  editingTask.value = task
  const column = ensureTaskColumnOption(task.project, task.column_id)
  taskForm.value = {
    title: task.title,
    project: column.project || task.project || formatDateKey(start),
    columnId: column.columnId,
    startDate: formatDateKey(start),
    startTime: formatTime(start),
    endDate: formatDateKey(end),
    endTime: formatTime(end),
    reminderMinutes: task.reminder_minutes ?? 0,
    repeatType: task.repeat_type ?? 'none',
    done: task.done,
  }
  dialogOpen.value = true
}

function closeDialog() {
  dialogOpen.value = false
  editingTask.value = null
  editorAnchor.value = null
}

function toggleCalendarTaskSelection(task: Task) {
  const next = new Set(selectedCalendarTaskIds.value)
  if (next.has(task.id)) next.delete(task.id)
  else next.add(task.id)
  selectedCalendarTaskIds.value = next
}

function clearCalendarTaskSelection() {
  if (selectedCalendarTaskIds.value.size === 0) return
  selectedCalendarTaskIds.value = new Set()
}

function selectedCalendarCountLabel() {
  return t('schedule.selectedCount').replace('{count}', String(selectedCalendarTaskCount.value))
}

function onDocumentPointerDown(event: PointerEvent) {
  if (!dialogOpen.value) return
  const target = event.target as Node | null
  if (target && editorPanelRef.value?.contains(target)) return
  closeDialog()
}

async function saveTask() {
  const title = taskForm.value.title.trim()
  if (!title) return
  const column = selectedTaskColumnOption()
  const input: CalendarTaskInput = {
    title,
    project: column.project || taskForm.value.startDate,
    column_id: column.columnId || undefined,
    scheduled_start_at: `${taskForm.value.startDate}T${taskForm.value.startTime}:00`,
    scheduled_end_at: `${taskForm.value.endDate}T${taskForm.value.endTime}:00`,
    reminder_minutes: taskForm.value.reminderMinutes,
    repeat_type: taskForm.value.repeatType,
    done: taskForm.value.done,
  }
  try {
    if (isTauriRuntime()) {
      if (editingTask.value) {
        await invoke('update_calendar_task', { id: editingTask.value.id, input, scope: 'single' })
      } else {
        await createCalendarTask(input)
      }
    } else if (editingTask.value) {
      updateLocalCalendarTask(editingTask.value.id, input)
    } else {
      await createCalendarTask(input)
    }
  } catch (error) {
    console.error(error)
    return
  }
  anchorDate.value = parseTaskDate(input.scheduled_start_at)
  closeDialog()
  await loadTasks()
}

async function createCalendarTask(input: CalendarTaskInput): Promise<Task> {
  if (isTauriRuntime()) {
    return await invoke('create_calendar_task', { input })
  }
  return createLocalCalendarTask(input)
}

async function toggleTask(task: Task) {
  if (isTauriRuntime()) {
    await invoke('toggle_task', { id: task.id })
  } else {
    toggleLocalCalendarTask(task.id)
  }
  await loadTasks()
}

async function removeFromSchedule(task: Task) {
  if (isTauriRuntime()) {
    await invoke('remove_task_from_schedule', { id: task.id, scope: 'single' })
  } else {
    removeLocalCalendarTaskFromSchedule(task.id)
  }
  closeDialog()
  await loadTasks()
}

async function deleteTask(task: Task) {
  if (isTauriRuntime() && task.recurrence_series_id && window.confirm(t('schedule.deleteFutureRecurringConfirm'))) {
    await invoke('delete_recurring_tasks', { id: task.id, scope: 'future' })
  } else if (isTauriRuntime()) {
    await invoke('delete_task', { id: task.id })
  } else {
    deleteLocalCalendarTask(task.id)
  }
  closeDialog()
  await loadTasks()
}

function isTauriRuntime() {
  return typeof window !== 'undefined' && Boolean((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
}

function loadAllLocalCalendarTasks(): Task[] {
  try {
    const raw = localStorage.getItem(LOCAL_CALENDAR_TASKS_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function loadAllLocalTaskColumns(): Array<{ id: string; project: string; name: string; sort_order: number; created_at: string; updated_at: string }> {
  try {
    const raw = localStorage.getItem(LOCAL_TASK_COLUMNS_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

function saveAllLocalTaskColumns(items: Array<{ id: string; project: string; name: string; sort_order: number; created_at: string; updated_at: string }>) {
  localStorage.setItem(LOCAL_TASK_COLUMNS_KEY, JSON.stringify(items))
}

function ensureLocalDefaultColumn(project: string) {
  const columns = loadAllLocalTaskColumns()
  const existing = columns.find((column) => column.project === project)
  if (existing) return existing
  const now = new Date().toISOString()
  const column = {
    id: `${project}__local_default_column`,
    project,
    name: '重要',
    sort_order: 0,
    created_at: now,
    updated_at: now,
  }
  saveAllLocalTaskColumns([...columns, column])
  return column
}

function saveAllLocalCalendarTasks(items: Task[]) {
  localStorage.setItem(LOCAL_CALENDAR_TASKS_KEY, JSON.stringify(items))
}

function loadLocalCalendarTasks(range: { startAt: string; endAt: string }) {
  const start = parseTaskDate(range.startAt).getTime()
  const end = parseTaskDate(range.endAt).getTime()
  return loadAllLocalCalendarTasks().filter((task) => {
    if (!task.scheduled_start_at) return false
    const taskStart = parseTaskDate(task.scheduled_start_at).getTime()
    const taskEnd = parseTaskDate(task.scheduled_end_at || task.scheduled_start_at).getTime()
    return taskEnd >= start && taskStart <= end
  })
}

function createLocalCalendarTask(input: CalendarTaskInput): Task {
  const now = new Date().toISOString()
  const inputColumn = input.column_id ? loadAllLocalTaskColumns().find((item) => item.id === input.column_id) : null
  const project = inputColumn?.project || input.project || formatDateKey(parseTaskDate(input.scheduled_start_at))
  const column = inputColumn ?? ensureLocalDefaultColumn(project)
  const task: Task = {
    id: crypto.randomUUID(),
    project,
    title: input.title,
    done: input.done,
    progress: input.done ? 100 : 0,
    column_id: column.id,
    position: Date.now(),
    created_at: now,
    updated_at: now,
    scheduled_start_at: input.scheduled_start_at,
    scheduled_end_at: input.scheduled_end_at,
    reminder_minutes: input.reminder_minutes,
    completed_at: input.done ? now : null,
    repeat_type: input.repeat_type,
    recurrence_series_id: null,
    recurrence_sequence: null,
    recurrence_origin_at: null,
    recurrence_detached: false,
  }
  saveAllLocalCalendarTasks([...loadAllLocalCalendarTasks(), task])
  return task
}

function updateLocalCalendarTask(id: string, input: CalendarTaskInput) {
  const inputColumn = input.column_id ? loadAllLocalTaskColumns().find((item) => item.id === input.column_id) : null
  const project = inputColumn?.project || input.project || formatDateKey(parseTaskDate(input.scheduled_start_at))
  const column = inputColumn ?? ensureLocalDefaultColumn(project)
  saveAllLocalCalendarTasks(
    loadAllLocalCalendarTasks().map((task) =>
      task.id === id
        ? {
            ...task,
            project,
            title: input.title,
            done: input.done,
            progress: input.done ? 100 : 0,
            column_id: column.project === project ? column.id : ensureLocalDefaultColumn(project).id,
            scheduled_start_at: input.scheduled_start_at,
            scheduled_end_at: input.scheduled_end_at,
            reminder_minutes: input.reminder_minutes,
            repeat_type: input.repeat_type,
            completed_at: input.done ? task.completed_at || new Date().toISOString() : null,
            updated_at: new Date().toISOString(),
          }
        : task,
    ),
  )
}

function selectedTaskColumnOption() {
  const selected = taskColumnOptions.value.find((option) => option.columnId === taskForm.value.columnId)
  if (selected) return selected
  return ensureTaskColumnOption(taskForm.value.project || taskForm.value.startDate, taskForm.value.columnId)
}

function ensureTaskColumnOption(project: string, columnId = ''): TaskColumnOption {
  const existing = taskColumnOptions.value.find((option) => (columnId ? option.columnId === columnId : option.project === project))
  if (existing) return existing
  const localColumn = !isTauriRuntime()
    ? (columnId ? loadAllLocalTaskColumns().find((column) => column.id === columnId) : null) ?? ensureLocalDefaultColumn(project)
    : null
  const option = {
    columnId: localColumn?.id || columnId,
    project: localColumn?.project || project,
    label: `${displayGroupName(localColumn?.project || project)} / ${localColumn?.name || t('tasks.newColumn')}`,
  }
  if (option.columnId && !taskColumnOptions.value.some((item) => item.columnId === option.columnId)) {
    taskColumnOptions.value = [...taskColumnOptions.value, option].sort((a, b) => a.label.localeCompare(b.label))
  }
  return option
}

function displayGroupName(project: string) {
  return project === 'default' ? t('tasks.defaultGroup') : project
}

function isDateProject(project: string) {
  return /^\d{4}-\d{2}-\d{2}$/.test(project)
}

function toggleLocalCalendarTask(id: string) {
  saveAllLocalCalendarTasks(
    loadAllLocalCalendarTasks().map((task) => {
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

function removeLocalCalendarTaskFromSchedule(id: string) {
  saveAllLocalCalendarTasks(
    loadAllLocalCalendarTasks().map((task) =>
      task.id === id
        ? {
            ...task,
            scheduled_start_at: null,
            scheduled_end_at: null,
            repeat_type: 'none',
            recurrence_series_id: null,
            recurrence_sequence: null,
            recurrence_origin_at: null,
            recurrence_detached: true,
            updated_at: new Date().toISOString(),
          }
        : task,
    ),
  )
}

function deleteLocalCalendarTask(id: string) {
  saveAllLocalCalendarTasks(loadAllLocalCalendarTasks().filter((task) => task.id !== id))
}

function clearTextSelection() {
  window.getSelection()?.removeAllRanges()
}

function onHourPointerDown(dayIndex: number, date: Date, hour: number, event: PointerEvent) {
  if (event.button !== 0) return
  if (dragEditingTask.value) return
  event.preventDefault()
  clearTextSelection()
  const minute = getSnappedMinuteOfDay(hour, event)
  selecting.value = { start: { dayIndex, minute }, current: { dayIndex, minute } }
  window.addEventListener('pointerup', onHourPointerUp, { once: true })
}

function onEventPointerDown(item: PositionedTask, event: PointerEvent) {
  if (event.button !== 0) return
  const target = event.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return
  event.preventDefault()
  clearTextSelection()
  if (event.metaKey || event.ctrlKey) {
    event.stopPropagation()
    toggleCalendarTaskSelection(item.task)
    return
  }
  const card = event.currentTarget as HTMLElement
  const rect = card.getBoundingClientRect()
  const y = event.clientY - rect.top
  const mode: DragEditMode = y <= 8 ? 'resize-start' : y >= rect.height - 8 ? 'resize-end' : 'move'
  const originStart = parseTaskDate(item.task.scheduled_start_at || '')
  const originEnd = item.task.scheduled_end_at ? parseTaskDate(item.task.scheduled_end_at) : new Date(originStart.getTime() + 60 * 60000)
  dragEditingTask.value = {
    task: item.task,
    mode,
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    originStart,
    originEnd,
    previewStart: originStart,
    previewEnd: originEnd,
    active: false,
  }
  document.body.style.userSelect = 'none'
  window.addEventListener('pointermove', onEventPointerMove)
  window.addEventListener('pointerup', onEventPointerUp, { once: true })
  window.addEventListener('pointercancel', onEventPointerCancel, { once: true })
}

function onMonthTaskClick(task: Task, event: MouseEvent) {
  if (event.metaKey || event.ctrlKey) {
    event.preventDefault()
    toggleCalendarTaskSelection(task)
    return
  }
  openEditDialog(task, event)
}

function onCalendarKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') clearCalendarTaskSelection()
}

function onEventPointerMove(event: PointerEvent) {
  const drag = dragEditingTask.value
  if (!drag || drag.pointerId !== event.pointerId) return
  const distance = Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY)
  if (!drag.active) {
    if (distance < DRAG_THRESHOLD_PX) return
    drag.active = true
    clearCalendarTaskSelection()
  }
  event.preventDefault()
  clearTextSelection()
  const point = calendarPointFromPointer(event)
  if (!point) return
  const targetDate = viewMode.value === 'day' ? anchorDate.value : weekDays.value[point.dayIndex]?.date
  if (!targetDate) return
  const target = new Date(targetDate.getFullYear(), targetDate.getMonth(), targetDate.getDate(), 0, point.minute)
  const duration = Math.max((drag.originEnd.getTime() - drag.originStart.getTime()) / 60000, MIN_TASK_DURATION_MINUTES)
  if (drag.mode === 'move') {
    drag.previewStart = target
    drag.previewEnd = new Date(target.getTime() + duration * 60000)
  } else if (drag.mode === 'resize-start') {
    const maxStart = drag.originEnd.getTime() - MIN_TASK_DURATION_MINUTES * 60000
    drag.previewStart = new Date(Math.min(target.getTime(), maxStart))
    drag.previewEnd = drag.originEnd
  } else {
    const minEnd = drag.originStart.getTime() + MIN_TASK_DURATION_MINUTES * 60000
    drag.previewStart = drag.originStart
    drag.previewEnd = new Date(Math.max(target.getTime(), minEnd))
  }
}

async function onEventPointerUp(event: PointerEvent) {
  const drag = dragEditingTask.value
  if (!drag || drag.pointerId !== event.pointerId) return
  const shouldSave = drag.active
  const task = drag.task
  const start = drag.previewStart
  const end = drag.previewEnd
  stopEventDrag()
  if (!shouldSave) return
  suppressCalendarClickUntil = Date.now() + 250
  event.preventDefault()
  await updateTaskTime(task, start, end)
}

function onEventPointerCancel() {
  stopEventDrag()
}

function stopEventDrag() {
  window.removeEventListener('pointermove', onEventPointerMove)
  window.removeEventListener('pointerup', onEventPointerUp)
  window.removeEventListener('pointercancel', onEventPointerCancel)
  document.body.style.userSelect = ''
  clearTextSelection()
  dragEditingTask.value = null
}

async function updateTaskTime(task: Task, start: Date, end: Date) {
  await updateTaskTimeWithoutReload(task, start, end)
  anchorDate.value = startOfDay(start)
  await loadTasks()
}

async function updateTaskTimeWithoutReload(task: Task, start: Date, end: Date) {
  const input: CalendarTaskInput = {
    title: task.title,
    project: task.project || formatDateKey(start),
    column_id: task.column_id || undefined,
    scheduled_start_at: `${formatDateKey(start)}T${formatTime(start)}:00`,
    scheduled_end_at: `${formatDateKey(end)}T${formatTime(end)}:00`,
    reminder_minutes: task.reminder_minutes ?? 0,
    repeat_type: task.repeat_type ?? 'none',
    done: task.done,
  }
  try {
    if (isTauriRuntime()) {
      await invoke('update_calendar_task', { id: task.id, input, scope: 'single' })
    } else {
      updateLocalCalendarTask(task.id, input)
    }
  } catch (error) {
    console.error(error)
    throw error
  }
}

async function bulkDeleteSelectedCalendarTasks() {
  const ids = Array.from(selectedCalendarTaskIds.value)
  if (ids.length === 0) return
  if (!window.confirm(t('schedule.bulkDeleteConfirm'))) return
  try {
    if (isTauriRuntime()) {
      await invoke('bulk_delete_tasks_with_snapshot', { ids })
    } else {
      saveAllLocalCalendarTasks(loadAllLocalCalendarTasks().filter((task) => !selectedCalendarTaskIds.value.has(task.id)))
    }
    clearCalendarTaskSelection()
    closeDialog()
    await loadTasks()
  } catch (error) {
    console.error(error)
  }
}

function calendarPointFromPointer(event: PointerEvent) {
  const layer = document.querySelector<HTMLElement>(viewMode.value === 'week' ? '[data-schedule-layer="week"]' : '[data-schedule-layer="day"]')
  if (!layer) return null
  const rect = layer.getBoundingClientRect()
  const x = Math.min(Math.max(event.clientX - rect.left, 0), rect.width - 1)
  const y = Math.min(Math.max(event.clientY - rect.top, 0), HOUR_ROW_HEIGHT * 24)
  const dayCount = viewMode.value === 'week' ? 7 : 1
  const dayIndex = Math.min(dayCount - 1, Math.max(0, Math.floor((x / rect.width) * dayCount)))
  const minute = Math.min(24 * 60, Math.max(0, Math.round((y / HOUR_ROW_HEIGHT) * (60 / SELECTION_STEP_MINUTES)) * SELECTION_STEP_MINUTES))
  return { dayIndex, minute }
}

function dragPreviewStyleForDate(date: Date) {
  const drag = dragEditingTask.value
  if (!drag?.active) return null
  const dayStart = startOfDay(date)
  const dayEnd = addDays(dayStart, 1)
  const startTime = drag.previewStart.getTime()
  const endTime = Math.max(drag.previewEnd.getTime(), startTime + MIN_TASK_DURATION_MINUTES * 60000)
  if (endTime <= dayStart.getTime() || startTime >= dayEnd.getTime()) return null
  const visibleStart = new Date(Math.max(startTime, dayStart.getTime()))
  const visibleEnd = new Date(Math.min(endTime, dayEnd.getTime()))
  const startMinutes = (visibleStart.getTime() - dayStart.getTime()) / 60000
  const endMinutes = (visibleEnd.getTime() - dayStart.getTime()) / 60000
  return {
    top: `${(startMinutes / 60) * HOUR_ROW_HEIGHT}px`,
    height: `${Math.max(((endMinutes - startMinutes) / 60) * HOUR_ROW_HEIGHT, 24)}px`,
  }
}

function onHourPointerMove(dayIndex: number, hour: number, event: PointerEvent) {
  if (!selecting.value) return
  event.preventDefault()
  clearTextSelection()
  selecting.value.current = { dayIndex, minute: getSnappedMinuteOfDay(hour, event) }
}

function getSnappedMinuteOfDay(hour: number, event: PointerEvent) {
  const target = event.currentTarget as HTMLElement
  const rect = target.getBoundingClientRect()
  const offsetY = Math.min(Math.max(event.clientY - rect.top, 0), rect.height - 1)
  const minuteInHour = Math.floor(((offsetY / rect.height) * 60) / SELECTION_STEP_MINUTES) * SELECTION_STEP_MINUTES
  return hour * 60 + minuteInHour
}

function onHourPointerUp(event: PointerEvent) {
  if (!selecting.value) return
  event.preventDefault()
  clearTextSelection()
  const range = normalizedSelection()
  selecting.value = null
  if (range.start.dayIndex === range.end.dayIndex && range.start.minute === range.end.minute) return
  const startDate = viewMode.value === 'day' ? anchorDate.value : weekDays.value[range.start.dayIndex].date
  const endDate = viewMode.value === 'day' ? anchorDate.value : weekDays.value[range.end.dayIndex].date
  const start = new Date(startDate.getFullYear(), startDate.getMonth(), startDate.getDate(), 0, range.start.minute)
  const end = new Date(endDate.getFullYear(), endDate.getMonth(), endDate.getDate(), 0, range.end.minute + SELECTION_STEP_MINUTES)
  createTaskFromRange(start, end, event)
}

function normalizedSelection() {
  const selection = selecting.value!
  const points = [selection.start, selection.current].sort((a, b) => (a.dayIndex - b.dayIndex) || (a.minute - b.minute))
  return { start: points[0], end: points[1] }
}

function isSelectingCell(dayIndex: number, hour: number) {
  if (!selecting.value) return false
  const range = normalizedSelection()
  const cellStart = hour * 60
  const cellEnd = cellStart + 60
  const selectionStart = range.start.minute
  const selectionEnd = range.end.minute + SELECTION_STEP_MINUTES
  if (dayIndex > range.start.dayIndex && dayIndex < range.end.dayIndex) return true
  if (dayIndex === range.start.dayIndex && dayIndex === range.end.dayIndex) {
    return cellStart < selectionEnd && cellEnd > selectionStart
  }
  if (dayIndex === range.start.dayIndex) return cellEnd > selectionStart
  if (dayIndex === range.end.dayIndex) return cellStart < selectionEnd
  return false
}

function selectionPreviewStyle(dayIndex: number) {
  if (!selecting.value) return null
  const range = normalizedSelection()
  if (dayIndex < range.start.dayIndex || dayIndex > range.end.dayIndex) return null
  const startMinute = dayIndex === range.start.dayIndex ? range.start.minute : 0
  const endMinute = dayIndex === range.end.dayIndex ? range.end.minute + SELECTION_STEP_MINUTES : 24 * 60
  return {
    top: `${(startMinute / 60) * HOUR_ROW_HEIGHT}px`,
    height: `${Math.max(((endMinute - startMinute) / 60) * HOUR_ROW_HEIGHT, 8)}px`,
  }
}

function getPositionedTasksForDate(date: Date): PositionedTask[] {
  const dayStart = startOfDay(date)
  const dayEnd = addDays(dayStart, 1)
  const dayStartTime = dayStart.getTime()
  const dayEndTime = dayEnd.getTime()
  const segments: TaskSegment[] = tasks.value
    .filter((task) => task.scheduled_start_at)
    .map((task) => {
      const start = parseTaskDate(task.scheduled_start_at || '')
      const end = task.scheduled_end_at ? parseTaskDate(task.scheduled_end_at) : new Date(start.getTime() + 60 * 60000)
      const startTime = start.getTime()
      const endTime = Math.max(end.getTime(), startTime + 15 * 60000)
      if (endTime <= dayStartTime || startTime >= dayEndTime) return null
      const visibleStart = new Date(Math.max(startTime, dayStartTime))
      const visibleEnd = new Date(Math.min(endTime, dayEndTime))
      const startMinutes = (visibleStart.getTime() - dayStartTime) / 60000
      const endMinutes = (visibleEnd.getTime() - dayStartTime) / 60000
      return {
        task,
        startTime: visibleStart.getTime(),
        endTime: visibleEnd.getTime(),
        top: (startMinutes / 60) * HOUR_ROW_HEIGHT,
        height: Math.max(((endMinutes - startMinutes) / 60) * HOUR_ROW_HEIGHT, 24),
        startsBeforeDay: startTime < dayStartTime,
        endsAfterDay: endTime > dayEndTime,
      }
    })
    .filter((segment): segment is NonNullable<typeof segment> => Boolean(segment))
    .sort((a, b) => a.startTime - b.startTime || a.endTime - b.endTime)

  const positioned: PositionedTaskSegment[] = []
  let group: TaskSegment[] = []
  let groupEndTime = 0

  function flushGroup() {
    if (group.length === 0) return
    const lanes: Array<{ endTime: number }> = []
    const groupPositioned = group.map((segment) => {
      let laneIndex = lanes.findIndex((lane) => lane.endTime <= segment.startTime)
      if (laneIndex === -1) {
        laneIndex = lanes.length
        lanes.push({ endTime: segment.endTime })
      } else {
        lanes[laneIndex].endTime = segment.endTime
      }
      return { ...segment, laneIndex }
    })
    const laneCount = Math.max(lanes.length, 1)
    positioned.push(...groupPositioned.map((segment) => ({ ...segment, laneCount })))
    group = []
    groupEndTime = 0
  }

  for (const segment of segments) {
    if (group.length > 0 && segment.startTime >= groupEndTime) flushGroup()
    group.push(segment)
    groupEndTime = Math.max(groupEndTime, segment.endTime)
  }
  flushGroup()

  return positioned.map((segment) => ({
    task: segment.task,
    top: segment.top,
    height: segment.height,
    left: (segment.laneIndex / segment.laneCount) * 100,
    width: 100 / segment.laneCount,
    startsBeforeDay: segment.startsBeforeDay,
    endsAfterDay: segment.endsAfterDay,
  }))
}

function formatHour(hour: number) {
  return `${String(hour).padStart(2, '0')}:00`
}

function formatTime(date: Date) {
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

function formatTaskTime(task: Task) {
  if (!task.scheduled_start_at) return ''
  const start = parseTaskDate(task.scheduled_start_at)
  const end = task.scheduled_end_at ? parseTaskDate(task.scheduled_end_at) : null
  return end ? `${formatTime(start)}-${formatTime(end)}` : formatTime(start)
}

function eventBlockStyle(item: PositionedTask) {
  const topInset = item.startsBeforeDay ? 0 : EVENT_CELL_INSET_Y
  return {
    top: `${item.top + topInset}px`,
    height: `${Math.max(item.height - topInset, 24)}px`,
    left: `calc(${(item.left * EVENT_AREA_PERCENT) / 100}% + ${EVENT_CELL_INSET_X}px)`,
    width: `max(24px, calc(${(item.width * EVENT_AREA_PERCENT) / 100}% - ${EVENT_CELL_INSET_X * 2}px))`,
  }
}

function formatFullDate(date: Date) {
  if (locale.value === 'zh') return `${date.getFullYear()} ${t('schedule.year')} ${date.getMonth() + 1} ${t('schedule.monthUnit')} ${date.getDate()} ${t('schedule.dayUnit')}`
  return date.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' })
}

function formatMonthDay(date: Date) {
  if (locale.value === 'zh') return `${date.getMonth() + 1}${t('schedule.monthUnit')}${date.getDate()}${t('schedule.dayUnit')}`
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
}

function formatMonthTitle(date: Date) {
  if (locale.value === 'zh') return `${date.getFullYear()}${t('schedule.year')}${date.getMonth() + 1}${t('schedule.monthUnit')}`
  return date.toLocaleDateString('en-US', { month: 'long', year: 'numeric' })
}

function formatDateKey(date: Date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

function parseTaskDate(value: string) {
  const [datePart, timePart = '00:00:00'] = value.split('T')
  const [year, month, day] = datePart.split('-').map(Number)
  const [hour = 0, minute = 0, second = 0] = timePart.split(':').map(Number)
  return new Date(year, month - 1, day, hour, minute, second)
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

function startOfWeek(date: Date) {
  const day = date.getDay()
  const diff = day === 0 ? -6 : 1 - day
  return addDays(startOfDay(date), diff)
}

function addDays(date: Date, days: number) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days)
}

function addMonths(date: Date, months: number) {
  return new Date(date.getFullYear(), date.getMonth() + months, 1)
}

function isSameDate(left: Date, right: Date) {
  return formatDateKey(left) === formatDateKey(right)
}

function refreshCurrentTime() {
  currentTime.value = new Date()
}

async function scrollToCurrentTimeCell(behavior: ScrollBehavior = 'auto') {
  if (viewMode.value !== 'day' && viewMode.value !== 'week') return
  await nextTick()
  requestAnimationFrame(() => {
    const container = viewMode.value === 'week' ? weekScrollRef.value : dayScrollRef.value
    if (!container) return
    const minutes = currentTime.value.getHours() * 60 + currentTime.value.getMinutes()
    const headerOffset = viewMode.value === 'week' ? WEEK_HEADER_HEIGHT : 0
    const targetTop = headerOffset + (minutes / 60) * HOUR_ROW_HEIGHT
    const centeredTop = targetTop - container.clientHeight * 0.35
    const maxTop = Math.max(0, container.scrollHeight - container.clientHeight)
    container.scrollTo({
      top: Math.min(Math.max(centeredTop, 0), maxTop),
      behavior,
    })
  })
}

onMounted(async () => {
  document.addEventListener('pointerdown', onDocumentPointerDown)
  document.addEventListener('keydown', onCalendarKeydown)
  await loadTasks()
  refreshCurrentTime()
  await scrollToCurrentTimeCell('auto')
  interval = setInterval(refreshCurrentTime, 60000)
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointerDown)
  document.removeEventListener('keydown', onCalendarKeydown)
  clearInterval(interval)
})
useAiRefresh(loadTasks)
</script>

<template>
  <div class="flex h-full min-h-0 flex-col px-4 py-4 md:px-6">
    <header class="grid gap-4 border-b pb-4 lg:grid-cols-[minmax(180px,1fr)_auto_minmax(180px,1fr)] lg:items-center">
      <div class="flex min-w-0 items-center gap-3">
        <h1 class="text-2xl font-semibold">{{ t('schedule.title') }}</h1>
        <button class="inline-flex h-9 items-center gap-2 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90" type="button" @click="openHeaderCreateDialog($event)">
          <Plus class="h-4 w-4" />
          <span>{{ t('schedule.addTask') }}</span>
        </button>
      </div>

      <div class="flex flex-col items-center gap-2 sm:flex-row sm:justify-center">
        <div class="flex items-center rounded-md border bg-card p-1">
          <button :aria-label="t('schedule.previous')" class="rounded-sm p-2 text-muted-foreground hover:bg-secondary hover:text-foreground" type="button" @click="goPrevious">
            <ChevronLeft class="h-4 w-4" />
          </button>
          <button class="rounded-sm px-3 py-2 text-sm font-medium hover:bg-secondary" type="button" @click="goToday">
            {{ t('schedule.today') }}
          </button>
          <button :aria-label="t('schedule.next')" class="rounded-sm p-2 text-muted-foreground hover:bg-secondary hover:text-foreground" type="button" @click="goNext">
            <ChevronRight class="h-4 w-4" />
          </button>
        </div>

        <div class="flex min-w-0 items-center gap-2 text-sm text-muted-foreground">
          <CalendarDays class="h-4 w-4 shrink-0" />
          <span class="truncate">{{ visibleTitle }}</span>
        </div>
      </div>

      <div class="flex justify-center lg:justify-end">
        <div class="flex items-center gap-2 rounded-md border bg-card p-1.5">
          <button
            v-for="mode in viewModes"
            :key="mode.value"
            class="rounded-sm px-4 py-2 text-sm font-medium transition-colors"
            :class="viewMode === mode.value ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground'"
            type="button"
            @click="setViewMode(mode.value)"
          >
            {{ mode.label }}
          </button>
        </div>
      </div>
    </header>

    <main class="min-h-0 flex-1 overflow-hidden pt-4">
      <section ref="dayScrollRef" v-if="viewMode === 'day'" class="h-full select-none overflow-auto rounded-md border bg-card">
        <div class="relative min-w-[360px]">
          <div v-for="hour in hours" :key="hour" class="grid border-b last:border-b-0" :style="{ gridTemplateColumns: '64px minmax(0, 1fr)', minHeight: `${HOUR_ROW_HEIGHT}px` }">
            <div class="border-r px-2 pt-2 text-right text-xs tabular-nums text-muted-foreground">
              {{ formatHour(hour) }}
            </div>
            <div class="relative cursor-cell px-3 py-2" :class="{ 'bg-primary/10': isSelectingCell(0, hour) }" @pointerdown="onHourPointerDown(0, anchorDate, hour, $event)" @pointermove="onHourPointerMove(0, hour, $event)">
            </div>
          </div>
          <div data-schedule-layer="day" class="pointer-events-none absolute left-16 right-0 top-0" :style="{ height: `${HOUR_ROW_HEIGHT * 24}px` }">
            <div v-if="selectionPreviewStyle(0)" class="absolute left-2 right-2 z-10 rounded-md border border-primary/30 bg-primary/15" :style="selectionPreviewStyle(0)!"></div>
            <div v-if="dragPreviewStyleForDate(anchorDate)" class="absolute left-2 right-[10%] z-30 rounded-md border border-primary/50 bg-primary/20 shadow-sm" :style="dragPreviewStyleForDate(anchorDate)!"></div>
            <div v-if="isSameDate(anchorDate, currentTime)" class="absolute left-0 right-0 z-20 border-t border-primary" :style="{ top: currentTimeTop }">
              <span class="absolute -left-1 -top-1.5 h-3 w-3 rounded-full bg-primary"></span>
            </div>
            <div
              v-for="item in getPositionedTasksForDate(anchorDate)"
              :key="`${item.task.id}-${item.top}`"
              class="pointer-events-auto absolute"
              :style="eventBlockStyle(item)"
            >
              <div
                class="group relative flex h-full min-h-6 touch-none select-none items-start gap-2 overflow-hidden rounded-md border border-primary/25 bg-accent/95 px-3 py-2 text-sm shadow-sm"
                :class="{ 'opacity-55': item.task.done || dragEditingTask?.task.id === item.task.id, 'rounded-t-sm': item.startsBeforeDay, 'rounded-b-sm': item.endsAfterDay, 'items-center py-1': item.height < 44, 'border-primary bg-primary/15 ring-1 ring-primary/50': selectedCalendarTaskIds.has(item.task.id) }"
                @pointerdown.stop="onEventPointerDown(item, $event)"
                @click="openEditDialog(item.task, $event)"
              >
                <span class="absolute inset-x-0 top-0 h-2 cursor-ns-resize"></span>
                <span class="absolute inset-x-0 bottom-0 h-2 cursor-ns-resize"></span>
                <button class="mt-0.5 rounded-sm text-muted-foreground hover:text-success" type="button" @click.stop="toggleTask(item.task)">
                  <Check v-if="item.task.done" class="h-4 w-4 text-success" />
                  <Circle v-else class="h-4 w-4" />
                </button>
                <div v-if="item.height < 44" class="min-w-0 flex-1 truncate">
                  <span class="font-medium" :class="{ 'line-through': item.task.done }">{{ item.task.title }}</span>
                  <span class="text-xs tabular-nums text-muted-foreground"> · {{ formatTaskTime(item.task) }}</span>
                </div>
                <div v-else class="min-w-0 flex-1">
                  <div class="truncate font-medium" :class="{ 'line-through': item.task.done }">{{ item.task.title }}</div>
                  <div class="mt-0.5 truncate text-xs tabular-nums text-muted-foreground">{{ formatTaskTime(item.task) }}</div>
                </div>
                <span v-if="selectedCalendarTaskIds.has(item.task.id)" class="absolute right-1 top-1 flex h-4 w-4 items-center justify-center rounded-full bg-primary text-primary-foreground">
                  <Check class="h-3 w-3" />
                </span>
                <Pencil v-if="item.height >= 44" class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100" />
              </div>
            </div>
          </div>
          <div v-if="getPositionedTasksForDate(anchorDate).length === 0" class="pointer-events-none absolute inset-x-16 top-24 text-center text-sm text-muted-foreground">
            {{ t('schedule.noEventsToday') }}
          </div>
        </div>
      </section>

      <section ref="weekScrollRef" v-else-if="viewMode === 'week'" class="h-full select-none overflow-auto rounded-md border bg-card">
        <div class="relative min-w-[960px]">
          <div class="sticky top-0 z-20 grid border-b bg-card" :style="{ gridTemplateColumns: '64px repeat(7, minmax(120px, 1fr))', minHeight: `${WEEK_HEADER_HEIGHT}px` }">
            <div class="border-r"></div>
            <div v-for="day in weekDays" :key="day.key" class="border-r px-2 py-3 text-left last:border-r-0">
              <div class="text-xs text-muted-foreground">{{ day.label }}</div>
              <div class="mt-1 inline-flex h-7 min-w-7 items-center justify-center rounded-full px-2 text-sm font-semibold" :class="day.isToday ? 'bg-primary text-primary-foreground' : ''">
                {{ day.day }}
              </div>
            </div>
          </div>

          <div v-for="hour in hours" :key="hour" class="grid border-b last:border-b-0" :style="{ gridTemplateColumns: '64px repeat(7, minmax(120px, 1fr))', minHeight: `${HOUR_ROW_HEIGHT}px` }">
            <div class="border-r px-2 pt-2 text-right text-xs tabular-nums text-muted-foreground">
              {{ formatHour(hour) }}
            </div>
            <div
              v-for="(day, dayIndex) in weekDays"
              :key="`${day.key}-${hour}`"
              class="relative cursor-cell border-r px-2 py-2 last:border-r-0"
              :class="[day.isToday ? 'bg-accent/30' : '', isSelectingCell(dayIndex, hour) ? 'bg-primary/10' : '']"
              @pointerdown="onHourPointerDown(dayIndex, day.date, hour, $event)"
              @pointermove="onHourPointerMove(dayIndex, hour, $event)"
            >
            </div>
          </div>
          <div data-schedule-layer="week" class="pointer-events-none absolute left-16 right-0 grid" :style="{ top: `${WEEK_HEADER_HEIGHT}px`, gridTemplateColumns: 'repeat(7, minmax(120px, 1fr))', height: `${HOUR_ROW_HEIGHT * 24}px` }">
            <div v-for="(day, dayIndex) in weekDays" :key="`${day.key}-events`" class="relative border-r last:border-r-0">
              <div v-if="selectionPreviewStyle(dayIndex)" class="absolute left-2 right-2 z-10 rounded-md border border-primary/30 bg-primary/15" :style="selectionPreviewStyle(dayIndex)!"></div>
              <div v-if="dragPreviewStyleForDate(day.date)" class="absolute left-2 right-[10%] z-30 rounded-md border border-primary/50 bg-primary/20 shadow-sm" :style="dragPreviewStyleForDate(day.date)!"></div>
              <div v-if="day.isToday" class="absolute left-0 right-0 z-20 border-t border-primary" :style="{ top: currentTimeTop }">
                <span class="absolute -left-1 -top-1.5 h-3 w-3 rounded-full bg-primary"></span>
              </div>
              <div
                v-for="item in getPositionedTasksForDate(day.date)"
                :key="`${day.key}-${item.task.id}-${item.top}`"
                class="pointer-events-auto absolute"
                :style="eventBlockStyle(item)"
              >
                <div
                  class="group relative h-full min-h-6 touch-none select-none overflow-hidden rounded-md border border-primary/25 bg-accent/95 px-2 py-1.5 text-xs shadow-sm"
                  :class="{ 'opacity-55': item.task.done || dragEditingTask?.task.id === item.task.id, 'rounded-t-sm': item.startsBeforeDay, 'rounded-b-sm': item.endsAfterDay, 'border-primary bg-primary/15 ring-1 ring-primary/50': selectedCalendarTaskIds.has(item.task.id) }"
                  @pointerdown.stop="onEventPointerDown(item, $event)"
                  @click="openEditDialog(item.task, $event)"
                >
                  <span class="absolute inset-x-0 top-0 h-2 cursor-ns-resize"></span>
                  <span class="absolute inset-x-0 bottom-0 h-2 cursor-ns-resize"></span>
                  <div class="flex items-start gap-1.5">
                    <button class="mt-0.5 shrink-0 text-muted-foreground hover:text-success" type="button" @click.stop="toggleTask(item.task)">
                      <Check v-if="item.task.done" class="h-3.5 w-3.5 text-success" />
                      <Circle v-else class="h-3.5 w-3.5" />
                    </button>
                    <div class="min-w-0 flex-1">
                      <div class="truncate font-medium" :class="{ 'line-through': item.task.done }">{{ item.task.title }}</div>
                      <div class="truncate tabular-nums text-muted-foreground">{{ formatTaskTime(item.task) }}</div>
                    </div>
                  </div>
                  <span v-if="selectedCalendarTaskIds.has(item.task.id)" class="absolute right-1 top-1 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-primary text-primary-foreground">
                    <Check class="h-2.5 w-2.5" />
                  </span>
                </div>
              </div>
            </div>
          </div>
          <div v-if="!weekHasTasks" class="pointer-events-none absolute left-16 right-0 top-24 text-center text-sm text-muted-foreground">
            {{ t('schedule.noEventsThisWeek') }}
          </div>
        </div>
      </section>

      <section v-else class="h-full select-none overflow-auto rounded-md border bg-card">
        <div class="grid min-w-[720px] grid-cols-7 border-b bg-muted/40">
          <div v-for="label in weekDayLabels" :key="label" class="border-r px-3 py-2 text-xs font-medium text-muted-foreground last:border-r-0">
            {{ label }}
          </div>
        </div>
        <div class="grid min-w-[720px] grid-cols-7">
          <div
            v-for="cell in monthCells"
            :key="cell.key"
            role="button"
            tabindex="0"
            class="min-h-32 border-r border-b p-2 text-left last:border-r-0 hover:bg-secondary/70"
            :class="cell.isCurrentMonth ? 'bg-card' : 'bg-muted/30 text-muted-foreground'"
            @click="openMonthCreateDialog(cell.date, $event)"
            @keydown.enter.prevent="openMonthCreateDialog(cell.date)"
            @keydown.space.prevent="openMonthCreateDialog(cell.date)"
          >
            <div class="mb-2 flex items-center justify-between">
              <span class="inline-flex h-7 min-w-7 items-center justify-center rounded-full px-2 text-sm font-semibold" :class="cell.isToday ? 'bg-primary text-primary-foreground' : ''">
                {{ cell.day }}
              </span>
              <span v-if="cell.tasks.length > MAX_MONTH_EVENTS" class="text-xs text-muted-foreground">
                +{{ cell.tasks.length - MAX_MONTH_EVENTS }}
              </span>
            </div>
            <div class="space-y-1">
              <div
                v-for="task in cell.tasks.slice(0, MAX_MONTH_EVENTS)"
                :key="task.id"
                class="truncate rounded-sm bg-accent px-2 py-1 text-xs"
                :class="{ 'opacity-55 line-through': task.done, 'bg-primary/15 ring-1 ring-primary/50': selectedCalendarTaskIds.has(task.id) }"
                @click.stop="onMonthTaskClick(task, $event)"
              >
                <span class="tabular-nums text-muted-foreground">{{ formatTaskTime(task) }}</span>
                {{ task.title }}
              </div>
            </div>
          </div>
        </div>
      </section>

      <aside ref="editorPanelRef" v-if="dialogOpen" class="fixed z-50 max-h-[calc(100vh-2rem)] overflow-auto rounded-xl border bg-card p-4 shadow-2xl" :style="editorPanelStyle">
      <form @submit.prevent="saveTask">
        <div class="mb-4 flex items-center justify-between">
          <h2 class="text-lg font-semibold">{{ editingTask ? t('schedule.editTask') : t('schedule.addTask') }}</h2>
          <button class="rounded-sm p-1 text-muted-foreground hover:bg-secondary hover:text-foreground" type="button" @click="closeDialog">
            <X class="h-4 w-4" />
          </button>
        </div>

        <div class="space-y-4">
          <Input v-model="taskForm.title" :placeholder="t('schedule.taskTitle')" autofocus />
          <label class="space-y-1 text-sm">
            <span class="text-muted-foreground">{{ t('schedule.taskGroup') }}</span>
            <select v-model="taskForm.columnId" data-schedule-task-group-select class="h-10 w-full rounded-md border bg-background px-3 text-sm">
              <option v-for="column in editorTaskColumns" :key="column.columnId || column.label" :value="column.columnId">
                {{ column.label }}
              </option>
            </select>
          </label>
          <div class="grid gap-3 sm:grid-cols-2">
            <div class="space-y-1 text-sm">
              <span class="text-muted-foreground">{{ t('schedule.startTime') }}</span>
              <div class="grid grid-cols-[1fr_auto] gap-2">
                <Input v-model="taskForm.startDate" type="date" />
                <Input v-model="taskForm.startTime" class="w-28" type="time" />
              </div>
            </div>
            <div class="space-y-1 text-sm">
              <span class="text-muted-foreground">{{ t('schedule.endTime') }}</span>
              <div class="grid grid-cols-[1fr_auto] gap-2">
                <Input v-model="taskForm.endDate" type="date" />
                <Input v-model="taskForm.endTime" class="w-28" type="time" />
              </div>
            </div>
          </div>
          <div class="grid gap-3 sm:grid-cols-2">
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{{ t('schedule.reminderMinutes') }}</span>
              <Input v-model.number="taskForm.reminderMinutes" min="0" type="number" />
            </label>
            <label class="space-y-1 text-sm">
              <span class="text-muted-foreground">{{ t('schedule.repeat') }}</span>
              <select v-model="taskForm.repeatType" class="h-10 w-full rounded-md border bg-background px-3 text-sm">
                <option v-for="option in repeatOptions" :key="option.value" :value="option.value">
                  {{ option.label }}
                </option>
              </select>
            </label>
          </div>
          <label class="flex items-center gap-2 text-sm">
            <input v-model="taskForm.done" type="checkbox" />
            <span>{{ t('schedule.markDone') }}</span>
          </label>
        </div>

        <div class="mt-5 flex flex-wrap items-center gap-2">
          <button v-if="editingTask" class="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm text-muted-foreground hover:bg-secondary" type="button" @click="removeFromSchedule(editingTask)">
            <X class="h-4 w-4" />
            {{ t('schedule.removeFromSchedule') }}
          </button>
          <button v-if="editingTask" class="inline-flex h-9 items-center gap-2 rounded-md border px-3 text-sm text-destructive hover:bg-destructive/10" type="button" @click="deleteTask(editingTask)">
            <Trash2 class="h-4 w-4" />
            {{ t('schedule.deleteTask') }}
          </button>
          <button class="ml-auto inline-flex h-9 items-center justify-center rounded-md border px-4 text-sm hover:bg-secondary" type="button" @click="closeDialog">
            {{ t('common.cancel') }}
          </button>
          <button class="inline-flex h-9 items-center justify-center rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90" type="submit" :disabled="!taskForm.title.trim()">
            {{ t('common.save') }}
          </button>
        </div>
      </form>
      </aside>

      <div
        v-if="selectedCalendarTaskCount > 0"
        class="fixed bottom-6 left-1/2 z-40 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 flex-wrap items-center gap-2 rounded-lg border bg-card px-4 py-3 shadow-lg"
        @click.stop
      >
        <span class="mr-2 text-sm text-muted-foreground">{{ selectedCalendarCountLabel() }}</span>
        <button class="inline-flex h-8 items-center gap-1.5 rounded-md border px-3 text-sm text-destructive hover:bg-destructive/10" type="button" @click="bulkDeleteSelectedCalendarTasks">
          <Trash2 class="h-4 w-4" />
          {{ t('schedule.bulkDelete') }}
        </button>
        <button class="inline-flex h-8 w-8 items-center justify-center rounded-md border text-muted-foreground hover:bg-secondary" type="button" :title="t('schedule.clearSelection')" @click="clearCalendarTaskSelection">
          <X class="h-4 w-4" />
        </button>
      </div>
    </main>
  </div>
</template>
