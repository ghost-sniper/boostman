//! Oracle uses OCI on blocking workers; cancellation must reach the live connection.
use base64::{Engine, engine::general_purpose::STANDARD};
use oracle::{
    Connection, Statement,
    sql_type::{OracleType, Timestamp},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

fn descriptor(c: &Value, timeout_ms: u64) -> Result<String, String> {
    let mut value = c.clone();
    value["driver"] = json!("oracle");
    let config = crate::db_config::ConnectionConfig::read(&value)?;
    config.validate_address()?;
    let host = config.host.as_str();
    let name = config.database.as_str();
    let port = config.port;
    let mode = match config.oracle_mode {
        crate::db_config::OracleMode::Service => "SERVICE_NAME",
        crate::db_config::OracleMode::Sid => "SID",
    };
    let seconds = timeout_ms.div_ceil(1000).max(1);
    Ok(format!(
        "(DESCRIPTION=(CONNECT_TIMEOUT={seconds})(TRANSPORT_CONNECT_TIMEOUT={seconds})(RETRY_COUNT=0)(ADDRESS=(PROTOCOL=TCP)(HOST={host})(PORT={port}))(CONNECT_DATA=({mode}={name})))"
    ))
}
fn driver_error(e: oracle::Error) -> String {
    let message = e.to_string();
    if message.contains("DPI-1047") {
        format!(
            "未找到 Oracle Instant Client。请安装与 Boostman 架构一致的 Oracle Instant Client Basic，并将其目录加入 PATH 后重启应用。详情: {message}"
        )
    } else {
        message
    }
}
fn stopped(signal: &AtomicU8) -> Result<(), String> {
    match signal.load(Ordering::SeqCst) {
        1 => Err("已取消".into()),
        2 => Err("执行超时".into()),
        _ => Ok(()),
    }
}
fn remaining(start: Instant, ms: u64, signal: &AtomicU8) -> Result<Duration, String> {
    stopped(signal)?;
    let left = ms.saturating_sub(start.elapsed().as_millis() as u64);
    if left == 0 {
        signal.store(2, Ordering::SeqCst);
        return Err("执行超时".into());
    }
    Ok(Duration::from_millis(left))
}

pub async fn execute(r: &Value, c: &Value, cancel: &AtomicBool) -> Result<Value, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".into());
    }
    crate::db_config::ConnectionConfig::read(c)?.validate()?;
    let timeout = crate::http::timeout(r);
    let connect = descriptor(c, timeout)?;
    let request = r.clone();
    let config = c.clone();
    let start = Instant::now();
    let signal = Arc::new(AtomicU8::new(0));
    let active: Arc<Mutex<Option<Arc<Connection>>>> = Arc::new(Mutex::new(None));
    let worker_signal = signal.clone();
    let worker_active = active.clone();
    let mut worker = tokio::task::spawn_blocking(move || {
        run(
            &request,
            &config,
            &connect,
            start,
            timeout,
            &worker_signal,
            &worker_active,
        )
    });
    loop {
        tokio::select! {
            result=&mut worker=>{stopped(&signal)?;return result.map_err(|e|format!("Oracle 执行线程异常: {e}"))?},
            _=tokio::time::sleep(Duration::from_millis(10))=>{
                let reason=if cancel.load(Ordering::SeqCst){1}else if start.elapsed()>=Duration::from_millis(timeout){2}else{0};
                if reason!=0&&signal.compare_exchange(0,reason,Ordering::SeqCst,Ordering::SeqCst).is_ok(){
                    let conn=active.lock().map_err(|_|"Oracle 连接状态异常")?.clone();
                if let Some(conn)=conn { let _=tokio::task::spawn_blocking(move||conn.break_execution()).await; }
                    // Wait for the worker to stop; dropping a blocking task alone does not cancel OCI.
                    let _=worker.await;return stopped(&signal).and(Err("Oracle 执行已中断".into()));
                }
            }
        }
    }
}
fn validate_binds(names: &[&str], count: usize) -> Result<(), String> {
    let unique: HashSet<_> = names.iter().copied().collect();
    if unique.len() != count || (1..=count).any(|i| !unique.contains(i.to_string().as_str())) {
        return Err(format!(
            "Oracle 参数不匹配：SQL 含 {} 个不同占位符，实际 {} 个参数。请使用 :1、:2 等连续编号占位符",
            unique.len(),
            count
        ));
    }
    Ok(())
}
fn bind(stmt: &mut Statement, params: &[Value]) -> Result<(), String> {
    validate_binds(&stmt.bind_names(), params.len())?;
    for (index, param) in params.iter().enumerate() {
        let typed = param.get("type").and_then(Value::as_str);
        let value = if typed.is_some() {
            &param["value"]
        } else {
            param
        };
        let name = (index + 1).to_string();
        let i = name.as_str();
        let kind = typed.unwrap_or(match value {
            Value::Null | Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Number(n) if n.is_i64() => "i64",
            Value::Number(n) if n.is_u64() => "u64",
            Value::Number(_) => "f64",
            _ => return Err("对象/数组参数请使用 {type:json,value:...}".into()),
        });
        let oracle_type = match kind {
            "string" => OracleType::Varchar2(1),
            "i64" | "u64" | "decimal" => OracleType::Number(0, 0),
            "f64" => OracleType::BinaryDouble,
            "bool" => OracleType::Number(1, 0),
            "bytes" => OracleType::Raw(1),
            "json" => OracleType::CLOB,
            _ => return Err(format!("未知 Oracle 参数类型 {kind}")),
        };
        let result = if value.is_null() {
            stmt.bind(i, &oracle_type)
        } else {
            match kind {
                "string" => stmt.bind(i, &value.as_str().ok_or("string 参数必须为字符串")?),
                "i64" => stmt.bind(
                    i,
                    &value
                        .as_i64()
                        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                        .ok_or("i64 参数无效")?,
                ),
                "u64" => stmt.bind(
                    i,
                    &value
                        .as_u64()
                        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
                        .ok_or("u64 参数无效")?,
                ),
                "f64" => stmt.bind(i, &value.as_f64().ok_or("f64 参数无效")?),
                "bool" => stmt.bind(i, &i32::from(value.as_bool().ok_or("bool 参数无效")?)),
                "decimal" => {
                    let raw = value.as_str().ok_or("decimal 必须为字符串")?;
                    let decimal = raw
                        .parse::<sqlx::types::BigDecimal>()
                        .map_err(|_| "decimal 参数无效")?;
                    stmt.bind(i, &(&decimal.to_string(), &OracleType::Number(0, 0)))
                }
                "bytes" => stmt.bind(
                    i,
                    &STANDARD
                        .decode(value.as_str().ok_or("bytes 必须为 Base64")?)
                        .map_err(|e| e.to_string())?,
                ),
                "json" => stmt.bind(i, &(&value.to_string(), &OracleType::CLOB)),
                _ => unreachable!(),
            }
        };
        result.map_err(driver_error)?;
    }
    Ok(())
}
fn number(raw: String, kind: &OracleType) -> Result<Value, String> {
    let decimal = raw
        .parse::<sqlx::types::BigDecimal>()
        .map_err(|_| "Oracle NUMBER 格式无效")?;
    if matches!(kind,OracleType::Number(_,scale) if *scale>0)
        || matches!(kind, OracleType::Float(_))
    {
        return Ok(json!(raw));
    }
    if decimal == decimal.with_scale(0) {
        if let Ok(n) = decimal.with_scale(0).to_string().parse::<i64>() {
            return Ok(crate::sql::integer(n));
        }
    }
    Ok(json!(raw))
}
fn timestamp(t: Timestamp, zoned: bool) -> Result<String, String> {
    let date = chrono::NaiveDate::from_ymd_opt(t.year(), t.month(), t.day())
        .and_then(|d| d.and_hms_nano_opt(t.hour(), t.minute(), t.second(), t.nanosecond()))
        .ok_or("Oracle 日期时间超出支持范围")?;
    if zoned {
        use chrono::TimeZone;
        let offset = chrono::FixedOffset::east_opt(t.tz_offset()).ok_or("Oracle 时区偏移无效")?;
        Ok(offset
            .from_local_datetime(&date)
            .single()
            .ok_or("Oracle 时区时间无效")?
            .to_rfc3339())
    } else {
        Ok(date.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
    }
}
fn row_json(row: &oracle::Row) -> Result<Value, String> {
    let mut object = Map::new();
    for (i, col) in row.column_info().iter().enumerate() {
        let kind = col.oracle_type();
        let value = if row.sql_values()[i].is_null().map_err(driver_error)? {
            Value::Null
        } else {
            match kind {
                OracleType::Number(..) | OracleType::Float(_) => {
                    number(row.get::<_, String>(i).map_err(driver_error)?, kind)?
                }
                OracleType::BinaryFloat | OracleType::BinaryDouble => {
                    let n = row.get::<_, f64>(i).map_err(driver_error)?;
                    if !n.is_finite() {
                        return Err("Oracle 浮点数不是有限值，无法输出 JSON".into());
                    }
                    json!(n)
                }
                OracleType::Int64 => crate::sql::integer(row.get(i).map_err(driver_error)?),
                OracleType::UInt64 => crate::sql::unsigned(row.get(i).map_err(driver_error)?),
                OracleType::Date | OracleType::Timestamp(_) | OracleType::TimestampLTZ(_) => {
                    json!(timestamp(row.get(i).map_err(driver_error)?, false)?)
                }
                OracleType::TimestampTZ(_) => {
                    json!(timestamp(row.get(i).map_err(driver_error)?, true)?)
                }
                OracleType::Raw(_) | OracleType::LongRaw | OracleType::BLOB => {
                    json!(STANDARD.encode(row.get::<_, Vec<u8>>(i).map_err(driver_error)?))
                }
                OracleType::Varchar2(_)
                | OracleType::NVarchar2(_)
                | OracleType::Char(_)
                | OracleType::NChar(_)
                | OracleType::CLOB
                | OracleType::NCLOB
                | OracleType::Long
                | OracleType::Rowid => json!(row.get::<_, String>(i).map_err(driver_error)?),
                _ => {
                    return Err(format!(
                        "暂不支持 Oracle 列类型 {kind}，请显式转换为 VARCHAR2/CLOB；JSON 类型请使用 JSON_SERIALIZE"
                    ));
                }
            }
        };
        object.insert(col.name().to_owned(), value);
    }
    Ok(json!(object))
}
fn run(
    r: &Value,
    c: &Value,
    connect: &str,
    start: Instant,
    ms: u64,
    signal: &AtomicU8,
    active: &Mutex<Option<Arc<Connection>>>,
) -> Result<Value, String> {
    let sql = r["sql"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("缺少 SQL")?;
    let empty = vec![];
    let params = if r["params"].is_null() {
        &empty
    } else {
        r["params"].as_array().ok_or("SQL params 必须为数组")?
    };
    stopped(signal)?;
    let config = crate::db_config::ConnectionConfig::read(c)?;
    let password = config.password()?;
    let conn =
        Arc::new(Connection::connect(&config.username, password, connect).map_err(driver_error)?);
    *active.lock().map_err(|_| "Oracle 连接状态异常")? = Some(conn.clone());
    let result = (|| {
        conn.set_call_timeout(Some(remaining(start, ms, signal)?))
            .map_err(driver_error)?;
        let mut stmt = conn.statement(sql).build().map_err(driver_error)?;
        if !stmt.is_query() && !stmt.is_dml() && !stmt.is_ddl() {
            return Err("Oracle 首版支持 SELECT、INSERT、UPDATE、DELETE、MERGE 和 DDL；暂不支持 PL/SQL、事务控制语句或 OUT 参数".into());
        }
        if stmt.is_returning() {
            return Err("Oracle 首版暂不支持 RETURNING INTO / OUT 参数".into());
        }
        bind(&mut stmt, params)?;
        let mut rows = vec![];
        let columns;
        let affected;
        conn.set_call_timeout(Some(remaining(start, ms, signal)?))
            .map_err(driver_error)?;
        if stmt.is_query() {
            let mut results = stmt.query(&[]).map_err(driver_error)?;
            let mut seen = HashSet::new();
            let mut cols = vec![];
            for col in results.column_info() {
                if !seen.insert(col.name()) {
                    return Err(format!("重复列名 {}，请使用 AS 别名", col.name()));
                }
                cols.push(json!({"name":col.name(),"type":col.oracle_type().to_string()}));
            }
            columns = json!(cols);
            loop {
                conn.set_call_timeout(Some(remaining(start, ms, signal)?))
                    .map_err(driver_error)?;
                let Some(row) = results.next() else { break };
                if rows.len() >= 100_000 {
                    return Err("结果超过 100000 行，请限制查询行数".into());
                }
                rows.push(row_json(&row.map_err(driver_error)?)?);
            }
            affected = 0;
        } else {
            stmt.execute(&[]).map_err(driver_error)?;
            affected = stmt.row_count().map_err(driver_error)?;
            columns = json!([]);
            conn.set_call_timeout(Some(remaining(start, ms, signal)?))
                .map_err(driver_error)?;
            conn.commit().map_err(driver_error)?;
        }
        stopped(signal)?;
        Ok(
            json!({"status":"succeeded","data":{"rows":rows,"columns":columns,"affectedRows":affected},"meta":{"driver":"oracle"},"error":null,"elapsedMs":start.elapsed().as_millis()}),
        )
    })();
    // Closing an uncommitted connection rolls back DML; Oracle DDL commits implicitly.
    *active.lock().map_err(|_| "Oracle 连接状态异常")? = None;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbered_binds() {
        validate_binds(&["2", "1", "1"], 2).unwrap();
        validate_binds(&[], 0).unwrap();
        assert!(validate_binds(&["2"], 1).is_err());
        assert!(validate_binds(&["name"], 1).is_err());
        assert!(validate_binds(&["1"], 0).is_err());
    }
    #[tokio::test]
    async fn cancellation_before_connect() {
        let err = execute(&json!({}), &json!({}), &AtomicBool::new(true))
            .await
            .unwrap_err();
        assert_eq!(err, "已取消");
    }
    #[test]
    fn service_and_sid() {
        let c = json!({"host":"127.0.0.1","database":"FREEPDB1"});
        let d = descriptor(&c, 1500).unwrap();
        assert!(d.contains("PORT=1521"));
        assert!(d.contains("SERVICE_NAME=FREEPDB1"));
        assert!(d.contains("CONNECT_TIMEOUT=2"));
        let d = descriptor(
            &json!({"host":"db","port":1522,"database":"ORCL","oracleMode":"sid"}),
            30000,
        )
        .unwrap();
        assert!(d.contains("SID=ORCL"));
        assert!(descriptor(&json!({"host":"db)(x=y","database":"ORCL"}), 1000).is_err());
        assert!(descriptor(&json!({"database":"bad/name"}), 1000).is_err());
    }
    #[test]
    fn exact_numbers() {
        assert_eq!(
            number("42".into(), &OracleType::Number(10, 0)).unwrap(),
            json!(42)
        );
        assert_eq!(
            number("9007199254740992".into(), &OracleType::Number(20, 0)).unwrap(),
            json!("9007199254740992")
        );
        assert_eq!(
            number("1.234567890123456789".into(), &OracleType::Number(30, 18)).unwrap(),
            json!("1.234567890123456789")
        );
        assert_eq!(
            number("1.25".into(), &OracleType::Number(0, 0)).unwrap(),
            json!("1.25")
        );
    }
    #[test]
    fn dates() {
        let t = Timestamp::new(2026, 11, 6, 12, 34, 56, 123000000).unwrap();
        assert_eq!(
            timestamp(t.clone(), false).unwrap(),
            "2026-11-06T12:34:56.123"
        );
        assert_eq!(
            timestamp(t.and_tz_offset(28800).unwrap(), true).unwrap(),
            "2026-11-06T12:34:56.123+08:00"
        );
    }
}
