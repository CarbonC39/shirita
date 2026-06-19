<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { useSettingsStore } from "../stores/settings";
import { useUiStore } from "../stores/ui";
import {
    listDefinitions,
    createDefinition,
    updateDefinition,
    deleteDefinition,
    getRegexScopes,
} from "../api/client";
import type { Definition, RegexScope } from "../api/types";
import { metaToRule, scopeFlagsToMeta } from "../utils/regexRule";
import { providerKey, type ProviderField } from "../utils/providerKeys";
import { fallbackModels } from "../api/modelCatalog";
import SliderControl from "../components/SliderControl.vue";
import RegexRuleEditor from "../components/RegexRuleEditor.vue";
import AssetPicker from "../components/AssetPicker.vue";
import FullscreenEditor from "../components/FullscreenEditor.vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import SegmentedControl from "../components/SegmentedControl.vue";
import { Maximize2, Eye, EyeOff, Check, Languages } from "lucide-vue-next";
import { ensureNotifyPermission } from "../utils/notify";

const settings = useSettingsStore();
const ui = useUiStore();
const loading = ref(true);
const error = ref<string | null>(null);
const regexRules = ref<Definition[]>([]);
// Per-rule scope metadata (global vs template, source names, compile error),
// keyed by rule id; merged into the list for the compact management UI.
const regexScopes = ref<Record<string, RegexScope>>({});
const regexSearch = ref("");
const hideDisabled = ref(false);
const openRuleId = ref<string | null>(null);
const showApiKey = ref(false);
const cssFullscreen = ref(false);
// Auto-save: settings persist on change (debounced); the header shows transient
// status instead of a Save button. `loaded` gates the watch so loading the
// settings doesn't immediately echo them back.
const loaded = ref(false);
const saveState = ref<"idle" | "saving" | "saved">("idle");

const providerSources = [
    "openai",
    "anthropic",
    "google",
    "openrouter",
    "mistral",
    "deepseek",
    "groq",
    "xai",
    "cohere",
    "together",
    "perplexity",
    "ollama",
    "custom",
];

const sourceLabels: Record<string, string> = {
    openai: "OpenAI",
    anthropic: "Anthropic",
    google: "Google",
    openrouter: "OpenRouter",
    mistral: "Mistral",
    deepseek: "DeepSeek",
    groq: "Groq",
    xai: "xAI",
    cohere: "Cohere",
    together: "Together",
    perplexity: "Perplexity",
    ollama: "Ollama (local)",
    custom: "Custom…",
};

const defaultBaseUrls: Record<string, string> = {
    openai: "https://api.openai.com/v1",
    anthropic: "https://api.anthropic.com/v1",
    google: "https://generativelanguage.googleapis.com/v1beta",
    openrouter: "https://openrouter.ai/api/v1",
    mistral: "https://api.mistral.ai/v1",
    deepseek: "https://api.deepseek.com/v1",
    groq: "https://api.groq.com/openai/v1",
    xai: "https://api.x.ai/v1",
    cohere: "https://api.cohere.ai/v1",
    together: "https://api.together.xyz/v1",
    perplexity: "https://api.perplexity.ai",
    ollama: "http://localhost:11434/v1",
    custom: "",
};

// Local providers like Ollama don't check the API key at all; gating the
// live /models fetch on a non-empty key (needed for hosted providers, where
// an empty key would just 401) would otherwise leave Ollama users stuck on
// the static fallback catalog.
const apiKeyOptional = computed(() => providerSource.value === "ollama");

// Writable computed helpers
function get(k: string) {
    return settings.data[k] ?? undefined;
}
function set(k: string, v: unknown) {
    settings.data[k] = v;
}

// Provider config is per-source: each provider keeps its own base_url/api_key/
// model under `provider.<source>.<field>` so switching source never clobbers
// the others (mirrors the backend's resolve_provider_config).
const pget = (field: ProviderField) =>
    (get(providerKey(providerSource.value, field)) as string) || "";
