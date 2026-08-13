use crate::error::MdlError;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Node {
    pub kind: NodeKind,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum NodeKind {
    Word(String),
    String(String),
    Number(String),
    Block(Vec<Node>),
    Comma,
    Colon,
}

#[derive(Debug, Clone, PartialEq)]
enum LexemeKind {
    Word(String),
    String(String),
    Number(String),
    LeftBrace,
    RightBrace,
    Comma,
    Colon,
}

#[derive(Debug, Clone, PartialEq)]
struct Lexeme {
    kind: LexemeKind,
    line: usize,
    column: usize,
}

pub(super) fn parse(source: &str) -> Result<Vec<Node>, MdlError> {
    let lexemes = lex(source)?;
    let mut index = 0;
    parse_nodes(&lexemes, &mut index, false)
}

fn lex(source: &str) -> Result<Vec<Lexeme>, MdlError> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut column) = (0, 1, 1);

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\n' {
            i += 1;
            line += 1;
            column = 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            column += 1;
            continue;
        }
        if ch == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                column += 1;
            }
            continue;
        }

        let start_line = line;
        let start_column = column;
        let simple = match ch {
            '{' => Some(LexemeKind::LeftBrace),
            '}' => Some(LexemeKind::RightBrace),
            ',' => Some(LexemeKind::Comma),
            ':' => Some(LexemeKind::Colon),
            _ => None,
        };
        if let Some(kind) = simple {
            out.push(Lexeme { kind, line, column });
            i += 1;
            column += 1;
            continue;
        }

        if ch == '"' {
            i += 1;
            column += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                match chars[i] {
                    '"' => {
                        i += 1;
                        column += 1;
                        closed = true;
                        break;
                    }
                    '\\' if chars.get(i + 1) == Some(&'"') => {
                        value.push('"');
                        i += 2;
                        column += 2;
                    }
                    '\r' => {
                        i += 1;
                    }
                    '\n' => {
                        value.push('\n');
                        i += 1;
                        line += 1;
                        column = 1;
                    }
                    other => {
                        value.push(other);
                        i += 1;
                        column += 1;
                    }
                }
            }
            if !closed {
                return Err(error("mdl-unterminated-string", start_line, start_column));
            }
            out.push(Lexeme {
                kind: LexemeKind::String(value),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if is_number_start(&chars, i) {
            let start = i;
            while i < chars.len() && is_number_part(chars[i]) {
                i += 1;
                column += 1;
            }
            let value: String = chars[start..i].iter().collect();
            if value.parse::<f64>().is_err() {
                return Err(
                    error("mdl-invalid-number", start_line, start_column).with_arg("value", value)
                );
            }
            out.push(Lexeme {
                kind: LexemeKind::Number(value),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '-'))
            {
                i += 1;
                column += 1;
            }
            out.push(Lexeme {
                kind: LexemeKind::Word(chars[start..i].iter().collect()),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        return Err(error("mdl-unexpected-character", line, column).with_arg("character", ch));
    }

    Ok(out)
}

fn is_number_start(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    ch.is_ascii_digit()
        || ch == '.'
        || ((ch == '-' || ch == '+')
            && chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_digit() || *next == '.'))
}

fn is_number_part(ch: char) -> bool {
    ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E')
}

fn parse_nodes(lexemes: &[Lexeme], index: &mut usize, nested: bool) -> Result<Vec<Node>, MdlError> {
    let mut out = Vec::new();
    while let Some(lexeme) = lexemes.get(*index) {
        *index += 1;
        let kind = match &lexeme.kind {
            LexemeKind::Word(value) => NodeKind::Word(value.clone()),
            LexemeKind::String(value) => NodeKind::String(value.clone()),
            LexemeKind::Number(value) => NodeKind::Number(value.clone()),
            LexemeKind::Comma => NodeKind::Comma,
            LexemeKind::Colon => NodeKind::Colon,
            LexemeKind::LeftBrace => NodeKind::Block(parse_nodes(lexemes, index, true)?),
            LexemeKind::RightBrace if nested => return Ok(out),
            LexemeKind::RightBrace => {
                return Err(error(
                    "mdl-unexpected-right-brace",
                    lexeme.line,
                    lexeme.column,
                ));
            }
        };
        out.push(Node {
            kind,
            line: lexeme.line,
            column: lexeme.column,
        });
    }
    if nested {
        let last = lexemes.last();
        return Err(error(
            "mdl-unterminated-block",
            last.map_or(1, |token| token.line),
            last.map_or(1, |token| token.column),
        ));
    }
    Ok(out)
}

pub(super) fn error(key: &'static str, line: usize, column: usize) -> MdlError {
    MdlError::new(key)
        .with_arg("line", line)
        .with_arg("column", column)
}

pub(super) fn word(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::Word(value) => Some(value),
        _ => None,
    }
}

pub(super) fn string(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::String(value) => Some(value),
        _ => None,
    }
}

pub(super) fn block(node: &Node) -> Option<&[Node]> {
    match &node.kind {
        NodeKind::Block(value) => Some(value),
        _ => None,
    }
}

pub(super) fn number<T>(node: &Node) -> Result<T, MdlError>
where
    T: std::str::FromStr,
{
    let NodeKind::Number(value) = &node.kind else {
        return Err(error("mdl-expected-number", node.line, node.column));
    };
    value.parse().map_err(|_| {
        error("mdl-number-out-of-range", node.line, node.column).with_arg("value", value)
    })
}

pub(super) fn named_block<'a>(nodes: &'a [Node], name: &str) -> Option<(&'a [Node], usize)> {
    nodes.iter().enumerate().find_map(|(index, node)| {
        if word(node) != Some(name) {
            return None;
        }
        for candidate in &nodes[index + 1..] {
            if let Some(body) = block(candidate) {
                return Some((body, index));
            }
            if matches!(candidate.kind, NodeKind::Comma) {
                break;
            }
        }
        None
    })
}

pub(super) fn repeated_blocks<'a>(
    nodes: &'a [Node],
    name: &str,
) -> impl Iterator<Item = (&'a [Node], Option<&'a str>)> {
    nodes.iter().enumerate().filter_map(move |(index, node)| {
        if word(node) != Some(name) {
            return None;
        }
        let mut label = None;
        for candidate in &nodes[index + 1..] {
            if label.is_none() {
                label = string(candidate);
            }
            if let Some(body) = block(candidate) {
                return Some((body, label));
            }
            if matches!(candidate.kind, NodeKind::Comma) {
                break;
            }
        }
        None
    })
}

pub(super) fn after<'a>(nodes: &'a [Node], name: &str) -> Option<&'a Node> {
    nodes.iter().enumerate().find_map(|(index, node)| {
        (word(node) == Some(name))
            .then(|| nodes.get(index + 1))
            .flatten()
    })
}

pub(super) fn contains_word(nodes: &[Node], name: &str) -> bool {
    nodes.iter().any(|node| word(node) == Some(name))
}

pub(super) fn vector<const N: usize>(node: &Node) -> Result<[f32; N], MdlError> {
    let Some(nodes) = block(node) else {
        return Err(error("mdl-expected-vector", node.line, node.column));
    };
    let values = nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Number(_)))
        .map(number::<f32>)
        .collect::<Result<Vec<_>, _>>()?;
    values.try_into().map_err(|values: Vec<f32>| {
        error("mdl-vector-size", node.line, node.column)
            .with_arg("expected", N)
            .with_arg("actual", values.len())
    })
}
