//! L# 的求值器：值、环境、解释循环、模式匹配与内置函数。

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::*;
use crate::error::LsError;

// ---------------------------------------------------------------- 值

pub enum ListCell {
    Nil,
    Cons(Value, Rc<ListCell>),
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Str(Rc<String>),
    Unit,
    Tuple(Rc<Vec<Value>>),
    List(Rc<ListCell>),
    Ctor(Rc<String>, Rc<Vec<Value>>),
    Closure(Rc<ClosureData>),
    /// 内置函数：名字、元数、已经收到的参数（柯里化用）
    Builtin(Rc<String>, usize, Rc<Vec<Value>>),
}

pub struct ClosureData {
    pub params: Vec<String>,
    pub body: Rc<Expr>,
    pub env: Env,
    pub applied: Vec<Value>,
}

// ---------------------------------------------------------------- 环境

pub struct EnvNode {
    pub vars: RefCell<HashMap<String, Value>>,
    pub parent: Option<Env>,
}

pub type Env = Rc<EnvNode>;

fn env_child(parent: &Env) -> Env {
    Rc::new(EnvNode { vars: RefCell::new(HashMap::new()), parent: Some(parent.clone()) })
}

fn env_extend(parent: &Env, pairs: Vec<(String, Value)>) -> Env {
    let mut map = HashMap::new();
    for (k, v) in pairs {
        map.insert(k, v);
    }
    Rc::new(EnvNode { vars: RefCell::new(map), parent: Some(parent.clone()) })
}

fn env_lookup(env: &Env, name: &str) -> Option<Value> {
    let mut cur: Option<&Env> = Some(env);
    while let Some(node) = cur {
        if let Some(v) = node.vars.borrow().get(name) {
            return Some(v.clone());
        }
        cur = node.parent.as_ref();
    }
    None
}

// ---------------------------------------------------------------- 解释器

pub struct Interp {
    pub ctors: HashMap<String, usize>,
    pub globals: Env,
    output: String,
    depth: usize,
    max_depth: usize,
}

impl Default for Interp {
    fn default() -> Self {
        Interp::new()
    }
}

impl Interp {
    pub fn new() -> Interp {
        Interp {
            ctors: HashMap::new(),
            globals: Rc::new(EnvNode { vars: RefCell::new(HashMap::new()), parent: None }),
            output: String::new(),
            depth: 0,
            max_depth: 1000,
        }
    }

    pub fn take_output(&mut self) -> String {
        std::mem::take(&mut self.output)
    }

    pub fn load_source(&mut self, src: &str) -> Result<(), LsError> {
        let prog = crate::parser::parse(src)?;
        self.load(&prog)
    }

    /// 按顺序执行一段程序：数据声明先登记构造子，绑定逐个求值，裸表达式求值后丢弃结果。
    pub fn load(&mut self, prog: &Program) -> Result<(), LsError> {
        for decl in &prog.decls {
            match decl {
                Decl::Data(d) => {
                    for c in &d.ctors {
                        self.ctors.insert(c.name.clone(), c.arity);
                    }
                }
                Decl::Let(l) => self.define(l)?,
                Decl::Expr(e) => {
                    let g = self.globals.clone();
                    self.eval(e, &g)?;
                }
            }
        }
        Ok(())
    }

    /// 顶层绑定：带参数的绑定是闭包，因此顶层函数天然互相递归。
    fn define(&mut self, l: &LetDecl) -> Result<(), LsError> {
        let g = self.globals.clone();
        let v = if l.params.is_empty() {
            self.eval(&l.body, &g)?
        } else {
            Value::Closure(Rc::new(ClosureData {
                params: l.params.clone(),
                body: Rc::new(l.body.clone()),
                env: g.clone(),
                applied: Vec::new(),
            }))
        };
        g.vars.borrow_mut().insert(l.name.clone(), v);
        Ok(())
    }

    pub fn eval_top(&mut self, e: &Expr) -> Result<Value, LsError> {
        let g = self.globals.clone();
        self.eval(e, &g)
    }

