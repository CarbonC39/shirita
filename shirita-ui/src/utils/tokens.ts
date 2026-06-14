// Rough, client-side token estimate. We don't ship a real tokenizer (model- and
// provider-specific); this is a display-only approximation good enough for
// budgeting. Blends a chars/4 heuristic with a per-word floor, which tracks
// GPT-style BPE reasonably for mixed English/markdown. CJK runs are counted
// closer to one token per character.
export function estimateTokens(text: string): number {
  if (!text) return 0
  // CJK ideographs + fullwidth forms — roughly one token each.
  const cjk = (text.match(/[　-鿿＀-￯]/g) || []).length
  const rest = text.length - cjk
  const words = (text.match(/[A-Za-z0-9]+/g) || []).length
  const byChars = rest / 4 + cjk
  return Math.max(1, Math.round(Math.max(byChars, words * 0.75)))
}

// "1,234" — compact, locale-grouped for display.
export function formatTokens(n: number): string {
  return n.toLocaleString()
}