const pset = (field: ProviderField, v: string) =>
    set(providerKey(providerSource.value, field), v);

const providerSource = computed({
    get: () => (get("provider_source") as string) || "openai",
    set: (v: string) => {
        set("provider_source", v);
        // Seed this source's base URL only if it has none saved yet.
        if (!get(providerKey(v, "base_url")))
            set(providerKey(v, "base_url"), defaultBaseUrls[v] || "");
    },
});
const providerBaseUrl = computed({
    get: () => pget("base_url"),
    set: (v: string) => pset("base_url", v),
});
const providerApiKey = computed({
    get: () => pget("api_key"),
    set: (v: string) => pset("api_key", v),
});
const providerModel = computed({
    get: () => pget("model"),
    set: (v: string) => pset("model", v),
});
const providerStream = computed({
    get: () => (get("provider_stream") as boolean) ?? true,
    set: (v: boolean) => set("provider_stream", v),
});
const genTemp = computed({
    get: () => (get("gen_temperature") as number) ?? 0.7,
    set: (v: number) => set("gen_temperature", v),
});
const genTopP = computed({
    get: () => (get("gen_top_p") as number) ?? 0.9,
    set: (v: number) => set("gen_top_p", v),
});
const genFreqPenalty = computed({
    get: () => (get("gen_frequency_penalty") as number) ?? 0,
    set: (v: number) => set("gen_frequency_penalty", v),
});
const genPresPenalty = computed({
    get: () => (get("gen_presence_penalty") as number) ?? 0,
    set: (v: number) => set("gen_presence_penalty", v),
});
// NB: the backend reads the response-token limit from `provider_max_tokens`
// (conversation.rs / summarize.rs), so write that key — not gen_max_response_tokens.
const genMaxTokens = computed({
    get: () => (get("provider_max_tokens") as number) ?? 4096,
    set: (v: number) => set("provider_max_tokens", v),
});
const customCss = computed({
    get: () => (get("custom_css") as string) || "",
    set: (v: string) => set("custom_css", v),
});

// Notifications opt-in
const notifyEnabled = computed({
    get: () => (get("notify_enabled") as boolean) ?? false,
    set: (v: boolean) => set("notify_enabled", v),
});

// Context / auto-summarize (keys consumed by summarize.rs). Threshold is stored
// as a 0..1 fraction but edited as a percentage.
const summarizeEnabled = computed({
    get: () => (get("summarize.enabled") as boolean) ?? true,
    set: (v: boolean) => set("summarize.enabled", v),
});
const contextWindow = computed({
    get: () => (get("context.window") as number) ?? 200000,
    set: (v: number) => set("context.window", v),
});
const contextThreshold = computed({
    get: () => Math.round(((get("context.threshold") as number) ?? 0.8) * 100),
    set: (v: number) => set("context.threshold", Math.min(100, Math.max(0, v)) / 100),
});
const keepRecent = computed({
    get: () => (get("context.keep_recent") as number) ?? 10,
    set: (v: number) => set("context.keep_recent", v),
});
const summarizeInstruction = computed({
    get: () => (get("summarize.instruction") as string) || "",
    set: (v: string) => set("summarize.instruction", v),
});

// The model is chosen from the dropdown only. If a saved value isn't in the
// fetched/fallback list, keep it selectable so it isn't silently dropped.
const modelOptions = computed(() => {
    const list = settings.models;
    const cur = providerModel.value;
    return cur && !list.includes(cur) ? [cur, ...list] : list;
});

