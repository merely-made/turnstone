// SPDX-License-Identifier: MPL-2.0
//! App-owned values for public site discovery; domain and keys stay in the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueSiteDraft {
    pub address: String,
    pub title: String,
    pub abstract_text: String,
    pub scroll_subject: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredSite {
    pub digest: [u8; 32],
    pub provider: [u8; 32],
    pub collection: [u8; 32],
    pub sequence: u64,
    pub address: String,
    pub title: String,
    pub abstract_text: String,
    pub scroll_subject: Option<u8>,
    pub expires_at_ms: u64,
}
pub fn subject_name(class: u8) -> &'static str {
    match class {
        0 => "General knowledge",
        1 => "Philosophy",
        2 => "Religion",
        3 => "Social sciences",
        4 => "Unclassified",
        5 => "Natural sciences",
        6 => "Applied sciences",
        7 => "Arts",
        8 => "Literature",
        9 => "History and geography",
        _ => "Unknown",
    }
}

/// Catalogue controls are one app action family; grants stay in the worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogueAction {
    BeginAdvertiseSite {
        subject: u8,
    },
    CommitSiteAdvertisement {
        expected_address: [u8; 32],
        subject: u8,
        abstract_text: String,
    },
    BrowseSites {
        subject: Option<u8>,
    },
    OpenDiscoveredSite {
        digest: [u8; 32],
    },
    WithdrawSite {
        collection: [u8; 32],
    },
}
