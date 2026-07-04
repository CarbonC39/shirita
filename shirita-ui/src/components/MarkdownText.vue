<script lang="ts">
import { defineComponent, h, type VNode } from 'vue'
import { parseMarkdown, isHtmlDocument, type Inline, type Block, type MdNode, type ListItem } from '../utils/markdown'
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

// Render a single list item (handles task-list checkboxes).
function renderListItem(item: ListItem, ordered: boolean): VNode {
  if (item.checked === undefined) {
    return h('li', renderInline(item.children))
  }
  return h('li', { class: 'md-task' }, [
    h('input', { type: 'checkbox', checked: item.checked, disabled: true, class: 'md-task-box' }),
    h('span', { class: item.checked ? 'md-task-done' : '' }, renderInline(item.children)),
  ])
}

function renderBlock(n: Block): VNode | VNode[] | null {
  switch (n.type) {
    case 'heading': {
      const size = 1.6 - (n.level - 1) * 0.14
      return h(`h${n.level}`, { class: 'md-h', style: { fontSize: `${size.toFixed(2)}rem`, fontWeight: 700 } }, renderInline(n.children))
    }
    case 'hr':
      return h('hr', { class: 'md-hr' })
    case 'blockquote':
      return h('blockquote', { class: 'md-quote' }, n.children.map(renderBlock).flat())
    case 'list':
      return h(n.ordered ? 'ol' : 'ul', { class: n.ordered ? 'md-ol' : 'md-ul' }, n.items.map((it) => renderListItem(it, n.ordered)))
    case 'paragraph':
      return h('p', { class: 'md-p' }, renderInline(n.children))
    default:
      return null
  }
}

export default defineComponent({
  name: 'MarkdownText',
  props: { text: { type: String, default: '' } },
  setup(props) {
    return () => {
      if (isHtmlDocument(props.text)) return h(HtmlCardFrame, { html: props.text })
      const nodes = parseMarkdown(props.text) as MdNode[]
      return h(
        'span',
        { class: 'md' },
        nodes.map((n) => {
          if (n.type === 'codeblock') {
            return n.lang === 'html' || isHtmlDocument(n.value)
              ? h(HtmlCardFrame, { html: n.value })
              : h('pre', { class: 'md-pre' }, h('code', n.value))
          }
          // Bare inline node (rare, e.g. a single emphasis span with no block).
          if (n.type === 'text' || n.type === 'strong' || n.type === 'em' || n.type === 'del' || n.type === 'code' || n.type === 'link') {
            return h('p', { class: 'md-p' }, renderInline([n]))
          }
          return renderBlock(n as Block)
        }),
      )
    }
  },
})
</script>
