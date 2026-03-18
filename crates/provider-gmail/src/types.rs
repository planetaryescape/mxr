use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailListResponse {
    pub messages: Option<Vec<GmailMessageRef>>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessageRef {
    pub id: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    pub label_ids: Option<Vec<String>>,
    pub snippet: Option<String>,
    pub history_id: Option<String>,
    pub internal_date: Option<String>,
    pub size_estimate: Option<u64>,
    pub payload: Option<GmailPayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailPayload {
    pub mime_type: Option<String>,
    pub headers: Option<Vec<GmailHeader>>,
    pub body: Option<GmailBody>,
    pub parts: Option<Vec<GmailPayload>>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailBody {
    pub attachment_id: Option<String>,
    pub size: Option<u64>,
    pub data: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryResponse {
    pub history: Option<Vec<GmailHistoryRecord>>,
    pub next_page_token: Option<String>,
    pub history_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryRecord {
    pub id: String,
    pub messages: Option<Vec<GmailMessageRef>>,
    pub messages_added: Option<Vec<GmailHistoryMessageAdded>>,
    pub messages_deleted: Option<Vec<GmailHistoryMessageDeleted>>,
    pub labels_added: Option<Vec<GmailHistoryLabelAdded>>,
    pub labels_removed: Option<Vec<GmailHistoryLabelRemoved>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryMessageAdded {
    pub message: GmailMessageRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryMessageDeleted {
    pub message: GmailMessageRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryLabelAdded {
    pub message: GmailMessageRef,
    pub label_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryLabelRemoved {
    pub message: GmailMessageRef,
    pub label_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabelsResponse {
    pub labels: Option<Vec<GmailLabel>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: Option<String>,
    pub messages_total: Option<u32>,
    pub messages_unread: Option<u32>,
    pub color: Option<GmailLabelColor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabelColor {
    pub text_color: Option<String>,
    pub background_color: Option<String>,
}
