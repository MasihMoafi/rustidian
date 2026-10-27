//! Markdown math ranges retain the source delimiters for editing and selection.

use pulldown_cmark::{Event, Options, Parser};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Byte range in the original Markdown, including `$` or `$$` delimiters.
    pub range: Range<usize>,
    /// TeX content, with Markdown container prefixes and table escapes resolved.
    pub formula: String,
    pub display: bool,
}

pub fn spans(source: &str) -> Vec<Span> {
    Parser::new_ext(
        source,
        Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_MATH,
    )
    .into_offset_iter()
    .filter_map(|(event, range)| {
        let (formula, display) = match event {
            Event::InlineMath(formula) => (formula, false),
            Event::DisplayMath(formula) => (formula, true),
            _ => return None,
        };
        // A closing dollar followed by a digit is a price, not a math boundary.
        if !display
            && source
                .as_bytes()
                .get(range.end)
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        Some(Span {
            range,
            formula: formula.into_string(),
            display,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_retain_delimited_byte_ranges_and_display_mode() {
        for (source, expected) in [
            ("left $x^2$ right", vec![(5..10, "x^2", false)]),
            ("$$a+b$$", vec![(0..7, "a+b", true)]),
            ("$x$ + $y$", vec![(0..3, "x", false), (6..9, "y", false)]),
            ("π $x$ سلام", vec![(3..6, "x", false)]),
            (
                "before\n\n$$\n\\frac{a}{b}\n$$\n\nafter",
                vec![(8..25, "\n\\frac{a}{b}\n", true)],
            ),
        ] {
            let actual = spans(source);
            assert_eq!(actual.len(), expected.len(), "{source}");
            for (actual, (range, formula, display)) in actual.iter().zip(expected) {
                assert_eq!(actual.range, range, "{source}");
                assert_eq!(actual.formula, formula, "{source}");
                assert_eq!(actual.display, display, "{source}");
                let delimiters = if display { "$$" } else { "$" };
                assert_eq!(
                    &source[actual.range.clone()],
                    format!("{delimiters}{formula}{delimiters}"),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn code_prices_escaped_and_unmatched_dollars_remain_literal() {
        for source in [
            "`$x$` and `$$y$$`",
            "```tex\n$x$\n$$y$$\n```",
            "    $x$\n    $$y$$\n",
            "$5 and $10",
            "$5 and$10",
            "$x$2",
            r"\$x\$",
            "$x",
            "x$",
            "$$x",
            "$ x$",
            "$x $",
        ] {
            assert!(spans(source).is_empty(), "{source}");
        }
        assert_eq!(
            spans("$5 and$10; formula $x$"),
            [Span {
                range: 19..22,
                formula: "x".into(),
                display: false,
            }]
        );
    }

    #[test]
    fn nested_markdown_keeps_raw_ranges_and_resolves_formula_content() {
        let source = "> $$\n> a+b\n> $$\n\n| Formula |\n| --- |\n| $a\\|b$ |\n";
        let actual = spans(source);
        assert_eq!(actual.len(), 2);
        assert_eq!(&source[actual[0].range.clone()], "$$\n> a+b\n> $$");
        assert_eq!(actual[0].formula, "\na+b\n");
        assert!(actual[0].display);
        assert_eq!(&source[actual[1].range.clone()], r"$a\|b$");
        assert_eq!(actual[1].formula, "a|b");
        assert!(!actual[1].display);
    }

    #[test]
    fn math_does_not_create_vault_links_or_attachment_reads() {
        let source = "$[[formula]] ![x](inside.png)$\n\n[[Real]] ![real](outside.png)";
        assert_eq!(crate::vault::wiki_links(source), ["Real"]);
        let images = crate::assets::embedded_images(source);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].target, "outside.png");
    }
}
