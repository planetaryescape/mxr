//! Updates in the TUI: the twice-daily briefing the daemon serves
//! (`GetUpdatesDigest`) and its teaching copy (`GetModeGuide`). The daemon
//! owns the cut, the sections, every line and the let-go selection; this
//! state holds what came back and what the user is doing with it.

use mxr_protocol::{
    ModeGuideData, UpdateLineData, UpdateSourceSettingData, UpdatesDigestData, UpdatesLetGoData,
};

/// Routine lines shown before the rest fold into "> N quieter sources".
pub const ROUTINE_SHOWN: usize = 4;

/// One selectable row in the briefing, in section order.
#[derive(Debug, Clone, Copy)]
pub enum UpdatesRow<'a> {
    Line(&'a UpdateLineData),
    /// The folded routine lines; Enter unfolds them.
    MoreRoutine(usize),
}

/// The `K` menu for one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatesTuneMenu {
    pub account_id: mxr_core::AccountId,
    pub source_key: String,
    pub source_name: String,
    pub current: UpdateSourceSettingData,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatesPageState {
    pub digest: Option<UpdatesDigestData>,
    pub guide: Option<ModeGuideData>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    pub card_close_mutation: Option<crate::app::MutationId>,
    /// Ask the runtime for the digest and its guide.
    pub pending_refresh: bool,
    pub routine_open: bool,
    /// Ask the daemon what letting go of the whole digest would do.
    pub pending_let_go_preview: bool,
    /// The daemon's dry run: Enter lets go of exactly this selection.
    pub let_go_preview: Option<UpdatesLetGoData>,
    pub tune: Option<UpdatesTuneMenu>,
}

impl UpdatesPageState {
    /// Selectable rows: Needs a look, Changed, then Routine (folded after
    /// `ROUTINE_SHOWN` until opened).
    pub fn rows(&self) -> Vec<UpdatesRow<'_>> {
        let Some(digest) = &self.digest else {
            return Vec::new();
        };
        let mut rows: Vec<UpdatesRow<'_>> = digest
            .needs_a_look
            .iter()
            .chain(&digest.changed)
            .map(UpdatesRow::Line)
            .collect();
        let shown = if self.routine_open {
            digest.routine.len()
        } else {
            digest.routine.len().min(ROUTINE_SHOWN)
        };
        rows.extend(digest.routine[..shown].iter().map(UpdatesRow::Line));
        if shown < digest.routine.len() {
            rows.push(UpdatesRow::MoreRoutine(digest.routine.len() - shown));
        }
        rows
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    /// The first-encounter card shows at the top once the digest has
    /// lines, until it is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && self.digest.as_ref().is_some_and(|digest| {
                !(digest.needs_a_look.is_empty()
                    && digest.changed.is_empty()
                    && digest.routine.is_empty())
            })
    }

    /// Take a source's lines off the page before the daemon answers.
    pub fn remove_source(&mut self, source_key: &str) {
        if let Some(digest) = self.digest.as_mut() {
            for section in [
                &mut digest.needs_a_look,
                &mut digest.changed,
                &mut digest.routine,
            ] {
                section.retain(|line| line.source_key != source_key);
            }
        }
    }

    /// Clear the digest's lines once it is let go; since stays.
    pub fn clear_digest(&mut self) {
        if let Some(digest) = self.digest.as_mut() {
            digest.needs_a_look.clear();
            digest.changed.clear();
            digest.routine.clear();
        }
    }
}
