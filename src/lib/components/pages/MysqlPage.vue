<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { downloadDir } from '@tauri-apps/api/path'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { open, save } from '@tauri-apps/plugin-dialog'
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs'
import { openPath } from '@tauri-apps/plugin-opener'
import {
  Check, ChevronLeft, ChevronRight, CircleAlert, Copy, Database, Download, Eye, EyeOff,
  FileDown, FileUp, FolderOpen, KeyRound, LoaderCircle, Plus, RefreshCw, Save, Server,
  Settings, ShieldCheck, Trash2, Upload, User, Users, X
} from 'lucide-vue-next'
import type { MysqlUser } from '$lib/types'
import { Tabs, TabsList, TabsTrigger } from '$lib/components/ui/tabs'
import { Button } from '$lib/components/ui/button'
import { Input } from '$lib/components/ui/input'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '$lib/components/ui/dialog'

type Tab = 'databases' | 'users' | 'transfer' | 'settings';
type ServerUser = { user: string; host: string; plugin: string };
type MessageType = 'success' | 'error';
type TransferKind = 'import' | 'export';
const activeTab = ref<Tab>('databases')
const config = reactive({ host: '127.0.0.1', port: 3306, user: 'root', password: '', database: '' })
const connected = ref(false)
const loading = ref(false)
const error = ref('')
const success = ref('')
const databases = ref<string[]>([])
const users = ref<MysqlUser[]>([])
const currentPage = ref(1)
const pageSize = 10
const createOpen = ref(false)
const credentialOpen = ref(false)
const selectedUser = ref<MysqlUser | null>(null)
const createForm = reactive({ database: '', username: '', password: '', host: 'localhost' })
const credentialForm = reactive({ database: '', username: '', password: '' })
const showCreatePassword = ref(false)
const showCredentialPassword = ref(false)
const showUserPassword = ref(false)
const updatedPassword = ref('')
const actionLoading = ref(false)
const selectedExportDatabase = ref('')
const exportDir = ref('')
const lastExportPath = ref('')
const selectedImportDatabase = ref('')
const importFile = ref('')
const serverUsers = ref<ServerUser[]>([])
const messageDialogOpen = ref(false)
const messageDialogType = ref<MessageType>('success')
const messageDialogTitle = ref('')
const messageDialogMessage = ref('')
const confirmOpen = ref(false)
const confirmTitle = ref('')
const confirmMessage = ref('')
let confirmResolver: ((value: boolean) => void) | null = null
const transferProgress = ref<{ kind: TransferKind; label: string; percent: number } | null>(null)
let transferTimer: ReturnType<typeof setInterval> | null = null
const PROTECTED_USERS = ['root', 'mysql.sys', 'mysql.session', 'mysql.infoschema', 'debian-sys-maint']
const managedDatabases = computed(() => [...new Set([...databases.value, ...users.value.flatMap((item) => item.databases.split(',').map((value) => value.trim()).filter(Boolean))])].sort())
const totalPages = computed(() => Math.max(1, Math.ceil(managedDatabases.value.length / pageSize)))
const pagedDatabases = computed(() => managedDatabases.value.slice((currentPage.value - 1) * pageSize, currentPage.value * pageSize))

function clearMessages() {
  error.value = ''
  success.value = ''
}

function showSuccess(message: string, title = '操作完成') {
  success.value = message
  error.value = ''
  messageDialogType.value = 'success'
  messageDialogTitle.value = title
  messageDialogMessage.value = message
  messageDialogOpen.value = true
}

function showError(exception: unknown, title = '操作失败') {
  const message = String(exception)
  error.value = message
  success.value = ''
  messageDialogType.value = 'error'
  messageDialogTitle.value = title
  messageDialogMessage.value = message
  messageDialogOpen.value = true
}

function requestConfirm(title: string, message: string) {
  confirmTitle.value = title
  confirmMessage.value = message
  confirmOpen.value = true
  return new Promise<boolean>((resolve) => {
    confirmResolver = resolve
  })
}

function resolveConfirm(value: boolean) {
  confirmOpen.value = false
  confirmResolver?.(value)
  confirmResolver = null
}