// Model list: with an API key we fetch the provider's live /models (debounced);
// without one we fall back to a hardcoded per-source catalog.
let modelsTimer: ReturnType<typeof setTimeout> | undefined;
watch(
    () => [providerSource.value, providerBaseUrl.value, providerApiKey.value],
    () => {
        clearTimeout(modelsTimer);
        if (!(providerApiKey.value || apiKeyOptional.value) || !providerBaseUrl.value) {
            settings.useFallbackModels(
                fallbackModels[providerSource.value] ?? [],
            );
            return;
        }
        modelsTimer = setTimeout(async () => {
            // persist creds so the server's /models uses them, then fetch.
            await settings.save({
                provider_source: providerSource.value,
                [providerKey(providerSource.value, "base_url")]: providerBaseUrl.value,
                [providerKey(providerSource.value, "api_key")]: providerApiKey.value,
            });
            await settings.fetchModels();
        }, 800);
    },
);

// Ordered + filtered regex list for the management UI: global rules pinned on
// top, then by name; optional name search and hide-disabled filters.
const visibleRegexRules = computed(() => {
    const q = regexSearch.value.trim().toLowerCase();
    return [...regexRules.value]
        .filter((r) =>
            hideDisabled.value
                ? (r.meta as Record<string, unknown>).disabled !== true
                : true,
        )
        .filter((r) => !q || r.name.toLowerCase().includes(q))
        .sort((a, b) => {
            const ga = regexScopes.value[a.id]?.scope === "global" ? 0 : 1;
            const gb = regexScopes.value[b.id]?.scope === "global" ? 0 : 1;
            return ga - gb || a.name.localeCompare(b.name);
        });
});

// Persist a regex rule's name + meta, debounced so typing doesn't fire a
// request per keystroke. The whole rule object is the source of truth.
const ruleTimers = new Map<string, ReturnType<typeof setTimeout>>();
function persistRule(rule: Definition) {
    clearTimeout(ruleTimers.get(rule.id));
    ruleTimers.set(
        rule.id,
        setTimeout(() => {
            updateDefinition(rule.id, { name: rule.name, meta: rule.meta });
        }, 500),
    );
}

onMounted(async () => {
    try {
        await settings.load();
        // server is the source of truth for the background; sync the UI store cache
        const bg = settings.data.appearance_background;
        if (typeof bg === "string" && bg !== ui.background)
            ui.setBackground(bg);
        const cw = settings.data.appearance_content_width;
        if (typeof cw === "number" && cw !== ui.contentWidth)
            ui.setContentWidth(cw);
        // Legacy flat provider keys → active source's namespace (one-time mirror;
        // the backend does the same server-side on its first provider call).
        const flatMap: [string, ProviderField][] = [
            ["provider_base_url", "base_url"],
            ["provider_api_key", "api_key"],
            ["provider_model", "model"],
        ];
        const migration: Record<string, unknown> = {};
        for (const [flat, field] of flatMap) {
            const nsKey = providerKey(providerSource.value, field);
            if (settings.data[nsKey] == null && settings.data[flat] != null) {
                settings.data[nsKey] = settings.data[flat];
                migration[nsKey] = settings.data[flat];
            }
        }
        if (Object.keys(migration).length) await settings.save(migration);
        const allDefs = await listDefinitions();
        regexRules.value = allDefs.filter((d) => d.type === "regex_rule");
        const sc = await getRegexScopes();
        regexScopes.value = Object.fromEntries(sc.map((s) => [s.id, s]));
        // seed the model list: live fetch needs a key, otherwise show the catalog
        if ((providerApiKey.value || apiKeyOptional.value) && providerBaseUrl.value)
            await settings.fetchModels();
        else
            settings.useFallbackModels(
                fallbackModels[providerSource.value] ?? [],
            );
    } catch (e) {
        error.value = (e as Error).message;
    } finally {
        loading.value = false;
        loaded.value = true;
    }
});

