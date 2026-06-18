// Split a message into reasoning ("thinking") and answer segments. Many
// reasoning models emit their chain-of-thought inline as <think>…</think>;
// rather than show it as raw text (or strip it entirely), we fold it into a
// collapsible block. The content already lives in raw/display_content, so this
// is purely presentational — no backend, storage, or provider changes.
//
// A trailing <think> with no closing tag (still streaming) is reported as open.

export type Segment = { type: 'think'; content: string; open: boolean } | { type: 'text'; content: string }

const THINK = /<think>([\s\S]*?)<\/think>/g
const OPEN = '<think>'

export function splitThinking(text: string): Segment[] {
  const segs: Segment[] = []
  let last = 0
  let m: RegExpExecArray | null
  THINK.lastIndex = 0
  while ((m = THINK.exec(text))) {
    if (m.index > last) segs.push({ type: 'text', content: text.slice(last, m.index) })
    segs.push({ type: 'think', content: m[1], open: false })
    last = m.index + m[0].length
  }
  const tail = text.slice(last)
  const openIdx = tail.indexOf(OPEN)
  if (openIdx !== -1) {
    if (openIdx > 0) segs.push({ type: 'text', content: tail.slice(0, openIdx) })
    segs.push({ type: 'think', content: tail.slice(openIdx + OPEN.length), open: true })
  } else if (tail.length > 0) {
    segs.push({ type: 'text', content: tail })
  }
  return segs
}
