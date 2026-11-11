//! Lossless immutable red/green syntax. All offsets are UTF-8 byte offsets.
use rowan::{GreenNode, GreenNodeBuilder, Language};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum Kind {
    Ident,
    Number,
    String,
    Whitespace,
    Newline,
    Comment,
    Colon,
    Dot,
    Hash,
    At,
    Arrow,
    BiArrow,
    Line,
    BoldArrow,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Semicolon,
    Other,
    Root,
    NodeDecl,
    PortDecl,
    EdgeStmt,
    ZoneDecl,
    AlignDecl,
    DistributeDecl,
    FlowDecl,
    LayoutBlock,
    LayoutRule,
    Annotation,
    AttrList,
    Group,
    ViewDecl,
    StrataDecl,
    StoryDecl,
    Endpoint,
    PortAnchor,
    KeyVal,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiagramLanguage {}
impl Language for DiagramLanguage {
    type Kind = Kind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Kind {
        const KINDS: &[Kind] = &[
            Kind::Ident,
            Kind::Number,
            Kind::String,
            Kind::Whitespace,
            Kind::Newline,
            Kind::Comment,
            Kind::Colon,
            Kind::Dot,
            Kind::Hash,
            Kind::At,
            Kind::Arrow,
            Kind::BiArrow,
            Kind::Line,
            Kind::BoldArrow,
            Kind::LBrace,
            Kind::RBrace,
            Kind::LBracket,
            Kind::RBracket,
            Kind::LParen,
            Kind::RParen,
            Kind::Comma,
            Kind::Plus,
            Kind::Minus,
            Kind::Semicolon,
            Kind::Other,
            Kind::Root,
            Kind::NodeDecl,
            Kind::PortDecl,
            Kind::EdgeStmt,
            Kind::ZoneDecl,
            Kind::AlignDecl,
            Kind::DistributeDecl,
            Kind::FlowDecl,
            Kind::LayoutBlock,
            Kind::LayoutRule,
            Kind::Annotation,
            Kind::AttrList,
            Kind::Group,
            Kind::ViewDecl,
            Kind::StrataDecl,
            Kind::StoryDecl,
            Kind::Endpoint,
            Kind::PortAnchor,
            Kind::KeyVal,
            Kind::Error,
        ];
        KINDS[raw.0 as usize]
    }
    fn kind_to_raw(kind: Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}
pub type SyntaxNode = rowan::SyntaxNode<DiagramLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<DiagramLanguage>;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub fn range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
    pub fn text(self, source: &str) -> &str {
        &source[self.range()]
    }
}
impl From<rowan::TextRange> for Span {
    fn from(value: rowan::TextRange) -> Self {
        Self::new(
            u32::from(value.start()) as usize,
            u32::from(value.end()) as usize,
        )
    }
}
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Kind,
    pub span: Span,
}
impl Token {
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        self.span.text(source)
    }
}
#[derive(Debug, Clone)]
pub struct Parse {
    pub source: Arc<str>,
    pub green: GreenNode,
    pub errors: Vec<(Span, String)>,
}
impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
    /// Reparses one top-level statement when delimiters are unchanged; otherwise safely reparses the file.
    pub fn reparse(&self, span: Span, insert: &str) -> Result<Self, crate::Error> {
        if span.start > span.end
            || span.end > self.source.len()
            || !self.source.is_char_boundary(span.start)
            || !self.source.is_char_boundary(span.end)
        {
            return Err(crate::Error::Edit(
                "patch is not a valid UTF-8 byte range".into(),
            ));
        }
        let mut source = self.source.to_string();
        source.replace_range(span.range(), insert);
        if self.errors.is_empty()
            && !insert.contains(['{', '}', '\n', '\r', ';'])
            && !span
                .text(&self.source)
                .contains(['{', '}', '\n', '\r', ';'])
        {
            let root = self.syntax();
            for (i, child) in root.children_with_tokens().enumerate() {
                if let Some(node) = child.as_node() {
                    let r = Span::from(node.text_range());
                    if span.start >= r.start && span.end < r.end {
                        let end = (r.end as isize + insert.len() as isize
                            - (span.end - span.start) as isize)
                            as usize;
                        let local = parse(&source[r.start..end]);
                        let children: Vec<_> = local.green.children().collect();
                        // A new comment can swallow the statement's semicolon
                        // and following declarations even without changing any
                        // delimiter bytes. Preserve the lexical end boundary.
                        let boundary_preserved = r.end == self.source.len()
                            || local
                                .syntax()
                                .last_token()
                                .zip(node.last_token())
                                .is_some_and(|(a, b)| a.kind() == b.kind() && a.text() == b.text());
                        if local.errors.is_empty()
                            && boundary_preserved
                            && children.len() == 1
                            && children[0].as_node().is_some()
                        {
                            let green = self.green.replace_child(i, children[0].to_owned());
                            return Ok(Self {
                                source: source.into(),
                                green,
                                errors: Vec::new(),
                            });
                        }
                    }
                }
            }
        }
        Ok(parse(&source))
    }
}
pub fn lex(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let start = i;
        let rest = &source[i..];
        let c = rest.chars().next().unwrap();
        let (kind, width) = if rest.starts_with("<->") {
            (Kind::BiArrow, 3)
        } else if rest.starts_with("==>") {
            (Kind::BoldArrow, 3)
        } else if rest.starts_with("->") {
            (Kind::Arrow, 2)
        } else if rest.starts_with("--") {
            (Kind::Line, 2)
        } else if c == '\r' && rest.starts_with("\r\n") {
            (Kind::Newline, 2)
        } else if c == '\n' || c == '\r' {
            (Kind::Newline, 1)
        } else if c.is_whitespace() {
            let n = rest
                .char_indices()
                .take_while(|(_, c)| c.is_whitespace() && *c != '\n' && *c != '\r')
                .last()
                .map(|(n, c)| n + c.len_utf8())
                .unwrap();
            (Kind::Whitespace, n)
        } else if c == '#'
            && (i == 0
                || source[..i]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            (Kind::Comment, rest.find(['\r', '\n']).unwrap_or(rest.len()))
        } else if c == '"' {
            let mut n = c.len_utf8();
            let mut escape = false;
            for d in rest[1..].chars() {
                n += d.len_utf8();
                if d == '"' && !escape {
                    break;
                }
                if d == '\\' {
                    escape = !escape;
                } else {
                    escape = false;
                }
            }
            (Kind::String, n)
        } else if c.is_ascii_digit() {
            let n = rest
                .char_indices()
                .take_while(|(_, d)| d.is_ascii_digit() || *d == '.')
                .last()
                .map(|(n, d)| n + d.len_utf8())
                .unwrap();
            (Kind::Number, n)
        } else if c.is_alphabetic() || c == '_' {
            let n = rest
                .char_indices()
                .take_while(|(_, d)| d.is_alphanumeric() || *d == '_')
                .last()
                .map(|(n, d)| n + d.len_utf8())
                .unwrap();
            (Kind::Ident, n)
        } else {
            (
                match c {
                    ':' => Kind::Colon,
                    '.' => Kind::Dot,
                    '#' => Kind::Hash,
                    '@' => Kind::At,
                    '{' => Kind::LBrace,
                    '}' => Kind::RBrace,
                    '[' => Kind::LBracket,
                    ']' => Kind::RBracket,
                    '(' => Kind::LParen,
                    ')' => Kind::RParen,
                    ',' => Kind::Comma,
                    '+' => Kind::Plus,
                    '-' => Kind::Minus,
                    ';' => Kind::Semicolon,
                    _ => Kind::Other,
                },
                c.len_utf8(),
            )
        };
        i += width;
        out.push(Token {
            kind,
            span: Span::new(start, i),
        });
    }
    out
}
pub fn significant(tokens: &[Token]) -> impl Iterator<Item = &Token> {
    tokens
        .iter()
        .filter(|t| !matches!(t.kind, Kind::Whitespace | Kind::Newline | Kind::Comment))
}
pub fn parse(source: &str) -> Parse {
    let tokens = lex(source);
    let mut p = Parser {
        source,
        tokens: &tokens,
        i: 0,
        b: GreenNodeBuilder::new(),
        errors: tokens
            .iter()
            .filter(|t| {
                t.kind == Kind::String && serde_json::from_str::<String>(t.text(source)).is_err()
            })
            .map(|t| (t.span, "invalid or unterminated quoted string".into()))
            .collect(),
    };
    p.b.start_node(Kind::Root.into());
    p.statements(false);
    p.b.finish_node();
    Parse {
        source: Arc::from(source),
        green: p.b.finish(),
        errors: p.errors,
    }
}
impl From<Kind> for rowan::SyntaxKind {
    fn from(k: Kind) -> Self {
        Self(k as u16)
    }
}
struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    i: usize,
    b: GreenNodeBuilder<'a>,
    errors: Vec<(Span, String)>,
}
impl Parser<'_> {
    fn token(&mut self) {
        let t = &self.tokens[self.i];
        self.b.token(t.kind.into(), t.text(self.source));
        self.i += 1;
    }
    fn statements(&mut self, nested: bool) {
        while self.i < self.tokens.len() {
            let t = &self.tokens[self.i];
            if t.kind == Kind::RBrace {
                if nested {
                    return;
                }
                self.errors
                    .push((t.span, "unexpected closing brace".into()));
                self.token();
                continue;
            }
            if matches!(
                t.kind,
                Kind::Whitespace | Kind::Newline | Kind::Comment | Kind::Semicolon
            ) {
                self.token();
                continue;
            }
            self.statement();
        }
        if nested {
            self.errors.push((
                Span::new(self.source.len(), self.source.len()),
                "unclosed block".into(),
            ));
        }
    }
    fn statement(&mut self) {
        let first = self.tokens[self.i].text(self.source);
        let line_end = self.tokens[self.i..]
            .iter()
            .position(|t| {
                matches!(
                    t.kind,
                    Kind::Newline | Kind::Semicolon | Kind::LBrace | Kind::RBrace
                )
            })
            .map(|n| n + self.i)
            .unwrap_or(self.tokens.len());
        let kind = match first {
            "port" => Kind::PortDecl,
            "zone" => Kind::ZoneDecl,
            "align" => Kind::AlignDecl,
            "distribute" => Kind::DistributeDecl,
            "flow" => Kind::FlowDecl,
            "view" => Kind::ViewDecl,
            "strata" => Kind::StrataDecl,
            "@" if self
                .tokens
                .get(self.i + 1)
                .is_some_and(|t| t.text(self.source) == "story") =>
            {
                Kind::StoryDecl
            }
            "@" if self
                .tokens
                .get(self.i + 1)
                .is_some_and(|t| t.text(self.source) == "layout") =>
            {
                Kind::LayoutBlock
            }
            _ if self.tokens[self.i..line_end]
                .iter()
                .any(|t| is_edge(t.kind)) =>
            {
                Kind::EdgeStmt
            }
            _ => Kind::NodeDecl,
        };
        self.b.start_node(kind.into());
        if kind == Kind::EdgeStmt && first != "(" {
            self.edge_prefix();
        }
        while self.i < self.tokens.len() {
            match self.tokens[self.i].kind {
                Kind::Newline | Kind::Semicolon => {
                    self.token();
                    break;
                }
                Kind::RBrace => break,
                Kind::LBrace => {
                    self.token();
                    self.statements(true);
                    if self.i < self.tokens.len() {
                        self.token();
                    }
                    break;
                }
                Kind::LBracket => self.group(Kind::AttrList, Kind::RBracket),
                Kind::LParen => self.group(Kind::Group, Kind::RParen),
                Kind::RBracket | Kind::RParen => {
                    self.errors
                        .push((self.tokens[self.i].span, "unmatched delimiter".into()));
                    self.token();
                }
                Kind::At => {
                    let ahead: Vec<_> = significant(&self.tokens[self.i..])
                        .take(3)
                        .map(|t| t.kind)
                        .collect();
                    if ahead == [Kind::At, Kind::Ident, Kind::LParen] {
                        self.b.start_node(Kind::Annotation.into());
                        self.token();
                        while self.i < self.tokens.len() && self.tokens[self.i].kind != Kind::LParen
                        {
                            self.token();
                        }
                        self.group(Kind::Group, Kind::RParen);
                        self.b.finish_node();
                    } else {
                        self.token();
                    }
                }
                _ => self.token(),
            }
        }
        self.b.finish_node();
    }
    fn edge_prefix(&mut self) {
        loop {
            while self.i < self.tokens.len() && self.tokens[self.i].kind == Kind::Whitespace {
                self.token();
            }
            if self.i >= self.tokens.len() {
                break;
            }
            let start = self.i;
            let mut end = start;
            let mut depth = 0;
            while end < self.tokens.len() {
                let k = self.tokens[end].kind;
                if depth == 0
                    && (is_edge(k)
                        || matches!(
                            k,
                            Kind::Colon
                                | Kind::LBracket
                                | Kind::Newline
                                | Kind::Comment
                                | Kind::Semicolon
                                | Kind::RBrace
                        )
                        || (k == Kind::At
                            && self
                                .tokens
                                .get(end + 1)
                                .is_some_and(|t| t.kind == Kind::Ident)))
                {
                    break;
                }
                if k == Kind::LParen {
                    depth += 1;
                }
                if k == Kind::RParen {
                    depth -= 1;
                }
                end += 1;
            }
            if end == start {
                break;
            }
            self.b.start_node(Kind::Endpoint.into());
            let mut anchor = false;
            while self.i < end {
                let k = self.tokens[self.i].kind;
                if !anchor
                    && (k == Kind::Hash
                        || k == Kind::Dot
                            && self.tokens.get(self.i + 1).is_some_and(|t| {
                                matches!(t.text(self.source), "N" | "E" | "S" | "W")
                            }))
                {
                    self.b.start_node(Kind::PortAnchor.into());
                    anchor = true;
                }
                if k == Kind::LParen {
                    self.group(Kind::Group, Kind::RParen);
                } else {
                    self.token();
                }
            }
            if anchor {
                self.b.finish_node();
            }
            self.b.finish_node();
            if self.i < self.tokens.len() && is_edge(self.tokens[self.i].kind) {
                self.token();
            } else {
                break;
            }
        }
    }
    fn group(&mut self, kind: Kind, close: Kind) {
        self.b.start_node(kind.into());
        self.token();
        let mut closed = false;
        while self.i < self.tokens.len() {
            match self.tokens[self.i].kind {
                k if k == close => {
                    self.token();
                    closed = true;
                    break;
                }
                Kind::LBracket => self.group(Kind::AttrList, Kind::RBracket),
                Kind::LParen => self.group(Kind::Group, Kind::RParen),
                Kind::RBrace => break,
                Kind::Ident
                    if significant(&self.tokens[self.i..])
                        .nth(1)
                        .is_some_and(|t| t.kind == Kind::Colon) =>
                {
                    self.key_val(close)
                }
                _ => self.token(),
            }
        }
        if !closed {
            self.errors.push((
                Span::new(self.source.len(), self.source.len()),
                format!("expected {close:?}"),
            ));
        }
        self.b.finish_node();
    }
    fn key_val(&mut self, close: Kind) {
        self.b.start_node(Kind::KeyVal.into());
        self.token();
        while self.i < self.tokens.len() {
            let k = self.tokens[self.i].kind;
            if k == close || k == Kind::RBrace {
                break;
            }
            if k == Kind::Comma {
                let mut next = significant(&self.tokens[self.i + 1..]);
                if next.next().is_none_or(|t| t.kind == close)
                    || next.next().is_some_and(|t| t.kind == Kind::Colon)
                {
                    break;
                }
            }
            match k {
                Kind::LBracket => self.group(Kind::AttrList, Kind::RBracket),
                Kind::LParen => self.group(Kind::Group, Kind::RParen),
                _ => self.token(),
            }
        }
        self.b.finish_node();
    }
}
pub fn is_edge(k: Kind) -> bool {
    matches!(
        k,
        Kind::Arrow | Kind::BiArrow | Kind::Line | Kind::BoldArrow
    )
}
/// Typed views over red syntax nodes, suitable for inspection without copying source.
pub mod ast {
    use super::*;
    pub trait AstNode: Sized {
        const KIND: Kind;
        fn syntax(&self) -> &SyntaxNode;
        fn cast(node: SyntaxNode) -> Option<Self>;
    }
    macro_rules! view {
        ($name:ident,$kind:ident) => {
            #[derive(Debug, Clone, PartialEq, Eq)]
            pub struct $name(SyntaxNode);
            impl AstNode for $name {
                const KIND: Kind = Kind::$kind;
                fn syntax(&self) -> &SyntaxNode {
                    &self.0
                }
                fn cast(node: SyntaxNode) -> Option<Self> {
                    (node.kind() == Self::KIND).then_some(Self(node))
                }
            }
        };
    }
    view!(NodeDecl, NodeDecl);
    view!(EdgeStmt, EdgeStmt);
    view!(Annotation, Annotation);
    view!(PortDecl, PortDecl);
    view!(ZoneDecl, ZoneDecl);
    view!(LayoutBlock, LayoutBlock);
    view!(Endpoint, Endpoint);
    view!(PortAnchor, PortAnchor);
    view!(KeyVal, KeyVal);
}