function startTransfer(kind: TransferKind, label: string) {
  if (transferTimer) clearInterval(transferTimer)
  transferProgress.value = { kind, label, percent: 8 }
  transferTimer = setInterval(() => {
    if (!transferProgress.value || transferProgress.value.percent >= 90) return
    transferProgress.value.percent += transferProgress.value.percent < 60 ? 8 : 3
  }, 500)
}

function finishTransfer(successful: boolean) {
  if (transferTimer) {
    clearInterval(transferTimer)
    transferTimer = null
  }
  if (transferProgress.value) {
    transferProgress.value.percent = successful ? 100 : 0
  }
  setTimeout(() => {
    transferProgress.value = null
  }, successful ? 900 : 300)
}

function userForDatabase(database: string) { return users.value.find((item) => item.databases.split(',').map((value) => value.trim()).includes(database)) }

function formatDate(value?: string) {
  if (!value) return '服务器已有'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : new Intl.DateTimeFormat('zh-CN', { dateStyle: 'short', timeStyle: 'short' }).format(date)
}

function saveConfig() { localStorage.setItem('nalu-mysql-config', JSON.stringify(config)) }

async function loadUsers() { users.value = await invoke('get_mysql_users') }

async function testConnection(showMessage = true) {
  loading.value = true
  clearMessages()
  try {
    connected.value = await invoke('mysql_test_connection', { config })
    saveConfig()
    if (showMessage) showSuccess('MySQL 连接成功')
    if (connected.value) await Promise.all([refreshDatabases(), loadServerUsers()])
  } catch (exception) {
    connected.value = false
    showError(exception, '连接失败')
  } finally {
    loading.value = false
  }
}

async function refreshDatabases() {
  try {
    const [serverDatabases, managedUsers] = await Promise.all([invoke<string[]>('mysql_list_databases', { config }), invoke<MysqlUser[]>('get_mysql_users')])
    databases.value = serverDatabases
    users.value = managedUsers
    selectedExportDatabase.value ||= serverDatabases[0] || ''
    selectedImportDatabase.value ||= serverDatabases[0] || ''
  } catch (exception) {
    showError(exception, '刷新数据库失败')
  }
}

function openCreate() {
  Object.assign(createForm, { database: '', username: '', password: '', host: 'localhost' })
  createOpen.value = true
}

async function loadServerUsers() {
  clearMessages()
  try {
    serverUsers.value = await invoke<ServerUser[]>('mysql_list_server_users', { config })
  } catch (exception) {
    showError(exception, '读取用户失败')
  }
}

async function dropServerUser(item: ServerUser) {
  if (!await requestConfirm('删除 MySQL 用户', `确认删除用户 ${item.user}@${item.host}？此操作不可恢复。`)) return
  actionLoading.value = true
  clearMessages()
  try {
    await invoke('mysql_drop_server_user', { config, username: item.user, host: item.host })
    await loadServerUsers()
    showSuccess(`用户 ${item.user}@${item.host} 已删除`)
  } catch (exception) {
    showError(exception, '删除用户失败')
  } finally {
    actionLoading.value = false
  }
}

function openCredential(database = '') {
  Object.assign(credentialForm, { database, username: '', password: '' })
  credentialOpen.value = true
}

async function testCredential(item: { database: string; username: string; password: string }) {
  await invoke('mysql_test_connection', { config: { ...config, user: item.username, password: item.password, database: item.database } })
}

async function saveCredential() {
  actionLoading.value = true
  clearMessages()
  try {
    await testCredential(credentialForm)
    await invoke('upsert_mysql_user', { ...credentialForm })
    await loadUsers()
    credentialOpen.value = false
    showSuccess('凭据已验证并保存')
  } catch (exception) {
    showError(exception, '保存凭据失败')
  } finally {
    actionLoading.value = false
  }
}

async function createDatabase() {
  actionLoading.value = true
  clearMessages()
  try {
    await invoke('mysql_create_database_with_user', { config, databaseName: createForm.database.trim(), newUsername: createForm.username.trim(), newPassword: createForm.password, host: createForm.host.trim() || 'localhost' })
    createOpen.value = false
    await refreshDatabases()
    showSuccess(`数据库 ${createForm.database} 和管理用户已创建`)
  } catch (exception) {
    showError(exception, '创建数据库失败')
  } finally {
    actionLoading.value = false
  }
}

