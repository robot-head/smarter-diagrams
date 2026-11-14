//! Fenced code block parsing, tokenization, and node metric measurement.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Type,
    String,
    Number,
    Comment,
    Func,
}

impl TokenKind {
    pub fn css_class(self) -> Option<&'static str> {
        match self {
            Self::Plain => None,
            Self::Keyword => Some("syn-kw"),
            Self::Type => Some("syn-type"),
            Self::String => Some("syn-str"),
            Self::Number => Some("syn-num"),
            Self::Comment => Some("syn-cmt"),
            Self::Func => Some("syn-fn"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeToken {
    pub kind: TokenKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub lang: String,
    pub raw_lines: Vec<String>,
    pub lines: Vec<Vec<CodeToken>>,
    pub max_cols: usize,
    pub header_h: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct CodeLayout {
    pub title: String,
    pub title_w: f64,
    pub title_h: f64,
    pub blocks: Vec<CodeBlock>,
    pub outer_x: f64,
    pub outer_y: f64,
    pub gap_title: f64,
    pub gap_blocks: f64,
    pub total_w: f64,
    pub total_h: f64,
}

const CHAR_W: f64 = 8.0;
const LINE_H: f64 = 17.0;
const BOX_PAD_X: f64 = 14.0;
const BOX_PAD_Y: f64 = 9.0;

pub fn has_code_blocks(label: &str) -> bool {
    let mut opens = false;
    for line in label.replace("\r\n", "\n").lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            if opens {
                return true;
            }
            opens = true;
        }
    }
    false
}

pub fn extract_title(label: &str) -> String {
    let normalized = label.replace("\r\n", "\n");
    let mut title_lines = Vec::new();
    let mut in_fence = false;
    for line in normalized.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                title_lines.push(trimmed);
            }
        }
    }
    if title_lines.is_empty() {
        String::new()
    } else {
        crate::labels::wrap_text(&title_lines.join(" "), 260.)
    }
}

pub fn measure(label: &str, shape: &str) -> Option<CodeLayout> {
    if !has_code_blocks(label) {
        return None;
    }
    let normalized = label.replace("\r\n", "\n");
    let mut title_lines = Vec::new();
    let mut blocks = Vec::new();
    let mut cur_lang: Option<String> = None;
    let mut cur_lines: Vec<String> = Vec::new();

    for line in normalized.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("```") {
            if let Some(lang) = cur_lang.take() {
                blocks.push(build_block(lang, std::mem::take(&mut cur_lines)));
            } else {
                cur_lang = Some(rest.trim().to_string());
                cur_lines.clear();
            }
        } else if cur_lang.is_some() {
            cur_lines.push(line.to_string());
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                title_lines.push(trimmed);
            }
        }
    }
    if blocks.is_empty() {
        return None;
    }

    let title = if title_lines.is_empty() {
        String::new()
    } else {
        crate::labels::wrap_text(&title_lines.join(" "), 260.)
    };
    let (title_w, title_h) = if title.is_empty() {
        (0., 0.)
    } else {
        (
            crate::labels::text_width(&title) * 16. / 15.,
            crate::labels::text_height(&title),
        )
    };
    let outer_x = 14.;
    let outer_y = 12.;
    let gap_title = if title.is_empty() { 0. } else { 8. };
    let gap_blocks = 8.;
    let max_box_w = blocks.iter().map(|b| b.w).fold(0., f64::max);
    let gutter = crate::shapes::title_gutter(shape);
    let db_extra = if shape == "database" {
        crate::shapes::CYLINDER_RY * 2. + 6.
    } else {
        0.
    };
    let content_w = max_box_w.max(title_w);
    let total_w = (content_w + outer_x * 2. + gutter).max(112.);
    let blocks_h: f64 = blocks.iter().map(|b| b.h).sum::<f64>()
        + blocks.len().saturating_sub(1) as f64 * gap_blocks;
    let total_h = outer_y * 2. + title_h + gap_title + blocks_h + db_extra;

    Some(CodeLayout {
        title,
        title_w,
        title_h,
        blocks,
        outer_x,
        outer_y,
        gap_title,
        gap_blocks,
        total_w,
        total_h,
    })
}

