<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  ArrowDown,
  ArrowUp,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  Columns3,
  GripHorizontal,
  Pencil,
  Plus,
  Search,
  Trash2,
  X,
} from 'lucide-vue-next'
import type { ColumnWithTasks, GroupData, Task, TaskColumn, TaskSnapshot } from '$lib/types'
import { useI18n } from '$lib/i18n'

const { t } = useI18n()

type SheetMode = 'task' | 'group' | 'column' | 'addGroup' | 'addColumn' | 'addTask'

interface SheetState {
  mode: SheetMode
  title: string
  value: string
  project?: string
  columnId?: string
  task?: Task
  column?: TaskColumn
  originalValue?: string
}

const groups = ref<GroupData[]>([])
const searchQuery = ref('')
const collapsedGroups = ref<Record<string, boolean>>({})
const sheet = ref<SheetState | null>(null)
const savingSheet = ref(false)
const toastMessage = ref('')
const toastVisible = ref(false)
const toastUndoAction = ref<(() => Promise<void>) | null>(null)
const dragging = ref<{
  task: Task
  pointerId: number
  element: HTMLElement
  startX: number
  startY: number
  x: number
  y: number
  width: number
  active: boolean
} | null>(null)
const dropTarget = ref<{ columnId: string; position: number } | null>(null)
let toastTimer: ReturnType<typeof setTimeout> | null = null
let suppressClickUntil = 0

const dragStyle = computed(() => {
  if (!dragging.value) return {}
  return {
    width: `${Math.min(Math.max(dragging.value.width, 220), 320)}px`,
    transform: `translate3d(${dragging.value.x + 12}px, ${dragging.value.y + 12}px, 0)`,
  }
})

const filteredGroups = computed(() => {
  const query = searchQuery.value.trim().toLowerCase()
  if (!query) return groups.value
  return groups.value
    .map((group) => ({
      ...group,
      columns: group.columns
        .map((column) => ({
          ...column,
          tasks: column.tasks.filter((task) => task.title.toLowerCase().includes(query)),
        }))
        .filter((column) => column.tasks.length > 0 || column.column.name.toLowerCase().includes(query)),
    }))
    .filter((group) => group.columns.length > 0 || displayGroupName(group.project).toLowerCase().includes(query))
})

function vibrate(pattern: number | number[]) {
  if (typeof navigator === 'undefined' || !navigator.vibrate) return
  try { navigator.vibrate(pattern) } catch {}
}

function displayGroupName(project: string) {
  return project === 'default' ? t('tasks.defaultGroup') : project
}

function groupTaskCount(group: GroupData) {
  return group.columns.reduce((sum, column) => sum + column.tasks.length, 0)
}

function isGroupExpanded(project: string) {
  return searchQuery.value.trim().length > 0 || !collapsedGroups.value[project]
}

function toggleGroup(project: string) {
  if (searchQuery.value.trim()) return
  collapsedGroups.value = { ...collapsedGroups.value, [project]: !collapsedGroups.value[project] }
  try { localStorage.setItem('nalu-mobile-task-groups', JSON.stringify(collapsedGroups.value)) } catch {}
}

function loadCollapsedGroups() {
  try {
    const saved = localStorage.getItem('nalu-mobile-task-groups')
    if (saved) collapsedGroups.value = JSON.parse(saved)
  } catch {}
}

async function loadBoard() {
  groups.value = await invoke<GroupData[]>('get_board')
}

function showToast(message: string, undoAction: (() => Promise<void>) | null = null) {
  if (toastTimer) clearTimeout(toastTimer)
  toastMessage.value = message
  toastUndoAction.value = undoAction
  toastVisible.value = true
  toastTimer = setTimeout(() => {
    toastVisible.value = false
    toastUndoAction.value = null
  }, 3200)
}

async function undoToast() {
  const action = toastUndoAction.value
  toastVisible.value = false
  toastUndoAction.value = null
  if (!action) return
  await action()
}

function openSheet(next: SheetState) {
  sheet.value = next
  vibrate(8)
  nextTick(() => {
    const input = document.querySelector('[data-mobile-task-sheet-input]') as HTMLInputElement | HTMLTextAreaElement | null
    input?.focus()
    input?.select()
  })
}

