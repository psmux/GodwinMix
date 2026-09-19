# Workspace layout

`ui/shell/dock-model.js` contains the layout model without DOM dependencies.
`dock.js` owns panel lifecycle and geometry. `dock-pointer.js` handles pointer
input and dividers. `dock-menu.js` supplies the keyboard accessible controls.
The modules have no third party dependencies.

A leaf is `{tabs: [panelId], active: panelId}`. A split is
`{axis: "x" | "y", ratio: number, a: node, b: node}`. Ratios are clamped from
0.15 to 0.85. Normalization removes duplicate panel ids, collapses empty
branches, and bounds tree depth. The browser stores version 1 with `tree` and
`hidden` under `gmx.workspace.v1`. Storage failure leaves a working session
layout. `gmx.layout` remains the source of legacy slot defaults.

## Panel contract

Existing trusted custom elements and sandboxed iframe panels register through
`window.godwinmixPanels` or the panel registry. Header and modal panels remain
in fixed shell slots. All other registered panels participate in docking.
The registry's `slots` property supplies initial placement, not a restriction
on subsequent operator arrangements.

Active panel elements stay connected while a pane moves or resizes. Geometry
changes are CSS writes scheduled with `requestAnimationFrame`; a resize does
not rebuild the panel or add a media subscription. By default there is one instantiated
panel per active tab. An inactive or closed panel is destroyed, and activating
it asks the registry for a fresh instance. The optional suspension hook below
allows trusted panels to preserve local state while inactive. Trusted panels must release their
subscriptions, timers and observers in `disconnectedCallback`. A sandboxed
panel's bridge is destroyed through the registry.

Keep durable panel state in the client store or an explicit preference. Do not
expect transient element fields to survive closing a panel or switching tabs.
New panels use the existing public client and plugin APIs; docking introduces
no private mixer endpoint.

See [Write a panel](../how-to/write-a-panel.md) for registration and the
sandbox contract, and [Customize the workspace](../how-to/customize-the-workspace.md)
for operator controls.

## Optional suspension hook

A trusted panel can implement `setWorkspaceActive(active)` to preserve its
instance across tab changes. The workspace calls it with `false` before hiding
the element and with `true` after showing it again. The initial visible mount
uses the normal `connectedCallback`. Closing the panel still disconnects and
destroys it.

On suspension, release preview subscriptions, meters, push streams, timers and
other work that exists only for a visible surface. On activation, reacquire
only what the current view requests. Keep form values and selection in the
element if they should survive a tab switch. The hook is synchronous. If
suspension throws, the workspace destroys the panel as a safe fallback.
Panels without the hook use the normal destroy and instantiate lifecycle.

The Programme panel retains its controls between tab switches and releases both
its programme mosaic and preview requests when suspended. The Outputs panel
also retains its instance, so switching tabs does not reset its local controls.

## Shared scene data and source selection

Scenes and Sources acquire the same `SceneClient` through
`ui/shell/scene-session.js`. The last panel releases its protocol listeners,
so closing the Scenes dock does not turn the Sources dock into a global list.
The focused scene selects the gallery. Older `gmx.sources.scope` preferences
are ignored.

The source chooser is loaded from `panels/sources/chooser.js` on demand. It
extends the source picker with an existing-source category and reuses
`scene.item.add` for insertion. Device and file categories remain directly
available in the same modal. Its preview
component requests the shared multiview only after the operator selects
Preview, attaches one source cell, and releases it when closed or hidden.
Scene removal uses `scene.item.remove`; it does not delete the mixer source.
