import DOMPurify from 'dompurify'

// What a status panel legitimately needs. Everything else (script, iframe,
// object, embed, form, link, meta, base, event handlers, javascript:/data: urls)
// is dropped. data-* attributes survive (bindings in this plan, actions in Plan 3).
const ALLOWED_TAGS = [
  'div', 'span', 'p', 'a', 'b', 'i', 'em', 'strong', 'small', 'br', 'hr',
  'ul', 'ol', 'li', 'table', 'thead', 'tbody', 'tr', 'td', 'th',
  'details', 'summary', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'img', 'button',
  'svg', 'path', 'g', 'circle', 'rect',
]
const ALLOWED_ATTR = [
  'class', 'style', 'title', 'alt', 'src', 'width', 'height', 'open', 'colspan', 'rowspan',
  'data-bind', 'data-show', 'data-diff-key', 'data-diff-op', 'data-diff-value', 'data-insert', 'data-send',
  'viewBox', 'fill', 'stroke', 'stroke-width', 'd', 'cx', 'cy', 'r', 'x', 'y',
]

/** Sanitize author panel HTML to a safe subset — no script, no remote/js URLs. */
export function sanitizePanelHtml(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    FORBID_TAGS: ['script', 'iframe', 'object', 'embed', 'form', 'link', 'meta', 'base', 'style'],
    // href/src may only be local or relative (blocks remote exfil, javascript:, data:).
    ALLOWED_URI_REGEXP: /^(?:\/assets\/|\/|\.\/|#)/i,
  }) as unknown as string
}

// Defensive CSS fence — the shadow root already scopes selectors; this removes the
// few properties that escape the box or phone home. Applied to the css field and
// to each element's inline style attribute (status cards rely on inline styles).
export function fenceCss(css: string): string {
  return css
    .replace(/@import[^;]*;?/gi, '')
    .replace(/position\s*:\s*(?:fixed|sticky)\s*;?/gi, '')
    .replace(/url\(\s*['"]?\s*(?:https?:)?\/\/[^)]*\)/gi, 'url()')
    .replace(/expression\s*\([^)]*\)/gi, '')
    .replace(/behavior\s*:[^;]*;?/gi, '')
}
