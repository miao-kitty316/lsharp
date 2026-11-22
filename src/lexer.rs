//! L# 的词法分析器。

use crate::error::LsError;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Ident(String),
    /// 首字母大写的标识符：构造子名 / 类型名
    Upper(String),
    Kw(String),
    Sym(String),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

const KEYWORDS: &[&str] = &[
    "data", "let", "in", "if", "then", "else", "match", "with", "rec", "true", "false",
];

const TWO_CHAR_SYMBOLS: &[&str] = &["->", "::", "==", "/=", "<=", ">=", "&&", "||"];

const ONE_CHAR_SYMBOLS: &str = "()[]{},=|\\+-*/%<>!.:;";

pub fn lex(src: &str) -> Result<Vec<Token>, LsError> {
    let mut lx = Lexer { chars: src.chars().collect(), i: 0, line: 1, col: 1 };
    let mut out = Vec::new();
    loop {
        let t = lx.next_token()?;
        let done = matches!(t.tok, Tok::Eof);
        out.push(t);
        if done {
            break;
        }
    }
    Ok(out)
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_trivia(&mut self) -> Result<(), LsError> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                // 行注释 -- ...
                Some('-') if self.peek_at(1) == Some('-') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                // 块注释 {- ... -}
                Some('{') if self.peek_at(1) == Some('-') => {
                    let (line, col) = (self.line, self.col);
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => {
                                return Err(LsError::eof("块注释 {- ... -} 没有闭合", line, col))
                            }
                            Some('-') if self.peek_at(1) == Some('}') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, LsError> {
        self.skip_trivia()?;
        let (line, col) = (self.line, self.col);
        let c = match self.peek() {
            Some(c) => c,
            None => return Ok(Token { tok: Tok::Eof, line, col }),
        };
        if c.is_ascii_digit() {
            return self.number(line, col);
        }
        if c.is_alphabetic() || c == '_' {
            return self.ident(line, col);
        }
        if c == '"' {
            return self.string_lit(line, col);
        }
        if c == '\'' {
            return self.char_lit(line, col);
        }
        self.symbol(line, col)
    }

    fn number(&mut self, line: usize, col: usize) -> Result<Token, LsError> {
        let start = self.i;
        if self.peek() == Some('0') && matches!(self.peek_at(1), Some('x') | Some('X')) {
            self.bump();
            self.bump();
            let digits_start = self.i;
            while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                self.bump();
            }
            if digits_start == self.i {
                return Err(LsError::at("十六进制字面量缺少数字", line, col));
            }
            let text: String = self.chars[digits_start..self.i].iter().collect();
            let v = i64::from_str_radix(&text, 16)
                .map_err(|_| LsError::at("整数字面量超出 i64 范围", line, col))?;
            return Ok(Token { tok: Tok::Int(v), line, col });
        }

        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.bump();
        }
        let mut is_float = false;
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
            }
        }
        let text: String = self.chars[start..self.i].iter().collect();
        if is_float {
            let v: f64 = text
                .parse()
                .map_err(|_| LsError::at("浮点字面量解析失败", line, col))?;
            Ok(Token { tok: Tok::Float(v), line, col })
        } else {
            let v: i64 = text
                .parse()
                .map_err(|_| LsError::at("整数字面量超出 i64 范围", line, col))?;
            Ok(Token { tok: Tok::Int(v), line, col })
        }
    }

    fn ident(&mut self, line: usize, col: usize) -> Result<Token, LsError> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.bump();
        }
        let s: String = self.chars[start..self.i].iter().collect();
        let tok = if KEYWORDS.contains(&s.as_str()) {
            Tok::Kw(s)
        } else if s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            Tok::Upper(s)
        } else {
            Tok::Ident(s)
        };
        Ok(Token { tok, line, col })
    }

    fn string_lit(&mut self, line: usize, col: usize) -> Result<Token, LsError> {
        self.bump(); // 开引号
        let mut s = String::new();
        loop {
            match self.bump() {
                None => return Err(LsError::eof("字符串没有闭合", line, col)),
                Some('"') => break,
                Some('\\') => match self.bump() {
                    None => return Err(LsError::eof("字符串没有闭合", line, col)),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('0') => s.push('\0'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('\'') => s.push('\''),
                    Some(other) => {
                        return Err(LsError::at(format!("未知转义 `\\{}`", other), line, col))
                    }
                },
                Some(c) => s.push(c),
            }
        }
        Ok(Token { tok: Tok::Str(s), line, col })
    }

    fn char_lit(&mut self, line: usize, col: usize) -> Result<Token, LsError> {
        self.bump(); // 开单引号
        let c = match self.bump() {
            None => return Err(LsError::eof("字符字面量没有闭合", line, col)),
            Some('\\') => match self.bump() {
                None => return Err(LsError::eof("字符字面量没有闭合", line, col)),
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('0') => '\0',
                Some('\\') => '\\',
                Some('"') => '"',
                Some('\'') => '\'',
                Some(other) => {
                    return Err(LsError::at(format!("未知转义 `\\{}`", other), line, col))
                }
            },
            Some(c) => c,
        };
        if self.peek() != Some('\'') {
            return Err(LsError::at("字符字面量只能包含一个字符", line, col));
        }
        self.bump();
        Ok(Token { tok: Tok::Char(c), line, col })
    }

    fn symbol(&mut self, line: usize, col: usize) -> Result<Token, LsError> {
        let c = self.bump().unwrap();
        let two = match self.peek() {
            Some(n) => format!("{}{}", c, n),
            None => String::new(),
        };
        if TWO_CHAR_SYMBOLS.contains(&two.as_str()) {
            self.bump();
            return Ok(Token { tok: Tok::Sym(two), line, col });
        }
        if ONE_CHAR_SYMBOLS.contains(c) {
            Ok(Token { tok: Tok::Sym(c.to_string()), line, col })
        } else {
            Err(LsError::at(format!("无法识别的字符 `{}`", c), line, col))
        }
    }
}
