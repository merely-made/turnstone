// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn gemtext_feed_preserves_relative_urls_dates_and_dedupes() {
    let parsed = parse_document(
            "gemini://capsule.test/log/index.gmi",
            &FetchedPage::text(
                Some("text/gemini; charset=utf-8".into()),
                "# Log\n=> entry-one.gmi 2026-08-17 First\n=> entry-one.gmi 2026-08-17 Duplicate\n```\n=> hidden.gmi 2026-08-18 Hidden\n```\n=> /two.gmi 2026-08-18 Second\n",
            ),
        )
        .unwrap();
    assert_eq!(parsed.title.as_deref(), Some("Log"));
    assert_eq!(parsed.entries.len(), 2);
    assert_eq!(
        parsed.entries[0].url,
        "gemini://capsule.test/log/entry-one.gmi"
    );
    assert_eq!(parsed.entries[0].date.as_deref(), Some("2026-08-17"));
    assert_eq!(parsed.entries[1].url, "gemini://capsule.test/two.gmi");
}

#[test]
fn rss_and_json_feed_share_the_entry_shape() {
    let rss = parse_document(
            "https://example.test/feed.xml",
            &FetchedPage::text(
                Some("application/rss+xml".into()),
                "<rss><channel><title>Notes</title><item><title>One</title><link>/one</link><pubDate>today</pubDate><description>Hello</description></item></channel></rss>",
            ),
        )
        .unwrap();
    assert_eq!(rss.entries[0].url, "https://example.test/one");

    let json = parse_document(
        "https://example.test/feed.json",
        &FetchedPage::text(
            Some("application/feed+json".into()),
            r#"{"title":"Notes","items":[{"id":"one","url":"/one","title":"One"}]}"#,
        ),
    )
    .unwrap();
    assert_eq!(json.entries[0].url, rss.entries[0].url);
}

