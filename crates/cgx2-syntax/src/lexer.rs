/// A zero-based, half-open UTF-8 byte range in the original source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Name(String),
    Integer(String),
    StringLiteral(String),
    Module,
    Import,
    Pub,
    Fn,
    Let,
    Mut,
    For,
    If,
    Else,
    In,
    Return,
    NoGc,
    True,
    False,
    LParen,
    RParen,
    LAngle,
    RAngle,
    Comma,
    Dot,
    Colon,
    Arrow,
    Equal,
    EqualEqual,
    NotEqual,
    LessEqual,
    GreaterEqual,
    PlusEqual,
    Plus,
    Minus,
    Star,
    Slash,
    Newline,
    Indent,
    Dedent,
    Eof,
}

fn token(tokens: &mut Vec<Token>, kind: TokenKind, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        span: Span { start, end },
    });
}

fn word_kind(word: &str) -> TokenKind {
    match word {
        "module" => TokenKind::Module,
        "import" => TokenKind::Import,
        "pub" => TokenKind::Pub,
        "fn" => TokenKind::Fn,
        "let" => TokenKind::Let,
        "mut" => TokenKind::Mut,
        "for" => TokenKind::For,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "in" => TokenKind::In,
        "return" => TokenKind::Return,
        "no_gc" => TokenKind::NoGc,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        _ => TokenKind::Name(word.to_owned()),
    }
}