// Debounced auto-save of every settings-backed field. The intermediate
// computeds dedupe, so saving (which merges the patch back into the store)
// produces no change and can't loop. Theme/style/background save on their own.
let saveTimer: ReturnType<typeof setTimeout> | undefined;
watch(
    () => [
        providerSource.value,
        providerBaseUrl.value,
        providerApiKey.value,
        providerModel.value,
        providerStream.value,
        genTemp.value,
        genTopP.value,
        genFreqPenalty.value,
        genPresPenalty.value,
        genMaxTokens.value,
        summarizeEnabled.value,
        contextWindow.value,
        contextThreshold.value,
        keepRecent.value,
        summarizeInstruction.value,
        notifyEnabled.value,
        customCss.value,
    ],
    () => {
        if (!loaded.value) return;
        clearTimeout(saveTimer);
        saveState.value = "saving";
        saveTimer = setTimeout(async () => {
            try {
                await settings.save({
                    provider_source: providerSource.value,
                    [providerKey(providerSource.value, "base_url")]: providerBaseUrl.value,
                    [providerKey(providerSource.value, "api_key")]: providerApiKey.value,
                    [providerKey(providerSource.value, "model")]: providerModel.value,
                    provider_stream: providerStream.value,
                    gen_temperature: genTemp.value,
                    gen_top_p: genTopP.value,
                    gen_frequency_penalty: genFreqPenalty.value,
                    gen_presence_penalty: genPresPenalty.value,
                    provider_max_tokens: genMaxTokens.value,
                    ...(summarizeEnabled.value !== undefined && { "summarize.enabled": summarizeEnabled.value }),
                    "context.window": contextWindow.value,
                    "context.threshold": contextThreshold.value / 100,
                    "context.keep_recent": keepRecent.value,
                    "summarize.instruction": summarizeInstruction.value,
                    ...(notifyEnabled.value !== undefined && { "notify_enabled": notifyEnabled.value }),
                    custom_css: customCss.value,
                });
                saveState.value = "saved";
                setTimeout(() => {
                    if (saveState.value === "saved") saveState.value = "idle";
                }, 1500);
            } catch (e) {
                error.value = (e as Error).message;
                saveState.value = "idle";
            }
        }, 600);
    },
);

function onBackgroundChange(path: string) {
    ui.setBackground(path);
    settings.save({ appearance_background: path }).catch((e) => {
        error.value = (e as Error).message;
    });
}

function onWidthChange(v: string) {
    const px = parseInt(v) || 760;
    ui.setContentWidth(px);
    settings.save({ appearance_content_width: px }).catch((e) => {
        error.value = (e as Error).message;
    });
}

async function handleNotifyToggle(enabled: boolean) {
    notifyEnabled.value = enabled;
    if (enabled && !(await ensureNotifyPermission())) {
        notifyEnabled.value = false;
    }
}

async function handleTestConnection() {
    await settings.save({
        provider_source: providerSource.value,
        provider_base_url: providerBaseUrl.value,
        provider_api_key: providerApiKey.value,
        provider_model: providerModel.value,
    });
    await settings.testConnection();
}
</script>

