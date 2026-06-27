<script setup lang="ts">
import { ref, reactive, computed, watch, onMounted, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Pencil, Upload, Download, Copy, Trash2, Star } from "lucide-vue-next";
import { useLibraryStore } from "../stores/library";
import { useUiStore } from "../stores/ui";
import { useMediaStore } from "../stores/media";
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
    downloadPackExport,
    exportDefinitionPath,
    exportTemplatePath,
    createPack,
    updatePack,
    deletePack,
    duplicatePack,
    getOrphanDefinitionsForPack,
} from "../api/client";
import type { PromptNode, Definition, Trigger, Session, VarDecl, OnConflict, ImportSummary } from "../api/types";
import { selectOneSiblingsToDisable } from "../utils/tree";
import PromptTree from "../components/PromptTree.vue";
import DefinitionEditor from "../components/DefinitionEditor.vue";
import VariablesEditor from "../components/VariablesEditor.vue";
import EntityPicker from "../components/EntityPicker.vue";
import PackEditor from "../components/PackEditor.vue";

// Aliased to `tr` because `t` is already used as a local binding throughout
// this file (template lookups, arrow params). Template uses the global `$t`.
const { t: tr } = useI18n();
const library = useLibraryStore();
const ui = useUiStore();
const media = useMediaStore();
const loading = ref(true);
const error = ref<string | null>(null);
const selectedTemplateId = ref<string | null>(null);
const nodes = ref<PromptNode[]>([]);
// A new template is composed as a local draft and only persisted on first
// manual Save — so picking "+ New template" never litters the list.
const templateName = ref("");
const renamingTemplate = ref(false);
const templateNameInput = ref<HTMLInputElement | null>(null);

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
        // Character-card import can write a new avatar Asset server-side
        // (bypassing media.upload), so the picker's cached list must be
        // dropped or the new avatar never shows up in the gallery.
        media.invalidate("avatar");
        // Jump straight to the newly imported pack/template instead of leaving
        // the editor on whatever was selected before — otherwise the import
        // looks like it did nothing until the user manually finds it in the
        // picker (the summary line alone doesn't make the new content visible).
        const newPack = importSummary.value.created.find((c) => c.kind === "pack");
        const newTemplate = importSummary.value.created.find((c) => c.kind === "template");
        if (newPack) {
            selectedPackId.value = newPack.id;
        } else if (newTemplate) {
            await selectTemplate(newTemplate.id);
        }
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

// ── variables: local (this chat) ────────────────────────────
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
async function localCreateNewInContainer(parentId: string | null, typeId: string) {
    const sid = ui.activeChatId;
    if (!sid) return;
    try {
        await ensureMaterialized();
        const isRx = typeId === "regex_rule";
        const def = await createDefinition({ type: typeId, name: isRx ? "New rule" : `New ${typeId}`, content: "", meta: isRx ? { pattern: "", replacement: "", disabled: false, scope: "display", targets: ["ai_output"] } : {} });
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
    const enabling = !node.enabled;
    try {
        await ensureMaterialized();
        await updateNode(nodeId, { enabled: enabling });
        if (enabling) {
            for (const sib of selectOneSiblingsToDisable(localNodes.value, nodeId)) {
                await updateNode(sib, { enabled: false });
            }
        }
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
            library.loadPacks(),
        ]);
        // Restore the last-edited template/pack/definition (Book remounts on
        // navigation). Fall back to the default template, then the first.
        const savedTemplate = localStorage.getItem("book.templateId");
        const defaultTemplate = library.templates.find((t) => (t.meta as Record<string, unknown>)?.default)?.id;
        const templateToSelect =
            (savedTemplate && library.templates.some((t) => t.id === savedTemplate) ? savedTemplate : null) ??
            defaultTemplate ??
            library.templates[0]?.id ??
            null;
        if (templateToSelect) await selectTemplate(templateToSelect);

        const savedPack = localStorage.getItem("book.packId");
        if (savedPack && library.packs.some((p) => p.id === savedPack)) selectedPackId.value = savedPack;

        const savedDef = localStorage.getItem("book.defId");
        if (savedDef && library.definitions.some((d) => d.id === savedDef)) selectDefinition(savedDef);
    } catch (e) {
        error.value = (e as Error).message;
    } finally {
        loading.value = false;
    }
});

