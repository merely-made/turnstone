# Relationship navigation and visible pane bindings

The graph supplies document identity and relationships. Read, Map, Inspector,
and Roster present that data; their placement and source binding belong to the
workspace. This pass makes those relationships usable while reading and makes
Inspector/Roster source rules visible.

## Interaction

The toolbar's Related disclosure projects the current member's existing
relationships. Each row names a destination and explains its direction and
kind: Links to, Linked from, Cites, Cited by, Went to, Reached from, and the
other recognized relation kinds. Six rows per page keep larger neighborhoods
reachable. Panes exposes the same disclosure when toolbar space is constrained.
Reveal in Map and document activation use existing navigation commands, preserving
member identity and the current reading arrangement. The captured graph remains
explicit through reading activation, including when another loaded graph holds
the same member identities.

Inspector and Roster display their source rule and resolved source above their
content. The disclosure offers Follow active pane, Follow a specific Map/Read/
document publisher, and Pin this document (Inspector) or Pin this graph (Roster).
A source pin names the existing member or graph; it does not save a content revision.
Inspector labels the separate spatial anchor as Map position pinned.
Following a specific pane stays with that publisher across focus changes.
Following active pane resolves within the follower's current window.

No selected document, a removed pinned source, and a closed publisher have
explicit unavailable states. They do not resolve to another canvas selection.
Source choices capture a process admission and exact target; a context change
invalidates old choices. Escape collapses the source disclosure.

## Ownership and reuse

- The kernel's canonical projected relations remain the relationship read model.
  Rendering the navigator creates no resources, members, or assertions.
- Turnstone owns graph lookup, source validation, captured choices, navigation,
  persistence, and action/ring declarations.
- Existing Workspace PaneSpec descriptors own source and context. No second
  binding map or persistence file is introduced. Descriptors travel with pane
  custody through floating, docking, transfer, and saved layouts.
- Cambium supplies `shell_source_binding` and `shell_navigation_item`, composed
  from ordinary accessible controls and shared shell styling. They accept host
  strings, actions, and disclosure state; they contain no graph model.
- Inspector observations use the same per-pane resolution as the visible UI.

The source/context separation follows the pane registry and graph views design
(2026-08-08), especially its Context publication section. The peripheral pane
architecture (Mere, 2026-06-06) supplies the projection/authority distinction;
its historical pane catalog is superseded by Turnstone's later consolidation.

## Evidence

Qualification and native captures are recorded with this implementation under
`/Users/markik/Code/output/turnstone-relationships-bindings-20261021`.
The receipt distinguishes focused tests, broader suites, native execution,
and OS window captures. This is a macOS/native slice; it makes no other-platform
or remote publication claim.
