use crate::fql;
use serde_json::{Map, Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub const KINDS: &[&str] = &[
    "constant",
    "list",
    "record",
    "select",
    "delay",
    "log",
    "display",
    "string",
    "bool",
    "number",
    "null",
    "now",
    "date",
    "datetime",
    "createVariable",
    "getVariable",
    "or",
    "collect",
    "output",
    "response",
];
pub fn evaluate(data: &Value, root: &Value) -> Result<Value, String> {
    match data["kind"].as_str().unwrap_or("") {
        "constant" => Ok(data.get("value").cloned().unwrap_or(Value::Null)),
        "string" | "bool" | "number" => {
            let v = data.get("value").ok_or("缺少常量值")?;
            let valid = match data["kind"].as_str().unwrap() {
                "string" => v.is_string(),
                "bool" => v.is_boolean(),
                _ => v.is_number(),
            };
            if !valid {
                return Err(format!("{} 常量类型不正确", data["kind"]));
            }
            Ok(v.clone())
        }
        "null" => Ok(Value::Null),
        "now" => Ok(json!(chrono::Utc::now().timestamp_millis())),
        "date" => {
            let text = data["value"].as_str().ok_or("Date 需要 YYYY-MM-DD 日期")?;
            if text.len() != 10 {
                return Err("Date 需要 YYYY-MM-DD 日期".into());
            }
            let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map_err(|e| format!("Date 日期无效: {e}"))?;
            Ok(json!(
                date.and_hms_opt(0, 0, 0)
                    .ok_or("日期无效")?
                    .and_utc()
                    .timestamp_millis()
            ))
        }
        "datetime" => {
            let date = chrono::DateTime::parse_from_rfc3339(
                data["value"]
                    .as_str()
                    .ok_or("Date & Time 需要带时区的 ISO 8601 时间")?,
            )
            .map_err(|e| format!("Date & Time 时间无效: {e}"))?;
            Ok(json!(date.timestamp_millis()))
        }
        "or" => Ok(root["input"].clone()),
        "getVariable" => {
            let name = variable_name(data)?;
            root["locals"]
                .get(name)
                .cloned()
                .ok_or_else(|| format!("流程变量尚未创建: {name}"))
        }
        "createVariable" | "collect" | "output" | "response" => {
            fql::query(data["expression"].as_str().unwrap_or("input"), root)
        }
        "list" => {
            let items = data["entries"]
                .as_array()
                .ok_or("List entries 必须为数组")?;
            if items.len() > 10000 {
                return Err("List 最多支持 10000 项".into());
            }
            items
                .iter()
                .enumerate()
                .map(|(i, e)| expression(e, root).map_err(|e| format!("List 第 {} 项: {e}", i + 1)))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        "record" => {
            let fields = data["entries"]
                .as_array()
                .ok_or("Record entries 必须为数组")?;
            if fields.len() > 10000 {
                return Err("Record 最多支持 10000 字段".into());
            }
            let mut object = Map::new();
            for field in fields {
                let key = field["key"]
                    .as_str()
                    .filter(|k| !k.is_empty())
                    .ok_or("Record 字段名不能为空")?;
                if object.contains_key(key) {
                    return Err(format!("Record 字段名重复: {key}"));
                }
                object.insert(
                    key.into(),
                    expression(field, root).map_err(|e| format!("Record {key}: {e}"))?,
                );
            }
            Ok(Value::Object(object))
        }
        "select" => {
            let path = data["path"].as_str().unwrap_or("");
            if !path.is_empty() && !path.starts_with('/') {
                return Err(
                    "Select 使用 JSON Pointer 路径，例如 /rows/0/id；空路径表示整个输入".into(),
                );
            }
            root["input"]
                .pointer(path)
                .cloned()
                .ok_or_else(|| format!("Select 路径不存在: {path}"))
        }
        "display" | "log" | "delay" => {
            fql::query(data["expression"].as_str().unwrap_or("input"), root)
        }
        _ => Err("未知数据组件".into()),
    }
}
pub fn variable_name(data: &Value) -> Result<&str, String> {
    data["variableName"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "变量名称不能为空".into())
}
pub fn branches(data: &Value) -> Result<Vec<String>, String> {
    match data["kind"].as_str().unwrap_or("") {
        "if" => Ok(vec!["then".into(), "else".into()]),
        "validate" => Ok(vec!["pass".into(), "fail".into()]),
        "condition" => {
            let conditions = data["conditions"]
                .as_array()
                .ok_or("Condition 缺少条件列表")?;
            if conditions.len() > 100 {
                return Err("Condition 最多支持 100 个条件".into());
            }
            let mut seen = std::collections::HashSet::new();
            let mut result = vec![];
            for condition in conditions {
                let id = condition["id"]
                    .as_str()
                    .filter(|s| {
                        !s.is_empty()
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    })
                    .ok_or("Condition 分支 ID 无效")?;
                if !seen.insert(id) {
                    return Err("Condition 分支 ID 重复".into());
                }
                result.push(format!("case-{id}"));
            }
            result.push("default".into());
            Ok(result)
        }
        "repeat" | "while" | "foreach" => Ok(vec!["body".into()]),
        _ => Ok(vec![]),
    }
}
pub fn condition(data: &Value, root: &Value) -> Result<String, String> {
    branches(data)?;
    for condition in data["conditions"].as_array().unwrap() {
        if fql::boolean(&expression(condition, root)?)? {
            return Ok(format!("case-{}", condition["id"].as_str().unwrap()));
        }
    }
    Ok("default".into())
}
pub fn validate(data: &Value, value: &Value) -> Result<Value, String> {
    let schema = data.get("schema").ok_or("Validate 缺少 JSON Schema")?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(schema)
        .map_err(|e| format!("JSON Schema 无效: {e}"))?;
    let errors: Vec<_> = validator
        .iter_errors(value)
        .take(100)
        .map(|e| json!({"path":e.instance_path().to_string(),"message":e.to_string()}))
        .collect();
    Ok(json!({"passed":errors.is_empty(),"errors":errors}))
}
pub fn response(data: &Value, body: Value) -> Result<Value, String> {
    let code = data["statusCode"]
        .as_u64()
        .filter(|v| (100..=599).contains(v))
        .ok_or("Response 状态码必须为 100–599 的整数")?;
    let headers = data["headers"]
        .as_object()
        .ok_or("Response Headers 必须为 JSON 对象")?;
    for (k, v) in headers {
        reqwest::header::HeaderName::from_bytes(k.as_bytes())
            .map_err(|_| format!("Response Header 名称无效: {k}"))?;
        reqwest::header::HeaderValue::from_str(v.as_str().ok_or("Response Header 值必须为字符串")?)
            .map_err(|_| format!("Response Header 值无效: {k}"))?;
    }
    Ok(json!({"statusCode":code,"headers":headers,"body":body}))
}
fn expression(entry: &Value, root: &Value) -> Result<Value, String> {
    fql::query(entry["expression"].as_str().ok_or("缺少 FQL 表达式")?, root)
}
pub fn display(data: &Value, value: Value) -> Result<Value, String> {
    let mode = data["displayMode"].as_str().unwrap_or("json");
    match mode {
        "json" | "text" => {}
        "boolean" if value.is_boolean() => {}
        "table"
            if value
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_object)) => {}
        "line" => {
            let values = value.as_array().ok_or("折线图输入必须为数组")?;
            let x = data["xKey"].as_str().unwrap_or("x");
            let y = data["yKey"].as_str().unwrap_or("y");
            for (i, v) in values.iter().enumerate() {
                if !(v.is_number() || v.is_object() && v.get(x).is_some() && v[y].is_number()) {
                    return Err(format!(
                        "折线图第 {} 项需要为数字或包含 {x}、数值 {y} 的对象",
                        i + 1
                    ));
                }
            }
        }
        "boolean" => return Err("布尔展示输入必须为 boolean".into()),
        "table" => return Err("表格展示输入必须为对象数组".into()),
        _ => return Err(format!("不支持的展示格式: {mode}")),
    }
    Ok(
        json!({"mode":mode,"value":value,"xKey":data["xKey"].as_str().unwrap_or("x"),"yKey":data["yKey"].as_str().unwrap_or("y")}),
    )
}
pub async fn delay(value: &Value, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let ms = value
        .as_u64()
        .filter(|v| *v <= 600_000)
        .ok_or("Delay 必须为 0–600000 的整数毫秒")?;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("已取消".into());
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        tokio::time::sleep(remaining.min(std::time::Duration::from_millis(25))).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_constants_dates_and_variables() {
        let root = json!({"locals":{"empty":null}});
        for (kind, value) in [
            ("string", json!("中文\ntext")),
            ("bool", json!(false)),
            ("number", json!(0)),
        ] {
            assert_eq!(
                evaluate(&json!({"kind":kind,"value":value}), &root).unwrap(),
                value
            );
        }
        for (kind, value) in [
            ("string", json!(1)),
            ("bool", json!("false")),
            ("number", json!("1")),
        ] {
            assert!(evaluate(&json!({"kind":kind,"value":value}), &root).is_err());
        }
        assert_eq!(
            evaluate(&json!({"kind":"null"}), &root).unwrap(),
            Value::Null
        );
        assert_eq!(
            evaluate(&json!({"kind":"date","value":"1970-01-01"}), &root).unwrap(),
            0
        );
        assert_eq!(
            evaluate(
                &json!({"kind":"datetime","value":"1970-01-01T08:00:00+08:00"}),
                &root
            )
            .unwrap(),
            0
        );
        for (kind, value) in [("date", "2025-02-29"), ("datetime", "2026-11-06T12:00:00")] {
            assert!(evaluate(&json!({"kind":kind,"value":value}), &root).is_err());
        }
        let now = evaluate(&json!({"kind":"now"}), &root)
            .unwrap()
            .as_i64()
            .unwrap();
        assert!((chrono::Utc::now().timestamp_millis() - now).abs() < 1000);
        assert_eq!(
            evaluate(&json!({"kind":"getVariable","variableName":"empty"}), &root).unwrap(),
            Value::Null
        );
        assert!(
            evaluate(
                &json!({"kind":"getVariable","variableName":"missing"}),
                &root
            )
            .is_err()
        );
        assert_eq!(
            evaluate(&json!({"kind":"or"}), &json!({"input":false})).unwrap(),
            false
        );
    }
    #[test]
    fn schema_condition_and_response_semantics() {
        let schema = json!({"schema":{"type":"object","required":["id"],"properties":{"id":{"$ref":"#/$defs/id"}},"$defs":{"id":{"type":"integer","minimum":1}},"additionalProperties":false}});
        assert_eq!(validate(&schema, &json!({"id":2})).unwrap()["passed"], true);
        let invalid = validate(&schema, &json!({"id":0,"extra":true})).unwrap();
        assert_eq!(invalid["passed"], false);
        assert!(!invalid["errors"].as_array().unwrap().is_empty());
        assert!(validate(&json!({"schema":{"type":"invalid"}}), &Value::Null).is_err());
        assert!(
            validate(
                &json!({"schema":{"$ref":"https://example.com/schema.json"}}),
                &Value::Null
            )
            .is_err()
        );
        let cases = json!({"kind":"condition","conditions":[{"id":"a","expression":"input < 1"},{"id":"b","expression":"true"},{"id":"c","expression":"$unknown()"}]});
        assert_eq!(condition(&cases, &json!({"input":0})).unwrap(), "case-a");
        assert_eq!(condition(&cases, &json!({"input":5})).unwrap(), "case-b");
        assert_eq!(
            condition(&json!({"kind":"condition","conditions":[]}), &json!({})).unwrap(),
            "default"
        );
        assert!(
            condition(
                &json!({"kind":"condition","conditions":[{"id":"a","expression":"1"}]}),
                &json!({})
            )
            .is_err()
        );
        assert!(response(&json!({"statusCode":600,"headers":{}}), json!(1)).is_err());
        assert!(
            response(
                &json!({"statusCode":200,"headers":{"X-Test":"x\r\nbad"}}),
                json!(1)
            )
            .is_err()
        );
        assert_eq!(
            response(
                &json!({"statusCode":201,"headers":{"X-Test":"ok"}}),
                Value::Null
            )
            .unwrap()["body"],
            Value::Null
        );
    }
    #[test]
    fn construction_selection_and_strict_errors() {
        let root =
            json!({"input":{"rows":[{"id":7}],"a/b":{"~":null}},"variables":{"enabled":true}});
        assert_eq!(evaluate(&json!({"kind":"list","entries":[{"expression":"input.rows[0].id"},{"expression":"variables.enabled"}]}),&root).unwrap(),json!([7,true]));
        assert_eq!(evaluate(&json!({"kind":"record","entries":[{"key":"items","expression":"input.rows"},{"key":"enabled","expression":"variables.enabled"}]}),&root).unwrap()["enabled"],true);
        assert_eq!(
            evaluate(&json!({"kind":"select","path":"/rows/0/id"}), &root).unwrap(),
            7
        );
        assert_eq!(
            evaluate(&json!({"kind":"select","path":"/a~1b/~0"}), &root).unwrap(),
            Value::Null
        );
        assert!(evaluate(&json!({"kind":"select","path":"/missing"}), &root).is_err());
        assert!(evaluate(&json!({"kind":"record","entries":[{"key":"x","expression":"1"},{"key":"x","expression":"2"}]}),&root).unwrap_err().contains("重复"));
        assert!(
            evaluate(
                &json!({"kind":"list","entries":[{"expression":"$unknown(input)"}]}),
                &root
            )
            .unwrap_err()
            .contains("第 1 项")
        );
        assert_eq!(
            evaluate(&json!({"kind":"list","entries":[]}), &root).unwrap(),
            json!([])
        );
        assert_eq!(
            evaluate(&json!({"kind":"constant","value":null}), &root).unwrap(),
            Value::Null
        );
    }
    #[test]
    fn strict_display_types_and_empty_data() {
        for (mode, value) in [
            ("table", json!([])),
            ("line", json!([])),
            ("line", json!([1, 3, 2])),
            ("boolean", json!(false)),
            ("json", Value::Null),
        ] {
            assert!(display(&json!({"displayMode":mode}), value).is_ok());
        }
        assert!(
            display(
                &json!({"displayMode":"line","xKey":"name","yKey":"price"}),
                json!([{"name":"a","price":2}])
            )
            .is_ok()
        );
        for (mode, value) in [
            ("table", json!([1])),
            ("boolean", json!("false")),
            ("line", json!([{"x":1,"y":"2"}])),
        ] {
            assert!(display(&json!({"displayMode":mode}), value).is_err());
        }
    }
    #[test]
    fn delay_limits_zero_and_cancellation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        rt.block_on(delay(&json!(0), &cancel)).unwrap();
        for value in [json!(-1), json!(1.5), json!(600001), json!("1")] {
            assert!(rt.block_on(delay(&value, &cancel)).is_err());
        }
        let cloned = cancel.clone();
        let task = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            cloned.store(true, Ordering::SeqCst);
        });
        let started = std::time::Instant::now();
        assert!(rt.block_on(delay(&json!(10000), &cancel)).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        task.join().unwrap();
    }
}
