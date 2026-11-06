use crate::{fql, http, scripts, sql};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
pub struct Context {
    pub locals: Value,
    pub environment: Value,
    pub collection: Value,
    pub iteration: Value,
    pub nodes: Value,
    pub input: Value,
    pub item: Value,
    pub index: usize,
    pub iteration_index: usize,
}
impl Context {
    fn new() -> Self {
        Self {
            locals: json!({}),
            environment: json!({}),
            collection: json!({}),
            iteration: json!({}),
            nodes: json!({}),
            input: Value::Null,
            item: Value::Null,
            index: 0,
            iteration_index: 0,
        }
    }
    fn variables(&self) -> Value {
        let mut v = Map::new();
        for scope in [
            &self.collection,
            &self.environment,
            &self.iteration,
            &self.locals,
        ] {
            if let Some(o) = scope.as_object() {
                v.extend(o.clone())
            }
        }
        Value::Object(v)
    }
    fn root(&self) -> Value {
        json!({"variables":self.variables(),"nodes":self.nodes,"input":self.input,"item":self.item,"index":self.index,"iteration":self.iteration_index})
    }
}
pub fn interpolate(v: &Value, vars: &Value) -> Result<Value, String> {
    match v {
        Value::String(s) => {
            let mut rest = s.as_str();
            let mut out = String::new();
            while let Some(i) = rest.find("{{") {
                out.push_str(&rest[..i]);
                rest = &rest[i + 2..];
                let end = rest.find("}}").ok_or("变量占位符未闭合")?;
                let key = rest[..end].trim();
                let value = vars.get(key).ok_or_else(|| format!("变量不存在: {key}"))?;
                if out.is_empty() && end + 2 == rest.len() {
                    return Ok(value.clone());
                }
                out.push_str(&fql::text(value));
                rest = &rest[end + 2..];
            }
            out.push_str(rest);
            Ok(json!(out))
        }
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|v| interpolate(v, vars))
                .collect::<Result<_, _>>()?,
        )),
        Value::Object(o) => Ok(Value::Object(
            o.iter()
                .map(|(k, v)| Ok((k.clone(), interpolate(v, vars)?)))
                .collect::<Result<_, String>>()?,
        )),
        _ => Ok(v.clone()),
    }
}
fn interpolate_http(value: &Value, vars: &Value) -> Result<Value, String> {
    match value {
        Value::String(_) => Ok(json!(fql::text(&interpolate(value, vars)?))),
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|v| interpolate_http(v, vars))
                .collect::<Result<_, _>>()?,
        )),
        Value::Object(o) => Ok(Value::Object(
            o.iter()
                .filter(|(k, _)| !["preScript", "postScript", "bindings"].contains(&k.as_str()))
                .map(|(k, v)| Ok((k.clone(), interpolate_http(v, vars)?)))
                .collect::<Result<_, String>>()?,
        )),
        _ => Ok(value.clone()),
    }
}
struct Runner<'a> {
    emitter: &'a dyn Fn(Value),
    pool: Option<sqlx::SqlitePool>,
    workspace: Value,
    host: scripts::Host,
    ctx: Context,
    options: Value,
    results: Vec<Value>,
    env_id: String,
    collection_id: String,
    base_env: Value,
    base_collection: Value,
    path: Vec<usize>,
    steps: usize,
}
impl Runner<'_> {
    fn inherited(&self, request_id: &str) -> Vec<Value> {
        fn search(items: &[Value], id: &str, parents: &[Value]) -> Option<Vec<Value>> {
            for item in items {
                if item["type"] == "folder" {
                    let mut path = parents.to_vec();
                    path.push(item.clone());
                    if let Some(found) = search(
                        item["items"].as_array().map(Vec::as_slice).unwrap_or(&[]),
                        id,
                        &path,
                    ) {
                        return Some(found);
                    }
                } else if item["requestId"] == id {
                    return Some(parents.to_vec());
                }
            }
            None
        }
        if let Some(c) = self.workspace["collections"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["id"] == self.collection_id))
        {
            let parents = vec![c.clone()];
            return search(
                c["items"].as_array().map(Vec::as_slice).unwrap_or(&[]),
                request_id,
                &parents,
            )
            .unwrap_or(parents);
        }
        vec![]
    }
    fn check(&self) -> Result<(), String> {
        if self.host.cancel.load(Ordering::SeqCst) {
            Err("已取消".into())
        } else {
            Ok(())
        }
    }
    fn emit(&self, event: Value) {
        (self.emitter)(event)
    }
    fn script(
        &mut self,
        sources: &[String],
        request: Value,
        response: Value,
    ) -> Result<Value, String> {
        if sources.iter().all(|s| s.trim().is_empty()) {
            return Ok(json!({"request":request,"tests":[],"logs":[]}));
        }
        let seed = json!({"request":request,"response":response,"locals":self.ctx.locals,"environment":self.ctx.environment,"collection":self.ctx.collection,"iteration":self.ctx.iteration,"iterationIndex":self.ctx.iteration_index});
        let result = scripts::run(
            sources,
            seed,
            &self.host,
            self.options["scriptTimeoutMs"]
                .as_u64()
                .unwrap_or(5000)
                .clamp(1, 600_000),
        )?;
        self.ctx.locals = result["locals"].clone();
        self.ctx.environment = result["environment"].clone();
        self.ctx.collection = result["collection"].clone();
        if let Some(error) = result["error"].as_str() {
            return Err(error.to_owned());
        }
        Ok(result)
    }
    fn request(
        &mut self,
        mut request: Value,
        inherited: &[Value],
        node_id: &str,
    ) -> Result<Value, String> {
        self.check()?;
        self.steps += 1;
        if self.steps > 100_000 {
            return Err("运行超过 100000 步".into());
        }
        let start = Instant::now();
        let request_id = request["id"].as_str().unwrap_or(node_id).to_owned();
        self.emit(json!({"type":"step","nodeId":node_id,"requestId":request_id,"path":self.path,"status":"running"}));
        let result = (|| -> Result<Value, String> {
            let before: Vec<_> = inherited
                .iter()
                .chain(std::iter::once(&request))
                .map(|v| v["preScript"].as_str().unwrap_or("").to_owned())
                .collect();
            let after: Vec<_> = inherited
                .iter()
                .chain(std::iter::once(&request))
                .map(|v| v["postScript"].as_str().unwrap_or("").to_owned())
                .collect();
            let pre = self.script(&before, request.clone(), Value::Null)?;
            request = pre["request"].clone();
            let mut resolved = request.clone();
            if request["kind"] == "sql" {
                resolved["params"] = interpolate(&request["params"], &self.ctx.variables())?;
            } else {
                resolved = interpolate_http(&request, &self.ctx.variables())?;
            }
            if let Some(bindings) = request["bindings"].as_object() {
                for (k, expr) in bindings {
                    let val = fql::query(
                        expr.as_str().ok_or("输入映射必须为 FQL 字符串")?,
                        &self.ctx.root(),
                    )?;
                    if request["kind"] == "sql" && k != "params" {
                        return Err("SQL 输入映射只允许映射 params".into());
                    }
                    resolved[k] = val;
                }
            }
            let mut response = if request["kind"] == "sql" {
                let c = self.workspace["connections"]
                    .as_array()
                    .and_then(|a| a.iter().find(|c| c["id"] == request["connectionId"]))
                    .ok_or("数据库连接不存在")?;
                self.host
                    .runtime
                    .block_on(sql::execute(&resolved, c, &self.host.cancel))?
            } else {
                self.host.runtime.block_on(http::execute(
                    &resolved,
                    self.host.jar.clone(),
                    &self.host.cancel,
                ))?
            };
            let post = self.script(&after, resolved, response.clone())?;
            let tests: Vec<Value> = pre["tests"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(post["tests"].as_array().into_iter().flatten())
                .cloned()
                .collect();
            response["tests"] = json!(tests);
            response["logs"] = json!(
                pre["logs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(post["logs"].as_array().into_iter().flatten())
                    .cloned()
                    .collect::<Vec<_>>()
            );
            if tests.iter().any(|t| t["passed"] == false) {
                response["status"] = json!("failed");
                response["error"] = json!("断言失败")
            }
            Ok(response)
        })();
        let mut result=result.unwrap_or_else(|e|json!({"status":if self.host.cancel.load(Ordering::SeqCst){"cancelled"}else{"failed"},"data":null,"error":e}));
        result["elapsedMs"] = json!(start.elapsed().as_millis());
        self.ctx.nodes[node_id] = result.clone();
        self.ctx.input = result["data"].clone();
        self.emit(json!({"type":"step","nodeId":node_id,"requestId":request_id,"path":self.path,"status":result["status"],"result":result}));
        self.results.push(
            json!({"nodeId":node_id,"requestId":request_id,"path":self.path,"result":result}),
        );
        if result["status"] != "succeeded" {
            return Err(result["error"].as_str().unwrap_or("执行失败").into());
        }
        Ok(result)
    }
    fn collection_items(&mut self, items: &[Value], inherited: &[Value]) -> Result<(), String> {
        if inherited.len() > 32 {
            return Err("集合文件夹嵌套超过 32 层".into());
        }
        for item in items {
            self.check()?;
            if item["type"] == "folder" {
                let mut parents = inherited.to_vec();
                parents.push(item.clone());
                self.collection_items(
                    item["items"].as_array().ok_or("文件夹 items 必须为数组")?,
                    &parents,
                )?;
            } else {
                let req = self.workspace["requests"]
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r["id"] == item["requestId"]))
                    .cloned()
                    .ok_or("集合引用的请求不存在")?;
                let id = item["id"]
                    .as_str()
                    .or(item["requestId"].as_str())
                    .unwrap_or("request");
                let r = self.request(req, inherited, id);
                if self.options["continueOnError"] != true {
                    r?;
                }
            }
        }
        Ok(())
    }
    fn flow_scope(&mut self, flow: &Value, scope: &str, depth: usize) -> Result<Value, String> {
        if depth > 32 {
            return Err("流程嵌套超过 32 层".into());
        }
        let ordered = ordered_scope(flow, scope)?;
        for node in ordered {
            self.check()?;
            self.steps += 1;
            if self.steps > 100_000 {
                return Err("流程执行超过 100000 步".into());
            }
            let id = node["id"].as_str().ok_or("节点缺少 ID")?;
            let d = &node["data"];
            let kind = d["kind"].as_str().unwrap_or("");
            if kind == "http" || kind == "sql" {
                let request =
                    if let Some(ref_id) = d["requestId"].as_str().filter(|s| !s.is_empty()) {
                        self.workspace["requests"]
                            .as_array()
                            .and_then(|a| a.iter().find(|r| r["id"] == ref_id))
                            .cloned()
                            .ok_or("节点引用的请求不存在")?
                    } else {
                        let mut r = d["request"].clone();
                        if !r.is_object() {
                            return Err("请求节点缺少配置".into());
                        }
                        r["kind"] = json!(kind);
                        r
                    };
                if request["kind"] != kind {
                    return Err("请求类型与流程节点不匹配".into());
                }
                let parents = self.inherited(request["id"].as_str().unwrap_or(""));
                self.request(request, &parents, id)?;
                continue;
            }
            self.emit(json!({"type":"step","nodeId":id,"path":self.path,"status":"running"}));
            let outcome = (|| -> Result<Value, String> {
                match kind {
                    "start" | "end" => Ok(self.ctx.input.clone()),
                    "evaluate" => {
                        let val = fql::query(
                            d["expression"].as_str().unwrap_or("input"),
                            &self.ctx.root(),
                        )?;
                        if let Some(name) = d["assignTo"].as_str().filter(|s| !s.is_empty()) {
                            self.ctx.locals[name] = val.clone();
                        }
                        Ok(val)
                    }
                    "if" => {
                        let cond = fql::boolean(&fql::query(
                            d["expression"].as_str().unwrap_or("false"),
                            &self.ctx.root(),
                        )?)?;
                        let skipped = format!("{id}:{}", if cond { "else" } else { "then" });
                        for n in flow["nodes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|n| n["data"]["scope"] == skipped)
                        {
                            self.emit(json!({"type":"step","nodeId":n["id"],"path":self.path,"status":"skipped"}));
                        }
                        self.flow_scope(
                            flow,
                            &format!("{id}:{}", if cond { "then" } else { "else" }),
                            depth + 1,
                        )
                    }
                    "repeat" | "while" | "foreach" => {
                        let limit = d["maxIterations"]
                            .as_u64()
                            .unwrap_or(1000)
                            .clamp(1, 1_000_000) as usize;
                        let expression = d["expression"].as_str().unwrap_or(if kind == "repeat" {
                            "1"
                        } else if kind == "foreach" {
                            "input"
                        } else {
                            "false"
                        });
                        let initial = if kind == "while" {
                            Value::Null
                        } else {
                            fql::query(expression, &self.ctx.root())?
                        };
                        let count = match kind {
                            "repeat" => {
                                let n = initial.as_f64().ok_or("固定循环次数必须为非负整数")?;
                                if n < 0.0 || n.fract() != 0.0 || n > limit as f64 {
                                    return Err("固定循环次数无效或超过上限".into());
                                }
                                n as usize
                            }
                            "foreach" => initial.as_array().ok_or("ForEach 输入必须为数组")?.len(),
                            _ => usize::MAX,
                        };
                        if kind != "while" && count > limit {
                            return Err(format!("循环次数超过上限 {limit}"));
                        }
                        let old_item = self.ctx.item.clone();
                        let old_index = self.ctx.index;
                        let mut values = vec![];
                        let mut i = 0;
                        let result = (|| -> Result<(), String> {
                            loop {
                                self.check()?;
                                if kind == "while" {
                                    if !fql::boolean(&fql::query(expression, &self.ctx.root())?)? {
                                        break;
                                    }
                                } else if i >= count {
                                    break;
                                }
                                if i >= limit {
                                    return Err(format!("循环次数超过上限 {limit}"));
                                }
                                self.ctx.index = i;
                                if kind == "foreach" {
                                    self.ctx.item = initial[i].clone();
                                    self.ctx.input = initial[i].clone();
                                }
                                self.path.push(i);
                                let r = self.flow_scope(flow, &format!("{id}:body"), depth + 1);
                                self.path.pop();
                                values.push(r?);
                                i += 1;
                            }
                            Ok(())
                        })();
                        self.ctx.item = old_item;
                        self.ctx.index = old_index;
                        result?;
                        Ok(json!(values))
                    }
                    _ => Err(format!("不支持的节点类型 {kind}")),
                }
            })();
            let status = if outcome.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            let result = match &outcome {
                Ok(v) => json!({"status":status,"data":v}),
                Err(e) => json!({"status":status,"error":e}),
            };
            self.emit(
                json!({"type":"step","nodeId":id,"path":self.path,"status":status,"result":result}),
            );
            self.ctx.nodes[id] = result;
            self.ctx.input = outcome?;
        }
        Ok(self.ctx.input.clone())
    }
    fn persist_variables(&self) -> Result<(), String> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        self.host.runtime.block_on(async {
            let mut current = crate::store::load(pool).await?;
            for (key, id, base, changed) in [
                (
                    "environments",
                    &self.env_id,
                    &self.base_env,
                    &self.ctx.environment,
                ),
                (
                    "collections",
                    &self.collection_id,
                    &self.base_collection,
                    &self.ctx.collection,
                ),
            ] {
                if id.is_empty() || base == changed {
                    continue;
                }
                if let Some(target) = current[key]
                    .as_array_mut()
                    .and_then(|a| a.iter_mut().find(|v| v["id"].as_str() == Some(id)))
                {
                    let mut merged = target["variables"].as_object().cloned().unwrap_or_default();
                    let before = base.as_object().cloned().unwrap_or_default();
                    let after = changed.as_object().cloned().unwrap_or_default();
                    for k in before.keys() {
                        if !after.contains_key(k) {
                            merged.remove(k);
                        }
                    }
                    for (k, v) in after {
                        if before.get(&k) != Some(&v) {
                            merged.insert(k, v);
                        }
                    }
                    target["variables"] = json!(merged);
                }
            }
            crate::store::save(pool, &current).await
        })
    }
}
pub fn ordered_scope<'a>(flow: &'a Value, scope: &str) -> Result<Vec<&'a Value>, String> {
    let nodes = flow["nodes"].as_array().ok_or("流程 nodes 必须为数组")?;
    let edges = flow["edges"].as_array().ok_or("流程 edges 必须为数组")?;
    let selected: Vec<_> = nodes
        .iter()
        .filter(|n| n["data"]["scope"].as_str().unwrap_or("root") == scope)
        .collect();
    if selected.is_empty() {
        return Ok(vec![]);
    }
    let ids: HashSet<_> = selected.iter().filter_map(|n| n["id"].as_str()).collect();
    let mut next = HashMap::new();
    let mut incoming = HashSet::new();
    for e in edges {
        let source = e["source"].as_str().ok_or("连线缺少 source")?;
        let target = e["target"].as_str().ok_or("连线缺少 target")?;
        if !ids.contains(source) {
            continue;
        }
        if !ids.contains(target) {
            return Err("不允许跨子流程连线".into());
        }
        if next.insert(source, target).is_some() || !incoming.insert(target) {
            return Err("首版每个节点最多一条入线和出线".into());
        }
    }
    let roots: Vec<_> = ids.iter().filter(|id| !incoming.contains(**id)).collect();
    if roots.len() != 1 {
        return Err(format!("子流程 {scope} 必须是一条无环的顺序链"));
    }
    let mut cur = *roots[0];
    let mut seen = HashSet::new();
    let mut out = vec![];
    loop {
        if !seen.insert(cur) {
            return Err("流程包含环".into());
        }
        out.push(
            *selected
                .iter()
                .find(|n| n["id"].as_str() == Some(cur))
                .ok_or("节点不存在")?,
        );
        if let Some(n) = next.get(cur) {
            cur = n
        } else {
            break;
        }
    }
    if out.len() != selected.len() {
        return Err("流程存在未连接节点或循环".into());
    }
    Ok(out)
}
pub fn validate_flow(flow: &Value) -> Result<(), String> {
    let nodes = flow["nodes"].as_array().ok_or("缺少 nodes")?;
    let mut ids = HashSet::new();
    for n in nodes {
        let id = n["id"].as_str().ok_or("节点缺少 ID")?;
        if !ids.insert(id) {
            return Err("节点 ID 重复".into());
        }
    }
    for e in flow["edges"].as_array().ok_or("缺少 edges")? {
        if !ids.contains(e["source"].as_str().unwrap_or(""))
            || !ids.contains(e["target"].as_str().unwrap_or(""))
        {
            return Err("连线引用不存在的节点".into());
        }
    }
    let mut scopes = HashSet::from(["root"]);
    for n in nodes {
        let scope = n["data"]["scope"].as_str().unwrap_or("root");
        scopes.insert(scope);
        let mut chain = HashSet::new();
        let mut s = scope;
        while s != "root" {
            if !chain.insert(s) {
                return Err("子流程嵌套存在环".into());
            }
            let (owner, branch) = s.rsplit_once(':').ok_or("子流程标识无效")?;
            let parent = nodes
                .iter()
                .find(|n| n["id"] == owner)
                .ok_or("子流程控制器不存在")?;
            let kind = parent["data"]["kind"].as_str().unwrap_or("");
            if !(kind == "if" && ["then", "else"].contains(&branch)
                || ["repeat", "while", "foreach"].contains(&kind) && branch == "body")
            {
                return Err("子流程与控制器类型不匹配".into());
            }
            s = parent["data"]["scope"].as_str().unwrap_or("root");
            if chain.len() > 32 {
                return Err("流程嵌套超过 32 层".into());
            }
        }
    }
    for scope in scopes {
        ordered_scope(flow, scope)?;
    }
    let root = ordered_scope(flow, "root")?;
    if root.first().map(|n| &n["data"]["kind"]) != Some(&json!("start"))
        || root.last().map(|n| &n["data"]["kind"]) != Some(&json!("end"))
    {
        return Err("主流程必须从 Start 开始，以 End 结束".into());
    }
    if nodes
        .iter()
        .filter(|n| n["data"]["kind"] == "start")
        .count()
        != 1
        || nodes.iter().filter(|n| n["data"]["kind"] == "end").count() != 1
    {
        return Err("流程必须恰好包含一个 Start 和 End".into());
    }
    Ok(())
}
pub fn execute(
    emitter: &dyn Fn(Value),
    pool: Option<sqlx::SqlitePool>,
    cancel: Arc<AtomicBool>,
    workspace: Value,
    kind: String,
    target: Value,
    options: Value,
) -> Value {
    let started = Instant::now();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => Arc::new(r),
        Err(e) => return json!({"status":"failed","error":e.to_string()}),
    };
    let env_id = options["environmentId"].as_str().unwrap_or("").to_owned();
    let mut ctx = Context::new();
    if let Some(env) = workspace["environments"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e["id"] == env_id))
    {
        ctx.environment = env["variables"].clone();
    }
    let collection_id = if kind == "collection" {
        target["id"].as_str().unwrap_or("").to_owned()
    } else {
        options["collectionId"].as_str().unwrap_or("").to_owned()
    };
    if let Some(c) = workspace["collections"]
        .as_array()
        .and_then(|a| a.iter().find(|c| c["id"] == collection_id))
    {
        ctx.collection = c["variables"].clone();
    }
    let mut runner = Runner {
        emitter,
        pool,
        workspace,
        host: scripts::Host {
            runtime,
            jar: Arc::default(),
            cancel: cancel.clone(),
        },
        base_env: ctx.environment.clone(),
        base_collection: ctx.collection.clone(),
        ctx,
        options,
        results: vec![],
        env_id,
        collection_id,
        path: vec![],
        steps: 0,
    };
    let outcome = (|| -> Result<Value, String> {
        match kind.as_str() {
            "request" => {
                let id = target["id"].as_str().unwrap_or("request").to_owned();
                let parents = runner.inherited(&id);
                runner.request(target, &parents, &id)
            }
            "collection" => {
                let items = target["items"].as_array().ok_or("集合 items 必须为数组")?;
                let data = if runner.options["data"].is_null() {
                    vec![json!({})]
                } else {
                    runner.options["data"]
                        .as_array()
                        .cloned()
                        .ok_or("迭代数据必须为 JSON 数组")?
                };
                let rounds = if runner.options["iterations"].is_null() {
                    1
                } else {
                    runner.options["iterations"]
                        .as_u64()
                        .ok_or("迭代轮数必须为非负整数")?
                };
                if rounds > 10000 {
                    return Err("集合轮数超过 10000".into());
                }
                let mut index = 0;
                for round in 0..rounds {
                    for (row, item) in data.iter().enumerate() {
                        if !item.is_object() {
                            return Err("迭代数据的每行必须为 JSON 对象".into());
                        }
                        runner.ctx.iteration = item.clone();
                        runner.ctx.iteration_index = index;
                        runner.path = vec![round as usize, row];
                        runner.collection_items(items, &[target.clone()])?;
                        index += 1;
                    }
                }
                Ok(json!(runner.results))
            }
            "flow" => {
                validate_flow(&target)?;
                runner.flow_scope(&target, "root", 0)
            }
            _ => Err("未知运行类型".into()),
        }
    })();
    let persisted = runner.persist_variables();
    let failed = outcome.is_err()
        || persisted.is_err()
        || runner
            .results
            .iter()
            .any(|r| r["result"]["status"] != "succeeded");
    json!({"status":if cancel.load(Ordering::SeqCst){"cancelled"}else if failed{"failed"}else{"succeeded"},"data":outcome.as_ref().ok(),"error":outcome.as_ref().err(),"persistenceError":persisted.err(),"steps":runner.results,"context":runner.ctx.root(),"elapsedMs":started.elapsed().as_millis()})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_continue_policy() {
        let requests = json!([{"id":"a","kind":"http","url":"ftp://localhost/a"},{"id":"b","kind":"http","url":"ftp://localhost/b"}]);
        let target = json!({"id":"c","items":[{"requestId":"a"},{"requestId":"b"}]});
        for keep in [false, true] {
            let r = run(
                "collection",
                target.clone(),
                requests.clone(),
                json!({"iterations":2,"continueOnError":keep}),
            );
            assert_eq!(r["status"], "failed");
            assert_eq!(
                r["steps"].as_array().unwrap().len(),
                if keep { 4 } else { 1 }
            );
        }
    }
    #[test]
    fn inherited_scripts_and_persisted_variables() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let join = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut buf = [0; 1024];
            let _ = s.read(&mut buf);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .unwrap();
        });
        let request = json!({"id":"r","name":"test","kind":"http","url":format!("http://{addr}"),"preScript":"pm.variables.set('trace',pm.variables.get('trace')+'r')","postScript":"pm.test('order',()=>pm.expect(pm.variables.get('trace')).to.equal('cfrCF'));pm.environment.set('saved','yes');pm.collectionVariables.set('count',1)"});
        let c = json!({"id":"c","name":"collection","variables":{},"preScript":"pm.variables.set('trace','c')","postScript":"pm.variables.set('trace',pm.variables.get('trace')+'C')","items":[{"id":"folder","type":"folder","preScript":"pm.variables.set('trace',pm.variables.get('trace')+'f')","postScript":"pm.variables.set('trace',pm.variables.get('trace')+'F')","items":[{"requestId":"r"}]}]});
        let workspace = json!({"version":1,"requests":[request.clone()],"collections":[c],"environments":[{"id":"env","name":"test","variables":{"untouched":42}}],"connections":[],"flows":[]});
        let path = std::env::temp_dir().join(format!("boostman-{}.sqlite", uuid::Uuid::new_v4()));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let pool = rt.block_on(crate::store::open(&path)).unwrap();
        rt.block_on(crate::store::save(&pool, &workspace)).unwrap();
        let r = execute(
            &|_| {},
            Some(pool.clone()),
            Arc::new(AtomicBool::new(false)),
            workspace,
            "request".into(),
            request,
            json!({"collectionId":"c","environmentId":"env"}),
        );
        join.join().unwrap();
        assert_eq!(r["status"], "succeeded", "{r}");
        let saved = rt.block_on(crate::store::load(&pool)).unwrap();
        assert_eq!(
            saved["environments"][0]["variables"],
            json!({"untouched":42,"saved":"yes"})
        );
        assert_eq!(saved["collections"][0]["variables"]["count"], 1);
        rt.block_on(pool.close());
        drop(pool);
        for attempt in 0..50 {
            match std::fs::remove_file(&path) {
                Ok(()) => break,
                Err(e) if attempt < 49 && e.raw_os_error() == Some(32) => {
                    std::thread::sleep(std::time::Duration::from_millis(10))
                }
                Err(e) => panic!("临时数据库清理失败: {e}"),
            }
        }
    }
    fn run(kind: &str, target: Value, requests: Value, options: Value) -> Value {
        execute(
            &|_| {},
            None,
            Arc::new(AtomicBool::new(false)),
            json!({"version":1,"requests":requests,"connections":[],"collections":[],"environments":[],"flows":[]}),
            kind.into(),
            target,
            options,
        )
    }
    fn node(id: &str, kind: &str, scope: &str, expression: &str) -> Value {
        json!({"id":id,"data":{"kind":kind,"scope":scope,"expression":expression}})
    }
    fn flow(nodes: Vec<Value>, links: Vec<(&str, &str)>) -> Value {
        json!({"nodes":nodes,"edges":links.into_iter().map(|(a,b)|json!({"source":a,"target":b})).collect::<Vec<_>>()})
    }
    #[test]
    fn nested_foreach_condition_and_while() {
        let f = flow(
            vec![
                node("s", "start", "root", ""),
                node("list", "evaluate", "root", "[1,2,3]"),
                node("loop", "foreach", "root", "input"),
                node("choose", "if", "loop:body", "item > 1"),
                node("yes", "evaluate", "choose:then", "item * 10"),
                node("no", "evaluate", "choose:else", "0"),
                node("e", "end", "root", ""),
            ],
            vec![("s", "list"), ("list", "loop"), ("loop", "e")],
        );
        let r = run("flow", f, json!([]), json!({}));
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["data"], json!([0, 20.0, 30.0]));
        let mut init = node("init", "evaluate", "root", "0");
        init["data"]["assignTo"] = json!("n");
        let mut inc = node("inc", "evaluate", "w:body", "variables.n + 1");
        inc["data"]["assignTo"] = json!("n");
        let f = flow(
            vec![
                node("s", "start", "root", ""),
                init,
                node("w", "while", "root", "variables.n < 3"),
                inc,
                node("e", "end", "root", ""),
            ],
            vec![("s", "init"), ("init", "w"), ("w", "e")],
        );
        let r = run("flow", f, json!([]), json!({}));
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["context"]["variables"]["n"], json!(3.0));
    }
    #[test]
    fn empty_loop_and_limit() {
        for expr in ["[]", "[1]"] {
            let mut loop_node = node("loop", "foreach", "root", expr);
            loop_node["data"]["maxIterations"] = json!(1);
            let f = flow(
                vec![
                    node("s", "start", "root", ""),
                    loop_node,
                    node("body", "evaluate", "loop:body", "item"),
                    node("e", "end", "root", ""),
                ],
                vec![("s", "loop"), ("loop", "e")],
            );
            let r = run("flow", f, json!([]), json!({}));
            assert_eq!(r["status"], "succeeded", "{r}");
            assert_eq!(
                r["data"].as_array().unwrap().len(),
                if expr == "[]" { 0 } else { 1 }
            );
        }
        let mut w = node("w", "while", "root", "true");
        w["data"]["maxIterations"] = json!(2);
        let r = run(
            "flow",
            flow(
                vec![
                    node("s", "start", "root", ""),
                    w,
                    node("e", "end", "root", ""),
                ],
                vec![("s", "w"), ("w", "e")],
            ),
            json!([]),
            json!({}),
        );
        assert_eq!(r["status"], "failed");
        assert!(r["error"].as_str().unwrap().contains("上限"));
    }
    #[test]
    fn same_request_across_all_entrypoints() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let join = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut s, _) = server.accept().unwrap();
                let mut buf = [0; 4096];
                let count = s.read(&mut buf).unwrap();
                let text = String::from_utf8_lossy(&buf[..count]);
                assert!(text.contains("/check?number=7"), "{text}");
                assert!(text.to_lowercase().contains("x-test: yes"));
                s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
            }
        });
        let request = json!({"id":"r","name":"test","kind":"http","url":format!("http://{addr}/check"),"method":"GET","query":[{"key":"number","value":"{{number}}"}],"preScript":"pm.variables.set('number',7);pm.request.headers.upsert({key:'X-Test',value:'yes'});","postScript":"pm.test('ok',()=>pm.expect(pm.response.json().ok).to.be.true);"});
        let single = run(
            "request",
            request.clone(),
            json!([request.clone()]),
            json!({}),
        );
        let collection = run(
            "collection",
            json!({"id":"c","items":[{"requestId":"r"}]}),
            json!([request.clone()]),
            json!({"iterations":1}),
        );
        let mut n = node("rnode", "http", "root", "");
        n["data"]["requestId"] = json!("r");
        let f = flow(
            vec![
                node("s", "start", "root", ""),
                n,
                node("e", "end", "root", ""),
            ],
            vec![("s", "rnode"), ("rnode", "e")],
        );
        let flow_result = run("flow", f, json!([request]), json!({}));
        join.join().unwrap();
        for result in [&single, &collection, &flow_result] {
            assert_eq!(result["status"], "succeeded", "{result}");
            assert_eq!(result["steps"][0]["result"]["data"], json!({"ok":true}));
            assert_eq!(result["steps"][0]["result"]["tests"][0]["passed"], true);
        }
    }
    #[test]
    fn variable_precedence() {
        let mut c = Context::new();
        c.collection = json!({"a":1});
        c.environment = json!({"a":2});
        c.iteration = json!({"a":3});
        c.locals = json!({"a":4});
        assert_eq!(
            interpolate(&json!("{{a}}"), &c.variables()).unwrap(),
            json!(4)
        );
        assert_eq!(
            interpolate(&json!("a={{a}}"), &c.variables()).unwrap(),
            json!("a=4")
        );
        assert!(interpolate(&json!("{{missing}}"), &c.variables()).is_err());
    }
    #[test]
    fn graph_validation() {
        let flow = json!({"nodes":[{"id":"a","data":{"kind":"start"}},{"id":"b","data":{"kind":"end"}}],"edges":[{"source":"a","target":"b"}]});
        validate_flow(&flow).unwrap();
        let mut bad = flow.clone();
        bad["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source":"b","target":"a"}));
        assert!(validate_flow(&bad).is_err());
    }
}
