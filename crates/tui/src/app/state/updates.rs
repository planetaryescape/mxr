
use mxr_protocol::{
    ModeGuideData, UpdateLineData, UpdateSourceSettingData, UpdatesDigestData, UpdatesLetGoData,
};

pub const ROUTINE_SHOWN: usize = 4;

#[derive(Debug, Clone, Copy)]
pub enum UpdatesRow<'a> {
    Line(&'a UpdateLineData),
    MoreRoutine(usize),
}

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
    pub pending_refresh: bool,
    pub routine_open: bool,
    pub pending_let_go_preview: bool,
    pub let_go_source: Option<(mxr_core::id::AccountId, String)>,
    pub let_go_preview: Option<UpdatesLetGoData>,
    pub tune: Option<UpdatesTuneMenu>,
    pub last_copied: Option<String>,
    pub pending_latest_code: bool,
    pub peek: Option<String>,
}

impl UpdatesPageState {
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

    pub fn clear_digest(&mut self) {
        if let Some(digest) = self.digest.as_mut() {
            digest.needs_a_look.clear();
            digest.changed.clear();
            digest.routine.clear();
        }
    }
}
