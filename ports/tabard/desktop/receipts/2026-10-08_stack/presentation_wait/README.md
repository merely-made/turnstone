# Native presentation investigation, 2026-10-08

The shared host now exposes each redraw attempt's successful PresentedFrame identity. Mesquite advances native scenario steps, settle counts, frame limits, and capture grace only for new presentations, while polling pending asynchronous readback on every turn. Continuous unpresented native work has a separate 10-second elapsed deadline. JSON and text receipts distinguish presentations, redraw attempts, and unpresented redraws.

Native attempts below ran tabard-desktop with WindowFrame::App at 1180x800 against either the saved authored library or a fresh absent library. All processes exited unsuccessfully with zero presentations and zero captures; no native screenshot was produced. Scenario actions and assertions did not advance without a presentation. These results do not replace the earlier successful 95-frame, five-capture usability receipt at /tmp/tabard-stack-native-20261008.

| Attempt | Receipt | Presentations | Redraws | Deadline |
| --- | --- | ---: | ---: | ---: |
| Saved authored library | saved-presentation.done | 0 | 201 | 10000 ms |
| Fresh absent library | fresh-presentation.done | 0 | 218 | 10000 ms |
| Saved native-state diagnostic | diagnostic-presentation.done | 0 | 129 | 10000 ms |

Companion JSON files are explicit structured projections of the text receipts, not separately generated scenario outcomes. Logs retain the first and last 24 lines, mark omitted repetitions and preserve the full original log SHA256. The redraw counts are Mesquite after_frame turns; the initial hidden synchronous redraw precedes that hook. All three diagnostic binaries temporarily attempted focus_window once after initial a11y reveal. The attempt did not activate the app or change native visibility, and that ineffective behavior was removed from the final source. The final source retains an explicit first-show redraw request and opt-in owned-window diagnostics under CAMBIUM_HOST_FRAME_TRACE=1.

The owned macOS window reported Regular activation policy, finished launching, app not hidden, window visible, canBecomeKeyWindow=true, onActiveSpace=true, not minimized, and a valid 1180x800 frame at native origin (690, 320). App active=false and key window=false persisted after focus_window. NSWindow occlusionState was 8192 throughout, which does not include the Visible bit (2). wgpu-hal 30's Metal surface acquire checks that same Visible bit before nextDrawable and returns Occluded when absent. The remaining fresh-process presentation is therefore blocked at the native activation/compositor boundary; no stylesheet or product-state failure is demonstrated. No external native UI workaround was used.

Saved authored library SHA256 stayed ecb9fd0047ff229b58f436ddda7ef015143f83f2eb919a1cd448dffecf0e4f62. Sidecar SHA256 after read-only diagnostics: 784f3a3a5c330bbd4a3f56d648a56e10a0e5ae1df9eed8c94ada9c510e1f0452.

Validation passed: 69 Rootstock unit tests, 24 Mesquite unit tests, 18 native-host library tests, one windowless asynchronous capture regression and three stamped-presentation capture regressions. Scoped rustfmt and git diff checks passed.

The successful-presentation unit tests cover unavailable/repeated native turns, capture grace remaining intact after 150 unpresented turns, delayed readback during occlusion, cancellation/exit at the independent deadline, startup failure before any presentation, and clearing stale host presentation identity. Existing windowless async capture and stamped presentation pairing regressions also pass.
