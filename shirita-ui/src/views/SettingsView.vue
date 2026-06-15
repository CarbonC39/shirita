<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { useSettingsStore } from '../stores/settings'
import { useUiStore } from '../stores/ui'
import { listDefinitions, createDefinition, updateDefinition, deleteDefinition } from '../api/client'
import type { Definition } from '../api/types'
import { fallbackModels } from '../api/modelCatalog'
import SliderControl from '../components/SliderControl.vue'
import RegexRuleEditor from '../components/RegexRuleEditor.vue'
import BackgroundPicker from '../components/BackgroundPicker.vue'
import FullscreenEditor from '../components/FullscreenEditor.vue'
import ToggleSwitch from '../components/ToggleSwitch.vue'
import SegmentedControl from '../components/SegmentedControl.vue'
import { Eye, EyeOff, Check } from 'lucide-vue-next'

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

// Model list: with an API key we fetch the provider's live /models (debounced);
// without one we fall back to a hardcoded per-source catalog.
let modelsTimer: ReturnType<typeof setTimeout> | undefined
watch(
  () => [providerSource.value, providerBaseUrl.value, providerApiKey.value],
  () => {
    clearTimeout(modelsTimer)
    if (!providerApiKey.value || !providerBaseUrl.value) {
      settings.useFallbackModels(fallbackModels[providerSource.value] ?? [])
      return
    }
    modelsTimer = setTimeout(async () => {
      // persist creds so the server's /models uses them, then fetch.
      await settings.save({
        provider_source: providerSource.value,
        provider_base_url: providerBaseUrl.value,
        provider_api_key: providerApiKey.value,
      })
      await settings.fetchModels()
    }, 800)
  },
)

// Persist a regex rule's name + meta, debounced so typing doesn't fire a
// request per keystroke. The whole rule object is the source of truth.
const ruleTimers = new Map<string, ReturnType<typeof setTimeout>>()
function persistRule(rule: Definition) {
  clearTimeout(ruleTimers.get(rule.id))
  ruleTimers.set(rule.id, setTimeout(() => {
    updateDefinition(rule.id, { name: rule.name, meta: rule.meta })
  }, 500))
}

onMounted(async () => {
  try {
    await settings.load()
    // server is the source of truth for the background; sync the UI store cache
    const bg = settings.data.appearance_background
    if (typeof bg === 'string' && bg !== ui.background) ui.setBackground(bg)
    const allDefs = await listDefinitions()
    regexRules.value = allDefs.filter(d => d.type === 'regex_rule')
    // seed the model list: live fetch needs a key, otherwise show the catalog
    if (providerApiKey.value && providerBaseUrl.value) await settings.fetchModels()
    else settings.useFallbackModels(fallbackModels[providerSource.value] ?? [])
  } catch (e) { error.value = (e as Error).message }
  finally { loading.value = false }
})

