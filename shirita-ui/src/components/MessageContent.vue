<script setup lang="ts">
import { computed } from 'vue'
import { splitThinking } from '../utils/thinking'
import MarkdownText from './MarkdownText.vue'

// Render a message body: reasoning (<think>…</think>) folds into a collapsible
// block (auto-open while still streaming, collapsed once closed); everything
// else renders as Markdown.
const props = defineProps<{ text: string }>()
const segments = computed(() => splitThinking(props.text))
</script>

<template>
  <span class="msg-content"><template v-for="(seg, i) in segments" :key="i"><details
        v-if="seg.type === 'think'"
        class="md-think"
        :open="seg.open"
      ><summary class="md-think-summary">{{ $t('chat.thinking') }}</summary><MarkdownText :text="seg.content" /></details><MarkdownText
        v-else
        :text="seg.content"
      /></template></span>
</template>
