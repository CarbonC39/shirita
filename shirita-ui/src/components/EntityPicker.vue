<script setup lang="ts">
import { ref, computed } from "vue";
import { Plus, X, Check } from "lucide-vue-next";

const props = defineProps<{
  items: { id: string; name: string; avatar?: string | null }[];
  placeholder?: string;
  label?: string;
  createLabel?: string;
  disabled?: boolean;
  /** Show this text in the toggle button instead of the placeholder (e.g. selected template). */
  selectedLabel?: string;
}>();

const emit = defineEmits<{
  select: [id: string];
  /** Emitted with the entered name when the user confirms creation. */
  create: [name: string];
}>();

const open = ref(false);
const search = ref("");
const creating = ref(false);
const newName = ref("");
const filter = computed(() =>
  props.items.filter((o) =>
    o.name.toLowerCase().includes(search.value.toLowerCase()),
  ),
);

function toggle() {
  if (props.disabled) return;
  open.value = !open.value;
  creating.value = false;
  if (open.value) search.value = "";
}

function pick(id: string) {
  emit("select", id);
  open.value = false;
  search.value = "";
}

function enterCreating() {
  newName.value = search.value;
  creating.value = true;
}

function confirmCreate() {
  const name = newName.value.trim();
  if (!name) return;
  emit("create", name);
  creating.value = false;
  open.value = false;
  search.value = "";
  newName.value = "";
}

function cancelCreate() {
  creating.value = false;
  newName.value = "";
}

function close() {
  open.value = false;
  search.value = "";
  creating.value = false;
}
</script>

<template>
  <div class="relative" :class="{ 'pointer-events-none opacity-50': disabled }">
    <!-- With left-side label (used in BookView) -->
    <button
      v-if="label"
      class="flex items-center gap-2 text-[13px] text-ink w-full"
      @click="toggle"
    >
      <span class="w-24 text-muted shrink-0 text-right">{{ label }}</span>
      <span
        class="flex-1 text-left px-3 py-2 bg-card border border-line rounded-lg"
        :class="selectedLabel ? 'text-ink' : 'text-muted'"
      >{{ selectedLabel || placeholder || "&nbsp;" }}</span>
    </button>
    <!-- Standalone toggle (used in NewChatView) -->
    <button
      v-else
      class="w-full text-left px-3 py-2 bg-card border border-line rounded-lg text-[14px]"
      :class="selectedLabel ? 'text-ink' : 'text-muted'"
      @click="toggle"
    >{{ selectedLabel || placeholder || "&nbsp;" }}</button>
    <div
      v-if="open"
      class="absolute z-30 mt-1 rounded-xl border border-line bg-card shadow-xl min-w-[220px]"
    >
      <!-- Creating mode: name input + X / ✓ -->
      <div v-if="creating" class="flex items-center gap-1 px-2 py-1">
        <input
          v-model="newName"
          type="text"
          class="flex-1 bg-transparent px-2 py-1.5 text-[14px] text-ink placeholder:text-muted/60 outline-none"
          :placeholder="createLabel"
          autofocus
          @keydown.enter="confirmCreate"
          @keydown.escape="cancelCreate"
        />
        <button
          class="p-1.5 text-muted hover:text-ink transition-colors"
          title="Cancel"
          @click="cancelCreate"
        >
          <X :size="16" />
        </button>
        <button
          class="p-1.5 text-primary hover:text-emerald-400 transition-colors"
          title="Confirm"
          :class="{ 'opacity-30 pointer-events-none': !newName.trim() }"
          @click="confirmCreate"
        >
          <Check :size="16" />
        </button>
      </div>

      <!-- Search / select mode -->
      <template v-else>
        <input
          v-model="search"
          type="text"
          class="w-full bg-transparent px-3 py-2 text-[14px] text-ink placeholder:text-muted/70 outline-none"
          :placeholder="placeholder || 'Search…'"
          autofocus
        />
        <div v-if="filter.length === 0 && createLabel" class="border-t border-line">
          <button
            class="w-full flex items-center gap-1.5 px-3 py-1.5 text-[13px] text-muted hover:text-primary transition-colors"
            @click="enterCreating"
          >
            <Plus :size="14" />
            <span>{{ createLabel }}</span>
          </button>
        </div>
        <div v-else-if="filter.length > 0" class="max-h-48 overflow-y-auto border-t border-line">
          <button
            v-for="o in filter"
            :key="o.id"
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
          <!-- When typing doesn't match any existing option, also show create option -->
          <div v-if="search.trim() && createLabel" class="border-t border-line">
            <button
              class="w-full flex items-center gap-1.5 px-3 py-1.5 text-[13px] text-muted hover:text-primary transition-colors"
              @click="enterCreating"
            >
              <Plus :size="14" />
              <span>{{ createLabel }}</span>
            </button>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
