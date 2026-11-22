use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{
    ModeDoneOutcomeData, ModeGuideData, ReadingEditionData, ReadingItemData, ReadingItemDetailData,
    ReadingLinkData, ReadingUnsubscribeData,
};

pub const READING_LINKS_SHOWN: usize = 4;

#[derive(Debug, Clone, Copy)]
pub enum ReadingRow<'a> {
    Item(&'a ReadingItemData),
    Link(&'a ReadingItemData, &'a ReadingLinkData),
}

impl<'a> ReadingRow<'a> {
    pub fn key(&self) -> &'a str {
        match self {
            Self::Item(item) => &item.item_key,
            Self::Link(_, link) => &link.item_key,
        }
    }

    pub fn issue(&self) -> &'a ReadingItemData {
        match self {
            Self::Item(item) | Self::Link(item, _) => item,
        }
    }

    pub fn is_link(&self) -> bool {
        matches!(self, Self::Link(..))
    }

    pub fn domain(&self) -> Option<&'a str> {
        match self {
            Self::Item(item) => item.url.as_ref().and(item.domain.as_deref()),
            Self::Link(_, link) => Some(link.domain.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadingView {
    #[default]
    Issue,
    Article,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingUnsubscribeTarget {
    pub account_id: AccountId,
    pub sender_email: String,
    pub source: String,
    pub evidence: String,
    pub method: ReadingUnsubscribeData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadingConfirm {
    LetGoAll {
        thread_ids: Vec<ThreadId>,
        items: Vec<ModeDoneOutcomeData>,
    },
    Unsubscribe {
        target: ReadingUnsubscribeTarget,
        message_count: u32,
        preview_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HighlightsPanel {
    pub selected: usize,
    pub step: HighlightStep,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HighlightStep {
    #[default]
    Browse,
    Note(String),
    Remove,
}

#[derive(Debug, Clone, Default)]
pub struct ReadingPageState {
    pub edition: Option<ReadingEditionData>,
    pub guide: Option<ModeGuideData>,
    pub pending_refresh: bool,
    pub pending_mark_visit: bool,
    pub reader: Option<ReadingItemDetailData>,
    pub pending_item: Option<String>,
    pub item_in_flight: Option<String>,
    pub pending_original: Option<mxr_core::id::MessageId>,
    pub pending_fetch: Option<(String, bool)>,
    pub pending_let_go_preview: Option<Vec<ThreadId>>,
    pub pending_unsubscribe_preview: Option<ReadingUnsubscribeTarget>,
    pub confirm: Option<ReadingConfirm>,
    pub highlights: Option<HighlightsPanel>,
    pub later_shelf: bool,
    pub reader_focused: bool,
    pub view: ReadingView,
    pub scroll: u16,
    pub reader_lines: u16,
    pub opened_at: Option<std::time::Instant>,
    pub original_text: Option<String>,
}

impl ReadingPageState {
    pub fn rows(&self) -> Vec<ReadingRow<'_>> {
        let Some(edition) = &self.edition else {
            return Vec::new();
        };
        if self.later_shelf {
            return edition.later.iter().map(ReadingRow::Item).collect();
        }
        let mut rows = Vec::new();
        for band in &edition.bands {
            for item in &band.items {
                rows.push(ReadingRow::Item(item));
                rows.extend(
                    item.links
                        .iter()
                        .take(READING_LINKS_SHOWN)
                        .map(|link| ReadingRow::Link(item, link)),
                );
            }
        }
        rows
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    pub fn thread_ids(&self) -> Vec<ThreadId> {
        let mut out: Vec<ThreadId> = Vec::new();
        for row in self.rows() {
            let thread = &row.issue().thread_id;
            if !out.contains(thread) {
                out.push(thread.clone());
            }
        }
        out
    }

    pub fn hint_anchors(&self) -> (Option<&str>, Option<&str>) {
        let Some(edition) = self.edition.as_ref().filter(|_| !self.later_shelf) else {
            return (None, None);
        };
        let fading = edition
            .bands
            .iter()
            .find(|band| band.band == mxr_protocol::ReadingBandData::Fading)
            .and_then(|band| band.items.first())
            .map(|item| item.item_key.as_str());
        let link = edition
            .bands
            .iter()
            .flat_map(|band| &band.items)
            .find_map(|item| item.links.first())
            .map(|link| link.item_key.as_str());
        (fading, link)
    }

    pub fn remove_thread(&mut self, thread_id: &ThreadId) {
        if let Some(edition) = self.edition.as_mut() {
            for band in &mut edition.bands {
                band.items.retain(|item| &item.thread_id != thread_id);
            }
            edition.bands.retain(|band| !band.items.is_empty());
        }
        if self
            .reader
            .as_ref()
            .is_some_and(|reader| &reader.item.thread_id == thread_id)
        {
            self.reader = None;
            self.reader_focused = false;
        }
    }

    pub fn progress(&self) -> f64 {
        if self.reader_lines == 0 {
            return 0.0;
        }
        (f64::from(self.scroll) / f64::from(self.reader_lines)).clamp(0.0, 1.0)
    }
}
