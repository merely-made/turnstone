// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn report(s: &dyn Surface) -> serde_json::Value {
    let c = s.capabilities();
    serde_json::json!({"scope":format!("{:?}",s.scope()),"engine":c.identity.engine,"version":c.identity.version,"platform":c.identity.platform,"build_features":c.identity.build_features,"presentation":format!("{:?}",c.presentation),"evidence":format!("{:?}",c.evidence),"semantics":format!("{:?}",c.semantics),"features":c.features().map(|(f,s)|(format!("{f:?}"),serde_json::json!({"permits":s.permits(),"status":format!("{s:?}")}))).collect::<serde_json::Map<_,_>>()})
}
