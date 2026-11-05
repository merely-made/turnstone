// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! User-configurable offers for handing an address to another application.

use chirograph::{AdvertisedAction, IntentEffect, IntentReference, PresentationSemantics};
use mere::kernel::address::AddressKind;
use serde::{Deserialize, Serialize};

pub const OPEN_ADDRESS_INTENT: &str = "graphshell.address.open";
pub const OPEN_ADDRESS_SCHEMA: &str = "graphshell.address.open/v1";
pub const SYSTEM_DEFAULT_HANDLER: &str = "system.default";
pub const INSPECT_HANDLER: &str = "graphshell.inspect";
pub const ATTACHED_BROWSER_HANDLER: &str = "browser.attached";

pub fn inspect_handler() -> HandlerOffer {
    HandlerOffer {
        id: INSPECT_HANDLER.into(),
        label: "Inspect in Graphshell".into(),
        explanation: "Keep the address in the graph portal.".into(),
        address_kinds: vec![
            AddressKind::Http,
            AddressKind::File,
            AddressKind::Data,
            AddressKind::GraphshellClip,
            AddressKind::Directory,
            AddressKind::Unknown,
        ],
    }
}

/// Opens a web address in the browser this surface is attached to, through
/// that browser's own tab API. No other application is launched.
pub fn attached_browser_handler() -> HandlerOffer {
    HandlerOffer {
        id: ATTACHED_BROWSER_HANDLER.into(),
        label: "Open in this browser".into(),
        explanation: "Open this web address in a new tab of the browser Graphshell runs in.".into(),
        address_kinds: vec![AddressKind::Http],
    }
}

/// The installed browser profile's offers. Native dispatch
/// ([`SYSTEM_DEFAULT_HANDLER`]) arrives only with an admitted native host.
pub fn local_profile_handlers() -> HandlerRegistry {
    HandlerRegistry::new(vec![inspect_handler(), attached_browser_handler()])
}

/// The address the attached browser may open: HTTP(S) with a host. Other
/// schemes are refused here, never passed on to a shell or another handler.
pub fn attached_browser_target(address: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(address).map_err(|error| format!("invalid address: {error}"))?;
    if matches!(url.scheme(), "http" | "https") && url.host_str().is_some() {
        Ok(url)
    } else {
        Err(format!(
            "this browser opens web addresses only; {}: needs a reader that is not attached",
            url.scheme()
        ))
    }
}

/// Who can open a copied reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceReach {
    /// A network address another client can try.
    Network,
    /// A loopback host or local file: only this device can open it.
    ThisDevice,
}

/// An address copied as-is for someone else. Nothing is published, signed or
/// granted: forwarding it gives others exactly what the address gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceLink {
    pub address: String,
    pub reach: ReferenceReach,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceRefusal {
    Invalid,
    /// A graph or identity id only this store resolves; not a link.
    LocalId(String),
    /// Inline content or a browser-internal or executable scheme.
    NotALink(String),
}

impl std::fmt::Display for ReferenceRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => write!(formatter, "the address is not a valid URL"),
            Self::LocalId(scheme) => write!(
                formatter,
                "{scheme}: names something only this store resolves; it is not a shareable link"
            ),
            Self::NotALink(scheme) => {
                write!(
                    formatter,
                    "{scheme}: addresses are content or browser-internal, not links"
                )
            },
        }
    }
}

/// Prepare a reference to `address`, kept exactly as stored, query and
/// fragment included. Graph ids, inline content and internal schemes refuse.
pub fn prepare_reference(address: &str) -> Result<ReferenceLink, ReferenceRefusal> {
    let url = url::Url::parse(address.trim()).map_err(|_| ReferenceRefusal::Invalid)?;
    let scheme = url.scheme();
    match scheme {
        "mere" | "graphshell" | "verso" | "personae" => {
            return Err(ReferenceRefusal::LocalId(scheme.into()));
        },
        "data" | "javascript" | "blob" | "about" | "view-source" | "filesystem" | "chrome"
        | "chrome-extension" | "moz-extension" | "resource" | "edge" => {
            return Err(ReferenceRefusal::NotALink(scheme.into()));
        },
        _ => {},
    }
    let reach = if scheme == "file" || url.host().is_some_and(|host| is_loopback(&host)) {
        ReferenceReach::ThisDevice
    } else if url.host_str().is_some() {
        ReferenceReach::Network
    } else {
        return Err(ReferenceRefusal::NotALink(scheme.into()));
    };
    Ok(ReferenceLink {
        address: address.trim().into(),
        reach,
    })
}

fn is_loopback(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(name) => {
            let name = name.trim_end_matches('.');
            name.eq_ignore_ascii_case("localhost")
                || name.to_ascii_lowercase().ends_with(".localhost")
        },
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
    }
}

