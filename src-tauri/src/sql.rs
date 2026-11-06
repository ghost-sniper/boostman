use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::TryStreamExt;
use serde_json::{Map, Value, json};
use sqlx::{Column, Connection, Either, Executor, Row, TypeInfo, ValueRef};
use std::{collections::HashSet, str::FromStr, sync::atomic::AtomicBool, time::Instant};
fn connection(c: &Value) -> Result<(String, String, u16, String, String, String), String> {
    let config = crate::db_config::ConnectionConfig::read(c)?;
    config.validate()?;
    Ok((
        c["driver"].as_str().ok_or("缺少数据库类型")?.into(),
        config.host.clone(),
        config.port,
        config.database.clone(),
        config.username.clone(),
        config.password()?,
    ))
}
pub async fn test(c: &Value) -> Result<(), String> {
    execute(
        &json!({"sql":if c["driver"]=="oracle"{"SELECT 1 AS connected FROM dual"}else{"SELECT 1 AS connected"},"params":[],"timeoutMs":10000}),
        c,
        &AtomicBool::new(false),
    )
    .await
    .map(|_| ())
}
pub async fn execute(r: &Value, c: &Value, cancel: &AtomicBool) -> Result<Value, String> {
    if c["driver"] == "oracle" {
        return crate::oracle_adapter::execute(r, c, cancel).await;
    }
    crate::http::cancellable(cancel, crate::http::timeout(r), run(r, c)).await
}
pub(crate) fn integer(n: i64) -> Value {
    if n.unsigned_abs() > 9_007_199_254_740_991 {
        json!(n.to_string())
    } else {
        json!(n)
    }
}
pub(crate) fn unsigned(n: u64) -> Value {
    if n > 9_007_199_254_740_991 {
        json!(n.to_string())
    } else {
        json!(n)
    }
}
fn columns<C: Column>(cols: &[C]) -> Result<Value, String> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for c in cols {
        if !seen.insert(c.name()) {
            return Err(format!("重复列名 {}，请使用 AS 别名", c.name()));
        }
        out.push(json!({"name":c.name(),"type":c.type_info().name()}));
    }
    Ok(json!(out))
}
macro_rules! decode_common {
    ($row:ident,$i:ident,$t:ty) => {
        $row.try_get::<$t, _>($i).map_err(|e| e.to_string())?
    };
}
fn pg_row(row: &sqlx::postgres::PgRow) -> Result<Value, String> {
    let mut out = Map::new();
    for (i, c) in row.columns().iter().enumerate() {
        let t = c.type_info().name();
        let v = if row.try_get_raw(i).map_err(|e| e.to_string())?.is_null() {
            Value::Null
        } else {
            match t {
                "BOOL" => json!(decode_common!(row, i, bool)),
                "INT2" => integer(decode_common!(row, i, i16) as i64),
                "INT4" => integer(decode_common!(row, i, i32) as i64),
                "INT8" => integer(decode_common!(row, i, i64)),
                "FLOAT4" => json!(decode_common!(row, i, f32)),
                "FLOAT8" => json!(decode_common!(row, i, f64)),
                "NUMERIC" => json!(decode_common!(row, i, sqlx::types::BigDecimal).to_string()),
                "JSON" | "JSONB" => decode_common!(row, i, Value),
                "BYTEA" => json!(STANDARD.encode(decode_common!(row, i, Vec<u8>))),
                "UUID" => json!(decode_common!(row, i, uuid::Uuid).to_string()),
                "DATE" => json!(decode_common!(row, i, chrono::NaiveDate).to_string()),
                "TIME" => json!(decode_common!(row, i, chrono::NaiveTime).to_string()),
                "TIMESTAMP" => json!(
                    decode_common!(row, i, chrono::NaiveDateTime)
                        .format("%Y-%m-%dT%H:%M:%S%.f")
                        .to_string()
                ),
                "TIMESTAMPTZ" => {
                    json!(decode_common!(row, i, chrono::DateTime<chrono::Utc>).to_rfc3339())
                }
                "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "UNKNOWN" => {
                    json!(decode_common!(row, i, String))
                }
                _ => return Err(format!("暂不支持 PostgreSQL 列类型 {t}，请显式转换为 text")),
            }
        };
        out.insert(c.name().into(), v);
    }
    Ok(Value::Object(out))
}
fn my_row(row: &sqlx::mysql::MySqlRow) -> Result<Value, String> {
    let mut out = Map::new();
    for (i, c) in row.columns().iter().enumerate() {
        let t = c.type_info().name();
        let v = if row.try_get_raw(i).map_err(|e| e.to_string())?.is_null() {
            Value::Null
        } else {
            match t {
                "BOOLEAN" => json!(decode_common!(row, i, bool)),
                "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" => {
                    integer(decode_common!(row, i, i64))
                }
                "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED"
                | "INT UNSIGNED" | "BIGINT UNSIGNED" => unsigned(decode_common!(row, i, u64)),
                "FLOAT" => json!(decode_common!(row, i, f32)),
                "DOUBLE" => json!(decode_common!(row, i, f64)),
                "DECIMAL" => json!(decode_common!(row, i, sqlx::types::BigDecimal).to_string()),
                "JSON" => decode_common!(row, i, Value),
                "DATE" => json!(decode_common!(row, i, chrono::NaiveDate).to_string()),
                "DATETIME" | "TIMESTAMP" => json!(
                    decode_common!(row, i, chrono::NaiveDateTime)
                        .format("%Y-%m-%dT%H:%M:%S%.f")
                        .to_string()
                ),
                "TIME" => json!(decode_common!(row, i, chrono::NaiveTime).to_string()),
                "BINARY" | "VARBINARY" | "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB"
                | "BIT" => json!(STANDARD.encode(decode_common!(row, i, Vec<u8>))),
                "CHAR" | "VARCHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" => {
                    json!(decode_common!(row, i, String))
                }
                _ => return Err(format!("暂不支持 MySQL 列类型 {t}，请显式转换")),
            }
        };
        out.insert(c.name().into(), v);
    }
    Ok(Value::Object(out))
}
// Typed parameters permit exact decimals and typed NULL without embedding values in SQL.
macro_rules! bind_params {
    ($q:ident,$params:ident) => {
        for p in $params {
            let typed = p.get("type").and_then(Value::as_str);
            let v = if typed.is_some() { &p["value"] } else { p };
            $q = match typed {
                Some("i64") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(
                        v.as_i64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                            .ok_or("i64 参数无效")?,
                    )
                }),
                Some("f64") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(v.as_f64().ok_or("f64 参数无效")?)
                }),
                Some("bool") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(v.as_bool().ok_or("bool 参数无效")?)
                }),
                Some("decimal") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(
                        sqlx::types::BigDecimal::from_str(
                            v.as_str().ok_or("decimal 必须为字符串")?,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                }),
                Some("json") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(sqlx::types::Json(v.clone()))
                }),
                Some("bytes") => $q.bind(if v.is_null() {
                    None
                } else {
                    Some(
                        STANDARD
                            .decode(v.as_str().ok_or("bytes 必须为 Base64")?)
                            .map_err(|e| e.to_string())?,
                    )
                }),
                Some("string") => $q.bind(if v.is_null(){None}else{Some(v.as_str().ok_or("string 参数必须为字符串或 null")?.to_owned())}),
                Some(other) => return Err(format!("未知参数类型 {other}")),
                None => match v {
                    Value::Null => $q.bind(Option::<String>::None),
                    Value::Bool(b) => $q.bind(*b),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            $q.bind(i)
                        } else if n.is_u64() {
                            return Err("整数参数超出 i64 范围，请使用 decimal 类型的字符串参数".into());
                        } else {
                            $q.bind(n.as_f64().ok_or("数字参数无效")?)
                        }
                    }
                    Value::String(s) => $q.bind(s.clone()),
                    _ => return Err("对象/数组参数请使用 {type:json,value:...}".into()),
                },
            };
        }
    };
}
async fn run(r: &Value, c: &Value) -> Result<Value, String> {
    let start = Instant::now();
    let (driver, host, port, db, user, password) = connection(c)?;
    let sql = r["sql"].as_str().ok_or("缺少 SQL")?;
    let empty = vec![];
    let params = if r["params"].is_null() {
        &empty
    } else {
        r["params"].as_array().ok_or("SQL params 必须为数组")?
    };
    let mut rows = vec![];
    let mut affected = 0u64;
    let cols;
    match driver.as_str() {
        "postgres" => {
            let options = sqlx::postgres::PgConnectOptions::new()
                .host(&host)
                .port(port)
                .database(&db)
                .username(&user)
                .password(&password);
            let mut conn = sqlx::PgConnection::connect_with(&options)
                .await
                .map_err(|e| e.to_string())?;
            cols = columns(
                conn.describe(sql)
                    .await
                    .map_err(|e| e.to_string())?
                    .columns(),
            )?;
            let mut q = sqlx::query(sql);
            bind_params!(q, params);
            #[allow(deprecated)]
            let mut stream = q.fetch_many(&mut conn);
            while let Some(next) = stream.try_next().await.map_err(|e| e.to_string())? {
                match next {
                    Either::Left(done) => affected = done.rows_affected(),
                    Either::Right(row) => {
                        if rows.len() >= 100_000 {
                            return Err("结果超过 100000 行，请用 LIMIT 限制查询".into());
                        }
                        rows.push(pg_row(&row)?);
                    }
                }
            }
        }
        "mysql" => {
            let options = sqlx::mysql::MySqlConnectOptions::new()
                .host(&host)
                .port(port)
                .database(&db)
                .username(&user)
                .password(&password);
            let mut conn = sqlx::MySqlConnection::connect_with(&options)
                .await
                .map_err(|e| e.to_string())?;
            cols = columns(
                conn.describe(sql)
                    .await
                    .map_err(|e| e.to_string())?
                    .columns(),
            )?;
            let mut q = sqlx::query(sql);
            bind_params!(q, params);
            #[allow(deprecated)]
            let mut stream = q.fetch_many(&mut conn);
            while let Some(next) = stream.try_next().await.map_err(|e| e.to_string())? {
                match next {
                    Either::Left(done) => affected = done.rows_affected(),
                    Either::Right(row) => {
                        if rows.len() >= 100_000 {
                            return Err("结果超过 100000 行，请用 LIMIT 限制查询".into());
                        }
                        rows.push(my_row(&row)?);
                    }
                }
            }
        }
        _ => return Err("仅支持 mysql/postgres/oracle".into()),
    }
    Ok(
        json!({"status":"succeeded","data":{"rows":rows,"columns":cols,"affectedRows":affected},"meta":{"driver":driver},"error":null,"elapsedMs":start.elapsed().as_millis()}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_numbers() {
        assert_eq!(integer(9007199254740992), json!("9007199254740992"));
        assert_eq!(integer(1), json!(1));
        assert_eq!(unsigned(u64::MAX), json!(u64::MAX.to_string()));
    }
}
