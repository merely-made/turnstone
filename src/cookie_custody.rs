// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable custody of the session cookie jar (ranged fetch plan, lane T3).
//!
//! The jar itself is Mere's process session, shared by the page actor, the
//! page-image fetcher and the Redshank tile. It had been in memory only since
//! Meerkat retired: no host called `fetch::persist_cookies`, so a login never
//! survived a restart. This module owns the store the jar is kept in, under the
//! profile's data root rather than a session, because cookies belong to the
//! persona (the native session store plan). It is loaded once at start and
//! flushed on each event-loop drain; the flush is dirty-gated inside `fetch`,
//! so a drain that set no cookies costs nothing.
//!
//! The persona is the built-in default one for now. Turnstone identifies its
//! user by the personae vault's root key and has no persona id of the kind the
//! store is keyed by; until personas arrive every profile shares one jar, which
//! is what the in-memory jar already did. Recorded as the placeholder it is.

use std::path::Path;

use eidetic::Store;
use eidetic_fjall::FjallStore;
use pandect::PersonaId;

/// Where the jar lives under the profile's data root.
pub fn cookie_dir(data_root: &Path) -> std::path::PathBuf {
    data_root.join("cookies")
}

/// The custody of one profile's cookies: the store, and the persona they are
/// kept for.
pub struct CookieCustody {
    store: FjallStore,
    persona: PersonaId,
}

impl CookieCustody {
    /// Open the store and load whatever it holds into the session jar.
    pub fn open(data_root: &Path) -> Result<Self, String> {
        let dir = cookie_dir(data_root);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let mut custody = Self {
            store: FjallStore::open(&dir).map_err(|error| error.to_string())?,
            persona: PersonaId::default_persona(),
        };
        custody.load();
        Ok(custody)
    }

    fn load(&mut self) {
        fetch::load_cookies(&mut self.store as &mut dyn Store, self.persona);
    }

    /// Write through whatever changed since the last flush. Cheap when nothing did.
    pub fn flush(&mut self) {
        fetch::persist_cookies(&mut self.store as &mut dyn Store, self.persona);
    }
}

#[cfg(test)]
mod tests {
    use netfetcher::{CookieStore as _, SameSiteContext};

    use super::*;

    #[test]
    fn a_cookie_set_in_one_run_is_there_in_the_next() {
        let root = tempfile::tempdir().unwrap();
        // A domain no other test uses: the jar is process-wide.
        let url = url::Url::parse("https://custody.test.invalid/").unwrap();

        {
            let mut first = CookieCustody::open(root.path()).unwrap();
            fetch::session_jar().set_cookie(&url, "kept=yes; Path=/");
            fetch::mark_cookies_dirty();
            first.flush();
        }

        // The jar is process-wide and cannot be swapped for a fresh one, so the
        // cookie is expired out of it in memory while the store still holds it;
        // that is the state a restart leaves, seen from the store's side.
        fetch::session_jar().set_cookie(&url, "kept=gone; Path=/; Max-Age=0");
        assert!(
            fetch::session_jar()
                .cookies_for(&url, SameSiteContext::same_site())
                .is_empty(),
            "the jar must have forgotten the cookie before the reload"
        );

        let _second = CookieCustody::open(root.path()).unwrap();
        let cookies = fetch::session_jar().cookies_for(&url, SameSiteContext::same_site());
        assert_eq!(cookies, vec!["kept=yes".to_owned()]);
    }
}