/// A web-only offer. The receiving native host owns browser dispatch.
pub fn web_browser_handler() -> HandlerOffer {
    HandlerOffer {
        id: SYSTEM_DEFAULT_HANDLER.into(),
        label: "Open in browser".into(),
        explanation: "Open this web address in this device’s default browser after the owner accepts the request.".into(),
        address_kinds: vec![AddressKind::Http],
    }
}

/// The typed payload accepted by Graphshell's local open intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAddressV1 {
    pub address: String,
    pub handler: String,
}

/// A handler the host is willing to offer for selected address kinds.
///
/// This is a semantic offer, not an executable command line. Native launch
/// authority remains outside the portable Graphshell/Mere cone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerOffer {
    pub id: String,
    pub label: String,
    pub explanation: String,
    pub address_kinds: Vec<AddressKind>,
}

impl HandlerOffer {
    pub fn supports(&self, kind: AddressKind) -> bool {
        self.address_kinds.contains(&kind)
    }

    pub fn intent_id(&self) -> String {
        intent_id(&self.id)
    }

    pub fn advertised_action(&self) -> AdvertisedAction {
        AdvertisedAction {
            intent: IntentReference(self.intent_id()),
            label: self.label.clone(),
            explanation: self.explanation.clone(),
            payload_schema: OPEN_ADDRESS_SCHEMA.to_string(),
            input_form: None,
            effect: IntentEffect::ExternalEffect,
        }
    }
}

/// Ordered handler policy. Hosts replace or reorder this list from settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerRegistry {
    offers: Vec<HandlerOffer>,
}

impl HandlerRegistry {
    pub fn new(offers: Vec<HandlerOffer>) -> Self {
        Self { offers }
    }

    pub fn offers_for(&self, kind: AddressKind) -> impl Iterator<Item = &HandlerOffer> {
        self.offers.iter().filter(move |offer| offer.supports(kind))
    }

    pub fn get(&self, id: &str) -> Option<&HandlerOffer> {
        self.offers.iter().find(|offer| offer.id == id)
    }

    pub fn actions_for(&self, kind: AddressKind) -> Vec<AdvertisedAction> {
        self.offers_for(kind)
            .map(HandlerOffer::advertised_action)
            .collect()
    }

    pub fn attach_actions(&self, semantics: &mut PresentationSemantics, kind: AddressKind) {
        semantics.actions.extend(self.actions_for(kind));
    }
}

pub fn intent_id(handler: &str) -> String {
    format!("{OPEN_ADDRESS_INTENT}/{handler}")
}

pub fn handler_from_intent(intent: &str) -> Option<&str> {
    intent
        .strip_prefix(OPEN_ADDRESS_INTENT)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .filter(|handler| !handler.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_keep_the_exact_address_and_name_their_reach() {
        let address = "https://example.org/a/b?utm_source=x&q=1#part";
        let link = prepare_reference(address).unwrap();
        assert_eq!(link.address, address);
        assert_eq!(link.reach, ReferenceReach::Network);
        let gemini = prepare_reference("gemini://example.org/log.gmi").unwrap();
        assert_eq!(gemini.reach, ReferenceReach::Network);
        for local in [
            "http://localhost:8080/x",
            "http://app.localhost/",
            "http://127.0.0.1/",
            "http://[::1]/",
            "file:///Users/me/notes.md",
        ] {
            assert_eq!(
                prepare_reference(local).unwrap().reach,
                ReferenceReach::ThisDevice,
                "{local}"
            );
        }
    }

    #[test]
    fn graph_ids_inline_content_and_internal_schemes_are_not_links() {
        for address in [
            "mere://node/1",
            "graphshell://clip/2",
            "personae://persona/alice",
        ] {
            assert!(matches!(
                prepare_reference(address),
                Err(ReferenceRefusal::LocalId(_))
            ));
        }
        for address in [
            "data:text/plain,hi",
            "javascript:alert(1)",
            "about:blank",
            "chrome-extension://abc/graph.html",
            "mailto:someone@example.org",
        ] {
            assert!(
                matches!(
                    prepare_reference(address),
                    Err(ReferenceRefusal::NotALink(_))
                ),
                "{address}"
            );
        }
        assert_eq!(
            prepare_reference("not a url"),
            Err(ReferenceRefusal::Invalid)
        );
    }

    #[test]
    fn the_attached_browser_opens_web_addresses_only() {
        assert!(attached_browser_target("https://example.org/x?y#z").is_ok());
        for address in [
            "gemini://example.org/",
            "file:///etc/hosts",
            "javascript:alert(1)",
            "about:blank",
        ] {
            assert!(attached_browser_target(address).is_err(), "{address}");
        }
        let offers = local_profile_handlers();
        assert!(offers.get(SYSTEM_DEFAULT_HANDLER).is_none());
        assert!(
            offers
                .get(ATTACHED_BROWSER_HANDLER)
                .unwrap()
                .supports(AddressKind::Http)
        );
        assert!(
            !offers
                .get(ATTACHED_BROWSER_HANDLER)
                .unwrap()
                .supports(AddressKind::Unknown)
        );
    }
}
