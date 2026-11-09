use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use sqlx::{
    Postgres, TypeInfo,
    postgres::{PgArguments, PgTypeInfo},
    query::Query,
    types::{BigDecimal, Json},
};
use std::{borrow::Cow, str::FromStr};

fn parse<T: FromStr>(value: &Value) -> Option<T> {
    match value {
        Value::String(text) => text.parse().ok(),
        Value::Number(number) => number.to_string().parse().ok(),
        _ => None,
    }
}

pub(super) fn bind<'q>(
    mut query: Query<'q, Postgres, PgArguments>,
    params: &[Value],
    types: &[PgTypeInfo],
) -> Result<Query<'q, Postgres, PgArguments>, String> {
    if params.len() != types.len() {
        return Err(format!(
            "SQL 需要 {} 个参数，实际提供 {} 个",
            types.len(),
            params.len()
        ));
    }
    for (index, (parameter, target)) in params.iter().zip(types).enumerate() {
        let kind = parameter.get("type").and_then(Value::as_str);
        let value = if kind.is_some() {
            &parameter["value"]
        } else {
            parameter
        };
        let target = target.name();
        let invalid = || format!("参数 ${} 无法转换为 PostgreSQL {target}", index + 1);
        // Validate explicit numeric wrappers before converting to the server type.
        let value = match kind {
            Some("i64") if !value.is_null() => {
                Cow::Owned(Value::from(parse::<i64>(value).ok_or_else(invalid)?))
            }
            Some("u64") if !value.is_null() => {
                Cow::Owned(Value::from(parse::<u64>(value).ok_or_else(invalid)?))
            }
            None
            | Some("i64" | "u64" | "f64" | "bool" | "decimal" | "json" | "bytes" | "string") => {
                Cow::Borrowed(value)
            }
            Some(other) => return Err(format!("未知参数类型 {other}")),
        };
        if value.is_null() {
            // A protocol NULL has no payload, regardless of its SQL type.
            query = query.bind(Option::<String>::None);
            continue;
        }
        let value = value.as_ref();
        query = match target {
            "INT2" => query.bind(parse::<i16>(value).ok_or_else(invalid)?),
            "INT4" => query.bind(parse::<i32>(value).ok_or_else(invalid)?),
            "INT8" => query.bind(parse::<i64>(value).ok_or_else(invalid)?),
            "FLOAT4" => query.bind(
                parse::<f32>(value)
                    .filter(|v| v.is_finite())
                    .ok_or_else(invalid)?,
            ),
            "FLOAT8" => query.bind(
                parse::<f64>(value)
                    .filter(|v| v.is_finite())
                    .ok_or_else(invalid)?,
            ),
            "NUMERIC" => query.bind(parse::<BigDecimal>(value).ok_or_else(invalid)?),
            "BOOL" => query.bind(
                value
                    .as_bool()
                    .or_else(|| parse::<bool>(value))
                    .ok_or_else(invalid)?,
            ),
            "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "UNKNOWN" => query.bind(match value {
                Value::String(text) => text.clone(),
                Value::Bool(_) | Value::Number(_) => value.to_string(),
                _ => return Err(invalid()),
            }),
            "JSON" | "JSONB" => query.bind(Json(value.clone())),
            "BYTEA" => query.bind(
                STANDARD
                    .decode(value.as_str().ok_or_else(invalid)?)
                    .map_err(|_| invalid())?,
            ),
            "UUID" => query.bind(parse::<uuid::Uuid>(value).ok_or_else(invalid)?),
            "DATE" => query.bind(parse::<chrono::NaiveDate>(value).ok_or_else(invalid)?),
            "TIME" => query.bind(parse::<chrono::NaiveTime>(value).ok_or_else(invalid)?),
            "TIMESTAMP" => query.bind(parse::<chrono::NaiveDateTime>(value).ok_or_else(invalid)?),
            "TIMESTAMPTZ" => {
                query.bind(parse::<chrono::DateTime<chrono::Utc>>(value).ok_or_else(invalid)?)
            }
            _ => {
                return Err(format!(
                    "参数 ${} 暂不支持 PostgreSQL {target}，请在 SQL 中显式转换输入类型",
                    index + 1
                ));
            }
        };
    }
    Ok(query)
}
