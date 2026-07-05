<script setup lang="ts">
import { ref, computed } from 'vue'
import type { Target } from './types'
import SessionTemplateRoot from './SessionTemplateRoot.vue'
import DefinitionView from './DefinitionView.vue'

const props = defineProps<{ rootTarget: Target }>()

// In-component navigation stack (NOT URL router — preserves chat-page state).
const stack = ref<Target[]>([props.rootTarget])
// Reactive transition direction: 'forward' on push, 'backward' on pop.
const transitionDirection = ref<'forward' | 'backward'>('forward')

function push(target: Target) {
  stack.value.push(target)
  transitionDirection.value = 'forward'
}
function pop() {
  if (stack.value.length > 1) {
    stack.value.pop()
    transitionDirection.value = 'backward'
  }
}

const current = computed<Target>(() => stack.value[stack.value.length - 1])
const canPop = computed(() => stack.value.length > 1)

// Breadcrumb label describes the level a "back" press returns to.
const backLabelKey = computed(() => {
  if (stack.value.length < 2) return ''
  const prev = stack.value[stack.value.length - 2]
  if (prev.kind === 'sessionRoot') return 'book.nav.backToTemplate'
  if (prev.kind === 'pack') return 'book.nav.backToPack'
  return 'book.nav.back'
})
</script>

<template>
  <div data-test="book-navigator" class="book-navigator">
    <button
      v-if="canPop"
      data-test="nav-back"
      class="flex items-center gap-1 text-[13px] text-muted hover:text-ink mb-3"
      @click="pop"
    >
      <span aria-hidden>‹</span> {{ $t(backLabelKey) }}
    </button>
    <Transition :name="transitionDirection" mode="out-in">
      <SessionTemplateRoot
        v-if="current.kind === 'sessionRoot'"
        :key="'root'"
        data-test="nav-level-sessionRoot"
        @drill="push"
      />
      <DefinitionView
        v-else-if="current.kind === 'definition'"
        :key="current.definitionId"
        :definition-id="current.definitionId"
        data-test="nav-level-definition"
        @drill="push"
      />
      <!-- pack level added in Phase 1B -->
    </Transition>
  </div>
</template>

<style scoped>
/* Drill-in: new level slides in from the right. */
.forward-enter-active, .forward-leave-active,
.backward-enter-active, .backward-leave-active {
  transition: transform 180ms ease, opacity 180ms ease;
}
.forward-enter-from { transform: translateX(24px); opacity: 0; }
.forward-leave-to   { transform: translateX(-24px); opacity: 0; }
.backward-enter-from { transform: translateX(-24px); opacity: 0; }
.backward-leave-to   { transform: translateX(24px); opacity: 0; }
</style>
