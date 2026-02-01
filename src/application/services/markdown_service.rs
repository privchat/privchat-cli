use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use std::iter::Peekable;
use std::str::CharIndices;
use std::sync::Arc;

use super::syntax_highlighting::{SyntaxHighlighter, SyntectHighlighter};

pub trait MentionResolver: Send + Sync {
    fn resolve(&self, user_id: &str) -> Option<String>;
}

#[derive(Debug, Clone)]
pub enum MdBlock {
    Header(u8, Vec<MdInline>),
    List {
        indent: u8,
        content: Vec<MdInline>,
        bullet: char,
    },
    BlockQuote(Vec<MdBlock>),
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    Subtext(Vec<MdInline>),
    Paragraph(Vec<MdInline>),
    Empty,
}

#[derive(Debug, Clone)]
pub enum MdInline {
    Text(String),
    Bold(Vec<MdInline>),
    Italic(Vec<MdInline>),
    Underline(Vec<MdInline>),
    Strike(Vec<MdInline>),
    Spoiler(Vec<MdInline>),
    Code(String),
    Mention(String),
}

pub struct MarkdownService {
    highlighter: Arc<dyn SyntaxHighlighter>,
}

impl MarkdownService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            highlighter: Arc::new(SyntectHighlighter::new()),
        }
    }

    #[must_use]
    pub fn with_highlighter(highlighter: Arc<dyn SyntaxHighlighter>) -> Self {
        Self { highlighter }
    }

    #[must_use]
    pub fn parse(&self, content: &str) -> Vec<MdBlock> {
        Parser::parse(content)
    }

    #[must_use]
    pub fn render(&self, content: &str, resolver: Option<&dyn MentionResolver>) -> Text<'static> {
        self.render_with_spoilers(content, resolver, false)
    }

    #[must_use]
    pub fn render_with_spoilers(
        &self,
        content: &str,
        resolver: Option<&dyn MentionResolver>,
        show_spoilers: bool,
    ) -> Text<'static> {
        let blocks = self.parse(content);
        self.render_ast(&blocks, resolver, show_spoilers)
    }

    #[must_use]
    pub fn render_ast(
        &self,
        blocks: &[MdBlock],
        resolver: Option<&dyn MentionResolver>,
        show_spoilers: bool,
    ) -> Text<'static> {
        let mut renderer = Renderer::new(resolver, &self.highlighter, show_spoilers);
        renderer.render(blocks.to_vec())
    }
}

#[must_use]
pub fn parse_markdown(content: &str) -> Vec<MdBlock> {
    Parser::parse(content)
}

impl Default for MarkdownService {
    fn default() -> Self {
        Self::new()
    }
}

struct Parser<'a> {
    #[allow(dead_code)]
    input: &'a str,
}

