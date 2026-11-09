use serde_json::Value;
use std::borrow::Cow;

pub(crate) enum Dialect {
    Mysql {
        backslash_escapes: bool,
        ansi_quotes: bool,
    },
    Oracle,
}

// PostgreSQL already accepts the portable $1 notation. Translate it only for
// drivers that need another marker, keeping values out of the SQL text.
pub(crate) fn adapt<'a>(
    sql: &'a str,
    params: &'a [Value],
    dialect: Dialect,
) -> Result<(Cow<'a, str>, Cow<'a, [Value]>), String> {
    if !sql.contains('$') || params.is_empty() {
        return Ok((sql.into(), params.into()));
    }
    let mysql = matches!(dialect, Dialect::Mysql { .. });
    let bytes = sql.as_bytes();
    let mut rewritten = String::new();
    let mut order = Vec::new();
    let mut copied = 0;
    let mut i = 0;
    let mut native = false;
    while i < bytes.len() {
        let b = bytes[i];
        let next = bytes.get(i + 1).copied();
        match b {
            b'-' if next == Some(b'-')
                && (!mysql || bytes.get(i + 2).is_none_or(|b| *b <= b' ')) =>
            {
                i = skip_line(bytes, i + 2);
            }
            b'#' if mysql => i = skip_line(bytes, i + 1),
            b'/' if next == Some(b'*') => {
                i = bytes[i + 2..]
                    .windows(2)
                    .position(|pair| pair == b"*/")
                    .map_or(bytes.len(), |end| i + 4 + end);
            }
            b'q' | b'Q' if !mysql && next == Some(b'\'') && i + 2 < bytes.len() => {
                let open = sql[i + 2..].chars().next().unwrap();
                let close = match open {
                    '[' => ']',
                    '{' => '}',
                    '(' => ')',
                    '<' => '>',
                    other => other,
                };
                let end = format!("{close}'");
                let content = i + 2 + open.len_utf8();
                i = sql[content..]
                    .find(&end)
                    .map_or(bytes.len(), |at| content + at + end.len());
            }
            b'\'' | b'"' | b'`' => {
                let escapes = matches!(
                    dialect,
                    Dialect::Mysql {
                        backslash_escapes: true,
                        ..
                    }
                ) && (b == b'\''
                    || b == b'"'
                        && matches!(
                            dialect,
                            Dialect::Mysql {
                                ansi_quotes: false,
                                ..
                            }
                        ));
                i = skip_quoted(bytes, i + 1, b, escapes);
            }
            b'$' if next.is_some_and(|b| b.is_ascii_digit())
                && (i == 0 || !identifier_byte(bytes[i - 1])) =>
            {
                let end = (i + 1..bytes.len())
                    .find(|&n| !bytes[n].is_ascii_digit())
                    .unwrap_or(bytes.len());
                if bytes.get(end).is_some_and(|&b| identifier_byte(b)) {
                    i = end;
                    continue;
                }
                let marker = &sql[i + 1..end];
                let number = marker
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=params.len()).contains(n))
                    .ok_or_else(|| format!("占位符 ${marker} 没有对应的绑定参数"))?;
                rewritten.push_str(&sql[copied..i]);
                if mysql {
                    rewritten.push('?');
                } else {
                    rewritten.push(':');
                    rewritten.push_str(&number.to_string());
                }
                order.push(number - 1);
                copied = end;
                i = end;
            }
            b'?' if mysql => {
                native = true;
                i += 1;
            }
            b':' if !mysql && next.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') => {
                native = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    if order.is_empty() {
        return Ok((sql.into(), params.into()));
    }
    if native {
        return Err("同一条 SQL 请勿混用 $1、$2 与数据库原生占位符".into());
    }
    let mut used = vec![false; params.len()];
    for &index in &order {
        used[index] = true;
    }
    if let Some(index) = used.iter().position(|used| !used) {
        return Err(format!("第 {} 个绑定参数未被 SQL 引用", index + 1));
    }
    rewritten.push_str(&sql[copied..]);
    let params = if mysql {
        Cow::Owned(
            order
                .into_iter()
                .map(|index| params[index].clone())
                .collect(),
        )
    } else {
        Cow::Borrowed(params)
    };
    Ok((rewritten.into(), params))
}

fn identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') || !byte.is_ascii()
}

fn skip_line(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
        i += 1;
    }
    i
}

fn skip_quoted(bytes: &[u8], mut i: usize, quote: u8, escapes: bool) -> usize {
    while let Some(&b) = bytes.get(i) {
        i += 1;
        if b == quote {
            if bytes.get(i) != Some(&quote) {
                break;
            }
            i += 1;
        } else if b == b'\\' && escapes && i < bytes.len() {
            i += 1;
        }
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MYSQL: Dialect = Dialect::Mysql {
        backslash_escapes: true,
        ansi_quotes: false,
    };

    #[test]
    fn mysql_reorders_repeated_parameters_without_rewriting_literals() {
        let sql = "SELECT $2, $1, $2, '$1', 'a\\' $8', `$3` /* $4 */ # $5\n-- $6";
        let params = [json!(1), json!("' OR 1=1")];
        let (sql, values) = adapt(sql, &params, MYSQL).unwrap();
        assert_eq!(
            sql,
            "SELECT ?, ?, ?, '$1', 'a\\' $8', `$3` /* $4 */ # $5\n-- $6"
        );
        assert_eq!(
            values.as_ref(),
            [params[1].clone(), params[0].clone(), params[1].clone()]
        );
    }

    #[test]
    fn oracle_preserves_numeric_indices_and_alternative_quotes() {
        let params = [json!(1), json!(2)];
        let (sql, values) = adapt(
            "SELECT $2, $1, $2, q'[a' $3]', \"$4\" FROM dual -- $5",
            &params,
            Dialect::Oracle,
        )
        .unwrap();
        assert_eq!(sql, "SELECT :2, :1, :2, q'[a' $3]', \"$4\" FROM dual -- $5");
        assert_eq!(values.as_ref(), params);
    }

    #[test]
    fn mixed_and_unmatched_markers_fail_and_native_markers_survive() {
        let params = [json!(1), json!(2)];
        assert!(adapt("SELECT $1, ?", &params, MYSQL).is_err());
        assert!(adapt("SELECT :1, $2 FROM dual", &params, Dialect::Oracle).is_err());
        assert!(adapt("SELECT $1, $3", &params, MYSQL).is_err());
        assert!(adapt("SELECT $1", &params, MYSQL).is_err());
        assert_eq!(
            adapt("SELECT ?, ?", &params, MYSQL).unwrap().0,
            "SELECT ?, ?"
        );
    }
}
