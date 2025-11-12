# Registry Policy

This document describes the project policy for global registries embedded or loaded
at runtime (commands, error codes, devices, etc.). The goal is to make initialization
and runtime overrides predictable and safe.

## Principles

- Prefer non-destructive initialization: adding built-in definitions should not
  unexpectedly remove or replace user-provided entries.
- Keep global registries simple: most registries are intended to be set once at
  application startup. When a merge-friendly behavior is required, it must be
  explicit and documented.

## Specific registries

- `ErrorRegistry` (error codes)
  - Behavior: merge-friendly. Built-in `error_codes.toml` is loaded via
    `ErrorRegistry::from_str(...).register_or_merge()` so it will add entries and
    merge tables without clearing any user-provided entries.
  - Rationale: libraries and apps may both want to contribute known error code
    tables. Merging avoids surprising removal of entries when the library is used
    inside a larger application.

- `CommandRegistry`
  - Behavior: set-once. Commands are provided by `commands.toml`. The convenience
    loader `CommandRegistry::load_and_set_global_from_src()` will set the global
    registry but return an error if the registry has already been set. This
    preserves the invariant that a single authoritative command spec should be
    used at runtime.
  - Rationale: command specifications determine wire formats. Allowing multiple
    conflicting command tables to overwrite each other is error-prone.

- `DeviceRegistry` / device overrides
  - Behavior: initial set from compiled-in `devices.toml`. Runtime override via
    a sled-backed cache is supported; overrides are explicitly namespaced and do
    not destructively replace the embedded database unless the application
    chooses to do so.

## Guidance for contributors

- When adding a new registry, document the intended lifecycle: "set-once" vs
  "merge-friendly".
- Avoid APIs that silently clear global registries. Prefer explicit `replace`
  or `clear_and_set` operations that return a clear log/error message when used.
- Tests should exercise the intended behavior (e.g., merging vs already-set
  errors) to avoid regressions.

## Migration notes

- If a registry's behavior is changed (e.g. from set-once to merge-friendly),
  update this document and add regression tests that demonstrate the new
  desired behavior.

*End of registry policy.*
