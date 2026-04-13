use crate::{DocumentError, DocumentFormat};

/// Extracts plain text from document bytes.
///
/// Phase 1 supports TXT and Markdown natively. PDF and DOCX return
/// placeholder errors — real extraction will be added in Phase 2
/// (via `pdf-extract` / `docx-rs` or an external microservice).
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
// PDF extraction — Phase 2
// ---------------------------------------------------------------------------

fn extract_pdf(data: &[u8]) -> Result<String, DocumentError> {
    // Phase 2: use `pdf-extract` or `lopdf` crate for native extraction,
    // or call out to an external service (e.g. Apache Tika).
    if data.len() < 5 || &data[..5] != b"%PDF-" {
        return Err(DocumentError::Extraction(
            "not a valid PDF file".to_string(),
        ));
    }
    Err(DocumentError::UnsupportedFormat(
        "PDF text extraction not yet implemented — upload TXT or Markdown for Phase 1".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// DOCX extraction — Phase 2
// ---------------------------------------------------------------------------

fn extract_docx(data: &[u8]) -> Result<String, DocumentError> {
    // Phase 2: use `docx-rs` or zip + XML parsing.
    // DOCX files are ZIP archives starting with PK signature.
    if data.len() < 4 || &data[..4] != b"PK\x03\x04" {
        return Err(DocumentError::Extraction(
            "not a valid DOCX file".to_string(),
        ));
    }
    Err(DocumentError::UnsupportedFormat(
        "DOCX text extraction not yet implemented — upload TXT or Markdown for Phase 1".to_string(),
    ))
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
    fn pdf_not_yet_supported() {
        let data = b"%PDF-1.4 fake pdf content";
        let result = extract_text(data, DocumentFormat::Pdf);
        assert!(matches!(result, Err(DocumentError::UnsupportedFormat(_))));
    }

    #[test]
    fn docx_not_yet_supported() {
        let data = b"PK\x03\x04 fake docx content";
        let result = extract_text(data, DocumentFormat::Docx);
        assert!(matches!(result, Err(DocumentError::UnsupportedFormat(_))));
    }
}
