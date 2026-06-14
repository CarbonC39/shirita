<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useSettingsStore } from '../stores/settings'
import { useUiStore } from '../stores/ui'
import { listDefinitions, createDefinition, updateDefinition, deleteDefinition } from '../api/client'
import type { Definition } from '../api/types'
import SliderControl from '../components/SliderControl.vue'
import RegexRuleEditor from '../components/RegexRuleEditor.vue'
import FullscreenEditor from '../components/FullscreenEditor.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import SegmentedControl from '../components/SegmentedControl.vue'
import { Eye, EyeOff } from 'lucide-vue-next'

const settings = useSettingsStore()
const ui = useUiStore()
const loading = ref(true)
const error = ref<string | null>(null)
const regexRules = ref<Definition[]>([])
const showApiKey = ref(false)
const cssFullscreen = ref(false)
const saveMessage = ref('')

const providerSources = ['openai', 'anthropic', 'google', 'openrouter', 'mistral', 'deepseek', 'groq', 'xai', 'cohere', 'together', 'perplexity', 'custom']

const sourceLabels: Record<string, string> = {
  openai: 'OpenAI', anthropic: 'Anthropic', google: 'Google', openrouter: 'OpenRouter',
  mistral: 'Mistral', deepseek: 'DeepSeek', groq: 'Groq', xai: 'xAI',
  cohere: 'Cohere', together: 'Together', perplexity: 'Perplexity', custom: 'Custom…',
}

const defaultBaseUrls: Record<string, string> = {
  openai: 'https://api.openai.com/v1', anthropic: 'https://api.anthropic.com/v1', google: 'https://generativelanguage.googleapis.com/v1beta',
  openrouter: 'https://openrouter.ai/api/v1', mistral: 'https://api.mistral.ai/v1', deepseek: 'https://api.deepseek.com/v1',
  groq: 'https://api.groq.com/openai/v1', xai: 'https://api.x.ai/v1', cohere: 'https://api.cohere.ai/v1',
  together: 'https://api.together.xyz/v1', perplexity: 'https://api.perplexity.ai', custom: '',
}

// Writable computed helpers
function get(k: string) { return settings.data[k] ?? undefined }
function set(k: string, v: unknown) { settings.data[k] = v }

const providerSource = computed({ get: () => get('provider_source') as string || 'openai', set: (v: string) => { set('provider_source', v); set('provider_base_url', defaultBaseUrls[v] || '') } })
const providerBaseUrl = computed({ get: () => get('provider_base_url') as string || '', set: (v: string) => set('provider_base_url', v) })
const providerApiKey = computed({ get: () => get('provider_api_key') as string || '', set: (v: string) => set('provider_api_key', v) })
const providerModel = computed({ get: () => get('provider_model') as string || '', set: (v: string) => set('provider_model', v) })
const providerStream = computed({ get: () => (get('provider_stream') as boolean) ?? true, set: (v: boolean) => set('provider_stream', v) })
const genTemp = computed({ get: () => (get('gen_temperature') as number) ?? 0.7, set: (v: number) => set('gen_temperature', v) })
const genTopP = computed({ get: () => (get('gen_top_p') as number) ?? 0.9, set: (v: number) => set('gen_top_p', v) })
const genFreqPenalty = computed({ get: () => (get('gen_frequency_penalty') as number) ?? 0, set: (v: number) => set('gen_frequency_penalty', v) })
const genPresPenalty = computed({ get: () => (get('gen_presence_penalty') as number) ?? 0, set: (v: number) => set('gen_presence_penalty', v) })
const genMaxTokens = computed({ get: () => (get('gen_max_response_tokens') as number) ?? 4096, set: (v: number) => set('gen_max_response_tokens', v) })
const customCss = computed({ get: () => (get('custom_css') as string) || '', set: (v: string) => set('custom_css', v) })

onMounted(async () => {
  try {
    await settings.load()
    const allDefs = await listDefinitions()
    regexRules.value = allDefs.filter(d => d.type === 'regex_rule')
  } catch (e) { error.value = (e as Error).message }
  finally { loading.value = false }
})

async function handleSave() {
  try {
    await settings.save({
      provider_source: providerSource.value, provider_base_url: providerBaseUrl.value,
      provider_api_key: providerApiKey.value, provider_model: providerModel.value, provider_stream: providerStream.value,
      gen_temperature: genTemp.value, gen_top_p: genTopP.value, gen_frequency_penalty: genFreqPenalty.value,
      gen_presence_penalty: genPresPenalty.value, gen_max_response_tokens: genMaxTokens.value,
      custom_css: customCss.value,
    })
    saveMessage.value = 'Saved'; setTimeout(() => { saveMessage.value = '' }, 2000)
  } catch (e) { error.value = (e as Error).message }
}

