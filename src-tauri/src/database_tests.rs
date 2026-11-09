//! Opt-in integration tests. Each creates and drops a uniquely named table in a dedicated test DB.
use crate::{engine, sql};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::{Arc, atomic::AtomicBool},
};
struct Fixture {
    connection: Value,
    table: String,
    secret_id: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            let _ = rt.block_on(sql::execute(
                &json!({"sql":format!("DROP TABLE {}{}",if self.connection["driver"]=="oracle"{""}else{"IF EXISTS "},self.table)}),
                &self.connection,
                &AtomicBool::new(false),
            ));
        }
        if let Ok(entry) = keyring::Entry::new("boostman", &self.secret_id) {
            let _ = entry.delete_credential();
        }
    }
}
fn integration(driver: &str, variable: &str) {
    let mut connection: Value =
        serde_json::from_str(&std::env::var(variable).expect("请按 README 设置数据库测试配置"))
            .expect("测试配置必须为 JSON");
    let secret_id = format!("boostman-test-{}", uuid::Uuid::new_v4());
    let password = connection
        .as_object_mut()
        .unwrap()
        .remove("password")
        .unwrap_or(json!(""));
    keyring::Entry::new("boostman", &secret_id)
        .unwrap()
        .set_password(password.as_str().expect("password 必须为字符串"))
        .unwrap();
    connection["secretId"] = json!(secret_id);
    connection["driver"] = json!(driver);
    connection["id"] = json!("db");
    let fixture = Fixture {
        connection,
        secret_id,
        table: format!("boostman_it_{}", uuid::Uuid::new_v4().simple()),
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let query = |text: String, params: Value| {
        rt.block_on(sql::execute(
            &json!({"sql":text,"params":params}),
            &fixture.connection,
            &AtomicBool::new(false),
        ))
    };
    query(format!("CREATE TABLE {} (id INTEGER PRIMARY KEY, amount DECIMAL(30,10), processed BOOLEAN, note VARCHAR(100))",fixture.table),json!([])).unwrap();
    let marks = if driver == "postgres" {
        "$1,$2,$3,$4"
    } else {
        "?,?,?,?"
    };
    for id in [1, 2] {
        query(
            format!(
                "INSERT INTO {} (id,amount,processed,note) VALUES ({marks})",
                fixture.table
            ),
            json!([id,{"type":"decimal","value":"9007199254740992.1234567890"},false,null]),
        )
        .unwrap();
    }
    let by_id = format!("SELECT id FROM {} WHERE id = $1", fixture.table);
    for params in [json!([1]), json!([{"type":"u64","value":"1"}])] {
        let result = query(by_id.clone(), params).unwrap();
        assert_eq!(result["data"]["rows"], json!([{"id":1}]));
    }
    let reordered = query(
        "SELECT $2 AS second, $1 AS first, $1 AS repeated".into(),
        json!(["one", "two"]),
    )
    .unwrap();
    assert_eq!(
        reordered["data"]["rows"],
        json!([{"second":"two","first":"one","repeated":"one"}])
    );
    if driver == "postgres" {
        let converted = query(
            "SELECT $1::smallint AS small, $2::real AS fraction, $3::json AS document, $4::bytea AS bytes, $5::date AS day".into(),
            json!(["-2", 1.25, {"type":"json","value":{"ok":true}}, "AP8=", "2026-11-09"]),
        ).unwrap();
        assert_eq!(
            converted["data"]["rows"],
            json!([{"small":-2,"fraction":1.25,"document":{"ok":true},"bytes":"AP8=","day":"2026-11-09"}])
        );
        assert!(
            query(by_id, json!([2_147_483_648_u64]))
                .unwrap_err()
                .contains("参数 $1 无法转换为 PostgreSQL INT4")
        );
    }
    let got = query(
        format!("SELECT id,amount,note FROM {} ORDER BY id", fixture.table),
        json!([]),
    )
    .unwrap();
    assert_eq!(
        got["data"]["rows"][0]["amount"]
            .as_str()
            .unwrap()
            .parse::<sqlx::types::BigDecimal>()
            .unwrap(),
        "9007199254740992.1234567890"
            .parse::<sqlx::types::BigDecimal>()
            .unwrap()
    );
    assert!(got["data"]["rows"][0]["note"].is_null());
    let empty = query(
        format!("SELECT id FROM {} WHERE 1=0", fixture.table),
        json!([]),
    )
    .unwrap();
    assert_eq!(empty["data"]["rows"], json!([]));
    assert_eq!(empty["data"]["columns"][0]["name"], "id");
    assert!(
        query(
            format!(
                "SELECT id AS duplicate,id AS duplicate FROM {}",
                fixture.table
            ),
            json!([])
        )
        .unwrap_err()
        .contains("重复列名")
    );
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = server.local_addr().unwrap();
    let server_thread = std::thread::spawn(move || {
        for stream in server.incoming().take(2) {
            let mut stream = stream.unwrap();
            let mut buf = [0; 2048];
            let _ = stream.read(&mut buf);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"active\":true}").unwrap();
        }
    });
    let node = |id: &str, kind: &str, scope: &str, expression: &str| json!({"id":id,"data":{"kind":kind,"scope":scope,"expression":expression}});
    let mut read = node("read", "sql", "root", "");
    read["data"]["request"] = json!({"kind":"sql","connectionId":"db","sql":format!("SELECT id FROM {} ORDER BY id",fixture.table),"params":[]});
    let mut send = node("send", "http", "each:body", "");
    send["data"]["request"] = json!({"kind":"http","url":format!("http://{addr}"),"bindings":{"query":"[{\"key\":\"id\",\"value\":$json(item.id)}]"}});
    let mut update = node("update", "sql", "check:then", "");
    update["data"]["request"] = json!({"kind":"sql","connectionId":"db","sql":format!("UPDATE {} SET processed = TRUE WHERE id = {}",fixture.table,if driver=="postgres"{"$1"}else{"?"}),"bindings":{"params":"[item.id]"}});
    let flow = json!({"nodes":[node("start","start","root",""),read,node("each","foreach","root","input.rows"),send,node("check","if","each:body","input.active = true"),update,node("summary","evaluate","root","{\"processed\":$count(input)}"),node("end","end","root","")],"edges":[{"source":"start","target":"read"},{"source":"read","target":"each"},{"source":"each","target":"summary"},{"source":"summary","target":"end"},{"source":"send","target":"check"}]});
    let result = engine::execute(
        &|_| {},
        None,
        Arc::new(AtomicBool::new(false)),
        json!({"connections":[fixture.connection.clone()],"requests":[],"collections":[],"environments":[]}),
        "flow".into(),
        flow,
        json!({}),
    );
    assert_eq!(result["status"], "succeeded", "{result}");
    server_thread.join().unwrap();
    assert_eq!(result["data"]["processed"], 2);
    let checked = query(
        format!(
            "SELECT COUNT(*) AS n FROM {} WHERE processed = TRUE",
            fixture.table
        ),
        json!([]),
    )
    .unwrap();
    assert_eq!(checked["data"]["rows"][0]["n"], 2);
    let mark = if driver == "postgres" { "$1" } else { "?" };
    let removed = query(
        format!("DELETE FROM {} WHERE id = {mark}", fixture.table),
        json!([2]),
    )
    .unwrap();
    assert_eq!(removed["data"]["affectedRows"], 1);
    let remaining = query(
        format!("SELECT COUNT(*) AS n FROM {}", fixture.table),
        json!([]),
    )
    .unwrap();
    assert_eq!(remaining["data"]["rows"][0]["n"], 1);
    if driver == "postgres" {
        let typed=query("SELECT 9007199254740992::bigint AS large_id, TRUE AS enabled, DATE '2026-11-06' AS day, TIMESTAMP '2026-11-06 12:34:56' AS moment, TIMESTAMPTZ '2026-11-06 12:34:56+00' AS zoned, decode('00ff','hex') AS binary, '{\"a\":1}'::jsonb AS document".into(),json!([])).unwrap();
        let row = &typed["data"]["rows"][0];
        assert_eq!(row["large_id"], "9007199254740992");
        assert_eq!(row["enabled"], true);
        assert_eq!(row["day"], "2026-11-06");
        assert_eq!(row["moment"], "2026-11-06T12:34:56");
        assert_eq!(row["zoned"], "2026-11-06T12:34:56+00:00");
        assert_eq!(row["binary"], "AP8=");
        assert_eq!(row["document"], json!({"a":1}));
    }
    // Exercise driver cancellation while an actual statement is in flight.
    let slow = if driver == "postgres" {
        "SELECT pg_sleep(2)"
    } else {
        "SELECT SLEEP(2)"
    };
    let err = rt
        .block_on(sql::execute(
            &json!({"sql":slow,"timeoutMs":25}),
            &fixture.connection,
            &AtomicBool::new(false),
        ))
        .unwrap_err();
    assert_eq!(err, "执行超时");
    query(format!("DROP TABLE {}", fixture.table), json!([])).unwrap();
    let cleanup = query(
        format!("SELECT COUNT(*) AS n FROM information_schema.tables WHERE table_name = {mark}"),
        json!([fixture.table]),
    )
    .unwrap();
    assert_eq!(cleanup["data"]["rows"][0]["n"], 0, "测试表未清理");
    drop(rt);
    drop(fixture);
}
#[test]
#[ignore = "需要专用 PostgreSQL 测试数据库，见 README"]
fn postgres_end_to_end() {
    integration("postgres", "BOOSTMAN_TEST_POSTGRES")
}
#[test]
#[ignore = "需要专用 MySQL 测试数据库，见 README"]
fn mysql_end_to_end() {
    integration("mysql", "BOOSTMAN_TEST_MYSQL")
}

