<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Upload, Download, Copy, Trash2 } from "lucide-vue-next";
import { useLibraryStore } from "../stores/library";
import { useUiStore } from "../stores/ui";
import { estimateTokens, formatTokens } from "../utils/tokens";
import {
    listNodes,
    createNode,
    updateNode,
    deleteNode,
    reorderNodes,
    updateDefinition,
    createDefinition,
    deleteDefinition,
    createTemplate,
    updateTemplate,
    duplicateTemplate,
    deleteTemplate,
    getOrphanDefinitions,
    getSession,
    setLocalDefinition,
    clearLocalDefinition,
    promoteLocalDefinition,
    materializeNodes,
    setLocalVariables,
    importFile,
    downloadExport,
    exportDefinitionPath,
    exportTemplatePath,
} from "../api/client";
import type { PromptNode, Definition, Trigger, Session, VarDecl, OnConflict, ImportSummary } from "../api/types";
import PromptTree from "../components/PromptTree.vue";
import DefinitionEditor from "../components/DefinitionEditor.vue";
import VariablesEditor from "../components/VariablesEditor.vue";

// Aliased to `tr` because `t` is already used as a local binding throughout
// this file (template lookups, arrow params). Template uses the global `$t`.
const { t: tr } = useI18n();
const library = useLibraryStore();
const ui = useUiStore();
const loading = ref(true);
const error = ref<string | null>(null);
const selectedTemplateId = ref<string | null>(null);
const nodes = ref<PromptNode[]>([]);
// A new template is composed as a local draft and only persisted on first
// manual Save — so picking "+ New template" never litters the list.
const isDraft = ref(false);
const templateName = ref("");

// Rough token total for the template: sum the content of every enabled ref
// node's definition. Display-only estimate (real assembly happens server-side).
const templateTokens = computed(() => {
    const byId = new Map(library.definitions.map((d) => [d.id, d]));
    return nodes.value.reduce((sum, n) => {
        if (n.kind !== "ref" || !n.enabled || !n.definition_id) return sum;
        return sum + estimateTokens(byId.get(n.definition_id)?.content ?? "");
    }, 0);
});

// ── M7 import / export ──────────────────────────────────────
// Imports default silently to "skip" (never destructive). The
// overwrite/duplicate choice only surfaces afterwards, and only if the
// import actually hit a name+type conflict — most imports never do, so
// there's no dropdown to puzzle over up front.
const importSummary = ref<ImportSummary | null>(null);
const importBusy = ref(false);
const importInput = ref<HTMLInputElement | null>(null);
const pendingImportFile = ref<File | null>(null);

async function runImport(file: File, onConflict: OnConflict) {
    importBusy.value = true;
    try {
        importSummary.value = await importFile(file, onConflict);
        pendingImportFile.value = importSummary.value.skipped.length > 0 ? file : null;
        await library.loadAll();
    } catch (err) {
        importSummary.value = null;
        pendingImportFile.value = null;
        error.value = (err as Error).message;
    } finally {
        importBusy.value = false;
    }
}

async function onImportPicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    await runImport(file, "skip");
    input.value = ""; // allow re-picking the same file
}

async function resolveImportConflicts(onConflict: OnConflict) {
    if (!pendingImportFile.value) return;
    await runImport(pendingImportFile.value, onConflict);
}

async function exportDefinition(d: Definition) {
    if (!d.id) return;
    await downloadExport(exportDefinitionPath(d.id), `${d.name || "definition"}.json`);
}

async function exportSelectedTemplate() {
    if (!selectedTemplateId.value) return;
    await downloadExport(exportTemplatePath(selectedTemplateId.value), `${templateName.value || "template"}.json`);
}

function blankDef(): Definition {
    return { id: "", type: "char", name: "", content: "", meta: {} };
}
const editDef = reactive<Definition>(blankDef());
// Whether the definition editor body is revealed — mirrors the template picker:
// the search/picker is always shown, the fields appear once one is chosen/new.
const defActive = ref(false);
function loadDef(d: Definition) {
    Object.assign(editDef, {
        id: d.id,
        type: d.type,
        name: d.name,
        content: d.content,
        meta: { ...d.meta },
    });
}

