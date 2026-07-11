<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import { Plus } from "lucide-vue-next";

const props = defineProps<{
  items: { id: string; name: string; avatar?: string | null }[];
  placeholder?: string;
  label?: string;
  createLabel?: string;
  disabled?: boolean;
  selectedLabel?: string;
}>();

const emit = defineEmits<{
  select: [id: string];
  /** Emitted when the user clicks "+ New X" — parent takes over creation flow. */
  'intent-create': [searchDraft: string];
}>();

const root = ref<HTMLElement | null>(null);
const triggerEl = ref<HTMLButtonElement | null>(null);
const open = ref(false);
const search = ref("");

const filter = computed(() =>
  props.items.filter((o) =>
    o.name.toLowerCase().includes(search.value.toLowerCase()),
  ),
);

function toggle() {
  if (props.disabled) return;
  open.value ? close() : openMenu();
}
function openMenu() {
  if (props.disabled || open.value) return;
  open.value = true;
  search.value = "";
}
function close() {
  if (!open.value) return;
  open.value = false;
  search.value = "";
  // Return focus to the trigger so keyboard users aren't dropped.
  triggerEl.value?.focus();
}

function pick(id: string) {
  emit("select", id);
  close();
}

function startCreate() {
  emit("intent-create", search.value);
  close();
}

// Close when clicking outside the picker (mousedown so it fires before the
// toggle's own click, avoiding a double-toggle race).
function onDocPointer(e: MouseEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) close();
}
// Escape closes from anywhere (e.g. while typing in the search field).
function onDocKey(e: KeyboardEvent) {
  if (open.value && e.key === "Escape") { e.stopPropagation(); close(); }
}
onMounted(() => {
  document.addEventListener("mousedown", onDocPointer);
  document.addEventListener("keydown", onDocKey);
});
onBeforeUnmount(() => {
  document.removeEventListener("mousedown", onDocPointer);
  document.removeEventListener("keydown", onDocKey);
});
</script>

<template>
  <div ref="root" class="relative" :class="{ 'pointer-events-none opacity-50': disabled }">
    <!-- With left-side label -->
    <button
      v-if="label"
      ref="triggerEl"
      class="flex items-center gap-2 text-[13px] text-ink w-full"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="toggle"
    >
      <span class="w-24 text-muted shrink-0 text-right">{{ label }}</span>
      <span
        class="flex-1 text-left px-3 py-2 bg-card border border-line rounded-lg"
        :class="selectedLabel ? 'text-ink' : 'text-muted'"
      >{{ selectedLabel || placeholder || "&nbsp;" }}</span>
    </button>
    <!-- Standalone toggle -->
    <button
      v-else
      ref="triggerEl"
      class="w-full text-left px-3 py-2 bg-card border border-line rounded-lg text-[14px]"
      :class="selectedLabel ? 'text-ink' : 'text-muted'"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="toggle"
    >{{ selectedLabel || placeholder || "&nbsp;" }}</button>

    <div
      v-if="open"
      role="listbox"
      class="absolute z-30 mt-1 rounded-xl border border-line bg-card shadow-xl min-w-[220px]"
    >
      <!-- Search input -->
      <input
        v-model="search"
        type="text"
        class="w-full bg-transparent px-3 py-2 text-[14px] text-ink placeholder:text-muted outline-none"
        :placeholder="placeholder || 'Search…'"
        autofocus
      />
      <!-- No results: show + New X -->
      <div v-if="filter.length === 0 && createLabel" class="border-t border-line">
        <button
          class="w-full flex items-center gap-1.5 px-3 py-1.5 text-[13px] text-muted hover:text-primary transition-colors"
          @click="startCreate"
        >
          <Plus :size="14" />
          <span>{{ createLabel }}</span>
        </button>
      </div>
      <!-- Results list -->
      <div v-else-if="filter.length > 0" class="max-h-48 overflow-y-auto border-t border-line">
        <button
          v-for="o in filter"
          :key="o.id"
          role="option"
          class="w-full flex items-center gap-2 px-3 py-1.5 text-[14px] text-ink hover:bg-card/50 transition-colors"
          @click="pick(o.id)"
        >
          <img
            v-if="o.avatar"
            :src="o.avatar"
            class="w-5 h-5 rounded-full object-cover"
            alt=""
          />
          <span>{{ o.name }}</span>
        </button>
        <!-- When typing doesn't match, also show create option -->
        <div v-if="search.trim() && createLabel" class="border-t border-line">
          <button
            class="w-full flex items-center gap-1.5 px-3 py-1.5 text-[13px] text-muted hover:text-primary transition-colors"
            @click="startCreate"
          >
            <Plus :size="14" />
            <span>{{ createLabel }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
