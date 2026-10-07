//! Reading in the TUI: the edition the daemon serves
//! (`GetReadingEdition`), one item in the reader (`GetReadingItem`), the
//! mode's teaching copy, and the two confirmations (let go of everything
//! shown, unsubscribe). The daemon owns every band, rank and line; this
//! state only holds what came back and what the user is doing with it.

use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{
    ModeDoneOutcomeData, ModeGuideData, ReadingEditionData, ReadingItemData, ReadingItemDetailData,
    ReadingLinkData, ReadingUnsubscribeData,
};

/// Digest links shown under their issue; the rest are "+ N more".
pub const READING_LINKS_SHOWN: usize = 4;

/// One selectable row of the edition: an item, or one of a digest's
/// links under it.
#[derive(Debug, Clone, Copy)]
pub enum ReadingRow<'a> {
    Item(&'a ReadingItemData),
    Link(&'a ReadingItemData, &'a ReadingLinkData),
}

impl<'a> ReadingRow<'a> {
    /// The item key the reader, Later and highlights take.
    pub fn key(&self) -> &'a str {
        match self {
            Self::Item(item) => &item.item_key,
            Self::Link(_, link) => &link.item_key,
        }
    }

    /// The issue the row belongs to: its thread, sender and source.
    pub fn issue(&self) -> &'a ReadingItemData {
        match self {
            Self::Item(item) | Self::Link(item, _) => item,
        }
    }

    pub fn is_link(&self) -> bool {
        matches!(self, Self::Link(..))
    }

    /// The domain a fetch would contact, when the row points at an article.
    pub fn domain(&self) -> Option<&'a str> {
        match self {
            Self::Item(item) => item.url.as_ref().and(item.domain.as_deref()),
            Self::Link(_, link) => Some(link.domain.as_str()),
        }
    }
}

/// Which text the reader shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadingView {
    #[default]
    Issue,
    Article,
}

/// What `D` asks about before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingUnsubscribeTarget {
    pub account_id: AccountId,
    pub sender_email: String,
    pub source: String,
    /// "You opened 0 of the last 11 issues."
    pub evidence: String,
    pub method: ReadingUnsubscribeData,
}

/// A preview on screen, waiting for Enter or Esc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadingConfirm {
    /// `A`: the daemon's dry run of letting go of these threads. Enter
    /// lets go of exactly these.
    LetGoAll {
        thread_ids: Vec<ThreadId>,
        items: Vec<ModeDoneOutcomeData>,
    },
    /// `D`: the daemon's dry run of unsubscribing from a source.
    Unsubscribe {
        target: ReadingUnsubscribeTarget,
        /// Issues in the mailbox that leave with it.
        message_count: u32,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ReadingPageState {
    pub edition: Option<ReadingEditionData>,
    pub guide: Option<ModeGuideData>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    pub card_close_mutation: Option<crate::app::MutationId>,
    /// Ask the runtime for the edition and the guide.
    pub pending_refresh: bool,
    /// The next refresh counts as a visit. Only opening the lens does.
    pub pending_mark_visit: bool,
    /// The item in the reader pane.
    pub reader: Option<ReadingItemDetailData>,
    /// Ask the runtime for this item (a refetch after a change).
    pub pending_item: Option<String>,
    /// The item the runtime is fetching, so the cursor asks once.
    pub item_in_flight: Option<String>,
    /// Ask the runtime for this email's own text, for `R`.
    pub pending_original: Option<mxr_core::id::MessageId>,
    /// Ask the runtime to fetch this item's article (`refresh`).
    pub pending_fetch: Option<(String, bool)>,
    /// Ask the daemon what letting go of these threads would do.
    pub pending_let_go_preview: Option<Vec<ThreadId>>,
    /// Ask the daemon what unsubscribing would do.
    pub pending_unsubscribe_preview: Option<ReadingUnsubscribeTarget>,
    pub confirm: Option<ReadingConfirm>,
    /// `B`: the Later shelf instead of the edition.
    pub later_shelf: bool,
    /// The keyboard is in the reader pane (Enter), not the list.
    pub reader_focused: bool,
    pub view: ReadingView,
    /// Reader lines scrolled past.
    pub scroll: u16,
    /// Lines the reader drew last, for progress.
    pub reader_lines: u16,
    /// When the reader was entered, for the time-read report.
    pub opened_at: Option<std::time::Instant>,
    /// The email's own text, for `R` on a source that asked for it.
    pub original_text: Option<String>,
}

impl ReadingPageState {
    /// Selectable rows in display order: each band's items, a digest's
    /// first links under it.
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

    /// Every thread in the edition, once each, in order: what `A` lets go.
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

    /// The first-encounter card shows at the top once Reading has items,
    /// until it is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && self.row_count() > 0
    }

    /// Take a thread out of the edition before the daemon answers.
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

    /// How far down the reader is, 0 to 1.
    pub fn progress(&self) -> f64 {
        if self.reader_lines == 0 {
            return 0.0;
        }
        (f64::from(self.scroll) / f64::from(self.reader_lines)).clamp(0.0, 1.0)
    }
}