#[test]
fn unchanged_unbound_entry_gets_an_explicit_view_without_adopting_a_sibling() {
    let source = Uuid::new_v4();
    let sibling = Uuid::new_v4();
    let entry_view = Uuid::new_v4();
    let source_url = "https://feed-repair.test/feed";
    let entry_url = "https://feed-repair.test/entry";
    let mut graph = mere::kernel::graph::Graph::new();
    for (id, url) in [(source, source_url), (sibling, entry_url)] {
        mere::kernel::graph::apply::add_node(&mut graph, Some(id), url.into(), Default::default());
    }
    let mut feeds = FeedSubscriptions::default();
    feeds.subscribe(source, source_url.into(), servitor::Period::Hour);
    let parsed = ParsedFeed {
        entries: vec![FeedEntry {
            url: entry_url.into(),
            title: "Entry".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    feeds.merge(source, parsed.clone(), 1).unwrap();
    feeds.bind_entry(source, entry_url, entry_view);
    feeds.mark_read(entry_view);
    // Reopen/reconcile refuses an arbitrary equal-URL sibling and retains
    // the prior read status while there is no explicit view.
    feeds.reconcile(&graph);
    assert_eq!(feeds.surface_flags(sibling), (false, false, false));
    let repaired = feeds.merge(source, parsed.clone(), 2).unwrap();
    assert_eq!(
        repaired.entries.len(),
        1,
        "unchanged content still needs an explicit view binding"
    );
    assert_eq!(repaired.entries[0].member, None);
    feeds.bind_entry(source, entry_url, entry_view);
    assert_eq!(
        feeds.surface_flags(entry_view),
        (false, true, false),
        "binding repair preserves read status"
    );
    assert!(
        feeds
            .merge(source, parsed.clone(), 3)
            .unwrap()
            .entries
            .is_empty()
    );
    // A changed document is new unread evidence, independently of repair.
    let mut changed = parsed;
    changed.entries[0].title = "Changed".into();
    assert_eq!(feeds.merge(source, changed, 4).unwrap().entries.len(), 1);
    assert_eq!(feeds.surface_flags(entry_view), (false, true, true));
}

#[test]
fn unread_entry_missing_binding_retains_status_through_restart_and_repair() {
    let root = tempfile::tempdir().unwrap();
    let source = Uuid::new_v4();
    let entry = Uuid::new_v4();
    let mut graph = mere::kernel::graph::Graph::new();
    mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(source),
        "https://unread-repair.test/feed".into(),
        Default::default(),
    );
    let parsed = ParsedFeed {
        entries: vec![FeedEntry {
            url: "https://unread-repair.test/entry".into(),
            title: "Entry".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    for unread in [false, true] {
        let mut feeds = FeedSubscriptions::default();
        feeds.subscribe(
            source,
            "https://unread-repair.test/feed".into(),
            servitor::Period::Hour,
        );
        feeds.merge(source, parsed.clone(), 1).unwrap();
        feeds.bind_entry(source, "https://unread-repair.test/entry", entry);
        if !unread {
            feeds.mark_read(entry);
        }
        feeds.save(root.path()).unwrap();
        let mut feeds = FeedSubscriptions::load(root.path());
        feeds.reconcile(&graph); // The exact bound view is absent.
        assert_eq!(feeds.unread_count(), usize::from(unread));
        let repair = feeds.merge(source, parsed.clone(), 2).unwrap();
        assert_eq!(repair.entries.len(), 1);
        let fresh = Uuid::new_v4();
        feeds.bind_entry(source, &repair.entries[0].entry.url, fresh);
        assert_eq!(feeds.surface_flags(fresh), (false, true, unread));
        assert!(
            feeds
                .merge(source, parsed.clone(), 3)
                .unwrap()
                .entries
                .is_empty()
        );
    }
}

#[test]
fn separate_guid_entries_with_one_url_keep_distinct_bindings() {
    let source = Uuid::new_v4();
    let mut feeds = FeedSubscriptions::default();
    feeds.subscribe(
        source,
        "https://guid-binding.test/feed".into(),
        servitor::Period::Hour,
    );
    let parsed = ParsedFeed {
        entries: ["left", "right"]
            .into_iter()
            .map(|guid| FeedEntry {
                guid: Some(guid.into()),
                url: "https://guid-binding.test/shared".into(),
                title: guid.into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    let projections = feeds.merge(source, parsed.clone(), 1).unwrap();
    let members = [Uuid::new_v4(), Uuid::new_v4()];
    for (projection, member) in projections.entries.iter().zip(members) {
        assert!(feeds.bind_projected_entry(source, &projection.entry, member));
    }
    assert!(
        feeds
            .merge(source, parsed.clone(), 2)
            .unwrap()
            .entries
            .is_empty(),
        "both explicit GUID placements must remain bound"
    );
    assert_eq!(feeds.entry_members(source).len(), 2);
    for member in members {
        assert_eq!(feeds.surface_flags(member), (false, true, true));
    }
    // A URL alone must not choose between these two entries.
    feeds.bind_entry(source, "https://guid-binding.test/shared", Uuid::new_v4());
    for member in members {
        assert_eq!(feeds.surface_flags(member), (false, true, true));
    }
}

#[test]
fn schedule_survives_restart_and_suppresses_unchanged_entries() {
    let dir = tempfile::tempdir().unwrap();
    let source = Uuid::new_v4();
    let mut feeds = FeedSubscriptions::default();
    assert!(feeds.subscribe(
        source,
        "gemini://capsule.test/feed".into(),
        servitor::Period::Hour
    ));
    assert_eq!(feeds.start_due(0).len(), 1);
    let parsed = ParsedFeed {
        title: Some("Log".into()),
        entries: vec![FeedEntry {
            url: "gemini://capsule.test/one".into(),
            title: "One".into(),
            date: Some("2026-08-18".into()),
            summary: None,
            ..FeedEntry::default()
        }],
        ..ParsedFeed::default()
    };
    let merge = feeds.merge(source, parsed.clone(), 10).unwrap();
    assert_eq!(merge.entries.len(), 1);
    feeds.bind_entry(source, "gemini://capsule.test/one", Uuid::new_v4());
    assert!(feeds.merge(source, parsed, 20).unwrap().entries.is_empty());
    feeds.save(dir.path()).unwrap();

    let mut reopened = FeedSubscriptions::load(dir.path());
    assert!(reopened.start_due(3_599_999).is_empty());
    assert_eq!(reopened.start_due(3_600_020).len(), 1);
}

#[test]
fn podcast_facts_resolve_against_the_caller_base_and_guid_wins_deduplication() {
    let parsed = parse_document(
            "https://example.test/shows/feed.xml",
            &FetchedPage::text(
                Some("application/rss+xml".into()),
                r#"<rss xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd"
                    xmlns:podcast="https://podcastindex.org/namespace/1.0"
                    xmlns:mystery="https://example.test/mystery">
                  <channel>
                    <title>Field Notes</title>
                    <itunes:image href="art/feed.jpg"/>
                    <item>
                      <guid>episode-42</guid><title>Wetland</title><link>episodes/42</link>
                      <enclosure url="audio/42.mp3" type="audio/mpeg" length="123456"/>
                      <itunes:duration>01:02:03</itunes:duration>
                      <itunes:image href="art/42.jpg"/>
                      <podcast:chapters url="chapters/42.json" type="application/json+chapters"/>
                      <podcast:transcript url="transcripts/42.vtt" type="text/vtt" language="en"/>
                      <mystery:waveform bins="32"/>
                    </item>
                    <item><guid>episode-42</guid><title>Duplicate</title><link>elsewhere</link></item>
                  </channel>
                </rss>"#,
            ),
        )
        .unwrap();

    assert_eq!(
        parsed.artwork.as_deref(),
        Some("https://example.test/shows/art/feed.jpg")
    );
    assert_eq!(parsed.entries.len(), 1);
    let entry = &parsed.entries[0];
    assert_eq!(entry.guid.as_deref(), Some("episode-42"));
    assert_eq!(entry.url, "https://example.test/shows/episodes/42");
    assert_eq!(
        entry.enclosures[0].url,
        "https://example.test/shows/audio/42.mp3"
    );
    assert_eq!(entry.enclosures[0].byte_length, Some(123_456));
    assert_eq!(entry.duration.as_deref(), Some("01:02:03"));
    assert_eq!(
        entry.artwork.as_deref(),
        Some("https://example.test/shows/art/42.jpg")
    );
    assert_eq!(
        entry.chapters[0].url,
        "https://example.test/shows/chapters/42.json"
    );
    assert_eq!(
        entry.transcripts[0].url,
        "https://example.test/shows/transcripts/42.vtt"
    );
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("mystery:waveform"))
    );
}

#[test]
fn guid_identity_survives_an_enclosure_or_link_change() {
    let source = Uuid::new_v4();
    let member = Uuid::new_v4();
    let mut feeds = FeedSubscriptions::default();
    feeds.subscribe(
        source,
        "https://example.test/feed.xml".into(),
        servitor::Period::Hour,
    );
    let entry = |url: &str| FeedEntry {
        url: url.into(),
        guid: Some("stable-guid".into()),
        title: "Episode".into(),
        enclosures: vec![FeedEnclosure {
            url: format!("{url}.mp3"),
            media_type: Some("audio/mpeg".into()),
            byte_length: None,
        }],
        ..FeedEntry::default()
    };
    let first = feeds
        .merge(
            source,
            ParsedFeed {
                entries: vec![entry("https://example.test/first")],
                ..ParsedFeed::default()
            },
            10,
        )
        .unwrap();
    assert_eq!(first.entries.len(), 1);
    feeds.bind_entry(source, "https://example.test/first", member);

    let changed = feeds
        .merge(
            source,
            ParsedFeed {
                entries: vec![entry("https://example.test/revised")],
                ..ParsedFeed::default()
            },
            20,
        )
        .unwrap();
    assert_eq!(changed.entries.len(), 1);
    assert_eq!(changed.entries[0].member, Some(member));
    assert_eq!(feeds.subscriptions[&source].entries.len(), 1);
}