async function handleTestConnection() {
  await settings.save({ provider_source: providerSource.value, provider_base_url: providerBaseUrl.value, provider_api_key: providerApiKey.value, provider_model: providerModel.value })
  await settings.testConnection()
}
</script>

<template>
  <div class="max-w-[520px] mx-auto px-5 pt-8 pb-12">
    <p v-if="loading" class="text-muted text-sm text-center pt-12">Loading…</p>
    <template v-else>
      <div class="flex items-center justify-between mb-8">
        <h2 class="text-lg font-semibold">Settings</h2>
        <div class="flex items-center gap-2">
          <span v-if="saveMessage" class="text-[12px] text-muted">{{ saveMessage }}</span>
          <button class="px-5 py-1.5 text-[13px] font-medium bg-primary text-white rounded-full hover:bg-primary-strong transition-colors" @click="handleSave">Save</button>
        </div>
      </div>

      <!-- Provider -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">Provider</h3>
        <div class="space-y-4">
          <div>
            <label class="text-[13px] text-ink block mb-1.5">Source</label>
            <select :value="providerSource" class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50" @change="providerSource = ($event.target as HTMLSelectElement).value">
              <option v-for="src in providerSources" :key="src" :value="src">{{ sourceLabels[src] || src }}</option>
            </select>
          </div>
          <div><label class="text-[13px] text-ink block mb-1.5">Base URL</label><input :value="providerBaseUrl" type="text" class="w-full border border-line rounded-lg px-3 py-2 text-[14px] outline-none focus:border-primary/50 font-mono" @input="providerBaseUrl = ($event.target as HTMLInputElement).value" /></div>
          <div>
            <label class="text-[13px] text-ink block mb-1.5">API Key</label>
            <div class="relative">
              <input :value="providerApiKey" :type="showApiKey ? 'text' : 'password'" class="w-full border border-line rounded-lg px-3 py-2 pr-9 text-[14px] outline-none focus:border-primary/50 font-mono" @input="providerApiKey = ($event.target as HTMLInputElement).value" />
              <button class="absolute right-2.5 top-2.5 text-muted hover:text-ink" @click="showApiKey = !showApiKey"><Eye v-if="!showApiKey" :size="16" /><EyeOff v-else :size="16" /></button>
            </div>
          </div>
          <div>
            <label class="text-[13px] text-ink block mb-1.5">Model</label>
            <div class="flex gap-2">
              <input :value="providerModel" type="text" placeholder="gpt-4o" class="flex-1 border border-line rounded-lg px-3 py-2 text-[14px] outline-none focus:border-primary/50" @input="providerModel = ($event.target as HTMLInputElement).value" />
              <button class="shrink-0 px-3 py-2 text-[13px] border border-line rounded-lg hover:border-primary/50 transition-colors disabled:opacity-50 text-muted hover:text-ink" :disabled="settings.modelsLoading" @click="settings.fetchModels()">
                {{ settings.modelsLoading ? 'Fetching…' : 'Fetch models' }}
              </button>
            </div>
            <p v-if="settings.modelsError" class="text-[12px] text-coral mt-1">{{ settings.modelsError }}</p>
            <select v-if="settings.models.length > 0" class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50 mt-2" @change="providerModel = ($event.target as HTMLSelectElement).value">
              <option value="">— select model —</option>
              <option v-for="m in settings.models" :key="m" :value="m" :selected="m === providerModel">{{ m }}</option>
            </select>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[14px] text-ink">Stream responses</span>
            <ToggleSwitch :model-value="providerStream" @update:model-value="providerStream = $event" />
          </div>
          <button class="flex items-center gap-2 px-4 py-2 text-[13px] border border-line rounded-lg hover:border-primary/50 transition-colors disabled:opacity-50" :disabled="settings.testStatus === 'testing'" @click="handleTestConnection">
            <span v-if="settings.testStatus === 'testing'" class="w-3 h-3 rounded-full border-2 border-muted border-t-transparent animate-spin" />
            <span v-else-if="settings.testStatus === 'ok'" class="w-3 h-3 rounded-full bg-green-500" />
            <span v-else-if="settings.testStatus === 'fail'" class="w-3 h-3 rounded-full bg-coral" />
            {{ settings.testStatus === 'testing' ? 'Testing…' : 'Test connection' }}
          </button>
          <p v-if="settings.testError" class="text-[12px] text-coral">{{ settings.testError }}</p>
        </div>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Generation -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">Generation</h3>
        <SliderControl v-model="genTemp" label="Temperature" :min="0" :max="2" :step="0.01" />
        <SliderControl v-model="genTopP" label="Top P" :min="0" :max="1" :step="0.01" />
        <SliderControl v-model="genFreqPenalty" label="Frequency penalty" :min="-2" :max="2" :step="0.01" />
        <SliderControl v-model="genPresPenalty" label="Presence penalty" :min="-2" :max="2" :step="0.01" />
        <div class="flex items-center justify-between">
          <span class="text-[14px] text-ink">Max response tokens</span>
          <input
            :value="genMaxTokens"
            type="number"
            min="1"
            class="w-[88px] border border-line rounded-lg px-3 py-2 text-[14px] text-right tabular-nums outline-none focus:border-primary/50"
            @input="genMaxTokens = parseInt(($event.target as HTMLInputElement).value) || 0"
          />
        </div>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Appearance -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">Appearance</h3>
        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <span class="text-[14px] text-ink">Message style</span>
            <SegmentedControl
              :model-value="ui.messageStyle"
              :options="[{ value: 'bubble', label: 'Bubble' }, { value: 'flat', label: 'Flat' }]"
              @update:model-value="ui.setMessageStyle($event as 'bubble' | 'flat')"
            />
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[14px] text-ink">Theme</span>
            <SegmentedControl
              :model-value="ui.theme"
              :options="[{ value: 'light', label: 'Light' }, { value: 'dark', label: 'Dark' }, { value: 'system', label: 'System' }]"
              @update:model-value="ui.setTheme($event as 'light' | 'dark' | 'system')"
            />
          </div>
          <div>
            <div class="flex items-center justify-between mb-1.5"><label class="text-[13px] text-ink">Custom CSS</label><button class="text-[12px] text-muted hover:text-ink" @click="cssFullscreen = true">Fullscreen</button></div>
            <textarea :value="customCss" rows="6" class="w-full border border-line rounded-lg px-3.5 py-2.5 text-[13px] leading-relaxed font-mono bg-[#1e1e1e] text-[#d4d4d4] resize-y outline-none focus:border-primary/50" placeholder="/* custom CSS */" @input="customCss = ($event.target as HTMLTextAreaElement).value" />
          </div>
          <FullscreenEditor :model-value="customCss" :open="cssFullscreen" @close="cssFullscreen = false" @update:model-value="customCss = $event" />
        </div>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Regex -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">Regex</h3>
        <RegexRuleEditor v-for="rule in regexRules" :key="rule.id" :rule="{
          id: rule.id, name: rule.name,
          pattern: (rule.meta as any).pattern as string || '', replacement: (rule.meta as any).replacement as string || '',
          enabled: !!(rule.meta as any).enabled,
          scope: (rule.meta as any).scope as any || { ai_output: true, user_input: false, display_only: true },
        }"
          @update:enabled="(enabled: boolean) => { const meta = { ...rule.meta as any, enabled }; updateDefinition(rule.id, { meta }) }"
          @update:pattern="(p: string) => { (rule.meta as any).pattern = p }"
          @update:replacement="(r: string) => { (rule.meta as any).replacement = r }"
          @update:scope="(s: any) => { (rule.meta as any).scope = s }"
          @delete="async () => { await deleteDefinition(rule.id); regexRules = regexRules.filter(r => r.id !== rule.id) }" />
        <button class="w-full py-2 border-2 border-dashed border-line rounded-xl text-muted text-[13px] hover:text-primary hover:border-primary/30 transition-colors mt-2"
          @click="async () => { const created = await createDefinition({ type: 'regex_rule', name: 'New rule', content: '', meta: { pattern: '', replacement: '', enabled: true, name: 'New rule', scope: { ai_output: true, user_input: false, display_only: true } } }); regexRules = [...regexRules, created] }">+ Add rule</button>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Language -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">Language</h3>
        <select class="w-full border border-line rounded-lg px-3 py-2 text-[14px] bg-white outline-none focus:border-primary/50">
          <option value="en">English</option><option value="zh">中文</option>
        </select>
      </section>

      <div class="border-t border-line my-6" />

      <!-- About -->
      <section>
        <h3 class="text-[13px] font-semibold text-muted uppercase tracking-wide mb-4">About</h3>
        <div class="text-[14px] text-muted space-y-2">
          <p>Shirita — a SillyTavern alternative.</p>
          <p class="flex items-center gap-3"><button class="hover:text-ink underline underline-offset-2">Export all data</button><button class="hover:text-ink underline underline-offset-2">Import all data</button></p>
        </div>
      </section>

      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