// ── local (this conversation) copy-on-write overrides ──────
const localSession = ref<Session | null>(null);
const localDefs = computed<Record<string, Record<string, unknown>>>(
    () =>
        (localSession.value?.override_config as Record<string, unknown>)
            ?.local_definitions as Record<string, Record<string, unknown>> ?? {},
);
async function loadLocal() {
    if (!ui.activeChatId) {
        localSession.value = null;
        return;
    }
    try {
        localSession.value = await getSession(ui.activeChatId);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
watch(() => ui.activeChatId, loadLocal, { immediate: true });

const localEditDef = reactive<Definition>(blankDef());
const localDefActive = ref(false);
function defName(defId: string): string {
    return library.definitions.find((d) => d.id === defId)?.name ?? defId;
}
// Load a global definition + its local patch into the local editor.
function editLocal(defId: string) {
    if (!defId) return;
    const base = library.definitions.find((d) => d.id === defId);
    if (!base) return;
    const patch = localDefs.value[defId] ?? {};
    Object.assign(localEditDef, {
        id: base.id,
        type: base.type,
        name: (patch.name as string) ?? base.name,
        content: (patch.content as string) ?? base.content,
        meta: {
            ...base.meta,
            ...(patch.trigger ? { trigger: patch.trigger } : {}),
            ...(patch.scan ? { scan: patch.scan } : {}),
        },
    });
    localDefActive.value = true;
}
// Save only the fields that differ from the global definition as a patch.
async function saveLocal() {
    if (!ui.activeChatId || !localEditDef.id) return;
    const base = library.definitions.find((d) => d.id === localEditDef.id);
    const patch: Record<string, unknown> = {};
    if (base && localEditDef.content !== base.content)
        patch.content = localEditDef.content;
    if (base && localEditDef.name !== base.name) patch.name = localEditDef.name;
    const t = (localEditDef.meta as Record<string, unknown>).trigger;
    if (t) patch.trigger = t;
    const s = (localEditDef.meta as Record<string, unknown>).scan;
    if (s) patch.scan = s;
    try {
        await setLocalDefinition(ui.activeChatId, localEditDef.id, patch);
        await loadLocal();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function promoteLocal(defId: string) {
    if (!ui.activeChatId) return;
    if (!confirm(tr("book.promoteConfirm"))) return;
    try {
        await promoteLocalDefinition(ui.activeChatId, defId);
        await Promise.all([library.loadDefinitions(), loadLocal()]);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function revertLocal(defId: string) {
    if (!ui.activeChatId) return;
    try {
        await clearLocalDefinition(ui.activeChatId, defId);
        if (localEditDef.id === defId) {
            Object.assign(localEditDef, blankDef());
            localDefActive.value = false;
        }
        await loadLocal();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// Merge a field patch into the local override for `defId` (keeps other fields).
async function setLocalPatch(defId: string, fields: Record<string, unknown>) {
    if (!ui.activeChatId) return;
    try {
        const existing = localDefs.value[defId] ?? {};
        await setLocalDefinition(ui.activeChatId, defId, { ...existing, ...fields });
        await loadLocal();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// ── variables: global (template meta) + local (this chat) ──
const templateVars = computed<VarDecl[]>(() => {
    const t = library.templates.find((x) => x.id === selectedTemplateId.value);
    return ((t?.meta as Record<string, unknown> | undefined)?.variables as VarDecl[]) ?? [];
});
async function saveTemplateVars(vars: VarDecl[]) {
    if (!selectedTemplateId.value) return;
    const t = library.templates.find((x) => x.id === selectedTemplateId.value);
    const meta = { ...((t?.meta as Record<string, unknown>) ?? {}), variables: vars };
    try {
        await updateTemplate(selectedTemplateId.value, templateName.value.trim() || "Template", meta);
        await library.loadTemplates();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

const localVars = computed<VarDecl[]>(
    () => ((localSession.value?.override_config as Record<string, unknown> | undefined)?.local_variables as VarDecl[]) ?? [],
);
async function saveLocalVars(vars: VarDecl[]) {
    if (!ui.activeChatId) return;
    try {
        await setLocalVariables(ui.activeChatId, vars);
        await loadLocal();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// ── local template tree (session-owned, copy-on-write) ─────
const localNodes = ref<PromptNode[]>([]);
async function loadLocalNodes() {
    if (!ui.activeChatId) {
        localNodes.value = [];
        return;
    }
    try {
        localNodes.value = await listNodes("session", ui.activeChatId);
    } catch {
        localNodes.value = [];
    }
}
watch(() => ui.activeChatId, loadLocalNodes, { immediate: true });

// First structural edit copies the template tree into the session (idempotent).
async function ensureMaterialized() {
    if (!ui.activeChatId) return;
    if (localNodes.value.length === 0) {
        await materializeNodes(ui.activeChatId);
        await loadLocalNodes();
    }
}

async function localAddPrompt(definitionId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        await createNode("session", sid, { parent_id: null, kind: "ref", definition_id: definitionId });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localAddContainer(typeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        await createNode("session", sid, { parent_id: null, kind: "folder", tag: typeId });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localAddRefToContainer(parentId: string, definitionId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        await createNode("session", sid, { parent_id: parentId, kind: "ref", definition_id: definitionId });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localCreateNewPrompt(name: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        const def = await createDefinition({ type: "prompt", name: name?.trim() || "New prompt", content: "", meta: {} });
        await library.loadDefinitions();
        await createNode("session", sid, { parent_id: null, kind: "ref", definition_id: def.id });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localCreateNewInContainer(parentId: string, typeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        const def = await createDefinition({ type: typeId, name: `New ${typeId}`, content: "", meta: {} });
        await library.loadDefinitions();
        await createNode("session", sid, { parent_id: parentId, kind: "ref", definition_id: def.id });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localCreateType(name: string) {
    if (!name.trim()) return;
    try {
        const created = await library.addType(slugifyType(name), name.trim());
        await localAddContainer(created.id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localToggleEnabled(nodeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    const node = localNodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    try {
        await ensureMaterialized();
        await updateNode(nodeId, { enabled: !node.enabled });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localUpdateNodeMeta(nodeId: string, meta: Record<string, unknown>) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        await updateNode(nodeId, { meta });
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localDeleteNode(nodeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    const node = localNodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    const childCount = localNodes.value.filter((n) => n.parent_id === nodeId).length;
    if (node.kind === "folder" && childCount > 0 && !confirm(tr("prompt.deleteContainerConfirm", childCount))) return;
    try {
        await ensureMaterialized();
        await deleteNode(nodeId);
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function localReorder(orderedIds: string[]) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        await reorderNodes("session", sid, orderedIds);
        await loadLocalNodes();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
// Inline content/trigger edits in the local tree write a local definition patch
// (not the global definition).
function localUpdateContent(definitionId: string, content: string) {
    void setLocalPatch(definitionId, { content });
}
function localUpdateTrigger(definitionId: string, trigger: Trigger) {
    void setLocalPatch(definitionId, { trigger });
}

onMounted(async () => {
    try {
        await Promise.all([
            library.loadTemplates(),
            library.loadDefinitions(),
            library.loadTypes(),
        ]);
    } catch (e) {
        error.value = (e as Error).message;
    } finally {
        loading.value = false;
    }
});

// ── templates ──────────────────────────────────────────────
async function selectTemplate(id: string) {
    if (id === "__new__") {
        startDraft();
        return;
    }
    isDraft.value = false;
    selectedTemplateId.value = id || null;
    templateName.value = library.templates.find((t) => t.id === id)?.name ?? "";
    if (id) {
        try {
            nodes.value = await listNodes("template", id);
        } catch {
            nodes.value = [];
        }
    } else {
        nodes.value = [];
    }
}
function startDraft() {
    isDraft.value = true;
    selectedTemplateId.value = null;
    templateName.value = "New template";
    nodes.value = [];
}
async function saveDraft() {
    try {
        const t = await createTemplate(
            templateName.value.trim() || "New template",
        );
        await library.loadTemplates();
        isDraft.value = false;
        selectedTemplateId.value = t.id;
        templateName.value = t.name;
        nodes.value = await listNodes("template", t.id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function renameTemplate() {
    if (!selectedTemplateId.value) return;
    const name = templateName.value.trim();
    const current = library.templates.find(
        (t) => t.id === selectedTemplateId.value,
    );
    if (!name || !current || name === current.name) {
        templateName.value = current?.name ?? name;
        return;
    }
    try {
        await updateTemplate(selectedTemplateId.value, name);
        await library.loadTemplates();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function dupTemplate() {
    if (!selectedTemplateId.value) return;
    try {
        const t = await duplicateTemplate(selectedTemplateId.value);
        await library.loadTemplates();
        await selectTemplate(t.id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function delTemplate() {
    if (!selectedTemplateId.value) return;
    if (!confirm(tr("book.deleteTemplateConfirm"))) return;
    try {
        const orphans = await getOrphanDefinitions(selectedTemplateId.value);
        const deleteOrphans =
            orphans.length > 0 && confirm(tr("book.deleteTemplateOrphans", orphans.length));
        await deleteTemplate(selectedTemplateId.value, deleteOrphans);
        selectedTemplateId.value = null;
        isDraft.value = false;
        templateName.value = "";
        nodes.value = [];
        await library.loadTemplates();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// ── tree ───────────────────────────────────────────────────
async function reload() {
    if (selectedTemplateId.value)
        nodes.value = await listNodes("template", selectedTemplateId.value);
}
async function addPrompt(definitionId: string) {
    if (!selectedTemplateId.value) return;
    try {
        await createNode("template", selectedTemplateId.value, {
            parent_id: null,
            kind: "ref",
            definition_id: definitionId,
        });
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function addContainer(typeId: string) {
    if (!selectedTemplateId.value) return;
    try {
        await createNode("template", selectedTemplateId.value, {
            parent_id: null,
            kind: "folder",
            tag: typeId,
        });
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function addRefToContainer(parentId: string, definitionId: string) {
    if (!selectedTemplateId.value) return;
    try {
        await createNode("template", selectedTemplateId.value, {
            parent_id: parentId,
            kind: "ref",
            definition_id: definitionId,
        });
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function createNewPrompt(name: string) {
    if (!selectedTemplateId.value) return;
    try {
        const def = await createDefinition({
            type: "prompt",
            name: name?.trim() || "New prompt",
            content: "",
            meta: {},
        });
        await library.loadDefinitions();
        await createNode("template", selectedTemplateId.value, {
            parent_id: null,
            kind: "ref",
            definition_id: def.id,
        });
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
function slugifyType(name: string) {
    const slug = name
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "");
    return slug || `type-${Date.now().toString(36)}`;
}
async function createType(name: string) {
    if (!selectedTemplateId.value || !name.trim()) return;
    try {
        const created = await library.addType(slugifyType(name), name.trim());
        await addContainer(created.id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function createTypeFromEditor(name: string) {
    if (!name.trim()) return;
    try {
        await library.addType(slugifyType(name), name.trim());
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function deleteTypeFromEditor(id: string) {
    const inUse = library.definitions.some((d) => d.type === id);
    const msg = inUse
        ? tr("book.deleteTypeInUse", { id })
        : tr("book.deleteType", { id });
    if (!confirm(msg)) return;
    try {
        await library.removeType(id);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function createNewInContainer(parentId: string, typeId: string) {
    if (!selectedTemplateId.value) return;
    try {
        const def = await createDefinition({
            type: typeId,
            name: `New ${typeId}`,
            content: "",
            meta: {},
        });
        await library.loadDefinitions();
        await createNode("template", selectedTemplateId.value, {
            parent_id: parentId,
            kind: "ref",
            definition_id: def.id,
        });
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleToggleEnabled(nodeId: string) {
    const node = nodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    try {
        const updated = await updateNode(nodeId, { enabled: !node.enabled });
        const i = nodes.value.findIndex((n) => n.id === nodeId);
        if (i !== -1)
            nodes.value = [
                ...nodes.value.slice(0, i),
                updated,
                ...nodes.value.slice(i + 1),
            ];
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleUpdateContent(definitionId: string, content: string) {
    try {
        await updateDefinition(definitionId, { content });
        await library.loadDefinitions();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleUpdateNodeMeta(nodeId: string, meta: Record<string, unknown>) {
    try {
        const updated = await updateNode(nodeId, { meta });
        const i = nodes.value.findIndex((n) => n.id === nodeId);
        if (i !== -1)
            nodes.value = [
                ...nodes.value.slice(0, i),
                updated,
                ...nodes.value.slice(i + 1),
            ];
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleDeleteNode(nodeId: string) {
    const node = nodes.value.find((n) => n.id === nodeId);
    if (!node) return;
    const childCount = nodes.value.filter((n) => n.parent_id === nodeId).length;
    if (
        node.kind === "folder" &&
        childCount > 0 &&
        !confirm(tr("prompt.deleteContainerConfirm", childCount))
    )
        return;
    try {
        await deleteNode(nodeId);
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleReorder(orderedIds: string[]) {
    if (!selectedTemplateId.value) return;
    try {
        await reorderNodes("template", selectedTemplateId.value, orderedIds);
        await reload();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function handleUpdateTrigger(definitionId: string, trigger: Trigger) {
    const def = library.definitions.find((d) => d.id === definitionId);
    if (!def) return;
    try {
        await updateDefinition(definitionId, {
            meta: { ...def.meta, trigger },
        });
        await library.loadDefinitions();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// ── definition editor ──────────────────────────────────────
function selectDefinition(id: string) {
    if (!id) {
        loadDef(blankDef());
        defActive.value = true;
        return;
    }
    const found = library.definitions.find((d) => d.id === id);
    if (found) {
        loadDef(found);
        defActive.value = true;
    }
}
async function saveDefinition() {
    try {
        if (editDef.id) {
            await updateDefinition(editDef.id, {
                type: editDef.type,
                name: editDef.name,
                content: editDef.content,
                meta: editDef.meta,
            });
        } else {
            const created = await createDefinition({
                type: editDef.type,
                name: editDef.name || "Untitled",
                content: editDef.content,
                meta: editDef.meta,
            });
            editDef.id = created.id;
        }
        await library.loadDefinitions();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function deleteDef() {
    if (!editDef.id) {
        loadDef(blankDef());
        defActive.value = false;
        return;
    }
    try {
        await deleteDefinition(editDef.id);
        loadDef(blankDef());
        defActive.value = false;
        await library.loadDefinitions();
    } catch (e) {
        error.value = (e as Error).message;
    }
}
async function duplicateDef() {
    try {
        const created = await createDefinition({
            type: editDef.type,
            name: `${editDef.name || "Untitled"} copy`,
            content: editDef.content,
            meta: editDef.meta,
        });
        await library.loadDefinitions();
        loadDef(created);
    } catch (e) {
        error.value = (e as Error).message;
    }
}
</script>

<template>
    <div class="max-w-[480px] mx-auto px-5 pt-6 pb-12">
        <p v-if="loading" class="text-muted text-sm text-center pt-12">
            {{ $t("common.loading") }}
        </p>
        <template v-else>
            <!-- this-conversation copy-on-write overrides, shown only while
                 you're inside a chat. Sits above the global library. -->
            <section v-if="ui.activeChatId" data-test="book-local" class="mb-6">
                <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2.5">
                    {{ $t("book.localHeading") }}
                </h3>
                <div
                    v-if="Object.keys(localDefs).length"
                    data-test="local-chips"
                    class="flex flex-wrap items-center gap-2 mb-3"
                >
                    <span class="text-[12px] text-muted">{{ $t("book.localChangedLabel") }}</span>
                    <span
                        v-for="(_patch, defId) in localDefs"
                        :key="defId"
                        class="inline-flex items-center gap-1 rounded-full border border-primary/30 bg-primary/10 px-2.5 py-1 text-[12px]"
                    >
                        <button class="text-ink" @click="editLocal(defId)">{{ defName(defId) }}</button>
                        <button class="text-muted hover:text-primary" :title="$t('book.syncToGlobal')" @click="promoteLocal(defId)">↥</button>
                        <button class="text-muted hover:text-coral" :title="$t('book.revertToGlobal')" @click="revertLocal(defId)">×</button>
                    </span>
                </div>
                <!-- local node tree: session-owned, materialized on first edit -->
                <template v-if="localSession && localSession.template_id">
                    <div
                        v-if="localNodes.length === 0"
                        class="text-[13px] text-muted py-3 flex items-center gap-2"
                    >
                        <span>{{ $t("book.followsTemplate") }}</span>
                        <button
                            data-test="customize-locally"
                            class="btn btn-primary !px-2.5 !py-1 text-[12px]"
                            @click="ensureMaterialized"
                        >
                            {{ $t("book.customizeLocally") }}
                        </button>
                    </div>
                    <PromptTree
                        v-else
                        :nodes="localNodes"
                        :definitions="library.definitions"
                        :types="library.containerTypes"
                        @toggle-enabled="localToggleEnabled"
                        @add-prompt="localAddPrompt"
                        @add-container="localAddContainer"
                        @add-ref-to-container="localAddRefToContainer"
                        @create-new-prompt="localCreateNewPrompt"
                        @create-new-in-container="localCreateNewInContainer"
                        @create-type="localCreateType"
                        @update-content="localUpdateContent"
                        @update-trigger="localUpdateTrigger"
                        @update-node-meta="localUpdateNodeMeta"
                        @delete-node="localDeleteNode"
                        @reorder="localReorder"
                    />
                    <div class="h-px bg-line my-5" />
                </template>

                <div class="mb-4">
                    <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">{{ $t("book.variablesThisChat") }}</h3>
                    <VariablesEditor :model-value="localVars" @update:model-value="saveLocalVars" />
                </div>

                <DefinitionEditor
                    :definition="localEditDef"
                    :all-definitions="library.definitions"
                    :types="library.containerTypes"
                    :active="localDefActive"
                    :header-actions="false"
                    @select-definition="editLocal"
                    @update:name="localEditDef.name = $event"
                    @update:type="localEditDef.type = $event as Definition['type']"
                    @update:content="localEditDef.content = $event"
                    @update:meta="localEditDef.meta = $event"
                    @save="saveLocal"
                />
            </section>
            <div v-if="ui.activeChatId" class="h-px bg-line my-6" />

            <section data-test="book-global">
            <!-- template picker + ops -->
            <div class="flex items-center gap-2">
                <select
                    :value="isDraft ? '__new__' : (selectedTemplateId ?? '')"
                    class="field flex-1"
                    @change="
                        selectTemplate(
                            ($event.target as HTMLSelectElement).value,
                        )
                    "
                >
                    <option value="" disabled>{{ $t("book.selectTemplate") }}</option>
                    <option value="__new__">{{ $t("book.newTemplate") }}</option>
                    <option
                        v-for="t in library.templates"
                        :key="t.id"
                        :value="t.id"
                    >
                        {{ t.name }}
                    </option>
                </select>
                <div class="flex items-center">
                    <button
                        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40"
                        :title="$t('book.importTitle')"
                        :disabled="importBusy"
                        @click="importInput?.click()"
                    >
                        <Upload :size="16" />
                    </button>
                    <input
                        ref="importInput"
                        type="file"
                        accept=".png,.json,application/json,image/png"
                        class="hidden"
                        @change="onImportPicked"
                    />
                    <button
                        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40"
                        :title="$t('book.exportTemplateTitle')"
                        :disabled="!selectedTemplateId"
                        @click="exportSelectedTemplate"
                    >
                        <Download :size="16" />
                    </button>
                    <button
                        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40"
                        :title="$t('common.duplicate')"
                        :disabled="!selectedTemplateId"
                        @click="dupTemplate"
                    >
                        <Copy :size="16" />
                    </button>
                    <button
                        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg disabled:opacity-40"
                        :title="$t('common.delete')"
                        :disabled="!selectedTemplateId"
                        @click="delTemplate"
                    >
                        <Trash2 :size="16" />
                    </button>
                </div>
            </div>

            <p v-if="importSummary" data-test="import-summary" class="text-[12px] text-muted mt-2">
                {{
                    $t("book.importSummary", {
                        created: importSummary.created.length,
                        skipped: importSummary.skipped.length,
                        overwritten: importSummary.overwritten.length,
                    })
                }}
            </p>

            <!-- only shown when the import actually hit a name+type conflict -->
            <div v-if="pendingImportFile" data-test="import-conflict-resolve" class="flex items-center gap-2 mt-1.5 text-[12px] text-muted">
                <span>{{ $t("book.importConflictFound", { skipped: importSummary?.skipped.length ?? 0 }) }}</span>
                <button class="text-primary hover:underline" :disabled="importBusy" @click="resolveImportConflicts('overwrite')">{{ $t("book.conflictOverwrite") }}</button>
                <button class="text-primary hover:underline" :disabled="importBusy" @click="resolveImportConflicts('duplicate')">{{ $t("book.conflictDuplicate") }}</button>
                <button class="text-muted hover:text-ink" :disabled="importBusy" @click="pendingImportFile = null">{{ $t("common.dismiss") }}</button>
            </div>

            <!-- name + save/saved state -->
            <div
                v-if="isDraft || selectedTemplateId"
                class="flex items-center gap-2 mt-2 mb-3.5"
            >
                <input
                    v-model="templateName"
                    type="text"
                    class="field flex-1"
                    :placeholder="$t('book.templateNamePlaceholder')"
                    @change="renameTemplate"
                    @keydown.enter="($event.target as HTMLInputElement).blur()"
                />
                <button
                    v-if="isDraft"
                    class="btn btn-primary shrink-0"
                    @click="saveDraft"
                >
                    {{ $t("common.save") }}
                </button>
                <span v-else class="flex items-center gap-2.5 shrink-0">
                    <span class="text-[11.5px] text-muted tabular-nums"
                        >{{ $t("common.tokensEstimate", { tokens: formatTokens(templateTokens) }, templateTokens) }}</span
                    >
                    <span class="flex items-center gap-1.5 text-primary">
                        <Check :size="13" :stroke-width="2.4" />
                        <span class="text-[11.5px] text-muted">{{ $t("common.saved") }}</span>
                    </span>
                </span>
            </div>

            <p v-if="isDraft" class="text-muted text-[13px] py-4">
                {{ $t("book.draftHint") }}
            </p>
            <PromptTree
                v-if="selectedTemplateId"
                :nodes="nodes"
                :definitions="library.definitions"
                :types="library.containerTypes"
                @toggle-enabled="handleToggleEnabled"
                @add-prompt="addPrompt"
                @add-container="addContainer"
                @add-ref-to-container="addRefToContainer"
                @create-new-prompt="createNewPrompt"
                @create-new-in-container="createNewInContainer"
                @create-type="createType"
                @update-content="handleUpdateContent"
                @update-trigger="handleUpdateTrigger"
                @update-node-meta="handleUpdateNodeMeta"
                @delete-node="handleDeleteNode"
                @reorder="handleReorder"
            />
            <div v-if="selectedTemplateId" class="mt-4">
                <h3 class="text-[11px] font-semibold text-ink/65 uppercase tracking-[0.06em] mb-2">{{ $t("book.variables") }}</h3>
                <VariablesEditor :model-value="templateVars" @update:model-value="saveTemplateVars" />
            </div>
            <div class="h-px bg-line my-6" />

            <DefinitionEditor
                :definition="editDef"
                :all-definitions="library.definitions"
                :types="library.containerTypes"
                :active="defActive"
                @select-definition="selectDefinition"
                @update:name="editDef.name = $event"
                @update:type="editDef.type = $event as Definition['type']"
                @update:content="editDef.content = $event"
                @update:meta="editDef.meta = $event"
                @save="saveDefinition"
                @delete="deleteDef"
                @duplicate="duplicateDef"
                @import="importInput?.click()"
                @export="exportDefinition(editDef)"
                @create-type="createTypeFromEditor"
                @delete-type="deleteTypeFromEditor"
            />

            <button
                v-if="defActive && editDef.id"
                data-test="export-def"
                class="inline-flex items-center gap-1 mt-2 text-[12px] text-muted hover:text-ink"
                @click="exportDefinition(editDef)"
            >
                <Download :size="14" /> {{ $t("book.exportDefinition") }}
            </button>

            <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
            </section>
        </template>
    </div>
</template>