function onBackgroundChange(path: string) {
  ui.setBackground(path)
  settings.save({ appearance_background: path }).catch((e) => { error.value = (e as Error).message })
}

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
          <button class="btn btn-primary px-5" @click="handleSave">Save</button>
        </div>
      </div>

      <!-- Provider -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">Provider</h3>
        <div class="space-y-4">
          <div>
            <label class="text-[13px] text-ink block mb-1.5">Source</label>
            <select :value="providerSource" class="field w-full" @change="providerSource = ($event.target as HTMLSelectElement).value">
              <option v-for="src in providerSources" :key="src" :value="src">{{ sourceLabels[src] || src }}</option>
            </select>
          </div>
          <div><label class="text-[13px] text-ink block mb-1.5">Base URL</label><input :value="providerBaseUrl" type="text" class="field w-full font-mono" @input="providerBaseUrl = ($event.target as HTMLInputElement).value" /></div>
          <div>
            <label class="text-[13px] text-ink block mb-1.5">API Key</label>
            <div class="relative">
              <input :value="providerApiKey" :type="showApiKey ? 'text' : 'password'" class="field w-full pr-9 font-mono" @input="providerApiKey = ($event.target as HTMLInputElement).value" />
              <button class="absolute right-2.5 top-2.5 text-muted hover:text-ink" @click="showApiKey = !showApiKey"><Eye v-if="!showApiKey" :size="16" /><EyeOff v-else :size="16" /></button>
            </div>
          </div>
          <div>
            <label class="text-[13px] text-ink block mb-1.5">Model</label>
            <div class="flex items-center gap-2">
              <input :value="providerModel" type="text" placeholder="gpt-4o" class="field flex-1" @input="providerModel = ($event.target as HTMLInputElement).value" />
              <span v-if="settings.modelsLoading" class="flex items-center gap-1.5 text-[12px] text-muted whitespace-nowrap"><span class="w-2.5 h-2.5 rounded-full border-2 border-muted border-t-transparent animate-spin" />Fetching…</span>
              <span v-else-if="settings.models.length && !settings.modelsError" class="flex items-center gap-1 text-[12px] text-primary whitespace-nowrap"><Check :size="13" :stroke-width="2.6" />{{ settings.models.length }} models</span>
            </div>
            <p v-if="settings.modelsError" class="text-[12px] text-coral mt-1">{{ settings.modelsError }}</p>
            <select v-if="settings.models.length > 0" :value="providerModel" class="field w-full mt-2" @change="providerModel = ($event.target as HTMLSelectElement).value">
              <option value="" disabled>— select model —</option>
              <option v-for="m in settings.models" :key="m" :value="m">{{ m }}</option>
            </select>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-[14px] text-ink">Stream responses</span>
            <ToggleSwitch :model-value="providerStream" @update:model-value="providerStream = $event" />
          </div>
          <button class="btn btn-ghost" :disabled="settings.testStatus === 'testing'" @click="handleTestConnection">
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
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">Generation</h3>
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
            class="field w-[88px] text-right tabular-nums"
            @input="genMaxTokens = parseInt(($event.target as HTMLInputElement).value) || 0"
          />
        </div>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Appearance -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">Appearance</h3>
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
          <div class="flex items-center justify-between">
            <span class="text-[14px] text-ink">Background</span>
            <BackgroundPicker :model-value="ui.background" @update:model-value="onBackgroundChange" />
          </div>
          <div>
            <div class="flex items-center justify-between mb-1.5"><label class="text-[13px] text-ink">Custom CSS</label><button class="text-[12px] text-muted hover:text-ink" @click="cssFullscreen = true">Fullscreen</button></div>
            <textarea :value="customCss" rows="6" class="field w-full text-[13px] leading-relaxed font-mono resize-y" placeholder="/* custom CSS */" @input="customCss = ($event.target as HTMLTextAreaElement).value" />
          </div>
          <FullscreenEditor :model-value="customCss" :open="cssFullscreen" @close="cssFullscreen = false" @update:model-value="customCss = $event" />
        </div>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Regex -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">Regex</h3>
        <RegexRuleEditor v-for="rule in regexRules" :key="rule.id" :rule="{
          id: rule.id, name: rule.name,
          pattern: (rule.meta as any).pattern as string || '', replacement: (rule.meta as any).replacement as string || '',
          enabled: !!(rule.meta as any).enabled,
          scope: (rule.meta as any).scope as any || { ai_output: true, user_input: false, display_only: true },
        }"
          @update:enabled="(enabled: boolean) => { (rule.meta as any).enabled = enabled; persistRule(rule) }"
          @update:name="(n: string) => { rule.name = n; (rule.meta as any).name = n; persistRule(rule) }"
          @update:pattern="(p: string) => { (rule.meta as any).pattern = p; persistRule(rule) }"
          @update:replacement="(r: string) => { (rule.meta as any).replacement = r; persistRule(rule) }"
          @update:scope="(s: any) => { (rule.meta as any).scope = s; persistRule(rule) }"
          @delete="async () => { await deleteDefinition(rule.id); regexRules = regexRules.filter(r => r.id !== rule.id) }" />
        <button class="w-full py-2 border-2 border-dashed border-line rounded-xl text-muted text-[13px] hover:text-primary hover:border-primary/30 transition-colors mt-2"
          @click="async () => { const created = await createDefinition({ type: 'regex_rule', name: 'New rule', content: '', meta: { pattern: '', replacement: '', enabled: true, name: 'New rule', scope: { ai_output: true, user_input: false, display_only: true } } }); regexRules = [...regexRules, created] }">+ Add rule</button>
      </section>

      <div class="border-t border-line my-6" />

      <!-- Language -->
      <section class="mb-8">
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">Language</h3>
        <select class="field w-full">
          <option value="en">English</option><option value="zh">中文</option>
        </select>
      </section>

      <div class="border-t border-line my-6" />

      <!-- About -->
      <section>
        <h3 class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4">About</h3>
        <div class="text-[14px] text-muted space-y-2">
          <p>Shirita — a SillyTavern alternative.</p>
          <p class="flex items-center gap-3"><button class="hover:text-ink underline underline-offset-2">Export all data</button><button class="hover:text-ink underline underline-offset-2">Import all data</button></p>
        </div>
      </section>

      <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
    </template>
  </div>
</template>
