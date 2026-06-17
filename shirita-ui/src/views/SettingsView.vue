<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { useSettingsStore } from "../stores/settings";
import { useUiStore } from "../stores/ui";
import {
    listDefinitions,
    createDefinition,
    updateDefinition,
    deleteDefinition,
} from "../api/client";
import type { Definition } from "../api/types";
import { fallbackModels } from "../api/modelCatalog";
import SliderControl from "../components/SliderControl.vue";
import RegexRuleEditor from "../components/RegexRuleEditor.vue";
import AssetPicker from "../components/AssetPicker.vue";
import FullscreenEditor from "../components/FullscreenEditor.vue";
import ToggleSwitch from "../components/ToggleSwitch.vue";
import SegmentedControl from "../components/SegmentedControl.vue";
import { Maximize2, Eye, EyeOff, Check, Languages } from "lucide-vue-next";

const settings = useSettingsStore();
const ui = useUiStore();
const loading = ref(true);
const error = ref<string | null>(null);
const regexRules = ref<Definition[]>([]);
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
    custom: "",
};

// Writable computed helpers
function get(k: string) {
    return settings.data[k] ?? undefined;
}
function set(k: string, v: unknown) {
    settings.data[k] = v;
}

const providerSource = computed({
    get: () => (get("provider_source") as string) || "openai",
    set: (v: string) => {
        set("provider_source", v);
        set("provider_base_url", defaultBaseUrls[v] || "");
    },
});
const providerBaseUrl = computed({
    get: () => (get("provider_base_url") as string) || "",
    set: (v: string) => set("provider_base_url", v),
});
const providerApiKey = computed({
    get: () => (get("provider_api_key") as string) || "",
    set: (v: string) => set("provider_api_key", v),
});
const providerModel = computed({
    get: () => (get("provider_model") as string) || "",
    set: (v: string) => set("provider_model", v),
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
const genMaxTokens = computed({
    get: () => (get("gen_max_response_tokens") as number) ?? 4096,
    set: (v: number) => set("gen_max_response_tokens", v),
});
const customCss = computed({
    get: () => (get("custom_css") as string) || "",
    set: (v: string) => set("custom_css", v),
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
        if (!providerApiKey.value || !providerBaseUrl.value) {
            settings.useFallbackModels(
                fallbackModels[providerSource.value] ?? [],
            );
            return;
        }
        modelsTimer = setTimeout(async () => {
            // persist creds so the server's /models uses them, then fetch.
            await settings.save({
                provider_source: providerSource.value,
                provider_base_url: providerBaseUrl.value,
                provider_api_key: providerApiKey.value,
            });
            await settings.fetchModels();
        }, 800);
    },
);

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
        const allDefs = await listDefinitions();
        regexRules.value = allDefs.filter((d) => d.type === "regex_rule");
        // seed the model list: live fetch needs a key, otherwise show the catalog
        if (providerApiKey.value && providerBaseUrl.value)
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
                    provider_base_url: providerBaseUrl.value,
                    provider_api_key: providerApiKey.value,
                    provider_model: providerModel.value,
                    provider_stream: providerStream.value,
                    gen_temperature: genTemp.value,
                    gen_top_p: genTopP.value,
                    gen_frequency_penalty: genFreqPenalty.value,
                    gen_presence_penalty: genPresPenalty.value,
                    gen_max_response_tokens: genMaxTokens.value,
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
                            >{{ $t("settings.apiKey") }}</label
                        >
                        <div class="relative">
                            <input
                                :value="providerApiKey"
                                :type="showApiKey ? 'text' : 'password'"
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
                                @update:model-value="onBackgroundChange"
                            />
                        </div>
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
                            placeholder="/* custom CSS */"
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

            <!-- Regex -->
            <section class="mb-8">
                <h3
                    class="text-[13px] font-semibold text-ink/65 uppercase tracking-wide mb-4"
                >
                    {{ $t("settings.regex") }}
                </h3>
                <RegexRuleEditor
                    v-for="rule in regexRules"
                    :key="rule.id"
                    :rule="{
                        id: rule.id,
                        name: rule.name,
                        pattern: ((rule.meta as any).pattern as string) || '',
                        replacement:
                            ((rule.meta as any).replacement as string) || '',
                        enabled: !!(rule.meta as any).enabled,
                        scope: ((rule.meta as any).scope as any) || {
                            ai_output: true,
                            user_input: false,
                            display_only: true,
                        },
                    }"
                    @update:enabled="
                        (enabled: boolean) => {
                            (rule.meta as any).enabled = enabled;
                            persistRule(rule);
                        }
                    "
                    @update:name="
                        (n: string) => {
                            rule.name = n;
                            (rule.meta as any).name = n;
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
                            (rule.meta as any).scope = s;
                            persistRule(rule);
                        }
                    "
                    @delete="
                        async () => {
                            await deleteDefinition(rule.id);
                            regexRules = regexRules.filter(
                                (r) => r.id !== rule.id,
                            );
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
                                    enabled: true,
                                    name: 'New rule',
                                    scope: {
                                        ai_output: true,
                                        user_input: false,
                                        display_only: true,
                                    },
                                },
                            });
                            regexRules = [...regexRules, created];
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
