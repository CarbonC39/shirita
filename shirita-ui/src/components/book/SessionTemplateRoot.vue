<script setup lang="ts">
import { inject } from 'vue'
import PromptTree from '../PromptTree.vue'
import { LOCAL_BOOK_KEY, type Target } from './types'

const emit = defineEmits<{ drill: [target: Target] }>()

// Local-book API provided by BookView (session-owner nodes + handlers).
const book = inject(LOCAL_BOOK_KEY)!

// Drill into a definition: push a definition target onto the navigator stack.
function onOpenDefinition(definitionId: string) {
  emit('drill', { kind: 'definition', definitionId })
}
</script>

<template>
  <div>
    <h3 class="text-[11px] font-semibold text-mauve uppercase tracking-wide border-l-2 border-mauve pl-2 mb-2">
      {{ $t('book.templateHeading') }}
    </h3>
    <PromptTree
      :nodes="book.templateNodes.value"
      :definitions="book.definitions"
      :types="book.types"
      @toggle-enabled="book.tree.toggleEnabled"
      @add-prompt="book.tree.addPrompt"
      @add-container="book.tree.addContainer"
      @add-ref-to-container="book.tree.addRefToContainer"
      @create-new-prompt="book.tree.createNewPrompt"
      @create-new-in-container="book.tree.createNewInContainer"
      @create-type="book.tree.createType"
      @update-content="book.tree.updateContent"
      @update-trigger="book.tree.updateTrigger"
      @update-node-meta="book.tree.updateNodeMeta"
      @update-def-meta="book.tree.updateDefMeta"
      @update-def-name="book.tree.updateDefName"
      @delete-node="book.tree.deleteNode"
      @reorder="book.tree.reorder"
      @open-definition="onOpenDefinition"
    />
  </div>
</template>
