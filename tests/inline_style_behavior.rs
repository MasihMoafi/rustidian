use rustidian::{
    highlight::{Color, NativeColor},
    inline_style::{Element, Style, Tag, color, parse},
};

#[test]
fn requested_compound_mark_keeps_rgba_and_ignores_inherited_color_and_padding() {
    assert_eq!(
        parse(
            r#"<mark style="background-color:rgba(46,204,113,0.4);color:inherit;padding:0.1em 0.2em">"#,
        ),
        Some(Tag::Open(
            Element::Mark,
            Style {
                background: Some([46, 204, 113, 102]),
                ..Style::default()
            },
        )),
    );
}

#[test]
fn foreground_and_numeric_bold_work_together_on_span() {
    assert_eq!(
        parse(r##"<span style="color:#2b7a4b;font-weight:700">"##),
        Some(Tag::Open(
            Element::Span,
            Style {
                color: Some([43, 122, 75, 255]),
                bold: Some(true),
                ..Style::default()
            },
        )),
    );
}

#[test]
fn default_marks_and_existing_portable_color_marks_remain_supported() {
    assert_eq!(
        parse("<mark>"),
        Some(Tag::Open(
            Element::Mark,
            Style {
                background: Some(Color::Yellow.rgba()),
                ..Style::default()
            },
        )),
    );
    for highlight in Color::ALL {
        let (opening, _) = highlight.markers();
        if opening == "==" {
            continue;
        }
        assert_eq!(
            parse(opening),
            Some(Tag::Open(
                Element::Mark,
                Style {
                    background: Some(highlight.rgba()),
                    ..Style::default()
                },
            )),
        );
    }
}

#[test]
fn native_color_tags_are_left_to_the_existing_theme_aware_handler() {
    for native in NativeColor::ALL {
        assert_eq!(parse(native.tag()), None);
    }
    for tag in [
        r##"<mark style="color:#fff" data-highlight="green">"##,
        r##"<mark DATA-HIGHLIGHT='green' style='background:#fff'>"##,
        r##"<span data-highlight='green' style='color:#fff'>"##,
    ] {
        assert_eq!(parse(tag), None, "{tag}");
    }
}

#[test]
fn hex_rgb_and_rgba_colors_have_expected_bytes_and_alpha() {
    for (input, expected) in [
        ("#abc", [170, 187, 204, 255]),
        ("#abcd", [170, 187, 204, 221]),
        (" #2B7a4B ", [43, 122, 75, 255]),
        ("#2b7a4b80", [43, 122, 75, 128]),
        ("rgb(46, 204, 113)", [46, 204, 113, 255]),
        ("rgba(46,204,113,0.4)", [46, 204, 113, 102]),
        ("RGBA(46,204,113,40%)", [46, 204, 113, 102]),
        ("rgb(100%,0%,50%)", [255, 0, 128, 255]),
        ("rgb(46 204 113 / 40%)", [46, 204, 113, 102]),
        ("rgba(46 204 113 / .4)", [46, 204, 113, 102]),
        ("transparent", [0, 0, 0, 0]),
    ] {
        assert_eq!(color(input), Some(expected), "{input}");
    }
}

#[test]
fn malformed_and_unsupported_colors_are_rejected_without_panicking() {
    for input in [
        "",
        "#",
        "#12",
        "#12345",
        "#1234567",
        "#ggg",
        "#۱۲۳",
        "red",
        "inherit",
        "rgb(1,2)",
        "rgb(1,2,3,4)",
        "rgba(1,2,3)",
        "rgba(1,2,3,4)",
        "rgb(-1,0,0)",
        "rgb(256,0,0)",
        "rgb(101%,0%,0%)",
        "rgba(1,2,3,-0.1)",
        "rgba(1,2,3,NaN)",
        "rgba(1,2,3,inf)",
        "rgb(1 2 3 / .5 / .2)",
        "rgb(1,2,3 / .5)",
        "rgb(1,2,3))",
        "rgb(1,2,3) trailing",
        "url(image.png)",
    ] {
        assert_eq!(color(input), None, "{input}");
    }
}

#[test]
fn emphasis_decoration_and_case_insensitive_attributes_are_parsed() {
    assert_eq!(
        parse(
            "<SPAN STYLE = ' COLOR: #ABC; FONT-WEIGHT:bold; font-style:italic; text-decoration:underline line-through; border-radius:4px; font-size:1.2em;'>",
        ),
        Some(Tag::Open(
            Element::Span,
            Style {
                color: Some([170, 187, 204, 255]),
                bold: Some(true),
                italic: Some(true),
                strike: Some(true),
                underline: Some(true),
                ..Style::default()
            },
        )),
    );
}

#[test]
fn later_declarations_and_explicit_normal_values_replace_earlier_styles() {
    assert_eq!(
        parse(
            r##"<span style="color:#123;color:#456 !important;font-weight:bold;font-weight:400;font-style:italic;font-style:normal;text-decoration:underline line-through;text-decoration:none">"##,
        ),
        Some(Tag::Open(
            Element::Span,
            Style {
                color: Some([68, 85, 102, 255]),
                bold: Some(false),
                italic: Some(false),
                strike: Some(false),
                underline: Some(false),
                ..Style::default()
            },
        )),
    );
}

#[test]
fn overlay_retains_parent_values_and_applies_explicit_false() {
    let mut style = Style {
        color: Some([43, 122, 75, 255]),
        background: Some([46, 204, 113, 102]),
        bold: Some(true),
        italic: Some(true),
        strike: Some(true),
        underline: Some(true),
    };
    style.overlay(Style {
        color: Some([255, 255, 255, 255]),
        bold: Some(false),
        strike: Some(false),
        underline: Some(false),
        ..Style::default()
    });
    assert_eq!(
        style,
        Style {
            color: Some([255, 255, 255, 255]),
            background: Some([46, 204, 113, 102]),
            bold: Some(false),
            italic: Some(true),
            strike: Some(false),
            underline: Some(false),
        },
    );
}

#[test]
fn closing_tags_are_typed_and_plain_spans_preserve_inherited_styles() {
    assert_eq!(parse("</mark>"), Some(Tag::Close(Element::Mark)));
    assert_eq!(parse("</SPAN >"), Some(Tag::Close(Element::Span)));
    assert_eq!(
        parse("<span>"),
        Some(Tag::Open(Element::Span, Style::default())),
    );
    assert_eq!(
        parse("<span style='color:inherit;font-weight:inherit'>"),
        Some(Tag::Open(Element::Span, Style::default())),
    );
}

#[test]
fn unsafe_tags_attributes_and_css_are_rejected() {
    for tag in [
        "<script>",
        "<a href='https://example.com'>",
        "<mark onclick='alert(1)'>",
        "<span onmouseover='alert(1)'>",
        "<mark style='background:url(image.png)'>",
        "<span style='background-image:URL (image.png);color:#fff'>",
        "<span style='width:expression(alert(1));color:#fff'>",
        "<span style='behavior:url(script.htc)'>",
        r"<span style='background:u\72l(image.png)'>",
        "<span style='color:#fff' onclick='alert(1)'>",
    ] {
        assert_eq!(parse(tag), None, "{tag}");
    }
}

#[test]
fn incomplete_or_ambiguous_markup_is_not_hidden_as_a_valid_style() {
    for tag in [
        "<span",
        "span>",
        "<mark/>",
        "<mark />",
        "</mark extra>",
        "<mark style=background:#fff>",
        "<mark style='background:#fff>",
        "<mark style='color:#fff'junk>",
        "<mark style='color:#fff' style='color:#000'>",
        "<span style='color'>",
        "<span style='color:'>",
        "<span style='font-weight:huge'>",
        "<span style='font-style:wobbly'>",
        "<span style='text-decoration:unknown'>",
        "<span style='color:#xyz'>",
        "<span>remaining text",
        "<span></span>",
    ] {
        assert_eq!(parse(tag), None, "{tag}");
    }
}
