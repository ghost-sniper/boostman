//! Real, read-only HTTP + PostgreSQL controller verification. No business data is changed.
use crate::{engine, sql};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
struct Credential(String);
impl Drop for Credential {
    fn drop(&mut self) {
        if let Ok(entry) = keyring::Entry::new("boostman", &self.0) {
            let _ = entry.delete_credential();
        }
    }
}
fn node(id: &str, kind: &str, scope: &str, config: Value) -> Value {
    let mut data = json!({"kind":kind,"label":id,"scope":scope});
    data.as_object_mut()
        .unwrap()
        .extend(config.as_object().unwrap().clone());
    json!({"id":id,"type":"boost","position":{"x":100,"y":100},"data":data})
}
fn link(a: &str, b: &str) -> Value {
    json!({"id":format!("{a}-{b}"),"source":a,"target":b})
}
fn flow(table: &str) -> Value {
    let initialize = r#"const body=pm.response.json();pm.test('HTTP 200',()=>pm.expect(pm.response.code).to.equal(200));pm.test('标签数量一致',()=>pm.expect(body.tags).to.have.lengthOf(body.count));for(const name of ['visited','trueCount','falseCount','whileCount','whileTotal','repeatCount','matchedCount'])pm.variables.set(name,0);console.log('HTTP 标签',body);"#;
    let compare = r#"{"countsMatch": nodes.api.data.count = $count(nodes.api.data.tags) and nodes.api.data.count = $count(nodes.db.data.rows), "mismatches":$filter(nodes.api.data.tags,fn($tag){$count($filter(nodes.db.data.rows,fn($row){$row.id=$tag.id and $row.tag_name=$tag.tag_name and $row.status=$tag.status})) != 1}), "suggestionMismatches":$filter(nodes.api.data.suggestions,fn($tag){$count($filter(nodes.db.data.rows,fn($row){$row.id=$tag.id and $row.tag_name=$tag.tag_name})) != 1})}"#;
    let cross = r#"{"passed":input.countsMatch and $count(input.mismatches)=0 and $count(input.suggestionMismatches)=0,"details":input}"#;
    let summary = r#"{"passed": variables.cross.passed and variables.repeatCount=2 and variables.whileTotal=4 and variables.visited=4*nodes.api.data.count and variables.matchedCount=variables.visited and variables.trueCount=4 and variables.falseCount=4*(nodes.api.data.count-1),"httpCount":nodes.api.data.count,"sqlCount":$count(nodes.db.data.rows),"repeatIterations":variables.repeatCount,"whileIterations":variables.whileTotal,"foreachVisits":variables.visited,"fqlMatches":variables.matchedCount,"thenVisits":variables.trueCount,"elseVisits":variables.falseCount,"crossCheck":variables.cross}"#;
    let branch = |label: &str, counter: &str| json!({"id":label,"name":label,"kind":"sql","connectionId":"db","sql":format!("SELECT id, tag_name, status, $1::bigint AS expected_id, $2::text AS expected_name, $3::bigint AS expected_status FROM {table} WHERE id=$1"),"bindings":{"params":"[item.id,item.tag_name,item.status]"},"postScript":format!("const rows=pm.response.json().rows;console.log('{label}',rows);pm.test('逐条 SQL 与 HTTP 标签一致',()=>{{pm.expect(rows).to.have.lengthOf(1);const r=rows[0];pm.expect({{id:r.id,name:r.tag_name,status:r.status}}).to.deep.equal({{id:r.expected_id,name:r.expected_name,status:r.expected_status}})}});pm.variables.set('visited',pm.variables.get('visited')+1);pm.variables.set('{counter}',pm.variables.get('{counter}')+1);")});
    let matched = r#"{"matched":$count(input.rows)=1 and input.rows[0].id=item.id and input.rows[0].tag_name=item.tag_name and input.rows[0].status=item.status,"id":item.id}"#;
    let mut nodes = vec![
        node("start", "start", "root", json!({})),
        node(
            "api",
            "http",
            "root",
            json!({"request":{"id":"api","name":"tags API","kind":"http","method":"GET","url":"http://localhost:9000/api/tags","preScript":"console.log('开始标签流程');","postScript":initialize}}),
        ),
        node(
            "db",
            "sql",
            "root",
            json!({"request":{"id":"db","kind":"sql","connectionId":"db","sql":format!("SELECT id,tag_name,status FROM {table} ORDER BY id"),"params":[],"postScript":"console.info('SQL 全量标签',pm.response.json().rows);"}}),
        ),
        node("compare", "evaluate", "root", json!({"expression":compare})),
        node(
            "crossStatus",
            "evaluate",
            "root",
            json!({"expression":cross,"assignTo":"cross"}),
        ),
        node(
            "repeat",
            "repeat",
            "root",
            json!({"expression":"2","maxIterations":2}),
        ),
        node(
            "resetWhile",
            "evaluate",
            "repeat:body",
            json!({"expression":"0","assignTo":"whileCount"}),
        ),
        node(
            "while",
            "while",
            "repeat:body",
            json!({"expression":"variables.whileCount < 2","maxIterations":3}),
        ),
        node(
            "repeatLog",
            "sql",
            "repeat:body",
            json!({"request":{"id":"repeatLog","kind":"sql","connectionId":"db","sql":"SELECT 1 AS done","params":[],"postScript":"pm.variables.set('repeatCount',pm.variables.get('repeatCount')+1);console.log('固定循环完成',pm.variables.get('repeatCount'));"}}),
        ),
        node(
            "each",
            "foreach",
            "while:body",
            json!({"expression":"nodes.api.data.tags","maxIterations":1000}),
        ),
        node(
            "tick",
            "evaluate",
            "while:body",
            json!({"expression":"variables.whileCount+1","assignTo":"whileCount"}),
        ),
        node(
            "whileLog",
            "sql",
            "while:body",
            json!({"request":{"id":"whileLog","kind":"sql","connectionId":"db","sql":"SELECT $1::double precision AS while_index","bindings":{"params":"[variables.whileCount]"},"postScript":"pm.variables.set('whileTotal',pm.variables.get('whileTotal')+1);console.log('While 循环完成',pm.response.json().rows, {visited:pm.variables.get('visited')});"}}),
        ),
        node(
            "choose",
            "if",
            "each:body",
            json!({"expression":"item.id = nodes.api.data.tags[0].id"}),
        ),
        node(
            "thenSql",
            "sql",
            "choose:then",
            json!({"request":branch("条件成立","trueCount")}),
        ),
        node(
            "thenMatch",
            "evaluate",
            "choose:then",
            json!({"expression":matched}),
        ),
        node(
            "elseSql",
            "sql",
            "choose:else",
            json!({"request":branch("条件不成立","falseCount")}),
        ),
        node(
            "elseMatch",
            "evaluate",
            "choose:else",
            json!({"expression":matched}),
        ),
        node(
            "recordMatch",
            "evaluate",
            "each:body",
            json!({"expression":"variables.matchedCount + (input.matched ? 1 : 0)","assignTo":"matchedCount"}),
        ),
        node("summary", "evaluate", "root", json!({"expression":summary})),
        node("end", "end", "root", json!({})),
    ];
    // Use readable positions in each subflow when reviewing the generated flow in the editor.
    let mut positions = std::collections::HashMap::new();
    for n in &mut nodes {
        let scope = n["data"]["scope"].as_str().unwrap().to_owned();
        let x = positions.entry(scope).or_insert(0);
        n["position"] = json!({"x":80+*x*260,"y":140});
        *x += 1;
    }
    json!({"id":"tags-controller-validation","name":"真实 tags + SQL + 嵌套控制器验证","nodes":nodes,"edges":[link("start","api"),link("api","db"),link("db","compare"),link("compare","crossStatus"),link("crossStatus","repeat"),link("repeat","summary"),link("summary","end"),link("resetWhile","while"),link("while","repeatLog"),link("each","tick"),link("tick","whileLog"),link("choose","recordMatch"),link("thenSql","thenMatch"),link("elseSql","elseMatch")]})
}
fn set_node(flow: &mut Value, id: &str, key: &str, value: Value) {
    let n = flow["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["id"] == id)
        .unwrap();
    n["data"][key] = value;
}
fn run(flow: Value, connection: &Value) -> (Value, Vec<Value>) {
    let events = Mutex::new(vec![]);
    let result = engine::execute(
        &|event| events.lock().unwrap().push(event),
        None,
        Arc::new(AtomicBool::new(false)),
        json!({"connections":[connection],"requests":[],"collections":[],"environments":[]}),
        "flow".into(),
        flow,
        json!({}),
    );
    (result, events.into_inner().unwrap())
}

#[test]
#[ignore = "需要本地 tags HTTP 接口与 PostgreSQL 配置；只读业务数据"]
fn postgres_tags_controllers_and_console() {
    let mut connection: Value = serde_json::from_str(
        &std::env::var("BOOSTMAN_TEST_POSTGRES").expect("缺少 PostgreSQL 测试配置"),
    )
    .unwrap();
    let password = connection
        .as_object_mut()
        .unwrap()
        .remove("password")
        .unwrap();
    let credential = Credential(format!(
        "boostman-controllers-test-{}",
        uuid::Uuid::new_v4()
    ));
    keyring::Entry::new("boostman", &credential.0)
        .unwrap()
        .set_password(password.as_str().unwrap())
        .unwrap();
    connection["driver"] = json!("postgres");
    connection["id"] = json!("db");
    connection["secretId"] = json!(credential.0);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let discovery=rt.block_on(sql::execute(&json!({"sql":"SELECT table_schema,table_name FROM information_schema.columns WHERE column_name IN ('id','tag_name','status') AND table_schema NOT IN ('pg_catalog','information_schema') GROUP BY table_schema,table_name HAVING COUNT(DISTINCT column_name)=3 ORDER BY table_schema,table_name","params":[]}),&connection,&AtomicBool::new(false))).unwrap();
    let tables = discovery["data"]["rows"].as_array().unwrap();
    let preferred: Vec<_> = tables
        .iter()
        .filter(|t| ["tag", "tags"].contains(&t["table_name"].as_str().unwrap_or("")))
        .collect();
    let table = if preferred.len() == 1 {
        preferred[0]
    } else if tables.len() == 1 {
        &tables[0]
    } else {
        panic!("无法唯一定位标签源表")
    };
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let table_name = format!(
        "{}.{}",
        quote(table["table_schema"].as_str().unwrap()),
        quote(table["table_name"].as_str().unwrap())
    );
    let flow = flow(&table_name);
    let (result, events) = run(flow.clone(), &connection);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("artifacts");
    std::fs::create_dir_all(&directory).unwrap();
    // Keep the trace even if a real service regression causes an assertion to fail.
    std::fs::write(directory.join("tags-controller-validation.json"),serde_json::to_string_pretty(&json!({"verifiedAt":chrono::Utc::now().to_rfc3339(),"table":table_name,"flow":flow,"result":result,"events":events})).unwrap()).unwrap();
    assert_eq!(result["status"], "succeeded", "{result}");
    assert_eq!(result["data"]["passed"], true, "{result}");
    let count = result["data"]["httpCount"].as_u64().unwrap();
    assert!(count >= 2, "至少两个标签才可覆盖条件两条分支");
    assert_eq!(result["data"]["foreachVisits"], 4 * count);
    assert_eq!(result["data"]["thenVisits"], 4);
    assert_eq!(result["data"]["elseVisits"], 4 * (count - 1));
    let logs = result["logs"].as_array().unwrap();
    assert!(logs.iter().any(|l| l["nodeId"] == "thenSql"));
    assert!(logs.iter().any(|l| l["nodeId"] == "elseSql"));
    assert!(
        logs.iter()
            .any(|l| l["path"].as_array().is_some_and(|p| p.len() == 3))
    );
    let first_console = events.iter().position(|e| e["type"] == "console").unwrap();
    let api_done = events
        .iter()
        .position(|e| e["nodeId"] == "api" && e["status"] == "succeeded")
        .unwrap();
    assert!(first_console < api_done);
    let mut negative = flow.clone();
    let request = negative["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["id"] == "thenSql")
        .unwrap();
    request["data"]["request"]["bindings"]["params"] =
        json!("[item.id,item.tag_name & '__deliberate_mismatch__',item.status]");
    let (negative, _) = run(negative, &connection);
    assert_eq!(negative["status"], "failed");
    assert_eq!(negative["error"], "断言失败");
    assert!(!negative["logs"].as_array().unwrap().is_empty());
    let mut zero = flow.clone();
    set_node(&mut zero, "repeat", "expression", json!("0"));
    set_node(
        &mut zero,
        "summary",
        "expression",
        json!(
            "{\"passed\":variables.visited=0 and variables.repeatCount=0,\"visited\":variables.visited}"
        ),
    );
    let (zero, zero_events) = run(zero, &connection);
    assert_eq!(zero["status"], "succeeded");
    assert_eq!(zero["data"]["passed"], true);
    assert!(
        !zero_events
            .iter()
            .any(|e| e["nodeId"] == "thenSql" || e["nodeId"] == "elseSql")
    );
    let mut empty = flow.clone();
    set_node(&mut empty, "each", "expression", json!("[]"));
    set_node(
        &mut empty,
        "summary",
        "expression",
        json!(
            "{\"passed\":variables.visited=0 and variables.repeatCount=2 and variables.whileTotal=4}"
        ),
    );
    let (empty, _) = run(empty, &connection);
    assert_eq!(empty["status"], "succeeded");
    assert_eq!(empty["data"]["passed"], true);
    let mut limited = flow.clone();
    set_node(&mut limited, "while", "maxIterations", json!(1));
    let (limited, _) = run(limited, &connection);
    assert_eq!(limited["status"], "failed");
    assert!(limited["error"].as_str().unwrap().contains("上限"));
    let mut invalid = flow.clone();
    set_node(&mut invalid, "choose", "expression", json!("1"));
    let (invalid, _) = run(invalid, &connection);
    assert_eq!(invalid["status"], "failed");
    assert!(invalid["error"].as_str().unwrap().contains("布尔"));
    let scenario = |name: &str, r: &Value| json!({"name":name,"status":r["status"],"data":r["data"],"error":r["error"],"logCount":r["logs"].as_array().unwrap().len()});
    let report = json!({"verifiedAt":chrono::Utc::now().to_rfc3339(),"endpoint":"http://localhost:9000/api/tags","table":table_name,"readOnly":true,"summary":result["data"],"scenarios":[scenario("完整嵌套循环及条件分支",&result),scenario("故意不匹配并保留失败日志",&negative),scenario("零次固定循环",&zero),scenario("空 ForEach",&empty),scenario("While 超过上限",&limited),scenario("非布尔条件拒绝",&invalid)],"logs":logs,"events":events,"flow":flow});
    std::fs::write(
        directory.join("tags-controller-validation.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    let text = logs
        .iter()
        .map(|l| {
            format!(
                "[{}] [{}] {} {:?}: {}",
                l["timestamp"].as_str().unwrap(),
                l["level"].as_str().unwrap(),
                l["nodeId"].as_str().unwrap(),
                l["path"],
                l["message"].as_str().unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(directory.join("tags-controller-console.log"), text).unwrap();
    println!("完整功能验证通过: {}", result["data"]);
}
