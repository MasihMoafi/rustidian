use pulldown_cmark::Alignment;
use rust_notes::{
    editor::Markdown,
    tables::{Action, Table},
};

fn table(source: &str) -> Table {
    let mut parsed = Markdown::parse(source);
    assert_eq!(parsed.tables.len(), 1, "{source:?}");
    parsed.tables.remove(0)
}

#[test]
fn cell_edit_preserves_unicode_markup_escapes_crlf_and_every_other_byte() {
    let mut source = "Before π\r\n\r\n| Item | Value |\r\n| :--- | ---: |\r\n| **Alpha** | `a\\|b` |\r\n| [[Home\\|Alias]] | |\r\n\r\nAfter سلام\r\n".to_owned();
    let original = source.clone();
    let model = table(&source);
    assert_eq!(model.rows.len(), 3);
    assert_eq!(model.cell_text(&source, 1, 1), "`a|b`");
    assert_eq!(model.cell_text(&source, 2, 0), "[[Home|Alias]]");
    model
        .apply(
            &mut source,
            Action::Cell {
                row: 1,
                column: 0,
                text: "سلام | π".into(),
            },
        )
        .unwrap();
    assert_eq!(source, original.replace("**Alpha**", "سلام \\| π"));
    assert!(
        model.apply(&mut source, Action::RemoveRow(1)).is_err(),
        "stale snapshot must refuse"
    );
    let model = table(&source);
    let before = source.clone();
    for action in [
        Action::Cell {
            row: 8,
            column: 0,
            text: "lost".into(),
        },
        Action::Cell {
            row: 1,
            column: 0,
            text: "line\nbreak".into(),
        },
        Action::RemoveRow(0),
        Action::MoveRow { from: 0, to: 1 },
        Action::RemoveColumn(9),
    ] {
        assert!(model.apply(&mut source, action).is_err());
        assert_eq!(source, before);
    }
}

#[test]
fn structural_operations_reparse_with_alignment_empty_cells_and_surroundings() {
    let mut source = "Prefix\n\nA | B\n:-- | --:\nπ | 10\nBeta | 2\nEmpty |\n\nSuffix\n".to_owned();
    let apply = |source: &mut String, action| table(source).apply(source, action).unwrap();
    apply(&mut source, Action::InsertRow(2));
    assert_eq!(table(&source).rows.len(), 5);
    apply(&mut source, Action::RemoveRow(2));
    apply(
        &mut source,
        Action::Cell {
            row: 3,
            column: 1,
            text: "3".into(),
        },
    );
    apply(
        &mut source,
        Action::Sort {
            column: 1,
            descending: false,
        },
    );
    assert_eq!(table(&source).cell(&source, 1, 0), "Beta");
    assert_eq!(table(&source).cell(&source, 3, 0), "π");
    apply(&mut source, Action::MoveRow { from: 3, to: 1 });
    assert_eq!(table(&source).cell(&source, 1, 0), "π");
    apply(&mut source, Action::InsertColumn(1));
    assert_eq!(table(&source).alignments.len(), 3);
    assert_eq!(table(&source).cell(&source, 1, 2), "10");
    apply(&mut source, Action::MoveColumn { from: 2, to: 0 });
    assert_eq!(table(&source).cell(&source, 1, 0), "10");
    apply(
        &mut source,
        Action::Align {
            column: 0,
            alignment: Alignment::Center,
        },
    );
    assert_eq!(table(&source).alignments[0], Alignment::Center);
    apply(&mut source, Action::RemoveColumn(1));
    assert_eq!(table(&source).alignments.len(), 2);
    assert!(source.starts_with("Prefix\n\n"));
    assert!(source.ends_with("\n\nSuffix\n"));
}

#[test]
fn nested_quotes_lists_and_header_only_tables_keep_their_containers() {
    for original in [
        "> | A | B |\r\n> | --- | --- |\r\n> | α | β |\r\n",
        "- | A | B |\n  | --- | --- |\n  | α | β |\n",
        "> > A | B\n> > --- | ---\n> > α | β\n",
        "| A |\n| --- |",
    ] {
        let mut source = original.to_owned();
        let original_model = table(&source);
        original_model
            .apply(&mut source, Action::InsertRow(1))
            .unwrap();
        let model = table(&source);
        assert_eq!(
            model.rows.len(),
            original_model.rows.len() + 1,
            "{source:?}"
        );
        assert_eq!(model.cell(&source, 0, 0), "A");
        assert_eq!(source.ends_with('\n'), original.ends_with('\n'));
        if original.starts_with("> ") {
            assert!(source.lines().all(|line| line.starts_with("> ")));
        }
        if original.starts_with("- ") {
            assert!(source.starts_with("- |"));
            assert!(source.lines().skip(1).all(|line| line.starts_with("  |")));
        }
        assert_eq!(source.contains("\r\n"), original.contains("\r\n"));
    }
}