impl<'a> Parser<'a> {
    fn parse(input: &'a str) -> Vec<MdBlock> {
        let mut blocks = Vec::new();
        let mut lines = input.lines().peekable();

        while let Some(line) = lines.next() {
            let line_trim_end = line.trim_end();

            if line_trim_end.is_empty() {
                blocks.push(MdBlock::Empty);
                continue;
            }

            if line_trim_end.starts_with("```") {
                let lang = line_trim_end.trim_start_matches('`').trim().to_string();
                let lang = if lang.is_empty() { None } else { Some(lang) };
                let mut code = String::new();

                while let Some(code_line) = lines.peek() {
                    if code_line.trim().starts_with("```") {
                        lines.next();
                        break;
                    }
                    code.push_str(lines.next().unwrap());
                    code.push('\n');
                }

                if code.ends_with('\n') {
                    code.pop();
                }

                blocks.push(MdBlock::CodeBlock { lang, code });
                continue;
            }

            if let Some(content) = line.strip_prefix("-# ") {
                blocks.push(MdBlock::Subtext(parse_inline(content)));
                continue;
            } else if line == "-#" {
                blocks.push(MdBlock::Subtext(Vec::new()));
                continue;
            }

            if let Some(content) = line.strip_prefix("### ") {
                blocks.push(MdBlock::Header(3, parse_inline(content)));
                continue;
            }
            if let Some(content) = line.strip_prefix("## ") {
                blocks.push(MdBlock::Header(2, parse_inline(content)));
                continue;
            }
            if let Some(content) = line.strip_prefix("# ") {
                blocks.push(MdBlock::Header(1, parse_inline(content)));
                continue;
            }

            if let Some(content) = line.strip_prefix(">>> ") {
                let mut quote_content = String::from(content);
                quote_content.push('\n');

                for l in lines.by_ref() {
                    quote_content.push_str(l);
                    quote_content.push('\n');
                }

                let inner_blocks = Parser::parse(&quote_content);
                blocks.push(MdBlock::BlockQuote(inner_blocks));
                continue;
            }

            if let Some(content) = line.strip_prefix("> ") {
                let mut inner_blocks = vec![MdBlock::Paragraph(parse_inline(content))];

                while let Some(next_line) = lines.peek() {
                    if next_line.starts_with("> ") && !next_line.starts_with(">>> ") {
                        let next_content = &lines.next().unwrap()[2..];
                        inner_blocks.push(MdBlock::Paragraph(parse_inline(next_content)));
                    } else {
                        break;
                    }
                }
                blocks.push(MdBlock::BlockQuote(inner_blocks));
                continue;
            }

            let trimmed = line.trim_start();
            let indent_len = line.len() - trimmed.len();

            if let Some(content) = trimmed.strip_prefix("- ") {
                blocks.push(MdBlock::List {
                    indent: u8::try_from(indent_len / 2).unwrap_or(0),
                    content: parse_inline(content),
                    bullet: '-',
                });
                continue;
            }
            if let Some(content) = trimmed.strip_prefix("* ") {
                blocks.push(MdBlock::List {
                    indent: u8::try_from(indent_len / 2).unwrap_or(0),
                    content: parse_inline(content),
                    bullet: '*',
                });
                continue;
            }

            blocks.push(MdBlock::Paragraph(parse_inline(line)));
        }

        blocks
    }
}

fn parse_inline(input: &str) -> Vec<MdInline> {
    let mut inlines = Vec::new();
    let mut chars = input.char_indices().peekable();
    let mut start = 0;

    while let Some((idx, ch)) = chars.next() {
        handle_special_chars(input, idx, ch, &mut start, &mut inlines, &mut chars);
    }

    if start < input.len() {
        inlines.push(MdInline::Text(input[start..].to_string()));
    }

    inlines
}

fn handle_special_chars(
    input: &str,
    idx: usize,
    ch: char,
    start: &mut usize,
    inlines: &mut Vec<MdInline>,
    chars: &mut Peekable<CharIndices>,
) -> bool {
    match ch {
        '*' => {
            let remaining = &input[idx..];
            if remaining.starts_with("***") {
                handle_container(input, idx, start, inlines, chars, "***", |c| {
                    MdInline::Italic(vec![MdInline::Bold(c)])
                });
            } else if remaining.starts_with("**") {
                handle_container(input, idx, start, inlines, chars, "**", MdInline::Bold);
            } else {
                handle_container(input, idx, start, inlines, chars, "*", MdInline::Italic);
            }
            true
        }
        '_' => {
            let remaining = &input[idx..];
            if remaining.starts_with("__") {
                handle_container(input, idx, start, inlines, chars, "__", MdInline::Underline);
            } else {
                handle_container(input, idx, start, inlines, chars, "_", MdInline::Italic);
            }
            true
        }
        '~' => {
            let remaining = &input[idx..];
            if remaining.starts_with("~~") {
                handle_container(input, idx, start, inlines, chars, "~~", MdInline::Strike);
            }
            true
        }
        '|' => {
            let remaining = &input[idx..];
            if remaining.starts_with("||") {
                handle_container(input, idx, start, inlines, chars, "||", MdInline::Spoiler);
            }
            true
        }
        '`' => {
            handle_inline_code(input, idx, start, inlines, chars);
            true
        }
        '<' => {
            handle_mention(input, idx, start, inlines, chars);
            true
        }
        '\\' => {
            handle_escape(input, idx, start, inlines, chars);
            true
        }
        _ => false,
    }
}

