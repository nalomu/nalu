<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import {
  MessageCircle,
  Send,
  LoaderCircle,
  Bot,
  User,
  ChevronDown,
  ChevronUp,
  Brain,
  Wrench
} from 'lucide-vue-next'
import { useSettingsStore } from '$lib/stores/settingsStore'
import { useI18n } from '$lib/i18n'

type ContextKey = 'tasks' | 'calendarTasks' | 'notes' | 'schedules' | 'alarms';
type AiContext = Record<ContextKey, ContextItem[]>;
type AiActionCommand = keyof typeof ACTION_LABELS;

interface ActionExecution {
  command: AiActionCommand | string;
  params: Record<string, unknown>;
  success: boolean;
  result?: string;
}

interface DisplayMessage {
  role: 'user' | 'assistant';
  content: string;
  actions?: ActionExecution[];
  reasoning?: string;
}

interface ContextItem {
  id: string;

  [key: string]: unknown;
}

const ACTION_LABELS = {
  add_task: 'Task created',
  add_task_to_column: 'Task created',
  add_task_to_group: 'Task created',
  toggle_task: 'Task toggled',
  update_task_content: 'Task updated',
  update_task_progress: 'Progress updated',
  delete_task: 'Task deleted',
  create_calendar_task: 'Calendar task created',
  update_calendar_task: 'Calendar task updated',
  remove_task_from_schedule: 'Task removed from schedule',
  cancel_task_recurrence: 'Recurrence cancelled',
  delete_recurring_tasks: 'Recurring tasks deleted',
  create_task_group: 'Group created',
  delete_task_group: 'Group deleted',
  complete_task_group: 'Group completed',
  copy_task_group: 'Group copied',
  rename_task_group: 'Group renamed',
  rename_column: 'Column renamed',
  delete_column: 'Column deleted',
  add_note: 'Note created',
  update_note: 'Note updated',
  delete_note: 'Note deleted',
  add_schedule: 'Schedule created',
  toggle_schedule: 'Schedule toggled',
  delete_schedule: 'Schedule deleted',
  add_alarm: 'Alarm created',
  delete_alarm: 'Alarm deleted',
  add_clipboard_entry: 'Saved to clipboard'
} as const

const CONTEXT_LOADERS: Array<{
  key: ContextKey;
  command: string;
  params?: Record<string, unknown>;
}> = [
  { key: 'tasks', command: 'get_tasks' },
  { key: 'calendarTasks', command: 'get_calendar_tasks' },
  { key: 'notes', command: 'get_notes' },
  { key: 'schedules', command: 'get_schedules' },
  { key: 'alarms', command: 'get_alarms' }
]

const CONTEXT_LIMITS: Record<ContextKey, number> = {
  tasks: 20,
  calendarTasks: 20,
  notes: 12,
  schedules: 20,
  alarms: 20
}

const ACTION_PATTERN = new RegExp(String.raw`\[ACTION]\s*({[\s\S]*?})\s*\[/ACTION]`, 'g')
const MAX_ACTIONS_PER_RESPONSE = 20
const MAX_PROMPT_STRING_LENGTH = 500
const MAX_ACTION_RESULT_LENGTH = 600

const { t } = useI18n()
const settings = useSettingsStore()
const { aiConfig } = storeToRefs(settings)

const messages = ref<DisplayMessage[]>([])
const inputText = ref('')
const collapsed = ref(false)
const loading = ref(false)
const messagesContainer = ref<HTMLDivElement>()
const textareaRef = ref<HTMLTextAreaElement>()
const context = ref<AiContext>({
  tasks: [],
  calendarTasks: [],
  notes: [],
  schedules: [],
  alarms: []
})

const configError = computed(() => !aiConfig.value.api_key?.trim())

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isAiActionCommand(command: unknown): command is AiActionCommand {
  return typeof command === 'string' && command in ACTION_LABELS
}