    fn eval(&mut self, expr: &Expr, env: &Env) -> Result<Value, LsError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            self.depth -= 1;
            return Err(LsError::new(format!(
                "调用层数超过上限 {}，可能是无限递归",
                self.max_depth
            )));
        }
        let r = self.eval_inner(expr, env);
        self.depth -= 1;
        r
    }

    fn eval_inner(&mut self, expr: &Expr, env: &Env) -> Result<Value, LsError> {
        match expr {
            Expr::Lit(l) => Ok(lit_value(l)),

            Expr::Var(name) => {
                if let Some(v) = env_lookup(env, name) {
                    return Ok(v);
                }
                if let Some(arity) = builtin_arity(name) {
                    return Ok(Value::Builtin(Rc::new(name.clone()), arity, Rc::new(Vec::new())));
                }
                Err(LsError::new(format!("未绑定变量 `{}`", name)))
            }

            Expr::Ctor(name, args) => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a, env)?);
                }
                Ok(Value::Ctor(Rc::new(name.clone()), Rc::new(vals)))
            }

            Expr::App(f, x) => {
                let fv = self.eval(f, env)?;
                let xv = self.eval(x, env)?;
                self.apply(fv, xv)
            }

            Expr::Lambda(params, body) => Ok(Value::Closure(Rc::new(ClosureData {
                params: params.clone(),
                body: Rc::new((**body).clone()),
                env: env.clone(),
                applied: Vec::new(),
            }))),

            Expr::If(c, t, e) => {
                let cv = self.eval(c, env)?;
                match cv {
                    Value::Bool(true) => self.eval(t, env),
                    Value::Bool(false) => self.eval(e, env),
                    other => Err(LsError::new(format!(
                        "if 的条件必须是 Bool，得到 {}",
                        show(&other)
                    ))),
                }
            }

            Expr::Match(scrut, arms) => {
                let sv = self.eval(scrut, env)?;
                for (pat, body) in arms {
                    let mut binds = Vec::new();
                    if self.match_pattern(pat, &sv, &mut binds)? {
                        let new_env = env_extend(env, binds);
                        return self.eval(body, &new_env);
                    }
                }
                Err(LsError::new(format!(
                    "match 没有匹配到任何分支，值为 {}",
                    show(&sv)
                )))
            }

            Expr::Let { rec, name, params, value, body } => {
                let new_env = env_child(env);
                if *rec && !params.is_empty() {
                    // 递归局部函数：闭包捕获“包含自己”的环境
                    let clo = Value::Closure(Rc::new(ClosureData {
                        params: params.clone(),
                        body: Rc::new((**value).clone()),
                        env: new_env.clone(),
                        applied: Vec::new(),
                    }));
                    new_env.vars.borrow_mut().insert(name.clone(), clo);
                } else if params.is_empty() {
                    let v = self.eval(value, env)?;
                    new_env.vars.borrow_mut().insert(name.clone(), v);
                } else {
                    let clo = Value::Closure(Rc::new(ClosureData {
                        params: params.clone(),
                        body: Rc::new((**value).clone()),
                        env: env.clone(),
                        applied: Vec::new(),
                    }));
                    new_env.vars.borrow_mut().insert(name.clone(), clo);
                }
                self.eval(body, &new_env)
            }

            Expr::Bin(op, l, r) => match op {
                BinOp::And => match self.eval(l, env)? {
                    Value::Bool(false) => Ok(Value::Bool(false)),
                    Value::Bool(true) => {
                        let rv = self.eval(r, env)?;
                        Ok(Value::Bool(as_bool(&rv)?))
                    }
                    other => Err(type_err("&&", &other)),
                },
                BinOp::Or => match self.eval(l, env)? {
                    Value::Bool(true) => Ok(Value::Bool(true)),
                    Value::Bool(false) => {
                        let rv = self.eval(r, env)?;
                        Ok(Value::Bool(as_bool(&rv)?))
                    }
                    other => Err(type_err("||", &other)),
                },
                _ => {
                    let a = self.eval(l, env)?;
                    let b = self.eval(r, env)?;
                    self.binop(*op, a, b)
                }
            },

            Expr::Un(op, e) => {
                let v = self.eval(e, env)?;
                match op {
                    UnOp::Not => Ok(Value::Bool(!as_bool(&v)?)),
                    UnOp::Neg => match v {
                        Value::Int(i) => i
                            .checked_neg()
                            .map(Value::Int)
                            .ok_or_else(|| LsError::new("整数取负溢出")),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(LsError::new(format!("无法对 {} 取负", show(&other)))),
                    },
                }
            }

            Expr::ListLit(elems) => {
                let mut vals = Vec::with_capacity(elems.len());
                for e in elems {
                    vals.push(self.eval(e, env)?);
                }
                Ok(vec_to_list(vals))
            }

            Expr::TupleLit(elems) => {
                let mut vals = Vec::with_capacity(elems.len());
                for e in elems {
                    vals.push(self.eval(e, env)?);
                }
                Ok(Value::Tuple(Rc::new(vals)))
            }
        }
    }

    // ------------------------------------------------------------ 函数应用

    fn apply(&mut self, f: Value, arg: Value) -> Result<Value, LsError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            self.depth -= 1;
            return Err(LsError::new(format!(
                "调用层数超过上限 {}，可能是无限递归",
                self.max_depth
            )));
        }
        let r = self.apply_inner(f, arg);
        self.depth -= 1;
        r
    }

    fn apply_inner(&mut self, f: Value, arg: Value) -> Result<Value, LsError> {
        match f {
            Value::Closure(c) => {
                let mut applied = c.applied.clone();
                applied.push(arg);
                if applied.len() < c.params.len() {
                    return Ok(Value::Closure(Rc::new(ClosureData {
                        params: c.params.clone(),
                        body: c.body.clone(),
                        env: c.env.clone(),
                        applied,
                    })));
                }
                let pairs: Vec<(String, Value)> =
                    c.params.iter().cloned().zip(applied.into_iter()).collect();
                let env = env_extend(&c.env, pairs);
                let body = c.body.clone();
                self.eval(&body, &env)
            }

            Value::Builtin(name, arity, applied) => {
                let mut args = (*applied).clone();
                args.push(arg);
                if args.len() < arity {
                    return Ok(Value::Builtin(name, arity, Rc::new(args)));
                }
                let n = (*name).clone();
                self.call_builtin(&n, args)
            }

            Value::Ctor(name, args) => {
                let mut vals = (*args).clone();
                vals.push(arg);
                if let Some(&expected) = self.ctors.get(name.as_str()) {
                    if vals.len() > expected {
                        return Err(LsError::new(format!(
                            "构造子 `{}` 只接受 {} 个参数，却收到 {} 个",
                            name,
                            expected,
                            vals.len()
                        )));
                    }
                }
                Ok(Value::Ctor(name, Rc::new(vals)))
            }

            other => Err(LsError::new(format!("{} 不是函数，无法应用", show(&other)))),
        }
    }

    // ------------------------------------------------------------ 模式匹配

    fn match_pattern(
        &mut self,
        pat: &Pattern,
        val: &Value,
        binds: &mut Vec<(String, Value)>,
    ) -> Result<bool, LsError> {
        match pat {
            Pattern::Wild => Ok(true),
            Pattern::Var(n) => {
                binds.push((n.clone(), val.clone()));
                Ok(true)
            }
            Pattern::Lit(l) => Ok(lit_eq(l, val)),
            Pattern::Ctor(name, subs) => match val {
                Value::Ctor(vn, args) if vn.as_str() == name.as_str() => {
                    if subs.len() != args.len() {
                        return Ok(false);
                    }
                    for (p, a) in subs.iter().zip(args.iter()) {
                        if !self.match_pattern(p, a, binds)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            },
            Pattern::Tuple(subs) => match val {
                Value::Tuple(vals) => {
                    if subs.len() != vals.len() {
                        return Ok(false);
                    }
                    for (p, v) in subs.iter().zip(vals.iter()) {
                        if !self.match_pattern(p, v, binds)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            },
            Pattern::ListPat(subs) => match val {
                Value::List(l) => {
                    let vals = list_values(l);
                    if vals.len() != subs.len() {
                        return Ok(false);
                    }
                    for (p, v) in subs.iter().zip(vals.iter()) {
                        if !self.match_pattern(p, v, binds)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            },
            Pattern::Cons(head, tail) => match val {
                Value::List(l) => match &**l {
                    ListCell::Cons(h, t) => {
                        if !self.match_pattern(head, h, binds)? {
                            return Ok(false);
                        }
                        self.match_pattern(tail, &Value::List(t.clone()), binds)
                    }
                    ListCell::Nil => Ok(false),
                },
                _ => Ok(false),
            },
        }
    }

    // ------------------------------------------------------------ 运算符

    fn binop(&mut self, op: BinOp, a: Value, b: Value) -> Result<Value, LsError> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                self.arith(op, a, b)
            }
            BinOp::Eq => Ok(Value::Bool(value_eq(&a, &b))),
            BinOp::Ne => Ok(Value::Bool(!value_eq(&a, &b))),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let ord = match compare(&a, &b) {
                    Some(o) => o,
                    None => {
                        return Err(LsError::new(format!(
                            "无法比较 {} 和 {}",
                            show(&a),
                            show(&b)
                        )))
                    }
                };
                let res = match op {
                    BinOp::Lt => ord == Ordering::Less,
                    BinOp::Le => ord != Ordering::Greater,
                    BinOp::Gt => ord == Ordering::Greater,
                    _ => ord != Ordering::Less,
                };
                Ok(Value::Bool(res))
            }
            BinOp::Cons => match b {
                Value::List(tail) => Ok(Value::List(Rc::new(ListCell::Cons(a, tail)))),
                other => Err(LsError::new(format!(
                    "`::` 的右操作数必须是列表，得到 {}",
                    show(&other)
                ))),
            },
            BinOp::And | BinOp::Or => unreachable!("短路运算符在 eval 中处理"),
        }
    }

    fn arith(&mut self, op: BinOp, a: Value, b: Value) -> Result<Value, LsError> {
        if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
            let r = match op {
                BinOp::Add => x.checked_add(*y),
                BinOp::Sub => x.checked_sub(*y),
                BinOp::Mul => x.checked_mul(*y),
                BinOp::Div => {
                    if *y == 0 {
                        return Err(LsError::new("整数除以零"));
                    }
                    x.checked_div(*y)
                }
                BinOp::Mod => {
                    if *y == 0 {
                        return Err(LsError::new("整数对零取余"));
                    }
                    x.checked_rem(*y)
                }
                _ => None,
            };
            return r.map(Value::Int).ok_or_else(|| LsError::new("整数运算溢出"));
        }

        if op == BinOp::Add {
            if let (Value::Str(x), Value::Str(y)) = (&a, &b) {
                return Ok(Value::Str(Rc::new(format!("{}{}", x.as_str(), y.as_str()))));
            }
            if let (Value::List(x), Value::List(y)) = (&a, &b) {
                let mut vals = list_values(x);
                vals.extend(list_values(y));
                return Ok(vec_to_list(vals));
            }
        }

        let (x, y) = match (as_float(&a), as_float(&b)) {
            (Some(x), Some(y)) => (x, y),
            _ => {
                return Err(LsError::new(format!(
                    "运算符 `{}` 不支持 {} 和 {}",
                    op.symbol(),
                    show(&a),
                    show(&b)
                )))
            }
        };
        let r = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => x / y,
            _ => {
                return Err(LsError::new(format!(
                    "运算符 `{}` 不支持浮点数",
                    op.symbol()
                )))
            }
        };
        Ok(Value::Float(r))
    }

    // ------------------------------------------------------------ 内置函数

    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, LsError> {
        match name {
            "print" => {
                // 字符串按原文输出，其他值用 show 的表示形式
                let s = match &args[0] {
                    Value::Str(s) => (**s).clone(),
                    other => show(other),
                };
                self.output.push_str(&s);
                self.output.push('\n');
                Ok(Value::Unit)
            }
            "str" => Ok(Value::Str(Rc::new(show(&args[0])))),

            "len" => match &args[0] {
                Value::List(l) => Ok(Value::Int(list_values(l).len() as i64)),
                Value::Str(s) => Ok(Value::Int(s.chars().count() as i64)),
                Value::Tuple(t) => Ok(Value::Int(t.len() as i64)),
                other => Err(LsError::new(format!("len 不支持 {}", show(other)))),
            },

            "head" => match &args[0] {
                Value::List(l) => match &**l {
                    ListCell::Cons(h, _) => Ok(h.clone()),
                    ListCell::Nil => Err(LsError::new("head 不能用于空列表")),
                },
                other => Err(LsError::new(format!("head 需要列表，得到 {}", show(other)))),
            },

            "tail" => match &args[0] {
                Value::List(l) => match &**l {
                    ListCell::Cons(_, t) => Ok(Value::List(t.clone())),
                    ListCell::Nil => Err(LsError::new("tail 不能用于空列表")),
                },
                other => Err(LsError::new(format!("tail 需要列表，得到 {}", show(other)))),
            },

            "is_empty" => match &args[0] {
                Value::List(l) => Ok(Value::Bool(matches!(&**l, ListCell::Nil))),
                other => Err(LsError::new(format!(
                    "is_empty 需要列表，得到 {}",
                    show(other)
                ))),
            },

            "append" => match (&args[0], &args[1]) {
                (Value::List(x), Value::List(y)) => {
                    let mut vals = list_values(x);
                    vals.extend(list_values(y));
                    Ok(vec_to_list(vals))
                }
                (a, b) => Err(LsError::new(format!(
                    "append 需要两个列表，得到 {} 和 {}",
                    show(a),
                    show(b)
                ))),
            },

            "reverse" => {
                let mut vals = as_list(&args[0])?;
                vals.reverse();
                Ok(vec_to_list(vals))
            }

            "nth" => {
                let idx = as_int(&args[0])?;
                let vals = as_list(&args[1])?;
                if idx < 0 || idx as usize >= vals.len() {
                    return Err(LsError::new(format!(
                        "nth 下标 {} 越界，列表长度 {}",
                        idx,
                        vals.len()
                    )));
                }
                Ok(vals[idx as usize].clone())
            }

            "map" => {
                let f = args[0].clone();
                let vals = as_list(&args[1])?;
                let mut out = Vec::with_capacity(vals.len());
                for v in vals {
                    out.push(self.apply(f.clone(), v)?);
                }
                Ok(vec_to_list(out))
            }

            "filter" => {
                let f = args[0].clone();
                let vals = as_list(&args[1])?;
                let mut out = Vec::new();
                for v in vals {
                    let keep = self.apply(f.clone(), v.clone())?;
                    if as_bool(&keep)? {
                        out.push(v);
                    }
                }
                Ok(vec_to_list(out))
            }

            "foldl" => {
                let f = args[0].clone();
                let mut acc = args[1].clone();
                for v in as_list(&args[2])? {
                    let step = self.apply(f.clone(), acc)?;
                    acc = self.apply(step, v)?;
                }
                Ok(acc)
            }

            "foldr" => {
                let f = args[0].clone();
                let mut acc = args[1].clone();
                let mut vals = as_list(&args[2])?;
                vals.reverse();
                for v in vals {
                    let step = self.apply(f.clone(), v)?;
                    acc = self.apply(step, acc)?;
                }
                Ok(acc)
            }

            "range" => {
                let a = as_int(&args[0])?;
                let b = as_int(&args[1])?;
                let vals: Vec<Value> = if b <= a {
                    Vec::new()
                } else {
                    (a..b).map(Value::Int).collect()
                };
                Ok(vec_to_list(vals))
            }

            "abs" => match &args[0] {
                Value::Int(i) => Ok(Value::Int(i.wrapping_abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                other => Err(LsError::new(format!("abs 需要数值，得到 {}", show(other)))),
            },

            "min" | "max" => {
                let ord = compare(&args[0], &args[1]).ok_or_else(|| {
                    LsError::new(format!(
                        "{} 无法比较 {} 和 {}",
                        name,
                        show(&args[0]),
                        show(&args[1])
                    ))
                })?;
                let take_first = if name == "min" {
                    ord != Ordering::Greater
                } else {
                    ord != Ordering::Less
                };
                Ok(if take_first {
                    args[0].clone()
                } else {
                    args[1].clone()
                })
            }

            "sqrt" => {
                let x = as_float(&args[0])
                    .ok_or_else(|| LsError::new(format!("sqrt 需要数值，得到 {}", show(&args[0]))))?;
                if x < 0.0 {
                    return Err(LsError::new("sqrt 不能对负数开方"));
                }
                Ok(Value::Float(x.sqrt()))
            }

            "to_int" => match &args[0] {
                Value::Int(i) => Ok(Value::Int(*i)),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                Value::Char(c) => Ok(Value::Int(*c as u32 as i64)),
                Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
                Value::Str(s) => s
                    .trim()
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| LsError::new(format!("无法把字符串 {} 转成整数", show(&args[0])))),
                other => Err(LsError::new(format!("to_int 不支持 {}", show(other)))),
            },

            "to_float" => match &args[0] {
                Value::Float(f) => Ok(Value::Float(*f)),
                Value::Int(i) => Ok(Value::Float(*i as f64)),
                Value::Str(s) => s
                    .trim()
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| LsError::new(format!("无法把字符串 {} 转成浮点数", show(&args[0])))),
                other => Err(LsError::new(format!("to_float 不支持 {}", show(other)))),
            },

            "ord" => match &args[0] {
                Value::Char(c) => Ok(Value::Int(*c as u32 as i64)),
                other => Err(LsError::new(format!("ord 需要字符，得到 {}", show(other)))),
            },

            "chr" => {
                let n = as_int(&args[0])?;
                let c = u32::try_from(n)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| LsError::new(format!("{} 不是合法的字符码点", n)))?;
                Ok(Value::Char(c))
            }

            "error" => {
                let msg = match &args[0] {
                    Value::Str(s) => (**s).clone(),
                    other => show(other),
                };
                Err(LsError::new(msg))
            }

            _ => Err(LsError::new(format!("未知的内置函数 `{}`", name))),
        }
    }
}

