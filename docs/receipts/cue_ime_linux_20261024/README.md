# ThinkPad physical IME receiving, 2026-10-24

**Reviewed result: bounded physical receiving passes for IBus Typing Booster.**
The operator physically typed, selected candidates, cancelled composition and
switched focus in Turnstone's address field, retained Genet document, document
lens and contributed Sky field. All four now have positive operator checklists
and native winit composition observations. This receipt spans two executables;
it does not claim one complete four-stage run on the repaired executable.

| Physical path | Executable | Nonempty native preedit events | Native commits | Operator checklist |
| --- | --- | ---: | ---: | --- |
| Address field | A | 478 | 2 | All seven checks passed |
| Genet document, primary | A | 29 | 2 | All seven checks passed |
| Genet document, lens window 1 | A | 34 | 3 | All seven checks passed |
| Sky Observer label, after repair | B | 65 | 3 | All seven checks passed |

Event counts are notifications, including repeated preedit updates, rather than
counts of physical keystrokes. The checklists supply the physical provenance
and observations of the OS candidate popup. Composition exercises contain no
scenario text injection. Document and lens captures retain the committed
synthetic text. The Sky capture retains its changed value after the repair.
The address capture was taken after the operator's cleanup and is empty; its
positive composition evidence is the operator report and native events.

## Source and executable custody

- A: SHA-256 `57ad069a97511ba38a5417d0ac37550d5bf424acd47eefc891ccb6bf71c1a263`,
  based on `0c6f5997bb1a483c3ea9c3d2e438489893b8976c` plus
  [the initial source patch](initial/source.patch).
- B: SHA-256 `ee27bcacba3278100d6d816091eaf907b1a2ad9a38d9a09b65ab7314441b31e7`,
  built from the repaired source published in
  [`16df175`](https://github.com/merely-made/turnstone/commit/16df1756e343c2ee51faa1fd39e76100ed1872d9).
  Its original [source patch](sky-repair/source.patch) and the
  [19-file byte comparison](sky-repair/published-adoption.json) retain the
  relationship between the tested working tree and published source.

Raw logs, copied executables and disposable profiles remain private under
`Code/output/cue-ime-linux-20261024` on the Mac and ThinkPad. The reviewed JSON
records their hashes. Earlier sealed Linux receipts were not changed.

## Preserved failures and their resolution

The original scenarios retain `RESULT fail`. The first used unsupported
`key ctrl+l`; the Sky retest used a setup wheel step that cannot target a
contributed pane. Each physical exercise ran after its setup error. Those
failures remain in the original outcomes and run JSON. They are not reported
as globally passing scenarios.

The first collector also read stderr while tracing wrote native observations
to stdout. [Initial review](initial/review.json) correlates the intact UTC
stdout timestamps with the original ready/done marker times and native window
IDs. The corrected collector reads stdout. [Sky review](sky-repair/review.json)
preserves its positive stage observations separately from the failed setup.
Removing the erroneous wheel step gives a separate
[passing automated setup check](setup-smoke/run.json), using executable B and
79 presented frames. That check does not repeat or qualify physical typing.

## Repairs and bounds

The host now enables OS IME for its focused retained document and contributed
editor, synchronizes focus admission, uses contributed caret geometry and
requests lens redraws for handled composition. Sky fields now have legible
text, borders, caret and preedit styling. Original input sources, source
history, Typing Booster recording setting and US engine were restored; the
current session was also checked with the US source and engine selected.

[Focused validation](validation.json) records the 61 Linux input regressions,
Sky composition tests at two scales on Linux and macOS, scenario grammar and
the Servo-enabled Linux build. Collector tests also pass. No full library-suite
rerun is claimed.

This qualifies the tested Latin Typing Booster exercises on native GNOME
Wayland and the ThinkPad Vulkan host. Candidate visibility and placement are
operator observations; app captures exclude OS popup pixels and asynchronous
captures do not prove exact preedit timing. Genet still lacks shared
`TextInput` geometry publication. Physical CJK methods, Servo IME, X11,
human accessibility and physical device loss retain separate bounds.
