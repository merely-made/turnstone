// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Host declarations for offered commands. The reducer and ring gates retain
//! execution authority; this metadata does not grant permission or add a
//! confirmation flow. Effect class describes the semantic operation rather
//! than incidental saving of local view state.

use super::Action;
use cambium::EffectClass;

/// Every new Action requires an explicit effect-class decision here.
pub(crate) fn command_metadata(action: &Action) -> (EffectClass, bool) {
    let effect = match action {
        Action::ContentNavigationCommitted { .. }
        | Action::ContentTitleChanged { .. }
        | Action::KeepNode { .. }
        | Action::SetNodeSprite { .. }
        | Action::ResearchNoteCreated { .. }
        | Action::ForkNode { .. }
        | Action::ForkFocusedNode
        | Action::NewSession
        | Action::CloseSession
        | Action::DeleteFocusedNode
        | Action::RecoverDeletedNode(..)
        | Action::EmptyRecycleBin
        | Action::WriteNote { .. }
        | Action::RecoverSession(..) => EffectClass::DomainTruth,
        Action::OpenAddress(..)
        | Action::CapturePage { .. }
        | Action::CaptureSourceDocument { .. }
        | Action::RetrySourceDocumentCapture { .. }
        | Action::OpenSavedSource(..)
        | Action::ChoosePermission { .. }
        | Action::SubmitAuthentication { .. }
        | Action::CancelAuthentication { .. }
        | Action::NavBack
        | Action::NavForward
        | Action::Reload
        | Action::Stop
        | Action::SubscribeFocusedFeed { .. }
        | Action::RefreshFeeds
        | Action::SaveSession
        | Action::FoundPlace { .. }
        | Action::ExportPlaceCard { .. }
        | Action::RefreshPlaceContact { .. }
        | Action::ReconnectPlaceWithCard { .. }
        | Action::OfferPlacePrekey { .. }
        | Action::OfferPlacePrekeyForCard { .. }
        | Action::InviteToPlace { .. }
        | Action::InviteToPlaceWithPrekey { .. }
        | Action::RevokePlaceMember { .. }
        | Action::JoinPlaceFile { .. }
        | Action::JoinPlace(..)
        | Action::LeavePlace
        | Action::ReconnectPlace
        | Action::RejoinPlace
        | Action::CopyLocalRendezvous
        | Action::SendPlaceMessage { .. }
        | Action::ShareFocusedNode
        | Action::ResyncPlace
        | Action::ToggleNodeContent
        | Action::RepeatShellEntry(..)
        | Action::OpenShellEntryTarget(..)
        | Action::OpenAnswers
        | Action::NewResearchNote
        | Action::OpenResearchNote
        | Action::ChooseResearchNote { .. }
        | Action::CommitResearchNote { .. }
        | Action::ChooseKnotDocumentFile { .. }
        | Action::ChooseClipPackageFile
        | Action::RetainProducerSelection { .. }
        | Action::ImportClipPackage(..)
        | Action::ChooseClipExportFile
        | Action::ExportClipPackage { .. }
        | Action::InstallDenizen { .. }
        | Action::ConfirmInstallDenizen
        | Action::UninstallDenizen { .. }
        | Action::RunDenizen { .. } => EffectClass::ExternalEffect,
        Action::OpenThemeWorkshop
        | Action::ComposeFocusedSmolwebSubmission
        | Action::ComposeFocusedMicronForm
        | Action::BeginSmolwebSubmission { .. }
        | Action::SmolwebSubmissionFile { .. }
        | Action::OpenDocumentFind
        | Action::CloseDocumentFind
        | Action::InsertDocumentFind(..)
        | Action::BackspaceDocumentFind
        | Action::StepDocumentFind(..)
        | Action::PageZoomIn { .. }
        | Action::PageZoomOut { .. }
        | Action::PageZoomReset { .. }
        | Action::SearchSavedSources
        | Action::FocusAuthenticationField(..)
        | Action::InsertAuthentication(..)
        | Action::BackspaceAuthentication
        | Action::ToggleAuthenticationMemory
        | Action::UnsubscribeFocusedFeed
        | Action::MarkFocusedFeedEntryRead
        | Action::NewWindow
        | Action::TearOutActivePane
        | Action::FloatActivePane
        | Action::DockActivePane
        | Action::ReturnActivePaneToPrimary
        | Action::SetViewerOverride { .. }
        | Action::ReseedLayout
        | Action::ResetToSeed
        | Action::SetSeed
        | Action::RestoreRecipeSeed
        | Action::ToggleForegroundPin
        | Action::DismissRetained
        | Action::GrowTiles
        | Action::ShrinkTiles
        | Action::FoldSelection
        | Action::ExpandFold
        | Action::ExpandAllFolds
        | Action::UndoFold
        | Action::RedoFold
        | Action::FitView
        | Action::SetLayoutStrategy(..)
        | Action::SetPhysicsLaw(..)
        | Action::SetPhysicsOverlay(..)
        | Action::SetPhysicsKindSource(..)
        | Action::SetPhysicsMassSource(..)
        | Action::SetPhysicsDepthSource(..)
        | Action::ApplyPhysicsProfile(..)
        | Action::SetPhysicsRole(..)
        | Action::SetPhysicsRoleGroups(..)
        | Action::ApplyPhysicsMix(..)
        | Action::ApplyPhysicsGrouping(..)
        | Action::ApplyPhysicsSchedule(..)
        | Action::UseSingleLaw
        | Action::SetPhysicsSpeed(..)
        | Action::ToggleTerritory
        | Action::ToggleIsometric
        | Action::OrbitBy(..)
        | Action::TiltBy(..)
        | Action::ToggleHeightByDegree
        | Action::TogglePhysics
        | Action::ToggleSizeByRecency
        | Action::BeginFoundPlace
        | Action::BeginExportPlaceCard
        | Action::BeginRefreshPlaceContact
        | Action::BeginOfferPlacePrekey
        | Action::BeginInviteToPlace
        | Action::BeginInviteToPlaceAsReader
        | Action::BeginJoinPlaceFile
        | Action::BeginSendPlaceMessage
        | Action::ShowPlaceStatus
        | Action::SetPlaceCollection(..)
        | Action::SearchCapturedPages
        | Action::OmnibarOpen { .. }
        | Action::OmnibarClose
        | Action::OmnibarChar(..)
        | Action::OmnibarInsert(..)
        | Action::OmnibarBackspace
        | Action::OmnibarDelete
        | Action::OmnibarCaret(..)
        | Action::OmnibarMove(..)
        | Action::OmnibarCommit
        | Action::OmnibarCommitRow(..)
        | Action::OmnibarShowAllCommands
        | Action::OmnibarToggleKeepRow(..)
        | Action::OmnibarToggleKeepSelected
        | Action::SummonPane(..)
        | Action::SetAnswerSelection { .. }
        | Action::SummonContributedPane { .. }
        | Action::CloseActivePane
        | Action::OuterWorkspace { .. }
        | Action::SetActivePaneDivider(..)
        | Action::SetSplitRatio { .. }
        | Action::ToggleMaximizePane
        | Action::TogglePaneSection { .. }
        | Action::MovePaneSection { .. }
        | Action::OpenInWorkbench
        | Action::SetWorkspaceMap(..)
        | Action::ReadGraphMember(..)
        | Action::NavigateRelationship(..)
        | Action::SetPaneBinding(..)
        | Action::RevealGraphMember(..)
        | Action::ToggleToolPane(..)
        | Action::SetChromeLayout(..)
        | Action::TearOutTile { .. }
        | Action::SwitchSession(..)
        | Action::BeginRenameSession
        | Action::RenameSession { .. }
        | Action::CancelInstallDenizen
        | Action::Reading(..)
        | Action::WorkbenchActivate(..)
        | Action::CloseWorkbenchTile
        | Action::WorkbenchStackOnto { .. }
        | Action::WorkbenchSplitBeside { .. }
        | Action::WorkbenchSplitOut { .. }
        | Action::WorkbenchSetFractions { .. } => EffectClass::Curation,
    };
    let destructive = matches!(
        action,
        Action::DeleteFocusedNode | Action::EmptyRecycleBin | Action::CloseSession
    );
    (effect, destructive)
}
