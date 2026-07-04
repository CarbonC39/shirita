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

export type Block =
  | { type: 'heading'; level: 1 | 2 | 3 | 4 | 5 | 6; children: Inline[] }
  | { type: 'hr' }
  | { type: 'blockquote'; children: Block[] }
  | { type: 'list'; ordered: boolean; items: ListItem[] }
  | { type: 'paragraph'; children: Inline[] }

export type ListItem = { children: Inline[]; checked?: boolean }

export type MdNode = Inline | { type: 'codeblock'; lang: string | null; value: string } | Block

// SillyTavern character cards sometimes ship a full HTML/CSS/JS document as
// their first message (a "card front-end"), occasionally fenced in ```html.
// Detected separately from the rest of markdown parsing because it needs a
// completely different render path (sandboxed iframe, not the VNode whitelist).
export function isHtmlDocument(text: string): boolean {
  const s = text.trim().toLowerCase()
  return s.startsWith('<!doctype html') || s.startsWith('<html')
}

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

// Recognize ATX headings (# .. ######) and setext headings (text ===/---).
const ATX = /^(#{1,6})\s+(.+?)\s*#*\s*$/
const HR = /^\s*(-{3,}|\*{3,}|_{3,})\s*$/

export function parseMarkdown(src: string): MdNode[] {
  const out: MdNode[] = []
  let last = 0
  for (const m of src.matchAll(FENCE)) {
    const idx = m.index ?? 0
    if (idx > last) out.push(...parseBlocks(src.slice(last, idx)))
    const lang = m[1].trim()
    out.push({ type: 'codeblock', lang: lang || null, value: m[2] })
    last = idx + m[0].length
  }
  if (last < src.length) out.push(...parseBlocks(src.slice(last)))
  return out
}

// Turn a chunk of non-fenced text into block-level nodes. Splits on blank
// lines so paragraphs/quotes/lists get their own boundaries.
function parseBlocks(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, '\n').split('\n')
  const out: Block[] = []
  let i = 0
  while (i < lines.length) {
    const line = lines[i]

    // Blank line — skip.
    if (/^\s*$/.test(line)) { i++; continue }

    // Horizontal rule.
    if (HR.test(line)) { out.push({ type: 'hr' }); i++; continue }

    // ATX heading.
    const atx = ATX.exec(line)
    if (atx) {
      out.push({ type: 'heading', level: atx[1].length as 1 | 2 | 3 | 4 | 5 | 6, children: parseInline(atx[2]) })
      i++
      continue
    }

    // Blockquote: gather consecutive '>' lines.
    if (/^>\s?/.test(line)) {
      const buf: string[] = []
      while (i < lines.length && /^>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^>\s?/, ''))
        i++
      }
      out.push({ type: 'blockquote', children: parseBlocks(buf.join('\n')) })
      continue
    }

    // List: gather consecutive list-item lines (and their lazy continuation).
    const ul = /^\s*([-*+])\s+(\[[ xX]]\s+)?(.*)$/.exec(line)
    const ol = /^\s*(\d+)\.\s+(.*)$/.exec(line)
    if (ul || ol) {
      const ordered = !!ol
      const items: ListItem[] = []
      const itemRe = ordered ? /^\s*\d+\.\s+(.*)$/ : /^\s*[-*+]\s+(\[[ xX]]\s+)?(.*)$/
      while (i < lines.length) {
        const cur = lines[i]
        if (/^\s*$/.test(cur)) {
          // A blank line then a non-item line ends the list; a following item
          // continues it (loose vs tight). Peek ahead.
          if (i + 1 < lines.length && (itemRe.test(lines[i + 1]))) { i++; continue }
          break
        }
        const m = itemRe.exec(cur)
        if (m) {
          if (ordered) {
            items.push({ children: parseInline(m[1]) })
          } else {
            const checked = m[1] ? m[1].trim().toLowerCase() === '[x]' : undefined
            items.push({ children: parseInline(m[2]), ...(checked !== undefined ? { checked } : {}) })
          }
          i++
        } else if (/^\s{2,}\S/.test(cur) && items.length) {
          // Lazy continuation of the previous item.
          const prev = items[items.length - 1]
          prev.children = [...prev.children, ...parseInline('\n' + cur.trim())]
          i++
        } else {
          break
        }
      }
      out.push({ type: 'list', ordered, items })
      continue
    }

    // Setext heading: a line of text followed by ===/---.
    if (i + 1 < lines.length && /^=+\s*$/.test(lines[i + 1]) && line.trim()) {
      out.push({ type: 'heading', level: 1, children: parseInline(line.trim()) })
      i += 2
      continue
    }
    if (i + 1 < lines.length && /^-{2,}\s*$/.test(lines[i + 1]) && line.trim() && !HR.test(line)) {
      out.push({ type: 'heading', level: 2, children: parseInline(line.trim()) })
      i += 2
      continue
    }

    // Paragraph: gather until a blank line or a block-starting construct.
    const buf: string[] = [line]
    i++
    while (
      i < lines.length &&
      !/^\s*$/.test(lines[i]) &&
      !ATX.test(lines[i]) &&
      !HR.test(lines[i]) &&
      !/^>\s?/.test(lines[i]) &&
      !/^\s*([-*+])\s+(\[[ xX]]\s+)?/.test(lines[i]) &&
      !/^\s*\d+\.\s+/.test(lines[i])
    ) {
      buf.push(lines[i])
      i++
    }
    out.push({ type: 'paragraph', children: parseInline(buf.join('\n')) })
  }
  return out
}
