// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl FeedSubscriptions {
    pub fn load(session_dir: &Path) -> Self {
        let path = session_dir.join(FILE_NAME);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(subscriptions) if subscriptions.version == FORMAT_VERSION => subscriptions,
            Ok(_) => {
                tracing::warn!(path = ?path, "unsupported feed subscription version");
                Self::default()
            },
            Err(error) => {
                tracing::warn!(%error, path = ?path, "failed to read feed subscriptions");
                Self::default()
            },
        }
    }

    pub fn save(&self, session_dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let target = session_dir.join(FILE_NAME);
        let temporary = target.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        if let Err(error) = (|| -> io::Result<()> {
            std::fs::write(&temporary, bytes)?;
            if target.exists() {
                std::fs::remove_file(&target)?;
            }
            std::fs::rename(&temporary, &target)
        })() {
            let _ = std::fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }

    pub fn subscribe(&mut self, source: Uuid, url: String, period: servitor::Period) -> bool {
        let replacement = Subscription {
            source,
            url: url.clone(),
            period,
            last_checked_ms: None,
            last_success_ms: None,
            last_error: None,
            artwork: None,
            diagnostics: Vec::new(),
            entries: BTreeMap::new(),
        };
        match self.subscriptions.get_mut(&source) {
            Some(subscription) if subscription.url == url => {
                let changed = subscription.period != period;
                subscription.period = period;
                changed
            },
            Some(subscription) => {
                *subscription = replacement;
                self.in_flight.remove(&source);
                true
            },
            None => {
                self.subscriptions.insert(source, replacement);
                true
            },
        }
    }

    pub fn unsubscribe(&mut self, source: Uuid) -> bool {
        self.in_flight.remove(&source);
        self.subscriptions.remove(&source).is_some()
    }

    pub fn entry_members(&self, source: Uuid) -> Vec<Uuid> {
        self.subscriptions
            .get(&source)
            .into_iter()
            .flat_map(|subscription| subscription.entries.values())
            .filter_map(|entry| entry.member)
            .collect()
    }

    pub fn is_current(&self, source: Uuid, url: &str) -> bool {
        self.subscriptions
            .get(&source)
            .is_some_and(|subscription| subscription.url == url)
    }

    pub fn start(&mut self, source: Uuid) -> Option<String> {
        let subscription = self.subscriptions.get(&source)?;
        if !self.in_flight.insert(source) {
            return None;
        }
        Some(subscription.url.clone())
    }

    pub fn start_all(&mut self) -> Vec<(Uuid, String)> {
        let sources: Vec<Uuid> = self.subscriptions.keys().copied().collect();
        sources
            .into_iter()
            .filter_map(|source| self.start(source).map(|url| (source, url)))
            .collect()
    }

    pub fn start_due(&mut self, now_ms: u64) -> Vec<(Uuid, String)> {
        let due: Vec<Uuid> = self
            .subscriptions
            .values()
            .filter(|subscription| {
                !self.in_flight.contains(&subscription.source)
                    && subscription.last_checked_ms.is_none_or(|last| {
                        now_ms.saturating_sub(last) >= subscription.period.millis()
                    })
            })
            .map(|subscription| subscription.source)
            .collect();
        due.into_iter()
            .filter_map(|source| self.start(source).map(|url| (source, url)))
            .collect()
    }

    pub fn fetched_error(&mut self, source: Uuid, error: String, now_ms: u64) -> bool {
        self.in_flight.remove(&source);
        let Some(subscription) = self.subscriptions.get_mut(&source) else {
            return false;
        };
        subscription.last_checked_ms = Some(now_ms);
        subscription.last_error = Some(error);
        true
    }

    pub fn merge(&mut self, source: Uuid, parsed: ParsedFeed, now_ms: u64) -> Option<FeedMerge> {
        self.in_flight.remove(&source);
        let subscription = self.subscriptions.get_mut(&source)?;
        subscription.last_checked_ms = Some(now_ms);
        subscription.last_success_ms = Some(now_ms);
        subscription.last_error = None;

        let ParsedFeed {
            title,
            artwork,
            entries,
            diagnostics,
        } = parsed;
        subscription.artwork = artwork;
        subscription.diagnostics = diagnostics.clone();
        let mut merge = FeedMerge {
            title,
            entries: Vec::new(),
            diagnostics,
        };
        for entry in entries {
            let identity = entry_identity(&entry);
            if !subscription.entries.contains_key(&identity)
                && entry.guid.is_some()
                && let Some(old_identity) = subscription
                    .entries
                    .iter()
                    .find(|(stored_identity, stored)| {
                        stored.guid.as_deref().is_none_or(str::is_empty)
                            && stored_identity.as_str() == entry.url
                    })
                    .map(|(identity, _)| identity.clone())
                && let Some(stored) = subscription.entries.remove(&old_identity)
            {
                subscription.entries.insert(identity.clone(), stored);
            }
            match subscription.entries.get_mut(&identity) {
                Some(stored) if stored.same_document(&entry) && stored.member.is_some() => {},
                Some(stored) if stored.same_document(&entry) => {
                    // The document is unchanged, but its placement is missing.
                    // Repair that binding without changing its stored read status.
                    merge.entries.push(EntryProjection {
                        member: None,
                        entry,
                    });
                },
                Some(stored) => {
                    stored.replace_document(&entry);
                    merge.entries.push(EntryProjection {
                        member: stored.member,
                        entry,
                    });
                },
                None => {
                    subscription.entries.insert(
                        identity,
                        StoredEntry {
                            member: None,
                            url: entry.url.clone(),
                            guid: entry.guid.clone(),
                            title: entry.title.clone(),
                            date: entry.date.clone(),
                            summary: entry.summary.clone(),
                            enclosures: entry.enclosures.clone(),
                            duration: entry.duration.clone(),
                            artwork: entry.artwork.clone(),
                            chapters: entry.chapters.clone(),
                            transcripts: entry.transcripts.clone(),
                            unread: true,
                        },
                    );
                    merge.entries.push(EntryProjection {
                        entry,
                        member: None,
                    });
                },
            }
        }
        Some(merge)
    }

    /// Compatibility for an unambiguous address. A URL cannot choose among
    /// different GUID entries, even when they describe the same resource.
    pub fn bind_entry(&mut self, source: Uuid, url: &str, member: Uuid) {
        let Some(subscription) = self.subscriptions.get_mut(&source) else {
            return;
        };
        let identities: Vec<_> = subscription
            .entries
            .iter()
            .filter(|(identity, entry)| identity.as_str() == url || entry.url == url)
            .map(|(identity, _)| identity.clone())
            .collect();
        if identities.len() == 1 {
            subscription
                .entries
                .get_mut(&identities[0])
                .expect("selected entry")
                .member = Some(member);
        }
    }

    /// Bind the exact entry that was projected, rather than the first URL match.
    pub(crate) fn bind_projected_entry(
        &mut self,
        source: Uuid,
        entry: &FeedEntry,
        member: Uuid,
    ) -> bool {
        let identity = entry_identity(entry);
        let Some(stored) = self
            .subscriptions
            .get_mut(&source)
            .and_then(|subscription| subscription.entries.get_mut(&identity))
        else {
            return false;
        };
        stored.member = Some(member);
        true
    }

    /// The subscribed episode an address names: its own page, its bound
    /// graph member's page, or its enclosure. Only entries with playable
    /// audio answer.
    pub fn episode_for_address(&self, url: &str) -> Option<FeedEpisodeProjection> {
        self.subscriptions.values().find_map(|subscription| {
            subscription.entries.iter().find_map(|(identity, entry)| {
                let enclosure = entry.enclosures.iter().find(|enclosure| {
                    is_podcast_audio(&enclosure.url, enclosure.media_type.as_deref())
                })?;
                let named = entry.url == url || identity.as_str() == url || enclosure.url == url;
                named.then(|| FeedEpisodeProjection {
                    feed_url: subscription.url.clone(),
                    guid: entry
                        .guid
                        .clone()
                        .unwrap_or_else(|| identity.as_str().to_owned()),
                    enclosure_url: enclosure.url.clone(),
                    media_type: enclosure.media_type.clone(),
                    title: entry.title.clone(),
                    published: entry.date.clone(),
                    artwork: entry
                        .artwork
                        .clone()
                        .or_else(|| subscription.artwork.clone()),
                    member: entry.member,
                })
            })
        })
    }

    /// The graph member bound to one subscribed episode. This is the durable
    /// association between a listening item and its node: the feed store is
    /// saved beside the session graph, while derived node fields are not.
    pub fn episode_member(&self, feed_url: &str, guid: &str) -> Option<Uuid> {
        self.subscriptions
            .values()
            .filter(|subscription| subscription.url == feed_url)
            .find_map(|subscription| {
                subscription.entries.iter().find_map(|(identity, entry)| {
                    let entry_guid = entry.guid.as_deref().unwrap_or(identity.as_str());
                    (entry_guid == guid).then_some(entry.member).flatten()
                })
            })
    }

    /// The subscribed episode one graph member projects, for the open verb on
    /// a feed-entry node.
    pub fn episode_for_member(&self, member: Uuid) -> Option<FeedEpisodeProjection> {
        let url = self.subscriptions.values().find_map(|subscription| {
            subscription
                .entries
                .values()
                .find(|entry| entry.member == Some(member))
                .map(|entry| entry.url.clone())
        })?;
        self.episode_for_address(&url)
    }

    pub fn mark_read(&mut self, member: Uuid) -> bool {
        let mut changed = false;
        for subscription in self.subscriptions.values_mut() {
            for entry in subscription.entries.values_mut() {
                if entry.member == Some(member) && entry.unread {
                    entry.unread = false;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn forget_member(&mut self, member: Uuid) -> bool {
        let removed_source = self.unsubscribe(member);
        let mut changed = removed_source;
        for subscription in self.subscriptions.values_mut() {
            for entry in subscription.entries.values_mut() {
                if entry.member == Some(member) {
                    entry.member = None;
                    entry.unread = false;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn reconcile(&mut self, graph: &mere::kernel::graph::Graph) {
        self.in_flight.clear();
        self.subscriptions.retain(|source, subscription| {
            graph
                .get_node_by_id(*source)
                .is_some_and(|(_, node)| node.url() == subscription.url)
        });
        for subscription in self.subscriptions.values_mut() {
            for (identity, entry) in &mut subscription.entries {
                let url = if entry.url.is_empty() {
                    identity.as_str()
                } else {
                    entry.url.as_str()
                };
                entry.member = entry.member.filter(|member| {
                    graph
                        .get_node_by_id(*member)
                        .is_some_and(|(_, node)| node.url() == url)
                });
                // Equal URLs do not prove placement identity; never adopt a sibling.
                // Retain the sidecar's read status while this exact placement
                // is absent. It is projected only after an explicit binding exists.
            }
        }
    }

    /// Projection for exactly one Surface. An old sidecar can explicitly bind
    /// it in several feeds; unread stays true while any such binding is unread.
    pub(crate) fn surface_flags(&self, member: Uuid) -> (bool, bool, bool) {
        let source = self.subscriptions.contains_key(&member);
        let mut entry_member = false;
        let mut unread = false;
        for entry in self
            .subscriptions
            .values()
            .flat_map(|subscription| subscription.entries.values())
        {
            if entry.member == Some(member) {
                entry_member = true;
                unread |= entry.unread;
            }
        }
        (source, entry_member, unread)
    }

    pub fn member_info(&self, member: Uuid) -> Option<FeedMemberInfo> {
        if let Some(subscription) = self.subscriptions.get(&member) {
            return Some(FeedMemberInfo::Source {
                period: subscription.period,
                last_checked_ms: subscription.last_checked_ms,
                last_error: subscription.last_error.clone(),
                unread: subscription
                    .entries
                    .values()
                    .filter(|entry| entry.unread)
                    .count(),
            });
        }
        self.subscriptions.values().find_map(|subscription| {
            subscription
                .entries
                .values()
                .find(|entry| entry.member == Some(member))
                .map(|entry| FeedMemberInfo::Entry {
                    source_url: subscription.url.clone(),
                    date: entry.date.clone(),
                    unread: entry.unread,
                })
        })
    }

    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn unread_count(&self) -> usize {
        self.subscriptions
            .values()
            .flat_map(|subscription| subscription.entries.values())
            .filter(|entry| entry.unread)
            .count()
    }
}
