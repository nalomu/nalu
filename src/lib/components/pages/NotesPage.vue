<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ArrowLeft, ChevronDown, Plus, Trash2, FileText, Search, Tag, Eye, Pencil, X } from 'lucide-vue-next'
import type { Note } from '$lib/types'
import { useI18n } from '$lib/i18n'
import { useAiRefresh } from '$lib/composables/useAiRefresh'
import { renderMarkdown } from '$lib/utils/markdown'
import { useMobile } from '$lib/composables/useMobile'

const { t } = useI18n()
const { isMobilePlatform } = useMobile()
const notes = ref<Note[]>([])
const selectedNote = ref<Note | null>(null)
const searchQuery = ref('')
const filterType = ref('all')
const selectedTag = ref('')
const preview = ref(false)
const showMobileTypePicker = ref(false)

// Tag editor state
const tagInput = ref('')
const showTagSuggestions = ref(false)
const tagInputRef = ref<HTMLInputElement>()

function parseTags(value: string) {
  return value.split(',').map((tag) => tag.trim()).filter(Boolean)
}

const allTags = computed(() => {
  const set = new Set<string>()
  for (const note of notes.value) {
    for (const tag of parseTags(note.tags)) set.add(tag)
  }
  return [...set].sort()
})

const currentNoteTags = computed(() => {
  if (!selectedNote.value) return [] as string[]
  return parseTags(selectedNote.value.tags)
})

const tagSuggestions = computed(() => {
  const current = new Set(currentNoteTags.value)
  const query = tagInput.value.toLowerCase()
  return allTags.value.filter((tag) => !current.has(tag) && (!query || tag.toLowerCase().includes(query)))
})

const filteredNotes = computed(() => notes.value.filter((note) => {
  const query = searchQuery.value.toLowerCase()
  if (query && !note.title.toLowerCase().includes(query) && !note.content.toLowerCase().includes(query)) return false
  if (filterType.value !== 'all' && note.note_type !== filterType.value) return false
  if (selectedTag.value && !parseTags(note.tags).includes(selectedTag.value)) return false
  return true
}))

const renderedContent = computed(() => renderMarkdown(selectedNote.value?.content ?? ''))

function setNoteTags(tags: string[]) {
  if (!selectedNote.value) return
  selectedNote.value.tags = tags.join(',')
  void updateNote()
}

function addTag(tag: string) {
  const normalized = tag.trim()
  if (!normalized || !selectedNote.value) return
  const tags = currentNoteTags.value
  if (!tags.includes(normalized)) {
    setNoteTags([...tags, normalized])
  }
  tagInput.value = ''
  showTagSuggestions.value = false
}

function removeTag(tag: string) {
  setNoteTags(currentNoteTags.value.filter((t) => t !== tag))
}

function onTagInputKeydown(event: KeyboardEvent) {
  if (event.isComposing) return
  if (event.key === 'Enter' || event.key === ',') {
    event.preventDefault()
    if (tagInput.value.trim()) {
      addTag(tagInput.value)
    }
  }
  if (event.key === 'Backspace' && !tagInput.value && currentNoteTags.value.length) {
    removeTag(currentNoteTags.value[currentNoteTags.value.length - 1])
  }
}

function onTagInputBlur() {
  setTimeout(() => {
    if (tagInput.value.trim()) addTag(tagInput.value)
    showTagSuggestions.value = false
  }, 150)
}

async function focusTagInput() {
  showTagSuggestions.value = true
  await nextTick()
  tagInputRef.value?.focus()
}

async function loadNotes() {
  try {
    notes.value = await invoke('get_notes')
  } catch (error) {
    console.error(error)
  }
}

async function addNote() {
  selectedNote.value = await invoke('add_note', { title: t('notes.untitled'), content: '', tags: '', noteType: 'memo' })
  preview.value = false
  tagInput.value = ''
  await loadNotes()
}

async function updateNote() {
  const note = selectedNote.value
  if (!note) return
  await invoke('update_note', { id: note.id, title: note.title, content: note.content, tags: note.tags })
  await loadNotes()
}

async function setNoteType(type: 'memo' | 'note') {
  if (!selectedNote.value) return
  selectedNote.value.note_type = type
  showMobileTypePicker.value = false
  await updateNote()
}

async function deleteNote(id: string) {
  await invoke('delete_note', { id })
  if (selectedNote.value?.id === id) selectedNote.value = null
  await loadNotes()
}

function selectNote(note: Note) {
  selectedNote.value = { ...note }
  preview.value = false
  tagInput.value = ''
  showTagSuggestions.value = false
}

