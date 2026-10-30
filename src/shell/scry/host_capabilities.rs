// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn host_capabilities(mut caps: WebSurfaceCapabilities) -> WebSurfaceCapabilities {
    caps.pointer.mouse = projected_event_capability(
        caps.pointer.mouse,
        "mouse buttons and Shift/Control are forwarded; WebView2 MouseVirtualKeys has no Alt/Meta state",
    );
    caps.document.find_in_page = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not forward document find commands or results",
    );
    caps.document.page_zoom = WebFeatureStatus::unsupported(
        "Scry's Inker adapter forwards legacy zoom settings but not typed page zoom or its outcome",
    );
    caps.document.page_capture = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not implement correlated page capture",
    );
    caps.pdf = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not expose PDF export commands or completions",
    );
    caps.devtools = projected_event_capability(
        caps.devtools,
        "legacy settings toggle DevTools availability; opening DevTools is not forwarded",
    );
    caps.downloads = projected_event_capability(
        caps.downloads,
        "download requests are projected; destination decisions, lifecycle and controls are not",
    );
    caps.popups = projected_event_capability(
        caps.popups,
        "new-window requests are projected; popup-widget surfaces are not",
    );
    caps.permissions = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not retain permission requests or forward their answers",
    );
    caps.auth = WebFeatureStatus::unsupported(
        "Scry's Inker adapter emits authentication diagnostics but does not forward credential answers",
    );
    caps.context_menus = projected_event_capability(
        caps.context_menus,
        "context-menu position and link/image targets are projected; menu items and decisions are not",
    );
    caps.ime_observability = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not project composition or caret geometry",
    );
    caps.accessibility = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not project an accessibility tree",
    );
    caps.pointer.pen = projected_event_capability(
        caps.pointer.pen,
        "winit 0.30 does not identify pen versus touch in Turnstone's native input path",
    );
    caps.degradation_reasons.push(
        "Turnstone uses captured textures on an owned offscreen HWND; keyboard/text is forwarded through CDP, native OS IME and native rehosting are not integrated".into(),
    );
    caps
}