#[test]
#[ignore = "需要 Oracle Instant Client 和专用 Oracle 测试实例，见 README"]
fn oracle_end_to_end() {
    let mut connection: Value =
        serde_json::from_str(&std::env::var("BOOSTMAN_TEST_ORACLE").expect("缺少 Oracle 测试配置"))
            .unwrap();
    let password = connection
        .as_object_mut()
        .unwrap()
        .remove("password")
        .unwrap();
    let secret_id = format!("boostman-oracle-test-{}", uuid::Uuid::new_v4());
    keyring::Entry::new("boostman", &secret_id)
        .unwrap()
        .set_password(password.as_str().unwrap())
        .unwrap();
    connection["id"] = json!("db");
    connection["driver"] = json!("oracle");
    connection["secretId"] = json!(secret_id);
    let fixture = Fixture {
        connection,
        secret_id,
        table: format!("bm_it_{}", &uuid::Uuid::new_v4().simple().to_string()[..20]),
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(sql::test(&fixture.connection)).unwrap();
    let query = |sql: String, params: Value| {
        rt.block_on(sql::execute(
            &json!({"sql":sql,"params":params}),
            &fixture.connection,
            &AtomicBool::new(false),
        ))
    };
    query(format!("CREATE TABLE {} (id NUMBER(19) PRIMARY KEY, amount NUMBER(30,10), note VARCHAR2(100), processed NUMBER(1))",fixture.table),json!([])).unwrap();
    for id in [1, 2] {
        let inserted=query(format!("INSERT INTO {} (id,amount,note,processed) VALUES (:1,:2,:3,:4)",fixture.table),json!([id,{"type":"decimal","value":"9007199254740992.1234567890"},{"type":"string","value":null},false])).unwrap();
        assert_eq!(inserted["data"]["affectedRows"], 1);
    }
    let read = query(
        format!(
            "SELECT id AS \"id\", amount AS \"amount\", note AS \"note\" FROM {} ORDER BY id",
            fixture.table
        ),
        json!([]),
    )
    .unwrap();
    assert_eq!(
        read["data"]["rows"][0]["amount"]
            .as_str()
            .unwrap()
            .parse::<sqlx::types::BigDecimal>()
            .unwrap(),
        "9007199254740992.1234567890"
            .parse::<sqlx::types::BigDecimal>()
            .unwrap()
    );
    assert!(read["data"]["rows"][0]["note"].is_null());
    let empty = query(
        format!("SELECT id AS \"id\" FROM {} WHERE 1=0", fixture.table),
        json!([]),
    )
    .unwrap();
    assert_eq!(empty["data"]["rows"], json!([]));
    assert_eq!(empty["data"]["columns"][0]["name"], "id");
    let repeated = query(
        "SELECT :2 AS \"second\", :1 AS \"first\", :1 AS \"again\" FROM dual".into(),
        json!([11, 22]),
    )
    .unwrap();
    assert_eq!(
        repeated["data"]["rows"][0],
        json!({"second":22,"first":11,"again":11})
    );
    let portable = query(
        format!("SELECT id AS \"id\" FROM {} WHERE id = $1", fixture.table),
        json!([{"type":"u64","value":"1"}]),
    )
    .unwrap();
    assert_eq!(portable["data"]["rows"], json!([{"id":1}]));
    assert!(
        query(
            format!(
                "SELECT id AS \"duplicate\", id AS \"duplicate\" FROM {}",
                fixture.table
            ),
            json!([])
        )
        .unwrap_err()
        .contains("重复列名")
    );
    let typed=query("SELECT CAST(9007199254740992 AS NUMBER(19)) AS \"large\", DATE '2026-11-06' AS \"day\", HEXTORAW('00FF') AS \"binary\" FROM dual".into(),json!([])).unwrap();
    assert_eq!(typed["data"]["rows"][0]["large"], "9007199254740992");
    assert_eq!(typed["data"]["rows"][0]["day"], "2026-11-06T00:00:00");
    assert_eq!(typed["data"]["rows"][0]["binary"], "AP8=");
    let node = |id: &str, kind: &str, scope: &str, expression: &str| json!({"id":id,"data":{"kind":kind,"scope":scope,"expression":expression}});
    let mut read_node = node("read", "sql", "root", "");
    read_node["data"]["request"] = json!({"kind":"sql","connectionId":"db","sql":format!("SELECT id AS \"id\" FROM {} ORDER BY id",fixture.table),"params":[]});
    let mut update = node("update", "sql", "each:body", "");
    update["data"]["request"] = json!({"kind":"sql","connectionId":"db","sql":format!("UPDATE {} SET processed=1 WHERE id=:1",fixture.table),"bindings":{"params":"[item.id]"}});
    let flow = json!({"nodes":[node("start","start","root",""),read_node,node("each","foreach","root","input.rows"),update,node("summary","evaluate","root","{\"processed\":$count(input)}"),node("end","end","root","")],"edges":[{"source":"start","target":"read"},{"source":"read","target":"each"},{"source":"each","target":"summary"},{"source":"summary","target":"end"}]});
    let result = engine::execute(
        &|_| {},
        None,
        Arc::new(AtomicBool::new(false)),
        json!({"connections":[fixture.connection.clone()],"requests":[],"collections":[],"environments":[]}),
        "flow".into(),
        flow,
        json!({}),
    );
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["data"]["processed"], 2);
    let updated = query(
        format!(
            "SELECT COUNT(*) AS \"n\" FROM {} WHERE processed=1",
            fixture.table
        ),
        json!([]),
    )
    .unwrap();
    assert_eq!(updated["data"]["rows"][0]["n"], 2);
    let deleted = query(
        format!("DELETE FROM {} WHERE id=:1", fixture.table),
        json!([2]),
    )
    .unwrap();
    assert_eq!(deleted["data"]["affectedRows"], 1);
    query(format!("DROP TABLE {}", fixture.table), json!([])).unwrap();
    let cleanup = query(
        "SELECT COUNT(*) AS \"n\" FROM user_tables WHERE table_name=:1".into(),
        json!([fixture.table.to_uppercase()]),
    )
    .unwrap();
    assert_eq!(cleanup["data"]["rows"][0]["n"], 0);
}

