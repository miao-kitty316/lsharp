//! L# 的抽象语法树。

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Cons,
}

impl BinOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "/=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Cons => "::",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(String),
    Unit,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Lit(Lit),
    Var(String),
    /// 构造子（可能有若干已经给出的参数）
    Ctor(String, Vec<Expr>),
    App(Box<Expr>, Box<Expr>),
    Lambda(Vec<String>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<(Pattern, Expr)>),
    Let {
        rec: bool,
        name: String,
        params: Vec<String>,
        value: Box<Expr>,
        body: Box<Expr>,
    },
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Un(UnOp, Box<Expr>),
    ListLit(Vec<Expr>),
    TupleLit(Vec<Expr>),
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wild,
    Var(String),
    Lit(Lit),
    Ctor(String, Vec<Pattern>),
    Tuple(Vec<Pattern>),
    ListPat(Vec<Pattern>),
    Cons(Box<Pattern>, Box<Pattern>),
}

#[derive(Clone, Debug)]
pub struct CtorDecl {
    pub name: String,
    pub arity: usize,
}

#[derive(Clone, Debug)]
pub struct DataDecl {
    pub name: String,
    pub ctors: Vec<CtorDecl>,
}

#[derive(Clone, Debug)]
pub struct LetDecl {
    pub name: String,
    pub params: Vec<String>,
    pub rec: bool,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Data(DataDecl),
    Let(LetDecl),
    /// 顶层裸表达式：求值后丢弃结果
    Expr(Expr),
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub decls: Vec<Decl>,
}

impl Program {
    /// 返回所有数据声明里的构造子（名字，元数）
    pub fn constructors(&self) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        for d in &self.decls {
            if let Decl::Data(data) = d {
                for c in &data.ctors {
                    out.push((c.name.clone(), c.arity));
                }
            }
        }
        out
    }
}
