// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The transport for edits a following view hands to the view that holds the
// writer lock (application experience plan, "Followers queue edits"). It has
// no DOM and no clock of its own: the page gives it a way to post on the
// views channel, to schedule, and to hand the Rust app what arrives. The Rust
// side (`follower_edit`) decides whether an edit is admitted.
//
// Channel messages, all on the views channel:
//   follower -> all   { type: "edit", request }          request = follower_edit::Request
//   writer   -> all   { type: "edit-received", id }      the writer has it
//   writer   -> all   { type: "edit-reply", reply }      reply = follower_edit::Reply
//   writer   -> all   { type: "writer-ready" }           a writer began serving
(function () {
  // A writer that has not acknowledged a request within this long may not have
  // heard it (it is between pages); the follower sends once more.
  const ACK_MS = 4000;
  // An acknowledged request not answered within this long is given up on.
  const REPLY_MS = 15000;
  // A request handed to the app and not answered by now is handed over again
  // if it arrives again: the app may not have been listening yet.
  const ADMIT_STALE_MS = 10000;

  const unanswered = (id) => ({
    id,
    view: "",
    status: "refused",
    reason: "No answer from the view that edits. A view in a hidden tab applies edits when it is shown; check the graph before trying again",
  });

  // A follower's side. `post` sends on the channel, `schedule(run, ms)` returns
  // a handle for `cancel`, `deliver(reply)` shows an answer to the app.
  //
  // A request is sent once when queued, once to each writer that announces
  // itself afterwards (a handover or takeover mid-request), and once more if
  // nothing acknowledged it. Duplicates are harmless: the writer answers an id
  // once. When the last wait runs out the request is answered as refused.
  function createSender({ post, schedule, cancel, deliver }) {
    const open = new Map();
    let writers = 0;

    function send(entry) {
      entry.sentAt = writers;
      post({ type: "edit", request: entry.request });
    }
    function wait(entry, ms) {
      if (entry.timer) cancel(entry.timer);
      entry.timer = schedule(() => expire(entry.request.id), ms);
    }
    function finish(id, reply) {
      const entry = open.get(id);
      if (!entry) return;
      if (entry.timer) cancel(entry.timer);
      open.delete(id);
      deliver(reply);
    }
    function expire(id) {
      const entry = open.get(id);
      if (!entry) return;
      entry.timer = null;
      if (!entry.timeoutResent) {
        entry.timeoutResent = true;
        send(entry);
        wait(entry, REPLY_MS);
      } else {
        finish(id, unanswered(id));
      }
    }

    return {
      submit(request) {
        if (open.has(request.id)) return;
        const entry = { request, timeoutResent: false, sentAt: writers, timer: null };
        open.set(request.id, entry);
        send(entry);
        wait(entry, ACK_MS);
      },
      onMessage(data) {
        if (data?.type === "edit-received") {
          const entry = open.get(data.id);
          if (entry) wait(entry, REPLY_MS);
        } else if (data?.type === "edit-reply" && data.reply) {
          finish(data.reply.id, data.reply);
        } else if (data?.type === "writer-ready") {
          writers += 1;
          for (const entry of open.values()) {
            send(entry);
            wait(entry, REPLY_MS);
          }
        }
      },
      // This view stops following (it took the writer): nothing more is sent.
      // Answers the number of edits left unanswered, which this view's new
      // writer cannot admit: they were made against the old writer's session.
      abandon() {
        const left = open.size;
        for (const entry of open.values()) if (entry.timer) cancel(entry.timer);
        open.clear();
        return left;
      },
      pending: () => open.size,
    };
  }

  // The writer's side. `admit(json)` hands a request to the Rust app, which
  // answers it later through `answered(json)`.
  function createReceiver({ post, admit, now }) {
    const inflight = new Map();
    return {
      onMessage(data) {
        if (data?.type !== "edit") return;
        const request = data.request;
        if (!request || typeof request.id !== "string" || !request.id) return;
        post({ type: "edit-received", id: request.id });
        const since = inflight.get(request.id);
        if (since !== undefined && now() - since < ADMIT_STALE_MS) return;
        inflight.set(request.id, now());
        admit(JSON.stringify(request));
      },
      answered(json) {
        let reply;
        try {
          reply = JSON.parse(json);
        } catch {
          return;
        }
        inflight.delete(reply.id);
        post({ type: "edit-reply", reply });
      },
      // An edit is being stored: this view must not reload for a handover.
      busy: () => inflight.size > 0,
      announce: () => post({ type: "writer-ready" }),
    };
  }

  globalThis.GraphshellFollowerQueue = { createSender, createReceiver };
})();
