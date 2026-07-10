//! The `.lss` lexer (04 §1). Whitespace is significant as the descendant
//! combinator, so each token records whether whitespace preceded it.

use crate::ast::{Span, Unit};

/// A lexical token.
#[derive(Clone, Debug, PartialEq)]
pub enum Tk {
    /// An identifier (letters, digits, `-`, `_`).
    Ident(String),
    /// `@keyword`.
    At(String),
    /// `#hash` (id selector or hex color — disambiguated by the parser).
    Hash(String),
    /// `$ident` token reference.
    Var(String),
    /// A double-quoted string.
    Str(String),
    /// A number with a unit.
    Num(f64, Unit),
    /// `.`.
    Dot,
    /// `:`.
    Colon,
    /// `;`.
    Semi,
    /// `,`.
    Comma,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `>`.
    Gt,
    /// `<`.
    Lt,
    /// `>=`.
    Ge,
    /// `<=`.
    Le,
    /// `&`.
    Amp,
    /// `!`.
    Bang,
    /// `*`.
    Star,
    /// `+` (calc operator, B.7 relative colors).
    Plus,
    /// A bare `-` (calc operator; `-` glued to a digit still lexes as a
    /// negative number).
    Minus,
    /// `%` not attached to a number (rare).
    Percent,
    /// End of input.
    Eof,
}

/// A token with its source span and whether whitespace/comment preceded it.
#[derive(Clone, Debug)]
pub struct Token {
    /// The token kind.
    pub kind: Tk,
    /// Source span.
    pub span: Span,
    /// Whether whitespace or a comment preceded this token.
    pub ws_before: bool,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

/// Tokenize `src`. Lexing never fails; malformed input becomes tokens the parser
/// rejects with spans.
pub fn lex(src: &str) -> Vec<Token> {
    let mut lx = Lexer {
        src: src.as_bytes(),
        pos: 0,
        line: 1,
        col: 1,
    };
    let mut out = Vec::new();
    loop {
        let ws = lx.skip_trivia();
        let span = Span {
            line: lx.line,
            col: lx.col,
        };
        let kind = lx.next_kind();
        let eof = kind == Tk::Eof;
        out.push(Token {
            kind,
            span,
            ws_before: ws,
        });
        if eof {
            break;
        }
    }
    out
}

impl Lexer<'_> {
    fn peek(&self) -> u8 {
        self.src.get(self.pos).copied().unwrap_or(0)
    }
    fn peek2(&self) -> u8 {
        self.src.get(self.pos + 1).copied().unwrap_or(0)
    }
    fn bump(&mut self) -> u8 {
        let c = self.peek();
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    /// Skip whitespace and comments; return whether anything was skipped.
    fn skip_trivia(&mut self) -> bool {
        let start = self.pos;
        loop {
            match self.peek() {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.bump();
                }
                b'/' if self.peek2() == b'/' => {
                    while self.peek() != b'\n' && self.peek() != 0 {
                        self.bump();
                    }
                }
                b'/' if self.peek2() == b'*' => {
                    self.bump();
                    self.bump();
                    while !(self.peek() == b'*' && self.peek2() == b'/') && self.peek() != 0 {
                        self.bump();
                    }
                    self.bump();
                    self.bump();
                }
                _ => break,
            }
        }
        self.pos != start
    }

    fn next_kind(&mut self) -> Tk {
        let c = self.peek();
        match c {
            0 => Tk::Eof,
            b'{' => {
                self.bump();
                Tk::LBrace
            }
            b'}' => {
                self.bump();
                Tk::RBrace
            }
            b'(' => {
                self.bump();
                Tk::LParen
            }
            b')' => {
                self.bump();
                Tk::RParen
            }
            b':' => {
                self.bump();
                Tk::Colon
            }
            b';' => {
                self.bump();
                Tk::Semi
            }
            b',' => {
                self.bump();
                Tk::Comma
            }
            b'&' => {
                self.bump();
                Tk::Amp
            }
            b'!' => {
                self.bump();
                Tk::Bang
            }
            b'*' => {
                self.bump();
                Tk::Star
            }
            b'%' => {
                self.bump();
                Tk::Percent
            }
            b'>' => {
                self.bump();
                if self.peek() == b'=' {
                    self.bump();
                    Tk::Ge
                } else {
                    Tk::Gt
                }
            }
            b'<' => {
                self.bump();
                if self.peek() == b'=' {
                    self.bump();
                    Tk::Le
                } else {
                    Tk::Lt
                }
            }
            b'@' => {
                self.bump();
                Tk::At(self.read_ident())
            }
            b'#' => {
                self.bump();
                Tk::Hash(self.read_ident_or_hex())
            }
            b'$' => {
                self.bump();
                Tk::Var(self.read_ident())
            }
            b'"' => Tk::Str(self.read_string()),
            b'.' if self.peek2().is_ascii_digit() => self.read_number(),
            b'.' => {
                self.bump();
                Tk::Dot
            }
            b'0'..=b'9' => self.read_number(),
            b'-' if self.peek2().is_ascii_digit() || self.peek2() == b'.' => self.read_number(),
            b'+' => {
                self.bump();
                Tk::Plus
            }
            b'-' => {
                self.bump();
                Tk::Minus
            }
            c if is_ident_start(c) => Tk::Ident(self.read_ident()),
            _ => {
                self.bump();
                // Unknown delimiter; surface as an empty ident so the parser can
                // produce a precise E0101 at this span.
                Tk::Ident(String::new())
            }
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        if self.peek() == b'-' {
            s.push('-');
            self.bump();
        }
        while is_ident_continue(self.peek()) {
            s.push(self.bump() as char);
        }
        s
    }

    fn read_ident_or_hex(&mut self) -> String {
        let mut s = String::new();
        while is_ident_continue(self.peek()) {
            s.push(self.bump() as char);
        }
        s
    }

    fn read_string(&mut self) -> String {
        self.bump(); // opening quote
        let mut s = String::new();
        while self.peek() != b'"' && self.peek() != 0 {
            s.push(self.bump() as char);
        }
        self.bump(); // closing quote (if present)
        s
    }

    fn read_number(&mut self) -> Tk {
        let mut s = String::new();
        if self.peek() == b'-' {
            s.push('-');
            self.bump();
        }
        while self.peek().is_ascii_digit() || self.peek() == b'.' {
            s.push(self.bump() as char);
        }
        let n: f64 = s.parse().unwrap_or(0.0);
        // Unit suffix.
        let unit = if self.peek() == b'%' {
            self.bump();
            Unit::Px // placeholder; replaced below
        } else {
            Unit::None
        };
        if matches!(unit, Unit::Px) {
            return Tk::Num(n, Unit::Percent);
        }
        let mut suffix = String::new();
        while is_ident_continue(self.peek()) {
            suffix.push(self.bump() as char);
        }
        let unit = match suffix.as_str() {
            "" => Unit::None,
            "px" => Unit::Px,
            "ms" => Unit::Ms,
            "s" => Unit::S,
            "deg" => Unit::Deg,
            _ => Unit::None, // unknown unit; treated as unitless
        };
        Tk::Num(n, unit)
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}
fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}
