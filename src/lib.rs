//! L# 的公共库接口。
//!
//! ```no_run
//! let out = lsharp::run("print (1 + 2)").unwrap();
//! assert_eq!(out.trim(), "3");
//! ```

pub mod ast;
pub mod error;
pub mod eval;
pub mod lexer;
pub mod parser;

pub use error::LsError;
pub use eval::{Interp, ListCell, Value};

/// 标准库源码，编译期嵌入可执行文件。
pub const PRELUDE: &str = include_str!("prelude.lsh");

/// 创建已经加载好标准库的解释器。
pub fn new_engine() -> Result<Interp, LsError> {
    let mut interp = Interp::new();
    interp.load_source(PRELUDE)?;
    Ok(interp)
}

/// 执行一段 L# 源码，返回程序打印出来的内容（含标准库加载）。
///
/// 求值器是深递归的，这里放到 64MB 栈的线程里执行，避免默认 8MB 栈被深度递归用尽。
pub fn run(src: &str) -> Result<String, LsError> {
    let src = src.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || -> Result<String, LsError> {
            let mut interp = new_engine()?;
            interp.load_source(&src)?;
            Ok(interp.take_output())
        })
        .map_err(|e| LsError::new(format!("无法创建解释器线程：{}", e)))?;
    handle
        .join()
        .unwrap_or_else(|_| Err(LsError::new("解释器线程异常结束")))
}