function truncateText(value: string, maxLength: number) {
  return value.length > maxLength ? `${value.slice(0, maxLength)}...` : value
}

function stringifyResult(value: unknown, maxLength = MAX_ACTION_RESULT_LENGTH) {
  const text = typeof value === 'string' ? value : JSON.stringify(value ?? 'OK')
  return truncateText(text, maxLength)
}

function sanitizeForPrompt(value: unknown, depth = 0): unknown {
  if (typeof value === 'string') return truncateText(value, MAX_PROMPT_STRING_LENGTH)
  if (typeof value === 'number' || typeof value === 'boolean' || value === null) return value
  if (Array.isArray(value)) {
    if (depth >= 2) return `[${value.length} items]`
    return value.slice(0, 8).map((item) => sanitizeForPrompt(item, depth + 1))
  }
  if (isPlainRecord(value)) {
    if (depth >= 3) return '[object]'
    return Object.fromEntries(
      Object.entries(value)
        .slice(0, 20)
        .map(([key, item]) => [key, sanitizeForPrompt(item, depth + 1)])
    )
  }
  return String(value)
}

function buildSafeContext() {
  const safeContext = {} as Record<ContextKey, unknown[]>

  for (const key of Object.keys(CONTEXT_LIMITS) as ContextKey[]) {
    safeContext[key] = context.value[key]
      .slice(0, CONTEXT_LIMITS[key])
      .map((item) => sanitizeForPrompt(item))
  }

  return JSON.stringify(safeContext, null, 2)
}

async function loadContext() {
  const entries = await Promise.all(
    CONTEXT_LOADERS.map(async ({ key, command, params }) => {
      try {
        const result = await invoke<unknown>(command, params)
        return [key, Array.isArray(result) ? (result as ContextItem[]) : []] as [ContextKey, ContextItem[]]
      } catch {
        return [key, [] as ContextItem[]] as [ContextKey, ContextItem[]]
      }
    })
  )

  const nextContext: AiContext = { ...context.value }

  for (const [key, value] of entries) {
    nextContext[key] = value
  }

  context.value = nextContext
}

function stripActionBlocks(content: string) {
  return content.replace(ACTION_PATTERN, '').trim()
}

async function parseActions(content: string) {
  const results: ActionExecution[] = []
  const matches = [...content.matchAll(ACTION_PATTERN)]

  for (const match of matches.slice(0, MAX_ACTIONS_PER_RESPONSE)) {
    let command: AiActionCommand | string = 'unknown'
    let params: Record<string, unknown> = {}

    try {
      const parsed = JSON.parse(match[1])
      if (!isPlainRecord(parsed)) {
        results.push({ command, params, success: false, result: 'Invalid action payload' })
        continue
      }

      command = String(parsed.command ?? 'unknown')
      if (!isAiActionCommand(parsed.command)) {
        results.push({ command, params, success: false, result: 'Command is not allowed' })
        continue
      }

      if (parsed.params !== undefined && !isPlainRecord(parsed.params)) {
        results.push({ command, params, success: false, result: 'Action params must be an object' })
        continue
      }

      command = parsed.command
      params = parsed.params ?? {}

      const result = await invoke<unknown>(command, params)
      results.push({ command, params, success: true, result: stringifyResult(result) })
    } catch (error) {
      results.push({ command, params, success: false, result: String(error) })
    }
  }

  if (matches.length > MAX_ACTIONS_PER_RESPONSE) {
    results.push({
      command: 'too_many_actions',
      params: {},
      success: false,
      result: `Only the first ${MAX_ACTIONS_PER_RESPONSE} actions were executed`
    })
  }

  return results
}

function actionLabel(command: string) {
  return ACTION_LABELS[command as AiActionCommand] || command
}

function formatParams(params: Record<string, unknown>) {
  const entries = Object.entries(params)
  if (entries.length === 0) return ''
  return entries
    .map(([key, value]) => {
      const text = typeof value === 'string' ? value : JSON.stringify(value)
      return `${key}: ${truncateText(text, 60)}`
    })
    .join(', ')
}