// ---------------------------------------------------------------- 辅助函数

fn builtin_arity(name: &str) -> Option<usize> {
    Some(match name {
        "print" => 1,
        "str" => 1,
        "len" => 1,
        "head" => 1,
        "tail" => 1,
        "is_empty" => 1,
        "append" => 2,
        "reverse" => 1,
        "nth" => 2,
        "map" => 2,
        "filter" => 2,
        "foldl" => 3,
        "foldr" => 3,
        "range" => 2,
        "abs" => 1,
        "min" => 2,
        "max" => 2,
        "sqrt" => 1,
        "to_int" => 1,
        "to_float" => 1,
        "ord" => 1,
        "chr" => 1,
        "error" => 1,
        _ => return None,
    })
}

fn type_err(op: &str, got: &Value) -> LsError {
    LsError::new(format!("`{}` 需要 Bool，得到 {}", op, show(got)))
}

fn as_bool(v: &Value) -> Result<bool, LsError> {
    match v {
        Value::Bool(b) => Ok(*b),
        other => Err(LsError::new(format!("这里需要 Bool，得到 {}", show(other)))),
    }
}

fn as_int(v: &Value) -> Result<i64, LsError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(LsError::new(format!("这里需要 Int，得到 {}", show(other)))),
    }
}

fn as_float(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn as_list(v: &Value) -> Result<Vec<Value>, LsError> {
    match v {
        Value::List(l) => Ok(list_values(l)),
        other => Err(LsError::new(format!("这里需要列表，得到 {}", show(other)))),
    }
}

pub fn list_values(l: &Rc<ListCell>) -> Vec<Value> {
    let mut out = Vec::new();
    let mut cur = l.clone();
    loop {
        match &*cur {
            ListCell::Nil => break,
            ListCell::Cons(h, t) => {
                out.push(h.clone());
                let next = t.clone();
                cur = next;
            }
        }
    }
    out
}

fn vec_to_list(vals: Vec<Value>) -> Value {
    let mut cell = Rc::new(ListCell::Nil);
    for v in vals.into_iter().rev() {
        cell = Rc::new(ListCell::Cons(v, cell));
    }
    Value::List(cell)
}

fn lit_value(l: &Lit) -> Value {
    match l {
        Lit::Int(i) => Value::Int(*i),
        Lit::Float(f) => Value::Float(*f),
        Lit::Bool(b) => Value::Bool(*b),
        Lit::Char(c) => Value::Char(*c),
        Lit::Str(s) => Value::Str(Rc::new(s.clone())),
        Lit::Unit => Value::Unit,
    }
}

fn lit_eq(l: &Lit, v: &Value) -> bool {
    match (l, v) {
        (Lit::Int(i), Value::Int(j)) => i == j,
        (Lit::Int(i), Value::Float(y)) => (*i as f64) == *y,
        (Lit::Float(x), Value::Float(y)) => x == y,
        (Lit::Bool(a), Value::Bool(b)) => a == b,
        (Lit::Char(a), Value::Char(b)) => a == b,
        (Lit::Str(a), Value::Str(b)) => a.as_str() == b.as_str(),
        (Lit::Unit, Value::Unit) => true,
        _ => false,
    }
}

fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Char(x), Value::Char(y)) => Some(x.cmp(y)),
        (Value::Str(x), Value::Str(y)) => Some(x.as_str().cmp(y.as_str())),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => {
            let x = as_float(a)?;
            let y = as_float(b)?;
            x.partial_cmp(&y)
        }
    }
}

