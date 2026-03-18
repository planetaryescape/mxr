use tantivy::schema::*;

pub struct MxrSchema {
    pub schema: Schema,
    pub message_id: Field,
    pub account_id: Field,
    pub thread_id: Field,
    pub subject: Field,
    pub from_name: Field,
    pub from_email: Field,
    pub to_email: Field,
    pub snippet: Field,
    pub body_text: Field,
    pub labels: Field,
    pub date: Field,
    pub flags: Field,
    pub has_attachments: Field,
    pub is_read: Field,
    pub is_starred: Field,
}

impl MxrSchema {
    pub fn build() -> Self {
        let mut builder = Schema::builder();

        let message_id = builder.add_text_field("message_id", STRING | STORED);
        let account_id = builder.add_text_field("account_id", STRING | STORED);
        let thread_id = builder.add_text_field("thread_id", STRING | STORED);

        let subject = builder.add_text_field("subject", TEXT);
        let from_name = builder.add_text_field("from_name", TEXT);
        let from_email = builder.add_text_field("from_email", STRING);
        let to_email = builder.add_text_field("to_email", STRING);
        let snippet = builder.add_text_field("snippet", TEXT);
        let body_text = builder.add_text_field("body_text", TEXT);

        let labels = builder.add_text_field("labels", STRING);
        let date = builder.add_date_field("date", INDEXED | STORED);
        let flags = builder.add_u64_field("flags", INDEXED);
        let has_attachments = builder.add_bool_field("has_attachments", INDEXED);
        let is_read = builder.add_bool_field("is_read", INDEXED);
        let is_starred = builder.add_bool_field("is_starred", INDEXED);

        let schema = builder.build();

        Self {
            schema,
            message_id,
            account_id,
            thread_id,
            subject,
            from_name,
            from_email,
            to_email,
            snippet,
            body_text,
            labels,
            date,
            flags,
            has_attachments,
            is_read,
            is_starred,
        }
    }
}
