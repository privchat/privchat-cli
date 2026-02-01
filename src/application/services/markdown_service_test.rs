#[cfg(test)]
mod tests {
    use crate::application::services::markdown_service::{
        MarkdownService, MdBlock, MdInline, parse_markdown,
    };
    use ratatui::style::Color;

    #[test]

    fn test_parse_simple_bold() {
        let content = "Hello **world**";
        let blocks = parse_markdown(content);

        match &blocks[0] {
            MdBlock::Paragraph(inlines) => {
                assert_eq!(inlines.len(), 2);
                if let MdInline::Text(t) = &inlines[0] {
                    assert_eq!(t, "Hello ");
                } else {
                    panic!("Expected text");
                }
                if let MdInline::Bold(children) = &inlines[1] {
                    if let MdInline::Text(t) = &children[0] {
                        assert_eq!(t, "world");
                    } else {
                        panic!("Expected text inside bold");
                    }
                } else {
                    panic!("Expected bold");
                }
            }
            _ => panic!("Expected paragraph"),
        }
    }

    #[test]
    fn test_parse_headers() {
        let content = "### Header 3\nText";
        let blocks = parse_markdown(content);
        assert_eq!(blocks.len(), 2);

        if let MdBlock::Header(level, inlines) = &blocks[0] {
            assert_eq!(*level, 3);
            if let MdInline::Text(t) = &inlines[0] {
                assert_eq!(t, "Header 3");
            }
        } else {
            panic!("Expected header");
        }
    }

    #[test]
    fn test_parse_spoiler() {
        let content = "Hidden ||spoiler|| content";
        let blocks = parse_markdown(content);

        if let MdBlock::Paragraph(inlines) = &blocks[0] {
            assert_eq!(inlines.len(), 3);
            match &inlines[1] {
                MdInline::Spoiler(children) => {
                    if let MdInline::Text(t) = &children[0] {
                        assert_eq!(t, "spoiler");
                    }
                }
                _ => panic!("Expected spoiler"),
            }
        }
    }

    #[test]
    fn test_parse_nested_styles() {
        let content = "***Bold Italic***";
        let blocks = parse_markdown(content);

        if let MdBlock::Paragraph(inlines) = &blocks[0] {
            match &inlines[0] {
                MdInline::Italic(children) => match &children[0] {
                    MdInline::Bold(inner) => {
                        if let MdInline::Text(t) = &inner[0] {
                            assert_eq!(t, "Bold Italic");
                        }
                    }
                    _ => panic!("Expected Bold inside Italic"),
                },
                _ => panic!("Expected Italic"),
            }
        }
    }

    #[test]
    fn test_render_with_spoilers_hidden() {
        let content = "||Secret||";
        let service = MarkdownService::new();
        let text = service.render_with_spoilers(content, None, false);

        let line = &text.lines[0];
        let span = &line.spans[0];

        assert_eq!(span.style.bg, Some(Color::DarkGray));
        assert_eq!(span.style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn test_render_with_spoilers_shown() {
        let content = "||Secret||";
        let service = MarkdownService::new();
        let text = service.render_with_spoilers(content, None, true);

        let line = &text.lines[0];
        let span = &line.spans[0];

        assert_eq!(span.style.bg, Some(Color::Rgb(50, 50, 50)));
        assert_ne!(span.style.fg, Some(Color::Rgb(50, 50, 50)));
    }
}