function resizeTextarea() {
  const textarea = textareaRef.value
  if (!textarea) return

  textarea.style.height = 'auto'
  textarea.style.height = `${Math.min(textarea.scrollHeight, 144)}px`
}

async function scrollToBottom() {
  await nextTick()
  if (messagesContainer.value) {
    messagesContainer.value.scrollTop = messagesContainer.value.scrollHeight
  }
}

async function sendMessage() {
  const text = inputText.value.trim()
  if (!text || loading.value || configError.value) return

  loading.value = true
  messages.value.push({ role: 'user', content: text })
  inputText.value = ''

  try {
    const apiMessages = messages.value.map(({ role, content }) => ({ role, content }))
    const response = await invoke<{ content: string; reasoning_content?: string }>('ai_chat', {
      config: aiConfig.value,
      messages: apiMessages,
      context: buildSafeContext()
    })
    const responseContent = response?.content ?? ''
    const reasoningContent = response?.reasoning_content
    const actions = await parseActions(responseContent)

    messages.value.push({
      role: 'assistant',
      content: stripActionBlocks(responseContent),
      actions: actions.length ? actions : undefined,
      reasoning: reasoningContent || undefined
    })

    if (actions.some((action) => action.success)) {
      await loadContext()
      await emit('ai-data-changed')
    }
  } catch (error) {
    messages.value.push({ role: 'assistant', content: `${t('common.error')}: ${error}` })
  } finally {
    loading.value = false
    await nextTick()
    resizeTextarea()
  }
}

function keydown(event: KeyboardEvent) {
  if (event.isComposing) return
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    void sendMessage()
  }
}

watch(() => messages.value.length, () => void scrollToBottom())
watch(inputText, async () => {
  await nextTick()
  resizeTextarea()
})

onMounted(async () => {
  await loadContext()
  resizeTextarea()
})
</script>

