// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Literals
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    True,
    False,

    // Identifiers & keywords
    Ident(String),
    LatestEventToState,
    HasExisted,
    HasExistedWithin,
    DurationWhere,
    DurationInCurState,
    Aggregate,
    GroupBy,
    Count,
    Sum,
    Avg,
    Min,
    Max,

    // Operators
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    AmpAmp,
    PipePipe,
    Bang,
    Pipe,
    Eq,

    // Delimiters
    LParen,
    RParen,
    Comma,
    Dot,

    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::StringLit(s) => write!(f, "\"{}\"", s),
            Token::IntLit(n) => write!(f, "{}", n),
            Token::FloatLit(n) => write!(f, "{}", n),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Ident(s) => write!(f, "{}", s),
            Token::LatestEventToState => write!(f, "latest_event_to_state"),
            Token::HasExisted => write!(f, "has_existed"),
            Token::HasExistedWithin => write!(f, "has_existed_within"),
            Token::DurationWhere => write!(f, "duration_where"),
            Token::DurationInCurState => write!(f, "duration_in_cur_state"),
            Token::Aggregate => write!(f, "aggregate"),
            Token::GroupBy => write!(f, "group_by"),
            Token::Count => write!(f, "count"),
            Token::Sum => write!(f, "sum"),
            Token::Avg => write!(f, "avg"),
            Token::Min => write!(f, "min"),
            Token::Max => write!(f, "max"),
            Token::EqualEqual => write!(f, "=="),
            Token::Greater => write!(f, ">"),
            Token::GreaterEqual => write!(f, ">="),
            Token::Less => write!(f, "<"),
            Token::LessEqual => write!(f, "<="),
            Token::AmpAmp => write!(f, "&&"),
            Token::PipePipe => write!(f, "||"),
            Token::Bang => write!(f, "!"),
            Token::Pipe => write!(f, "|"),
            Token::Eq => write!(f, "="),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spanned {
    pub token: Token,
    pub offset: usize,
}

pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Spanned>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            if self.pos >= self.input.len() {
                tokens.push(Spanned {
                    token: Token::Eof,
                    offset: self.pos,
                });
                return Ok(tokens);
            }
            tokens.push(self.next_token()?);
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.input.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> u8 {
        let b = self.input[self.pos];
        self.pos += 1;
        b
    }

    fn next_token(&mut self) -> Result<Spanned, LexError> {
        let offset = self.pos;

        let byte = self.peek().ok_or(LexError {
            offset,
            message: "unexpected end of input".to_string(),
        })?;
        match byte {
            b'"' => self.read_string(offset),
            b'(' => {
                self.advance();
                Ok(Spanned {
                    token: Token::LParen,
                    offset,
                })
            }
            b')' => {
                self.advance();
                Ok(Spanned {
                    token: Token::RParen,
                    offset,
                })
            }
            b',' => {
                self.advance();
                Ok(Spanned {
                    token: Token::Comma,
                    offset,
                })
            }
            b'.' => {
                self.advance();
                Ok(Spanned {
                    token: Token::Dot,
                    offset,
                })
            }
            b'!' => {
                self.advance();
                Ok(Spanned {
                    token: Token::Bang,
                    offset,
                })
            }
            b'=' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(Spanned {
                        token: Token::EqualEqual,
                        offset,
                    })
                } else {
                    Ok(Spanned {
                        token: Token::Eq,
                        offset,
                    })
                }
            }
            b'>' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(Spanned {
                        token: Token::GreaterEqual,
                        offset,
                    })
                } else {
                    Ok(Spanned {
                        token: Token::Greater,
                        offset,
                    })
                }
            }
            b'<' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(Spanned {
                        token: Token::LessEqual,
                        offset,
                    })
                } else {
                    Ok(Spanned {
                        token: Token::Less,
                        offset,
                    })
                }
            }
            b'&' => {
                self.advance();
                if self.peek() == Some(b'&') {
                    self.advance();
                    Ok(Spanned {
                        token: Token::AmpAmp,
                        offset,
                    })
                } else {
                    Err(LexError {
                        offset,
                        message: "expected '&&'".into(),
                    })
                }
            }
            b'|' => {
                self.advance();
                if self.peek() == Some(b'|') {
                    self.advance();
                    Ok(Spanned {
                        token: Token::PipePipe,
                        offset,
                    })
                } else {
                    Ok(Spanned {
                        token: Token::Pipe,
                        offset,
                    })
                }
            }
            b if b.is_ascii_digit() => self.read_number(offset),
            b if is_ident_start(b) => self.read_ident(offset),
            other => Err(LexError {
                offset,
                message: format!("unexpected character '{}'", other as char),
            }),
        }
    }

    fn read_string(&mut self, offset: usize) -> Result<Spanned, LexError> {
        self.advance(); // opening quote
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos] != b'"' {
            self.pos += 1;
        }
        if self.pos >= self.input.len() {
            return Err(LexError {
                offset,
                message: "unterminated string".into(),
            });
        }
        let s = String::from_utf8_lossy(&self.input[start..self.pos]).into_owned();
        self.advance(); // closing quote
        Ok(Spanned {
            token: Token::StringLit(s),
            offset,
        })
    }

    fn read_number(&mut self, offset: usize) -> Result<Spanned, LexError> {
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') && self.peek_next().map_or(false, |b| b.is_ascii_digit()) {
            self.advance(); // '.'
            while self.pos < self.input.len() && self.input[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| LexError {
                offset,
                message: "invalid UTF-8".to_string(),
            })?;
            let val: f64 = s.parse().map_err(|_| LexError {
                offset,
                message: format!("invalid float '{}'", s),
            })?;
            Ok(Spanned {
                token: Token::FloatLit(val),
                offset,
            })
        } else {
            let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| LexError {
                offset,
                message: "invalid UTF-8".to_string(),
            })?;
            let val: i64 = s.parse().map_err(|_| LexError {
                offset,
                message: format!("invalid integer '{}'", s),
            })?;
            Ok(Spanned {
                token: Token::IntLit(val),
                offset,
            })
        }
    }

    fn read_ident(&mut self, offset: usize) -> Result<Spanned, LexError> {
        let start = self.pos;
        while self.pos < self.input.len() && is_ident_cont(self.input[self.pos]) {
            self.pos += 1;
        }
        let word = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| LexError {
            offset,
            message: "invalid UTF-8".to_string(),
        })?;
        let token = match word {
            "latest_event_to_state" => Token::LatestEventToState,
            "has_existed" => Token::HasExisted,
            "has_existed_within" => Token::HasExistedWithin,
            "duration_where" => Token::DurationWhere,
            "duration_in_cur_state" => Token::DurationInCurState,
            "aggregate" => Token::Aggregate,
            "group_by" => Token::GroupBy,
            "count" => Token::Count,
            "sum" => Token::Sum,
            "avg" => Token::Avg,
            "min" => Token::Min,
            "max" => Token::Max,
            "true" => Token::True,
            "false" => Token::False,
            _ => Token::Ident(word.to_string()),
        };
        Ok(Spanned { token, offset })
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lex error at offset {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for LexError {}
