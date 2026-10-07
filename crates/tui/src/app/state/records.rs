//! Archive in the TUI: the ledger the daemon serves (`ListRecords`), the
//! answer box (`AnswerFromRecords`), a record's card (`GetRecord`), the
//! export preview and the mode's teaching copy. The daemon owns every
//! record, total and label; this state only holds what came back and what
//! the user is doing with it.

use mxr_core::id::MessageId;
use mxr_protocol::{
    ModeGuideData, RecordAnswerData, RecordChangeData, RecordData, RecordEditData,
    RecordExportData, RecordFilterData, RecordKindData, RecordLedgerData,
};

/// The kind chips under the answer box, in order. `g f` steps through
/// them; the first shows every kind.
pub const RECORD_KIND_CHIPS: &[(&str, &[RecordKindData])] = &[
    ("all", &[]),
    ("receipts", &[RecordKindData::Receipt]),
    ("orders", &[RecordKindData::Order]),
    ("trips", &[RecordKindData::Booking, RecordKindData::Ticket]),
    (
        "bills",
        &[RecordKindData::Invoice, RecordKindData::Statement],
    ),
    (
        "docs",
        &[
            RecordKindData::Contract,
            RecordKindData::Warranty,
            RecordKindData::Account,
        ],
    ),
];

/// The `,` prompt: `field=value`, previewed by the daemon before it is
/// applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFixPrompt {
    pub record_id: String,
    pub input: String,
    /// The edit the preview was asked for; Enter applies exactly this.
    pub previewed: Option<RecordEditData>,
    /// The daemon's dry run: "Would do: Fixed amount." and the card after.
    pub preview: Option<RecordChangeData>,
    pub error: Option<String>,
}

/// `T` on a conversation: pass it to a mode. The menu, then the card
/// filing it in Archive would make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassMenu {
    pub message_id: MessageId,
    /// FileRecord's dry run, once asked for; Enter files exactly this
    /// message.
    pub preview: Option<RecordChangeData>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordsPageState {
    pub ledger: Option<RecordLedgerData>,
    pub guide: Option<ModeGuideData>,
    /// What the ledger is filtered to: kind chip, year, issuer page.
    pub filter: RecordFilterData,
    /// Index into `RECORD_KIND_CHIPS`.
    pub kind_chip: usize,
    /// The answer box is taking keys.
    pub asking: bool,
    pub query: String,
    pub answer: Option<RecordAnswerData>,
    /// The full card (`GetRecord`) drawn over the ledger.
    pub card: Option<RecordData>,
    pub prompt: Option<RecordFixPrompt>,
    /// The export's dry run, waiting for Enter or Esc.
    pub export_preview: Option<RecordExportData>,
    pub pass_menu: Option<PassMenu>,
    /// What `y` or `Y` last copied, said back in the status line.
    pub last_copied: Option<String>,
    /// Closed here before the daemon answered, so it never flickers back.
    pub card_closed: bool,
    pub card_close_mutation: Option<crate::app::MutationId>,
    // Work for the runtime.
    pub pending_refresh: bool,
    pub pending_answer: Option<String>,
    pub pending_card: Option<String>,
    /// `true` writes the CSV; `false` asks for the preview.
    pub pending_export: Option<bool>,
    pub pending_fix_preview: Option<(String, RecordEditData)>,
    pub pending_file_preview: Option<MessageId>,
    /// The email to open: a record's newest source message.
    pub pending_open: Option<MessageId>,
}

impl RecordsPageState {
    /// The ledger's rows, newest first.
    pub fn rows(&self) -> &[RecordData] {
        self.ledger
            .as_ref()
            .map_or(&[][..], |ledger| ledger.records.as_slice())
    }

    pub fn row_count(&self) -> usize {
        self.rows().len()
    }

    /// The first-encounter card shows above the ledger once there are
    /// records, until it is closed here or retired anywhere.
    pub fn card_visible(&self) -> bool {
        !self.card_closed
            && self.guide.as_ref().is_some_and(|guide| !guide.card_seen)
            && !self.rows().is_empty()
    }

    /// The record an answer is about, while an answer is on screen.
    pub fn answer_record(&self) -> Option<&RecordData> {
        self.answer
            .as_ref()
            .and_then(|answer| answer.answer.as_ref())
            .map(|card| &card.record)
    }

    /// The year the ledger is stepping through: the filter's, else the
    /// newest record's.
    pub fn current_year(&self) -> Option<i32> {
        self.filter.year.or_else(|| {
            self.ledger
                .as_ref()
                .and_then(|ledger| ledger.facets.years.first())
                .and_then(|year| year.value.parse().ok())
        })
    }
}