async function deleteDatabase(database: string) {
  const user = userForDatabase(database)
  if (!await requestConfirm('删除数据库', `确认删除数据库 ${database}？此操作不可恢复。`)) return
  try {
    await invoke('mysql_delete_database_with_user', { config, databaseName: database, username: user?.username || null })
    await refreshDatabases()
    showSuccess(`数据库 ${database} 已删除`)
  } catch (exception) {
    showError(exception, '删除数据库失败')
  }
}

async function importCredentials() {
  const path = await open({ multiple: false, filters: [{ name: 'MySQL 凭据', extensions: ['json'] }] })
  if (!path) return
  try {
    const records = JSON.parse(await readTextFile(path as string))
    let imported = 0
    for (const record of records) {
      const item = { database: String(record.database ?? record.databases ?? '').split(',')[0].trim(), username: String(record.username ?? ''), password: String(record.password ?? '') }
      if (!item.database || !item.username || !item.password) continue
      await testCredential(item)
      await invoke('upsert_mysql_user', item)
      imported++
    }
    await loadUsers()
    showSuccess(`已验证并导入 ${imported} 条凭据`)
  } catch (exception) {
    showError(exception, '导入凭据失败')
  }
}

async function exportCredentials() {
  const path = await save({ defaultPath: 'mysql-credentials.json', filters: [{ name: 'MySQL 凭据', extensions: ['json'] }] })
  if (!path) return
  await writeTextFile(path, JSON.stringify(users.value.map(({ username, password, databases: database }) => ({ database, username, password })), null, 2))
  showSuccess('凭据已导出')
}

async function exportDatabase() {
  actionLoading.value = true
  clearMessages()
  startTransfer('export', `正在导出 ${selectedExportDatabase.value}`)
  try {
    lastExportPath.value = await invoke('mysql_export', { config: { ...config, database: selectedExportDatabase.value }, exportDir: exportDir.value, table: null })
    finishTransfer(true)
    showSuccess(`数据库已导出到 ${lastExportPath.value}`, '导出完成')
  } catch (exception) {
    finishTransfer(false)
    showError(exception, '导出失败')
  } finally {
    actionLoading.value = false
  }
}

async function importDatabase() {
  if (!await requestConfirm('导入 SQL 文件', `确认导入到 ${selectedImportDatabase.value}？导入过程可能覆盖同名对象。`)) return
  actionLoading.value = true
  clearMessages()
  startTransfer('import', `正在导入 ${selectedImportDatabase.value}`)
  try {
    const statementCount = await invoke<number>('mysql_import', { config: { ...config, database: selectedImportDatabase.value }, filePath: importFile.value })
    finishTransfer(true)
    showSuccess(`SQL 文件已导入，共执行约 ${statementCount} 条语句`, '导入完成')
  } catch (exception) {
    finishTransfer(false)
    showError(exception, '导入失败')
  } finally {
    actionLoading.value = false
  }
}

async function updatePassword() {
  if (!selectedUser.value || !updatedPassword.value) return
  await invoke('update_mysql_user', { id: selectedUser.value.id, password: updatedPassword.value, databases: null })
  await loadUsers()
  selectedUser.value = null
  showSuccess('本地凭据密码已更新')
}

onMounted(async () => {
  const saved = localStorage.getItem('nalu-mysql-config')
  if (saved) try {
    Object.assign(config, JSON.parse(saved))
  } catch {
    localStorage.removeItem('nalu-mysql-config')
  }
  try {
    exportDir.value = await downloadDir()
  } catch {
  }
  try {
    await loadUsers()
  } catch (exception) {
    showError(exception, '初始化 MySQL 管理器失败')
  }
  if (config.password) await testConnection(false)
})
</script>

