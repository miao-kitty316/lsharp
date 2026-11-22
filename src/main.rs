//! L# 的命令行入口：文件运行 / `-e` 单段执行 / 交互式 REPL。

use std::io::{self, BufRead, Write};

use lsharp::Interp;

/// 求值器是深递归的，主线程默认栈只有 8MB，这里换一个更大的栈来跑。
const STACK_SIZE: usize = 64 * 1024 * 1024;

fn main() {
    let handle = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(run_main)
        .expect("无法创建解释器线程");
    let code = handle.join().unwrap_or(1);
    std::process::exit(code);
}

fn run_main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut interp = match lsharp::new_engine() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("加载标准库失败：{}", e);
            return 1;
        }
    };

    if args.is_empty() {
        repl(&mut interp);
        return 0;
    }

    match args[0].as_str() {
        "-h" | "--help" => {
            print_help();
            0
        }
        "-r" | "--repl" => {
            repl(&mut interp);
            0
        }
        "-e" | "--eval" => match args.get(1) {
            Some(src) => {
                if run_source(&mut interp, src) {
                    0
                } else {
                    1
                }
            }
            None => {
                eprintln!("-e/--eval 需要一个字符串参数");
                2
            }
        },
        flag if flag.starts_with('-') => {
            eprintln!("未知选项 `{}`，用 --help 查看用法", flag);
            2
        }
        path => {
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("无法读取文件 {}：{}", path, e);
                    return 2;
                }
            };
            if run_source(&mut interp, &src) {
                0
            } else {
                1
            }
        }
    }
}

fn run_source(interp: &mut Interp, src: &str) -> bool {
    let ok = match interp.load_source(src) {
        Ok(()) => true,
        Err(e) => {
            flush_output(interp);
            eprintln!("错误：{}", e);
            false
        }
    };
    flush_output(interp);
    ok
}

fn flush_output(interp: &mut Interp) {
    let out = interp.take_output();
    if !out.is_empty() {
        print!("{}", out);
        let _ = io::stdout().flush();
    }
}

fn repl(interp: &mut Interp) {
    println!("L# REPL — :help 查看帮助，:quit 退出（Ctrl-D 亦可）");
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut buf = String::new();

    loop {
        print!("{}", if buf.is_empty() { "lsh> " } else { "  .. " });
        let _ = io::stdout().flush();

        let mut line = String::new();
        match input.read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }

        if buf.is_empty() {
            match line.trim() {
                ":quit" | ":q" => break,
                ":help" => {
                    print_help();
                    continue;
                }
                "" => continue,
                _ => {}
            }
        }

        buf.push_str(&line);

        match interp.load_source(&buf) {
            Ok(()) => {
                flush_output(interp);
                buf.clear();
            }
            // 还没写完（括号、字符串、表达式没结束），继续读下一行
            Err(e) if e.at_eof => {}
            Err(e) => {
                flush_output(interp);
                eprintln!("错误：{}", e);
                buf.clear();
            }
        }
    }
}

fn print_help() {
    println!(
        "L# — 一门纯函数式的轻量语言

用法：
  lsharp <文件.lsh>        运行一个 L# 源文件
  lsharp -e \"<代码>\"      直接执行一段代码
  lsharp                   进入交互式 REPL
  lsharp -h | --help       显示本帮助

示例：
  lsharp examples/hello.lsh
  lsharp -e 'print (sum (range 1 11))'

语法速查：
  let f x = x + 1          函数定义（自动柯里化）
  let v = [1, 2, 3]        列表
  data Maybe a = None | Some a
  match v with | [] -> 0 | x :: xs -> x
  \\x -> x * 2             匿名函数
  if x > 0 then 1 else 0

完整语法见 docs/SYNTAX.md"
    );
}