fn build_block(lang: String, mut raw_lines: Vec<String>) -> CodeBlock {
    while raw_lines.last().is_some_and(|l| l.trim().is_empty()) {
        raw_lines.pop();
    }
    if raw_lines.is_empty() {
        raw_lines.push(String::new());
    }
    let max_cols = raw_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);
    let mut in_block_comment = false;
    let lines: Vec<Vec<CodeToken>> = raw_lines
        .iter()
        .map(|line| tokenize_line(&lang, line, &mut in_block_comment))
        .collect();
    let header_h = if lang.is_empty() { 0. } else { 18. };
    let lang_w = if lang.is_empty() {
        0.
    } else {
        lang.chars().count() as f64 * 8. + 24.
    };
    let w = (max_cols as f64 * CHAR_W + BOX_PAD_X * 2.)
        .max(lang_w)
        .max(112.);
    let h = BOX_PAD_Y * 2. + header_h + raw_lines.len() as f64 * LINE_H;
    CodeBlock {
        lang,
        raw_lines,
        lines,
        max_cols,
        header_h,
        w,
        h,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LangKind {
    Sql,
    Rust,
    Go,
    Js,
    Python,
    Data,
    Shell,
    Generic,
}

fn lang_kind(lang: &str) -> LangKind {
    match lang.trim().to_ascii_lowercase().as_str() {
        "sql" | "postgres" | "postgresql" | "sqlite" | "mysql" => LangKind::Sql,
        "rust" | "rs" => LangKind::Rust,
        "go" | "golang" => LangKind::Go,
        "js" | "jsx" | "ts" | "tsx" | "javascript" | "typescript" => LangKind::Js,
        "py" | "python" => LangKind::Python,
        "json" | "yaml" | "yml" | "toml" => LangKind::Data,
        "sh" | "bash" | "zsh" | "shell" => LangKind::Shell,
        _ => LangKind::Generic,
    }
}

fn tokenize_line(lang: &str, line: &str, in_block_comment: &mut bool) -> Vec<CodeToken> {
    let kind = lang_kind(lang);
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    let push_tok = |out: &mut Vec<CodeToken>, tk: TokenKind, slice: &str| {
        if slice.is_empty() {
            return;
        }
        if let Some(last) = out.last_mut()
            && last.kind == tk
        {
            last.text.push_str(slice);
        } else {
            out.push(CodeToken {
                kind: tk,
                text: slice.to_string(),
            });
        }
    };

    while i < bytes.len() {
        if *in_block_comment {
            if let Some(end) = line[i..].find("*/") {
                push_tok(&mut out, TokenKind::Comment, &line[i..i + end + 2]);
                i += end + 2;
                *in_block_comment = false;
                continue;
            } else {
                push_tok(&mut out, TokenKind::Comment, &line[i..]);
                break;
            }
        }

        let rest = &line[i..];
        let hash_cmt = matches!(
            kind,
            LangKind::Python | LangKind::Data | LangKind::Shell | LangKind::Generic
        ) && rest.starts_with('#');
        let dash_cmt = kind == LangKind::Sql && rest.starts_with("--");
        let slash_cmt = !matches!(kind, LangKind::Sql | LangKind::Python) && rest.starts_with("//");
        if hash_cmt || dash_cmt || slash_cmt {
            push_tok(&mut out, TokenKind::Comment, rest);
            break;
        }
        if !matches!(kind, LangKind::Python | LangKind::Data) && rest.starts_with("/*") {
            *in_block_comment = true;
            if let Some(end) = rest[2..].find("*/") {
                push_tok(&mut out, TokenKind::Comment, &rest[..end + 4]);
                i += end + 4;
                *in_block_comment = false;
                continue;
            } else {
                push_tok(&mut out, TokenKind::Comment, rest);
                break;
            }
        }

        let ch = rest.chars().next().unwrap();
        if matches!(ch, '"' | '\'' | '`') {
            let mut n = ch.len_utf8();
            let mut escaped = false;
            for c in rest[n..].chars() {
                n += c.len_utf8();
                if c == ch && !escaped {
                    break;
                }
                if c == '\\' {
                    escaped = !escaped;
                } else {
                    escaped = false;
                }
            }
            // In JSON/YAML, string immediately followed by ':' is a key.
            let after = rest[n..].trim_start();
            let tk = if kind == LangKind::Data && after.starts_with(':') {
                TokenKind::Func
            } else {
                TokenKind::String
            };
            push_tok(&mut out, tk, &rest[..n]);
            i += n;
            continue;
        }

        if ch.is_ascii_digit() {
            let mut n = 1;
            while i + n < bytes.len()
                && (bytes[i + n].is_ascii_alphanumeric() || matches!(bytes[i + n], b'.' | b'_'))
            {
                n += 1;
            }
            push_tok(&mut out, TokenKind::Number, &rest[..n]);
            i += n;
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut n = 1;
            while i + n < bytes.len()
                && (bytes[i + n].is_ascii_alphanumeric() || matches!(bytes[i + n], b'_' | b'-'))
            {
                n += 1;
            }
            let ident = &rest[..n];
            let after = rest[n..].trim_start();
            let tk = classify_ident(kind, ident, after);
            push_tok(&mut out, tk, ident);
            i += n;
            continue;
        }

        let cw = ch.len_utf8();
        push_tok(&mut out, TokenKind::Plain, &rest[..cw]);
        i += cw;
    }

    out
}

fn classify_ident(kind: LangKind, ident: &str, after: &str) -> TokenKind {
    match kind {
        LangKind::Sql => {
            let up = ident.to_ascii_uppercase();
            if matches!(
                up.as_str(),
                "SELECT"
                    | "FROM"
                    | "WHERE"
                    | "INSERT"
                    | "INTO"
                    | "VALUES"
                    | "UPDATE"
                    | "SET"
                    | "DELETE"
                    | "JOIN"
                    | "LEFT"
                    | "RIGHT"
                    | "INNER"
                    | "OUTER"
                    | "ON"
                    | "GROUP"
                    | "BY"
                    | "ORDER"
                    | "HAVING"
                    | "LIMIT"
                    | "OFFSET"
                    | "CREATE"
                    | "ALTER"
                    | "DROP"
                    | "TABLE"
                    | "INDEX"
                    | "PRIMARY"
                    | "KEY"
                    | "REFERENCES"
                    | "AS"
                    | "AND"
                    | "OR"
                    | "NOT"
                    | "IN"
                    | "IS"
                    | "NULL"
                    | "TRUE"
                    | "FALSE"
                    | "RETURNING"
                    | "BEGIN"
                    | "COMMIT"
                    | "ROLLBACK"
                    | "WITH"
                    | "DISTINCT"
                    | "CASE"
                    | "WHEN"
                    | "THEN"
                    | "ELSE"
                    | "END"
                    | "DEFAULT"
                    | "UNIQUE"
                    | "DESC"
                    | "ASC"
            ) {
                TokenKind::Keyword
            } else if matches!(
                up.as_str(),
                "INT"
                    | "INTEGER"
                    | "BIGINT"
                    | "SMALLINT"
                    | "TEXT"
                    | "VARCHAR"
                    | "CHAR"
                    | "BOOLEAN"
                    | "BOOL"
                    | "UUID"
                    | "TIMESTAMP"
                    | "TIMESTAMPTZ"
                    | "JSON"
                    | "JSONB"
                    | "SERIAL"
                    | "FLOAT"
                    | "NUMERIC"
                    | "BYTEA"
            ) {
                TokenKind::Type
            } else if after.starts_with('(') {
                TokenKind::Func
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Rust => {
            if matches!(
                ident,
                "fn" | "let"
                    | "mut"
                    | "pub"
                    | "struct"
                    | "enum"
                    | "impl"
                    | "trait"
                    | "match"
                    | "if"
                    | "else"
                    | "for"
                    | "in"
                    | "while"
                    | "loop"
                    | "return"
                    | "async"
                    | "await"
                    | "use"
                    | "mod"
                    | "crate"
                    | "self"
                    | "Self"
                    | "where"
                    | "const"
                    | "static"
                    | "type"
                    | "true"
                    | "false"
            ) {
                TokenKind::Keyword
            } else if matches!(
                ident,
                "Result"
                    | "Option"
                    | "Vec"
                    | "String"
                    | "str"
                    | "bool"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "isize"
                    | "f32"
                    | "f64"
                    | "Box"
                    | "Arc"
            ) || ident.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            {
                TokenKind::Type
            } else if after.starts_with('(') || after.starts_with('!') {
                TokenKind::Func
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Go => {
            if matches!(
                ident,
                "func"
                    | "package"
                    | "import"
                    | "type"
                    | "struct"
                    | "interface"
                    | "return"
                    | "if"
                    | "else"
                    | "for"
                    | "range"
                    | "go"
                    | "select"
                    | "case"
                    | "default"
                    | "switch"
                    | "chan"
                    | "defer"
                    | "var"
                    | "const"
                    | "true"
                    | "false"
                    | "nil"
            ) {
                TokenKind::Keyword
            } else if matches!(
                ident,
                "string"
                    | "int"
                    | "int32"
                    | "int64"
                    | "uint"
                    | "uint64"
                    | "bool"
                    | "byte"
                    | "error"
            ) || ident.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            {
                TokenKind::Type
            } else if after.starts_with('(') {
                TokenKind::Func
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Js => {
            if matches!(
                ident,
                "const"
                    | "let"
                    | "var"
                    | "function"
                    | "return"
                    | "async"
                    | "await"
                    | "export"
                    | "import"
                    | "from"
                    | "if"
                    | "else"
                    | "class"
                    | "interface"
                    | "type"
                    | "new"
                    | "throw"
                    | "try"
                    | "catch"
                    | "true"
                    | "false"
                    | "null"
                    | "undefined"
            ) {
                TokenKind::Keyword
            } else if matches!(ident, "string" | "number" | "boolean" | "void" | "Promise")
                || ident.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            {
                TokenKind::Type
            } else if after.starts_with('(') {
                TokenKind::Func
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Python => {
            if matches!(
                ident,
                "def"
                    | "class"
                    | "return"
                    | "if"
                    | "elif"
                    | "else"
                    | "for"
                    | "in"
                    | "while"
                    | "import"
                    | "from"
                    | "as"
                    | "with"
                    | "async"
                    | "await"
                    | "yield"
                    | "try"
                    | "except"
                    | "raise"
                    | "True"
                    | "False"
                    | "None"
                    | "and"
                    | "or"
                    | "not"
            ) {
                TokenKind::Keyword
            } else if ident.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                TokenKind::Type
            } else if after.starts_with('(') {
                TokenKind::Func
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Data => {
            if after.starts_with(':') {
                TokenKind::Func
            } else if matches!(ident, "true" | "false" | "null" | "yes" | "no") {
                TokenKind::Keyword
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Shell => {
            if matches!(
                ident,
                "if" | "then"
                    | "else"
                    | "fi"
                    | "for"
                    | "in"
                    | "do"
                    | "done"
                    | "export"
                    | "echo"
                    | "curl"
                    | "git"
                    | "cargo"
                    | "npm"
                    | "docker"
            ) {
                TokenKind::Keyword
            } else {
                TokenKind::Plain
            }
        }
        LangKind::Generic => {
            if after.starts_with('(') {
                TokenKind::Func
            } else if ident.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                TokenKind::Type
            } else {
                TokenKind::Plain
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sql_and_rust_fenced_blocks_inside_node_labels() {
        let label = "Job Queue Lookup\n```sql\nSELECT id, status\nFROM jobs\nWHERE state = 'queued' -- pending\nLIMIT 25;\n```";
        let layout = measure(label, "rect").expect("code block must be detected");
        assert_eq!(layout.title, "Job Queue Lookup");
        assert_eq!(layout.blocks.len(), 1);
        assert_eq!(layout.blocks[0].lang, "sql");
        assert!(layout.total_w >= 260.);
        assert!(layout.total_h >= 120.);
        let all_tokens: Vec<_> = layout.blocks[0]
            .lines
            .iter()
            .flatten()
            .map(|t| (t.kind, t.text.as_str()))
            .collect();
        assert!(all_tokens.contains(&(TokenKind::Keyword, "SELECT")));
        assert!(all_tokens.contains(&(TokenKind::String, "'queued'")));
        assert!(all_tokens.contains(&(TokenKind::Comment, "-- pending")));
        assert!(all_tokens.contains(&(TokenKind::Number, "25")));
    }

    #[test]
    fn code_only_label_has_empty_title() {
        let label = "```rust\npub fn run() -> bool {\n    true\n}\n```";
        let layout = measure(label, "rect").unwrap();
        assert!(layout.title.is_empty());
        assert_eq!(layout.title_h, 0.);
        assert_eq!(layout.blocks[0].lang, "rust");
        let kws: Vec<_> = layout.blocks[0].lines[0]
            .iter()
            .filter(|t| t.kind == TokenKind::Keyword)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(kws, vec!["pub", "fn"]);
    }
}