pub fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) => (*x as f64) == *y,
        (Value::Float(x), Value::Int(y)) => *x == (*y as f64),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x.as_str() == y.as_str(),
        (Value::Unit, Value::Unit) => true,
        (Value::Tuple(x), Value::Tuple(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| value_eq(a, b))
        }
        (Value::List(x), Value::List(y)) => {
            let mut p = x.clone();
            let mut q = y.clone();
            loop {
                match (&*p, &*q) {
                    (ListCell::Nil, ListCell::Nil) => return true,
                    (ListCell::Cons(h1, t1), ListCell::Cons(h2, t2)) => {
                        if !value_eq(h1, h2) {
                            return false;
                        }
                        let (np, nq) = (t1.clone(), t2.clone());
                        p = np;
                        q = nq;
                    }
                    _ => return false,
                }
            }
        }
        (Value::Ctor(n1, a1), Value::Ctor(n2, a2)) => {
            n1.as_str() == n2.as_str()
                && a1.len() == a2.len()
                && a1.iter().zip(a2.iter()).all(|(x, y)| value_eq(x, y))
        }
        _ => false,
    }
}

pub fn show(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_finite() && f.fract() == 0.0 {
                format!("{:.1}", f)
            } else {
                format!("{}", f)
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => format!("'{}'", escape_char(*c)),
        Value::Str(s) => format!("\"{}\"", escape_str(s)),
        Value::Unit => "()".to_string(),
        Value::Tuple(xs) => {
            format!("({})", xs.iter().map(show).collect::<Vec<_>>().join(", "))
        }
        Value::List(l) => {
            let vals = list_values(l);
            format!("[{}]", vals.iter().map(show).collect::<Vec<_>>().join(", "))
        }
        Value::Ctor(n, args) => {
            if args.is_empty() {
                n.to_string()
            } else {
                format!(
                    "{}({})",
                    n,
                    args.iter().map(show).collect::<Vec<_>>().join(", ")
                )
            }
        }
        Value::Closure(_) => "<函数>".to_string(),
        Value::Builtin(n, _, _) => format!("<内置 {}>", n),
    }
}

fn escape_char(c: char) -> String {
    match c {
        '\n' => "\\n".to_string(),
        '\t' => "\\t".to_string(),
        '\r' => "\\r".to_string(),
        '\'' => "\\'".to_string(),
        '\\' => "\\\\".to_string(),
        other => other.to_string(),
    }
}

fn escape_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out
}
