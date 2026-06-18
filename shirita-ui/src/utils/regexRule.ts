import type { Definition, RegexRule } from '../api/types'

// Bridge the regex editor's view model to the backend's canonical regex_rule
// meta contract (shared with ST import): { disabled, scope: "display"|"both"|
// "prompt", targets: ("ai_output"|"user_input")[] }. The editor's enable toggle
// and "Apply to" checkboxes were previously written as `enabled` / a `scope`
// object, neither of which the backend reads — so they did nothing.

/** Canonical meta -> editor view model. */
export function metaToRule(def: Definition): RegexRule {
  const meta = def.meta as Record<string, unknown>
  const scopeStr = typeof meta.scope === 'string' ? meta.scope : 'display'
  const targets = Array.isArray(meta.targets) ? (meta.targets as unknown[]) : []
  const hasTargets = targets.length > 0
  return {
    id: def.id,
    name: def.name,
    pattern: typeof meta.pattern === 'string' ? meta.pattern : '',
    replacement: typeof meta.replacement === 'string' ? meta.replacement : '',
    enabled: meta.disabled !== true,
    scope: {
      // Empty/absent targets stay broad, matching the backend's apply path.
      ai_output: !hasTargets || targets.includes('ai_output'),
      user_input: targets.includes('user_input'),
      display_only: scopeStr === 'display',
    },
  }
}

/** Editor "Apply to" flags -> canonical meta fields. */
export function scopeFlagsToMeta(scope: RegexRule['scope']): { scope: string; targets: string[] } {
  const targets: string[] = []
  if (scope.ai_output) targets.push('ai_output')
  if (scope.user_input) targets.push('user_input')
  return { scope: scope.display_only ? 'display' : 'both', targets }
}
