#[cfg(feature = "local")]
use crate::mxr_core::types::AttachmentMeta;
#[cfg(feature = "local")]
use crate::mxr_reader::{clean, ReaderConfig};
#[cfg(feature = "local")]
use calamine::{open_workbook_auto, Reader};
#[cfg(feature = "local")]
use std::path::Path as StdPath;
#[cfg(feature = "local")]
use std::process::Command;

#[cfg(feature = "local")]
use super::text::normalize_text;
use super::OCR_MAX_PAGES;

#[cfg(feature = "local")]
pub(super) fn read_attachment_text(attachment: &AttachmentMeta) -> Option<String> {
    let path = attachment.local_path.as_ref()?;
    match attachment_kind(attachment, path) {
        AttachmentKind::Text => read_text_attachment(path, false),
        AttachmentKind::Html => read_text_attachment(path, true),
        AttachmentKind::Pdf => read_pdf_attachment(path),
        AttachmentKind::OfficeDocument => read_office_attachment(path),
        AttachmentKind::Spreadsheet => read_spreadsheet_attachment(attachment, path),
        AttachmentKind::Image => run_tesseract(path),
        AttachmentKind::Unknown => None,
    }
}

#[cfg(feature = "local")]
fn read_text_attachment(path: &StdPath, is_html: bool) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_html {
        return normalized_nonempty(&clean(None, Some(&content), &ReaderConfig::default()).content);
    }
    normalized_nonempty(&content)
}

#[cfg(feature = "local")]
fn read_office_attachment(path: &StdPath) -> Option<String> {
    let markdown = undoc::to_markdown(path).ok()?;
    normalized_nonempty(&markdown)
}

#[cfg(feature = "local")]
fn read_spreadsheet_attachment(attachment: &AttachmentMeta, path: &StdPath) -> Option<String> {
    let extension = attachment_extension(attachment, path);
    let mime = attachment.mime_type.to_ascii_lowercase();
    let undoc_text = should_try_undoc_spreadsheet(&mime, extension.as_deref())
        .then(|| read_office_attachment(path))
        .flatten();
    let table_text = read_spreadsheet_tables(path);
    combine_extracted_texts([undoc_text, table_text])
}

#[cfg(feature = "local")]
fn read_spreadsheet_tables(path: &StdPath) -> Option<String> {
    let mut workbook = open_workbook_auto(path).ok()?;
    let mut sections = Vec::new();

    for sheet_name in workbook.sheet_names().to_owned() {
        let Ok(range) = workbook.worksheet_range(&sheet_name) else {
            continue;
        };

        let mut rows = Vec::new();
        for row in range.rows() {
            let cells = row
                .iter()
                .map(ToString::to_string)
                .map(|cell| normalize_text(&cell))
                .filter(|cell: &String| !cell.is_empty())
                .collect::<Vec<_>>();
            if !cells.is_empty() {
                rows.push(cells.join(" | "));
            }
        }

        if !rows.is_empty() {
            sections.push(format!("sheet {sheet_name}\n{}", rows.join("\n")));
        }
    }

    normalized_nonempty(&sections.join("\n\n"))
}

#[cfg(feature = "local")]
fn should_try_undoc_spreadsheet(mime: &str, extension: Option<&str>) -> bool {
    mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        || matches!(extension, Some("xlsx"))
}

#[cfg(feature = "local")]
fn combine_extracted_texts<I>(parts: I) -> Option<String>
where
    I: IntoIterator<Item = Option<String>>,
{
    let mut combined = Vec::new();
    for part in parts.into_iter().flatten() {
        if combined.iter().any(|existing: &String| {
            existing == &part || existing.contains(&part) || part.contains(existing)
        }) {
            continue;
        }
        combined.push(part);
    }

    if combined.is_empty() {
        None
    } else {
        Some(combined.join("\n\n"))
    }
}

#[cfg(feature = "local")]
fn attachment_extension(attachment: &AttachmentMeta, path: &StdPath) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .or_else(|| attachment.filename.rsplit('.').next())
        .map(|ext| ext.trim().to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}

#[cfg(feature = "local")]
fn normalized_nonempty(text: &str) -> Option<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(feature = "local")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentKind {
    Text,
    Html,
    Pdf,
    OfficeDocument,
    Spreadsheet,
    Image,
    Unknown,
}

#[cfg(feature = "local")]
fn attachment_kind(attachment: &AttachmentMeta, path: &StdPath) -> AttachmentKind {
    let mime = attachment.mime_type.to_ascii_lowercase();
    let extension = attachment_extension(attachment, path);
    let extension = extension.as_deref();

    if mime == "text/html" || matches!(extension, Some("html" | "htm")) {
        return AttachmentKind::Html;
    }

    if mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/xml"
                | "application/x-yaml"
                | "application/yaml"
                | "application/markdown"
        )
        || matches!(
            extension,
            Some("txt" | "md" | "markdown" | "json" | "xml" | "yaml" | "yml" | "csv" | "tsv")
        )
    {
        return AttachmentKind::Text;
    }

    if mime == "application/pdf" || matches!(extension, Some("pdf")) {
        return AttachmentKind::Pdf;
    }

    if matches!(
        mime.as_str(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    ) || matches!(extension, Some("docx" | "pptx"))
    {
        return AttachmentKind::OfficeDocument;
    }

    if matches!(
        mime.as_str(),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-excel"
            | "application/vnd.ms-excel.sheet.binary.macroenabled.12"
            | "application/vnd.ms-excel.sheet.macroenabled.12"
            | "application/vnd.oasis.opendocument.spreadsheet"
    ) || matches!(extension, Some("xlsx" | "xlsm" | "xlsb" | "xls" | "ods"))
    {
        return AttachmentKind::Spreadsheet;
    }

    if mime.starts_with("image/")
        || matches!(
            extension,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff")
        )
    {
        return AttachmentKind::Image;
    }

    AttachmentKind::Unknown
}

