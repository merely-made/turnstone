# Shared Weld input follow-up

Owner: Mere `crates/inker/engines/weld-engine/src/welding_0_15.rs`.
The pinned source and current Mere checkout both reject Mouse PointerEvent in
`map_pointer`. Turnstone's real winit mouse path uses PointerEvent so it can
retain modifiers and held-button state. Converting to the legacy Inker
MouseEvent would discard those fields. The shared keyboard mapping emits raw
key down/up but not the separate CEF CHAR event required for text.

The direct consumer therefore retains a narrow native mouse/character bridge.
A thread-local shared producer alias forwards every CefSurfaceProducer method,
including ordered events and owned frames, to the same producer. This preserves
shared adapter command/event ownership and its find-query state. It does not
reconstruct or duplicate a producer. The ordinary MouseEvent, touch, drag and
raw keyboard paths still use the shared adapter.

The library follow-up should accept mouse PointerEvent through the native mouse
path with actual buttons/modifiers, and emit CEF character events for pressed
text. CHAR's Windows key code must be the character code, rather than the raw
virtual key code: a real two-page fixture initially produced ALPHA/BRAVO when
given lowercase input. The corrected host produces alpha/bravo, independently,
and retains the raw key codes for down/up. Adopt only after its unit contracts and Turnstone's two-page mouse/text,
find/zoom, permission and teardown scenarios pass on an exact source. Then the
host bridge and native producer alias can be removed together. Supplementary
Unicode and OS IME remain separate native gates. Broader Mere integration
remains held.
