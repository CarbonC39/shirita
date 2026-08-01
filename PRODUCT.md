# Shirita product direction

> Status: active product guidance. This document describes what Shirita is trying to become; it is not a claim that every item is implemented today.

## Purpose

Shirita is a modern AI role-playing platform that users can run as either a desktop application or a self-hosted web service.

It should make characters, knowledge, prompts, state, tools, and text transforms composable without forcing the product to reproduce the data model or behavior of another application. The default interface should make the common path reliable and understandable; customization can remain available without making the built-in UI expensive to maintain.

## Product commitments

- Desktop and self-hosted deployment are equal product targets.
- Users control their data and model-provider credentials.
- The Rust core owns conversation behavior, context construction, state transitions, and provider integration.
- Reusable content remains composable. Packs and definitions should describe Shirita concepts rather than compatibility concepts.
- Regex text transforms remain a supported, first-class capability for practical input and output normalization.
- Token usage and context-budget visibility are necessary agent-runtime capabilities, not optional decoration.
- The default UI prioritizes chat space, mobile usability, predictable behavior, and low maintenance cost.
- Native import and export must remain portable and documented.

## Non-goals for the current phase

- Reimplementing SillyTavern or preserving behavioral compatibility with it.
- Redesigning prompt composition before the chat runtime and UI are stable.
- Adding more visual systems to the default UI.
- Expanding the provider list before the shared provider/runtime boundary is reliable.

## Compatibility policy

Shirita is pre-1.0. Breaking changes are acceptable when they remove substantial maintenance burden or establish a clearer native model.

SillyTavern card, preset, World Info, TavernHelper, and related compatibility behavior are scheduled for removal from the main application. Generally useful capabilities that happen to overlap with SillyTavern — including branching, keyword activation, and regex transforms — are evaluated as Shirita features and are not removed merely because of their origin.

If migration remains useful, it should be provided as a bounded conversion path rather than a permanent compatibility layer inside the runtime.

## UI policy

The built-in interface is a dependable reference client, not a showcase for every possible presentation.

- Chat content and text input receive most of the available space.
- Mobile layouts must not inherit desktop navigation and message controls unchanged.
- A behavior should have one clear default presentation where possible.
- Custom CSS remains supported, but built-in layout correctness must not depend on it.
- Token information remains available without permanently consuming disproportionate composer space.

## Decision boundary

New work should answer at least one of these questions positively:

1. Does it improve reliable role-playing or agent interaction?
2. Does it reduce the cost of maintaining an existing capability?
3. Does it improve user control, portability, or observability?
4. Does it strengthen both desktop and self-hosted operation without coupling them?

Work that primarily increases compatibility surface or adds another special-case representation should normally be rejected or deferred.
