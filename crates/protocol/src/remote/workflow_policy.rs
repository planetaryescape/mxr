//! Explicit permissions for portable remote mail workflows.
/// Organization authority never includes host settings, executable rules or filesystem operations.
pub fn remote_triage_allowed(request: &crate::Request) -> bool {
    matches!(
        request,
        crate::Request::SyncNow { .. }
            | crate::Request::PreviewSyncAll
            | crate::Request::SyncAll
            | crate::Request::CreateLabel { .. }
            | crate::Request::DeleteLabel { .. }
            | crate::Request::RenameLabel { .. }
            | crate::Request::CreateCategory { .. }
            | crate::Request::RenameCategory { .. }
            | crate::Request::PreviewCategoryMembership { .. }
            | crate::Request::ApplyCategoryMembership { .. }
            | crate::Request::PreviewDeleteCategory { .. }
            | crate::Request::DeleteCategory { .. }
            | crate::Request::UndoCategoryMembership { .. }
            | crate::Request::CreateSavedSearch { .. }
            | crate::Request::DeleteSavedSearch { .. }
            | crate::Request::UpdateSavedSearch { .. }
            | crate::Request::SetFlags { .. }
            | crate::Request::Mutation { .. }
            | crate::Request::StartMutationJob { .. }
            | crate::Request::PreviewArchive { .. }
            | crate::Request::SubmitArchive { .. }
            | crate::Request::PreviewSelection { .. }
            | crate::Request::StartSelectionJob { .. }
            | crate::Request::UndoMutation { .. }
            | crate::Request::Unsubscribe { .. }
            | crate::Request::UnsubscribePurge { .. }
            | crate::Request::CommitUnsubscribePreview { .. }
            | crate::Request::PlanUnsubscribe { .. }
            | crate::Request::Snooze { .. }
            | crate::Request::Unsnooze { .. }
            | crate::Request::SetReplyLater { .. }
            | crate::Request::SetAutoReminder { .. }
            | crate::Request::CancelAutoReminder { .. }
            | crate::Request::ResolveDelivery { .. }
            | crate::Request::DismissDelivery { .. }
            | crate::Request::ResolveCommitment { .. }
            | crate::Request::SetScreenerDecision { .. }
            | crate::Request::ClearScreenerDecision { .. }
            | crate::Request::DismissDeskThreads { .. }
            | crate::Request::RestoreDeskThreads { .. }
            | crate::Request::RecordPromise { .. }
            | crate::Request::SetTodoState { .. }
            | crate::Request::CorrectTodo { .. }
            | crate::Request::UndoTodoCorrection { .. }
            | crate::Request::ResolveTodoSuggestion { .. }
            | crate::Request::ScheduleTodo { .. }
            | crate::Request::UpdateTodo { .. }
            | crate::Request::CreateTodo { .. }
            | crate::Request::SetTodoCatchup { .. }
            | crate::Request::SetHintSeen { .. }
            | crate::Request::SetModeRowSeen { .. }
            | crate::Request::MoveMessage { .. }
            | crate::Request::UndoMove { .. }
            | crate::Request::SetModeDone { .. }
            | crate::Request::AckMessage { .. }
            | crate::Request::MergePeople { .. }
            | crate::Request::SplitPerson { .. }
            | crate::Request::SetRecordField { .. }
            | crate::Request::DismissRecord { .. }
            | crate::Request::FileRecord { .. }
            | crate::Request::SetRecordSender { .. }
            | crate::Request::SetReadingLater { .. }
            | crate::Request::RecordReadingEngagement { .. }
            | crate::Request::FetchArticle { .. }
            | crate::Request::SaveHighlight { .. }
            | crate::Request::UpdateHighlight { .. }
            | crate::Request::RemoveHighlight { .. }
            | crate::Request::SetReadingSource { .. }
            | crate::Request::LetGoDigest { .. }
            | crate::Request::SetUpdateSource { .. }
            | crate::Request::SetSenderKind { .. }
            | crate::Request::PinMessages { .. }
            | crate::Request::SweepPlace { .. }
            | crate::Request::ResolveDeskItems { .. }
            | crate::Request::DeferThreads { .. }
    )
}

/// Mail intelligence still passes the host privacy gates. Assistant tool execution needs its own scoped admission.
pub fn remote_ai_allowed(request: &crate::Request) -> bool {
    matches!(
        request,
        crate::Request::GetThreadGist { .. }
            | crate::Request::GetThreadGists { .. }
            | crate::Request::SummarizeThread { .. }
            | crate::Request::TriageSearch { .. }
            | crate::Request::HumanizerScore { .. }
            | crate::Request::HumanizerRewrite { .. }
            | crate::Request::DraftCompose { .. }
            | crate::Request::DraftRefine { .. }
            | crate::Request::ComposeAssist { .. }
            | crate::Request::ApplyComposeProposal { .. }
            | crate::Request::ExtractDraftCommitments { .. }
            | crate::Request::ExplainEntity { .. }
            | crate::Request::FindExpert { .. }
            | crate::Request::SuggestCollaborators { .. }
            | crate::Request::GetThreadBriefing { .. }
            | crate::Request::GetRecipientBriefing { .. }
            | crate::Request::SendTimeRecommendation { .. }
            | crate::Request::ArchiveAsk { .. }
            | crate::Request::ListOwedReplies { .. }
            | crate::Request::DetectPromises { .. }
            | crate::Request::AnswerFromRecords { .. }
            | crate::Request::GetAssistantAvailability
            | crate::Request::GetAssistantModels
            | crate::Request::GetComposeModels
            | crate::Request::AssistantSend { .. }
            | crate::Request::AssistantDecide { .. }
            | crate::Request::AssistantCancel { .. }
            | crate::Request::SetAssistantAutoApprove { .. }
            | crate::Request::ListAssistantConversations { .. }
            | crate::Request::GetAssistantConversation { .. }
            | crate::Request::GetAssistantStepRows { .. }
            | crate::Request::DeleteAssistantConversation { .. }
    )
}