<template>
    <div class="max-w-[520px] mx-auto px-5 pt-8 pb-12">
        <p v-if="loading" class="text-muted text-sm text-center pt-12">
            {{ $t("common.loading") }}
        </p>
        <template v-else>
            <div class="flex items-center justify-between mb-8">
                <h2 class="text-lg font-semibold">{{ $t("settings.title") }}</h2>
                <span
                    class="flex items-center gap-1.5 text-[12px] text-muted transition-opacity"
                    :class="saveState === 'idle' ? 'opacity-0' : 'opacity-100'"
                >
                    <template v-if="saveState === 'saving'"
                        ><span
                            class="w-2.5 h-2.5 rounded-full border-2 border-muted border-t-transparent animate-spin"
                        />{{ $t("common.saving") }}</template
                    >
                    <template v-else-if="saveState === 'saved'"
                        ><Check
                            :size="13"
                            :stroke-width="2.6"
                            class="text-primary"
                        />{{ $t("common.saved") }}</template
                    >
                </span>
            </div>

            <!-- Provider -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.provider") }}
                </h3>
                <div class="space-y-4">
                    <div>
                        <label class="text-[13px] text-ink block mb-1.5"
                            >{{ $t("settings.source") }}</label
                        >
                        <select
                            :value="providerSource"
                            class="field w-full"
                            @change="
                                providerSource = (
                                    $event.target as HTMLSelectElement
                                ).value
                            "
                        >
                            <option
                                v-for="src in providerSources"
                                :key="src"
                                :value="src"
                            >
                                {{ sourceLabels[src] || src }}
                            </option>
                        </select>
                    </div>
                    <div>
                        <label class="text-[13px] text-ink block mb-1.5"
                            >{{ $t("settings.baseUrl") }}</label
                        ><input
                            :value="providerBaseUrl"
                            type="text"
                            class="field w-full font-mono"
                            @input="
                                providerBaseUrl = (
                                    $event.target as HTMLInputElement
                                ).value
                            "
                        />
                    </div>
                    <div>
                        <label class="text-[13px] text-ink block mb-1.5"
                            >{{ $t("settings.apiKey") }}
                            <span v-if="apiKeyOptional" class="text-muted"
                                >({{ $t("settings.apiKeyOptional") }})</span
                            ></label
                        >
                        <div class="relative">
                            <input
                                :value="providerApiKey"
                                :type="showApiKey ? 'text' : 'password'"
                                :placeholder="apiKeyOptional ? $t('settings.apiKeyOptional') : ''"
                                class="field w-full pr-9 font-mono"
                                @input="
                                    providerApiKey = (
                                        $event.target as HTMLInputElement
                                    ).value
                                "
                            />
                            <button
                                class="absolute right-2.5 top-2.5 text-muted hover:text-ink"
                                @click="showApiKey = !showApiKey"
                            >
                                <Eye v-if="!showApiKey" :size="16" /><EyeOff
                                    v-else
                                    :size="16"
                                />
                            </button>
                        </div>
                    </div>
                    <div>
                        <label class="text-[13px] text-ink block mb-1.5"
                            >{{ $t("settings.model") }}</label
                        >
                        <div class="flex items-center gap-2">
                            <select
                                v-if="modelOptions.length > 0"
                                :value="providerModel"
                                class="field flex-1"
                                @change="
                                    providerModel = (
                                        $event.target as HTMLSelectElement
                                    ).value
                                "
                            >
                                <option value="" disabled>
                                    {{ $t("settings.selectModel") }}
                                </option>
                                <option
                                    v-for="m in modelOptions"
                                    :key="m"
                                    :value="m"
                                >
                                    {{ m }}
                                </option>
                            </select>
                            <p
                                v-else-if="!settings.modelsLoading"
                                class="flex-1 text-[13px] text-muted/80"
                            >
                                {{ $t("settings.modelsHint") }}
                            </p>
                            <div v-else class="flex-1" />
                            <span
                                v-if="settings.modelsLoading"
                                class="flex items-center gap-1.5 text-[12px] text-muted whitespace-nowrap"
                                ><span
                                    class="w-2.5 h-2.5 rounded-full border-2 border-muted border-t-transparent animate-spin"
                                />{{ $t("settings.fetching") }}</span
                            >
                            <span
                                v-else-if="
                                    settings.models.length &&
                                    !settings.modelsError &&
                                    settings.modelsSource === 'live'
                                "
                                class="flex items-center gap-1 text-[12px] text-primary whitespace-nowrap"
                                :title="$t('settings.modelsLiveTitle')"
                                ><Check :size="13" :stroke-width="2.6" />{{
                                    $t("settings.modelsLive", settings.models.length)
                                }}</span
                            >
                            <span
                                v-else-if="
                                    settings.models.length &&
                                    !settings.modelsError
                                "
                                class="text-[12px] text-muted whitespace-nowrap"
                                :title="$t('settings.modelsCommonTitle')"
                                >{{ $t("settings.modelsCommon", { count: settings.models.length }) }}</span
                            >
                        </div>
                        <p
                            v-if="settings.modelsError"
                            class="text-[12px] text-coral mt-1"
                        >
                            {{ settings.modelsError }}
                        </p>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink"
                            >{{ $t("settings.streamResponses") }}</span
                        >
                        <ToggleSwitch
                            :model-value="providerStream"
                            @update:model-value="providerStream = $event"
                        />
                    </div>
                    <button
                        class="btn btn-ghost"
                        :disabled="settings.testStatus === 'testing'"
                        @click="handleTestConnection"
                    >
                        <span
                            v-if="settings.testStatus === 'testing'"
                            class="w-3 h-3 rounded-full border-2 border-muted border-t-transparent animate-spin"
                        />
                        <span
                            v-else-if="settings.testStatus === 'ok'"
                            class="w-3 h-3 rounded-full bg-green-500"
                        />
                        <span
                            v-else-if="settings.testStatus === 'fail'"
                            class="w-3 h-3 rounded-full bg-coral"
                        />
                        {{
                            settings.testStatus === "testing"
                                ? $t("settings.testing")
                                : $t("settings.testConnection")
                        }}
                    </button>
                    <p v-if="settings.testError" class="text-[12px] text-coral">
                        {{ settings.testError }}
                    </p>
                </div>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Generation -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.generation") }}
                </h3>
                <SliderControl
                    v-model="genTemp"
                    :label="$t('settings.temperature')"
                    :min="0"
                    :max="2"
                    :step="0.01"
                />
                <SliderControl
                    v-model="genTopP"
                    :label="$t('settings.topP')"
                    :min="0"
                    :max="1"
                    :step="0.01"
                />
                <SliderControl
                    v-model="genFreqPenalty"
                    :label="$t('settings.frequencyPenalty')"
                    :min="-2"
                    :max="2"
                    :step="0.01"
                />
                <SliderControl
                    v-model="genPresPenalty"
                    :label="$t('settings.presencePenalty')"
                    :min="-2"
                    :max="2"
                    :step="0.01"
                />
                <div class="flex items-center justify-between">
                    <span class="text-[14px] text-ink"
                        >{{ $t("settings.maxResponseTokens") }}</span
                    >
                    <input
                        :value="genMaxTokens"
                        type="number"
                        min="1"
                        class="field w-[88px] text-right tabular-nums"
                        @input="
                            genMaxTokens =
                                parseInt(
                                    ($event.target as HTMLInputElement).value,
                                ) || 0
                        "
                    />
                </div>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Context / auto-summarize -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.context") }}
                </h3>
                <div class="space-y-4">
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.autoSummarize") }}</span>
                        <ToggleSwitch
                            :model-value="summarizeEnabled"
                            @update:model-value="summarizeEnabled = $event"
                        />
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.contextWindow") }}</span>
                        <input
                            :value="contextWindow"
                            type="number"
                            min="1000"
                            step="1000"
                            class="field w-[120px] text-right tabular-nums"
                            @input="contextWindow = parseInt(($event.target as HTMLInputElement).value) || 0"
                        />
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.contextThreshold") }}</span>
                        <input
                            :value="contextThreshold"
                            type="number"
                            min="1"
                            max="100"
                            class="field w-[80px] text-right tabular-nums"
                            @input="contextThreshold = parseInt(($event.target as HTMLInputElement).value) || 0"
                        />
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.keepRecent") }}</span>
                        <input
                            :value="keepRecent"
                            type="number"
                            min="1"
                            class="field w-[80px] text-right tabular-nums"
                            @input="keepRecent = parseInt(($event.target as HTMLInputElement).value) || 1"
                        />
                    </div>
                    <div>
                        <label class="text-[13px] text-ink block mb-1.5"
                            >{{ $t("settings.summarizeInstruction") }}</label
                        >
                        <textarea
                            :value="summarizeInstruction"
                            rows="3"
                            class="field w-full text-[13px] leading-relaxed font-mono resize-y"
                            @input="summarizeInstruction = ($event.target as HTMLTextAreaElement).value"
                        />
                    </div>
                </div>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Appearance -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.appearance") }}
                </h3>
                <div class="space-y-4">
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.messageStyle") }}</span>
                        <SegmentedControl
                            :model-value="ui.messageStyle"
                            :options="[
                                { value: 'bubble', label: $t('settings.styleBubble') },
                                { value: 'flat', label: $t('settings.styleFlat') },
                            ]"
                            @update:model-value="
                                ui.setMessageStyle($event as 'bubble' | 'flat')
                            "
                        />
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.theme") }}</span>
                        <SegmentedControl
                            :model-value="ui.theme"
                            :options="[
                                { value: 'light', label: $t('settings.themeLight') },
                                { value: 'dark', label: $t('settings.themeDark') },
                                { value: 'system', label: $t('settings.themeSystem') },
                            ]"
                            @update:model-value="
                                ui.setTheme(
                                    $event as 'light' | 'dark' | 'system',
                                )
                            "
                        />
                    </div>
                    <div>
                        <span class="text-[14px] text-ink block mb-2"
                            >{{ $t("settings.background") }}</span
                        >
                        <div class="border border-line rounded-xl p-3 bg-card">
                            <AssetPicker
                                :model-value="ui.background"
                                shape="rect"
                                kind="background"
                                @update:model-value="onBackgroundChange"
                            />
                        </div>
                    </div>
                    <div class="flex items-center justify-between">
                        <span class="text-[14px] text-ink">{{ $t("settings.contentWidth") }}</span>
                        <input
                            :value="ui.contentWidth"
                            type="number"
                            min="560"
                            max="1100"
                            step="20"
                            class="field w-[88px] text-right tabular-nums"
                            @input="onWidthChange(($event.target as HTMLInputElement).value)"
                        />
                    </div>
                    <div>
                        <div
                            class="flex items-center justify-between mb-1.5 relative"
                        >
                            <label class="text-[13px] text-ink"
                                >{{ $t("settings.customCss") }}</label
                            ><button
                                data-test="fullscreen-btn"
                                class="absolute top-8 right-2 p-1 text-muted/70 hover:text-ink"
                                :title="$t('settings.fullscreen')"
                                @click="cssFullscreen = true"
                            >
                                <Maximize2 :size="15" />
                            </button>
                        </div>
                        <textarea
                            :value="customCss"
                            rows="6"
                            class="field w-full text-[13px] leading-relaxed font-mono resize-y"
                            placeholder="/* hooks: .app-chat-column .app-message[data-role] .app-composer [data-app=shell] */"
                            @input="
                                customCss = (
                                    $event.target as HTMLTextAreaElement
                                ).value
                            "
                        />
                    </div>
                    <FullscreenEditor
                        :model-value="customCss"
                        :open="cssFullscreen"
                        @close="cssFullscreen = false"
                        @update:model-value="customCss = $event"
                    />
                </div>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Notifications -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.notifications") }}
                </h3>
                <div class="flex items-center justify-between">
                    <span class="text-[14px] text-ink">{{ $t("settings.notifyReplies") }}</span>
                    <ToggleSwitch
                        :model-value="notifyEnabled"
                        @update:model-value="handleNotifyToggle($event)"
                    />
                </div>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Regex -->
            <section class="mb-8">
                <div class="flex items-center gap-3 mb-4">
                    <h3
                        class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide shrink-0"
                    >
                        {{ $t("settings.regex") }}
                    </h3>
                    <input
                        v-model="regexSearch"
                        type="search"
                        class="flex-1 min-w-0 border border-line rounded-md px-2 py-1 text-[12px] outline-none focus:border-primary/50"
                        :placeholder="$t('settings.regexSearch')"
                    />
                    <label
                        class="flex items-center gap-1.5 text-[12px] text-muted shrink-0"
                    >
                        <ToggleSwitch v-model="hideDisabled" />
                        {{ $t("settings.regexHideDisabled") }}
                    </label>
                </div>
                <RegexRuleEditor
                    v-for="rule in visibleRegexRules"
                    :key="rule.id"
                    :rule="metaToRule(rule)"
                    :scope="regexScopes[rule.id]?.scope ?? 'global'"
                    :source-names="regexScopes[rule.id]?.template_names ?? []"
                    :pattern-error="regexScopes[rule.id]?.pattern_error ?? null"
                    :open="openRuleId === rule.id"
                    @toggle-open="
                        openRuleId = openRuleId === rule.id ? null : rule.id
                    "
                    @update:enabled="
                        (enabled: boolean) => {
                            (rule.meta as any).disabled = !enabled;
                            persistRule(rule);
                        }
                    "
                    @update:name="
                        (n: string) => {
                            rule.name = n;
                            persistRule(rule);
                        }
                    "
                    @update:pattern="
                        (p: string) => {
                            (rule.meta as any).pattern = p;
                            persistRule(rule);
                        }
                    "
                    @update:replacement="
                        (r: string) => {
                            (rule.meta as any).replacement = r;
                            persistRule(rule);
                        }
                    "
                    @update:scope="
                        (s: any) => {
                            const m = scopeFlagsToMeta(s);
                            (rule.meta as any).scope = m.scope;
                            (rule.meta as any).targets = m.targets;
                            persistRule(rule);
                        }
                    "
                    @delete="
                        async () => {
                            await deleteDefinition(rule.id);
                            regexRules = regexRules.filter(
                                (r) => r.id !== rule.id,
                            );
                            delete regexScopes[rule.id];
                        }
                    "
                />
                <button
                    class="w-full py-2 border-2 border-dashed border-line rounded-xl text-muted text-[13px] hover:text-primary hover:border-primary/30 transition-colors mt-2"
                    @click="
                        async () => {
                            const created = await createDefinition({
                                type: 'regex_rule',
                                name: 'New rule',
                                content: '',
                                meta: {
                                    pattern: '',
                                    replacement: '',
                                    disabled: false,
                                    scope: 'display',
                                    targets: ['ai_output'],
                                },
                            });
                            regexRules = [...regexRules, created];
                            regexScopes[created.id] = {
                                id: created.id,
                                scope: 'global',
                                template_names: [],
                                pattern_error: null,
                            };
                            openRuleId = created.id;
                        }
                    "
                >
                    {{ $t("settings.addRule") }}
                </button>
            </section>

            <div class="border-t border-line my-6" />

            <!-- Language -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4 flex items-center gap-1.5"
                >
                    <Languages :size="14" />{{ $t("settings.language") }}
                </h3>
                <select
                    data-test="locale-switcher"
                    :value="ui.locale"
                    class="field w-full"
                    @change="
                        ui.setLocale(
                            ($event.target as HTMLSelectElement).value as
                                | 'en'
                                | 'zh-Hans'
                                | 'zh-Hant'
                                | 'ja',
                        )
                    "
                >
                    <option value="en">English</option>
                    <option value="zh-Hans">简体中文</option>
                    <option value="zh-Hant">繁體中文</option>
                    <option value="ja">日本語</option>
                </select>
            </section>

            <div class="border-t border-line my-6" />

            <!-- About -->
            <section>
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.about") }}
                </h3>
                <div class="text-[14px] text-muted space-y-2">
                    <p>{{ $t("settings.aboutText") }}</p>
                    <p class="flex items-center gap-3">
                        <button
                            class="hover:text-ink underline underline-offset-2"
                        >
                            {{ $t("settings.exportAll") }}</button
                        ><button
                            class="hover:text-ink underline underline-offset-2"
                        >
                            {{ $t("settings.importAll") }}
                        </button>
                    </p>
                </div>
            </section>

            <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
        </template>
    </div>
</template>
