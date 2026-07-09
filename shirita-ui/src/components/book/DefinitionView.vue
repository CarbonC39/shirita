<script setup lang="ts">
import { inject, watch, computed } from 'vue'
import DefinitionEditor from '../DefinitionEditor.vue'
import { LOCAL_BOOK_KEY, type Target } from './types'

const props = defineProps<{ definitionId: string }>()
const emit = defineEmits<{ drill: [target: Target] }>()

const book = inject(LOCAL_BOOK_KEY)!

const isOverridden = computed(() => Object.keys(book.localDefs.value).includes(props.definitionId))

watch(
  () => props.definitionId,
  (id) => { if (id) book.editLocal(id) },
  { immediate: true },
)

function onRevert() {
  book.revertLocal(props.definitionId)
}
</script>

<template>
  <div>
    <DefinitionEditor
      v-if="book.localDefActive.value"
      :definition="book.localEditDef"
      :types="book.types"
      :active="true"
      :hide-heading="true"
      :saved-tick="book.localSavedTick.value"
      @update:name="book.localEditDef.name = $event"
      @update:type="book.localEditDef.type = $event"
      @update:content="book.localEditDef.content = $event"
      @update:meta="book.localEditDef.meta = $event"
      @save="book.saveLocal"
    />
    <button
      v-if="isOverridden"
      data-test="def-revert"
      class="text-[12px] text-muted hover:text-coral mt-2"
      @click="onRevert"
    >
      {{ $t('book.revertToGlobal') }}
    </button>
  </div>
</template>
