// Installed by Turnstone in the selected document's isolated WebView2 world.
(() => {
    if (globalThis.__turnstonePractice) return true;
    const maxBytes = 16 * 1024 * 1024 - 128;
    const maxRead = 64 * 1024;
    let active = null;
    let sentPlayer = null;
    let lastError = "Mark the start of a playing passage first.";

    function stopTap(session) {
        clearTimeout(session.timer);
        session.events.abort();
        if (session.reader) session.reader.cancel().catch(() => {});
        if (session.stream) for (const track of session.stream.getTracks()) track.stop();
    }

    function cancel(reason = "Phrase capture cancelled.") {
        const previous = active;
        active = null;
        sentPlayer = null;
        lastError = reason;
        if (previous) {
            stopTap(previous);
            previous.ready?.({ok: false, error: reason});
            previous.finished?.({ok: false, error: reason});
        }
        return {ok: true};
    }

    function valid(session) {
        const el = session.element;
        return el.isConnected && el.currentSrc === session.src && !el.mediaKeys &&
            el.playbackRate === 1 && !el.paused && !el.seeking && !el.muted && el.volume > 0;
    }

    function title() {
        return document.title.slice(0, 160).replace(/[\u0000-\u001f\u007f]/g, " ").trim();
    }

    function finish(session) {
        if (session.wanted == null || session.frames < session.wanted) return;
        stopTap(session);
        session.sealed = true;
        session.finished({
            ok: true, title: title(), start_ms: session.start, end_ms: session.end,
            frames: session.wanted, sample_rate: session.rate, channels: session.channels
        });
        session.finished = null;
    }

    function append(session, audio) {
        const rate = audio.sampleRate;
        const channels = audio.numberOfChannels;
        const count = audio.numberOfFrames;
        if (!Number.isSafeInteger(rate) || rate < 1000 || rate > 384000 ||
            channels < 1 || channels > 32 || !count) throw Error("Unsupported audio format.");
        const now = performance.now();
        // Chromium timestamps the captured playout buffer with its output delay
        // subtracted from the monotonic clock. The corresponding source samples
        // are ahead of the element's audible clock by that same output delay.
        const delay = now - audio.timestamp / 1000;
        if (!Number.isFinite(delay) || delay < -30 || delay > 1000)
            throw Error("The page player did not provide a stable audio clock.");
        const mediaTime = session.element.currentTime * 1000;
        let skip = 0;
        if (!session.rate) {
            const first = Math.max(session.requested, mediaTime + Math.max(0, delay));
            session.start = Math.ceil(first);
            skip = Math.floor(session.start * rate / 1000) - Math.floor(first * rate / 1000);
            session.rate = rate;
            session.channels = Math.min(channels, 2);
            session.inputChannels = channels;
        } else if (session.rate !== rate || session.inputChannels !== channels) {
            throw Error("The page audio format changed. Mark the passage again.");
        } else if (Math.abs(mediaTime + Math.max(0, delay) -
                (session.start + session.frames * 1000 / rate)) > 175) {
            throw Error("The page audio jumped or stalled. Mark the passage again.");
        }
        if (session.processor.discardedFrames > 0)
            throw Error("The browser dropped audio while marking. Mark the passage again.");
        const framesToKeep = session.wanted == null ? count - skip :
            Math.min(count - skip, session.wanted - session.frames);
        if ((session.frames + framesToKeep) * session.channels * 2 > maxBytes)
            throw Error("This phrase exceeds 16 MiB. Mark a shorter passage.");
        const planes = Array.from({length: channels}, (_, i) => {
            const plane = new Float32Array(count);
            audio.copyTo(plane, {planeIndex: i, format: "f32-planar"});
            return plane;
        });
        // Store just PCM. No pictures or encoded container bytes enter the clip.
        const output = new Int16Array(framesToKeep * session.channels);
        let j = 0;
        for (let frame = skip; frame < skip + framesToKeep; ++frame) {
            let surround = 0;
            for (let c = 2; c < channels; ++c) surround += planes[c][frame];
            for (let c = 0; c < session.channels; ++c) {
                const sample = channels <= 2 ? planes[c][frame] :
                    (2 * planes[c][frame] + surround) / channels;
                if (!Number.isFinite(sample)) throw Error("The page supplied invalid audio.");
                output[j++] = Math.max(-32768, Math.min(32767, Math.round(sample * 32768)));
            }
        }
        session.chunks.push(new Uint8Array(output.buffer));
        session.frames += framesToKeep;
        if (session.ready) {
            clearTimeout(session.timer);
            session.ready({ok: true, title: title(), start_ms: session.start});
            session.ready = null;
        }
        finish(session);
    }

    async function pump(session) {
        try {
            while (active === session && !session.sealed) {
                const {done, value} = await session.reader.read();
                if (done) {
                    if (!session.sealed && active === session) throw Error("The audio track ended.");
                    return;
                }
                try {
                    if (active !== session || session.sealed) return;
                    if (!valid(session)) throw Error("Playback changed. Mark the passage again.");
                    append(session, value);
                } finally { value.close(); }
            }
        } catch (error) {
            if (active === session) cancel(String(error.message || error));
        }
    }

    function playerForMark(token) {
        // An explicit click always wins, including when its player subsequently
        // stopped or disappeared. Never substitute another audible element.
        if (token !== undefined) {
            const selection = globalThis.__turnstonePlayerSelection;
            if (!selection) throw Error("Click the embedded player again.");
            const player = selection.resolve(token);
            if (player.paused || player.ended || player.muted || player.volume === 0) {
                throw Error("Play the selected audio or video before marking.");
            }
            return player;
        }
        if (window !== window.top || document.activeElement?.matches("iframe,frame")) {
            throw Error("Click the audio or video in the embedded player before marking.");
        }
        const players = [...document.querySelectorAll("audio,video")].filter(
            el => !el.paused && !el.ended && !el.muted && el.volume > 0);
        if (players.length !== 1) throw Error(players.length ?
            "Click the audio or video you want to mark." :
            "Play the audio or video and click the player before marking.");
        return players[0];
    }

    async function markIn(token) {
        cancel();
        try {
            // origin is the effective document origin, including an inherited
            // origin for srcdoc and the opaque origin of sandboxed documents.
            if (!/^https?:\/\//.test(globalThis.origin)) {
                throw Error("The player needs an HTTP or HTTPS document with a non-opaque origin.");
            }
            const el = playerForMark(token);
            if (el.mediaKeys || el.srcObject) throw Error("This player does not expose recordable media.");
            if (el.seeking || el.playbackRate !== 1) throw Error("Play the passage at normal speed.");
            const src = new URL(el.currentSrc);
            if (src.origin !== globalThis.origin && el.crossOrigin === null)
                throw Error("This media source does not grant the page access to its audio.");
            if (!el.captureStream || !globalThis.MediaStreamTrackProcessor)
                throw Error("This browser cannot capture a playing media element.");
            const session = active = {
                element: el, src: el.currentSrc, requested: el.currentTime * 1000,
                events: new AbortController(), chunks: [], frames: 0, rate: 0, readOffset: 0,
                readChunk: 0, chunkOffset: 0
            };
            session.stream = el.captureStream();
            for (const t of session.stream.getVideoTracks()) t.stop();
            const tracks = session.stream.getAudioTracks().filter(t => t.enabled);
            if (tracks.length !== 1 || tracks[0].muted)
                throw Error("The player has no accessible, active audio track.");
            const changed = () => {
                if (active === session && !session.sealed)
                    cancel("Playback changed during the phrase. Play it again and mark a fresh start.");
            };
            for (const type of ["seeking","ratechange","pause","emptied","encrypted","waiting","ended"])
                el.addEventListener(type, changed, {signal: session.events.signal});
            for (const type of ["ended","mute"])
                tracks[0].addEventListener(type, changed, {signal: session.events.signal});
            session.processor = new MediaStreamTrackProcessor({track: tracks[0], maxBufferSize: 16});
            session.reader = session.processor.readable.getReader();
            return await new Promise(resolve => {
                session.ready = resolve;
                session.timer = setTimeout(() => cancel("The player supplied no recordable audio."), 2500);
                void pump(session);
            });
        } catch (error) {
            cancel(String(error.message || error));
            return {ok: false, error: lastError};
        }
    }

    async function markOut() {
        const s = active;
        if (!s || !s.rate) return {ok: false, error: lastError};
        if (s.sealed || s.finished) return {ok: false, error: "This passage already has an out mark."};
        if (!valid(s)) { cancel("Playback changed. Mark the passage again."); return {ok:false, error:lastError}; }
        s.end = Math.floor(s.element.currentTime * 1000);
        const frames = Math.floor(s.end * s.rate / 1000) - Math.floor(s.start * s.rate / 1000);
        if (frames <= 0) return {ok: false, error: "Let the phrase play past the in mark."};
        s.wanted = frames;
        return await new Promise(resolve => {
            s.finished = resolve;
            s.timer = setTimeout(() => cancel("The player stopped before the out mark."), 2000);
            finish(s);
        });
    }

    function read(offset, length) {
        const s = active;
        if (!s?.sealed || !Number.isSafeInteger(offset) || !Number.isSafeInteger(length) ||
            offset !== s.readOffset || length < 1 || length > maxRead ||
            length > s.wanted * s.channels * 2 - offset)
            return {ok: false, error: "Invalid phrase read."};
        const bytes = new Uint8Array(length);
        let index = 0;
        while (index < length) {
            const chunk = s.chunks[s.readChunk];
            const count = Math.min(length - index, chunk.length - s.chunkOffset);
            bytes.set(chunk.subarray(s.chunkOffset, s.chunkOffset + count), index);
            index += count;
            s.chunkOffset += count;
            if (s.chunkOffset === chunk.length) { ++s.readChunk; s.chunkOffset = 0; }
        }
        s.readOffset += length;
        let text = "";
        for (let i = 0; i < length; i += 8192)
            text += String.fromCharCode(...bytes.subarray(i, i + 8192));
        const data = btoa(text);
        if (s.readOffset === s.wanted * s.channels * 2) {
            cancel();
            sentPlayer = {ref: new WeakRef(s.element), src: s.src};
        }
        return {ok: true, data};
    }

    function pauseSentPlayer() {
        const player = sentPlayer;
        sentPlayer = null;
        const element = player?.ref.deref();
        if (element?.isConnected && element.currentSrc === player.src) element.pause();
        return {ok: true};
    }

    Object.defineProperty(globalThis, "__turnstonePractice", {
        value: Object.freeze({markIn, markOut, read, cancel, pauseSentPlayer}), configurable: false
    });
    addEventListener("pagehide", () => cancel("The page closed."));
    return true;
})()
