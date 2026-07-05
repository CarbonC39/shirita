// Deep-copy primitive. Used for copy-on-write session overrides so that
// editing a session-local value can never mutate the global library entity.
export function deepClone<T>(value: T): T {
  return structuredClone(value)
}
