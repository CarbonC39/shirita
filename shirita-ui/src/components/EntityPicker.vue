<script setup lang="ts">
import { ref, computed } from 'vue'
import { Search, ChevronDown, Plus } from 'lucide-vue-next'

const props = withDefaults(
  defineProps<{ items: { id: string; name: string }[]; placeholder?: string; createLabel?: string }>(),
  { placeholder: '', createLabel: 'New' },
)
const emit = defineEmits<{ select: [id: string]; create: [name: string] }>()

const query = ref('')
const open = ref(false)

const matches = computed(() => {
  const q = query.value.trim().toLowerCase()
  const list = q ? props.items.filter((i) => i.name.toLowerCase().includes(q)) : props.items
  return list.slice(0, 8)
})

function pick(id: string) { emit('select', id); open.value = false }
function create() { emit('create', query.value.trim()); query.value = ''; open.value = false }
</script>

<template>
  <div class="relative" @focusout="open = false">
    <div class="flex items-center gap-2.5 border border-line rounded-[10px] bg-card px-3 py-2.5 focus-within:border-primary/50">
      <Search :size="16" class="text-muted shrink-0" />
      <input
        v-model="query"
        type="text"
        data-test="entity-search"
        :placeholder="placeholder"
        class="flex-1 bg-transparent outline-none text-[14px] text-ink placeholder:text-muted/60"
        @focus="open = true"
      />
      <button class="text-muted shrink-0" tabindex="-1" @mousedown.prevent="open = !open"><ChevronDown :size="16" /></button>
    </div>
    <transition name="expand">
      <div v-if="open" class="absolute left-0 right-0 top-full mt-1 bg-card border border-line rounded-[10px] shadow-lg overflow-hidden z-20">
        <button
          data-test="entity-create"
          class="w-full flex items-center gap-2 text-left px-3 py-2 text-[13.5px] text-primary hover:bg-surface"
          @mousedown.prevent="create"
        >
          <Plus :size="15" class="shrink-0" />
          <span>{{ createLabel }}<template v-if="query.trim()"> “{{ query.trim() }}”</template></span>
        </button>
        <button
          v-for="i in matches"
          :key="i.id"
          data-test="entity-item"
          class="w-full flex items-center gap-2 px-3 py-2 text-left text-[13.5px] hover:bg-surface border-t border-line"
          @mousedown.prevent="pick(i.id)"
        >
          <span class="flex-1 truncate text-ink">{{ i.name }}</span>
        </button>
      </div>
    </transition>
  </div>
</template>
