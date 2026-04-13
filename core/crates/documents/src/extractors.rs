use crate::{DocumentError, DocumentFormat};
use std::io::Cursor;
use tracing::debug;

/// Extracts plain text from document bytes.
pub fn extract_text(data: &[u8], format: DocumentFormat) -> Result<String, DocumentError> {
    match format {
        DocumentFormat::PlainText | DocumentFormat::Markdown => extract_utf8(data),
        DocumentFormat::Pdf => extract_pdf(data),
        DocumentFormat::Docx => extract_docx(data),
    }
}

// ---------------------------------------------------------------------------
// UTF-8 text (TXT, Markdown)
// ---------------------------------------------------------------------------

fn extract_utf8(data: &[u8]) -> Result<String, DocumentError> {
    String::from_utf8(data.to_vec()).map_err(|e| {
        DocumentError::Extraction(format!("invalid UTF-8: {e}"))
    })
}

// ---------------------------------------------------------------------------
// PDF extraction via pdf-extract
// ---------------------------------------------------------------------------

fn extract_pdf(data: &[u8]) -> Result<String, DocumentError> {
    if data.len() < 5 || &data[..5] != b"%PDF-" {
        return Err(DocumentError::Extraction(
            "not a valid PDF file".to_string(),
        ));
    }

    debug!(size = data.len(), "Extracting text from PDF");

    let text = pdf_extract::extract_text_from_mem(data).map_err(|e| {
        DocumentError::Extraction(format!("PDF extraction failed: {e}"))
    })?;

    // Clean up: collapse excessive whitespace and blank lines
    let cleaned = collapse_whitespace(&text);

    if cleaned.trim().is_empty() {
        return Err(DocumentError::Extraction(
            "PDF contains no extractable text (may be image-based/scanned)".to_string(),
        ));
    }

    Ok(cleaned)
}

// ---------------------------------------------------------------------------
// DOCX extraction — ZIP archive with XML content
// ---------------------------------------------------------------------------

/// DOCX files are ZIP archives. The main content is in `word/document.xml`.
/// We parse the XML and extract all text runs (<w:t> elements).
fn extract_docx(data: &[u8]) -> Result<String, DocumentError> {
    if data.len() < 4 || &data[..4] != b"PK\x03\x04" {
        return Err(DocumentError::Extraction(
            "not a valid DOCX file".to_string(),
        ));
    }

    debug!(size = data.len(), "Extracting text from DOCX");

    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        DocumentError::Extraction(format!("failed to open DOCX archive: {e}"))
    })?;

    // Read word/document.xml — this contains the main body text
    let xml_content = {
        let mut file = archive.by_name("word/document.xml").map_err(|e| {
            DocumentError::Extraction(format!(
                "DOCX missing word/document.xml: {e}"
            ))
        })?;
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut file, &mut buf).map_err(|e| {
            DocumentError::Extraction(format!("failed to read document.xml: {e}"))
        })?;
        buf
    };

    // Parse XML and extract text from <w:t> elements
    let text = extract_text_from_docx_xml(&xml_content)?;

    if text.trim().is_empty() {
        return Err(DocumentError::Extraction(
            "DOCX contains no extractable text".to_string(),
        ));
    }

    Ok(text)
}

/// Parse DOCX XML and extract text content.
///
/// Structure: <w:body> contains <w:p> (paragraphs), each containing
/// <w:r> (runs) with <w:t> (text) elements. We also handle <w:tab>
/// and <w:br> for basic formatting.
fn extract_text_from_docx_xml(xml: &str) -> Result<String, DocumentError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    let mut result = String::new();
    let mut in_text = false;
    let mut in_paragraph = false;
    let mut paragraph_has_text = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"p" => {
                        // New paragraph — add newline if previous paragraph had text
                        if in_paragraph && paragraph_has_text {
                            result.push('\n');
                        }
                        in_paragraph = true;
                        paragraph_has_text = false;
                    }
                    b"t" => {
                        in_text = true;
                    }
                    b"tab" => {
                        result.push('\t');
                    }
                    b"br" => {
                        result.push('\n');
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_text {
                    let text = e.unescape().map_err(|err| {
                        DocumentError::Extraction(format!("XML decode error: {err}"))
                    })?;
                    result.push_str(&text);
                    paragraph_has_text = true;
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"t" => {
                        in_text = false;
                    }
                    b"p" => {
                        in_paragraph = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(DocumentError::Extraction(format!(
                    "XML parse error: {e}"
                )));
            }
            _ => {}
        }
    }

    // Final newline
    if paragraph_has_text {
        result.push('\n');
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collapse runs of whitespace/blank lines from PDF output.
fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut blank_count = 0;

    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_text() {
        let data = b"Hello, world!";
        let text = extract_text(data, DocumentFormat::PlainText).unwrap();
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn extract_markdown() {
        let data = b"# Title\n\nSome **bold** text.";
        let text = extract_text(data, DocumentFormat::Markdown).unwrap();
        assert!(text.contains("# Title"));
    }

    #[test]
    fn extract_invalid_utf8() {
        let data = &[0xFF, 0xFE, 0x00];
        let result = extract_text(data, DocumentFormat::PlainText);
        assert!(result.is_err());
    }

    #[test]
    fn pdf_invalid_header() {
        let data = b"not a pdf";
        let result = extract_text(data, DocumentFormat::Pdf);
        assert!(result.is_err());
    }

    #[test]
    fn docx_invalid_header() {
        let data = b"not a docx";
        let result = extract_text(data, DocumentFormat::Docx);
        assert!(result.is_err());
    }

    #[test]
    fn docx_xml_extraction() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:body>
                <w:p>
                    <w:r><w:t>Hello </w:t></w:r>
                    <w:r><w:t>World</w:t></w:r>
                </w:p>
                <w:p>
                    <w:r><w:t>Second paragraph</w:t></w:r>
                </w:p>
            </w:body>
        </w:document>"#;

        let text = extract_text_from_docx_xml(xml).unwrap();
        assert!(text.contains("Hello World"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn collapse_whitespace_works() {
        let input = "line1\n\n\n\n\nline2\n\nline3";
        let result = collapse_whitespace(input);
        assert_eq!(result, "line1\n\nline2\n\nline3");
    }
}
