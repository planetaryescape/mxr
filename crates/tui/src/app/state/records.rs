//! Archive in the TUI: the ledger the daemon serves (`ListRecords`), the
//! answer box (`AnswerFromRecords`), a record's card (`GetRecord`), the
//! subscriptions (`ListRecordSubscriptions`), the export preview and the
//! mode's teaching copy. The daemon owns every
//! record, total and label; this state only holds what came back and what
//! the user is doing with it.

use mxr_core::id::MessageId;
use mxr_protocol::{
    ModeGuideData, RecordAnswerData, RecordAnswerListData, RecordChangeData, RecordData,
    RecordEditData, RecordExportData, RecordFilterData, RecordKindData, RecordLedgerData,
    RecordSubscriptionData, RecordSubscriptionsData,
};

/// The chip that swaps the ledger for the subscriptions.
pub const SUBSCRIPTIONS_CHIP: &str = "subscriptions";

/// The kind chips under the answer box, in order. `g f` steps through
/// them; the first shows every kind, the last the subscriptions.
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
    (SUBSCRIPTIONS_CHIP, &[]),
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
    /// `a` asked for every match of the query on screen as a list.
    pub answer_list: bool,
    /// The full card (`GetRecord`) drawn over the ledger.
    pub card: Option<RecordData>,
    pub prompt: Option<RecordFixPrompt>,
    /// The export's dry run, waiting for Enter or Esc.
    pub export_preview: Option<RecordExportData>,
    pub pass_menu: Option<PassMenu>,
    /// The subscriptions, while their chip is on.
    pub subscriptions: Option<RecordSubscriptionsData>,
    /// The selected subscription's card is open over the list.
    pub subscription_open: bool,
    /// What `y` or `Y` last copied, said back in the status line.
    pub last_copied: Option<String>,
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
    /// The rows on screen, newest first: a list answer's matches while one
    /// is showing, else the ledger's.
    pub fn rows(&self) -> &[RecordData] {
        if self.showing_subscriptions() {
            return &[];
        }
        if let Some(list) = self.listed_matches() {
            return list.records.as_slice();
        }
        self.ledger
            .as_ref()
            .map_or(&[][..], |ledger| ledger.records.as_slice())
    }

    /// Every match of a broad query, while it is on screen.
    pub fn listed_matches(&self) -> Option<&RecordAnswerListData> {
        self.answer
            .as_ref()
            .and_then(|answer| answer.list.as_deref())
    }

    pub fn row_count(&self) -> usize {
        if self.showing_subscriptions() {
            return self.subscription_rows().len();
        }
        self.rows().len()
    }

    pub fn showing_subscriptions(&self) -> bool {
        RECORD_KIND_CHIPS
            .get(self.kind_chip)
            .is_some_and(|chip| chip.0 == SUBSCRIPTIONS_CHIP)
    }

    /// Live subscriptions first, then ended, as the daemon ordered them.
    pub fn subscription_rows(&self) -> &[RecordSubscriptionData] {
        self.subscriptions
            .as_ref()
            .map_or(&[][..], |data| data.subscriptions.as_slice())
    }

    /// The record an answer card is about, while one is on screen. A list
    /// has no card: the keys act on its rows.
    pub fn answer_record(&self) -> Option<&RecordData> {
        self.answer
            .as_ref()
            .filter(|answer| answer.list.is_none())
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