<template>
  <div>
    <div class="min-h-full bg-background">
      <div class="max-w-6xl mx-auto px-6 py-7">
        <div class="flex items-center justify-between mb-6">
          <div class="flex items-center gap-3">
            <div class="w-10 h-10 rounded-xl bg-primary text-primary-foreground flex items-center justify-center">
              <Database class="w-5 h-5" />
            </div>
            <div><h1 class="text-2xl font-bold">MySQL 管理器</h1>
              <p class="text-sm text-muted-foreground">管理数据库、独立用户和数据迁移</p></div>
          </div>
          <span
            class="text-xs rounded-full px-3 py-1.5 border"
            :class="connected ? 'bg-emerald-50 text-emerald-700' : 'bg-card text-muted-foreground'"
          >{{ connected ? `${config.user}@${config.host}:${config.port}` : '未连接' }}</span></div>
        <Tabs v-model="activeTab" class="mb-6 w-fit">
          <TabsList>
            <TabsTrigger v-for="tab in [{ id: 'databases', text: '数据库管理', icon: Database }, { id: 'users', text: '用户管理', icon: Users }, { id: 'transfer', text: '导入导出', icon: FileUp }, { id: 'settings', text: '系统配置', icon: Settings }]" :key="tab.id" :value="tab.id">
              <component :is="tab.icon" class="w-4 h-4" />
              {{ tab.text }}
            </TabsTrigger>
          </TabsList>
        </Tabs>
        <div v-if="error" class="message-box bg-red-50 text-red-700">
          <CircleAlert class="w-4 h-4" />
          <span class="flex-1">{{ error }}</span>
          <button class="rounded p-0.5 transition-colors hover:bg-red-100" @click="error = ''">
            <X class="w-4 h-4" />
          </button>
        </div>
        <div v-if="success" class="message-box bg-emerald-50 text-emerald-700">
          <Check class="w-4 h-4" />
          <span class="flex-1">{{ success }}</span>
          <button class="rounded p-0.5 transition-colors hover:bg-emerald-100" @click="success = ''">
            <X class="w-4 h-4" />
          </button>
        </div>

        <section v-if="activeTab === 'databases'">
          <div class="flex justify-between gap-4 mb-5">
            <div><h2 class="text-xl font-bold">数据库管理</h2>
              <p class="text-sm text-muted-foreground">创建数据库，或导入已有数据库凭据。</p></div>
            <div class="flex gap-2 flex-wrap">
              <Button variant="outline" @click="importCredentials">
                <Upload class="w-4 h-4" />
                导入凭据
              </Button>
              <Button variant="outline" :disabled="!users.length" @click="exportCredentials">
                <Download class="w-4 h-4" />
                导出凭据
              </Button>
              <Button variant="outline" @click="openCredential()">
                <KeyRound class="w-4 h-4" />
                手动录入
              </Button>
              <Button variant="outline" :disabled="!connected" @click="refreshDatabases">
                <RefreshCw class="w-4 h-4" />
                刷新
              </Button>
              <button class="primary-button" :disabled="!connected" @click="openCreate">
                <Plus class="w-4 h-4" />
                创建数据库
              </button>
            </div>
          </div>
          <div v-if="!connected && !managedDatabases.length" class="empty-state">
            <Server class="w-10 h-10 text-primary" />
            <h3>连接 MySQL 或导入旧系统凭据</h3>
            <button class="primary-button mt-4" @click="activeTab = 'settings'">前往系统配置</button>
          </div>
          <div v-else class="panel overflow-hidden">
            <table class="w-full">
              <thead>
              <tr>
                <th>数据库名</th>
                <th>管理用户</th>
                <th>创建时间</th>
                <th>操作</th>
              </tr>
              </thead>
              <tbody>
              <tr v-for="database in pagedDatabases" :key="database">
                <td class="font-medium">{{ database }}</td>
                <td>
                  <button v-if="userForDatabase(database)" class="user-chip" @click="selectedUser = userForDatabase(database)!">
                    <User class="w-3.5 h-3.5" />
                    {{ userForDatabase(database)?.username }}
                  </button>
                  <button v-else class="action-link" @click="openCredential(database)">录入凭据</button>
                </td>
                <td>{{ formatDate(userForDatabase(database)?.created_at) }}</td>
                <td>
                  <button v-if="connected" class="action-link text-red-500" @click="deleteDatabase(database)">
                    <Trash2 class="w-3.5 h-3.5" />
                    删除
                  </button>
                </td>
              </tr>
              </tbody>
            </table>
            <div class="flex justify-between px-4 py-3 border-t"><span class="text-xs">共 {{ managedDatabases.length }} 个数据库</span>
              <div class="flex items-center">
                <button class="rounded-md p-1 transition-colors hover:bg-secondary disabled:opacity-30 disabled:hover:bg-transparent" :disabled="currentPage === 1" @click="currentPage--">
                  <ChevronLeft class="w-4 h-4" />
                </button>
                <span class="px-3">{{ currentPage }} / {{ totalPages }}</span>
                <button class="rounded-md p-1 transition-colors hover:bg-secondary disabled:opacity-30 disabled:hover:bg-transparent" :disabled="currentPage === totalPages" @click="currentPage++">
                  <ChevronRight class="w-4 h-4" />
                </button>
              </div>
            </div>
          </div>
        </section>

        <section v-else-if="activeTab === 'users'">
          <div class="flex justify-between gap-4 mb-5">
            <div><h2 class="text-xl font-bold">用户管理</h2>
              <p class="text-sm text-muted-foreground">服务器上的所有 MySQL 用户，可删除多余或错误的用户。</p></div>
            <button class="secondary-button" :disabled="!connected" @click="loadServerUsers">
              <RefreshCw class="w-4 h-4" />
              刷新
            </button>
          </div>
          <div v-if="!connected" class="empty-state">
            <Server class="w-10 h-10 text-muted-foreground/50 mb-3" />
            <p class="text-sm text-muted-foreground">请先连接 MySQL 服务器</p>
            <button class="primary-button mt-4" @click="activeTab = 'settings'">前往系统配置</button>
          </div>
          <div v-else class="panel overflow-hidden">
            <table class="w-full">
              <thead>
              <tr>
                <th>用户名</th>
                <th>主机 (Host)</th>
                <th>认证插件</th>
                <th>操作</th>
              </tr>
              </thead>
              <tbody>
              <tr v-for="su in serverUsers" :key="`${su.user}@${su.host}`">
                <td class="font-medium">{{ su.user }}</td>
                <td>{{ su.host }}</td>
                <td class="text-xs text-muted-foreground">{{ su.plugin }}</td>
                <td>
                  <button
                    v-if="!PROTECTED_USERS.includes(su.user)"
                    class="action-link text-red-500"
                    :disabled="actionLoading"
                    @click="dropServerUser(su)"
                  >
                    <Trash2 class="w-4 h-4" />
                    删除
                  </button>
                  <span v-else class="text-xs text-muted-foreground">系统用户</span>
                </td>
              </tr>
              </tbody>
            </table>
            <div class="px-4 py-3 border-t"><span class="text-xs">共 {{ serverUsers.length }} 个用户</span></div>
          </div>
        </section>

        <section v-else-if="activeTab === 'transfer'"><h2 class="text-xl font-bold mb-5">数据库导入导出</h2>
          <div class="grid md:grid-cols-2 gap-5">
            <div class="panel p-5">
              <h3 class="font-semibold flex gap-2 mb-4">
                <FileDown class="w-4 h-4" />
                导出数据库
              </h3>
              <label class="field-label"><span>数据库</span><select v-model="selectedExportDatabase">
                <option v-for="database in databases" :key="database">{{ database }}</option>
              </select></label>
              <button class="path-picker mt-4" @click="open({ directory: true }).then(path => { if (path) exportDir = path as string })">
                <FolderOpen class="w-4 h-4" />
                {{ exportDir || '选择目录' }}
              </button>
              <button class="primary-button mt-4" :disabled="!selectedExportDatabase || !exportDir || actionLoading" @click="exportDatabase">
                <LoaderCircle v-if="transferProgress?.kind === 'export'" class="w-4 h-4 animate-spin" />
                <Download v-else class="w-4 h-4" />
                {{ transferProgress?.kind === 'export' ? '导出中' : '导出' }}
              </button>
              <button v-if="lastExportPath" class="action-link ml-3" @click="openPath(exportDir)">打开目录</button>
              <div v-if="transferProgress?.kind === 'export'" class="transfer-progress mt-4">
                <div class="flex items-center justify-between text-xs">
                  <span>{{ transferProgress.label }}</span>
                  <span>{{ transferProgress.percent }}%</span>
                </div>
                <div class="progress-track">
                  <div class="progress-bar" :style="{ width: `${transferProgress.percent}%` }" />
                </div>
              </div>
            </div>
            <div class="panel p-5">
              <h3 class="font-semibold flex gap-2 mb-4">
                <FileUp class="w-4 h-4" />
                导入数据库
              </h3>
              <label class="field-label"><span>目标数据库</span><select v-model="selectedImportDatabase">
                <option v-for="database in databases" :key="database">{{ database }}</option>
              </select></label>
              <button class="upload-zone mt-4" @click="open({ filters: [{ name: 'SQL', extensions: ['sql'] }] }).then(path => { if (path) importFile = path as string })">
                <Upload class="w-8 h-8" />
                <span>{{ importFile || '选择 SQL 文件' }}</span></button>
              <button class="primary-button mt-4" :disabled="!selectedImportDatabase || !importFile || actionLoading" @click="importDatabase">
                <LoaderCircle v-if="transferProgress?.kind === 'import'" class="w-4 h-4 animate-spin" />
                <Upload v-else class="w-4 h-4" />
                {{ transferProgress?.kind === 'import' ? '导入中' : '开始导入' }}
              </button>
              <div v-if="transferProgress?.kind === 'import'" class="transfer-progress mt-4">
                <div class="flex items-center justify-between text-xs">
                  <span>{{ transferProgress.label }}</span>
                  <span>{{ transferProgress.percent }}%</span>
                </div>
                <div class="progress-track">
                  <div class="progress-bar" :style="{ width: `${transferProgress.percent}%` }" />
                </div>
              </div>
            </div>
          </div>
        </section>

        <section v-else><h2 class="text-xl font-bold mb-5">系统配置</h2>
          <div class="panel max-w-3xl p-5">
            <div v-if="connected" class="flex gap-3 p-4 mb-6 rounded-xl bg-emerald-50 text-emerald-800">
              <ShieldCheck class="w-5 h-5" />
              <span>连接正常</span></div>
            <div class="grid gap-4 sm:grid-cols-2">
              <label class="field-label sm:col-span-2"><span>主机地址</span><input v-model="config.host" /></label><label class="field-label"><span>端口</span><input
              v-model.number="config.port"
              type="number"
            /></label><label class="field-label"><span>管理用户</span><input v-model="config.user" /></label><label class="field-label sm:col-span-2"><span>管理密码</span><input
              v-model="config.password"
              type="password"
            /></label></div>
            <button class="primary-button mt-6" :disabled="loading" @click="testConnection(true)">
              <LoaderCircle v-if="loading" class="w-4 h-4 animate-spin" />
              <Save v-else class="w-4 h-4" />
              保存并测试
            </button>
          </div>
        </section>
      </div>
    </div>

    <Dialog :open="createOpen || credentialOpen" @update:open="(v) => { if (!v) createOpen = credentialOpen = false }">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{{ createOpen ? '创建数据库和用户' : '录入已有数据库凭据' }}</DialogTitle>
        </DialogHeader>
        <div class="space-y-4">
          <label class="field-label"><span>数据库名</span><Input v-model="(createOpen ? createForm : credentialForm).database" /></label>
          <label class="field-label"><span>用户名</span><Input v-model="(createOpen ? createForm : credentialForm).username" /></label>
          <label class="field-label"><span>密码</span>
            <div class="password-field"><Input v-model="(createOpen ? createForm : credentialForm).password" :type="(createOpen ? showCreatePassword : showCredentialPassword) ? 'text' : 'password'" />
              <button type="button" @click="createOpen ? showCreatePassword = !showCreatePassword : showCredentialPassword = !showCredentialPassword">
                <Eye class="w-4 h-4" />
              </button>
            </div>
          </label>
          <label v-if="createOpen" class="field-label"><span>主机 (Host)</span>
            <select v-model="createForm.host">
              <option value="localhost">localhost (仅本机)</option>
              <option value="%">% (任意主机)</option>
            </select>
          </label>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="createOpen = credentialOpen = false">取消</Button>
          <Button :disabled="actionLoading" @click="createOpen ? createDatabase() : saveCredential()">{{ createOpen ? '创建' : '测试并保存' }}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    <Dialog :open="!!selectedUser" @update:open="(v) => { if (!v) selectedUser = null }">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>数据库用户信息</DialogTitle>
        </DialogHeader>
        <div v-if="selectedUser" class="space-y-4">
          <div
            v-for="[label, value] in [['数据库', selectedUser.databases], ['用户名', selectedUser.username], ['密码', showUserPassword ? selectedUser.password : '••••••••']]"
            :key="label"
            class="credential-row"
          >
            <div><span>{{ label }}</span><strong>{{ value }}</strong></div>
            <button class="rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-card hover:text-foreground" @click="writeText(String(value))">
              <Copy class="w-4 h-4" />
            </button>
          </div>
          <button class="action-link" @click="showUserPassword = !showUserPassword">
            <EyeOff v-if="showUserPassword" class="w-4 h-4" />
            <Eye v-else class="w-4 h-4" />
            显示/隐藏密码
          </button>
          <label class="field-label"><span>更新本地记录密码</span><Input v-model="updatedPassword" type="password" /></label>
          <Button :disabled="!updatedPassword" @click="updatePassword">保存本地密码</Button>
        </div>
      </DialogContent>
    </Dialog>
    <Dialog :open="confirmOpen" @update:open="(v) => { if (!v) resolveConfirm(false) }">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{{ confirmTitle }}</DialogTitle>
        </DialogHeader>
        <p class="text-sm text-muted-foreground leading-6">{{ confirmMessage }}</p>
        <DialogFooter>
          <Button variant="outline" @click="resolveConfirm(false)">取消</Button>
          <Button :disabled="actionLoading" @click="resolveConfirm(true)">确认</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
    <Dialog :open="messageDialogOpen" @update:open="messageDialogOpen = $event">
      <DialogContent>
        <DialogHeader>
          <DialogTitle class="flex items-center gap-2">
            <Check v-if="messageDialogType === 'success'" class="w-5 h-5 text-emerald-600" />
            <CircleAlert v-else class="w-5 h-5 text-red-600" />
            {{ messageDialogTitle }}
          </DialogTitle>
        </DialogHeader>
        <p class="text-sm text-muted-foreground leading-6 break-words">{{ messageDialogMessage }}</p>
        <DialogFooter>
          <Button @click="messageDialogOpen = false">知道了</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>

