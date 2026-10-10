// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The plain viewer's mount script (mer3ly site canvas plan, Rulings 121, 124
// and 157): load the bundle, then mount every `<graphshell-tree>` on its host
// dataset or history (`data-dataset-src`, `?dataset=` or `data-dataset`). No
// scenario lane, no receipt harness; receipt pages load `loader.js` instead.
//
//   <graphshell-tree data-dataset-src="history.json"></graphshell-tree>
//   <script type="module" src="viewer.js"></script>
//
// A page that loads the viewer on first interaction imports this module then;
// elements added later mount when they connect.
import { defineElements } from "./mount.js";

const module = await import("./pkg/graphshell_web.js");
await module.default();
defineElements(module);
