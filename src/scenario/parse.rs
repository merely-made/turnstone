// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn parse(body: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (i, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = expand_env(line, i + 1)?;
        let line = line.as_str();
        let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let err = |msg: &str| Err(format!("line {}: {msg}: '{line}'", i + 1));
        steps.push(match verb {
            "open" if !rest.is_empty() => Step::Open(rest.to_string()),
            "import-clip" if !rest.is_empty() => Step::ImportClip(rest.to_string()),
            "export-clip" if !rest.is_empty() => Step::ExportClip(rest.to_string()),
            "omnibar" => match rest {
                "find" => Step::Omnibar { command: false },
                "actions" => Step::Omnibar { command: true },
                _ => return err("omnibar wants find|actions"),
            },
            "type" if !rest.is_empty() => Step::Type(rest.to_string()),
            "insert" if !rest.is_empty() => Step::Insert(rest.to_string()),
            "key-window" => {
                let (window, key) = rest.split_once(char::is_whitespace).ok_or_else(|| format!("line {}: key-window wants '<slot> <key>'", i+1))?;
                let window = window.parse().map_err(|_| format!("line {}: invalid window slot", i+1))?;
                let mut key = parse(&format!("key {}", key.trim()))?;
                let Some(Step::Key(key)) = key.pop() else { return err("unsupported window key"); };
                Step::KeyWindow(window, key)
            },
            "key" => match rest {
                "enter" => Step::Key(EditKey::Enter),
                "escape" => Step::Key(EditKey::Escape),
                "tab" => Step::Key(EditKey::Tab),
                "shift+tab" => Step::Key(EditKey::BackTab),
                "ctrl+f6" => Step::Key(EditKey::FrameFocus),
                "ctrl+w" => Step::Key(EditKey::FrameClose),
                "backspace" => Step::Key(EditKey::Backspace),
                "delete" => Step::Key(EditKey::Delete),
                "up" => Step::Key(EditKey::Up),
                "down" => Step::Key(EditKey::Down),
                "left" => Step::Key(EditKey::Left),
                "right" => Step::Key(EditKey::Right),
                "shift+right" => Step::Key(EditKey::ShiftRight),
                "shift+left" => Step::Key(EditKey::ShiftLeft),
                "shift+enter" => Step::Key(EditKey::ShiftEnter),
                "ctrl+a" => Step::Key(EditKey::SelectAll),
                "home" => Step::Key(EditKey::Home),
                "end" => Step::Key(EditKey::End),
                "pagedown" => Step::Key(EditKey::PageDown),
                "pageup" => Step::Key(EditKey::PageUp),
                "space" => Step::Key(EditKey::Space),
                "ctrl+s" => Step::Key(EditKey::Save),
                "ctrl+f" => Step::Key(EditKey::Find),
                "ctrl+d" => Step::Key(EditKey::KeepCommand),
                _ => {
                    return err(
                        "key wants enter|escape|tab|backspace|delete|up|down|left|right|home|end|pagedown|pageup|space|shift+right|shift+left|shift+enter|ctrl+a|ctrl+s|ctrl+f|ctrl+d",
                    );
                }
            },
            "act" if !rest.is_empty() => Step::Act(rest.to_string()),
            "script" if !rest.is_empty() => Step::Script(rest.to_string()),
            "settle" => Step::Settle(if rest.is_empty() {
                20
            } else {
                rest.parse().map_err(|_| format!("line {}: bad settle count '{rest}'", i + 1))?
            }),
            "click-row" if !rest.is_empty() => Step::ClickRow(rest.to_string()),
            "drop-file" | "hover-file" => {
                let mut it = rest.splitn(3, char::is_whitespace);
                let x = it.next().and_then(|t| t.parse().ok());
                let y = it.next().and_then(|t| t.parse().ok());
                let path = it.next().map(str::trim).filter(|p| !p.is_empty());
                match (x, y, path) {
                    (Some(x), Some(y), Some(path)) if verb == "drop-file" => {
                        Step::DropFile(x, y, path.to_string())
                    }
                    (Some(x), Some(y), Some(path)) => {
                        Step::HoverFile(x, y, path.to_string())
                    }
                    _ => return err(&format!("{verb} wants '<x> <y> <path>'")),
                }
            }
            "drag-tab" => {
                // `drag-tab <a> out` releases OUTSIDE the workbench pane (on
                // the canvas): the tear-out trichotomy's branch arm.
                if let Some(from) = rest
                    .strip_suffix(" out")
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                {
                    Step::DragTabOut(from.to_string())
                } else {
                    let (from, onto) = rest
                        .split_once(" onto ")
                        .map(|(a, b)| (a.trim(), b.trim()))
                        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
                        .ok_or_else(|| {
                            format!("line {}: drag-tab wants '<a> onto <b>': '{line}'", i + 1)
                        })?;
                    let (onto, edge) = match onto.split_once(" @ ") {
                        Some((b, e)) => {
                            let e = e.trim();
                            if !matches!(e, "left" | "right" | "top" | "bottom") {
                                return err("drag-tab edge wants left|right|top|bottom");
                            }
                            (b.trim(), Some(e.to_string()))
                        }
                        None => (onto, None),
                    };
                    Step::DragTab(from.to_string(), onto.to_string(), edge)
                }
            }
            "click-tab" if !rest.is_empty() => Step::ClickTab(rest.to_string()),
            "click-node" if !rest.is_empty() => Step::ClickNode(rest.to_string()),
            "click-reading-reference" if !rest.is_empty() => Step::ClickReadingReference(rest.to_string()),
            "drag" => {
                let nums: Vec<f32> = rest
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                match nums[..] {
                    [x1, y1, x2, y2] => Step::Drag((x1, y1), (x2, y2)),
                    _ => return err("drag wants 'x1 y1 x2 y2'"),
                }
            }
            "click-at" => {
                let (x, y) = parse_xy(rest).ok_or_else(|| {
                    format!("line {}: click-at wants '<x> <y>': '{line}'", i + 1)
                })?;
                Step::Click(x, y)
            }
            "click-content-window" => {
                let values: Vec<_> = rest.split_whitespace().collect();
                if values.len() != 3 {
                    return err("click-content-window wants '<window> <x> <y>'");
                }
                let window = values[0].parse::<usize>();
                let x = values[1].parse::<f32>();
                let y = values[2].parse::<f32>();
                match (window, x, y) {
                    (Ok(window), Ok(x), Ok(y)) if x.is_finite() && y.is_finite() =>
                        Step::ClickContentWindow(window, x, y),
                    _ => return err("click-content-window wants a window index and finite coordinates"),
                }
            }
            "press-at" => {
                let (x, y) = parse_xy(rest).ok_or_else(|| {
                    format!("line {}: press-at wants '<x> <y>': '{line}'", i + 1)
                })?;
                Step::Press(x, y)
            }
            "release-at" => {
                let (x, y) = parse_xy(rest).ok_or_else(|| {
                    format!("line {}: release-at wants '<x> <y>': '{line}'", i + 1)
                })?;
                Step::Release(x, y)
            }
            "move-at" => {
                let (x, y) = parse_xy(rest).ok_or_else(|| {
                    format!("line {}: move-at wants '<x> <y>': '{line}'", i + 1)
                })?;
                Step::Move(x, y)
            }
            "touch-at" => {
                let mut fields = rest.split_whitespace();
                let id = fields.next().and_then(|value| value.parse().ok());
                let phase = match fields.next() {
                    Some("down") => Some(inker::PointerPhase::Down),
                    Some("move") => Some(inker::PointerPhase::Move),
                    Some("up") => Some(inker::PointerPhase::Up),
                    Some("cancel") => Some(inker::PointerPhase::Cancel),
                    _ => None,
                };
                let x = fields.next().and_then(|value| value.parse().ok());
                let y = fields.next().and_then(|value| value.parse().ok());
                let pressure = fields.next().map(str::parse).transpose().map_err(|_| {
                    format!("line {}: touch-at pressure must be a number: '{line}'", i + 1)
                })?;
                if fields.next().is_some() {
                    return err("touch-at has too many fields");
                }
                match (id, phase, x, y) {
                    (Some(id), Some(phase), Some(x), Some(y)) => {
                        Step::Touch(id, phase, x, y, pressure)
                    }
                    _ => {
                        return err(
                            "touch-at wants '<id> down|move|up|cancel <x> <y> [pressure]'",
                        );
                    }
                }
            }
            "right-click-at" => {
                let (x, y) = parse_xy(rest).ok_or_else(|| {
                    format!("line {}: right-click-at wants '<x> <y>': '{line}'", i + 1)
                })?;
                Step::RightClick(x, y)
            }
            "scroll" => {
                let mut it = rest.split_whitespace();
                let vals: Vec<f32> = it.by_ref().filter_map(|t| t.parse().ok()).collect();
                match vals.as_slice() {
                    // `scroll <x> <y> <dy>` (horizontal delta defaults to 0).
                    [x, y, dy] => Step::Scroll(*x, *y, 0.0, *dy),
                    [x, y, dx, dy] => Step::Scroll(*x, *y, *dx, *dy),
                    _ => return err("scroll wants '<x> <y> <dy>' or '<x> <y> <dx> <dy>'"),
                }
            }
            "scroll-reader" => {
                let mut fields = rest.split_whitespace();
                let role = fields.next();
                let dy = fields.next().and_then(|value| value.parse().ok());
                if fields.next().is_some() || role.is_none() || dy.is_none() {
                    return err("scroll-reader wants '<inset|workbench|tile> <dy>'");
                }
                Step::ScrollReader(role.unwrap().to_string(), dy.unwrap())
            }
            "divider" => {
                let ratio = rest
                    .parse()
                    .map_err(|_| format!("line {}: divider wants a ratio: '{line}'", i + 1))?;
                Step::Divider(ratio)
            }
            "capture" if !rest.is_empty() => Step::Capture(rest.to_string()),
            "captures-differ" => {
                let names: Vec<_> = rest.split_whitespace().collect();
                if names.len() != 2 { return err("captures-differ wants two completed capture names"); }
                Step::CapturesDiffer(names[0].into(), names[1].into())
            }
            "scroll-pane-window" => {
                let fields: Vec<_> = rest.split_whitespace().collect();
                if fields.len() != 5 { return err("scroll-pane-window wants window x y dx dy"); }
                let window = fields[0].parse().map_err(|_| format!("line {}: invalid window", i + 1))?;
                let values: Vec<f32> = fields[1..].iter().map(|value| value.parse::<f32>())
                    .collect::<Result<_, _>>().map_err(|_| format!("line {}: invalid pane wheel", i + 1))?;
                if values.iter().any(|value| !value.is_finite()) { return err("pane wheel values must be finite"); }
                Step::ScrollPaneWindow(window, values[0], values[1], values[2], values[3])
            }
            "capture-lens" if !rest.is_empty() => Step::CaptureLens(rest.to_string()),
            "record-reader-appearances" if !rest.is_empty() => {
                Step::RecordReaderAppearances(rest.to_string())
            }
            "record-idle" if !rest.is_empty() => Step::RecordIdle(rest.to_string()),
            "select-graph-member" if !rest.is_empty() => Step::SelectGraphMember(rest.to_string()),
            "activate-graph-fold" => Step::ActivateGraphFold(rest.parse().map_err(|_| format!("line {}: fold index required", i + 1))?),
            "double-click-node" if !rest.is_empty() => Step::DoubleClickGraphNode(rest.to_string()),
            "record-graph-scene" if !rest.is_empty() => {
                if rest.contains(['/', '\\']) { return err("simple checkpoint name required"); }
                Step::RecordGraphScene(rest.into())
            },
            "record-graph-folds" => {
                let (name, count) = rest.split_once(' ').ok_or_else(|| format!("line {}: name and fold count required", i + 1))?;
                if name.contains(['/', '\\']) { return err("simple checkpoint name required"); }
                Step::RecordGraphFolds(name.into(), count.parse().map_err(|_| format!("line {}: invalid fold count", i + 1))?)
            },
            "assert" => {
                let (what, arg) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                let arg = arg.trim();
                match what {
                    "omnibar" => match arg {
                        "open" => Step::AssertOmnibar(true),
                        "closed" => Step::AssertOmnibar(false),
                        _ => return err("assert omnibar wants open|closed"),
                    },
                    "omnibar-text" => Step::AssertText(arg.to_string()),
                    "event" if !arg.is_empty() => Step::AssertEvent(arg.to_string()),
                    "focused" if !arg.is_empty() => Step::AssertFocused(arg.to_string()),
                    "fetch" if !arg.is_empty() => Step::AssertBrowserFetch(arg.to_string()),
                    "link-preview" if !arg.is_empty() => Step::AssertLinkPreview(arg.to_string()),
                    "surface" if !arg.is_empty() => Step::AssertSurface(arg.to_string()),
                    "focus" if !arg.is_empty() => Step::AssertFocus(arg.to_string()),
                    "pane" if !arg.is_empty() => Step::AssertPane(arg.to_string()),
                    "maximized" => Step::AssertMaximized(true),
                    "not-maximized" => Step::AssertMaximized(false),
                    "scrolled" => match arg {
                        "moved" => Step::AssertScrolled(true),
                        "still" => Step::AssertScrolled(false),
                        _ => return err("assert scrolled wants moved|still"),
                    },
                    "row" if !arg.is_empty() => Step::AssertRow(arg.to_string()),
                    "no-row" if !arg.is_empty() => Step::AssertNoRow(arg.to_string()),
                    "tab" if !arg.is_empty() => Step::AssertTab(arg.to_string()),
                    "a11y" if !arg.is_empty() => Step::AssertA11y(arg.to_string()),
                    "lens-pane" if !arg.is_empty() => Step::AssertLensPane(arg.to_string()),
                    "lens-surface" if !arg.is_empty() => {
                        Step::AssertLensSurface(arg.to_string())
                    }
                    "no-pane" if !arg.is_empty() => Step::AssertNoPane(arg.to_string()),
                    "no-lens-pane" if !arg.is_empty() => {
                        Step::AssertNoLensPane(arg.to_string())
                    }
                    "no-surface" if !arg.is_empty() => Step::AssertNoSurface(arg.to_string()),
                    "reader-role" if !arg.is_empty() => Step::AssertReaderRole(arg.to_string()),
                    "reader-viewport" if !arg.is_empty() => {
                        Step::AssertReaderViewport(arg.to_string())
                    }
                    "reader-scroll" => {
                        let mut fields = arg.split_whitespace();
                        let role = fields.next();
                        let op = fields.next();
                        let amount = fields.next().and_then(|value| value.parse().ok());
                        if fields.next().is_some() || role.is_none() || op.is_none() || amount.is_none() {
                            return err("assert reader-scroll wants '<role> ==|>=|<= <px>'");
                        }
                        let op = match op.unwrap() {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert reader-scroll op wants ==|>=|<="),
                        };
                        Step::AssertReaderScroll(role.unwrap().to_string(), op, amount.unwrap())
                    }
                    "content-sessions-idle" if arg.is_empty() => Step::AssertContentSessionsIdle,
                    "content-ready" if arg.is_empty() => Step::AssertContentReady,
                    "reader-appearances" | "reader-sources" => {
                        let (op, n) = arg.split_once(char::is_whitespace).ok_or_else(|| {
                            format!("line {}: assert {what} wants '<op> <n>'", i + 1)
                        })?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert reader count op wants ==|>=|<="),
                        };
                        let n = n.trim().parse().map_err(|_| {
                            format!("line {}: bad {what} count", i + 1)
                        })?;
                        if what == "reader-appearances" {
                            Step::AssertReaderAppearances(op, n)
                        } else {
                            Step::AssertReaderSources(op, n)
                        }
                    }
                    "windows" | "sessions" | "nodes" | "overlaps" => {
                        let (op, n) = arg.split_once(char::is_whitespace).ok_or_else(|| {
                            format!("line {}: assert {what} wants '<op> <n>'", i + 1)
                        })?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert count op wants ==|>=|<="),
                        };
                        let n = n
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad {what} count", i + 1))?;
                        match what {
                            "overlaps" => Step::AssertOverlaps(op, n),
                            "sessions" => Step::AssertSessions(op, n),
                            "nodes" => Step::AssertNodes(op, n),
                            _ => Step::AssertWindows(op, n),
                        }
                    }
                    "session" if !arg.is_empty() => Step::AssertSession(arg.to_string()),
                    "wb-cell" if !arg.is_empty() => Step::AssertWbCell(arg.to_string()),
                    "wb-cells" => {
                        let (op, n) = arg
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| format!("line {}: assert wb-cells wants '<op> <n>'", i + 1))?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert wb-cells op wants ==|>=|<="),
                        };
                        let n = n
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad wb-cells count", i + 1))?;
                        Step::AssertWbCells(op, n)
                    }
                    "wb-fraction" => {
                        let (op, f) = arg
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| format!("line {}: assert wb-fraction wants '<op> <f>'", i + 1))?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert wb-fraction op wants ==|>=|<="),
                        };
                        let f = f
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad wb-fraction", i + 1))?;
                        Step::AssertWbFraction(op, f)
                    }
                    "ratio" | "active-ratio" => {
                        let (op, n) = arg
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| format!("line {}: assert ratio wants '<op> <r>'", i + 1))?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert ratio op wants ==|>=|<="),
                        };
                        let n = n
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad ratio", i + 1))?;
                        if what == "active-ratio" {
                            Step::AssertActiveRatio(op, n)
                        } else {
                            Step::AssertRatio(op, n)
                        }
                    }
                    "physics" => match arg {
                        "running" => Step::AssertPhysicsRunning(true),
                        "paused" => Step::AssertPhysicsRunning(false),
                        _ => return err("assert physics wants running|paused"),
                    },
                    "energy" => {
                        let (op, n) = arg.split_once(char::is_whitespace).ok_or_else(|| {
                            format!("line {}: assert energy wants '<op> <n>'", i + 1)
                        })?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert energy op wants ==|>=|<="),
                        };
                        let n = n
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad energy", i + 1))?;
                        Step::AssertEnergy(op, n)
                    }
                    "suggestions" => {
                        let (op, n) = arg
                            .split_once(char::is_whitespace)
                            .ok_or_else(|| format!("line {}: assert suggestions wants '<op> <n>'", i + 1))?;
                        let op = match op {
                            "==" => CmpOp::Eq,
                            ">=" => CmpOp::Ge,
                            "<=" => CmpOp::Le,
                            _ => return err("assert suggestions op wants ==|>=|<="),
                        };
                        let n = n
                            .trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad suggestion count", i + 1))?;
                        Step::AssertSuggestions(op, n)
                    }
                    "visible" => Step::AssertVisible,
                    "content-live" => Step::AssertContentLive,
                    _ => return err("unknown assert"),
                }
            }
            "wait-row" | "wait-status" | "wait-file" | "wait-knot" => {
                let (frames, arg) = rest.split_once(char::is_whitespace).ok_or_else(|| {
                    format!("line {}: {verb} wants '<frames> <substr>'", i + 1)
                })?;
                let frames: u32 = frames.trim().parse().map_err(|_| {
                    format!("line {}: bad {verb} frame count '{frames}'", i + 1)
                })?;
                let arg = arg.trim();
                if arg.is_empty() {
                    return err(&format!("{verb} wants '<frames> <substr>'"));
                }
                match verb {
                    "wait-row" => Step::WaitRow(frames, arg.to_string()),
                    "wait-status" => Step::WaitStatus(frames, arg.to_string()),
                    "wait-file" => Step::WaitFile(frames, arg.to_string()),
                    _ => Step::WaitKnot(frames, arg.to_string()),
                }
            }
            "wait-source-capture" => {
                let mut fields = rest.split_whitespace();
                let frames = fields.next().and_then(|value| value.parse().ok());
                let ordinal = fields.next().and_then(|value| value.parse().ok());
                let phase = fields.next().filter(|value| matches!(*value, "depositing" | "deposited" | "pending-save" | "saved" | "failed" | "recoverable"));
                if fields.next().is_some() || frames.is_none() || ordinal.is_none() || phase.is_none() {
                    return err("wait-source-capture wants '<frames> <ordinal> <depositing|deposited|pending-save|saved|failed|recoverable>'");
                }
                Step::WaitSourceCapture(frames.unwrap(), ordinal.unwrap(), phase.unwrap().to_string())
            }
            "wait-saved-sources" | "wait-saved-reader" => {
                let frames = rest.trim().parse::<u32>().map_err(|_| {
                    format!("line {}: {verb} wants '<frames>'", i + 1)
                })?;
                match verb {
                    "wait-saved-sources" => Step::WaitSavedSources(frames),
                    _ => Step::WaitSavedReader(frames),
                }
            }
            "record-place" if !rest.is_empty() => Step::RecordPlace(rest.to_string()),
            "record-source-captures" if !rest.is_empty() => {
                Step::RecordSourceCaptures(rest.to_string())
            }
            "record-saved-sources" if !rest.is_empty() => {
                Step::RecordSavedSources(rest.to_string())
            }
            "queue-act" if !rest.is_empty() => Step::QueueAct(rest.to_string()),
            "deliver-act" if rest.is_empty() => Step::DeliverAct,
            "touch" if !rest.is_empty() => Step::TouchFile(rest.to_string()),
            "log" => Step::Log(rest.to_string()),
            _ => return err("unknown verb"),
        });
    }
    Ok(steps)
}
