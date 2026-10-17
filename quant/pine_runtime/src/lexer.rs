use crate::Diagnostic;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Kind {
    Identifier(String),
    Number(f64),
    String(String),
    Operator(String),
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    Dot,
    Question,
    Colon,
    Newline,
    Indent,
    Dedent,
    End,
}

#[derive(Clone, Debug)]
pub(crate) struct Token {
    pub kind: Kind,
    pub line: usize,
    pub column: usize,
}

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut index = 0;
    let mut line = 1;
    let mut column = 1;
    let mut paren_depth = 0usize;
    let mut indentation = vec![0usize];
    let mut at_line_start = true;
    while index < bytes.len() {
        if at_line_start && paren_depth == 0 {
            let mut next = index;
            let mut width = 0usize;
            while let Some(byte) = bytes.get(next) {
                match byte {
                    b' ' => {
                        width += 1;
                        next += 1;
                    }
                    b'\t' => {
                        width += 4;
                        next += 1;
                    }
                    _ => break,
                }
            }
            if !matches!(bytes.get(next), Some(b'\n') | None)
                && !(bytes.get(next) == Some(&b'/') && bytes.get(next + 1) == Some(&b'/'))
            {
                let current = *indentation.last().unwrap_or(&0);
                if width > current {
                    indentation.push(width);
                    tokens.push(Token {
                        kind: Kind::Indent,
                        line,
                        column: width + 1,
                    });
                } else if width < current {
                    while indentation.last().is_some_and(|level| *level > width) {
                        indentation.pop();
                        tokens.push(Token {
                            kind: Kind::Dedent,
                            line,
                            column: width + 1,
                        });
                    }
                    if indentation.last().copied() != Some(width) {
                        errors.push(Diagnostic::error(
                            line,
                            width + 1,
                            "Block indentation must match its opening level.",
                        ));
                    }
                }
            }
            index = next;
            column = width + 1;
            at_line_start = false;
            if index >= bytes.len() {
                break;
            }
        }
        let byte = bytes[index];
        if byte == b' ' || byte == b'\t' || byte == b'\r' {
            index += 1;
            column += 1;
            continue;
        }
        if byte == b'\n' {
            if paren_depth == 0 {
                tokens.push(Token {
                    kind: Kind::Newline,
                    line,
                    column,
                });
            }
            index += 1;
            line += 1;
            column = 1;
            at_line_start = true;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
                column += 1;
            }
            continue;
        }
        let start = index;
        let start_column = column;
        if is_ident_start(byte) {
            index += 1;
            column += 1;
            while index < bytes.len() && is_ident_continue(bytes[index]) {
                index += 1;
                column += 1;
            }
            tokens.push(Token {
                kind: Kind::Identifier(source[start..index].to_string()),
                line,
                column: start_column,
            });
            continue;
        }
        if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(index + 1).is_some_and(u8::is_ascii_digit))
        {
            index += 1;
            column += 1;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'.') {
                index += 1;
                column += 1;
            }
            if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
                index += 1;
                column += 1;
                if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
                    index += 1;
                    column += 1;
                }
                while index < bytes.len() && bytes[index].is_ascii_digit() {
                    index += 1;
                    column += 1;
                }
            }
            match source[start..index].parse::<f64>() {
                Ok(value) if value.is_finite() => tokens.push(Token {
                    kind: Kind::Number(value),
                    line,
                    column: start_column,
                }),
                _ => errors.push(Diagnostic::error(
                    line,
                    start_column,
                    "Invalid numeric literal.",
                )),
            }
            continue;
        }
        if byte == b'"' {
            index += 1;
            column += 1;
            let mut value = String::new();
            let mut closed = false;
            while index < bytes.len() {
                match bytes[index] {
                    b'"' => {
                        index += 1;
                        column += 1;
                        closed = true;
                        break;
                    }
                    b'\\' if index + 1 < bytes.len() => {
                        index += 1;
                        column += 1;
                        value.push(match bytes[index] {
                            b'n' => '\n',
                            b't' => '\t',
                            other => other as char,
                        });
                        index += 1;
                        column += 1;
                    }
                    b'\n' => {
                        break;
                    }
                    other => {
                        value.push(other as char);
                        index += 1;
                        column += 1;
                    }
                }
            }
            if closed {
                tokens.push(Token {
                    kind: Kind::String(value),
                    line,
                    column: start_column,
                });
            } else {
                errors.push(Diagnostic::error(
                    line,
                    start_column,
                    "Unterminated string literal.",
                ));
            }
            continue;
        }
        let pair = bytes.get(index..index + 2).unwrap_or_default();
        if let Ok(operator) = std::str::from_utf8(pair) {
            if matches!(
                operator,
                ":=" | "+=" | "-=" | "*=" | "/=" | "==" | "!=" | "<=" | ">=" | "=>" | "&&" | "||"
            ) {
                tokens.push(Token {
                    kind: Kind::Operator(operator.to_string()),
                    line,
                    column: start_column,
                });
                index += 2;
                column += 2;
                continue;
            }
        }
        let kind = match byte {
            b'(' => {
                paren_depth += 1;
                Kind::LeftParen
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                Kind::RightParen
            }
            b'[' => Kind::LeftBracket,
            b']' => Kind::RightBracket,
            b',' => Kind::Comma,
            b'.' => Kind::Dot,
            b'?' => Kind::Question,
            b':' => Kind::Colon,
            b'+' | b'-' | b'*' | b'/' | b'%' | b'<' | b'>' | b'=' => {
                Kind::Operator((byte as char).to_string())
            }
            _ => {
                errors.push(Diagnostic::error(
                    line,
                    column,
                    format!("Unsupported character '{}'.", byte as char),
                ));
                index += 1;
                column += 1;
                continue;
            }
        };
        tokens.push(Token { kind, line, column });
        index += 1;
        column += 1;
    }
    while indentation.len() > 1 {
        indentation.pop();
        tokens.push(Token {
            kind: Kind::Dedent,
            line,
            column,
        });
    }
    tokens.push(Token {
        kind: Kind::End,
        line,
        column,
    });
    if tokens.len() > 4_096 {
        return Err(vec![Diagnostic::error(
            1,
            1,
            "Pine source exceeds the 4,096 token runtime limit.",
        )]);
    }
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
