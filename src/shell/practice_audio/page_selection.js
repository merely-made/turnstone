// Installed at document creation in the frame's private Turnstone world.
// Choosing a player binds a weak reference. Capture starts from the native toolbar.
(() => {
    if (globalThis.__turnstonePlayerSelection) return;
    let selected = null;
    let pendingControl = null;

    function choose(element, error) {
        const token = Array.from(crypto.getRandomValues(new Uint8Array(16)),
            n => n.toString(16).padStart(2, "0")).join("");
        selected = { token, element: element && new WeakRef(element), source: element?.currentSrc, error };
        if (typeof globalThis.__turnstoneSelectPlayer === "function") {
            globalThis.__turnstoneSelectPlayer(token);
        }
    }

    function controlledPlayers(control) {
        const root = control.getRootNode();
        const media = new Set();
        for (const id of control.getAttribute("aria-controls").trim().split(/\s+/)) {
            const target = root.getElementById(id);
            if (target instanceof HTMLMediaElement) media.add(target);
            else if (target) for (const el of target.querySelectorAll("audio,video")) media.add(el);
        }
        return media;
    }

    function onInput(event) {
        if (!event.isTrusted || event.button !== 0 || event.isPrimary === false) return;
        const path = event.composedPath().filter(node => node instanceof Element);
        pendingControl = null;
        const direct = path.find(el => el instanceof HTMLMediaElement);
        if (direct) {
            choose(direct);
            return;
        }
        const linked = path.find(el => el.hasAttribute("aria-controls"));
        if (linked) {
            const players = controlledPlayers(linked);
            if (players.size === 1) choose(players.values().next().value);
            else if (players.size > 1) choose(null, "Click the audio or video you want to mark.");
            return;
        }
        const control = path.find(el => el.matches(
            'button, [role="button"], input[type="button"], input[type="image"]'
        ));
        if (!control) return;

        // An unlabeled custom Play button is useful when it really starts a player.
        // Scope the association to the nearest player container and require its
        // actual play/pause transition. An unrelated button must not steal a mark.
        for (const container of path) {
            if (container === control || !container.contains(control)) continue;
            const players = container.querySelectorAll("audio,video");
            if (players.length === 0) continue;
            pendingControl = {
                before: new WeakMap(Array.from(players, player => [player, player.paused])),
                changed: null,
                until: performance.now() + 750
            };
            break;
        }
    }

    function onPlayback(event) {
        if (!event.isTrusted || !pendingControl) return;
        const { before, changed, until } = pendingControl;
        if (performance.now() <= until && before.has(event.target) &&
                before.get(event.target) !== event.target.paused) {
            if (changed && changed.deref() !== event.target) {
                pendingControl = null;
                choose(null, "That control changed multiple players. Click the audio or video itself.");
            } else {
                pendingControl.changed = new WeakRef(event.target);
                choose(event.target);
            }
        }
    }

    addEventListener("pointerdown", onInput, {capture: true, passive: true});
    // Native accessibility / keyboard activation can generate a trusted click
    // without a preceding pointer event.
    addEventListener("click", event => {
        if (event.detail === 0) onInput(event);
    }, true);
    addEventListener("play", onPlayback, true);
    addEventListener("pause", onPlayback, true);
    addEventListener("loadedmetadata", event => {
        if (selected?.element?.deref() === event.target && !selected.source) {
            selected.source = event.target.currentSrc;
        }
    }, true);
    addEventListener("pagehide", () => { selected = pendingControl = null; });

    Object.defineProperty(globalThis, "__turnstonePlayerSelection", {
        value: Object.freeze({
            resolve(token) {
                if (!selected || selected.token !== token) throw Error("Click the player again.");
                if (selected.error) throw Error(selected.error);
                const el = selected.element?.deref();
                if (!el?.isConnected || el.ownerDocument !== document ||
                        (selected.source && el.currentSrc !== selected.source)) {
                    throw Error("The selected player changed. Click it again.");
                }
                return el;
            }
        })
    });
})()