function closeMobileNote() {
  selectedNote.value = null
  preview.value = false
  tagInput.value = ''
  showTagSuggestions.value = false
}

onMounted(loadNotes)
useAiRefresh(loadNotes)
</script>

<template>
  <div v-if="isMobilePlatform" class="flex h-full min-h-0 flex-col overflow-hidden">
    <div v-if="!selectedNote" class="flex h-full flex-col">
      <div class="border-b bg-background px-4 py-3">
        <div class="flex items-center gap-3">
          <div class="mobile-field flex flex-1 items-center gap-2 bg-secondary px-3">
            <Search class="h-5 w-5 shrink-0 text-muted-foreground" />
            <input v-model="searchQuery" class="min-w-0 flex-1 bg-transparent text-base outline-none" :placeholder="t('notes.search')" />
          </div>
          <button class="mobile-icon-button h-14 w-14 rounded-2xl bg-primary text-primary-foreground transition-colors hover:bg-primary/90" @click="addNote">
            <Plus class="h-6 w-6" />
          </button>
        </div>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-for="type in ['all', 'memo', 'note']"
            :key="type"
            class="inline-flex h-10 min-w-16 items-center justify-center rounded-xl px-3 text-sm transition-colors"
            :class="filterType === type ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'"
            @click="filterType = type"
          >{{ t(`notes.${type}`) }}</button>
        </div>
        <div v-if="allTags.length" class="mt-2 flex items-center gap-1 overflow-x-auto">
          <Tag class="h-3 w-3 shrink-0 text-muted-foreground" />
          <button
            class="shrink-0 rounded px-2 py-1 text-[11px] transition-colors"
            :class="selectedTag === '' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'"
            @click="selectedTag = ''"
          >{{ t('notes.all') }}</button>
          <button
            v-for="tag in allTags"
            :key="tag"
            class="shrink-0 rounded px-2 py-1 text-[11px] transition-colors"
            :class="selectedTag === tag ? 'bg-accent text-accent-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
            @click="selectedTag = selectedTag === tag ? '' : tag"
          >#{{ tag }}</button>
        </div>
      </div>
      <div class="flex-1 overflow-y-auto">
        <button
          v-for="note in filteredNotes"
          :key="note.id"
          class="w-full border-b px-4 py-4 text-left transition-colors active:bg-secondary"
          @click="selectNote(note)"
        >
          <div class="flex min-w-0 items-center gap-3">
            <FileText class="h-5 w-5 shrink-0 text-muted-foreground" />
            <span class="truncate text-base font-medium">{{ note.title }}</span>
          </div>
          <div class="mt-1 truncate pl-8 text-sm text-muted-foreground">{{ note.content || t('notes.empty') }}</div>
          <div v-if="parseTags(note.tags).length" class="mt-1 flex flex-wrap gap-1 pl-8">
            <span v-for="tag in parseTags(note.tags)" :key="tag" class="text-[10px] text-primary">#{{ tag }}</span>
          </div>
        </button>
        <div v-if="filteredNotes.length === 0" class="py-8 text-center text-sm text-muted-foreground">{{ t('notes.empty') }}</div>
      </div>
    </div>

    <div v-else class="flex h-full min-h-0 flex-col overflow-hidden">
      <div class="shrink-0 border-b bg-background px-4 py-3">
        <div class="flex items-center gap-2">
          <button class="mobile-icon-button rounded-xl text-muted-foreground transition-colors hover:bg-secondary" @click="closeMobileNote">
            <ArrowLeft class="h-4 w-4" />
          </button>
          <input v-model="selectedNote.title" class="min-w-0 flex-1 bg-transparent text-base font-semibold outline-none" @blur="updateNote" />
          <button
            class="mobile-icon-button rounded-xl text-muted-foreground transition-colors hover:bg-secondary"
            :class="{ 'bg-secondary text-primary': preview }"
            :title="preview ? t('notes.edit') : t('notes.preview')"
            @click="preview = !preview"
          >
            <Eye v-if="!preview" class="h-4 w-4" />
            <Pencil v-else class="h-4 w-4" />
          </button>
          <button class="mobile-icon-button rounded-xl text-muted-foreground/60 transition-colors hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-500/10" @click="deleteNote(selectedNote.id)">
            <Trash2 class="h-4 w-4" />
          </button>
        </div>
        <div class="mt-3 grid grid-cols-[24px_minmax(0,1fr)_88px] items-center gap-2">
          <Tag class="mx-auto h-4 w-4 shrink-0 text-muted-foreground" />
          <div class="relative min-w-0 flex-1">
            <div class="mobile-field flex flex-wrap items-center gap-1.5 border border-input bg-card px-3 py-1" @click="focusTagInput">
              <span v-for="tag in currentNoteTags" :key="tag" class="inline-flex items-center gap-1 rounded-full bg-accent px-2 py-0.5 text-xs text-accent-foreground">
                #{{ tag }}
                <button class="transition-colors hover:text-red-500" @click.stop="removeTag(tag)">
                  <X class="h-3 w-3" />
                </button>
              </span>
              <input
                ref="tagInputRef"
                v-model="tagInput"
                class="min-w-[80px] flex-1 bg-transparent py-0.5 text-xs outline-none"
                :placeholder="currentNoteTags.length ? '' : t('notes.tagsPlaceholder')"
                @keydown="onTagInputKeydown"
                @focus="showTagSuggestions = true"
                @blur="onTagInputBlur"
              />
            </div>
            <div v-if="showTagSuggestions && tagSuggestions.length > 0" class="absolute left-0 right-0 top-full z-20 mt-1 max-h-40 overflow-y-auto rounded-lg border bg-card shadow-lg">
              <button
                v-for="tag in tagSuggestions"
                :key="tag"
                type="button"
                class="flex w-full items-center gap-1.5 px-3 py-2 text-left text-xs transition-colors hover:bg-accent"
                @mousedown.prevent="addTag(tag)"
              >
                <Tag class="h-3 w-3 text-muted-foreground" />
                #{{ tag }}
              </button>
            </div>
          </div>
          <button class="mobile-field flex items-center justify-between gap-2 bg-secondary px-3 text-sm" @click="showMobileTypePicker = true">
            <span class="truncate">{{ t(`notes.${selectedNote.note_type}`) }}</span>
            <ChevronDown class="h-4 w-4 shrink-0 text-muted-foreground" />
          </button>
        </div>
      </div>
      <div v-if="preview" class="min-h-0 flex-1 overflow-y-auto px-4 py-3 text-sm leading-relaxed prose-sm" v-html="renderedContent" />
      <textarea
        v-else
        v-model="selectedNote.content"
        class="min-h-0 flex-1 resize-none bg-transparent px-4 py-3 pb-[calc(var(--mobile-tabbar-height)+var(--mobile-safe-bottom)+24px)] font-mono text-base leading-relaxed outline-none"
        :placeholder="t('notes.startWriting')"
        @blur="updateNote"
      />
    </div>
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="showMobileTypePicker" class="fixed inset-0 z-[95] flex items-end bg-black/35 p-4 backdrop-blur-sm" @click.self="showMobileTypePicker = false">
          <div class="w-full overflow-hidden rounded-2xl border bg-card shadow-2xl">
            <button
              v-for="type in ['memo', 'note'] as const"
              :key="type"
              class="flex h-14 w-full items-center justify-between border-b px-4 text-left text-base last:border-b-0"
              @click="setNoteType(type)"
            >
              <span>{{ t(`notes.${type}`) }}</span>
              <span class="h-5 w-5 rounded-full border-2" :class="selectedNote?.note_type === type ? 'border-primary ring-4 ring-primary/15' : 'border-muted-foreground/50'" />
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>

  <div v-else class="flex h-full">
    <div class="flex w-72 flex-col border-r">
      <div class="space-y-2 border-b p-3">
        <div class="flex items-center gap-2">
          <div class="flex flex-1 items-center gap-2 rounded-lg bg-secondary px-3 py-1.5">
            <Search class="h-3.5 w-3.5 text-muted-foreground" />
            <input v-model="searchQuery" class="flex-1 bg-transparent text-sm outline-none" :placeholder="t('notes.search')" /></div>
          <button class="rounded-lg bg-primary p-1.5 text-primary-foreground transition-colors hover:bg-primary/90" @click="addNote">
            <Plus class="h-4 w-4" />
          </button>
        </div>
        <div class="flex gap-1">
          <button
            v-for="type in ['all', 'memo', 'note']"
            :key="type"
            class="rounded px-2 py-0.5 text-xs transition-colors"
            :class="filterType === type ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'"
            @click="filterType = type"
          >{{ t(`notes.${type}`) }}
          </button>
        </div>
        <div v-if="allTags.length" class="flex flex-wrap items-center gap-1">
          <Tag class="h-3 w-3 text-muted-foreground" />
          <button
            class="rounded px-2 py-0.5 text-[11px] transition-colors"
            :class="selectedTag === '' ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'"
            @click="selectedTag = ''"
          >{{ t('notes.all') }}</button>
          <button
            v-for="tag in allTags"
            :key="tag"
            class="rounded px-2 py-0.5 text-[11px] transition-colors"
            :class="selectedTag === tag ? 'bg-accent text-accent-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/70'"
            @click="selectedTag = selectedTag === tag ? '' : tag"
          >#{{ tag }}</button>
        </div>
      </div>
      <div class="flex-1 overflow-y-auto">
        <button
          v-for="note in filteredNotes"
          :key="note.id"
          class="w-full border-b px-4 py-3 text-left transition-colors hover:bg-secondary"
          :class="{ 'bg-accent': selectedNote?.id === note.id }"
          @click="selectNote(note)"
        >
          <div class="flex items-center gap-2">
            <FileText class="h-3.5 w-3.5 text-muted-foreground" />
            <span class="truncate text-sm font-medium">{{ note.title }}</span></div>
          <div class="mt-1 truncate pl-5 text-xs text-muted-foreground">{{ note.content || t('notes.empty') }}</div>
          <div v-if="parseTags(note.tags).length" class="mt-1 flex flex-wrap gap-1 pl-5">
            <span v-for="tag in parseTags(note.tags)" :key="tag" class="text-[10px] text-primary">#{{ tag }}</span>
          </div>
        </button>
        <div v-if="filteredNotes.length === 0" class="py-8 text-center text-sm text-muted-foreground">{{ t('notes.empty') }}</div>
      </div>
    </div>
    <div class="flex flex-1 flex-col">
      <div v-if="selectedNote" class="flex flex-1 flex-col p-6">
        <div class="mb-4 flex items-center gap-3">
          <input v-model="selectedNote.title" class="flex-1 border-b bg-transparent text-lg font-semibold outline-none" @blur="updateNote" />
          <button
            class="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-secondary"
            :class="{ 'bg-secondary text-primary': preview }"
            :title="preview ? t('notes.edit') : t('notes.preview')"
            @click="preview = !preview"
          >
            <Eye v-if="!preview" class="h-4 w-4" />
            <Pencil v-else class="h-4 w-4" />
          </button>
          <button class="rounded-md p-1 text-muted-foreground/50 transition-colors hover:bg-red-50 hover:text-red-400 dark:hover:bg-red-500/10" @click="deleteNote(selectedNote.id)">
            <Trash2 class="h-4 w-4" />
          </button>
        </div>
        <div class="mb-4 flex items-start gap-2">
          <Tag class="mt-1.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <div class="relative flex-1">
            <div class="flex min-h-[32px] flex-wrap items-center gap-1.5 rounded-lg border border-input bg-card px-2 py-1" @click="focusTagInput">
              <span v-for="tag in currentNoteTags" :key="tag" class="inline-flex items-center gap-1 rounded-full bg-accent px-2 py-0.5 text-xs text-accent-foreground">
                #{{ tag }}
                <button class="transition-colors hover:text-red-500" @click.stop="removeTag(tag)">
                  <X class="h-3 w-3" />
                </button>
              </span>
              <input
                ref="tagInputRef"
                v-model="tagInput"
                class="min-w-[80px] flex-1 bg-transparent py-0.5 text-xs outline-none"
                :placeholder="currentNoteTags.length ? '' : t('notes.tagsPlaceholder')"
                @keydown="onTagInputKeydown"
                @focus="showTagSuggestions = true"
                @blur="onTagInputBlur"
              />
            </div>
            <div v-if="showTagSuggestions && tagSuggestions.length > 0" class="absolute left-0 right-0 top-full z-20 mt-1 max-h-40 overflow-y-auto rounded-lg border bg-card shadow-lg">
              <button
                v-for="tag in tagSuggestions"
                :key="tag"
                type="button"
                class="flex w-full items-center gap-1.5 px-3 py-1.5 text-left text-xs transition-colors hover:bg-accent"
                @mousedown.prevent="addTag(tag)"
              >
                <Tag class="h-3 w-3 text-muted-foreground" />
                #{{ tag }}
              </button>
            </div>
          </div>
          <select v-model="selectedNote.note_type" class="shrink-0 rounded bg-secondary px-2 py-1 text-xs" @change="updateNote">
            <option value="memo">{{ t('notes.memo') }}</option>
            <option value="note">{{ t('notes.note') }}</option>
          </select>
        </div>
        <div v-if="preview" class="flex-1 overflow-y-auto text-sm leading-relaxed prose-sm" v-html="renderedContent" />
        <textarea
          v-else
          v-model="selectedNote.content"
          class="flex-1 resize-none bg-transparent font-mono text-sm leading-relaxed outline-none"
          :placeholder="t('notes.startWriting')"
          @blur="updateNote"
        />
      </div>
      <div v-else class="flex flex-1 items-center justify-center text-sm text-muted-foreground">{{ t('notes.selectOrCreate') }}</div>
    </div>
  </div>
</template>
