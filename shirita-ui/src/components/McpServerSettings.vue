<script setup lang="ts">
import { onMounted, ref } from 'vue'
import type { McpAccess, McpPolicy, McpServerConfig, McpServerView, McpToolDef, McpTransport } from '../api/types'
import {
  listMcpServers, createMcpServer, updateMcpServer, deleteMcpServer,
  testMcpServer, refreshMcpTools, getSettings, updateSettings,
} from '../api/client'
import ToggleSwitch from './ToggleSwitch.vue'

const servers = ref<McpServerView[]>([])
const editing = ref<McpServerConfig | null>(null)
const isNew = ref(false)
const error = ref<string | null>(null)
const busy = ref(false)
const testResult = ref<Record<string, { ok: boolean; error?: string }>>({})
const discovered = ref<Record<string, McpToolDef[]>>({})
const policy = ref<McpPolicy>({ tools: {} })

function blankConfig(): McpServerConfig {
  return {
    id: '',
    name: '',
    enabled: true,
    transport: { transport: 'streamable_http', url: '', headers: [] },
    request_timeout_ms: 5000,
  }
}

async function loadPolicy() {
  try {
    const settings = await getSettings()
    const raw = settings['mcp.policy']
    policy.value = raw ? (typeof raw === 'string' ? JSON.parse(raw) : raw as McpPolicy) : { tools: {} }
  } catch { policy.value = { tools: {} } }
}

async function load() {
  busy.value = true
  error.value = null
  try {
    servers.value = await listMcpServers()
    await loadPolicy()
  } catch (e) {
    error.value = (e as Error).message
  } finally {
    busy.value = false
  }
}

async function savePolicy() {
  try {
    await updateSettings({ 'mcp.policy': policy.value })
  } catch (e) {
    error.value = (e as Error).message
  }
}

function startNew() {
  editing.value = blankConfig()
  isNew.value = true
  error.value = null
}
function startEdit(server: McpServerView) {
  editing.value = {
    id: server.id,
    name: server.name,
    enabled: server.enabled,
    transport: JSON.parse(JSON.stringify(server.transport)),
    request_timeout_ms: server.request_timeout_ms,
  }
  isNew.value = false
  error.value = null
}

async function saveServer() {
  if (!editing.value) return
  error.value = null
  try {
    if (isNew.value) {
      await createMcpServer(editing.value)
    } else {
      await updateMcpServer(editing.value.id, editing.value)
    }
    editing.value = null
    await load()
  } catch (e) {
    error.value = (e as Error).message
  }
}

async function remove(id: string) {
  await deleteMcpServer(id)
  await load()
}

async function testServer(id: string) {
  testResult.value[id] = await testMcpServer(id)
}

async function refresh(id: string) {
  const result = await refreshMcpTools(id)
  if (result.ok && result.tools) {
    discovered.value[id] = result.tools
  }
  testResult.value[id] = { ok: result.ok, error: result.error }
  // Seed the policy for newly discovered tools as disabled (default).
  const cfg = servers.value.find((s) => s.id === id)
  if (cfg) {
    for (const tool of result.tools ?? []) {
      const name = `mcp.${id}.${tool.name}`
      if (!(name in policy.value.tools)) policy.value.tools[name] = 'ask'
    }
  }
}

function accessLabel(access: McpAccess | undefined): string {
  return access ?? 'disabled'
}

onMounted(load)
</script>