fn handle_inline_code(
    input: &str,
    idx: usize,
    start: &mut usize,
    inlines: &mut Vec<MdInline>,
    chars: &mut Peekable<CharIndices>,
) {
    if idx > *start {
        inlines.push(MdInline::Text(input[*start..idx].to_string()));
    }

    let scan = chars.clone();
    let mut found_end = None;
    for (next_idx, next_ch) in scan {
        if next_ch == '`' {
            found_end = Some(next_idx);
            break;
        }
    }

    if let Some(end_idx) = found_end {
        let code_content = &input[idx + 1..end_idx];
        inlines.push(MdInline::Code(code_content.to_string()));

        while let Some((curr, _)) = chars.peek() {
            if *curr <= end_idx {
                chars.next();
            } else {
                break;
            }
        }
        *start = end_idx + 1;
    }
}

fn handle_mention(
    input: &str,
    idx: usize,
    start: &mut usize,
    inlines: &mut Vec<MdInline>,
    chars: &mut Peekable<CharIndices>,
) {
    let remaining = &input[idx..];
    if remaining.starts_with("<@")
        && let Some(end) = remaining.find('>')
    {
        if idx > *start {
            inlines.push(MdInline::Text(input[*start..idx].to_string()));
        }
        let content = &remaining[..=end];
        let id_content = &content[2..end];
        let id = id_content.trim_start_matches('!');

        if id.chars().all(char::is_numeric) && !id.is_empty() {
            inlines.push(MdInline::Mention(id.to_string()));

            let end_pos = idx + end;
            while let Some((curr, _)) = chars.peek() {
                if *curr <= end_pos {
                    chars.next();
                } else {
                    break;
                }
            }
            *start = end_pos + 1;
        }
    }
}

fn handle_escape(
    input: &str,
    idx: usize,
    start: &mut usize,
    inlines: &mut Vec<MdInline>,
    chars: &mut Peekable<CharIndices>,
) {
    if idx > *start {
        inlines.push(MdInline::Text(input[*start..idx].to_string()));
    }
    if let Some((_, next_char)) = chars.next() {
        inlines.push(MdInline::Text(next_char.to_string()));
        *start = idx + 1 + next_char.len_utf8();
    } else {
        inlines.push(MdInline::Text("\\".to_string()));
        *start = idx + 1;
    }
}

fn handle_container<F>(
    input: &str,
    idx: usize,
    start: &mut usize,
    inlines: &mut Vec<MdInline>,
    chars: &mut Peekable<CharIndices>,
    delimiter: &str,
    constructor: F,
) where
    F: Fn(Vec<MdInline>) -> MdInline,
{
    let delim_len = delimiter.len();
    let remaining_after = &input[idx + delim_len..];

    if let Some(end_offset) = remaining_after.find(delimiter) {
        if idx > *start {
            inlines.push(MdInline::Text(input[*start..idx].to_string()));
        }

        let inner_start = idx + delim_len;
        let inner_end = inner_start + end_offset;
        let inner_text = &input[inner_start..inner_end];

        let inner_nodes = parse_inline(inner_text);
        inlines.push(constructor(inner_nodes));

        let end_idx = inner_end + delim_len;

        while let Some((curr, _)) = chars.peek() {
            if *curr < end_idx {
                chars.next();
            } else {
                break;
            }
        }
        *start = end_idx;
    }
}

struct Renderer<'a> {
    resolver: Option<&'a dyn MentionResolver>,
    highlighter: &'a Arc<dyn SyntaxHighlighter>,
    show_spoilers: bool,
}

impl<'a> Renderer<'a> {
    fn new(
        resolver: Option<&'a dyn MentionResolver>,
        highlighter: &'a Arc<dyn SyntaxHighlighter>,
        show_spoilers: bool,
    ) -> Self {
        Self {
            resolver,
            highlighter,
            show_spoilers,
        }
    }

