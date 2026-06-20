<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Plus, Search, ChevronDown, ChevronRight, Trash2, MoreHorizontal, GripHorizontal, Circle, CheckCircle2, Copy, Pencil, ArrowLeft, ArrowRight } from 'lucide-vue-next'
import type { GroupData, Task, TaskColumn, ColumnWithTasks, TaskSnapshot, ColumnSnapshot } from '$lib/types'
import { useMobile } from '$lib/composables/useMobile'
import { useI18n } from '$lib/i18n'
import { Input } from '$lib/components/ui/input'
import { useAiRefresh } from '$lib/composables/useAiRefresh'
import MobileTasksPage from './MobileTasksPage.vue'

const { t } = useI18n()

const { isCompactWidth, isMobilePlatform } = useMobile()

// Board data
const groups = ref<GroupData[]>([])
const searchQuery = ref('')
const collapsedGroups = ref<Record<string, boolean>>({})
const savedCollapsedGroups = ref<Record<string, boolean>>({})

// Task editing
const editingTaskId = ref<string | null>(null)
const editTaskTitle = ref('')

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

// Column editing
const editingColumnId = ref<string | null>(null)
const editColumnName = ref('')
const addingColumnProject = ref<string | null>(null)
const newColumnName = ref('')
const savingNewColumn = ref(false)

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
  element: HTMLElement
  width: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
