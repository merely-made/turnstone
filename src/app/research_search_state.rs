// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use crate::research_search::{Member, Request};
use eidetic_search::query::{
    Alias, ProducerKind, QueryMode, SearchQuery, SearchResult, SearchSettings, TagId,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Results,
    Compare,
    ChooseLeft,
    ChooseRight,
    ChooseCompare {
        left: bool,
        target: crate::research::Target,
    },
    RunCompare,
    ComparePage(isize),
    OpenCompare {
        left: bool,
        span: Option<usize>,
        request: uuid::Uuid,
    },
    Options,
    Modes,
    Types,
    Sources,
    Dates,
    Tags,
    Settings,
    Coverage,
    Mode(u8),
    Kind(ProducerKind),
    AllKinds,
    ScopeAll,
    ScopeChosen,
    Member(Member),
    Tag(TagId),
    Input(String),
    Submit(String),
    Page(isize),
    Open {
        target: crate::research::Target,
        request: uuid::Uuid,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Results,
    Compare,
    ChooseLeft,
    ChooseRight,
    Options,
    Modes,
    Types,
    Sources,
    Dates,
    Tags,
    Settings,
    Coverage,
    Input(String),
}
pub struct Preferences {
    pub version: u32,
    pub settings: SearchSettings,
    pub aliases: Vec<Alias>,
    pub comparison: eidetic_search::compare::ComparisonSettings,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredPreferences {
    version: u32,
    settings: StoredSettings,
    aliases: Vec<Alias>,
    #[serde(default)]
    comparison: StoredComparison,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredComparison {
    max_input_bytes: usize,
    max_identity_bytes: usize,
    max_diff_work: usize,
    max_output_spans: usize,
}
impl Default for StoredComparison {
    fn default() -> Self {
        (&eidetic_search::compare::ComparisonSettings::default()).into()
    }
}
impl From<&eidetic_search::compare::ComparisonSettings> for StoredComparison {
    fn from(s: &eidetic_search::compare::ComparisonSettings) -> Self {
        Self {
            max_input_bytes: s.max_input_bytes,
            max_identity_bytes: s.max_identity_bytes,
            max_diff_work: s.max_diff_work,
            max_output_spans: s.max_output_spans,
        }
    }
}
impl From<StoredComparison> for eidetic_search::compare::ComparisonSettings {
    fn from(s: StoredComparison) -> Self {
        Self {
            max_input_bytes: s.max_input_bytes,
            max_identity_bytes: s.max_identity_bytes,
            max_diff_work: s.max_diff_work,
            max_output_spans: s.max_output_spans,
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSettings {
    weights: eidetic_search::FieldWeights,
    snippet_context: usize,
    limit: usize,
    max_sources: usize,
    max_corpus_chars: usize,
    max_metadata_chars: usize,
    max_query_chars: usize,
}
impl From<&Preferences> for StoredPreferences {
    fn from(value: &Preferences) -> Self {
        let settings = &value.settings;
        Self {
            version: value.version,
            aliases: value.aliases.clone(),
            comparison: (&value.comparison).into(),
            settings: StoredSettings {
                weights: settings.weights,
                snippet_context: settings.snippet_context,
                limit: settings.limit,
                max_sources: settings.max_sources,
                max_corpus_chars: settings.max_corpus_chars,
                max_metadata_chars: settings.max_metadata_chars,
                max_query_chars: settings.max_query_chars,
            },
        }
    }
}
impl From<StoredPreferences> for Preferences {
    fn from(value: StoredPreferences) -> Self {
        let settings = value.settings;
        Self {
            version: value.version,
            aliases: value.aliases,
            comparison: value.comparison.into(),
            settings: SearchSettings {
                weights: settings.weights,
                snippet_context: settings.snippet_context,
                offset: 0,
                limit: settings.limit,
                max_sources: settings.max_sources,
                max_corpus_chars: settings.max_corpus_chars,
                max_metadata_chars: settings.max_metadata_chars,
                max_query_chars: settings.max_query_chars,
            },
        }
    }
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            settings: SearchSettings::default(),
            aliases: vec![],
            comparison: Default::default(),
        }
    }
}
impl Preferences {
    fn path(root: &std::path::Path) -> std::path::PathBuf {
        root.join("research-search-settings.json")
    }
    pub fn load(root: &std::path::Path) -> Result<Self, String> {
        match std::fs::read(Self::path(root)) {
            Ok(bytes) => {
                let stored: StoredPreferences = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Research settings are invalid: {e}"))?;
                let mut prefs: Self = stored.into();
                if prefs.version != 1 {
                    return Err("Research settings version is unsupported.".into());
                }
                prefs.settings.offset = 0;
                prefs.validate()?;
                Ok(prefs)
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("Research settings are unavailable: {error}")),
        }
    }
    fn validate(&self) -> Result<(), String> {
        let snapshot = eidetic_search::query::ResearchSnapshot {
            generation: "settings-validation".into(),
            scope: vec![],
            sources: vec![],
            coverage: eidetic_search::query::Coverage::Complete,
        };
        let query = SearchQuery {
            mode: QueryMode::Browse,
            aliases: self.aliases.clone(),
            ..Default::default()
        };
        eidetic_search::query::search(&snapshot, &query, &self.settings).map(|_| ())
    }
    pub fn save(&self, root: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        // Never replace a malformed or newer sidecar.
        Self::load(root)?;
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let temporary = root.join(format!(
            ".research-search-settings-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        let bytes =
            serde_json::to_vec_pretty(&StoredPreferences::from(self)).map_err(|e| e.to_string())?;
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        if let Err(error) = std::fs::rename(&temporary, Self::path(root)) {
            let _ = std::fs::remove_file(temporary);
            return Err(error.to_string());
        }
        Ok(())
    }
}

pub(crate) struct State {
    pub view: View,
    pub mode: u8,
    pub query: SearchQuery,
    pub preferences: Preferences,
    pub selected: Option<Vec<Member>>,
    pub page: usize,
    pub query_revision: u64,
    pub scope_revision: Option<u64>,
    pub request: Option<Request>,
    pub accepted: Option<Request>,
    pub result: Option<SearchResult>,
    pub compare_left: Option<crate::research::Target>,
    pub compare_right: Option<crate::research::Target>,
    pub compare_request: Option<crate::research_compare::Request>,
    pub compare_accepted: Option<crate::research_compare::Request>,
    pub comparison: Option<crate::research_compare::ResultView>,
    pub compare_page: usize,
    pub error: Option<String>,
    pub settings_error: Option<String>,
    pub original_text: String,
    pub rows: Vec<crate::ui::Suggestion>,
    pub first_row: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            view: View::Results,
            mode: 0,
            query: SearchQuery::default(),
            preferences: Preferences::default(),
            selected: None,
            page: 0,
            query_revision: 0,
            scope_revision: None,
            request: None,
            accepted: None,
            result: None,
            compare_left: None,
            compare_right: None,
            compare_request: None,
            compare_accepted: None,
            comparison: None,
            compare_page: 0,
            error: None,
            settings_error: None,
            original_text: String::new(),
            rows: vec![],
            first_row: 0,
        }
    }
}