<template>
  <div class="rounded-xl border bg-card shadow-sm overflow-hidden">
    <button
      class="w-full flex items-center justify-between px-4 py-3 hover:bg-secondary transition-colors"
      @click="collapsed = !collapsed"
    >
      <span class="flex items-center gap-2">
        <MessageCircle class="w-4 h-4 text-primary" />
        <span class="text-sm font-semibold text-foreground">
          {{ t('dashboardExt.aiChat') }}
        </span>
        <LoaderCircle v-if="loading" class="w-3.5 h-3.5 animate-spin text-muted-foreground" />
      </span>
      <ChevronDown v-if="collapsed" class="w-4 h-4 text-muted-foreground" />
      <ChevronUp v-else class="w-4 h-4 text-muted-foreground" />
    </button>

    <div v-if="!collapsed" class="border-t">
      <div v-if="configError" class="px-4 py-8 text-center">
        <Bot class="w-8 h-8 text-muted-foreground/50 mx-auto mb-2" />
        <p class="text-sm text-muted-foreground">Please configure AI settings first.</p>
      </div>

      <template v-else>
        <div ref="messagesContainer" class="h-[340px] max-h-[55vh] overflow-y-auto px-4 py-3 space-y-3">
          <div v-if="messages.length === 0" class="flex items-start gap-2">
            <Bot class="w-6 h-6 shrink-0 text-primary" />
            <div class="bg-muted rounded-xl px-3 py-2">
              <p class="text-sm text-foreground">
                {{ t('dashboardExt.aiWelcome') }}
              </p>
              <p class="text-xs text-muted-foreground mt-1">
                {{ t('dashboardExt.aiContextInfo') }}
              </p>
            </div>
          </div>

          <div
            v-for="(message, index) in messages"
            :key="index"
            :class="message.role === 'user' ? 'flex justify-end' : 'flex items-start gap-2'"
          >
            <Bot v-if="message.role === 'assistant'" class="w-6 h-6 shrink-0 text-primary" />

            <div class="max-w-[85%] space-y-1">
              <details
                v-if="message.reasoning"
                class="rounded-lg border border-violet-200 bg-violet-50/60 dark:border-violet-500/20 dark:bg-violet-500/5"
              >
                <summary class="flex cursor-pointer select-none items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-violet-600 dark:text-violet-400">
                  <Brain class="w-3.5 h-3.5" />
                  思考过程
                </summary>
                <div class="px-3 pb-2 pt-0.5">
                  <p class="text-xs whitespace-pre-wrap leading-relaxed text-violet-700/80 dark:text-violet-300/70">{{ message.reasoning }}</p>
                </div>
              </details>

              <div
                class="rounded-xl px-3 py-2 break-words"
                :class="
                  message.role === 'user'
                    ? 'bg-primary text-primary-foreground'
                    : 'bg-muted text-foreground'
                "
              >
                <p class="text-sm whitespace-pre-wrap">{{ message.content }}</p>
              </div>

              <details
                v-if="message.actions && message.actions.length"
                class="rounded-lg border bg-secondary/60"
              >
                <summary class="flex cursor-pointer select-none items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-muted-foreground">
                  <Wrench class="w-3.5 h-3.5" />
                  使用了 {{ message.actions.length }} 个工具
                  <span class="ml-auto flex items-center gap-1">
                    <span
                      v-for="(action, i) in message.actions"
                      :key="i"
                      class="w-1.5 h-1.5 rounded-full"
                      :class="action.success ? 'bg-green-400' : 'bg-red-400'"
                    />
                  </span>
                </summary>
                <div class="border-t px-3 py-2 space-y-2">
                  <div
                    v-for="(action, actionIndex) in message.actions"
                    :key="`${action.command}-${actionIndex}`"
                    class="text-xs"
                  >
                    <div class="flex items-center gap-2">
                      <span :class="action.success ? 'text-green-500' : 'text-red-500'">
                        {{ action.success ? '✓' : '✗' }}
                      </span>
                      <span class="font-medium text-foreground">{{ actionLabel(action.command) }}</span>
                      <code class="text-[10px] text-muted-foreground font-mono">{{ action.command }}</code>
                    </div>
                    <div v-if="formatParams(action.params)" class="ml-5 mt-0.5 text-[11px] text-muted-foreground font-mono break-all">
                      {{ formatParams(action.params) }}
                    </div>
                    <div v-if="action.result" class="ml-5 mt-0.5 text-[11px]" :class="action.success ? 'text-muted-foreground' : 'text-red-500 dark:text-red-400'">
                      → {{ action.result }}
                    </div>
                  </div>
                </div>
              </details>
            </div>

            <User v-if="message.role === 'user'" class="w-6 h-6 shrink-0 text-primary" />
          </div>

          <div v-if="loading" class="flex items-center gap-2 text-sm text-muted-foreground">
            <LoaderCircle class="w-4 h-4 animate-spin" />
            {{ t('dashboardExt.aiThinking') }}
          </div>
        </div>

        <div class="border-t px-3 py-2 flex items-end gap-2">
          <textarea
            ref="textareaRef"
            v-model="inputText"
            rows="1"
            class="max-h-36 flex-1 resize-none rounded-lg border border-input bg-transparent px-3 py-2 text-sm leading-5 outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/20 disabled:opacity-60"
            :placeholder="t('dashboardExt.aiPlaceholder')"
            :disabled="loading"
            @input="resizeTextarea"
            @keydown="keydown"
          />
          <button
            class="w-9 h-9 shrink-0 rounded-lg bg-primary text-primary-foreground flex items-center justify-center transition hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="loading || !inputText.trim()"
            @click="sendMessage"
          >
            <LoaderCircle v-if="loading" class="w-4 h-4 animate-spin" />
            <Send v-else class="w-4 h-4" />
          </button>
        </div>
      </template>
    </div>
  </div>
</template>
