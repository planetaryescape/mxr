use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mxr_config::SemanticConfig;
use mxr_core::id::{AccountId, AttachmentId, MessageId, ThreadId};
use mxr_core::types::{
    Account, Address, AttachmentDisposition, AttachmentMeta, BackendRef, Envelope, MessageBody,
    MessageFlags, MessageMetadata, ProviderKind, UnsubscribeMethod,
};
use mxr_semantic::SemanticEngine;
use mxr_store::Store;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn bench_semantic_ingest(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");

    c.bench_function("semantic_ingest_mixed_attachments", |b| {
        let temp = TempDir::new().expect("temp dir");
        let (mut engine, message_ids) =
            runtime.block_on(async { semantic_bench_fixture(temp.path()).await });

        b.iter(|| {
            runtime.block_on(async {
                engine
                    .ingest_messages(black_box(&message_ids))
                    .await
                    .expect("semantic ingest");
            });
        });
    });
}

async fn semantic_bench_fixture(data_dir: &Path) -> (SemanticEngine, Vec<MessageId>) {
    let store = Arc::new(Store::in_memory().await.expect("store"));
    let account_id = AccountId::new();
    store
        .insert_account(&Account {
            id: account_id.clone(),
            name: "Bench".into(),
            email: "bench@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "bench".into(),
            }),
            send_backend: None,
            enabled: true,
        })
        .await
        .expect("insert account");

    let attachments = build_attachment_files(data_dir);
    let mut message_ids = Vec::new();
    for index in 0..4 {
        let envelope = Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("semantic-bench-{index}"),
            thread_id: ThreadId::new(),
            message_id_header: Some(format!("<semantic-bench-{index}@example.com>")),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Bench".into()),
                email: "bench@example.com".into(),
            },
            to: vec![Address {
                name: None,
                email: "team@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: format!("Semantic benchmark {index}"),
            date: chrono::Utc::now(),
            flags: MessageFlags::READ,
            snippet: "semantic ingest benchmark".into(),
            has_attachments: true,
            size_bytes: 4096,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec!["INBOX".into()],
        };
        let body = MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some(format!(
                "Launch checklist {index}. Deployment timeline. Performance notes."
            )),
            text_html: Some(format!(
                "<p><strong>Launch</strong> checklist {index} with deployment timeline.</p>"
            )),
            attachments: attachments
                .iter()
                .enumerate()
                .map(|(att_index, attachment)| AttachmentMeta {
                    id: AttachmentId::new(),
                    message_id: envelope.id.clone(),
                    filename: attachment.filename.clone(),
                    mime_type: attachment.mime_type.clone(),
                    disposition: AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: std::fs::metadata(&attachment.path)
                        .expect("attachment metadata")
                        .len(),
                    local_path: Some(attachment.path.clone()),
                    provider_id: format!("att-{index}-{att_index}"),
                })
                .collect(),
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        };

        store
            .upsert_envelope(&envelope)
            .await
            .expect("upsert envelope");
        store.insert_body(&body).await.expect("insert body");
        message_ids.push(envelope.id);
    }

    (
        SemanticEngine::new(store, data_dir, SemanticConfig::default()),
        message_ids,
    )
}

struct BenchAttachmentFile {
    path: PathBuf,
    filename: String,
    mime_type: String,
}

fn build_attachment_files(dir: &Path) -> Vec<BenchAttachmentFile> {
    let txt_path = dir.join("notes.txt");
    std::fs::write(&txt_path, "Deployment notes and launch follow-up").expect("write txt");

    let html_path = dir.join("newsletter.html");
    std::fs::write(
        &html_path,
        "<html><body><h1>Release</h1><p>Launch status and rollout details.</p></body></html>",
    )
    .expect("write html");

    let docx_path = dir.join("roadmap.docx");
    write_docx(&docx_path, "Roadmap milestones and launch owners");

    let xlsx_path = dir.join("metrics.xlsx");
    write_xlsx(&xlsx_path);

    let pdf_path = dir.join("scan.pdf");
    std::fs::write(&pdf_path, b"not-a-real-pdf").expect("write pdf");

    vec![
        BenchAttachmentFile {
            path: txt_path,
            filename: "notes.txt".into(),
            mime_type: "text/plain".into(),
        },
        BenchAttachmentFile {
            path: html_path,
            filename: "newsletter.html".into(),
            mime_type: "text/html".into(),
        },
        BenchAttachmentFile {
            path: docx_path,
            filename: "roadmap.docx".into(),
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .into(),
        },
        BenchAttachmentFile {
            path: xlsx_path,
            filename: "metrics.xlsx".into(),
            mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
        },
        BenchAttachmentFile {
            path: pdf_path,
            filename: "scan.pdf".into(),
            mime_type: "application/pdf".into(),
        },
    ]
}

fn write_zip(path: &Path, files: &[(&str, String)]) {
    let file = File::create(path).expect("zip file");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for (name, contents) in files {
        zip.start_file(name, options).expect("zip entry");
        zip.write_all(contents.as_bytes()).expect("zip write");
    }
    zip.finish().expect("zip finish");
}

fn write_docx(path: &Path, text: &str) {
    write_zip(
        path,
        &[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#
                    .to_string(),
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
                    .to_string(),
            ),
            (
                "word/document.xml",
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body>
</w:document>"#
                ),
            ),
        ],
    );
}

fn write_xlsx(path: &Path) {
    write_zip(
        path,
        &[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"#
                    .to_string(),
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#
                    .to_string(),
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="Summary" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#
                    .to_string(),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"#
                    .to_string(),
            ),
            (
                "xl/sharedStrings.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="4" uniqueCount="4">
  <si><t>Name</t></si>
  <si><t>Value</t></si>
  <si><t>Alice</t></si>
  <si><t>42</t></si>
</sst>"#
                    .to_string(),
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>
    <row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2" t="s"><v>3</v></c></row>
  </sheetData>
</worksheet>"#
                    .to_string(),
            ),
        ],
    );
}

criterion_group!(benches, bench_semantic_ingest);
criterion_main!(benches);
