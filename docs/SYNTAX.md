# L# 语法设计文档 v0.1

L# 是一门**纯函数式**的轻量语言：没有可变变量、没有循环、没有赋值，一切计算都由表达式与递归完成。
本文件是设计稿，先定语法与语义，实现（`src/` 下的 Rust 解释器）严格按本文件落地。

## 1. 设计目标

| 目标 | 做法 |
| --- | --- |
| 足够小 | 只有 5 个关键字组：`data`、`let`、`if`、`match`、匿名函数 `\` |
| 纯函数式 | 值不可变；函数是一等公民；柯里化；递归代替循环 |
| 有代数数据类型 | `data` 声明构造子，`match` 做模式匹配 |
| 好写 | 表达式导向，`let ... in ...`，模式匹配带列表 / 元组 / 构造子模式 |
| 可解释 | Rust 实现的树遍历解释器，无外部依赖 |

## 2. 词法

```
注释      -- 行注释          {- 块注释 -}（不支持嵌套）
整数      0 | 42 | -7 | 0xFF          （负号是前缀运算符，不是数字的一部分）
浮点      3.14 | 2.0
字符串    "hello\n\"L#\""             （转义：\n \t \r \0 \\ \" \'）
字符      'a' | '\n'
布尔      true | false
小写标识  变量名 / 函数名 / 类型变量     [a-z_][A-Za-z0-9_]*
大写标识  构造子名 / 类型名             [A-Z][A-Za-z0-9_]*
关键字    data let in if then else match with rec true false
符号      ( ) [ ] , = | \ -> :: == /= < <= > >= && || + - * / % ! .
```

## 3. 语法（EBNF）

```ebnf
program     = { decl } ;
decl        = dataDecl | letDecl | expr ;          (* 顶层裸表达式：求值后丢弃结果 *)

dataDecl    = "data" Upper { typeVar } "=" ctor { "|" ctor } ;
ctor        = Upper { typeAtom } ;                 (* typeAtom 个数即构造子元数 *)
typeAtom    = Upper | typeVar | "(" ... ")" | "[" ... "]" ;

letDecl     = "let" [ "rec" ] lower { lower } "=" expr ;
                                                    (* 无参数 = 常量绑定；有参数 = 函数定义 *)

expr        = ifExpr | matchExpr | letExpr | lambda | binExpr ;

ifExpr      = "if" expr "then" expr "else" expr ;
matchExpr   = "match" expr "with" [ "|" ] arm { "|" arm } ;
arm         = pattern "->" expr ;
letExpr     = "let" [ "rec" ] lower { lower } "=" expr "in" expr ;
lambda      = "\" lower { lower } "->" expr ;

(* 优先级由低到高 *)
binExpr     = or ;
or          = and { "||" and } ;
and         = cmp { "&&" cmp } ;
cmp         = cons { ( "==" | "/=" | "<" | "<=" | ">" | ">=" ) cons } ;
cons        = add { "::" cons } ;                  (* 右结合 *)
add         = mul { ( "+" | "-" ) mul } ;
mul         = unary { ( "*" | "/" | "%" ) unary } ;
unary       = ( "-" | "!" ) unary | app ;
app         = atom { atom } ;                      (* 左结合，函数应用优先级最高 *)
atom        = Int | Float | Str | Char | true | false
            | lower | Upper
            | "(" ")"                              (* unit *)
            | "(" expr ")"
            | "(" expr "," { expr "," } ")"        (* 元组 *)
            | "[" [ expr { "," expr } ] "]" ;      (* 列表 *)

pattern     = patAtom [ "::" pattern ] ;           (* 右结合 *)
patAtom     = Int | Float | Str | Char | true | false
            | "_"                                   (* 通配 *)
            | lower                                 (* 变量绑定 *)
            | Upper { patAtom }                     (* 构造子模式 *)
            | "(" ")" | "(" pattern "," ... ")"     (* 单元组 / 元组模式 *)
            | "[" [ pattern { "," pattern } ] "]" ; (* 列表模式 *)
