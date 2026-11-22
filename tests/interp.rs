//! L# 解释器的集成测试。

use lsharp::run;

fn out(src: &str) -> String {
    run(src).expect("程序应当正常运行").trim_end().to_string()
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(out("print (1 + 2 * 3)"), "7");
    assert_eq!(out("print ((1 + 2) * 3)"), "9");
    assert_eq!(out("print (10 / 3)"), "3");
    assert_eq!(out("print (7 % 4)"), "3");
    assert_eq!(out("print (1.5 + 2)"), "3.5");
    assert_eq!(out("print (0xff)"), "255");
    assert_eq!(out("print (-(3 + 4))"), "-7");
}

#[test]
fn booleans_and_if() {
    assert_eq!(out("print (1 < 2 && 3 >= 3)"), "true");
    assert_eq!(out("print (if 5 > 2 then \"是\" else \"否\")"), "是");
    // 短路：右边若被求值会报错
    assert_eq!(out("print (false && error \"不该求值\")"), "false");
}

#[test]
fn recursion() {
    let src = "let fact n = if n == 0 then 1 else n * fact (n - 1)\nprint (fact 6)";
    assert_eq!(out(src), "720");
}

#[test]
fn currying_and_partial_application() {
    let src = "let add a b = a + b\nlet inc = add 1\nprint (inc 41)";
    assert_eq!(out(src), "42");
}

#[test]
fn closures_capture_environment() {
    let src = "let adder n = \\x -> x + n\nlet add5 = adder 5\nprint (add5 10)";
    assert_eq!(out(src), "15");
}

#[test]
fn local_let_and_recursion() {
    let src = "let f n = let rec go acc i = if i == 0 then acc else go (acc + i) (i - 1) in go 0 n\nprint (f 10)";
    assert_eq!(out(src), "55");
}

#[test]
fn algebraic_data_and_matching() {
    let src = "\
data Maybe a = None | Some a\nlet fromMaybe d m = match m with | None -> d | Some x -> x\nprint (fromMaybe 0 (Some 42))\nprint (fromMaybe 7 None)";
    assert_eq!(out(src), "42\n7");
}

#[test]
fn list_literals_and_patterns() {
    let src = "\
let describe xs = match xs with | [] -> \"空\" | [x] -> \"一个元素\" | x :: rest -> \"多于一个\"\nprint (describe [])\nprint (describe [1])\nprint (describe [1, 2, 3])";
    assert_eq!(out(src), "空\n一个元素\n多于一个");
}

#[test]
fn tuples_and_equality() {
    let src = "let pair = (1, \"a\")\nprint (pair == (1, \"a\"))\nprint (pair)";
    assert_eq!(out(src), "true\n(1, \"a\")");
}

#[test]
fn higher_order_builtins() {
    let src = "\
print (map (\\x -> x * 2) [1, 2, 3])\nprint (filter (\\x -> x > 2) [1, 2, 3, 4])\nprint (foldl (\\acc x -> acc + x) 0 [1, 2, 3, 4])\nprint (foldr (\\x acc -> x :: acc) [] [1, 2, 3])\nprint (range 0 4)\nprint (append [1, 2] [3])";
    assert_eq!(
        out(src),
        "[2, 4, 6]\n[3, 4]\n10\n[1, 2, 3]\n[0, 1, 2, 3]\n[1, 2, 3]"
    );
}

#[test]
fn prelude_helpers() {
    let src = "\
print (sum [1, 2, 3])\nprint (take 2 [1, 2, 3])\nprint (drop 2 [1, 2, 3])\nprint (length [4, 5])\nprint (count (\\x -> x > 2) [1, 3, 4])\nprint (any (\\x -> x == 9) [1, 9])\nprint (all (\\x -> x > 0) [1, 2])\nprint (zipWith (\\a b -> a + b) [1, 2] [10, 20])";
    assert_eq!(out(src), "6\n[1, 2]\n[3]\n2\n2\ntrue\ntrue\n[11, 22]");
}

#[test]
fn strings_and_chars() {
    let src = "\
print (\"L\" + \"#\")\nprint (len \"abc\")\nprint (ord 'A')\nprint (chr 66)\nprint (str [1, 2])";
    assert_eq!(out(src), "L#\n3\n65\n'B'\n[1, 2]");
}

#[test]
fn same_name_functions_are_mutually_visible() {
    let src = "\
let isEven n = if n == 0 then true else isOdd (n - 1)\nlet isOdd n = if n == 0 then false else isEven (n - 1)\nprint (isEven 10)";
    assert_eq!(out(src), "true");
}

#[test]
fn runtime_errors() {
    assert!(run("print (1 + \"a\")").is_err());
    assert!(run("print (head [])").is_err());
    assert!(run("print (1 / 0)").is_err());
    assert!(run("print (match 3 with | 4 -> 0)").is_err());
    assert!(run("print (undefined_name)").is_err());
}

#[test]
fn parse_errors() {
    assert!(run("let f = (1 + )").is_err());
    assert!(run("print (1 +").is_err());
    assert!(run("data Maybe a = None | a").is_err());
}

#[test]
fn recursion_limit_is_reported() {
    assert!(run("let loop n = loop (n + 1)\nprint (loop 0)").is_err());
}
