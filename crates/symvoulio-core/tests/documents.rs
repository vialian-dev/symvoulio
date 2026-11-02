// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use docx_rs::*;
use std::fs;
use symvoulio_core::{extract, DocumentError, DocumentKind};
#[test]
fn utf8_bom_hash_and_supporting_csv() {
    let dir = tempfile::tempdir().unwrap();
    for (name, kind) in [
        ("input.txt", DocumentKind::Txt),
        ("input.md", DocumentKind::Md),
        ("data.csv", DocumentKind::Csv),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, "\u{feff}hello").unwrap();
        let extracted = extract(&path).unwrap();
        assert_eq!(
            extracted.text,
            if kind == DocumentKind::Csv {
                "\u{feff}hello"
            } else {
                "hello"
            }
        );
        assert_eq!(extracted.kind, kind);
        assert_eq!(extracted.bytes, 8);
        assert_eq!(extracted.sha256.len(), 64);
    }
    let path = dir.path().join("known.txt");
    fs::write(&path, "abc").unwrap();
    assert_eq!(
        extract(&path).unwrap().sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
#[test]
fn rejects_invalid_utf8_unsupported_and_missing_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.txt");
    fs::write(&path, [255]).unwrap();
    assert!(matches!(extract(&path), Err(DocumentError::Utf8(_))));
    assert!(matches!(
        extract(&dir.path().join("bad.exe")),
        Err(DocumentError::Unsupported(_))
    ));
    assert!(matches!(
        extract(&dir.path().join("missing.md")),
        Err(DocumentError::Io(_))
    ));
}
#[test]
fn extracted_text_guard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.txt");
    fs::write(
        &path,
        vec![b'a'; symvoulio_core::documents::MAX_TEXT_BYTES + 1],
    )
    .unwrap();
    let error = extract(&path).unwrap_err();
    assert!(matches!(error, DocumentError::TooLarge));
    assert!(error.to_string().contains("10 MB"));
}
#[test]
fn docx_paragraphs_headings_and_tables_in_source_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.docx");
    let cell =
        |s: &str| TableCell::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(s)));
    Docx::new()
        .add_paragraph(
            Paragraph::new()
                .style("Title")
                .add_run(Run::new().add_text("Title")),
        )
        .add_paragraph(
            Paragraph::new()
                .style("Heading1")
                .add_run(Run::new().add_text("First")),
        )
        .add_paragraph(
            Paragraph::new()
                .style("Heading2")
                .add_run(Run::new().add_text("Second")),
        )
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Body text")))
        .add_table(Table::new(vec![TableRow::new(vec![cell("A"), cell("B")])]))
        .build()
        .pack(fs::File::create(&path).unwrap())
        .unwrap();
    assert_eq!(
        extract(&path).unwrap().text,
        "# Title\n\n# First\n\n## Second\n\nBody text\n\n| A | B |"
    );
}
#[test]
fn tiny_text_pdf_and_empty_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pdf");
    let bytes = include_bytes!("fixtures/text.pdf");
    fs::write(&path, bytes).unwrap();
    assert!(extract(&path).unwrap().text.contains("real text layer"));
    // Keep byte offsets intact while replacing all text with spaces.
    let mut bytes = bytes.to_vec();
    let start = bytes.iter().position(|&b| b == b'(').unwrap() + 1;
    let end = start + bytes[start..].iter().position(|&b| b == b')').unwrap();
    for characters in [0, 19, 20] {
        bytes[start..end].fill(b' ');
        bytes[start..start + characters].fill(b'A');
        fs::write(&path, &bytes).unwrap();
        if characters < 20 {
            assert!(matches!(extract(&path), Err(DocumentError::ScannedPdf)));
        } else {
            assert!(extract(&path).is_ok());
        }
    }
}

#[test]
fn damaged_docx_crc_and_expansion_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("damaged.docx");
    Docx::new()
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("hello")))
        .build()
        .pack(fs::File::create(&path).unwrap())
        .unwrap();
    let original = fs::read(&path).unwrap();
    for bomb in [false, true] {
        let mut bytes = original.clone();
        let central = bytes
            .windows(4)
            .enumerate()
            .find_map(|(i, sig)| {
                (sig == b"PK\x01\x02" && bytes.get(i + 46..i + 63) == Some(b"word/document.xml"))
                    .then_some(i)
            })
            .unwrap();
        if bomb {
            bytes[central + 24..central + 28]
                .copy_from_slice(&(100_u32 * 1024 * 1024).to_le_bytes());
        } else {
            bytes[central + 16] ^= 0xff;
        }
        fs::write(&path, bytes).unwrap();
        let outcome = std::panic::catch_unwind(|| extract(&path));
        assert!(outcome.is_ok(), "corrupt DOCX escaped as panic");
        let error = outcome.unwrap().unwrap_err().to_string();
        assert!(
            error.contains(if bomb {
                "expansion"
            } else {
                "damaged or not supported"
            }),
            "{error}"
        );
    }
}
