# Customize the workspace

The broadcast workspace can split into as many panes as your screen needs.
Drag a panel title over another panel. The highlighted area shows the result:
near an edge creates a split; the centre groups the panels as tabs. Release to
apply the change. Press Escape while dragging to cancel.

Drag a divider to resize adjacent panes. Layout changes affect the control
surface only. The mixer keeps sending the programme. Monitor geometry follows
your pointer immediately; stream resolution and sharp canvas sizing update
once resizing has been still for 150 milliseconds. Hiding a monitor releases
its streams immediately.

## Use the keyboard

Open a panel's actions button, choose a destination panel and a position, then
choose Move panel. Centre groups the panels as tabs. Focus a divider and use
arrow keys to resize it. In a tab group, left and right arrows select the next
panel. You can also select tabs with a pointer.

## Close, reopen and reset

The close button removes a panel from the workspace. Open Panels and layout in
the workspace toolbar to reopen it, or choose Reset workspace to restore the
arrangement supplied by the mixer. The browser remembers the split tree,
selected tabs, sizes and closed panels on this device. Existing slot layouts
are used as the starting arrangement the first time the workspace opens.

Moving or resizing a panel preserves its live instance. Closing a panel releases its instance and subscriptions. First party panels
preserve local controls and selection when switching tabs, while suspending
work for the hidden surface. Plugin panels can opt into the same suspension
contract. Older plugins are recreated on activation, so apply their unfinished
forms before switching tabs.

On a narrow screen the panes become a vertical stack. The saved desktop
arrangement remains intact, and returns when the window becomes wider. Use
the panel actions menu to rearrange panes when pointer docking is unavailable.

This workspace does not currently support separate native windows or floating
panes outside the browser window.

## Keep a layout for each show

Open Panels and layout, enter a name under Saved workspaces, and choose Save
current. A name you have already used replaces that saved arrangement. Select
a saved name and choose Load to restore it. Delete saved removes the stored
copy without changing the current workspace.

Export JSON downloads the current arrangement. Import JSON applies an exported
workspace from another browser or machine. These files contain panel ids,
split positions and closed panels. They do not contain mixer settings,
source addresses, stream keys or other credentials. Up to twenty named layouts
can be kept in browser storage.

The default desk puts Programme across the top, with Scenes, Sources and the
utility tabs below. Sources gets more width than Scenes so a camera gallery has
room to work. Reset workspace applies this arrangement; existing saved layouts
keep their positions.