function closeSheet() {
  sheet.value = null
  savingSheet.value = false
}

function cancelSheet() {
  closeSheet()
}

async function saveSheet() {
  if (!sheet.value || savingSheet.value) return
  const current = sheet.value
  const value = current.value.trim()
  savingSheet.value = true

  try {
    if (current.mode === 'task' && current.task) {
      if (value && value !== current.originalValue) {
        await invoke('update_task_content', { id: current.task.id, title: value })
      }
    } else if (current.mode === 'group' && current.project) {
      if (value && value !== current.project) {
        await invoke('rename_task_group', { project: current.project, name: value })
      }
    } else if (current.mode === 'column' && current.column) {
      if (value && value !== current.column.name) {
        await invoke('rename_column', { id: current.column.id, name: value })
      }
    } else if (current.mode === 'addGroup') {
      if (value) {
        await invoke('create_task_group', { project: value })
      }
    } else if (current.mode === 'addColumn' && current.project) {
      await invoke('create_column', { project: current.project, name: value || t('tasks.newColumn') })
    } else if (current.mode === 'addTask' && current.columnId) {
      if (value) {
        await invoke('add_task_to_column', { columnId: current.columnId, title: value })
      }
    }
    closeSheet()
    await loadBoard()
    vibrate(12)
  } catch (error: any) {
    savingSheet.value = false
    const message = String(error)
    if (message.includes('GROUP_EXISTS')) showToast(t('tasks.groupAlreadyExists'))
    else if (message.includes('DEFAULT_GROUP')) showToast(t('tasks.defaultGroupCannotRename'))
    else console.error('[MobileTasks] save sheet failed:', error)
  }
}

function onSheetKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    void saveSheet()
  } else if (event.key === 'Escape') {
    cancelSheet()
  }
}

function editTask(task: Task) {
  if (Date.now() < suppressClickUntil) return
  openSheet({
    mode: 'task',
    title: t('tasks.editTask'),
    value: task.title,
    task,
    originalValue: task.title,
  })
}

function editGroup(project: string) {
  if (project === 'default') return
  openSheet({
    mode: 'group',
    title: t('tasks.renameGroup'),
    value: project,
    project,
    originalValue: project,
  })
}

function editColumn(column: TaskColumn) {
  openSheet({
    mode: 'column',
    title: t('tasks.renameColumn'),
    value: column.name,
    column,
    originalValue: column.name,
  })
}

function addGroup() {
  openSheet({ mode: 'addGroup', title: t('tasks.addGroup'), value: '' })
}

function addColumn(project: string) {
  openSheet({ mode: 'addColumn', title: t('tasks.addColumn'), value: '', project })
}

function addTask(columnId: string) {
  openSheet({ mode: 'addTask', title: t('tasks.addTask'), value: '', columnId })
}

async function toggleTaskDone(task: Task) {
  vibrate(10)
  await invoke('toggle_task', { id: task.id })
  await loadBoard()
}

async function deleteTask(task: Task) {
  vibrate(12)
  const snapshot = await invoke<TaskSnapshot>('delete_task_with_snapshot', { id: task.id })
  await loadBoard()
  showToast(t('tasks.deleteTaskUndo'), async () => {
    await invoke('restore_task', { snapshot })
    await loadBoard()
  })
}

async function deleteGroup(project: string) {
  if (project === 'default') return
  try {
    await invoke('delete_task_group', { project })
    await loadBoard()
    showToast(t('tasks.deleteGroupSuccess'))
    vibrate(12)
  } catch (error: any) {
    const message = String(error)
    if (message.includes('HAS_INCOMPLETE_TASKS')) showToast(t('tasks.groupHasIncompleteTasks'))
    else if (message.includes('DEFAULT_GROUP')) showToast(t('tasks.defaultGroupCannotDelete'))
    else console.error('[MobileTasks] delete group failed:', error)
  }
}

