// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The chrome's live sheet, in layout pixels. It is regenerated from the
/// shell-owned value snapshot so theme and zoom change the retained host
/// surface on the next redraw, without a provider or pane reaching into a
/// renderer; the window's layout scale is the layout's, not the sheet's.
pub(crate) fn chrome_sheet(appearance: &crate::shell_services::AppearanceConfig) -> String {
    let zoom = appearance.zoom();
    let (z11, z12, z13, z14, z16) = (
        11.0 * zoom,
        12.0 * zoom,
        13.0 * zoom,
        14.0 * zoom,
        16.0 * zoom,
    );
    let interface = cambium::ShellMetrics::INTERFACE_FONT_SIZE * zoom;
    let (menu_row, relation_row) = (22.0 + 6.0 * zoom, 38.0 + 6.0 * zoom);
    let title_bar = cambium::TITLE_BAR_CSS;
    let sheet = format!(
        "{title_bar} {CHROME_SHEET} \
        .turnstone-title-frame {{ color: var(--tabard-color-text-header); background-color: var(--tabard-color-surface); font-size: {z13:.2}px; }} \
        .cambium-title-bar {{ height:100%; overflow:hidden; }} \
        .turnstone-title-identity {{ display:flex; align-items:center; gap:10px; min-width:0; overflow:hidden; white-space:nowrap; }} \
        .turnstone-title-name {{ font-weight:600; }} \
        .turnstone-title-context {{ color: var(--tabard-color-text-dim); font-size:{z11:.2}px; }} \
        .turnstone-stone {{ display:block; position:relative; box-sizing:border-box; width:22px; height:18px; border:2px solid var(--tabard-color-primary); border-radius:7px 5px 8px 4px; transform:rotate(-16deg); }} \
        .turnstone-stone-vein {{ display:block; position:absolute; left:3px; top:6px; width:12px; height:2px; background-color:var(--tabard-color-text-dim); }} \
        .omni {{ background-color: var(--tabard-color-bg); border-color: var(--tabard-color-text-dim); }} \
        .omni-input {{ color: var(--tabard-color-text-header); background-color: var(--tabard-color-surface-2); font-size: {z16:.2}px; }} \
        .omni-input .field-preedit {{ color: var(--tabard-color-secondary); }} \
        .omni-row {{ color: var(--tabard-color-text); font-size: {z14:.2}px; }} \
        .omni-row-sel {{ color: var(--tabard-color-on-primary); background-color: var(--tabard-color-primary); font-size: {z14:.2}px; }} \
        .omni-row-muted {{ color: var(--tabard-color-text-dim); font-size: {z14:.2}px; }} \
        .whereami {{ color: var(--tabard-color-text-dim); background-color: var(--tabard-color-surface); border-color: var(--tabard-color-text-dim); font-size: {z12:.2}px; }} \
        .browser-strip, .link-preview, .document-find, .decision-card {{ color: var(--tabard-color-text); background-color: var(--tabard-color-surface); border-color: var(--tabard-color-text-dim); }} \
        .browser-loading {{ border-bottom-color: var(--tabard-color-primary); }} \
        .browser-button {{ color: var(--tabard-color-text-header); background-color: var(--tabard-color-surface-2); border-color: var(--tabard-color-text-dim); font-size: {z12:.2}px; }} \
        .browser-button-disabled, .browser-status, .link-preview, .document-find .read-result, .decision-status, .decision-error {{ font-size: {z12:.2}px; }} \
        .document-find .read-query, .decision-input {{ color: var(--tabard-color-text-header); background-color: var(--tabard-color-surface-2); font-size: {z13:.2}px; }} \
        .decision-prompt {{ color: var(--tabard-color-text-header); font-size: {z14:.2}px; }} \
        .decision-input-active {{ border-color: var(--tabard-color-primary); }}"
    );
    let controls = format!(
        ".turnstone-stone-hidden {{ display:none; }}
        .browser-strip, .whereami {{ --shell-background:var(--tabard-color-surface); --shell-foreground:var(--tabard-color-text); --shell-rule:var(--tabard-color-text-dim); }}
        .browser-strip {{ background-color:transparent; border:0; border-radius:0; gap:2px; padding:0; }}
        .shell-control, .browser-status {{ color:inherit; font-size:{interface:.2}px; }}
        .shell-control[aria-pressed=true] {{ background-color:var(--tabard-color-surface-2); color:var(--tabard-color-text-header); }}
        .browser-status {{ --app-region:no-drag; text-align:left; flex:1 1 0; min-width:0; border:1px solid var(--tabard-color-text-dim); border-radius:5px; background-color:var(--tabard-color-bg); color:var(--tabard-color-text); padding:4px 7px; overflow:hidden; white-space:nowrap; }}
        .chrome-menu-dismiss {{ --app-region:no-drag; background:transparent; }}
        .chrome-menu {{ --app-region:no-drag; display:flex; flex-direction:column; box-sizing:border-box; padding:8px; gap:0; background-color:var(--tabard-color-surface); color:var(--tabard-color-text); border:1px solid var(--tabard-color-text-dim); border-radius:6px; }}
        .chrome-menu-entry {{ width:100%; }}
        .chrome-menu-entry[data-menu-focus=true] {{ outline:1px solid currentColor; outline-offset:-1px; }}
        .chrome-menu .shell-control, .chrome-menu .shell-paged-range {{ text-align:left; width:100%; height:{menu_row:.2}px; overflow:hidden; white-space:nowrap; flex-shrink:0; }}
        .chrome-relations-menu .shell-navigation-item {{ height:auto; min-height:{relation_row:.2}px; }}
        .chrome-menu .shell-popup {{ padding:0; border:0; background:transparent; max-height:none; }}
        .chrome-panes-menu .shell-popup {{ flex-direction:row; flex-wrap:wrap; }}
        .chrome-panes-menu .chrome-menu-entry {{ width:50%; flex-basis:50%; }}"
    );
    themed_sheet(
        appearance,
        format!(
            "{sheet}\n{}\n{}\n{controls}",
            cambium::SHELL_CSS,
            cambium::GRAPH_CANVAS_SWATCH_CSS
        ),
    )
}
