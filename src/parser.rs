//! L# 的递归下降语法分析器（含优先级爬升）。

use crate::ast::*;
use crate::error::LsError;
use crate::lexer::{lex, Tok, Token};

pub fn parse(src: &str) -> Result<Program, LsError> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    p.program()
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn peek_tok(&self) -> &Tok {
        &self.peek().tok
    }

    fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }

    fn at_kw(&self, k: &str) -> bool {
        matches!(self.peek_tok(), Tok::Kw(s) if s == k)
    }

    fn at_sym(&self, s: &str) -> bool {
        matches!(self.peek_tok(), Tok::Sym(x) if x == s)
    }

    fn err_here(&self, msg: impl Into<String>) -> LsError {
        let t = self.peek();
        let mut e = LsError::at(msg.into(), t.line, t.col);
        if matches!(t.tok, Tok::Eof) {
            e.at_eof = true;
        }
        e
    }

    fn expect_sym(&mut self, s: &str) -> Result<(), LsError> {
        if self.at_sym(s) {
            self.bump();
            Ok(())
        } else {
            Err(self.err_here(format!("这里需要 `{}`", s)))
        }
    }

    fn expect_kw(&mut self, k: &str) -> Result<(), LsError> {
        if self.at_kw(k) {
            self.bump();
            Ok(())
        } else {
            Err(self.err_here(format!("这里需要关键字 `{}`", k)))
        }
    }

    fn expect_ident(&mut self) -> Result<String, LsError> {
        match self.peek_tok().clone() {
            Tok::Ident(s) => {
                self.bump();
                Ok(s)
            }
            _ => Err(self.err_here("这里需要一个变量名")),
        }
    }

    fn expect_upper(&mut self) -> Result<String, LsError> {
        match self.peek_tok().clone() {
            Tok::Upper(s) => {
                self.bump();
                Ok(s)
            }
            _ => Err(self.err_here("这里需要一个构造子名（首字母大写）")),
        }
    }

    fn starts_atom(&self) -> bool {
        match self.peek_tok() {
            Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Char(_) => true,
            Tok::Ident(_) | Tok::Upper(_) => true,
            Tok::Kw(k) => k == "true" || k == "false",
            Tok::Sym(s) => s == "(" || s == "[",
            _ => false,
        }
    }

    fn program(&mut self) -> Result<Program, LsError> {
        let mut decls = Vec::new();
        while !matches!(self.peek_tok(), Tok::Eof) {
            if self.at_kw("data") {
                decls.push(Decl::Data(self.data_decl()?));
            } else if self.at_kw("let") {
                decls.push(Decl::Let(self.let_decl()?));
            } else {
                decls.push(Decl::Expr(self.expr()?));
            }
        }
        Ok(Program { decls })
    }

    fn data_decl(&mut self) -> Result<DataDecl, LsError> {
        self.expect_kw("data")?;
        let name = self.expect_upper()?;

        // 类型参数：收集 `=` 之前出现的小写标识符
        let mut tyvars: Vec<String> = Vec::new();
        while !self.at_sym("=") {
            if matches!(self.peek_tok(), Tok::Eof) {
                return Err(self.err_here("data 声明缺少 `=`"));
            }
            if let Tok::Ident(s) = self.peek_tok() {
                tyvars.push(s.clone());
            }
            self.bump();
        }
        self.expect_sym("=")?;

        let mut ctors = Vec::new();
        loop {
            let cname = self.expect_upper()?;
            let mut arity = 0usize;
            while self.at_type_atom(&tyvars) {
                self.skip_type_atom()?;
                arity += 1;
            }
            ctors.push(CtorDecl { name: cname, arity });
            if self.at_sym("|") {
                self.bump();
            } else {
                break;
            }
        }
        Ok(DataDecl { name, ctors })
    }

    /// 只接受“看起来像类型”的记号，避免把紧随其后的表达式当成类型参数吞掉
    fn at_type_atom(&self, tyvars: &[String]) -> bool {
        match self.peek_tok() {
            Tok::Upper(_) => true,
            Tok::Ident(s) => tyvars.iter().any(|t| t == s),
            Tok::Sym(s) => s == "(" || s == "[",
            _ => false,
        }
    }

    fn skip_type_atom(&mut self) -> Result<(), LsError> {
        match self.peek_tok().clone() {
            Tok::Upper(_) | Tok::Ident(_) => {
                self.bump();
                Ok(())
            }
            Tok::Sym(open) if open == "(" || open == "[" => {
                let close = if open == "(" { ")" } else { "]" };
                self.bump();
                let mut depth = 1usize;
                while depth > 0 {
                    match self.peek_tok().clone() {
                        Tok::Eof => return Err(self.err_here("类型括号没有闭合")),
                        Tok::Sym(x) if x == open => {
                            depth += 1;
                            self.bump();
                        }
                        Tok::Sym(x) if x == close => {
                            depth -= 1;
                            self.bump();
                        }
                        _ => {
                            self.bump();
                        }
                    }
                }
                Ok(())
            }
            _ => Err(self.err_here("无法解析类型")),
        }
    }

    fn let_decl(&mut self) -> Result<LetDecl, LsError> {
        self.expect_kw("let")?;
        let rec = if self.at_kw("rec") {
            self.bump();
            true
        } else {
            false
        };
        let name = self.expect_ident()?;
        let mut params = Vec::new();
        while matches!(self.peek_tok(), Tok::Ident(_)) {
            params.push(self.expect_ident()?);
        }
        self.expect_sym("=")?;
        let body = self.expr()?;
        Ok(LetDecl { name, params, rec, body })
    }

    fn expr(&mut self) -> Result<Expr, LsError> {
        if self.at_kw("if") {
            self.if_expr()
        } else if self.at_kw("match") {
            self.match_expr()
        } else if self.at_kw("let") {
            self.let_expr()
        } else if self.at_sym("\\") {
            self.lambda_expr()
        } else {
            self.bin_expr(0)
        }
    }

    fn if_expr(&mut self) -> Result<Expr, LsError> {
        self.expect_kw("if")?;
        let cond = self.expr()?;
        self.expect_kw("then")?;
        let then_branch = self.expr()?;
        self.expect_kw("else")?;
        let else_branch = self.expr()?;
        Ok(Expr::If(
            Box::new(cond),
            Box::new(then_branch),
            Box::new(else_branch),
        ))
    }

    fn match_expr(&mut self) -> Result<Expr, LsError> {
        self.expect_kw("match")?;
        let scrutinee = self.expr()?;
        self.expect_kw("with")?;

        let mut arms = Vec::new();
        if self.at_sym("|") {
            self.bump();
        }
        loop {
            let pat = self.pattern()?;
            self.expect_sym("->")?;
            let body = self.expr()?;
            arms.push((pat, body));
            if self.at_sym("|") {
                self.bump();
            } else {
                break;
            }
        }
        if arms.is_empty() {
            return Err(self.err_here("match 至少需要一个分支"));
        }
        Ok(Expr::Match(Box::new(scrutinee), arms))
    }

    fn let_expr(&mut self) -> Result<Expr, LsError> {
        self.expect_kw("let")?;
        let rec = if self.at_kw("rec") {
            self.bump();
            true
        } else {
            false
        };
        let name = self.expect_ident()?;
        let mut params = Vec::new();
        while matches!(self.peek_tok(), Tok::Ident(_)) {
            params.push(self.expect_ident()?);
        }
        self.expect_sym("=")?;
        let value = self.expr()?;
        self.expect_kw("in")?;
        let body = self.expr()?;
        Ok(Expr::Let {
            rec,
            name,
            params,
            value: Box::new(value),
            body: Box::new(body),
        })
    }

    fn lambda_expr(&mut self) -> Result<Expr, LsError> {
        self.expect_sym("\\")?;
        let mut params = Vec::new();
        while matches!(self.peek_tok(), Tok::Ident(_)) {
            params.push(self.expect_ident()?);
        }
        if params.is_empty() {
            return Err(self.err_here("匿名函数至少需要一个参数"));
        }
        self.expect_sym("->")?;
        let body = self.expr()?;
        Ok(Expr::Lambda(params, Box::new(body)))
    }

    fn bin_expr(&mut self, min_prec: u8) -> Result<Expr, LsError> {
        let mut lhs = self.unary()?;
        loop {
            let (op, prec, right_assoc) = match self.peek_binop() {
                Some(x) => x,
                None => break,
            };
            if prec < min_prec {
                break;
            }
            self.bump();
            let next_min = if right_assoc { prec } else { prec + 1 };
            let rhs = self.bin_expr(next_min)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn peek_binop(&self) -> Option<(BinOp, u8, bool)> {
        let s = match self.peek_tok() {
            Tok::Sym(s) => s.as_str(),
            _ => return None,
        };
        Some(match s {
            "||" => (BinOp::Or, 1, false),
            "&&" => (BinOp::And, 2, false),
            "==" => (BinOp::Eq, 3, false),
            "/=" => (BinOp::Ne, 3, false),
            "<" => (BinOp::Lt, 3, false),
            "<=" => (BinOp::Le, 3, false),
            ">" => (BinOp::Gt, 3, false),
            ">=" => (BinOp::Ge, 3, false),
            "::" => (BinOp::Cons, 4, true),
            "+" => (BinOp::Add, 5, false),
            "-" => (BinOp::Sub, 5, false),
            "*" => (BinOp::Mul, 6, false),
            "/" => (BinOp::Div, 6, false),
            "%" => (BinOp::Mod, 6, false),
            _ => return None,
        })
    }

    fn unary(&mut self) -> Result<Expr, LsError> {
        if self.at_sym("-") {
            self.bump();
            let e = self.unary()?;
            Ok(Expr::Un(UnOp::Neg, Box::new(e)))
        } else if self.at_sym("!") {
            self.bump();
            let e = self.unary()?;
            Ok(Expr::Un(UnOp::Not, Box::new(e)))
        } else {
            self.app()
        }
    }

    fn prev_line(&self) -> usize {
        if self.pos == 0 {
            0
        } else {
            self.toks[self.pos - 1].line
        }
    }

    fn app(&mut self) -> Result<Expr, LsError> {
        let mut f = self.atom()?;
        // 函数应用不跨行：换行意味着“表达式结束”（续行请把参数写在上一行或用括号）
        while self.starts_atom() && self.peek().line == self.prev_line() {
            let a = self.atom()?;
            f = Expr::App(Box::new(f), Box::new(a));
        }
        Ok(f)
    }

    fn atom(&mut self) -> Result<Expr, LsError> {
        match self.peek_tok().clone() {
            Tok::Int(i) => {
                self.bump();
                Ok(Expr::Lit(Lit::Int(i)))
            }
            Tok::Float(f) => {
                self.bump();
                Ok(Expr::Lit(Lit::Float(f)))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Lit(Lit::Str(s)))
            }
            Tok::Char(c) => {
                self.bump();
                Ok(Expr::Lit(Lit::Char(c)))
            }
            Tok::Kw(k) if k == "true" => {
                self.bump();
                Ok(Expr::Lit(Lit::Bool(true)))
            }
            Tok::Kw(k) if k == "false" => {
                self.bump();
                Ok(Expr::Lit(Lit::Bool(false)))
            }
            Tok::Ident(s) => {
                self.bump();
                if s == "_" {
                    Err(self.err_here("`_` 只能出现在模式里"))
                } else {
                    Ok(Expr::Var(s))
                }
            }
            Tok::Upper(s) => {
                self.bump();
                Ok(Expr::Ctor(s, Vec::new()))
            }
            Tok::Sym(s) if s == "(" => {
                self.bump();
                if self.at_sym(")") {
                    self.bump();
                    return Ok(Expr::Lit(Lit::Unit));
                }
                let first = self.expr()?;
                if self.at_sym(",") {
                    let mut elems = vec![first];
                    while self.at_sym(",") {
                        self.bump();
                        if self.at_sym(")") {
                            break;
                        }
                        elems.push(self.expr()?);
                    }
                    self.expect_sym(")")?;
                    Ok(Expr::TupleLit(elems))
                } else {
                    self.expect_sym(")")?;
                    Ok(first)
                }
            }
            Tok::Sym(s) if s == "[" => {
                self.bump();
                let mut elems = Vec::new();
                while !self.at_sym("]") {
                    elems.push(self.expr()?);
                    if self.at_sym(",") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_sym("]")?;
                Ok(Expr::ListLit(elems))
            }
            _ => Err(self.err_here("这里需要一个表达式")),
        }
    }

    fn pattern(&mut self) -> Result<Pattern, LsError> {
        let first = self.pat_atom()?;
        if self.at_sym("::") {
            self.bump();
            let rest = self.pattern()?;
            Ok(Pattern::Cons(Box::new(first), Box::new(rest)))
        } else {
            Ok(first)
        }
    }

    fn starts_pat_atom(&self) -> bool {
        match self.peek_tok() {
            Tok::Int(_) | Tok::Float(_) | Tok::Str(_) | Tok::Char(_) => true,
            Tok::Ident(_) | Tok::Upper(_) => true,
            Tok::Kw(k) => k == "true" || k == "false",
            Tok::Sym(s) => s == "(" || s == "[",
            _ => false,
        }
    }

    fn pat_atom(&mut self) -> Result<Pattern, LsError> {
        match self.peek_tok().clone() {
            Tok::Int(i) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Int(i)))
            }
            Tok::Float(f) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Float(f)))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Str(s)))
            }
            Tok::Char(c) => {
                self.bump();
                Ok(Pattern::Lit(Lit::Char(c)))
            }
            Tok::Kw(k) if k == "true" => {
                self.bump();
                Ok(Pattern::Lit(Lit::Bool(true)))
            }
            Tok::Kw(k) if k == "false" => {
                self.bump();
                Ok(Pattern::Lit(Lit::Bool(false)))
            }
            Tok::Ident(s) => {
                self.bump();
                if s == "_" {
                    Ok(Pattern::Wild)
                } else {
                    Ok(Pattern::Var(s))
                }
            }
            Tok::Upper(name) => {
                self.bump();
                let mut subs = Vec::new();
                while self.starts_pat_atom() {
                    subs.push(self.pat_atom()?);
                }
                Ok(Pattern::Ctor(name, subs))
            }
            Tok::Sym(s) if s == "(" => {
                self.bump();
                if self.at_sym(")") {
                    self.bump();
                    return Ok(Pattern::Lit(Lit::Unit));
                }
                let first = self.pattern()?;
                if self.at_sym(",") {
                    let mut elems = vec![first];
                    while self.at_sym(",") {
                        self.bump();
                        if self.at_sym(")") {
                            break;
                        }
                        elems.push(self.pattern()?);
                    }
                    self.expect_sym(")")?;
                    Ok(Pattern::Tuple(elems))
                } else {
                    self.expect_sym(")")?;
                    Ok(first)
                }
            }
            Tok::Sym(s) if s == "[" => {
                self.bump();
                let mut elems = Vec::new();
                while !self.at_sym("]") {
                    elems.push(self.pattern()?);
                    if self.at_sym(",") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_sym("]")?;
                Ok(Pattern::ListPat(elems))
            }
            _ => Err(self.err_here("这里需要一个模式")),
        }
    }
}