<style scoped>
@reference "../../../app.css";
.tab-button {
  @apply inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm text-muted-foreground;
}

.tab-active {
  @apply bg-accent text-accent-foreground font-medium;
}

.primary-button {
  @apply inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium transition-colors hover:bg-primary/90 disabled:opacity-40 disabled:hover:bg-primary;
}

.secondary-button {
  @apply inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg border bg-card text-sm transition-colors hover:bg-secondary disabled:opacity-40 disabled:hover:bg-card;
}

.panel {
  @apply rounded-xl border bg-card shadow-sm;
}

.field-label {
  @apply flex flex-col gap-1.5 text-sm;
}

.field-label select {
  @apply h-10 w-full rounded-md border border-input bg-card px-3 outline-none text-sm;
}

.message-box {
  @apply flex items-center gap-2 px-4 py-3 mb-5 rounded-xl border text-sm;
}

.empty-state {
  @apply min-h-72 rounded-xl border border-dashed bg-card flex flex-col items-center justify-center p-8;
}

table th {
  @apply bg-secondary px-4 py-3 text-left text-xs border-b text-muted-foreground;
}

table td {
  @apply px-4 py-4 text-sm border-b;
}

.user-chip, .action-link {
  @apply inline-flex items-center gap-1.5 text-xs font-medium text-primary transition-colors hover:text-primary/70;
}

.path-picker {
  @apply w-full flex items-center gap-2 rounded-lg border px-3 py-2 text-sm transition-colors hover:bg-secondary;
}

.upload-zone {
  @apply w-full min-h-36 rounded-xl border border-dashed flex flex-col items-center justify-center gap-2 transition-colors hover:bg-secondary hover:border-primary/40;
}

.transfer-progress {
  @apply rounded-lg border bg-secondary/50 px-3 py-2 text-muted-foreground;
}

.progress-track {
  @apply mt-2 h-2 overflow-hidden rounded-full bg-background;
}

.progress-bar {
  @apply h-full rounded-full bg-primary transition-all duration-500;
}

.password-field {
  @apply relative;
}

.password-field button {
  @apply absolute right-3 top-2.5 text-muted-foreground transition-colors hover:text-foreground;
}

.credential-row {
  @apply flex items-center justify-between gap-3 rounded-xl border bg-secondary px-4 py-3;
}

.credential-row div {
  @apply flex flex-col;
}

.credential-row span {
  @apply text-xs text-muted-foreground;
}

.credential-row strong {
  @apply font-mono text-sm;
}
</style>
