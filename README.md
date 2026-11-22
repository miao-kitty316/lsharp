# L# · 一门纯函数式的轻量语言

L# 是一个玩具级但完整的**纯函数式**语言：没有可变变量、没有循环、没有赋值，一切计算都由表达式、递归与高阶函数完成。
本仓库包含语言设计文档、Rust 实现的解释器、标准库与示例程序。

- **纯函数式**：值不可变，函数是一等公民，靠递归代替循环
- **代数数据类型**：`data` 声明构造子，`match` 做模式匹配（字面量 / 构造子 / 列表 / 元组 / `_`）
- **自动柯里化**：`add 1` 就是“少一个参数的函数”
- **零依赖**：解释器只用 Rust 标准库，`cargo build` 即可运行
- **语法先定稿**：见 [`docs/SYNTAX.md`](docs/SYNTAX.md)，实现严格按设计稿落地

## 快速开始

```bash
cargo build --release

cargo run -- examples/hello.lsh      # 运行示例
cargo run -- -e 'print (sum (range 1 11))'
cargo run                            # 交互式 REPL（:help / :quit）
cargo test                           # 16 个集成测试
```

`examples/hello.lsh` 的输出：

```
你好，L#！
```

## 语法一晤

```lsharp
-- 函数定义，自动柯里化
let add a b = a + b
let inc = add 1

-- 代数数据类型
data Maybe a = None | Some a

let fromMaybe d m = match m with
  | None -> d
  | Some x -> x

-- 列表与模式匹配
let describe xs = match xs with
  | [] -> "空"
  | [x] -> "一个元素"
  | x :: rest -> "多于一个"

-- 高阶函数与匿名函数
let total = foldl (\acc x -> acc + x) 0 (range 1 11)

-- 递归代替循环
let factorial n = if n == 0 then 1 else n * factorial (n - 1)

print (inc 41)                      -- 42
print (fromMaybe 0 (Some 42))       -- 42
print (describe [1, 2, 3])          -- 多于一个
print total                         -- 55
print (factorial 10)                -- 3628800
```

## 目录结构

```
.
├── Cargo.toml
├── docs/SYNTAX.md        语言设计稿：词法、EBNF、语义、内置函数、设计取舍
├── src/
│   ├── lexer.rs          词法分析（含 -- 与 {- -} 注释、字符串/字符转义）
│   ├── parser.rs         递归下降 + 优先级爬升，产出 AST
│   ├── ast.rs            语法树与声明
│   ├── eval.rs           求值器：环境链、闭包、柯里化、模式匹配、内置函数
│   ├── prelude.lsh       用 L# 写的标准库
│   ├── lib.rs            公共接口（new_engine / run）
│   └── main.rs           CLI：文件 / -e / REPL
├── examples/             5 个示例程序
└── tests/interp.rs       16 个集成测试
```

## 内置函数

`print` `str` `len` `head` `tail` `is_empty` `append` `reverse` `nth`
`map` `filter` `foldl` `foldr` `range`
`abs` `min` `max` `sqrt` `to_int` `to_float` `ord` `chr` `error`

标准库里再用 L# 自己定义：`id` `const` `flip` `compose` `not` `negate`
`sum` `product` `length` `any` `all` `count` `concat` `take` `drop` `elem` `zipWith`

## 几个约定

- **唯一的效果是 `print`**：除此之外没有任何 I/O，也没有可变状态。
- **换行即表达式结束**：函数应用不跨行；需要续行时把运算符写在行首，或把参数放进括号。
- **顶层绑定互相可见**：带参数的顶层 `let` 是闭包，所以函数可以互相调用（也就可以互相递归）。
- **递归深度上限 1000**：超过后报错，避免把栈打穿；解释器本身在 64MB 栈的线程里运行。
- **不做静态类型检查**：`data` 只为模式匹配提供构造子与元数信息。

## 许可

MIT，见 [LICENSE](LICENSE)。