```

要点：

- **函数应用**靠空格并置（`f x y`），优先级最高；`f -1` 会被读成 `f - 1`，取负数请写 `f (-1)`。
- **换行是“表达式结束”的信号**：函数应用只在一行内继续（`f` 换行再写 `x` 不会应用），
  二元运算可以续到下一行（把运算符写在行首）。所以块结构靠关键字与 `|` 分界，不靠缩进。
- **作用域**：`let x = e in body`；顶层 `let` 全部互相可见（函数之间可互相递归调用，顺序无关）。
- `rec` 只对“带参数的局部 `let`”有意义（闭包自引用），顶层函数天然可递归。
- 构造子的元数来自 `data` 声明，解释器据此检查参数个数。

## 4. 语义

### 4.1 求值

- **严格求值（call-by-value）**，从左到右；唯一例外是 `&&`、`||` 的短路。
- **柯里化**：`let add a b = a + b` 等价于 `\a -> \b -> a + b`；`add 1` 得到新函数，缺参时返回部分应用闭包。
- **不可变**：没有赋值、没有 `var`、没有循环；列表 `x :: xs` 生成新列表，原列表不变。
- **递归**：唯一的重复手段；解释器有调用深度上限（默认 1000）防止栈溢出。

### 4.2 模式匹配

`match` 自上而下尝试分支，第一个匹配成功者胜出；未匹配任何分支是运行时错误。

- 字面量模式按值比较（`Int` 与 `Float` 之间做数值比较）。
- 变量模式总会匹配，并绑定该名字。
- 构造子模式要求名字相同且参数个数相同。
- 列表模式 `[a, b]` 按长度精确匹配；`x :: xs` 匹配非空列表，`[]` 匹配空列表。
- 元组模式按元数匹配。

### 4.3 运算

| 运算 | Int | Float | 其他 |
| --- | --- | --- | --- |
| `+` | 整数加（溢出报错） | 浮点加 | `Str + Str`（字符串拼接）、`List + List`（列表拼接） |
| `-` `*` | 整数运算 | 浮点运算 | — |
| `/` | 整除（向零取整，除以 0 报错） | 浮点除 | — |
| `%` | 整数取余 | — | — |
| `==` `/=` | 结构相等 | 结构相等 | 列表、元组、构造子递归比较 |
| `<` `<=` `>` `>=` | 整数比较 | 浮点比较 | `Char`、`Str` 字典序 |
| `&&` `||` `!` | — | — | 仅 `Bool`，短路求值 |
| `::` | — | — | 右操作数必须是列表 |

`Int` 与 `Float` 混算：只要有一边是浮点，整体按浮点算。

### 4.4 唯一的效果

纯函数式语言不应有副作用。L# 只保留一个显式的输出原语 `print`（把值写到 stdout 并换行，返回 `()`），
除此之外没有任何 I/O：没有文件、没有环境变量、没有可变引用。`print` 的效果按求值顺序发生，
因此“纯计算 + 末端一次输出”是推荐写法。

## 5. 内置函数

| 名字 | 类型（直觉） | 说明 |
| --- | --- | --- |
| `print` | `a -> ()` | 打印并换行；字符串按原文输出，其他值用 `show` 的表示形式 |
| `str` | `a -> Str` | 值的字符串表示 |
| `len` | `List a -> Int` / `Str -> Int` | 长度 |
| `head` / `tail` | `List a -> a` / `List a -> List a` | 空列表报错 |
| `is_empty` | `List a -> Bool` | 是否空 |
| `append` | `List a -> List a -> List a` | 拼接 |
| `reverse` | `List a -> List a` | 反转 |
| `nth` | `Int -> List a -> a` | 下标，越界报错 |
| `map` / `filter` | `(a -> b) -> List a -> List b` | 高阶 |
| `foldl` / `foldr` | `(b -> a -> b) -> b -> List a -> b` | 折叠 |
| `range` | `Int -> Int -> List Int` | 左闭右开 |
| `abs` `min` `max` `sqrt` | 数值 | 数学 |
| `to_int` `to_float` `ord` `chr` | 转换 | `ord`/`chr` 处理字符码点 |
| `error` | `Str -> a` | 抛出运行时错误 |

标准库 `src/prelude.lsh` 用 L# 自己再定义一层便利函数：`id`、`const`、`flip`、`compose`、`not`、
`sum`、`product`、`length`、`all`、`any`、`count`、`take`、`drop`、`zipWith`。

## 6. 设计取舍

- **不做静态类型系统**：放弃 Hindley–Milner，换取解释器小、错误信息直观；`data` 只为模式匹配提供构造子元数。
- **不做惰性求值**：严格求值让求值顺序与输出顺序可预期，代价是无法表达无限结构。
- **不做模块系统**：一个文件就是一段程序，`--eval` 与 REPL 共用同一套顶层环境。
- **错误带位置**：词法/语法错误带行列号；运行时错误只给消息（没有求值栈，保持实现简单）。
- **命名**：语言名 `L#`，文件扩展名 `.lsh`，CLI 与 crate 名 `lsharp`（`#` 不适合出现在可执行文件名中）。