async function deleteColumn(column: ColumnWithTasks) {
  try {
    await invoke('delete_column', { id: column.column.id })
    await loadBoard()
    showToast(t('tasks.deleteColumnUndo'))
    vibrate(12)
  } catch (error: any) {
    const message = String(error)
    if (message.includes('NON_EMPTY')) showToast(t('tasks.nonEmptyColumn'))
    else if (message.includes('LAST_COLUMN')) showToast(t('tasks.lastColumn'))
    else console.error('[MobileTasks] delete column failed:', error)
  }
}

async function moveGroup(project: string, direction: -1 | 1) {
  const projects = groups.value.map((group) => group.project)
  const index = projects.indexOf(project)
  const nextIndex = index + direction
  if (index < 0 || nextIndex < 0 || nextIndex >= projects.length) return
  const [moved] = projects.splice(index, 1)
  projects.splice(nextIndex, 0, moved)
  await invoke('reorder_task_groups', { projects })
  await loadBoard()
  vibrate(10)
}

async function moveColumn(group: GroupData, columnId: string, direction: -1 | 1) {
  const ids = group.columns.map((column) => column.column.id)
  const index = ids.indexOf(columnId)
  const nextIndex = index + direction
  if (index < 0 || nextIndex < 0 || nextIndex >= ids.length) return
  const [moved] = ids.splice(index, 1)
  ids.splice(nextIndex, 0, moved)
  await invoke('reorder_columns', { columnIds: ids })
  await loadBoard()
  vibrate(10)
}

function findColumn(columnId: string) {
  for (const group of groups.value) {
    const column = group.columns.find((item) => item.column.id === columnId)
    if (column) return column
  }
  return null
}

function setDropTargetFromPoint(x: number, y: number) {
  const element = document.elementFromPoint(x, y) as HTMLElement | null
  if (!element || !dragging.value) {
    dropTarget.value = null
    return
  }

  const taskCard = element.closest<HTMLElement>('[data-mobile-task-card]')
  const taskId = taskCard?.dataset.mobileTaskCard
  if (taskCard && taskId && taskId !== dragging.value.task.id) {
    const columnId = taskCard.dataset.mobileTaskColumn
    const position = Number(taskCard.dataset.mobileTaskPosition ?? 0)
    const rect = taskCard.getBoundingClientRect()
    if (columnId) {
      dropTarget.value = { columnId, position: position + (y > rect.top + rect.height / 2 ? 1 : 0) }
      return
    }
  }

  const columnList = element.closest<HTMLElement>('[data-mobile-column-list]')
  const columnId = columnList?.dataset.mobileColumnList
  if (columnId) {
    const column = findColumn(columnId)
    dropTarget.value = { columnId, position: column?.tasks.length ?? 0 }
    return
  }

  dropTarget.value = null
}

function stopDragging() {
  const current = dragging.value
  window.removeEventListener('pointermove', onTaskPointerMove)
  window.removeEventListener('pointerup', onTaskPointerUp)
  window.removeEventListener('pointercancel', onTaskPointerCancel)
  try { current?.element.releasePointerCapture(current.pointerId) } catch {}
  document.body.style.userSelect = ''
  document.body.style.touchAction = ''
  dragging.value = null
}

function onTaskPointerDown(event: PointerEvent, task: Task) {
  if (searchQuery.value.trim() || event.button !== 0 || sheet.value) return
  const target = event.target as HTMLElement
  if (target.closest('button, input, textarea, select')) return

  const element = event.currentTarget as HTMLElement
  const rect = element.getBoundingClientRect()
  document.body.style.userSelect = 'none'
  document.body.style.touchAction = 'none'
  try { element.setPointerCapture(event.pointerId) } catch {}
  dragging.value = {
    task,
    pointerId: event.pointerId,
    element,
    startX: event.clientX,
    startY: event.clientY,
    x: event.clientX,
    y: event.clientY,
    width: rect.width,
    active: false,
  }
  window.addEventListener('pointermove', onTaskPointerMove, { passive: false })
  window.addEventListener('pointerup', onTaskPointerUp)
  window.addEventListener('pointercancel', onTaskPointerCancel)
}