#[test]
#[ignore = "需要本地 tags HTTP 接口和 PostgreSQL 配置"]
fn postgres_tags_cross_validation() {
    struct Credential(String);
    impl Drop for Credential {
        fn drop(&mut self) {
            if let Ok(e) = keyring::Entry::new("boostman", &self.0) {
                let _ = e.delete_credential();
            }
        }
    }
    let mut connection: Value = serde_json::from_str(
        &std::env::var("BOOSTMAN_TEST_POSTGRES").expect("缺少 PostgreSQL 测试配置"),
    )
    .unwrap();
    let password = connection
        .as_object_mut()
        .unwrap()
        .remove("password")
        .unwrap();
    let credential = Credential(format!("boostman-tags-test-{}", uuid::Uuid::new_v4()));
    keyring::Entry::new("boostman", &credential.0)
        .unwrap()
        .set_password(password.as_str().unwrap())
        .unwrap();
    connection["id"] = json!("db");
    connection["driver"] = json!("postgres");
    connection["secretId"] = json!(credential.0);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let candidates=rt.block_on(sql::execute(&json!({"sql":"SELECT table_schema, table_name FROM information_schema.columns WHERE column_name IN ('id','tag_name','status') AND table_schema NOT IN ('pg_catalog','information_schema') GROUP BY table_schema,table_name HAVING COUNT(DISTINCT column_name)=3 ORDER BY table_schema,table_name","params":[]}),&connection,&AtomicBool::new(false))).unwrap();
    let tables = candidates["data"]["rows"].as_array().unwrap();
    println!("候选标签数据表: {}", json!(tables));
    let preferred: Vec<_> = tables
        .iter()
        .filter(|t| ["tag", "tags"].contains(&t["table_name"].as_str().unwrap_or("")))
        .collect();
    let table = if preferred.len() == 1 {
        preferred[0]
    } else if tables.len() == 1 {
        &tables[0]
    } else {
        panic!("标签表不唯一或不存在，需指定对应数据表")
    };
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let table_name = format!(
        "{}.{}",
        quote(table["table_schema"].as_str().unwrap()),
        quote(table["table_name"].as_str().unwrap())
    );
    let compare = r#"{
        "countsMatch": nodes.api.data.count = $count(nodes.api.data.tags) and nodes.api.data.count = $count(nodes.db.data.rows),
        "tagMismatches": $filter(nodes.api.data.tags, fn($tag){ $count($filter(nodes.db.data.rows, fn($row){ $row.id = $tag.id and $row.tag_name = $tag.tag_name and $row.status = $tag.status })) != 1 }),
        "sqlOnly": $filter(nodes.db.data.rows, fn($row){ $count($filter(nodes.api.data.tags, fn($tag){ $tag.id = $row.id })) != 1 }),
        "suggestionMismatches": $filter(nodes.api.data.suggestions, fn($tag){ $count($filter(nodes.db.data.rows, fn($row){ $row.id = $tag.id and $row.tag_name = $tag.tag_name })) != 1 })
    }"#;
    let summarize = r#"{"passed": input.countsMatch and $count(input.tagMismatches) = 0 and $count(input.sqlOnly) = 0 and $count(input.suggestionMismatches) = 0, "httpCount":nodes.api.data.count, "sqlCount":$count(nodes.db.data.rows), "details":input}"#;
    let flow = json!({"id":"tags-cross-validation","name":"HTTP 标签与 PostgreSQL FQL 交叉验证","nodes":[
        {"id":"start","type":"boost","position":{"x":0,"y":100},"data":{"kind":"start","label":"开始","scope":"root"}},
        {"id":"api","type":"boost","position":{"x":260,"y":100},"data":{"kind":"http","label":"读取 tags 接口","scope":"root","request":{"id":"tags-http","kind":"http","method":"GET","url":"http://localhost:9000/api/tags"}}},
        {"id":"db","type":"boost","position":{"x":520,"y":100},"data":{"kind":"sql","label":"读取数据库标签","scope":"root","request":{"id":"tags-sql","kind":"sql","connectionId":"db","sql":format!("SELECT id, tag_name, status FROM {table_name} ORDER BY id"),"params":[]}}},
        {"id":"compare","type":"boost","position":{"x":780,"y":100},"data":{"kind":"evaluate","label":"FQL 逐条比较","scope":"root","expression":compare}},
        {"id":"summary","type":"boost","position":{"x":1040,"y":100},"data":{"kind":"evaluate","label":"FQL 汇总判定","scope":"root","expression":summarize}},
        {"id":"end","type":"boost","position":{"x":1300,"y":100},"data":{"kind":"end","label":"结束","scope":"root"}}
    ],"edges":[{"id":"1","source":"start","target":"api"},{"id":"2","source":"api","target":"db"},{"id":"3","source":"db","target":"compare"},{"id":"4","source":"compare","target":"summary"},{"id":"5","source":"summary","target":"end"}]});
    let result = engine::execute(
        &|_| {},
        None,
        Arc::new(AtomicBool::new(false)),
        json!({"connections":[connection],"requests":[],"collections":[],"environments":[]}),
        "flow".into(),
        flow.clone(),
        json!({}),
    );
    assert_eq!(result["status"], "succeeded", "{result}");
    let mut changed = result["context"].clone();
    if let Some(first) = changed["nodes"]["db"]["data"]["rows"]
        .as_array_mut()
        .unwrap()
        .first_mut()
    {
        first["tag_name"] = json!("__deliberate_mismatch__");
    } else {
        changed["nodes"]["db"]["data"]["rows"] =
            json!([{"id":-1,"tag_name":"__deliberate_mismatch__","status":0}]);
    }
    changed["input"] = crate::fql::query(compare, &changed).unwrap();
    let negative = crate::fql::query(summarize, &changed).unwrap();
    assert_eq!(
        negative["passed"], false,
        "交叉验证必须能检测故意构造的数据差异"
    );
    let report = json!({"verifiedAt":chrono::Utc::now().to_rfc3339(),"endpoint":"http://localhost:9000/api/tags","database":{"host":"127.0.0.1","port":5432,"database":"zion","table":table_name},"summary":result["data"],"httpTags":result["context"]["nodes"]["api"]["data"],"sqlRows":result["context"]["nodes"]["db"]["data"]["rows"],"negativeControl":negative,"flow":flow});
    let artifacts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    std::fs::write(
        artifacts.join("tags-cross-validation.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("FQL 交叉验证结果: {}", result["data"]);
    println!("故意修改内存中的 SQL 标签后: passed={}", negative["passed"]);
    assert_eq!(
        result["data"]["passed"], true,
        "接口与数据库存在差异，详见 artifacts/tags-cross-validation.json"
    );
}