    fn render(&mut self, blocks: Vec<MdBlock>) -> Text<'static> {
        let mut lines = Vec::new();
        for block in blocks {
            self.render_block(block, &mut lines, Style::default());
        }
        Text::from(lines)
    }

    fn render_block(&self, block: MdBlock, lines: &mut Vec<Line<'static>>, parent_style: Style) {
        match block {
            MdBlock::Empty => lines.push(Line::raw("")),
            MdBlock::Paragraph(inlines) => {
                let spans = self.render_inlines(inlines, parent_style);
                lines.push(Line::from(spans));
            }
            MdBlock::Header(level, inlines) => {
                let style = parent_style.add_modifier(Modifier::BOLD);
                let style = match level {
                    1 => style.fg(Color::Magenta),
                    2 => style.fg(Color::Cyan),
                    _ => style,
                };

                let mut spans = Vec::new();
                let prefix = "#".repeat(level as usize);
                spans.push(Span::styled(format!("{prefix} "), style));
                spans.extend(self.render_inlines(inlines, style));
                lines.push(Line::from(spans));
                lines.push(Line::raw(""));
            }
            MdBlock::Subtext(inlines) => {
                let style = parent_style.fg(Color::DarkGray).add_modifier(Modifier::DIM);
                let mut spans = Vec::new();
                spans.push(Span::styled("-# ", style));
                spans.extend(self.render_inlines(inlines, style));
                lines.push(Line::from(spans));
            }
            MdBlock::List {
                indent,
                content,
                bullet,
            } => {
                let mut spans = Vec::new();
                let indent_str = "  ".repeat(indent as usize);
                spans.push(Span::raw(indent_str));
                spans.push(Span::styled(
                    format!("{bullet} "),
                    parent_style.fg(Color::Cyan),
                ));
                spans.extend(self.render_inlines(content, parent_style));
                lines.push(Line::from(spans));
            }
            MdBlock::CodeBlock { lang, code } => {
                let highlighted = self.highlighter.highlight(&code, lang.as_deref());

                let mut current_line_spans = Vec::new();

                for span in highlighted {
                    let content = span.content;
                    let style = span.style;

                    let parts: Vec<&str> = content.split_inclusive('\n').collect();
                    for part in parts {
                        if let Some(text) = part.strip_suffix('\n') {
                            if !text.is_empty() {
                                current_line_spans.push(Span::styled(text.to_string(), style));
                            }
                            lines.push(Line::from(std::mem::take(&mut current_line_spans)));
                        } else if !part.is_empty() {
                            current_line_spans.push(Span::styled(part.to_string(), style));
                        }
                    }
                }

                if !current_line_spans.is_empty() {
                    lines.push(Line::from(current_line_spans));
                }
            }
            MdBlock::BlockQuote(inner_blocks) => {
                let mut inner_lines = Vec::new();
                for inner in inner_blocks {
                    self.render_block(
                        inner,
                        &mut inner_lines,
                        parent_style.add_modifier(Modifier::ITALIC),
                    );
                }

                while let Some(last) = inner_lines.last() {
                    if last.spans.iter().all(|s| s.content.trim().is_empty()) {
                        inner_lines.pop();
                    } else {
                        break;
                    }
                }

                for line in inner_lines {
                    let mut spans = vec![Span::styled("┃ ", Style::default().fg(Color::DarkGray))];
                    spans.extend(line.spans);
                    lines.push(Line::from(spans));
                }
            }
        }
    }

    fn render_inlines(&self, inlines: Vec<MdInline>, style: Style) -> Vec<Span<'static>> {
        let mut spans = Vec::new();

        for inline in inlines {
            match inline {
                MdInline::Text(t) => spans.push(Span::styled(t, style)),
                MdInline::Bold(children) => {
                    spans.extend(self.render_inlines(children, style.add_modifier(Modifier::BOLD)));
                }
                MdInline::Italic(children) => {
                    spans.extend(
                        self.render_inlines(children, style.add_modifier(Modifier::ITALIC)),
                    );
                }
                MdInline::Underline(children) => {
                    spans.extend(
                        self.render_inlines(children, style.add_modifier(Modifier::UNDERLINED)),
                    );
                }
                MdInline::Strike(children) => {
                    spans.extend(
                        self.render_inlines(children, style.add_modifier(Modifier::CROSSED_OUT)),
                    );
                }
                MdInline::Spoiler(children) => {
                    if self.show_spoilers {
                        let revealed_style = style.bg(Color::Rgb(50, 50, 50));
                        spans.extend(self.render_inlines(children, revealed_style));
                    } else {
                        let hidden_style = Style::default().bg(Color::DarkGray).fg(Color::DarkGray);
                        spans.extend(self.render_inlines(children, hidden_style));
                    }
                }
                MdInline::Code(code) => {
                    spans.push(Span::styled(code, style.fg(Color::Red)));
                }
                MdInline::Mention(id) => {
                    let name = self
                        .resolver
                        .and_then(|r| r.resolve(&id))
                        .map_or_else(|| format!("<@{id}>"), |n| format!("@{n}"));
                    spans.push(Span::styled(
                        name,
                        style.fg(Color::Blue).add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
        spans
    }
}