function onTaskPointerMove(event: PointerEvent) {
  const current = dragging.value
  if (!current || current.pointerId !== event.pointerId) return
  const distance = Math.hypot(event.clientX - current.startX, event.clientY - current.startY)
  if (!current.active) {
    if (distance < 10) return
    current.active = true
    vibrate(12)
  }
  event.preventDefault()
  current.x = event.clientX
  current.y = event.clientY
  setDropTargetFromPoint(event.clientX, event.clientY)
}

async function onTaskPointerUp(event: PointerEvent) {
  const current = dragging.value
  if (!current || current.pointerId !== event.pointerId) return
  const wasActive = current.active
  const task = current.task
  const target = dropTarget.value
  if (wasActive) {
    suppressClickUntil = Date.now() + 350
    event.preventDefault()
  }
  stopDragging()
  dropTarget.value = null
  if (!wasActive || !target) return
  await invoke('move_task', {
    id: task.id,
    targetColumnId: target.columnId,
    targetPosition: target.position,
  })
  await loadBoard()
  vibrate(18)
}

function onTaskPointerCancel() {
  stopDragging()
  dropTarget.value = null
}

onMounted(async () => {
  loadCollapsedGroups()
  await loadBoard()
})

onBeforeUnmount(() => {
  if (toastTimer) clearTimeout(toastTimer)
  stopDragging()
})
</script>

