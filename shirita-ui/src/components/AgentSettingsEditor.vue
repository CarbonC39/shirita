<script setup lang="ts">
import type { AgentSettings, AgentSettingsView } from '../api/types'
import ToggleSwitch from './ToggleSwitch.vue'

const props = defineProps<{
  modelValue: AgentSettings
  metadata: AgentSettingsView
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: AgentSettings] }>()

function set<K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) {
  emit('update:modelValue', { ...props.modelValue, [key]: value })
}

function numberValue(event: Event): number {
  return Number((event.target as HTMLInputElement).value)
}

function toggleTool(name: string, enabled: boolean) {
  const current = props.modelValue.enabled_tools
  set('enabled_tools', enabled ? [...new Set([...current, name])] : current.filter((item) => item !== name))
}
</script>

<template>
  <fieldset :disabled="disabled" class="space-y-4" data-test="agent-settings-editor">
    <label class="flex items-center justify-between gap-3 text-[13px]">
      <span>{{ $t('settings.agentEnabled') }}</span>
      <ToggleSwitch :model-value="modelValue.enabled" @update:model-value="set('enabled', $event)" />
    </label>

    <div>
      <label class="text-[13px] block mb-1.5">{{ $t('settings.agentTransport') }}</label>
      <select class="field w-full" :value="modelValue.transport" @change="set('transport', ($event.target as HTMLSelectElement).value as AgentSettings['transport'])">
        <option value="auto">{{ $t('settings.agentTransportAuto') }}</option>
        <option value="native">{{ $t('settings.agentTransportNative') }}</option>
        <option value="xml">{{ $t('settings.agentTransportXml') }}</option>
      </select>
    </div>

    <div class="grid grid-cols-2 gap-3">
      <label class="text-[12px] text-muted">{{ $t('settings.agentRounds') }}
        <input class="field w-full mt-1" type="number" min="1" :max="metadata.limits.hard_max_rounds" :value="modelValue.max_rounds" @input="set('max_rounds', numberValue($event))" />
      </label>
      <label class="text-[12px] text-muted">{{ $t('settings.agentToolCalls') }}
        <input class="field w-full mt-1" type="number" min="1" :max="metadata.limits.hard_max_tool_calls" :value="modelValue.max_tool_calls" @input="set('max_tool_calls', numberValue($event))" />
      </label>
      <label class="text-[12px] text-muted">{{ $t('settings.agentTimeout') }}
        <input class="field w-full mt-1" type="number" min="1" :max="metadata.limits.hard_max_tool_timeout_ms" :value="modelValue.tool_timeout_ms" @input="set('tool_timeout_ms', numberValue($event))" />
      </label>
      <label class="text-[12px] text-muted">{{ $t('settings.agentRepetition') }}
        <input class="field w-full mt-1" type="number" min="1" :max="modelValue.max_rounds" :value="modelValue.max_identical_call_rounds" @input="set('max_identical_call_rounds', numberValue($event))" />
      </label>
    </div>

    <div class="space-y-2">
      <label class="flex items-center justify-between gap-3 text-[13px]">
        <span>{{ $t('settings.agentActivity') }}</span>
        <ToggleSwitch :model-value="modelValue.show_activity" @update:model-value="set('show_activity', $event)" />
      </label>
      <label class="flex items-center justify-between gap-3 text-[13px]">
        <span>{{ $t('settings.agentUserStatus') }}</span>
        <ToggleSwitch :model-value="modelValue.show_user_status" @update:model-value="set('show_user_status', $event)" />
      </label>
    </div>

    <div v-if="metadata.tools.some((tool) => !tool.required)" class="space-y-2">
      <p class="text-[13px] font-medium">{{ $t('settings.agentTools') }}</p>
      <label v-for="tool in metadata.tools.filter((item) => !item.required)" :key="tool.name" class="flex items-start gap-2 text-[12px]">
        <input type="checkbox" class="mt-0.5" :checked="modelValue.enabled_tools.includes(tool.name)" @change="toggleTool(tool.name, ($event.target as HTMLInputElement).checked)" />
        <span><span class="font-mono text-ink">{{ tool.name }}</span><span class="block text-muted">{{ tool.description }}</span></span>
      </label>
    </div>

    <div>
      <label class="text-[13px] block mb-1.5">{{ $t('settings.agentSystemPrompt') }}</label>
      <textarea class="field w-full min-h-36 font-mono text-[12px]" :value="modelValue.system_prompt" @input="set('system_prompt', ($event.target as HTMLTextAreaElement).value)" />
    </div>
    <div>
      <label class="text-[13px] block mb-1.5">{{ $t('settings.agentUnfinishedPrompt') }}</label>
      <textarea class="field w-full min-h-24 font-mono text-[12px]" :value="modelValue.unfinished_prompt" @input="set('unfinished_prompt', ($event.target as HTMLTextAreaElement).value)" />
    </div>
  </fieldset>
</template>