const pointerGroupDrag = ref<{
  project: string
  pointerId: number
  element: HTMLElement
  width: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
const pointerColumnDrag = ref<{
  column: TaskColumn
  pointerId: number
  element: HTMLElement
  width: number
  startX: number
  startY: number
  active: boolean
} | null>(null)
const dragPreview = ref<{
  type: 'task' | 'column' | 'group'
  title: string
  subtitle?: string
  x: number
  y: number
  width: number
} | null>(null)
let suppressTaskClickUntil = 0
let suppressGroupClickUntil = 0
const nonPassivePointerOptions: AddEventListenerOptions = { passive: false }

// Toast / undo
const toastMessage = ref('')
const toastUndoAction = ref<(() => Promise<void>) | null>(null)
const toastVisible = ref(false)
let toastTimer: ReturnType<typeof setTimeout> | null = null

// Column menu
const columnMenuId = ref<string | null>(null)

const isSearching = computed(() => searchQuery.value.trim().length > 0)
const isPhoneTaskLayout = computed(() => isMobilePlatform.value && isCompactWidth.value)
const mobileEditingTask = computed(() =>
  isMobilePlatform.value && editingTaskId.value ? findTaskById(editingTaskId.value) : null,
)
const mobileEditingColumns = computed(() => {
  const task = mobileEditingTask.value
  if (!task) return [] as ColumnWithTasks[]
  const group = groups.value.find((item) => item.columns.some((column) => column.column.id === task.column_id))
  return group?.columns ?? []
})
const activeMobileColumns = ref<Record<string, string>>({})
const mobileEditColumnId = ref('')
const dragPreviewStyle = computed(() => {
  const preview = dragPreview.value
  if (!preview) return {}
  return {
    width: `${preview.width}px`,
    transform: `translate3d(${preview.x}px, ${preview.y}px, 0)`,
  }
})

function vibrate(pattern: number | number[]) {
  if (!isMobilePlatform.value || typeof navigator === 'undefined' || !navigator.vibrate) return
  try { navigator.vibrate(pattern) } catch {}
}

function startDragPreview(type: 'task' | 'column' | 'group', title: string, event: PointerEvent, width: number, subtitle?: string) {
  dragPreview.value = {
    type,
    title,
    subtitle,
    x: event.clientX + 12,
    y: event.clientY + 12,
    width: Math.min(Math.max(width, 180), type === 'task' ? 280 : 240),
  }
}

function moveDragPreview(event: PointerEvent) {
  if (!dragPreview.value) return
  dragPreview.value.x = event.clientX + 12
  dragPreview.value.y = event.clientY + 12
}

function stopDragPreview() {
  dragPreview.value = null
}

// Column width: fit up to 3 columns; 4+ columns scroll horizontally.
function columnStyle(colCount: number) {
  if (colCount <= 1) return 'flex: 1 1 0%;'
  if (colCount === 2) return 'flex: 1 1 calc(50% - 8px);'
  if (colCount === 3) return 'flex: 1 1 calc((100% - 32px) / 3);'
  return 'flex: 0 0 260px;'
}

// Filtered groups based on search
const filteredGroups = computed(() => {
  if (!isSearching.value) return groups.value
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

// Should a group be expanded?
function isGroupExpanded(project: string): boolean {
  if (isSearching.value) return true // expand all during search
  return !collapsedGroups.value[project]
}

function mobileActiveColumn(group: GroupData) {
  const activeId = activeMobileColumns.value[group.project]
  return group.columns.find((column) => column.column.id === activeId) ?? group.columns[0] ?? null
}

function setMobileActiveColumn(project: string, columnId: string) {
  activeMobileColumns.value = { ...activeMobileColumns.value, [project]: columnId }
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
  } catch {}
}

function displayGroupName(project: string): string {
  return project === 'default' ? t('tasks.defaultGroup') : project
}

// Data loading
async function loadBoard() {
  try {
    groups.value = await invoke('get_board')
  } catch (error) {
    console.error('Failed to load board:', error)
  }
}

// Search watcher: save/restore collapse state
watch(isSearching, (searching: boolean) => {
  if (searching) {
    savedCollapsedGroups.value = { ...collapsedGroups.value }
  } else {
    collapsedGroups.value = { ...savedCollapsedGroups.value }
  }
})

watch(groups, (items) => {
  const next = { ...activeMobileColumns.value }
  for (const group of items) {
    if (!next[group.project] || !group.columns.some((column) => column.column.id === next[group.project])) {
      next[group.project] = group.columns[0]?.column.id ?? ''
    }
  }
  activeMobileColumns.value = next
})

// Task operations
async function addTaskToColumn(columnId: string) {
  const title = newTaskTitle.value.trim()
  if (!title || savingNewTask.value) return
  savingNewTask.value = true
  try {
    await invoke('add_task_to_column', { title, columnId })
    newTaskTitle.value = ''
    addingColumnId.value = null
    await loadBoard()
  } catch (error) {
    console.error('Failed to add task:', error)
  } finally {
    savingNewTask.value = false
  }
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
  newGroupName.value = ''
  nextTick(() => {
    const input = document.querySelector('[data-group-add-input]') as HTMLInputElement
    input?.focus()
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
    console.error('Failed to create group:', error)
  } finally {
    savingNewGroup.value = false
  }
}

async function copyGroup(project: string) {
  if (isSearching.value) return
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

async function deleteGroup(project: string) {
  if (isSearching.value || project === 'default') return
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
    const input = document.querySelector(`[data-group-edit-input="${project}"], [data-mobile-group-edit-input]`) as HTMLInputElement
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

function startAddColumn(project: string) {
  if (isSearching.value) return
  addingColumnProject.value = project
  newColumnName.value = ''
  vibrate(8)
  nextTick(() => {
    const input = document.querySelector('[data-column-add-input]') as HTMLInputElement
    input?.focus()
  })
}

function cancelAddColumn() {
  addingColumnProject.value = null
  newColumnName.value = ''
}

async function saveNewColumn() {
  const project = addingColumnProject.value
  if (!project || savingNewColumn.value) return
  const name = newColumnName.value.trim() || t('tasks.newColumn')
  savingNewColumn.value = true
  try {
    const column: TaskColumn = await invoke('create_column', { project, name })
    setMobileActiveColumn(project, column.id)
    cancelAddColumn()
    await loadBoard()
    vibrate(12)
  } catch (error) {
    console.error('Failed to create column:', error)
  } finally {
    savingNewColumn.value = false
  }
}

function onAddColumnKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') saveNewColumn()
  else if (e.key === 'Escape') cancelAddColumn()
}

function startEditTask(task: Task) {
  if (Date.now() < suppressTaskClickUntil) return
  editingTaskId.value = task.id
  editTaskTitle.value = task.title
  mobileEditColumnId.value = task.column_id
  if (isMobilePlatform.value) {
    vibrate(8)
    nextTick(() => {
      const input = document.querySelector('[data-mobile-task-edit-input]') as HTMLInputElement
      input?.focus()
      input?.select()
    })
    return
  }
  nextTick(() => {
    const input = document.querySelector(`[data-edit-input="${task.id}"]`) as HTMLInputElement
    input?.focus()
  })
}

async function saveEditTask() {
  if (!editingTaskId.value) return
  const taskId = editingTaskId.value
  const originalTask = findTaskById(taskId)
  const trimmed = editTaskTitle.value.trim()
  if (!trimmed) {
    // Restore old value
    cancelEditTask()
    return
  }
  try {
    await invoke('update_task_content', { id: taskId, title: trimmed })
    if (isMobilePlatform.value && originalTask && mobileEditColumnId.value && mobileEditColumnId.value !== originalTask.column_id) {
      const targetColumn = findColumnById(mobileEditColumnId.value)
      await invoke('move_task', {
        id: taskId,
        targetColumnId: mobileEditColumnId.value,
        targetPosition: targetColumn?.tasks.length ?? 0,
      })
    }
    editingTaskId.value = null
    mobileEditColumnId.value = ''
    await loadBoard()
  } catch (error) {
    console.error('Failed to update task:', error)
  }
}

function cancelEditTask() {
  editingTaskId.value = null
  mobileEditColumnId.value = ''
}

async function toggleTaskDone(task: Task) {
  try {
    await invoke('toggle_task', { id: task.id })
    await loadBoard()
  } catch (error) {
    console.error('Failed to toggle task:', error)
  }
}

async function deleteTask(task: Task) {
  try {
    const snapshot: TaskSnapshot = await invoke('delete_task_with_snapshot', { id: task.id })
    await loadBoard()
    showToast(t('tasks.deleteTaskUndo'), async () => {
      await invoke('restore_task', { snapshot })
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
  vibrate(8)
  nextTick(() => {
    const input = document.querySelector(`[data-col-input="${col.id}"], [data-mobile-column-edit-input]`) as HTMLInputElement
    input?.focus()
    input?.select()
  })
}

async function saveEditColumn() {
  if (!editingColumnId.value) return
  const name = editColumnName.value.trim() || t('tasks.newColumn')
  try {
    await invoke('rename_column', { id: editingColumnId.value, name })
    editingColumnId.value = null
    await loadBoard()
  } catch (error) {
    console.error('Failed to rename column:', error)
  }
}

function cancelEditColumn() {
  editingColumnId.value = null
  editColumnName.value = ''
}

async function deleteColumn(col: TaskColumn) {
  columnMenuId.value = null
  try {
    const snapshot: ColumnSnapshot = await invoke('delete_column', { id: col.id })
    await loadBoard()
    showToast(t('tasks.deleteColumnUndo'), async () => {
      await invoke('restore_column', { snapshot })
      await loadBoard()
    })
  } catch (error: any) {
    if (error === 'NON_EMPTY') {
      showToast(t('tasks.nonEmptyColumn'), null)
    } else if (error === 'LAST_COLUMN') {
      showToast(t('tasks.lastColumn'), null)
    } else {
      console.error('Failed to delete column:', error)
    }
  }
}

async function moveMobileColumn(group: GroupData, direction: -1 | 1) {
  if (isSearching.value) return
  const activeColumn = mobileActiveColumn(group)?.column
  if (!activeColumn) return
  const colIds = group.columns.map((c) => c.column.id)
  const currentIndex = colIds.indexOf(activeColumn.id)
  const nextIndex = currentIndex + direction
  if (currentIndex === -1 || nextIndex < 0 || nextIndex >= colIds.length) return
  const [moved] = colIds.splice(currentIndex, 1)
  colIds.splice(nextIndex, 0, moved)
  try {
    await invoke('reorder_columns', { columnIds: colIds })
    await loadBoard()
    vibrate(12)
  } catch (error) {
    console.error('Reorder columns failed:', error)
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

function findTaskById(taskId: string) {
  const location = findTaskLocation(taskId)
  return location?.column.tasks.find((task) => task.id === taskId) ?? null
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
  return invoke('move_task', { id: task.id, targetColumnId: columnId, targetPosition: position })
    .then(() => loadBoard())
    .catch((err: unknown) => console.error('Move failed:', err))
}

function createColumnByDrag(task: Task, project: string) {
  return invoke<[TaskColumn, Task]>('create_column_by_drag', { taskId: task.id, project })
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
    .catch((err: any) => console.error('Create column by drag failed:', err))
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
  const drag = pointerTaskDrag.value
  window.removeEventListener('pointermove', onTaskPointerMove)
  window.removeEventListener('pointerup', onTaskPointerUp)
  window.removeEventListener('pointercancel', onTaskPointerCancel)
  try { drag?.element.releasePointerCapture(drag.pointerId) } catch {}
  document.body.style.userSelect = ''
  document.body.style.touchAction = ''
  clearTextSelection()
  pointerTaskDrag.value = null
  stopDragPreview()
}

function onTaskPointerDown(e: PointerEvent, task: Task) {
  if (isSearching.value || e.button !== 0 || editingTaskId.value === task.id) return
  const target = e.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return

  document.body.style.userSelect = 'none'
  document.body.style.touchAction = 'none'
  clearTextSelection()
  const element = e.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  try { element.setPointerCapture(e.pointerId) } catch {}
  pointerTaskDrag.value = {
    task,
    pointerId: e.pointerId,
    element,
    width: rect.width,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onTaskPointerMove, nonPassivePointerOptions)
  window.addEventListener('pointerup', onTaskPointerUp)
  window.addEventListener('pointercancel', onTaskPointerCancel)
}

function onTaskPointerMove(e: PointerEvent) {
  const drag = pointerTaskDrag.value
  if (!drag || drag.pointerId !== e.pointerId) return

  const distance = Math.hypot(e.clientX - drag.startX, e.clientY - drag.startY)
  if (!drag.active) {
    if (distance < (isMobilePlatform.value ? 12 : 6)) return
    drag.active = true
    draggedTask.value = drag.task
    draggedColumn.value = null
    columnMenuId.value = null
    startDragPreview('task', drag.task.title, e, drag.width)
    vibrate(12)
  }

  e.preventDefault()
  clearTextSelection()
  moveDragPreview(e)
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
    suppressTaskClickUntil = Date.now() + 350
    e.preventDefault()
  }

  stopPointerTaskDrag()
  onTaskDragEnd()

  if (!wasActive) return
  if (targetProject || (targetColumnId && targetPosition >= 0)) vibrate(18)
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
  const drag = pointerGroupDrag.value
  window.removeEventListener('pointermove', onGroupPointerMove)
  window.removeEventListener('pointerup', onGroupPointerUp)
  window.removeEventListener('pointercancel', onGroupPointerCancel)
  try { drag?.element.releasePointerCapture(drag.pointerId) } catch {}
  document.body.style.userSelect = ''
  document.body.style.touchAction = ''
  clearTextSelection()
  pointerGroupDrag.value = null
  draggedGroupProject.value = null
  clearGroupDropTarget()
  stopDragPreview()
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
  document.body.style.touchAction = 'none'
  clearTextSelection()
  const element = e.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  try { element.setPointerCapture(e.pointerId) } catch {}
  pointerGroupDrag.value = {
    project,
    pointerId: e.pointerId,
    element,
    width: rect.width,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onGroupPointerMove, nonPassivePointerOptions)
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
    startDragPreview('group', displayGroupName(drag.project), e, drag.width, t('tasks.project'))
    vibrate(12)
  }

  e.preventDefault()
  clearTextSelection()
  moveDragPreview(e)
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
  vibrate(18)

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
  const drag = pointerColumnDrag.value
  window.removeEventListener('pointermove', onColumnPointerMove)
  window.removeEventListener('pointerup', onColumnPointerUp)
  window.removeEventListener('pointercancel', onColumnPointerCancel)
  try { drag?.element.releasePointerCapture(drag.pointerId) } catch {}
  document.body.style.userSelect = ''
  document.body.style.touchAction = ''
  clearTextSelection()
  pointerColumnDrag.value = null
  draggedColumn.value = null
  clearColumnReorderDropTarget()
  stopDragPreview()
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
  document.body.style.touchAction = 'none'
  clearTextSelection()
  const element = e.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  try { element.setPointerCapture(e.pointerId) } catch {}
  pointerColumnDrag.value = {
    column: col,
    pointerId: e.pointerId,
    element,
    width: rect.width,
    startX: e.clientX,
    startY: e.clientY,
    active: false,
  }
  window.addEventListener('pointermove', onColumnPointerMove, nonPassivePointerOptions)
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
    startDragPreview('column', drag.column.name, e, drag.width, displayGroupName(drag.column.project))
    vibrate(12)
  }

  e.preventDefault()
  clearTextSelection()
  moveDragPreview(e)
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
  vibrate(18)

  const group = groups.value.find((g) => g.project === dragged.project)
  if (!group) return

  const colIds = group.columns.map((c) => c.column.id)
  const dragIdx = colIds.indexOf(dragged.id)
  const dropIdx = colIds.indexOf(targetColumnId)
  if (dragIdx === -1 || dropIdx === -1 || dragIdx === dropIdx) return

  colIds.splice(dragIdx, 1)
  colIds.splice(dropIdx, 0, dragged.id)

  try {
    await invoke('reorder_columns', { columnIds: colIds })
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

// Click outside handler for column menu
function onClickOutside(e: MouseEvent) {
  if (columnMenuId.value) {
    const target = e.target as HTMLElement
    if (!target.closest('[data-col-menu]')) {
      closeColumnMenu()
    }
  }
}

onMounted(() => {
  loadBoard()
  loadCollapsedState()
  document.addEventListener('click', onClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', onClickOutside)
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
    <div class="flex-1 overflow-y-auto pb-6" :class="isCompactWidth ? 'px-3' : 'px-6'">
      <!-- No results -->
      <div v-if="isSearching && filteredGroups.length === 0" class="text-center py-12 text-muted-foreground text-sm">
        {{ t('tasks.searchNoResults') }}
      </div>

      <!-- Phone layout: groups stay vertical, columns become status filters. -->
      <template v-if="isPhoneTaskLayout">
      <div
          v-for="group in (isSearching ? filteredGroups : groups)"
          :key="group.project"
          class="mb-5 rounded-2xl border bg-card/60 p-3 transition-colors"
          :class="dropTargetTaskGroupProject === group.project ? 'ring-2 ring-primary/30' : ''"
          :data-task-group-target="group.project"
        >
        <div
          class="flex w-full items-center gap-2 py-1.5"
          :data-task-group="group.project"
          @pointerdown="onGroupPointerDown($event, group.project)"
        >
          <button class="mobile-icon-button h-9 w-9 border-0 bg-transparent" @click="onGroupHeaderClick(group.project)">
            <ChevronDown v-if="isGroupExpanded(group.project)" class="w-4 h-4 text-muted-foreground transition-transform" />
            <ChevronRight v-else class="w-4 h-4 text-muted-foreground transition-transform" />
          </button>
          <button class="min-w-0 flex-1 truncate py-2 text-left text-sm font-semibold" @click="onGroupHeaderClick(group.project)">
            {{ displayGroupName(group.project) }}
          </button>
          <span class="text-xs text-muted-foreground">{{ group.columns.reduce((sum, c) => sum + c.tasks.length, 0) }}</span>
          <button
            v-if="!isSearching"
            class="mobile-icon-button h-9 w-9"
            :title="t('tasks.addColumn')"
            @pointerdown.stop
            @click.stop="startAddColumn(group.project)"
          >
            <Plus class="h-4 w-4" />
          </button>
          <button
            v-if="group.project !== 'default' && !isSearching"
            class="mobile-icon-button h-9 w-9"
            :title="t('tasks.renameGroup')"
            @pointerdown.stop
            @click.stop="startEditGroup(group.project)"
          >
            <Pencil class="h-4 w-4" />
          </button>
          <button
            v-if="!isSearching"
            class="mobile-icon-button h-9 w-9"
            :title="t('tasks.copyGroup')"
            @pointerdown.stop
            @click.stop="copyGroup(group.project)"
          >
            <Copy class="h-4 w-4" />
          </button>
          <button
            v-if="group.project !== 'default' && !isSearching"
            class="mobile-icon-button h-9 w-9 text-muted-foreground hover:text-red-400"
            :title="t('tasks.deleteGroup')"
            @pointerdown.stop
            @click.stop="deleteGroup(group.project)"
          >
            <Trash2 class="h-4 w-4" />
          </button>
        </div>

        <template v-if="isGroupExpanded(group.project)">
          <div class="mt-3 grid grid-cols-2 gap-2">
            <button
              v-for="cwt in group.columns"
              :key="cwt.column.id"
              class="flex min-w-0 items-center justify-between gap-2 rounded-xl border px-3 py-2 text-left text-sm transition-colors"
              :class="mobileActiveColumn(group)?.column.id === cwt.column.id ? 'border-primary bg-accent text-accent-foreground' : 'bg-background text-muted-foreground'"
              @click="setMobileActiveColumn(group.project, cwt.column.id)"
            >
              <span class="truncate font-medium">{{ cwt.column.name }}</span>
              <span class="shrink-0 text-xs tabular-nums">{{ cwt.tasks.length }}</span>
            </button>
          </div>

          <div
            v-if="mobileActiveColumn(group)"
            class="mt-3 space-y-2"
            :data-column-task-list="mobileActiveColumn(group)?.column.id"
          >
            <div
              class="flex items-center gap-2 rounded-xl border bg-background/80 px-3 py-2"
              :data-task-column="mobileActiveColumn(group)?.column.id"
            >
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-medium">{{ mobileActiveColumn(group)?.column.name }}</div>
                <div class="text-xs text-muted-foreground">{{ t('tasks.column') }}</div>
              </div>
              <button
                class="mobile-icon-button h-9 w-9"
                :disabled="group.columns.findIndex((c) => c.column.id === mobileActiveColumn(group)?.column.id) === 0"
                :title="t('tasks.moveColumnLeft')"
                @pointerdown.stop
                @click.stop="moveMobileColumn(group, -1)"
              >
                <ArrowLeft class="h-4 w-4" />
              </button>
              <button
                class="mobile-icon-button h-9 w-9"
                :disabled="group.columns.findIndex((c) => c.column.id === mobileActiveColumn(group)?.column.id) === group.columns.length - 1"
                :title="t('tasks.moveColumnRight')"
                @pointerdown.stop
                @click.stop="moveMobileColumn(group, 1)"
              >
                <ArrowRight class="h-4 w-4" />
              </button>
              <button
                class="mobile-icon-button h-9 w-9"
                :title="t('tasks.renameColumn')"
                @pointerdown.stop
                @click.stop="startEditColumn(mobileActiveColumn(group)!.column)"
              >
                <Pencil class="h-4 w-4" />
              </button>
              <button
                class="mobile-icon-button h-9 w-9 text-muted-foreground hover:text-red-400"
                :title="t('tasks.deleteColumn')"
                @pointerdown.stop
                @click.stop="deleteColumn(mobileActiveColumn(group)!.column)"
              >
                <Trash2 class="h-4 w-4" />
              </button>
            </div>
            <template v-for="(task, idx) in mobileActiveColumn(group)?.tasks ?? []" :key="task.id">
              <div
                v-if="dropTargetColumnId === mobileActiveColumn(group)?.column.id && dropTargetPosition === idx && draggedTask && draggedTask.id !== task.id"
                class="h-1 rounded-full bg-primary"
              />
              <div
                class="relative min-h-[60px] touch-none select-none rounded-xl border bg-background p-3 pr-20 transition-all duration-150 active:border-primary/45 active:bg-accent/20"
                :class="draggedTask?.id === task.id ? 'opacity-40' : ''"
                :data-task-card="task.id"
                @pointerdown="onTaskPointerDown($event, task)"
                @click="startEditTask(task)"
              >
                <div class="flex items-start gap-2.5">
                  <GripHorizontal class="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground/50" />
                  <p class="min-w-0 flex-1 text-sm leading-5" :class="{ 'line-through text-muted-foreground': task.done }">{{ task.title }}</p>
                </div>
                <button
                  class="absolute right-11 top-3 rounded p-1"
                  :class="{ 'text-primary': task.done }"
                  :title="task.done ? t('tasks.reopenTask') : t('tasks.completeTask')"
                  @pointerdown.stop
                  @click.stop="toggleTaskDone(task)"
                >
                  <CheckCircle2 v-if="task.done" class="w-4 h-4" />
                  <Circle v-else class="w-4 h-4" />
                </button>
                <button
                  class="absolute right-3 top-3 rounded p-1 text-muted-foreground hover:text-red-400"
                  @pointerdown.stop
                  @click.stop="deleteTask(task)"
                >
                  <Trash2 class="w-4 h-4" />
                </button>
              </div>
            </template>
            <div
              v-if="dropTargetColumnId === mobileActiveColumn(group)?.column.id && dropTargetPosition === (mobileActiveColumn(group)?.tasks.length ?? 0) && draggedTask"
              class="h-1 rounded-full bg-primary"
            />
            <div v-if="draggedTask" class="h-5" :data-column-drop-tail="mobileActiveColumn(group)?.column.id" />
            <div class="border-t pt-2">
              <template v-if="addingColumnId === mobileActiveColumn(group)?.column.id">
                <input
                  v-model="newTaskTitle"
                  :data-col-add-input="mobileActiveColumn(group)?.column.id"
                  class="w-full rounded-xl border bg-background px-3 py-3 text-sm outline-none focus:border-primary"
                  :placeholder="t('tasks.placeholder')"
                  :disabled="savingNewTask"
                  @keydown="onAddTaskKeydown($event, mobileActiveColumn(group)!.column.id)"
                  @blur="onAddTaskBlur(mobileActiveColumn(group)!.column.id)"
                />
              </template>
              <button
                v-else-if="!isSearching && mobileActiveColumn(group)"
                class="flex w-full items-center gap-1.5 rounded-xl px-3 py-3 text-sm text-muted-foreground transition-colors hover:bg-secondary/50"
                @click="startAddTaskToColumn(mobileActiveColumn(group)!.column.id)"
              >
                <Plus class="w-4 h-4" />
                <span>{{ t('tasks.addTask') }}</span>
              </button>
            </div>
          </div>
        </template>
        </div>
      </template>

      <!-- Groups -->
      <template v-else>
      <div
        v-for="group in (isSearching ? filteredGroups : groups)"
        :key="group.project"
        class="mb-6"
        :data-task-group-target="group.project"
        :class="{ 'ring-2 ring-primary/30 rounded-md': dropTargetTaskGroupProject === group.project }"
      >
        <!-- Group header -->
        <div
          class="flex items-center gap-2 py-2 cursor-pointer select-none group touch-none"
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
            ({{ group.columns.reduce((sum, c) => sum + c.tasks.length, 0) }})
          </span>
          <div class="ml-auto flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
            <button
              class="p-1 rounded transition-colors hover:bg-secondary"
              :title="t('tasks.copyGroup')"
              @click.stop="copyGroup(group.project)"
            >
              <Copy class="w-3.5 h-3.5 text-muted-foreground" />
            </button>
            <button
              v-if="group.project !== 'default'"
              class="p-1 rounded transition-colors hover:bg-secondary hover:text-red-400"
              :title="t('tasks.deleteGroup')"
              @click.stop="deleteGroup(group.project)"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        <!-- Columns (kanban) -->
        <div
          v-if="isGroupExpanded(group.project)"
          class="relative flex gap-3 overflow-x-auto pb-3"
          :class="isCompactWidth ? 'snap-x snap-mandatory scroll-px-3' : ''"
        >
          <div
            v-for="cwt in group.columns"
            :key="cwt.column.id"
            :data-task-column="cwt.column.id"
            class="flex-shrink-0 bg-card border rounded-lg flex flex-col min-h-[300px] max-h-[calc(100vh-280px)] transition-colors"
            :style="columnStyle(group.columns.length)"
            :class="[
              isCompactWidth ? 'snap-start rounded-xl' : '',
              {
                'opacity-50': (draggedTask && draggedTask.column_id === cwt.column.id) || draggedColumn?.id === cwt.column.id,
                'ring-2 ring-primary/30': (dropTargetColumnId === cwt.column.id && draggedTask) || dropTargetColumnReorderId === cwt.column.id,
              },
            ]"
          >
            <!-- Column header -->
            <div
              class="flex items-center gap-2 border-b cursor-grab active:cursor-grabbing select-none touch-none"
              :class="isCompactWidth ? 'px-4 py-3' : 'px-3 py-2'"
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
              <span class="text-xs text-muted-foreground flex-shrink-0">{{ cwt.tasks.length }}</span>
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
              class="flex-1 overflow-y-auto min-h-[40px]"
              :class="isCompactWidth ? 'space-y-3 p-3' : 'space-y-2 p-2'"
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
                  class="relative bg-background border group/card transition-opacity cursor-grab active:cursor-grabbing active:border-primary/45 active:bg-accent/20"
                  :class="[
                    isCompactWidth ? 'rounded-xl p-4 min-h-[64px]' : 'rounded-lg p-3',
                    isMobilePlatform ? 'touch-none select-none' : 'touch-pan-y',
                    draggedTask?.id === task.id ? 'opacity-40' : ''
                  ]"
                  :data-task-card="task.id"
                  @pointerdown="onTaskPointerDown($event, task)"
                  @click="startEditTask(task)"
                >
                  <!-- Drag handle + Title row -->
                  <div class="flex items-start gap-2.5">
                    <div
                      class="pt-0.5 flex-shrink-0 text-muted-foreground/50"
                      :data-task-drag-handle="task.id"
                    >
                      <GripHorizontal class="w-4 h-4 group-hover/card:text-muted-foreground/70" />
                    </div>
                    <!-- Task title -->
                    <template v-if="!isMobilePlatform && editingTaskId === task.id">
                      <input
                        v-model="editTaskTitle"
                        :data-edit-input="task.id"
                        class="flex-1 text-sm bg-transparent border-b border-primary outline-none"
                        @keydown="onEditTaskKeydown"
                        @blur="saveEditTask"
                      />
                    </template>
                    <template v-else>
                      <p
                        class="flex-1 cursor-text text-sm leading-5"
                        :class="{ 'line-through text-muted-foreground': task.done }"
                      >{{ task.title }}</p>
                    </template>
                  </div>

                  <!-- Complete + delete buttons -->
                  <button
                    class="absolute rounded transition-colors hover:text-primary"
                    :class="[
                      isCompactWidth ? 'right-11 top-3 p-1 opacity-100' : 'top-2 right-8 opacity-0 group-hover/card:opacity-100 p-1',
                      { 'opacity-100 text-primary': task.done },
                    ]"
                    :title="task.done ? t('tasks.reopenTask') : t('tasks.completeTask')"
                    @pointerdown.stop
                    @click.stop="toggleTaskDone(task)"
                  >
                    <CheckCircle2 v-if="task.done" class="w-3.5 h-3.5" />
                    <Circle v-else class="w-3.5 h-3.5" />
                  </button>
                  <button
                    class="absolute rounded transition-colors hover:text-red-400"
                    :class="isCompactWidth ? 'right-3 top-3 p-1 opacity-100' : 'top-2 right-2 opacity-0 group-hover/card:opacity-100 p-1'"
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
                  class="w-full flex items-center gap-1.5 text-sm text-muted-foreground/50 hover:text-muted-foreground transition-colors rounded hover:bg-secondary/50"
                  :class="isCompactWidth ? 'px-2 py-2' : 'px-1 py-1'"
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
            v-if="!isSearching && draggedTask && !isMobilePlatform"
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
      </template>
    </div>

    <!-- Toast -->
    <Transition name="toast">
      <div
        v-if="toastVisible"
        class="fixed left-1/2 -translate-x-1/2 z-50 flex max-w-[calc(100vw-32px)] items-center gap-3 bg-card border rounded-lg shadow-lg px-4 py-3"
        :class="isMobilePlatform ? 'bottom-[calc(var(--mobile-tabbar-height)+var(--mobile-safe-bottom)+16px)]' : 'bottom-6'"
      >
        <span class="text-sm">{{ toastMessage }}</span>
        <button
          v-if="toastUndoAction"
          class="text-sm font-medium text-primary hover:text-primary/80 transition-colors"
          @click="doUndo"
        >{{ t('tasks.undo') }}</button>
      </div>
    </Transition>
    <Teleport to="body">
      <Transition name="modal">
        <div
          v-if="mobileEditingTask"
          class="fixed inset-0 z-[90] flex items-end overflow-hidden bg-black/30 p-4 backdrop-blur-sm"
          @click.self="saveEditTask"
        >
          <div class="max-h-[calc(100dvh-32px)] w-full overflow-y-auto rounded-2xl border bg-card p-4 shadow-2xl">
            <h2 class="mb-3 text-sm font-semibold">{{ t('tasks.editTask') }}</h2>
            <input
              v-model="editTaskTitle"
              data-mobile-task-edit-input
              class="w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="t('tasks.placeholder')"
              @keydown="onEditTaskKeydown"
            />
            <label v-if="mobileEditingColumns.length" class="mt-3 block">
              <span class="mb-1 block text-xs text-muted-foreground">{{ t('tasks.column') }}</span>
              <select
                v-model="mobileEditColumnId"
                class="w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              >
                <option v-for="column in mobileEditingColumns" :key="column.column.id" :value="column.column.id">
                  {{ column.column.name }}
                </option>
              </select>
            </label>
            <div class="mt-4 flex justify-end gap-2">
              <button class="rounded-lg bg-secondary px-4 py-2 text-sm" @click="cancelEditTask">{{ t('common.cancel') }}</button>
              <button class="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground" @click="saveEditTask">{{ t('common.save') }}</button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
    <Teleport to="body">
      <Transition name="modal">
        <div
          v-if="isPhoneTaskLayout && editingGroupProject"
          class="fixed inset-0 z-[90] flex items-end overflow-hidden bg-black/30 p-4 backdrop-blur-sm"
          @click.self="saveEditGroup"
        >
          <div class="w-full rounded-2xl border bg-card p-4 shadow-2xl">
            <h2 class="mb-3 text-sm font-semibold">{{ t('tasks.renameGroup') }}</h2>
            <input
              v-model="editGroupName"
              data-mobile-group-edit-input
              class="w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="t('tasks.groupName')"
              :disabled="savingGroupRename"
              @keydown="onEditGroupKeydown"
            />
            <div class="mt-4 flex justify-end gap-2">
              <button class="rounded-lg bg-secondary px-4 py-2 text-sm" @click="cancelEditGroup">{{ t('common.cancel') }}</button>
              <button class="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground" @click="saveEditGroup">{{ t('common.save') }}</button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
    <Teleport to="body">
      <Transition name="modal">
        <div
          v-if="isPhoneTaskLayout && addingColumnProject"
          class="fixed inset-0 z-[90] flex items-end overflow-hidden bg-black/30 p-4 backdrop-blur-sm"
          @click.self="newColumnName.trim() ? saveNewColumn() : cancelAddColumn()"
        >
          <div class="w-full rounded-2xl border bg-card p-4 shadow-2xl">
            <h2 class="mb-3 text-sm font-semibold">{{ t('tasks.addColumn') }}</h2>
            <input
              v-model="newColumnName"
              data-column-add-input
              class="w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="t('tasks.columnName')"
              :disabled="savingNewColumn"
              @keydown="onAddColumnKeydown"
            />
            <div class="mt-4 flex justify-end gap-2">
              <button class="rounded-lg bg-secondary px-4 py-2 text-sm" @click="cancelAddColumn">{{ t('common.cancel') }}</button>
              <button class="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground" @click="saveNewColumn">{{ t('common.save') }}</button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
    <Teleport to="body">
      <Transition name="modal">
        <div
          v-if="isPhoneTaskLayout && editingColumnId"
          class="fixed inset-0 z-[90] flex items-end overflow-hidden bg-black/30 p-4 backdrop-blur-sm"
          @click.self="saveEditColumn"
        >
          <div class="w-full rounded-2xl border bg-card p-4 shadow-2xl">
            <h2 class="mb-3 text-sm font-semibold">{{ t('tasks.renameColumn') }}</h2>
            <input
              v-model="editColumnName"
              data-mobile-column-edit-input
              class="w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="t('tasks.columnName')"
              @keydown="onEditColumnKeydown"
            />
            <div class="mt-4 flex justify-end gap-2">
              <button class="rounded-lg bg-secondary px-4 py-2 text-sm" @click="cancelEditColumn">{{ t('common.cancel') }}</button>
              <button class="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground" @click="saveEditColumn">{{ t('common.save') }}</button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
    <Teleport to="body">
      <div
        v-if="dragPreview"
        class="pointer-events-none fixed left-0 top-0 z-[100] rounded-xl border border-primary/35 bg-card/95 px-3 py-2 text-sm shadow-2xl shadow-primary/20 backdrop-blur will-change-transform"
        :style="dragPreviewStyle"
      >
        <div class="flex items-center gap-2">
          <GripHorizontal class="h-4 w-4 shrink-0 text-primary" />
          <div class="min-w-0">
            <div class="truncate font-medium">{{ dragPreview.title }}</div>
            <div v-if="dragPreview.subtitle" class="truncate text-[11px] text-muted-foreground">{{ dragPreview.subtitle }}</div>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, 20px);
}
</style>
