# Composer attachments (chat-side file/image upload)

**Status: DEFERRED — planned, not implemented.** Captured 2026-06-17 from the
frontend review (finding #3).

## Context

`Composer.vue` renders a `+` button (`Plus` icon, `composer.attach` title) with
**no click handler** — attaching files/images to a chat message is unimplemented.
The button looks actionable but does nothing.

## Goal

Let the user attach an image (and later, arbitrary files) to a message they send,
shown inline in the transcript and — where the provider supports it — passed to
the model as multimodal content.

## Scope / open questions (resolve before building)

- **Display vs. multimodal:** attach-for-display-only first, or wire images into
  the provider request (vision content blocks) from the start?
- **Storage:** reuse the existing asset library (`POST /api/assets`, already in
  `client.ts` as `uploadAsset`) for the bytes. How is the link persisted on a
  `Message`? (new `attachments` field on the message → needs a migration, or
  store refs in `snapshot_state`/meta?)
- **Provider support:** only some providers accept images; the OpenAI/Anthropic
  adapters currently send text-only `content`. Needs a content-parts shape +
  per-provider capability gating, or graceful no-op.
- **UI:** thumbnail in the composer before send; render attachment in
  `MessageItem` (bubble + flat); remove-before-send affordance.

## Why deferred

Beyond the dead button, a real implementation spans storage (likely a message
migration), the provider adapters (multimodal content), and the message
renderer — a vertical slice, not a quick fix. Tracking it here so it isn't lost.

## Minimal first increment (when picked up)

1. Wire the `+` button to the existing asset upload + show a thumbnail chip.
2. Persist attachment asset refs on the sent message (decide storage shape).
3. Render attachments in `MessageItem`.
4. (Separate) provider multimodal content + capability gating.