<template>
  <div class="mobile-task-page min-h-full bg-background px-4 pb-4 pt-4">
    <header class="mb-4 flex items-center gap-3">
      <div class="min-w-0 flex-1">
        <h1 class="text-2xl font-bold leading-tight">{{ t('tasks.title') }}</h1>
        <p class="mt-1 text-xs text-muted-foreground">{{ groups.length }} {{ t('tasks.project') }} · {{ groups.reduce((sum, group) => sum + groupTaskCount(group), 0) }} {{ t('tasks.taskUnit') }}</p>
      </div>
      <button class="mobile-icon-button rounded-xl border bg-card text-primary active:bg-accent" :title="t('tasks.addGroup')" @click="addGroup">
        <Plus class="h-5 w-5" />
      </button>
    </header>

    <div class="sticky top-0 z-20 -mx-4 mb-4 bg-background/95 px-4 pb-3 backdrop-blur">
      <label class="flex h-11 items-center gap-2 rounded-xl border bg-card px-3">
        <Search class="h-4 w-4 shrink-0 text-muted-foreground" />
        <input v-model="searchQuery" class="min-h-0 flex-1 bg-transparent text-base outline-none" :placeholder="t('tasks.search')" />
        <button v-if="searchQuery" class="rounded-lg p-1 text-muted-foreground" @click="searchQuery = ''">
          <X class="h-4 w-4" />
        </button>
      </label>
      <p v-if="searchQuery.trim()" class="mt-2 text-xs text-muted-foreground">{{ t('tasks.searchDragDisabled') }}</p>
    </div>

    <TransitionGroup name="mobile-task-group" tag="div" class="space-y-4">
      <section
        v-for="(group, groupIndex) in filteredGroups"
        :key="group.project"
        class="overflow-hidden rounded-2xl border bg-card"
      >
        <div class="flex items-center gap-1 border-b px-3 py-2">
          <button class="mobile-icon-button h-10 w-10 rounded-xl" @click="toggleGroup(group.project)">
            <ChevronDown v-if="isGroupExpanded(group.project)" class="h-4 w-4 text-muted-foreground" />
            <ChevronRight v-else class="h-4 w-4 text-muted-foreground" />
          </button>
          <button class="min-w-0 flex-1 py-2 text-left" @click="toggleGroup(group.project)">
            <div class="truncate text-base font-semibold">{{ displayGroupName(group.project) }}</div>
            <div class="text-xs text-muted-foreground">{{ group.columns.length }} {{ t('tasks.columnUnit') }} · {{ groupTaskCount(group) }} {{ t('tasks.taskUnit') }}</div>
          </button>
          <button class="mobile-icon-button h-9 w-9 rounded-lg" :disabled="groupIndex === 0" :title="t('tasks.moveColumnLeft')" @click="moveGroup(group.project, -1)">
            <ArrowUp class="h-4 w-4" />
          </button>
          <button class="mobile-icon-button h-9 w-9 rounded-lg" :disabled="groupIndex === groups.length - 1" :title="t('tasks.moveColumnRight')" @click="moveGroup(group.project, 1)">
            <ArrowDown class="h-4 w-4" />
          </button>
          <button class="mobile-icon-button h-9 w-9 rounded-lg" :title="t('tasks.renameGroup')" :disabled="group.project === 'default'" @click="editGroup(group.project)">
            <Pencil class="h-4 w-4" />
          </button>
          <button class="mobile-icon-button h-9 w-9 rounded-lg text-muted-foreground hover:text-red-400" :disabled="group.project === 'default'" :title="t('tasks.deleteGroup')" @click="deleteGroup(group.project)">
            <Trash2 class="h-4 w-4" />
          </button>
        </div>

        <div v-if="isGroupExpanded(group.project)" class="space-y-3 p-3">
          <button class="flex w-full items-center justify-center gap-2 rounded-xl border border-dashed bg-background/50 px-3 py-3 text-sm text-muted-foreground active:bg-accent" @click="addColumn(group.project)">
            <Columns3 class="h-4 w-4" />
            {{ t('tasks.addColumn') }}
          </button>

          <section
            v-for="(column, columnIndex) in group.columns"
            :key="column.column.id"
            class="rounded-xl border bg-background"
            :data-mobile-column-list="column.column.id"
          >
            <div class="flex items-center gap-1 border-b px-3 py-2">
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-semibold">{{ column.column.name }}</div>
                <div class="text-xs text-muted-foreground">{{ column.tasks.length }} {{ t('tasks.taskUnit') }}</div>
              </div>
              <button class="mobile-icon-button h-9 w-9 rounded-lg" :disabled="columnIndex === 0" @click="moveColumn(group, column.column.id, -1)">
                <ArrowUp class="h-4 w-4" />
              </button>
              <button class="mobile-icon-button h-9 w-9 rounded-lg" :disabled="columnIndex === group.columns.length - 1" @click="moveColumn(group, column.column.id, 1)">
                <ArrowDown class="h-4 w-4" />
              </button>
              <button class="mobile-icon-button h-9 w-9 rounded-lg" @click="editColumn(column.column)">
                <Pencil class="h-4 w-4" />
              </button>
              <button class="mobile-icon-button h-9 w-9 rounded-lg text-muted-foreground hover:text-red-400" @click="deleteColumn(column)">
                <Trash2 class="h-4 w-4" />
              </button>
            </div>

            <div class="space-y-2 p-2" :data-mobile-column-list="column.column.id">
              <template v-for="(task, taskIndex) in column.tasks" :key="task.id">
                <div
                  v-if="dropTarget?.columnId === column.column.id && dropTarget.position === taskIndex && dragging?.task.id !== task.id"
                  class="h-1 rounded-full bg-primary"
                />
                <article
                  class="mobile-task-card touch-none select-none rounded-xl border bg-card px-3 py-2.5 transition-all duration-150 active:border-primary/50 active:bg-accent/30"
                  :class="{ 'opacity-35': dragging?.task.id === task.id }"
                  :data-mobile-task-card="task.id"
                  :data-mobile-task-column="column.column.id"
                  :data-mobile-task-position="taskIndex"
                  @pointerdown="onTaskPointerDown($event, task)"
                  @click="editTask(task)"
                >
                  <div class="grid grid-cols-[24px_minmax(0,1fr)_40px_40px] items-center gap-2">
                    <GripHorizontal class="h-4 w-4 justify-self-center text-muted-foreground/60" />
                    <p class="min-w-0 text-sm leading-5" :class="{ 'line-through text-muted-foreground': task.done }">{{ task.title }}</p>
                    <button class="mobile-icon-button h-9 w-9 rounded-lg" :class="{ 'text-primary': task.done }" @pointerdown.stop @click.stop="toggleTaskDone(task)">
                      <CheckCircle2 v-if="task.done" class="h-4 w-4" />
                      <Circle v-else class="h-4 w-4" />
                    </button>
                    <button class="mobile-icon-button h-9 w-9 rounded-lg text-muted-foreground hover:text-red-400" @pointerdown.stop @click.stop="deleteTask(task)">
                      <Trash2 class="h-4 w-4" />
                    </button>
                  </div>
                </article>
              </template>
              <div
                v-if="dropTarget?.columnId === column.column.id && dropTarget.position === column.tasks.length && dragging"
                class="h-1 rounded-full bg-primary"
              />
              <button class="flex w-full items-center gap-2 rounded-xl px-3 py-3 text-sm text-muted-foreground active:bg-secondary" @click="addTask(column.column.id)">
                <Plus class="h-4 w-4" />
                {{ t('tasks.addTask') }}
              </button>
            </div>
          </section>
        </div>
      </section>
    </TransitionGroup>

    <div v-if="filteredGroups.length === 0" class="py-12 text-center text-sm text-muted-foreground">
      {{ searchQuery ? t('tasks.searchNoResults') : t('tasks.noTasks') }}
    </div>

    <Transition name="toast">
      <div v-if="toastVisible" class="fixed left-4 right-4 z-[70] flex items-center gap-3 rounded-xl border bg-card px-4 py-3 shadow-xl" style="bottom: calc(var(--mobile-tabbar-height) + var(--mobile-safe-bottom) + 16px)">
        <span class="min-w-0 flex-1 text-sm">{{ toastMessage }}</span>
        <button v-if="toastUndoAction" class="text-sm font-medium text-primary" @click="undoToast">{{ t('tasks.undo') }}</button>
      </div>
    </Transition>

    <Teleport to="body">
      <Transition name="modal">
        <div v-if="sheet" class="fixed inset-0 z-[90] flex items-end overflow-hidden bg-black/35 p-4 backdrop-blur-sm" @click.self="saveSheet">
          <form class="modal-card-inner w-full rounded-2xl border bg-card p-4 shadow-2xl" @submit.prevent="saveSheet">
            <h2 class="text-base font-semibold">{{ sheet.title }}</h2>
            <textarea
              v-if="sheet.mode === 'task'"
              v-model="sheet.value"
              data-mobile-task-sheet-input
              rows="3"
              class="mt-3 w-full resize-none rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="t('tasks.placeholder')"
              @keydown="onSheetKeydown"
            />
            <input
              v-else
              v-model="sheet.value"
              data-mobile-task-sheet-input
              class="mt-3 w-full rounded-xl border bg-background px-3 py-3 text-base outline-none focus:border-primary"
              :placeholder="sheet.mode === 'addGroup' || sheet.mode === 'group' ? t('tasks.groupName') : t('tasks.columnName')"
              @keydown="onSheetKeydown"
            />
            <div class="mt-4 flex justify-end gap-2">
              <button type="button" class="rounded-xl bg-secondary px-4 py-2 text-sm" @click="cancelSheet">{{ t('common.cancel') }}</button>
              <button type="submit" class="rounded-xl bg-primary px-4 py-2 text-sm text-primary-foreground disabled:opacity-60" :disabled="savingSheet">{{ t('common.save') }}</button>
            </div>
          </form>
        </div>
      </Transition>
    </Teleport>

    <Teleport to="body">
      <div v-if="dragging?.active" class="pointer-events-none fixed left-0 top-0 z-[100] rounded-xl border border-primary/40 bg-card/95 px-3 py-2 shadow-2xl backdrop-blur" :style="dragStyle">
        <div class="flex items-center gap-2">
          <GripHorizontal class="h-4 w-4 shrink-0 text-primary" />
          <span class="min-w-0 truncate text-sm font-medium">{{ dragging.task.title }}</span>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.mobile-task-group-enter-active,
.mobile-task-group-leave-active,
.mobile-task-group-move {
  transition: opacity 180ms ease, transform 180ms ease;
}

.mobile-task-group-enter-from,
.mobile-task-group-leave-to {
  opacity: 0;
  transform: translateY(8px);
}

.mobile-task-card {
  min-height: 58px;
}
</style>
