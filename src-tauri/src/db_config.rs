//! Shared configuration contract for every database adapter (wire names remain compatible).
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    Mysql,
    Postgres,
    Oracle,
}
impl Driver {
    pub fn port(self) -> u16 {
        match self {
            Self::Mysql => 3306,
            Self::Postgres => 5432,
            Self::Oracle => 1521,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OracleMode {
    #[default]
    Service,
    Sid,
}
fn localhost() -> String {
    "localhost".into()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub driver: Driver,
    #[serde(default = "localhost")]
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub secret_id: String,
    #[serde(default)]
    pub oracle_mode: OracleMode,
}
impl ConnectionConfig {
    pub fn read(value: &Value) -> Result<Self, String> {
        let mut value = value.clone();
        let driver: Driver = serde_json::from_value(value["driver"].clone())
            .map_err(|_| "数据库类型必须为 mysql、postgres 或 oracle")?;
        if value.get("port").is_none() {
            value["port"] = serde_json::json!(driver.port());
        }
        let mut config: Self =
            serde_json::from_value(value).map_err(|e| format!("数据库连接配置格式无效: {e}"))?;
        if config.port == 0 {
            return Err("数据库端口必须在 1–65535 之间".into());
        }
        config.host = config.host.trim().to_owned();
        config.database = config.database.trim().to_owned();
        config.username = config.username.trim().to_owned();
        config.secret_id = config.secret_id.trim().to_owned();
        Ok(config)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_address()?;
        for (name, value) in [("用户名", &self.username), ("密码凭据 ID", &self.secret_id)] {
            if value.is_empty() {
                return Err(format!("数据库连接缺少{name}"));
            }
        }
        Ok(())
    }
    pub fn validate_address(&self) -> Result<(), String> {
        for (name, value) in [("主机", &self.host), ("数据库 / 服务名称", &self.database)]
        {
            if value.is_empty() {
                return Err(format!("数据库连接缺少{name}"));
            }
        }
        if self
            .host
            .chars()
            .any(|ch| ch.is_whitespace() || "()=/\\\"".contains(ch))
        {
            return Err("数据库主机格式无效".into());
        }
        if self.driver == Driver::Oracle
            && !self
                .database
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "_.-$#".contains(ch))
        {
            return Err("Oracle Service Name / SID 格式无效".into());
        }
        Ok(())
    }
    pub fn password(&self) -> Result<String, String> {
        crate::http::secret(&self.secret_id)
    }
}
pub fn normalize(value: &mut Value) -> Result<(), String> {
    let fields = serde_json::to_value(ConnectionConfig::read(value)?).map_err(|e| e.to_string())?;
    value
        .as_object_mut()
        .ok_or("数据库连接必须为对象")?
        .extend(fields.as_object().ok_or("数据库连接字段无效")?.clone());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn common_defaults_and_validation() {
        for (driver, port) in [("mysql", 3306), ("postgres", 5432), ("oracle", 1521)] {
            let mut value = json!({"driver":driver,"host":"  localhost  ","database":" sample ","username":" user ","secretId":" secret ","name":"保留名称","id":"saved"});
            normalize(&mut value).unwrap();
            let config = ConnectionConfig::read(&value).unwrap();
            config.validate().unwrap();
            assert_eq!(config.port, port);
            assert_eq!(config.host, "localhost");
            assert_eq!(value["id"], "saved");
            assert_eq!(value["oracleMode"], "service");
            for bad in [json!(0), json!(-1), json!(65536), json!("5432")] {
                value["port"] = bad;
                assert!(ConnectionConfig::read(&value).is_err());
            }
        }
    }
    #[test]
    fn old_sid_and_secrets_survive() {
        let mut v = json!({"driver":"oracle","host":"db","port":1522,"database":"ORCL","username":"test","secretId":"existing-reference","oracleMode":"sid"});
        let before = v.clone();
        normalize(&mut v).unwrap();
        assert_eq!(v, before);
        assert_eq!(
            ConnectionConfig::read(&v).unwrap().oracle_mode,
            OracleMode::Sid
        );
    }
    #[test]
    fn drafts_and_required_fields() {
        let v = json!({"driver":"postgres"});
        let config = ConnectionConfig::read(&v).unwrap();
        assert!(config.validate().unwrap_err().contains("数据库 / 服务名称"));
        assert!(ConnectionConfig::read(&json!({"driver":"unknown"})).is_err());
        assert!(ConnectionConfig::read(&json!({"driver":"mysql","oracleMode":"invalid"})).is_err());
    }
}
