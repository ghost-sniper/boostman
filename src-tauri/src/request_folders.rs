use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Folder {
    id: String,
    name: String,
    kind: String,
    #[serde(default)]
    parent_id: Option<String>,
}
pub fn normalize(workspace: &mut Value) -> Result<(), String> {
    if !workspace.is_object() {
        return Err("工作区必须为对象".into());
    }
    if workspace.get("requestFolders").is_none() {
        workspace["requestFolders"] = json!([]);
    }
    for folder in workspace["requestFolders"]
        .as_array_mut()
        .ok_or("requestFolders 必须为数组")?
    {
        folder
            .as_object_mut()
            .ok_or("请求目录必须为对象")?
            .entry("parentId")
            .or_insert(Value::Null);
    }
    Ok(())
}
pub fn validate(workspace: &Value) -> Result<(), String> {
    let folders: Vec<Folder> = serde_json::from_value(workspace["requestFolders"].clone())
        .map_err(|e| format!("请求目录格式无效: {e}"))?;
    let mut lookup = HashMap::new();
    for folder in &folders {
        if folder.id.is_empty() || folder.name.trim().is_empty() {
            return Err("请求目录 ID 和名称不能为空".into());
        }
        if !["http", "sql"].contains(&folder.kind.as_str()) {
            return Err("请求目录类型必须为 http/sql".into());
        }
        if lookup.insert(folder.id.as_str(), folder).is_some() {
            return Err("请求目录 ID 重复".into());
        }
    }
    for folder in &folders {
        let mut seen = HashSet::new();
        let mut current = Some(folder.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err("请求目录不能包含循环或移入自身".into());
            }
            if seen.len() > 32 {
                return Err("请求目录嵌套不能超过 32 层".into());
            }
            let parent = lookup.get(id).ok_or("请求目录的上级不存在")?;
            if parent.kind != folder.kind {
                return Err("HTTP 与 SQL 目录不能跨类型嵌套".into());
            }
            current = parent.parent_id.as_deref();
        }
    }
    for request in workspace["requests"]
        .as_array()
        .ok_or("requests 必须为数组")?
    {
        if let Some(id) = request.get("folderId").filter(|id| !id.is_null()) {
            let id = id.as_str().ok_or("请求的 folderId 必须为字符串或 null")?;
            let folder = lookup.get(id).ok_or("请求所属目录不存在")?;
            if request["kind"].as_str() != Some(folder.kind.as_str()) {
                return Err("请求只能放入同类型目录".into());
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        json!({"requests":[{"id":"r","kind":"http","folderId":"child"}],"requestFolders":[{"id":"parent","name":"用户组件","kind":"http","parentId":null},{"id":"child","name":"登录","kind":"http","parentId":"parent"},{"id":"sql","name":"用户组件","kind":"sql","parentId":null}]})
    }
    #[test]
    fn nested_and_legacy() {
        validate(&sample()).unwrap();
        let mut old = json!({"requests":[{"id":"legacy","kind":"http"}]});
        normalize(&mut old).unwrap();
        validate(&old).unwrap();
        assert_eq!(old["requestFolders"], json!([]));
        assert_eq!(old["requests"][0]["id"], "legacy");
    }
    #[test]
    fn reject_cycles_and_cross_kind() {
        let mut data = sample();
        data["requestFolders"][0]["parentId"] = json!("child");
        assert!(validate(&data).unwrap_err().contains("循环"));
        let mut data = sample();
        data["requests"][0]["folderId"] = json!("sql");
        assert!(validate(&data).unwrap_err().contains("同类型"));
        let mut data = sample();
        data["requestFolders"][1]["parentId"] = json!("sql");
        assert!(validate(&data).unwrap_err().contains("跨类型"));
        let mut data = sample();
        data["requests"][0]["folderId"] = json!("missing");
        assert!(validate(&data).unwrap_err().contains("不存在"));
    }
}
