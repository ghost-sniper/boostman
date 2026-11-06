use serde_json::{Value, json};
use sqlx::{
    Row,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::path::Path;
pub async fn open(path: &Path) -> Result<sqlx::SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true),
        )
        .await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), document TEXT NOT NULL)").execute(&pool).await?;
    Ok(pool)
}
pub async fn load(pool: &sqlx::SqlitePool) -> Result<Value, String> {
    match sqlx::query("SELECT document FROM workspace WHERE id=1")
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(row) => {
            let mut value: Value =
                serde_json::from_str(row.get::<&str, _>(0)).map_err(|e| e.to_string())?;
            normalize_connections(&mut value)?;
            Ok(value)
        }
        None => Ok(
            json!({"version":1,"requests":[],"collections":[],"environments":[],"connections":[],"flows":[]}),
        ),
    }
}
pub async fn save(pool: &sqlx::SqlitePool, value: &Value) -> Result<(), String> {
    let mut value = value.clone();
    if value["version"] != 1 {
        return Err("不支持的工作区版本".into());
    }
    for key in [
        "requests",
        "collections",
        "environments",
        "connections",
        "flows",
    ] {
        if !value[key].is_array() {
            return Err(format!("{key} 必须为数组"));
        }
    }
    let mut ids = std::collections::HashSet::new();
    normalize_connections(&mut value)?;
    for key in [
        "requests",
        "collections",
        "environments",
        "connections",
        "flows",
    ] {
        for item in value[key].as_array().ok_or("工作区列表无效")? {
            let id = item["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or("条目缺少 ID")?;
            if !ids.insert(id) {
                return Err(format!("工作区 ID 重复: {id}"));
            }
            if !item["name"].is_string() {
                return Err("条目名称必须为字符串".into());
            }
            if ["environments", "collections"].contains(&key) && !item["variables"].is_object() {
                return Err("变量必须为 JSON 对象".into());
            }
            if key == "collections" && !item["items"].is_array() {
                return Err("集合 items 必须为数组".into());
            }
            if key == "requests" && item["kind"] != "http" && item["kind"] != "sql" {
                return Err("请求 kind 必须为 http/sql".into());
            }
            if key == "connections"
                && !["mysql", "postgres", "oracle"].contains(&item["driver"].as_str().unwrap_or(""))
            {
                return Err("不支持的数据库类型".into());
            }
        }
    }
    sqlx::query("INSERT INTO workspace(id,document) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET document=excluded.document").bind(value.to_string()).execute(pool).await.map_err(|e|e.to_string())?;
    Ok(())
}
fn normalize_connections(workspace: &mut Value) -> Result<(), String> {
    for config in workspace["connections"]
        .as_array_mut()
        .ok_or("connections 必须为数组")?
    {
        crate::db_config::normalize(config)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn legacy_connections_are_read_without_losing_references() {
        let pool = open(Path::new(":memory:")).await.unwrap();
        let legacy = json!({"version":1,"requests":[],"collections":[],"environments":[],"flows":[],"connections":[{"id":"old-mysql","name":"MySQL","driver":"mysql","host":"db","database":"sample","username":"root","secretId":"keep-secret","customLabel":"keep"},{"id":"old-oracle","name":"Oracle","driver":"oracle","host":"db","port":1522,"database":"ORCL","username":"user","secretId":"old-password","oracleMode":"sid"}]});
        sqlx::query("INSERT INTO workspace(id,document) VALUES(1,?)")
            .bind(legacy.to_string())
            .execute(&pool)
            .await
            .unwrap();
        let loaded = load(&pool).await.unwrap();
        assert_eq!(loaded["connections"][0]["port"], 3306);
        assert_eq!(loaded["connections"][0]["secretId"], "keep-secret");
        assert_eq!(loaded["connections"][0]["customLabel"], "keep");
        assert_eq!(loaded["connections"][1], legacy["connections"][1]);
        let raw: String = sqlx::query_scalar("SELECT document FROM workspace WHERE id=1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), legacy);
        save(&pool, &loaded).await.unwrap();
        assert_eq!(load(&pool).await.unwrap(), loaded);
        pool.close().await;
    }
    #[tokio::test]
    async fn reopen_and_version() {
        let path = std::env::temp_dir().join(format!("boostman-{}.sqlite", uuid::Uuid::new_v4()));
        let pool = open(&path).await.unwrap();
        let mut w = load(&pool).await.unwrap();
        w["environments"] = json!([{"id":"env","name":"test","variables":{"token":"abc"}}]);
        w["connections"] = json!([{"id":"oracle-test","name":"Oracle","driver":"oracle","host":"localhost","port":1521,"database":"FREEPDB1","oracleMode":"service","secretId":"password-reference"}]);
        save(&pool, &w).await.unwrap();
        pool.close().await;
        let reopened = open(&path).await.unwrap();
        normalize_connections(&mut w).unwrap();
        assert_eq!(load(&reopened).await.unwrap(), w);
        w["version"] = json!(99);
        assert!(save(&reopened, &w).await.is_err());
        reopened.close().await;
        drop(reopened);
        drop(pool);
        // SQLite's worker can release the Windows file handle just after pool.close resolves.
        for attempt in 0..50 {
            match std::fs::remove_file(&path) {
                Ok(()) => break,
                Err(e) if attempt < 49 && e.raw_os_error() == Some(32) => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await
                }
                Err(e) => panic!("临时数据库清理失败: {e}"),
            }
        }
    }
}