// ── templates ──────────────────────────────────────────────
async function selectTemplate(id: string) {
    selectedTemplateId.value = id || null;
    try { localStorage.setItem("book.templateId", id || "") } catch { /* ignore */ }
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
async function createTemplateNamed(name: string) {
    try {
        const t = await createTemplate(name.trim() || "New template");
        await library.loadTemplates();
        await selectTemplate(t.id);
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
        templateName.value = "";
        nodes.value = [];
        await library.loadTemplates();
    } catch (e) {
        error.value = (e as Error).message;
    }
}

// The default template is auto-selected for new chats. At most one is default;
// setting a new one clears the previous. Stored on template.meta.default.
const isDefaultTemplate = computed(() => {
    const t = library.templates.find((x) => x.id === selectedTemplateId.value);
    return (t?.meta as Record<string, unknown> | undefined)?.default === true;
});
async function toggleDefaultTemplate() {
    const id = selectedTemplateId.value;
    if (!id) return;
    const turningOn = !isDefaultTemplate.value;
    try {
        for (const t of library.templates) {
            if ((t.meta as Record<string, unknown>)?.default && t.id !== id) {
                await updateTemplate(t.id, t.name, { ...t.meta, default: false });
            }
        }
        const cur = library.templates.find((t) => t.id === id);
        await updateTemplate(id, cur?.name ?? templateName.value, { ...(cur?.meta ?? {}), default: turningOn });
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
async function createNewInContainer(parentId: string | null, typeId: string) {
    if (!selectedTemplateId.value) return;
    try {
        const isRx = typeId === "regex_rule";
        const def = await createDefinition({
            type: typeId,
            name: isRx ? "New rule" : `New ${typeId}`,
            content: "",
            meta: isRx ? { pattern: "", replacement: "", disabled: false, scope: "display", targets: ["ai_output"] } : {},
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
    const enabling = !node.enabled;
    try {
        await updateNode(nodeId, { enabled: enabling });
        if (enabling) {
            for (const sib of selectOneSiblingsToDisable(nodes.value, nodeId)) {
                await updateNode(sib, { enabled: false });
            }
        }
        await reload();
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
// Persist a regex (or other brick) definition's meta/name edited inline in the
// tree — used by regex_rule refs carried by a template.
async function handleUpdateDefMeta(definitionId: string, meta: Record<string, unknown>) {
    try {
        await updateDefinition(definitionId, { meta });
        await library.loadDefinitions();
    } catch (e) { error.value = (e as Error).message; }
}
async function handleUpdateDefName(definitionId: string, name: string) {
    try {
        await updateDefinition(definitionId, { name });
        await library.loadDefinitions();
    } catch (e) { error.value = (e as Error).message; }
}

// ── packs ───────────────────────────────────────────────────
const selectedPackId = ref<string | null>(null);
const selectedPack = computed(() => library.packs.find((p) => p.id === selectedPackId.value) ?? null);
const renamingPack = ref(false);
const packNameDraft = ref("");

function selectPack(id: string) {
    selectedPackId.value = id || null;
    try { localStorage.setItem("book.packId", id || "") } catch { /* ignore */ }
}
async function createPackNamed(name: string) {
    try {
        const p = await createPack({ name: name?.trim() || "New pack" });
        await library.loadPacks();
        selectedPackId.value = p.id;
    } catch (e) { error.value = (e as Error).message; }
}
function startRenamePack() {
    if (!selectedPack.value) return;
    packNameDraft.value = selectedPack.value.name;
    renamingPack.value = true;
}
async function renamePack() {
    const p = selectedPack.value;
    const n = packNameDraft.value.trim();
    renamingPack.value = false;
    if (!p || !n || n === p.name) return;
    try {
        await updatePack(p.id, { name: n, identity: p.identity, meta: p.meta as Record<string, unknown> });
        await library.loadPacks();
    } catch (e) { error.value = (e as Error).message; }
}
async function dupPack() {
    if (!selectedPackId.value) return;
    try { const p = await duplicatePack(selectedPackId.value); await library.loadPacks(); selectedPackId.value = p.id; }
    catch (e) { error.value = (e as Error).message; }
}
async function delPack() {
    if (!selectedPackId.value) return;
    if (!confirm(tr("book.deletePackConfirm"))) return;
    try {
        const orphans = await getOrphanDefinitionsForPack(selectedPackId.value);
        const deleteOrphans =
            orphans.length > 0 && confirm(tr("book.deleteTemplateOrphans", orphans.length));
        await deletePack(selectedPackId.value, deleteOrphans);
        selectedPackId.value = null;
        await library.loadPacks();
    } catch (e) { error.value = (e as Error).message; }
}
async function exportSelectedPack() {
    if (!selectedPack.value) return;
    try {
        await downloadPackExport(selectedPack.value.id, selectedPack.value.name || "pack");
    } catch (e) { error.value = (e as Error).message; }
}

// ── definition editor ──────────────────────────────────────
function selectDefinition(id: string) {
    try { localStorage.setItem("book.defId", id || "") } catch { /* ignore */ }
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
    <div class="pt-6 pb-12">
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
                        @update-def-meta="handleUpdateDefMeta"
                        @update-def-name="handleUpdateDefName"
                        @delete-node="localDeleteNode"
                        @reorder="localReorder"
                    />
                    <div class="h-px bg-line my-5" />
                </template>

                <div data-test="local-variables" class="mb-4">
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
            <!-- TEMPLATE section (mauve accent) -->
            <div class="rounded-2xl bg-mauve/5 border border-line/60 p-4 mb-4">
            <h2 data-test="section-template" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-mauve border-l-2 border-mauve pl-2 mb-3">{{ $t('book.templateHeading') }}</h2>
            <!-- template picker / ops -->
            <div class="flex items-center gap-2 flex-wrap">
                <EntityPicker
                    class="flex-1 min-w-[180px]"
                    data-test="template-picker"
                    :items="library.templates.map((t) => ({ id: t.id, name: t.name }))"
                    :placeholder="$t('book.editTemplate')"
                    :create-label="$t('book.createTemplate')"
                    @select="selectTemplate"
                    @create="createTemplateNamed"
                />
                <div class="flex items-center flex-wrap">
                    <button
                        data-test="template-default"
                        class="w-[33px] h-[33px] grid place-items-center rounded-lg disabled:opacity-40"
                        :class="isDefaultTemplate ? 'text-amber-500' : 'text-muted hover:text-ink'"
                        :title="$t('book.defaultTemplate')"
                        :disabled="!selectedTemplateId"
                        @click="toggleDefaultTemplate"
                    >
                        <Star :size="15" :fill="isDefaultTemplate ? 'currentColor' : 'none'" />
                    </button>
                    <button
                        class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40"
                        :title="$t('common.rename')"
                        :disabled="!selectedTemplateId"
                        @click="renamingTemplate = true"
                    >
                        <Pencil :size="15" />
                    </button>
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
                        accept=".png,.json,.zip,application/json,image/png,application/zip"
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

            <p
                v-if="importSummary && importSummary.created.some((c) => c.kind === 'panel')"
                data-test="import-panel-hint"
                class="text-[12px] text-muted mt-1"
            >
                {{ $t("book.importPanelDetected") }}
            </p>

            <!-- only shown when the import actually hit a name+type conflict -->
            <div v-if="pendingImportFile" data-test="import-conflict-resolve" class="flex items-center gap-2 mt-1.5 text-[12px] text-muted">
                <span>{{ $t("book.importConflictFound", { skipped: importSummary?.skipped.length ?? 0 }) }}</span>
                <button class="text-primary hover:underline" :disabled="importBusy" @click="resolveImportConflicts('overwrite')">{{ $t("book.conflictOverwrite") }}</button>
                <button class="text-primary hover:underline" :disabled="importBusy" @click="resolveImportConflicts('duplicate')">{{ $t("book.conflictDuplicate") }}</button>
                <button class="text-muted hover:text-ink" :disabled="importBusy" @click="pendingImportFile = null">{{ $t("common.dismiss") }}</button>
            </div>

            <!-- rename inline: replaces content during rename -->
            <div v-if="renamingTemplate && selectedTemplateId" class="flex items-center gap-2 mt-2 mb-2">
                <input
                    ref="templateNameInput"
                    v-model="templateName"
                    type="text"
                    class="field flex-1"
                    :placeholder="$t('book.templateNamePlaceholder')"
                    @change="renameTemplate"
                    @keydown.enter="($event.target as HTMLInputElement).blur(); renamingTemplate = false"
                />
                <button class="text-muted hover:text-ink text-[12px] shrink-0" @click="renamingTemplate = false">{{ $t("common.done") }}</button>
            </div>

            <!-- state indicator inline in the toolbar -->
            <template v-if="selectedTemplateId && !renamingTemplate">
                <div class="flex items-center gap-2.5 mt-1.5 mb-1.5 justify-end">
                    <span class="text-[11.5px] text-muted tabular-nums"
                        >{{ $t("common.tokensEstimate", { tokens: formatTokens(templateTokens) }, templateTokens) }}</span
                    >
                    <span class="flex items-center gap-1.5 text-primary">
                        <Check :size="13" :stroke-width="2.4" />
                        <span class="text-[11.5px] text-muted">{{ $t("common.saved") }}</span>
                    </span>
                </div>
            </template>
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
                @update-def-meta="handleUpdateDefMeta"
                @update-def-name="handleUpdateDefName"
                @delete-node="handleDeleteNode"
                @reorder="handleReorder"
            />
            </div>

            <!-- PACK section (teal accent) -->
            <div class="rounded-2xl bg-primary/5 border border-line/60 p-4 mb-4">
            <h2 data-test="section-pack" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-primary border-l-2 border-primary pl-2 mb-3">{{ $t('book.packHeading') }}</h2>
            <div data-test="book-pack" class="mb-2">
                <div class="flex items-center gap-2 mb-3 flex-wrap">
                    <input
                        v-if="renamingPack"
                        v-model="packNameDraft"
                        type="text"
                        class="field flex-1 min-w-[180px]"
                        :placeholder="$t('book.packNamePlaceholder')"
                        @keydown.enter="renamePack"
                        @blur="renamePack"
                    />
                    <EntityPicker
                        v-else
                        class="flex-1 min-w-[180px]"
                        data-test="pack-picker"
                        :items="library.packs.map((p) => ({ id: p.id, name: p.name }))"
                        :placeholder="$t('book.editPack')"
                        :create-label="$t('book.createPack')"
                        @select="selectPack"
                        @create="createPackNamed"
                    />
                    <div class="flex items-center">
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.rename')" :disabled="!selectedPack" @click="startRenamePack"><Pencil :size="15" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.import')" data-test="pack-import" :disabled="importBusy" @click="importInput?.click()"><Upload :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('book.exportPackTitle')" data-test="pack-export" :disabled="!selectedPack" @click="exportSelectedPack"><Download :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-ink rounded-lg disabled:opacity-40" :title="$t('common.duplicate')" :disabled="!selectedPack" @click="dupPack"><Copy :size="16" /></button>
                        <button class="w-[33px] h-[33px] grid place-items-center text-muted hover:text-coral rounded-lg disabled:opacity-40" :title="$t('common.delete')" :disabled="!selectedPack" @click="delPack"><Trash2 :size="16" /></button>
                    </div>
                </div>
                <PackEditor v-if="selectedPack" :pack="selectedPack" @changed="library.loadPacks()" />
            </div>
            </div>

            <!-- DEFINITIONS section (neutral accent) -->
            <div class="rounded-2xl bg-ink/[0.03] border border-line/60 p-4">
            <h2 data-test="section-definitions" class="flex items-center text-[12px] font-semibold uppercase tracking-wide text-ink/55 border-l-2 border-muted/50 pl-2 mb-3">{{ $t('book.definitionsHeading') }}</h2>
            <DefinitionEditor
                hide-heading
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
            </div>

            <p v-if="error" class="text-coral text-sm mt-4">{{ error }}</p>
            </section>
        </template>
    </div>
</template>