<template>
  <div data-test="mcp-settings">
    <p v-if="error" class="text-[12px] text-coral">{{ error }}</p>

    <div class="space-y-3">
      <div v-for="server in servers" :key="server.id" class="rounded-lg border border-line p-3">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <span class="font-mono text-ink">{{ server.name }}</span>
            <span class="ml-2 text-[12px] text-muted">{{ server.transport.transport === 'stdio' ? 'stdio' : 'HTTP' }}{{ server.has_secret ? ' • 🔒' : '' }}</span>
          </div>
          <div class="flex gap-2 shrink-0">
            <button class="btn btn-ghost" @click="testServer(server.id)">{{ $t('mcp.test') }}</button>
            <button class="btn btn-ghost" @click="refresh(server.id)">{{ $t('mcp.refreshTools') }}</button>
            <button class="btn btn-ghost" @click="startEdit(server)">{{ $t('common.edit') }}</button>
            <button class="btn btn-ghost" @click="remove(server.id)">{{ $t('common.delete') }}</button>
          </div>
        </div>
        <p v-if="testResult[server.id]" class="mt-1 text-[12px]" :class="testResult[server.id].ok ? 'text-ink' : 'text-coral'">
          {{ testResult[server.id].ok ? $t('mcp.testOk') : (testResult[server.id].error ?? $t('mcp.testFailed')) }}
        </p>
        <div v-if="discovered[server.id]" class="mt-2 space-y-1 border-t border-line pt-2">
          <p class="text-[12px] font-semibold text-muted">{{ $t('mcp.discoveredTools') }}</p>
          <label v-for="tool in discovered[server.id]" :key="tool.name" class="flex items-center justify-between gap-3 text-[12px]">
            <span class="truncate font-mono">{{ tool.name }}</span>
            <select class="field max-w-32" :value="policy.tools[`mcp.${server.id}.${tool.name}`]" @change="policy.tools[`mcp.${server.id}.${tool.name}`] = ($event.target as HTMLSelectElement).value as McpAccess">
              <option value="disabled">{{ $t('mcp.disabled') }}</option>
              <option value="allow">{{ $t('mcp.allow') }}</option>
              <option value="ask">{{ $t('mcp.ask') }}</option>
            </select>
          </label>
          <button class="btn mt-2" @click="savePolicy">{{ $t('common.save') }}</button>
        </div>
      </div>
      <p v-if="!servers.length && !busy" class="text-[12px] text-muted">{{ $t('mcp.none') }}</p>
    </div>

    <div class="mt-3">
      <button v-if="!editing" class="btn" @click="startNew">{{ $t('mcp.addServer') }}</button>
    </div>

    <form v-if="editing" data-test="mcp-editor" class="mt-3 space-y-3 rounded-lg border border-line p-3" @submit.prevent="saveServer">
      <label class="block text-[12px]">
        {{ $t('mcp.serverId') }}
        <input class="field w-full mt-1" v-model="editing.id" :disabled="!isNew" />
      </label>
      <label class="block text-[12px]">
        {{ $t('mcp.serverName') }}
        <input class="field w-full mt-1" v-model="editing.name" />
      </label>
      <label class="flex items-center justify-between text-[12px]">
        <span>{{ $t('mcp.enabled') }}</span>
        <ToggleSwitch v-model="editing.enabled" />
      </label>
      <label class="block text-[12px]">
        {{ $t('mcp.transport') }}
        <select class="field w-full mt-1" :value="editing.transport.transport" @change="(editing.transport as any).transport = ($event.target as HTMLSelectElement).value">
          <option value="streamable_http">Streamable HTTP</option>
          <option value="stdio">stdio</option>
        </select>
      </label>
      <template v-if="editing.transport.transport === 'streamable_http'">
        <label class="block text-[12px]">
          {{ $t('mcp.url') }}
          <input class="field w-full mt-1" v-model="editing.transport.url" placeholder="https://…/mcp" />
        </label>
        <label class="block text-[12px]">
          {{ $t('mcp.headers') }}
          <div v-for="(h, i) in editing.transport.headers" :key="i" class="flex gap-2 mt-1">
            <input class="field flex-1" :value="h[0]" @input="editing.transport.headers[i][0] = ($event.target as HTMLInputElement).value" placeholder="x-api-key" />
            <input class="field flex-1" :value="h[1]" @input="editing.transport.headers[i][1] = ($event.target as HTMLInputElement).value" :placeholder="editing.transport.headers[i][1] ? '•••' : 'secret'" />
            <button type="button" class="btn btn-ghost" @click="editing.transport.headers.splice(i, 1)">✕</button>
          </div>
          <button type="button" class="btn btn-ghost mt-1" @click="(editing.transport as any).headers.push(['', ''])">+ {{ $t('mcp.addHeader') }}</button>
        </label>
      </template>
      <template v-else>
        <label class="block text-[12px]">
          {{ $t('mcp.command') }}
          <input class="field w-full mt-1" v-model="(editing.transport as any).command" />
        </label>
        <label class="block text-[12px]">
          {{ $t('mcp.args') }}
          <input class="field w-full mt-1" :value="((editing.transport as any).args ?? []).join(' ')" @input="(editing.transport as any).args = ($event.target as HTMLInputElement).value.split(/\s+/).filter(Boolean)" />
        </label>
      </template>
      <label class="block text-[12px]">
        {{ $t('mcp.timeoutMs') }}
        <input class="field w-full mt-1" type="number" min="1" max="300000" v-model="editing.request_timeout_ms" />
      </label>
      <div class="flex gap-2">
        <button class="btn" type="submit">{{ $t('common.save') }}</button>
        <button class="btn btn-ghost" type="button" @click="editing = null">{{ $t('common.cancel') }}</button>
      </div>
    </form>
  </div>
</template>
