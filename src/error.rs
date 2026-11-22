//! L# 的错误类型：词法 / 语法错误带行列号，运行时错误只带消息。

use std::fmt;

#[derive(Clone, Debug)]
pub struct LsError {
    pub message: String,
    pub line: usize,
    pub col: usize,
    /// 错误点落在文件末尾（REPL 据此判断“还没写完”，继续读下一行）
    pub at_eof: bool,
}

impl LsError {
    pub fn new(message: impl Into<String>) -> Self {
        LsError { message: message.into(), line: 0, col: 0, at_eof: false }
    }

    pub fn at(message: impl Into<String>, line: usize, col: usize) -> Self {
        LsError { message: message.into(), line, col, at_eof: false }
    }

    pub fn eof(message: impl Into<String>, line: usize, col: usize) -> Self {
        LsError { message: message.into(), line, col, at_eof: true }
    }

    pub fn is_positioned(&self) -> bool {
        self.line > 0
    }
}

impl fmt::Display for LsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_positioned() {
            write!(f, "第 {} 行第 {} 列：{}", self.line, self.col, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for LsError {}
