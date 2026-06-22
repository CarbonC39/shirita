<script lang="ts">
import { defineComponent, h, type VNode } from 'vue'
import { parseMarkdown, isHtmlDocument, type Inline } from '../utils/markdown'
import HtmlCardFrame from './HtmlCardFrame.vue'

// Render the Markdown AST to VNodes. Text becomes plain strings (Vue escapes
// them) and only a fixed whitelist of elements is produced — no v-html, no
// HTML strings, so there is nothing to sanitize and no XSS surface.
function renderInline(nodes: Inline[]): (VNode | string)[] {
  return nodes.map((n) => {
    switch (n.type) {
      case 'text':
        return n.value
      case 'strong':
        return h('strong', renderInline(n.children))
      case 'em':
        return h('em', renderInline(n.children))
      case 'del':
        return h('del', renderInline(n.children))
      case 'code':
        return h('code', { class: 'md-code' }, n.value)
      case 'link':
        return h('a', { href: n.href, target: '_blank', rel: 'noopener noreferrer', class: 'md-link' }, renderInline(n.children))
      default:
        return ''
    }
  })
}

export default defineComponent({
  name: 'MarkdownText',
  props: { text: { type: String, default: '' } },
  setup(props) {
    return () => {
      if (isHtmlDocument(props.text)) return h(HtmlCardFrame, { html: props.text })
      return h(
        'span',
        { class: 'md' },
        parseMarkdown(props.text).map((n) =>
          n.type === 'codeblock'
            ? n.lang === 'html' || isHtmlDocument(n.value)
              ? h(HtmlCardFrame, { html: n.value })
              : h('pre', { class: 'md-pre' }, h('code', n.value))
            : renderInline([n])[0],
        ),
      )
    }
  },
})
</script>