#[test]
fn parser_excludes_literal_tables_and_edits_never_discard_extra_columns() {
    for source in [
        "```md\n| A |\n| --- |\n| B |\n```\n",
        "---\nvalue: |\n  A | B\n  --- | ---\n---\n",
        "<table><tr><td>A | B</td></tr></table>",
    ] {
        assert!(Markdown::parse(source).tables.is_empty());
    }
    let mut source = "| A | B |\n| --- | --- |\n| one | two | extra |\n| short |\n".to_owned();
    assert_eq!(table(&source).rows[1].cells.len(), 3);
    table(&source)
        .apply(
            &mut source,
            Action::Cell {
                row: 2,
                column: 1,
                text: "filled".into(),
            },
        )
        .unwrap();
    assert!(source.contains("| one | two | extra |"));
    assert_eq!(table(&source).cell(&source, 2, 1), "filled");
    table(&source)
        .apply(&mut source, Action::InsertColumn(1))
        .unwrap();
    assert_eq!(table(&source).cell(&source, 1, 3), "extra");
    let model = table("| A |\n| --- |\n| value |\n");
    let mut one = "| A |\n| --- |\n| value |\n".to_owned();
    assert!(model.apply(&mut one, Action::RemoveColumn(0)).is_err());
}

#[test]
fn focused_drafts_preserve_typed_padding_and_do_not_reuse_another_cells_range() {
    let original =
        "Outside\r\n\r\n| A | B |\r\n| --- | --- |\r\n| old | other |\r\n| short |\r\n\r\nEnd\r\n";
    let mut source = original.to_owned();
    let mut draft = None;
    for text in ["A ", "A π ", "A π | B", " A π | B "] {
        draft = Some(
            table(&source)
                .edit_cell(&mut source, 1, 0, text.into(), draft.as_ref())
                .unwrap(),
        );
        assert_eq!(source, original.replace("old", &text.replace('|', "\\|")));
    }
    let other = table(&source)
        .edit_cell(&mut source, 1, 1, "updated".into(), draft.as_ref())
        .unwrap();
    assert!(other.is_current(&source));
    assert!(source.contains("|  A π \\| B  | updated |"));
    table(&source)
        .edit_cell(&mut source, 2, 1, " new | cell ".into(), None)
        .unwrap();
    assert!(source.contains("| short |  new \\| cell  |\r\n"));
    assert!(source.starts_with("Outside\r\n\r\n"));
    assert!(source.ends_with("\r\n\r\nEnd\r\n"));
    let before = source.clone();
    assert!(
        table(&source)
            .edit_cell(&mut source, 1, 1, "bad\nvalue".into(), Some(&other))
            .is_err()
    );
    assert_eq!(source, before);
    let mut empty = "| A | B |\r\n| --- | --- |\r\n|  |  |\r\n".to_owned();
    table(&empty)
        .edit_cell(&mut empty, 1, 1, "4".into(), None)
        .unwrap();
    assert!(empty.ends_with("|  | 4 |\r\n"), "{empty:?}");
}

#[test]
fn sorting_uses_displayed_markdown_and_stably_orders_numeric_cells() {
    let mut source = "| Name | Value |\n| --- | --- |\n| **Zulu** | `10` |\n| [[Home\\|Alpha]] | **2** |\n| beta | 3 |\n| Beta | 3 |\n".to_owned();
    table(&source)
        .apply(
            &mut source,
            Action::Sort {
                column: 0,
                descending: false,
            },
        )
        .unwrap();
    assert_eq!(table(&source).cell(&source, 1, 0), "[[Home\\|Alpha]]");
    assert_eq!(table(&source).cell(&source, 2, 0), "beta");
    assert_eq!(table(&source).cell(&source, 3, 0), "Beta");
    table(&source)
        .apply(
            &mut source,
            Action::Sort {
                column: 1,
                descending: true,
            },
        )
        .unwrap();
    assert_eq!(table(&source).cell(&source, 1, 1), "`10`");
    assert_eq!(table(&source).cell(&source, 4, 1), "**2**");
}