#[cfg(feature = "local")]
fn read_pdf_attachment(path: &StdPath) -> Option<String> {
    if let Some(extracted) = unpdf::to_markdown(path)
        .ok()
        .and_then(|markdown| normalized_nonempty(&markdown))
    {
        return Some(extracted);
    }

    ocr_pdf(path)
}

#[cfg(feature = "local")]
fn ocr_pdf(path: &StdPath) -> Option<String> {
    let pdftoppm = which::which("pdftoppm").ok()?;
    let tempdir = tempfile::tempdir().ok()?;
    let prefix = tempdir.path().join("page");
    let status = Command::new(pdftoppm)
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg(OCR_MAX_PAGES.to_string())
        .arg("-png")
        .arg(path)
        .arg(&prefix)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    let mut images = std::fs::read_dir(tempdir.path())
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect::<Vec<_>>();
    images.sort();

    let mut output = String::new();
    for image in images {
        if let Some(text) = run_tesseract(&image) {
            if !output.is_empty() {
                output.push(' ');
            }
            output.push_str(&text);
        }
    }

    let normalized = normalize_text(&output);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(feature = "local")]
fn run_tesseract(path: &StdPath) -> Option<String> {
    let tesseract = which::which("tesseract").ok()?;
    let output = Command::new(tesseract)
        .arg(path)
        .arg("stdout")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let normalized = normalize_text(&String::from_utf8_lossy(&output.stdout));
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(all(test, feature = "local"))]
mod tests {
    use super::*;
    use crate::mxr_core::id::{AttachmentId, MessageId};
    use std::fs::File;
    use std::io::Write;
    use std::path::Path as StdPath;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn attachment(path: &StdPath, filename: &str, mime_type: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: AttachmentId::new(),
            message_id: MessageId::new(),
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes: std::fs::metadata(path).unwrap().len(),
            local_path: Some(path.to_path_buf()),
            provider_id: "att-1".to_string(),
        }
    }

    fn write_zip(path: &StdPath, files: &[(&str, String)]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, contents) in files {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_docx(path: &StdPath, text: &str) {
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
  <w:body>
    <w:p>
      <w:r><w:t>{text}</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#
                    ),
                ),
            ],
        );
    }

    fn write_pptx(path: &StdPath, text: &str) {
        write_zip(
            path,
            &[
                (
                    "[Content_Types].xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"#
                        .to_string(),
                ),
                (
                    "_rels/.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "ppt/presentation.xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldIdLst>
    <p:sldId id="256" r:id="rId1"/>
  </p:sldIdLst>
</p:presentation>"#
                        .to_string(),
                ),
                (
                    "ppt/_rels/presentation.xml.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "ppt/slides/slide1.xml",
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="2" name="Title 1"/>
          <p:cNvSpPr/>
          <p:nvPr/>
        </p:nvSpPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
          <a:p><a:r><a:t>{text}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#
                    ),
                ),
            ],
        );
    }

    fn write_xlsx(path: &StdPath) {
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
  <sheets>
    <sheet name="Summary" sheetId="1" r:id="rId1"/>
  </sheets>
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
    <row r="1">
      <c r="A1" t="s"><v>0</v></c>
      <c r="B1" t="s"><v>1</v></c>
    </row>
    <row r="2">
      <c r="A2" t="s"><v>2</v></c>
      <c r="B2" t="s"><v>3</v></c>
    </row>
  </sheetData>
</worksheet>"#
                        .to_string(),
                ),
            ],
        );
    }

    #[test]
    fn attachment_kind_uses_extension_when_mime_is_generic() {
        let dir = tempdir().unwrap();
        let docx_path = dir.path().join("roadmap.docx");
        write_docx(&docx_path, "Quarterly roadmap");
        let attachment = attachment(&docx_path, "roadmap.docx", "application/octet-stream");

        assert_eq!(
            attachment_kind(&attachment, docx_path.as_path()),
            AttachmentKind::OfficeDocument
        );
    }

    #[test]
    fn read_attachment_text_extracts_docx_with_undoc() {
        let dir = tempdir().unwrap();
        let docx_path = dir.path().join("roadmap.docx");
        write_docx(&docx_path, "Quarterly roadmap for launch");
        let attachment = attachment(
            &docx_path,
            "roadmap.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("quarterly roadmap"));
        assert!(extracted.contains("launch"));
    }

    #[test]
    fn read_attachment_text_extracts_pptx_with_undoc() {
        let dir = tempdir().unwrap();
        let pptx_path = dir.path().join("deck.pptx");
        write_pptx(&pptx_path, "Launch metrics");
        let attachment = attachment(
            &pptx_path,
            "deck.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("launch metrics"));
    }

    #[test]
    fn read_attachment_text_extracts_xlsx_with_table_fallback() {
        let dir = tempdir().unwrap();
        let xlsx_path = dir.path().join("table.xlsx");
        write_xlsx(&xlsx_path);
        let attachment = attachment(
            &xlsx_path,
            "table.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("sheet summary"));
        assert!(extracted.contains("name | value"));
        assert!(extracted.contains("alice | 42"));
    }
}
