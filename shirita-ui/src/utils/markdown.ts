// A small, dependency-free Markdown parser for chat messages. It produces an
// AST that MarkdownText.vue renders to VNodes — never to an HTML string — so
// there is no innerHTML / v-html and nothing to sanitize (Vue escapes text; we
// only ever emit a fixed whitelist of tags, and link hrefs are scheme-checked).
//
// Scope is the high-value chat/RP subset: **bold**, *italic*/_italic_,
// ~~strike~~, `code`, ```fenced code```, and [links](url). Unsupported syntax
// is left as literal text. Newlines are preserved as text (the container keeps
// `white-space: pre-wrap`).

export type Inline =
  | { type: 'text'; value: string }
  | { type: 'strong'; children: Inline[] }
  | { type: 'em'; children: Inline[] }
  | { type: 'del'; children: Inline[] }
  | { type: 'code'; value: string }
  | { type: 'link'; href: string; children: Inline[] }

export type MdNode = Inline | { type: 'codeblock'; lang: string | null; value: string }

// Allow only obviously-safe link targets; anything else (javascript:, data:, …)
// falls through and the link renders as literal text.
function safeHref(href: string): boolean {
  return /^(https?:\/\/|mailto:|\/|#)/i.test(href) || !href.includes(':')
}

const INLINE_RULES: { re: RegExp; make: (m: RegExpExecArray) => Inline | null }[] = [
  { re: /`([^`]+)`/, make: (m) => ({ type: 'code', value: m[1] }) },
  { re: /\*\*([\s\S]+?)\*\*/, make: (m) => ({ type: 'strong', children: parseInline(m[1]) }) },
  { re: /~~([\s\S]+?)~~/, make: (m) => ({ type: 'del', children: parseInline(m[1]) }) },
  { re: /\*([^*\n]+?)\*/, make: (m) => ({ type: 'em', children: parseInline(m[1]) }) },
  { re: /_([^_\n]+?)_/, make: (m) => ({ type: 'em', children: parseInline(m[1]) }) },
  {
    re: /\[([^\]]+)\]\(([^)\s]+)\)/,
    make: (m) => (safeHref(m[2]) ? { type: 'link', href: m[2], children: parseInline(m[1]) } : null),
  },
]

function parseInline(text: string): Inline[] {
  if (!text) return []
  // Pick the earliest-starting rule; ties resolve by rule order (bold > italic).
  let best: { index: number; len: number; node: Inline } | null = null
  for (const rule of INLINE_RULES) {
    const m = rule.re.exec(text)
    if (!m) continue
    const node = rule.make(m)
    if (!node) continue // e.g. an unsafe link: ignore, leave as text
    if (best === null || m.index < best.index) {
      best = { index: m.index, len: m[0].length, node }
    }
  }
  if (best === null) return [{ type: 'text', value: text }]
  const out: Inline[] = []
  if (best.index > 0) out.push({ type: 'text', value: text.slice(0, best.index) })
  out.push(best.node)
  out.push(...parseInline(text.slice(best.index + best.len)))
  return out
}

const FENCE = /```([^\n]*)\n([\s\S]*?)```/g

export function parseMarkdown(src: string): MdNode[] {
  const out: MdNode[] = []
  let last = 0
  for (const m of src.matchAll(FENCE)) {
    const idx = m.index ?? 0
    if (idx > last) out.push(...parseInline(src.slice(last, idx)))
    const lang = m[1].trim()
    out.push({ type: 'codeblock', lang: lang || null, value: m[2] })
    last = idx + m[0].length
  }
  if (last < src.length) out.push(...parseInline(src.slice(last)))
  return out
}