/// Lex source into tokens, including logical NEWLINE, INDENT and DEDENT markers.
/// Blank and comment-only lines do not affect indentation.
pub fn lex(source: &str) -> Result<Vec<Token>, Vec<SyntaxDiagnostic>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut levels = vec![0usize];
    let mut offset = 0;
    let mut parens = 0usize;
    for line in source.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let content = body.strip_suffix('\r').unwrap_or(body);
        let mut i = 0;
        if parens == 0 {
            while i < content.len() && matches!(content.as_bytes()[i], b' ' | b'\t') {
                if content.as_bytes()[i] == b'\t' {
                    errors.push(SyntaxDiagnostic {
                        code: "CGS001",
                        message: "tab in indentation".into(),
                        span: Span {
                            start: offset + i,
                            end: offset + i + 1,
                        },
                    });
                }
                i += 1;
            }
            if content[i..].trim().is_empty() || content[i..].starts_with('#') {
                offset += line.len();
                continue;
            }
            let spaces = content[..i].bytes().filter(|b| *b == b' ').count();
            let current = *levels.last().unwrap_or(&0);
            if spaces > current {
                levels.push(spaces);
                token(&mut tokens, TokenKind::Indent, offset, offset + i);
            } else if spaces < current {
                while levels.len() > 1 && *levels.last().unwrap_or(&0) > spaces {
                    levels.pop();
                    token(&mut tokens, TokenKind::Dedent, offset + i, offset + i);
                }
                if *levels.last().unwrap_or(&0) != spaces {
                    errors.push(SyntaxDiagnostic {
                        code: "CGS002",
                        message: "indentation does not match an outer level".into(),
                        span: Span {
                            start: offset,
                            end: offset + i,
                        },
                    });
                }
            }
        } else {
            i = content.len() - content.trim_start_matches([' ', '\t']).len();
            for (column, byte) in content.as_bytes()[..i].iter().enumerate() {
                if *byte == b'\t' {
                    errors.push(SyntaxDiagnostic {
                        code: "CGS001",
                        message: "tab in indentation".into(),
                        span: Span {
                            start: offset + column,
                            end: offset + column + 1,
                        },
                    });
                }
            }
        }
        while i < content.len() {
            let ch = content[i..].chars().next().unwrap_or('\0');
            let start = offset + i;
            if ch == '#' {
                break;
            }
            if ch.is_whitespace() {
                i += ch.len_utf8();
                continue;
            }
            if ch == '_' || ch.is_alphabetic() {
                let begin = i;
                i += ch.len_utf8();
                while i < content.len() {
                    let next = content[i..].chars().next().unwrap_or('\0');
                    if next == '_' || next.is_alphanumeric() {
                        i += next.len_utf8();
                    } else {
                        break;
                    }
                }
                token(
                    &mut tokens,
                    word_kind(&content[begin..i]),
                    start,
                    offset + i,
                );
                continue;
            }
            if ch.is_ascii_digit() {
                let begin = i;
                i += 1;
                while i < content.len()
                    && (content.as_bytes()[i].is_ascii_digit() || content.as_bytes()[i] == b'_')
                {
                    i += 1;
                }
                token(
                    &mut tokens,
                    TokenKind::Integer(content[begin..i].into()),
                    start,
                    offset + i,
                );
                continue;
            }
            if ch == '"' {
                let begin = i;
                i += 1;
                let mut closed = false;
                while i < content.len() {
                    let next = content[i..].chars().next().unwrap_or('\0');
                    if next == '\\' {
                        i += 1;
                        if i < content.len() {
                            i += content[i..].chars().next().unwrap_or('\0').len_utf8();
                        }
                    } else if next == '"' {
                        i += 1;
                        closed = true;
                        break;
                    } else {
                        i += next.len_utf8();
                    }
                }
                if closed {
                    token(
                        &mut tokens,
                        TokenKind::StringLiteral(content[begin..i].into()),
                        start,
                        offset + i,
                    );
                } else {
                    errors.push(SyntaxDiagnostic {
                        code: "CGS003",
                        message: "unterminated string".into(),
                        span: Span {
                            start,
                            end: offset + i,
                        },
                    });
                }
                continue;
            }
            let rest = &content[i..];
            let (kind, width) = if rest.starts_with("->") {
                (Some(TokenKind::Arrow), 2)
            } else if rest.starts_with("+=") {
                (Some(TokenKind::PlusEqual), 2)
            } else if rest.starts_with("==") {
                (Some(TokenKind::EqualEqual), 2)
            } else if rest.starts_with("!=") {
                (Some(TokenKind::NotEqual), 2)
            } else if rest.starts_with("<=") {
                (Some(TokenKind::LessEqual), 2)
            } else if rest.starts_with(">=") {
                (Some(TokenKind::GreaterEqual), 2)
            } else {
                (
                    match ch {
                        '(' => {
                            parens += 1;
                            Some(TokenKind::LParen)
                        }
                        ')' => {
                            parens = parens.saturating_sub(1);
                            Some(TokenKind::RParen)
                        }
                        '<' => Some(TokenKind::LAngle),
                        '>' => Some(TokenKind::RAngle),
                        ',' => Some(TokenKind::Comma),
                        '.' => Some(TokenKind::Dot),
                        ':' => Some(TokenKind::Colon),
                        '=' => Some(TokenKind::Equal),
                        '+' => Some(TokenKind::Plus),
                        '-' => Some(TokenKind::Minus),
                        '*' => Some(TokenKind::Star),
                        '/' => Some(TokenKind::Slash),
                        _ => None,
                    },
                    ch.len_utf8(),
                )
            };
            if let Some(kind) = kind {
                token(&mut tokens, kind, start, start + width);
            } else {
                errors.push(SyntaxDiagnostic {
                    code: "CGS004",
                    message: format!("unexpected character {ch:?}"),
                    span: Span {
                        start,
                        end: start + width,
                    },
                });
            }
            i += width;
        }
        if parens == 0 {
            token(
                &mut tokens,
                TokenKind::Newline,
                offset + content.len(),
                offset + line.len(),
            );
        }
        offset += line.len();
    }
    if parens != 0 {
        errors.push(SyntaxDiagnostic {
            code: "CGS005",
            message: "unclosed parenthesis".into(),
            span: Span {
                start: source.len(),
                end: source.len(),
            },
        });
    }
    if !tokens.is_empty() && !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Newline)) {
        token(&mut tokens, TokenKind::Newline, source.len(), source.len());
    }
    while levels.len() > 1 {
        levels.pop();
        token(&mut tokens, TokenKind::Dedent, source.len(), source.len());
    }
    token(&mut tokens, TokenKind::Eof, source.len(), source.len());
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}
