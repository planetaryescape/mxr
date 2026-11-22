use mxr_core::id::MessageId;
use mxr_protocol::{
    ModeGuideData, RecordAnswerData, RecordAnswerListData, RecordChangeData, RecordData,
    RecordEditData, RecordExportData, RecordFilterData, RecordKindData, RecordLedgerData,
    RecordSubscriptionData, RecordSubscriptionsData,
};

pub const SUBSCRIPTIONS_CHIP: &str = "subscriptions";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFixPrompt {
    pub record_id: String,
    pub input: String,
    pub previewed: Option<RecordEditData>,
    pub preview: Option<RecordChangeData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassMenu {
    pub message_id: MessageId,
    pub preview: Option<RecordChangeData>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordsPageState {
    pub ledger: Option<RecordLedgerData>,
    pub guide: Option<ModeGuideData>,
    pub filter: RecordFilterData,
    pub kind_chip: usize,
    pub asking: bool,
    pub query: String,
    pub answer: Option<RecordAnswerData>,
    pub answer_list: bool,
    pub card: Option<RecordData>,
    pub prompt: Option<RecordFixPrompt>,
    pub export_preview: Option<RecordExportData>,
    pub pass_menu: Option<PassMenu>,
    pub subscriptions: Option<RecordSubscriptionsData>,
    pub subscription_open: bool,
    pub last_copied: Option<String>,
    pub pending_refresh: bool,
    pub pending_answer: Option<String>,
    pub pending_card: Option<String>,
    pub pending_export: Option<bool>,
    pub pending_fix_preview: Option<(String, RecordEditData)>,
    pub pending_file_preview: Option<MessageId>,
    pub pending_open: Option<MessageId>,
}

impl RecordsPageState {
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

    pub fn subscription_rows(&self) -> &[RecordSubscriptionData] {
        self.subscriptions
            .as_ref()
            .map_or(&[][..], |data| data.subscriptions.as_slice())
    }

    pub fn answer_record(&self) -> Option<&RecordData> {
        self.answer
            .as_ref()
            .filter(|answer| answer.list.is_none())
            .and_then(|answer| answer.answer.as_ref())
            .map(|card| &card.record)
    }

    pub fn current_year(&self) -> Option<i32> {
        self.filter.year.or_else(|| {
            self.ledger
                .as_ref()
                .and_then(|ledger| ledger.facets.years.first())
                .and_then(|year| year.value.parse().ok())
        })
    }
}
