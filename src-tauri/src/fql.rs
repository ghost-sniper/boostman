//! A deliberately bounded FQL subset. This is not a JavaScript evaluator.
use serde_json::{Map, Value, json};
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Name(String),
    Lit(Value),
    Sym(String),
    End,
}
#[derive(Clone, Debug)]
enum Expr {
    Lit(Value),
    Name(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Field(Box<Expr>, String),
    Select(Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Lambda(Vec<String>, Box<Expr>),
}
struct Parser {
    tokens: Vec<(Tok, usize)>,
    at: usize,
}
fn lex(s: &str) -> Result<Vec<(Tok, usize)>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        if c[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let t = if c[i] == '"' || c[i] == '\'' {
            let q = c[i];
            i += 1;
            let mut v = String::new();
            let mut closed = false;
            while i < c.len() {
                let ch = c[i];
                i += 1;
                if ch == q {
                    closed = true;
                    break;
                }
                if ch == '\\' {
                    if i >= c.len() {
                        break;
                    }
                    v.push(match c[i] {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        x => x,
                    });
                    i += 1;
                } else {
                    v.push(ch)
                }
            }
            if !closed {
                return Err(format!("位置 {start}: 字符串未闭合"));
            }
            Tok::Lit(json!(v))
        } else if c[i].is_ascii_digit() {
            i += 1;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1
            }
            let n: String = c[start..i].iter().collect();
            Tok::Lit(serde_json::from_str(&n).map_err(|_| format!("位置 {start}: 无效数字"))?)
        } else if c[i].is_alphabetic() || c[i] == '_' || c[i] == '$' {
            i += 1;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$') {
                i += 1
            }
            let n: String = c[start..i].iter().collect();
            match n.as_str() {
                "true" => Tok::Lit(json!(true)),
                "false" => Tok::Lit(json!(false)),
                "null" => Tok::Lit(Value::Null),
                "and" | "or" | "not" => Tok::Sym(n),
                _ => Tok::Name(n),
            }
        } else {
            let mut op = c[i].to_string();
            i += 1;
            if i < c.len()
                && ["==", "!=", "<=", ">=", "&&", "||"]
                    .contains(&format!("{}{}", op, c[i]).as_str())
            {
                op.push(c[i]);
                i += 1;
            }
            if ![
                "+", "-", "*", "/", "%", "=", "==", "!=", "<", ">", "<=", ">=", "&&", "||", "!",
                ".", "[", "]", "{", "}", "(", ")", ",", ":", "?", "&",
            ]
            .contains(&op.as_str())
            {
                return Err(format!("位置 {start}: 不支持的符号 {op}"));
            }
            Tok::Sym(op)
        };
        out.push((t, start));
    }
    out.push((Tok::End, c.len()));
    Ok(out)
}
impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.at].0
    }
    fn err(&self, s: &str) -> String {
        format!("位置 {}: {s}", self.tokens[self.at].1)
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.peek() == &Tok::Sym(s.into()) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn need(&mut self, s: &str) -> Result<(), String> {
        if self.eat(s) {
            Ok(())
        } else {
            Err(self.err(&format!("需要 {s}")))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        if let Tok::Name(s) = self.peek().clone() {
            self.at += 1;
            Ok(s)
        } else {
            Err(self.err("需要名称"))
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        if self.at > 5120 {
            return Err(self.err("表达式过长"));
        }
        let mut left = match self.peek().clone() {
            Tok::Lit(v) => {
                self.at += 1;
                Expr::Lit(v)
            }
            Tok::Name(n) => {
                self.at += 1;
                if n == "fn" {
                    self.need("(")?;
                    let mut args = vec![];
                    if !self.eat(")") {
                        loop {
                            args.push(self.name()?);
                            if self.eat(")") {
                                break;
                            }
                            self.need(",")?;
                        }
                    }
                    self.need("{")?;
                    let e = self.expr(0)?;
                    self.need("}")?;
                    Expr::Lambda(args, Box::new(e))
                } else if self.eat("(") {
                    let mut args = vec![];
                    if !self.eat(")") {
                        loop {
                            args.push(self.expr(0)?);
                            if self.eat(")") {
                                break;
                            }
                            self.need(",")?;
                        }
                    }
                    Expr::Call(n, args)
                } else {
                    Expr::Name(n)
                }
            }
            Tok::Sym(ref s) if s == "-" || s == "!" || s == "not" => {
                let s = s.clone();
                self.at += 1;
                Expr::Unary(s, Box::new(self.expr(8)?))
            }
            Tok::Sym(ref s) if s == "(" => {
                self.at += 1;
                let e = self.expr(0)?;
                self.need(")")?;
                e
            }
            Tok::Sym(ref s) if s == "[" => {
                self.at += 1;
                let mut a = vec![];
                if !self.eat("]") {
                    loop {
                        a.push(self.expr(0)?);
                        if self.eat("]") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                Expr::Array(a)
            }
            Tok::Sym(ref s) if s == "{" => {
                self.at += 1;
                let mut a = vec![];
                if !self.eat("}") {
                    loop {
                        let k = match self.peek().clone() {
                            Tok::Lit(Value::String(k)) | Tok::Name(k) => {
                                self.at += 1;
                                k
                            }
                            _ => return Err(self.err("需要对象字段名")),
                        };
                        self.need(":")?;
                        a.push((k, self.expr(0)?));
                        if self.eat("}") {
                            break;
                        }
                        self.need(",")?;
                    }
                }
                Expr::Object(a)
            }
            _ => return Err(self.err("需要表达式")),
        };
        loop {
            if self.peek() == &Tok::Sym(".".into()) {
                self.at += 1;
                left = Expr::Field(Box::new(left), self.name()?);
                continue;
            }
            if self.eat("[") {
                let filter = self.expr(0)?;
                self.need("]")?;
                left = Expr::Select(Box::new(left), Box::new(filter));
                continue;
            }
            if min == 0 && self.eat("?") {
                let yes = self.expr(0)?;
                self.need(":")?;
                left = Expr::If(Box::new(left), Box::new(yes), Box::new(self.expr(0)?));
                continue;
            }
            let op = if let Tok::Sym(s) = self.peek() {
                s.clone()
            } else {
                break;
            };
            let p = match op.as_str() {
                "or" | "||" => 1,
                "and" | "&&" => 2,
                "=" | "==" | "!=" => 3,
                "<" | ">" | "<=" | ">=" => 4,
                "+" | "-" | "&" => 5,
                "*" | "/" | "%" => 6,
                _ => break,
            };
            if p < min {
                break;
            }
            self.at += 1;
            let right = self.expr(p + 1)?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}
pub fn text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        s.to_owned()
    } else {
        v.to_string()
    }
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| "需要数字".into())
}
pub fn boolean(v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| "条件表达式必须返回布尔值".into())
}
fn field(v: &Value, k: &str) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(|v| field(v, k)).collect()),
        _ => v.get(k).cloned().unwrap_or(Value::Null),
    }
}
fn eval(
    e: &Expr,
    root: &Value,
    vars: &Map<String, Value>,
    budget: &mut usize,
) -> Result<Value, String> {
    if *budget == 0 {
        return Err("表达式执行步数超过上限".into());
    }
    *budget -= 1;
    let mut ev = |e: &Expr| eval(e, root, vars, budget);
    Ok(match e {
        Expr::Lit(v) => v.clone(),
        Expr::Name(n) => {
            if n == "$" {
                root.clone()
            } else {
                vars.get(n).cloned().unwrap_or_else(|| field(root, n))
            }
        }
        Expr::Array(a) => Value::Array(a.iter().map(&mut ev).collect::<Result<_, _>>()?),
        Expr::Object(a) => Value::Object(
            a.iter()
                .map(|(k, e)| Ok((k.clone(), ev(e)?)))
                .collect::<Result<_, String>>()?,
        ),
        Expr::Field(a, k) => field(&ev(a)?, k),
        Expr::Select(a, b) => {
            let a = ev(a)?;
            let list = a.as_array().ok_or("过滤/索引目标必须为数组")?;
            if let Expr::Lit(Value::Number(n)) = b.as_ref() {
                list.get(n.as_u64().ok_or("索引必须为非负整数")? as usize)
                    .cloned()
                    .unwrap_or(Value::Null)
            } else {
                let mut out = vec![];
                for item in list {
                    if boolean(&eval(b, item, vars, budget)?)? {
                        out.push(item.clone());
                    }
                }
                json!(out)
            }
        }
        Expr::Unary(op, a) => {
            let a = ev(a)?;
            if op == "-" {
                json!(-number(&a)?)
            } else {
                json!(!boolean(&a)?)
            }
        }
        Expr::If(c, a, b) => {
            if boolean(&ev(c)?)? {
                ev(a)?
            } else {
                ev(b)?
            }
        }
        Expr::Binary(op, a, b) => {
            let a = ev(a)?;
            if op == "and" || op == "&&" {
                if !boolean(&a)? {
                    json!(false)
                } else {
                    json!(boolean(&ev(b)?)?)
                }
            } else if op == "or" || op == "||" {
                if boolean(&a)? {
                    json!(true)
                } else {
                    json!(boolean(&ev(b)?)?)
                }
            } else {
                let b = ev(b)?;
                match op.as_str() {
                    "=" | "==" => json!(a == b),
                    "!=" => json!(a != b),
                    "&" => json!(format!("{}{}", text(&a), text(&b))),
                    "+" => json!(number(&a)? + number(&b)?),
                    "-" => json!(number(&a)? - number(&b)?),
                    "*" => json!(number(&a)? * number(&b)?),
                    "/" | "%" => {
                        let n = number(&b)?;
                        if n == 0.0 {
                            return Err("除数不能为零".into());
                        }
                        if op == "/" {
                            json!(number(&a)? / n)
                        } else {
                            json!(number(&a)? % n)
                        }
                    }
                    _ => {
                        let order = if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
                            a.partial_cmp(b)
                        } else {
                            number(&a)?.partial_cmp(&number(&b)?)
                        };
                        json!(match op.as_str() {
                            "<" => order == Some(std::cmp::Ordering::Less),
                            ">" => order == Some(std::cmp::Ordering::Greater),
                            "<=" => order != Some(std::cmp::Ordering::Greater),
                            _ => order != Some(std::cmp::Ordering::Less),
                        })
                    }
                }
            }
        }
        Expr::Lambda(..) => return Err("fn 只能作为 $map/$filter 参数".into()),
        Expr::Call(name, args) => {
            if name == "$map" || name == "$filter" {
                if args.len() != 2 {
                    return Err(format!("{name} 需要两个参数"));
                }
                let a = ev(&args[0])?;
                let a = a.as_array().ok_or("需要数组")?;
                let Expr::Lambda(params, body) = &args[1] else {
                    return Err("第二个参数必须为 fn".into());
                };
                let mut out = vec![];
                for (i, item) in a.iter().enumerate() {
                    let mut v = vars.clone();
                    for (p, val) in params.iter().zip([item.clone(), json!(i), json!(a)]) {
                        v.insert(p.clone(), val);
                    }
                    let result = eval(body, root, &v, budget)?;
                    if name == "$map" {
                        out.push(result)
                    } else if boolean(&result)? {
                        out.push(item.clone())
                    }
                }
                json!(out)
            } else {
                let values = args.iter().map(&mut ev).collect::<Result<Vec<_>, _>>()?;
                let arity = match name.as_str() {
                    "$lookup" | "$split" => 2,
                    "$join" => {
                        if values.len() == 1 {
                            1
                        } else {
                            2
                        }
                    }
                    "$count" | "$sum" | "$exists" | "$keys" | "$uppercase" | "$lowercase"
                    | "$trim" | "$json" | "$jsonParse" => 1,
                    _ => return Err(format!("不支持的 FQL 函数 {name}")),
                };
                if values.len() != arity {
                    return Err(format!("{name} 参数数量错误"));
                }
                let a = &values[0];
                match name.as_str() {
                    "$count" => json!(a.as_array().ok_or("需要数组")?.len()),
                    "$sum" => json!(
                        a.as_array()
                            .ok_or("需要数组")?
                            .iter()
                            .map(number)
                            .collect::<Result<Vec<_>, _>>()?
                            .iter()
                            .sum::<f64>()
                    ),
                    "$exists" => json!(!a.is_null()),
                    "$keys" => json!(a.as_object().ok_or("需要对象")?.keys().collect::<Vec<_>>()),
                    "$lookup" => field(a, &text(&values[1])),
                    "$join" => json!(
                        a.as_array()
                            .ok_or("需要数组")?
                            .iter()
                            .map(text)
                            .collect::<Vec<_>>()
                            .join(&values.get(1).map(text).unwrap_or_default())
                    ),
                    "$split" => json!(
                        a.as_str()
                            .ok_or("需要字符串")?
                            .split(&text(&values[1]))
                            .collect::<Vec<_>>()
                    ),
                    "$uppercase" => json!(a.as_str().ok_or("需要字符串")?.to_uppercase()),
                    "$lowercase" => json!(a.as_str().ok_or("需要字符串")?.to_lowercase()),
                    "$trim" => json!(a.as_str().ok_or("需要字符串")?.trim()),
                    "$json" => json!(a.to_string()),
                    "$jsonParse" => serde_json::from_str(a.as_str().ok_or("需要字符串")?)
                        .map_err(|e| e.to_string())?,
                    _ => unreachable!(),
                }
            }
        }
    })
}
pub fn query(source: &str, input: &Value) -> Result<Value, String> {
    if source.len() > 16384 {
        return Err("表达式长度超过 16KB".into());
    }
    let tokens = lex(source)?;
    if tokens.len() > 512 {
        return Err("表达式超过 512 个词法单元，请拆分到多个 Evaluate 节点".into());
    }
    // Bound nesting before recursive parsing to avoid exhausting the native stack.
    let mut depth = 0i32;
    for (t, _) in &tokens {
        if matches!(t,Tok::Sym(s) if ["(","[","{"].contains(&s.as_str())) {
            depth += 1;
            if depth > 64 {
                return Err("表达式嵌套超过 64 层".into());
            }
        }
        if matches!(t,Tok::Sym(s) if [")","]","}"].contains(&s.as_str())) {
            depth -= 1
        }
    }
    let mut p = Parser { tokens, at: 0 };
    let e = p.expr(0)?;
    if p.peek() != &Tok::End {
        return Err(p.err("存在未解析的表达式"));
    }
    eval(&e, input, &Map::new(), &mut 100_000)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transformations() {
        let v = json!({"items":[{"price":2},{"price":5}]});
        assert_eq!(query("items[price > 2].price", &v).unwrap(), json!([5]));
        assert_eq!(
            query("$map(items, fn($e){$e.price * 2})", &v).unwrap(),
            json!([4.0, 10.0])
        );
        assert_eq!(query("$sum(items.price)", &v).unwrap(), json!(7.0));
    }
    #[test]
    fn strict_errors() {
        assert!(
            query("$unknown(1)", &Value::Null)
                .unwrap_err()
                .contains("不支持")
        );
        assert!(query("1 / 0", &Value::Null).is_err());
        assert!(query("items[price]", &json!({"items":[{"price":1}]})).is_err());
        assert_eq!(
            query("false and (1 / 0 = 1)", &Value::Null).unwrap(),
            json!(false)
        );
    }
    #[test]
    fn documented_functions() {
        assert_eq!(
            query("$filter([1,2,3,4,5], fn($e){$e>3})", &Value::Null).unwrap(),
            json!([4, 5])
        );
        assert_eq!(query("$exists(null)", &Value::Null).unwrap(), json!(false));
        assert_eq!(
            query("$join([\"hello\",\"world\"], \"-\")", &Value::Null).unwrap(),
            json!("hello-world")
        );
        assert_eq!(
            query("true ? {\"name\": $trim(' a ')} : null", &Value::Null).unwrap(),
            json!({"name":"a"})
        );
    }
}
