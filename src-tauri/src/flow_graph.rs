//! Port-based, deterministic dataflow. Closed tokens propagate untaken branches.
use super::*;
use std::collections::VecDeque;
type Tokens = HashMap<(String, String), Option<Value>>;
type Event = (String, String, Option<Value>);
fn spec() -> &'static Value {
    static SPEC: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SPEC.get_or_init(|| serde_json::from_str(include_str!("../../src/flowPorts.json")).unwrap())
}
pub fn ports(d: &Value, side: &str) -> Vec<String> {
    let kind = d["kind"].as_str().unwrap_or("");
    let mut ports: Vec<String> = spec()[kind][side]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p[0].as_str().map(str::to_owned))
        .collect();
    if side == "outputs" && kind == "condition" {
        for c in d["conditions"].as_array().into_iter().flatten() {
            ports.insert(
                ports.len() - 1,
                format!("case-{}", c["id"].as_str().unwrap_or("")),
            );
        }
    }
    if side == "inputs" {
        if ["list", "record"].contains(&kind) {
            for (i, e) in d["entries"].as_array().into_iter().flatten().enumerate() {
                ports.push(format!(
                    "item:{}",
                    e["id"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| i.to_string())
                ));
            }
        }
        for b in d["inputBindings"].as_array().into_iter().flatten() {
            ports.push(format!("var:{}", b["id"].as_str().unwrap_or("")));
        }
    }
    ports
}
fn handle(e: &Value, side: &str, n: &Value) -> String {
    e[if side == "inputs" {
        "targetHandle"
    } else {
        "sourceHandle"
    }]
    .as_str()
    .map(str::to_owned)
    .unwrap_or_else(|| ports(&n["data"], side).first().cloned().unwrap_or_default())
}
pub fn validate(flow: &Value) -> Result<(), String> {
    let nodes = flow["nodes"].as_array().ok_or("缺少 nodes")?;
    let edges = flow["edges"].as_array().ok_or("缺少 edges")?;
    if nodes.len() > 10000 || edges.len() > 50000 {
        return Err("端口画布最多支持 10000 个节点和 50000 条连线".into());
    }
    let mut ids = HashMap::new();
    for n in nodes {
        let id = n["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("节点 ID 不能为空")?;
        if ids.insert(id, n).is_some() {
            return Err("节点 ID 重复".into());
        }
        let d = &n["data"];
        if spec().get(d["kind"].as_str().unwrap_or("")).is_none() {
            return Err("端口画布包含不支持的组件".into());
        }
        crate::flow_blocks::branches(d)?;
        let mut names = HashSet::new();
        for b in d["inputBindings"].as_array().into_iter().flatten() {
            let name = b["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("输入变量名称不能为空")?;
            if [
                "input",
                "variables",
                "nodes",
                "locals",
                "item",
                "index",
                "iteration",
            ]
            .contains(&name)
                || !names.insert(name)
            {
                return Err("输入变量名称重复或使用了保留名称".into());
            }
        }
        for side in ["inputs", "outputs"] {
            let p = ports(d, side);
            if p.iter().collect::<HashSet<_>>().len() != p.len() {
                return Err("端口 ID 重复".into());
            }
        }
    }
    if nodes
        .iter()
        .filter(|n| n["data"]["kind"] == "start")
        .count()
        != 1
    {
        return Err("端口画布需要一个 Start".into());
    }
    if nodes
        .iter()
        .filter(|n| n["data"]["kind"] == "output")
        .count()
        > 1
    {
        return Err("每个流程最多一个 Output".into());
    }
    let mut occupied = HashSet::new();
    let mut degree: HashMap<&str, usize> = ids.keys().map(|id| (*id, 0)).collect();
    for e in edges {
        let a = ids
            .get(e["source"].as_str().unwrap_or(""))
            .ok_or("连线起点不存在")?;
        let b = ids
            .get(e["target"].as_str().unwrap_or(""))
            .ok_or("连线终点不存在")?;
        let source = handle(e, "outputs", a);
        let target = handle(e, "inputs", b);
        if !ports(&a["data"], "outputs").contains(&source)
            || !ports(&b["data"], "inputs").contains(&target)
        {
            return Err("连线引用不存在的端口".into());
        }
        if !occupied.insert((b["id"].as_str().unwrap(), target)) {
            return Err("每个输入端口只能连接一条线".into());
        }
        *degree.get_mut(b["id"].as_str().unwrap()).unwrap() += 1;
    }
    let mut queue: VecDeque<_> = degree
        .iter()
        .filter(|(_, v)| **v == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut count = 0;
    while let Some(id) = queue.pop_front() {
        count += 1;
        for e in edges.iter().filter(|e| e["source"] == id) {
            let d = degree.get_mut(e["target"].as_str().unwrap()).unwrap();
            *d -= 1;
            if *d == 0 {
                queue.push_back(e["target"].as_str().unwrap());
            }
        }
    }
    if count != nodes.len() {
        return Err("端口画布不允许任意回边，请使用 Repeat / ForEach / While".into());
    }
    Ok(())
}
impl Runner<'_> {
    pub(super) fn graph(&mut self, flow: &Value) -> Result<Value, String> {
        let key = flow["id"].as_str().unwrap_or("<root>").to_owned();
        if self.flow_stack.len() >= 32 || self.flow_stack.contains(&key) {
            return Err("Flow 模块递归调用或嵌套超过 32 层".into());
        }
        self.flow_stack.push(key);
        let root_input = self.options.get("input").cloned().unwrap_or_else(|| {
            flow["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["data"]["kind"] == "start")
                .and_then(|n| n["data"].get("value"))
                .cloned()
                .unwrap_or(Value::Null)
        });
        self.options["input"] = root_input.clone();
        self.ctx.input = root_input;
        let execution = self.graph_events(flow, vec![], Tokens::new(), true, 0);
        self.flow_stack.pop();
        execution?;
        Ok(self
            .output
            .clone()
            .or(self.response.clone())
            .unwrap_or_else(|| self.ctx.input.clone()))
    }
    fn graph_events(
        &mut self,
        flow: &Value,
        seeds: Vec<Event>,
        inherited: Tokens,
        roots: bool,
        depth: usize,
    ) -> Result<HashMap<String, Vec<Value>>, String> {
        if depth > 32 {
            return Err("循环嵌套超过 32 层".into());
        }
        let nodes = flow["nodes"].as_array().unwrap();
        let edges = flow["edges"].as_array().unwrap();
        let mut tokens = inherited;
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut fired = HashSet::new();
        let mut collected: HashMap<String, Vec<Value>> = HashMap::new();
        let first_or = std::cell::RefCell::new(HashMap::<String, Value>::new());
        let deliver = |event: Event, tokens: &mut Tokens, queue: &mut VecDeque<String>| {
            for e in edges.iter().filter(|e| e["source"] == event.0) {
                let a = nodes.iter().find(|n| n["id"] == event.0).unwrap();
                if handle(e, "outputs", a) != event.1 {
                    continue;
                }
                let b = nodes.iter().find(|n| n["id"] == e["target"]).unwrap();
                if b["data"]["kind"] == "or" {
                    if let Some(value) = &event.2 {
                        first_or
                            .borrow_mut()
                            .entry(b["id"].as_str().unwrap().into())
                            .or_insert_with(|| value.clone());
                    }
                }
                tokens.insert(
                    (b["id"].as_str().unwrap().into(), handle(e, "inputs", b)),
                    event.2.clone(),
                );
                queue.push_back(b["id"].as_str().unwrap().into());
            }
        };
        for e in seeds {
            deliver(e, &mut tokens, &mut queue);
        }
        if roots {
            for n in nodes
                .iter()
                .filter(|n| !edges.iter().any(|e| e["target"] == n["id"]))
                .filter(|n| {
                    [
                        "start",
                        "constant",
                        "string",
                        "number",
                        "bool",
                        "null",
                        "date",
                        "datetime",
                        "now",
                        "list",
                        "record",
                        "evaluate",
                        "repeat",
                        "foreach",
                        "while",
                        "getVariable",
                    ]
                    .contains(&n["data"]["kind"].as_str().unwrap_or(""))
                })
            {
                queue.push_back(n["id"].as_str().unwrap().into());
            }
        }
        while let Some(id) = queue.pop_front() {
            self.check()?;
            if fired.contains(&id) {
                continue;
            }
            let node = nodes.iter().find(|n| n["id"] == id).unwrap();
            let mut resolved = node["data"].clone();
            if resolved["kind"] == "getVariable" {
                if let Some(reference) = resolved["variableRef"].as_str().filter(|s| !s.is_empty())
                {
                    let creator = nodes
                        .iter()
                        .find(|n| n["id"] == reference && n["data"]["kind"] == "createVariable")
                        .ok_or("Get Variable 引用的创建组件不存在")?;
                    resolved["variableName"] = creator["data"]["variableName"].clone();
                }
            }
            let d = &resolved;
            let kind = d["kind"].as_str().unwrap();
            let incoming: Vec<String> = edges
                .iter()
                .filter(|e| e["target"] == id)
                .map(|e| handle(e, "inputs", node))
                .collect();
            let available: Vec<_> = incoming
                .iter()
                .filter_map(|p| tokens.get(&(id.clone(), p.clone())))
                .collect();
            let is_or = kind == "or";
            if is_or && !available.iter().any(|v| v.is_some()) && available.len() < incoming.len() {
                continue;
            }
            if !is_or && available.len() != incoming.len() {
                continue;
            }
            if kind == "getVariable"
                && self
                    .ctx
                    .locals
                    .get(d["variableName"].as_str().unwrap_or(""))
                    .is_none()
            {
                continue;
            }
            let inactive = if is_or {
                !incoming.is_empty() && !available.iter().any(|v| v.is_some())
            } else {
                available.iter().any(|v| v.is_none())
            };
            fired.insert(id.clone());
            if inactive {
                self.emit(json!({"type":"step","nodeId":id,"path":self.path,"status":"skipped"}));
                for p in ports(d, "outputs") {
                    deliver((id.clone(), p, None), &mut tokens, &mut queue);
                }
                continue;
            }
            self.steps += 1;
            if self.steps > 100000 {
                return Err("流程执行超过 100000 步".into());
            }
            let mut values: HashMap<String, Value> = incoming
                .iter()
                .filter_map(|p| {
                    tokens
                        .get(&(id.clone(), p.clone()))
                        .cloned()
                        .flatten()
                        .map(|v| (p.clone(), v))
                })
                .collect();
            for (port, value) in values.iter_mut() {
                if let Some(path) = d["inputPaths"][port].as_str().filter(|s| !s.is_empty()) {
                    *value = value
                        .pointer(path)
                        .cloned()
                        .ok_or_else(|| format!("节点 {id} 输入 {port} 的路径不存在: {path}"))?;
                }
            }
            let input = if is_or {
                first_or.borrow().get(&id).cloned().unwrap_or(Value::Null)
            } else {
                values
                    .get("in")
                    .or(values.get("body"))
                    .or(values.get("list"))
                    .or(values.get("count"))
                    .cloned()
                    .unwrap_or_else(|| {
                        if roots && incoming.is_empty() {
                            self.options.get("input").cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        }
                    })
            };
            self.ctx.input = input.clone();
            let mut root = self.ctx.root();
            for b in d["inputBindings"].as_array().into_iter().flatten() {
                let port = format!("var:{}", b["id"].as_str().unwrap_or(""));
                root[b["name"].as_str().unwrap()] = if let Some(v) = values.get(&port) {
                    v.clone()
                } else {
                    fql::query(b["expression"].as_str().unwrap_or("null"), &root)?
                };
            }
            let started = Instant::now();
            self.emit(json!({"type":"step","nodeId":id,"path":self.path,"status":"running"}));
            let mut meta = json!({"label":d["label"].as_str().unwrap_or(&id)});
            let mut outputs: HashMap<String, Value> = HashMap::new();
            let mut tests = json!([]);
            let outcome = (|| -> Result<Value, String> {
                match kind {
                    "flow" => {
                        let target = self.workspace["flows"]
                            .as_array()
                            .and_then(|a| a.iter().find(|f| f["id"] == d["flowId"]))
                            .cloned()
                            .ok_or("Flow 模块不存在")?;
                        if target["executionMode"] != "dataflow" {
                            return Err("Flow 模块需要选择端口数据流画布".into());
                        }
                        if !target["nodes"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|n| n["data"]["kind"] == "output"))
                        {
                            return Err("Flow 模块需要一个 Output 组件".into());
                        }
                        validate_flow(&target)?;
                        let old_context = self.ctx.clone();
                        let old_output = self.output.take();
                        let old_response = self.response.take();
                        let old_input = self.options.get("input").cloned();
                        self.ctx = Context::new();
                        self.ctx.environment = old_context.environment.clone();
                        self.ctx.collection = old_context.collection.clone();
                        self.ctx.input = input.clone();
                        self.options["input"] = input.clone();
                        let executed = self.graph(&target);
                        let module_output = self.output.take();
                        let environment = self.ctx.environment.clone();
                        let collection = self.ctx.collection.clone();
                        self.ctx = old_context;
                        self.ctx.environment = environment;
                        self.ctx.collection = collection;
                        self.output = old_output;
                        self.response = old_response;
                        if let Some(v) = old_input {
                            self.options["input"] = v;
                        } else {
                            self.options.as_object_mut().unwrap().remove("input");
                        }
                        executed?;
                        let value = module_output.ok_or("Flow 模块未执行 Output")?;
                        outputs.insert("out".into(), value.clone());
                        Ok(value)
                    }
                    "http" | "sql" => {
                        let mut r = if let Some(refid) =
                            d["requestId"].as_str().filter(|s| !s.is_empty())
                        {
                            self.workspace["requests"]
                                .as_array()
                                .and_then(|a| a.iter().find(|r| r["id"] == refid))
                                .cloned()
                                .ok_or("请求不存在")?
                        } else {
                            d["request"].clone()
                        };
                        if d["requestId"].as_str().is_some_and(|s| !s.is_empty())
                            && r["kind"] != kind
                        {
                            return Err("请求类型与组件不匹配".into());
                        }
                        r["kind"] = json!(kind);
                        let parents = self.inherited(r["id"].as_str().unwrap_or(""));
                        let before = self.results.len();
                        let mut aliases = vec![];
                        for b in d["inputBindings"].as_array().into_iter().flatten() {
                            let name = b["name"].as_str().unwrap();
                            aliases.push((name, self.ctx.locals.get(name).cloned()));
                            self.ctx.locals[name] = root[name].clone();
                        }
                        self.suppress_steps = true;
                        let sent = self.request(r, &parents, &id);
                        self.suppress_steps = false;
                        for (name, previous) in aliases {
                            if let Some(v) = previous {
                                self.ctx.locals[name] = v;
                            } else {
                                self.ctx.locals.as_object_mut().unwrap().remove(name);
                            }
                        }
                        let result = self.ctx.nodes[&id].clone();
                        tests = result["tests"]
                            .as_array()
                            .cloned()
                            .map(Value::Array)
                            .unwrap_or(json!([]));
                        meta["statusCode"] = result["meta"]["statusCode"].clone();
                        meta["headers"] = result["meta"]["headers"].clone();
                        let code = result["meta"]["statusCode"].as_u64().unwrap_or(0);
                        let failed = sent.is_err()
                            || code >= 400
                            || result["tests"]
                                .as_array()
                                .is_some_and(|a| a.iter().any(|t| t["passed"] == false));
                        let port = if failed { "fail" } else { "success" };
                        if failed
                            && edges
                                .iter()
                                .any(|e| e["source"] == id && handle(e, "outputs", node) == "fail")
                        {
                            for entry in self.results.iter_mut().skip(before) {
                                entry["handled"] = json!(true);
                            }
                        } else if let Err(e) = sent {
                            return Err(e);
                        }
                        let value = if kind == "http" {
                            json!({"body":result["data"],"headers":result["meta"]["headers"],"status":code,"code":code,"responseTime":result["elapsedMs"],"error":result["error"]})
                        } else {
                            result["data"].clone()
                        };
                        outputs.insert(port.into(), value.clone());
                        if kind == "http" {
                            outputs.insert(
                                "tests".into(),
                                result["tests"]
                                    .as_array()
                                    .cloned()
                                    .map(Value::Array)
                                    .unwrap_or(json!([])),
                            );
                        }
                        Ok(value)
                    }
                    "condition" | "if" | "validate" => {
                        let port = match kind {
                            "condition" => crate::flow_blocks::condition(d, &root)?,
                            "if" => {
                                if fql::boolean(&fql::query(
                                    d["expression"].as_str().unwrap_or("false"),
                                    &root,
                                )?)? {
                                    "true".into()
                                } else {
                                    "false".into()
                                }
                            }
                            _ => {
                                let report = crate::flow_blocks::validate(d, &input)?;
                                let port = if report["passed"] == true {
                                    "pass"
                                } else {
                                    "fail"
                                };
                                meta["validation"] = report;
                                port.into()
                            }
                        };
                        meta["branch"] = json!(port);
                        outputs.insert(port, input.clone());
                        Ok(input.clone())
                    }
                    "repeat" | "foreach" | "while" => {
                        let initial = if let Some(v) =
                            values.get(if kind == "foreach" { "list" } else { "count" })
                        {
                            v.clone()
                        } else {
                            fql::query(
                                d["expression"].as_str().unwrap_or(if kind == "repeat" {
                                    "3"
                                } else {
                                    "input"
                                }),
                                &root,
                            )?
                        };
                        let limit = d["maxIterations"]
                            .as_u64()
                            .unwrap_or(1000)
                            .clamp(1, 1000000) as usize;
                        let count = match kind {
                            "foreach" => initial.as_array().ok_or("ForEach List 必须为数组")?.len(),
                            "repeat" => {
                                let v = initial.as_f64().ok_or("Count 必须为非负整数")?;
                                if v < 0.0 || v.fract() != 0.0 || v > limit as f64 {
                                    return Err("Count 无效或超过上限".into());
                                }
                                v as usize
                            }
                            _ => limit,
                        };
                        if count > limit {
                            return Err("循环次数超过上限".into());
                        }
                        let mut aggregated: HashMap<String, Vec<Value>> =
                            loop_collectors(flow, &id, 0)?
                                .into_iter()
                                .map(|id| (id, vec![]))
                                .collect();
                        let old_item = self.ctx.item.clone();
                        let old_index = self.ctx.index;
                        let mut results = vec![];
                        let repeated = (|| -> Result<(), String> {
                            for i in 0..count {
                                self.check()?;
                                if kind == "while"
                                    && !fql::boolean(&fql::query(
                                        d["expression"].as_str().unwrap_or("false"),
                                        &self.ctx.root(),
                                    )?)?
                                {
                                    break;
                                }
                                let item = if kind == "foreach" {
                                    initial[i].clone()
                                } else {
                                    json!(i)
                                };
                                self.ctx.item = item.clone();
                                self.ctx.index = i;
                                self.path.push(i);
                                let body = self.graph_events(
                                    flow,
                                    vec![
                                        (
                                            id.clone(),
                                            if kind == "foreach" {
                                                "item".into()
                                            } else {
                                                "repeat".into()
                                            },
                                            Some(item),
                                        ),
                                        (id.clone(), "index".into(), Some(json!(i))),
                                    ],
                                    tokens.clone(),
                                    false,
                                    depth + 1,
                                );
                                self.path.pop();
                                let body = body?;
                                for (nid, mut vals) in body {
                                    let list = aggregated.entry(nid).or_default();
                                    if list.len() + vals.len() > 10000 {
                                        return Err("Collect 最多收集 10000 项".into());
                                    }
                                    list.append(&mut vals);
                                }
                                results.push(self.ctx.input.clone());
                            }
                            Ok(())
                        })();
                        self.ctx.item = old_item;
                        self.ctx.index = old_index;
                        repeated?;
                        if kind == "while"
                            && results.len() == limit
                            && fql::boolean(&fql::query(
                                d["expression"].as_str().unwrap_or("false"),
                                &self.ctx.root(),
                            )?)?
                        {
                            return Err("While 超过循环次数上限".into());
                        }
                        for (cid, list) in aggregated {
                            let value = json!(list);
                            let result = json!({"status":"succeeded","data":value,"meta":{"collected":true,"loopId":id}});
                            self.ctx.nodes[&cid] = result.clone();
                            self.emit(json!({"type":"step","nodeId":cid,"path":self.path,"status":"succeeded","result":result}));
                            deliver(
                                (cid.clone(), "list".into(), Some(value)),
                                &mut tokens,
                                &mut queue,
                            );
                            deliver(
                                (cid, "finish".into(), Some(Value::Null)),
                                &mut tokens,
                                &mut queue,
                            );
                        }
                        outputs.insert("done".into(), Value::Null);
                        meta["loop"] = json!(true);
                        Ok(json!(results))
                    }
                    "collect" => {
                        if roots {
                            return Err("Collect 必须接在 Repeat / ForEach / While 循环体内".into());
                        }
                        let value = fql::query(d["expression"].as_str().unwrap_or("input"), &root)?;
                        collected.entry(id.clone()).or_default().push(value.clone());
                        meta["collecting"] = json!(true);
                        Ok(value)
                    }
                    "evaluate" => {
                        let value = fql::query(d["expression"].as_str().unwrap_or("input"), &root)?;
                        if let Some(name) = d["assignTo"].as_str().filter(|s| !s.is_empty()) {
                            self.ctx.locals[name] = value.clone();
                        }
                        outputs.insert("out".into(), value.clone());
                        Ok(value)
                    }
                    "start" | "end" => {
                        outputs.insert("out".into(), input.clone());
                        Ok(input.clone())
                    }
                    _ => {
                        let mut config = d.clone();
                        let value = if ["list", "record"].contains(&kind) {
                            let entries = d["entries"].as_array().ok_or("缺少 List/Record 项")?;
                            if entries.len() > 10000 {
                                return Err("最多支持 10000 项".into());
                            }
                            let mut list = vec![];
                            let mut object = Map::new();
                            for (i, e) in entries.iter().enumerate() {
                                let p = format!(
                                    "item:{}",
                                    e["id"]
                                        .as_str()
                                        .map(str::to_owned)
                                        .unwrap_or_else(|| i.to_string())
                                );
                                let v = if let Some(v) = values.get(&p) {
                                    v.clone()
                                } else {
                                    fql::query(
                                        e["expression"].as_str().ok_or("缺少表达式")?,
                                        &root,
                                    )?
                                };
                                if kind == "list" {
                                    list.push(v);
                                } else {
                                    let key = e["key"]
                                        .as_str()
                                        .filter(|s| !s.is_empty())
                                        .ok_or("Record 字段名不能为空")?;
                                    if object.insert(key.into(), v).is_some() {
                                        return Err("Record 字段名重复".into());
                                    }
                                }
                            }
                            if kind == "list" {
                                json!(list)
                            } else {
                                Value::Object(object)
                            }
                        } else {
                            crate::flow_blocks::evaluate(&config, &root)?
                        };
                        let value = match kind {
                            "createVariable" => {
                                self.ctx.locals[crate::flow_blocks::variable_name(d)?] =
                                    value.clone();
                                for get in nodes.iter().filter(|n| {
                                    n["data"]["kind"] == "getVariable"
                                        && (n["data"]["variableName"] == d["variableName"]
                                            || n["data"]["variableRef"] == id)
                                }) {
                                    queue.push_back(get["id"].as_str().unwrap().into());
                                }
                                value
                            }
                            "display" => {
                                meta["display"] = crate::flow_blocks::display(d, value)?;
                                input.clone()
                            }
                            "delay" => {
                                self.host.runtime.block_on(crate::flow_blocks::delay(
                                    &value,
                                    &self.host.cancel,
                                ))?;
                                input.clone()
                            }
                            "log" => {
                                let level = d["level"].as_str().unwrap_or("log");
                                if !["log", "info", "warn", "error", "debug"].contains(&level) {
                                    return Err("日志级别无效".into());
                                }
                                let message = value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string());
                                let log = json!({"type":"console","logId":uuid::Uuid::new_v4().to_string(),"nodeId":id,"path":self.path,"phase":"flow","timestamp":chrono::Utc::now().to_rfc3339(),"level":level,"message":message});
                                self.emit(log.clone());
                                self.logs.push(log);
                                if self.logs.len() > 2000 {
                                    self.logs.remove(0);
                                }
                                input.clone()
                            }
                            "response" => {
                                if self.response.is_some() {
                                    return Err("同一次运行只能执行一个 Response".into());
                                }
                                for p in ["statusCode", "headers"] {
                                    if let Some(v) = values.get(p) {
                                        config[p] = v.clone();
                                    }
                                }
                                let v = crate::flow_blocks::response(&config, value)?;
                                self.response = Some(v.clone());
                                v
                            }
                            "output" => {
                                self.output = Some(value.clone());
                                value
                            }
                            _ => value,
                        };
                        outputs.insert("out".into(), value.clone());
                        Ok(value)
                    }
                }
            })();
            let status = if self.host.cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else if outcome.is_ok() {
                "succeeded"
            } else {
                "failed"
            };
            let result = match &outcome {
                Ok(v) => {
                    json!({"status":status,"data":v,"meta":meta,"tests":tests,"elapsedMs":started.elapsed().as_millis()})
                }
                Err(e) => {
                    json!({"status":status,"error":e,"elapsedMs":started.elapsed().as_millis()})
                }
            };
            self.emit(
                json!({"type":"step","nodeId":id,"path":self.path,"status":status,"result":result}),
            );
            self.ctx.nodes[&id] = result;
            self.ctx.input = outcome?;
            if kind == "collect" {
                continue;
            }
            for p in ports(d, "outputs") {
                if meta["loop"] == true && p != "done" {
                    continue;
                }
                deliver(
                    (id.clone(), p.clone(), outputs.get(&p).cloned()),
                    &mut tokens,
                    &mut queue,
                );
            }
        }
        if roots {
            for n in nodes.iter().filter(|n| {
                !fired.contains(n["id"].as_str().unwrap())
                    && self.ctx.nodes.get(n["id"].as_str().unwrap()).is_none()
            }) {
                let id = n["id"].as_str().unwrap();
                if edges.iter().any(|e| e["target"] == id) {
                    self.emit(
                        json!({"type":"step","nodeId":id,"path":self.path,"status":"skipped"}),
                    );
                } else if n["data"]["kind"] == "getVariable"
                    && edges.iter().any(|e| e["source"] == id)
                {
                    return Err("Get Variable 引用未创建的变量".into());
                }
            }
        }
        Ok(collected)
    }
}
fn loop_collectors(flow: &Value, id: &str, depth: usize) -> Result<HashSet<String>, String> {
    if depth > 32 {
        return Err("循环嵌套超过 32 层".into());
    }
    let nodes = flow["nodes"].as_array().unwrap();
    let edges = flow["edges"].as_array().unwrap();
    let node = nodes.iter().find(|n| n["id"] == id).unwrap();
    let body = if node["data"]["kind"] == "foreach" {
        "item"
    } else {
        "repeat"
    };
    let mut queue: VecDeque<String> = edges
        .iter()
        .filter(|e| {
            e["source"] == id && [body, "index"].contains(&handle(e, "outputs", node).as_str())
        })
        .map(|e| e["target"].as_str().unwrap().into())
        .collect();
    let mut seen = HashSet::new();
    let mut collected = HashSet::new();
    while let Some(nid) = queue.pop_front() {
        if !seen.insert(nid.clone()) {
            continue;
        }
        let n = nodes.iter().find(|n| n["id"] == nid).unwrap();
        let kind = n["data"]["kind"].as_str().unwrap();
        if kind == "collect" {
            collected.insert(nid);
            continue;
        }
        if ["repeat", "foreach", "while"].contains(&kind) {
            for c in loop_collectors(flow, &nid, depth + 1)? {
                for e in edges.iter().filter(|e| e["source"] == c) {
                    queue.push_back(e["target"].as_str().unwrap().into());
                }
            }
            for e in edges
                .iter()
                .filter(|e| e["source"] == nid && handle(e, "outputs", n) == "done")
            {
                queue.push_back(e["target"].as_str().unwrap().into());
            }
        } else {
            for e in edges.iter().filter(|e| e["source"] == nid) {
                queue.push_back(e["target"].as_str().unwrap().into());
            }
        }
    }
    Ok(collected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn n(id: &str, kind: &str, extra: Value) -> Value {
        let mut data = json!({"kind":kind,"expression":"input","entries":[],"maxIterations":1000});
        for (k, v) in extra.as_object().unwrap() {
            data[k] = v.clone();
        }
        json!({"id":id,"data":data})
    }
    fn graph(mut nodes: Vec<Value>, links: Vec<(&str, &str, &str, &str)>) -> Value {
        nodes.insert(0, n("start", "start", json!({})));
        json!({"executionMode":"dataflow","nodes":nodes,"edges":links.into_iter().map(|(a,p,b,q)|json!({"source":a,"sourceHandle":p,"target":b,"targetHandle":q})).collect::<Vec<_>>()})
    }
    fn run(flow: Value) -> Value {
        execute(
            &|_| {},
            None,
            Arc::new(AtomicBool::new(false)),
            json!({"requests":[],"connections":[],"environments":[],"collections":[]}),
            "flow".into(),
            flow,
            json!({}),
        )
    }
    #[test]
    fn module_output_returns_to_calling_flow_and_recursion_is_rejected() {
        let mut child = graph(
            vec![
                n("calc", "evaluate", json!({"expression":"input + 1"})),
                n("output", "output", json!({})),
            ],
            vec![
                ("start", "out", "calc", "in"),
                ("calc", "out", "output", "in"),
            ],
        );
        child["id"] = json!("child");
        let parent = graph(
            vec![
                n("seed", "number", json!({"value":2})),
                n("call", "flow", json!({"flowId":"child"})),
                n("output", "output", json!({})),
            ],
            vec![
                ("seed", "out", "call", "in"),
                ("call", "out", "output", "in"),
            ],
        );
        let r = execute(
            &|_| {},
            None,
            Arc::new(AtomicBool::new(false)),
            json!({"requests":[],"connections":[],"environments":[],"collections":[],"flows":[child]}),
            "flow".into(),
            parent,
            json!({}),
        );
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["output"].as_f64(), Some(3.0));
        let mut recursive = graph(
            vec![
                n("call", "flow", json!({"flowId":"recursive"})),
                n("out", "output", json!({})),
            ],
            vec![("start", "out", "call", "in"), ("call", "out", "out", "in")],
        );
        recursive["id"] = json!("recursive");
        let r = execute(
            &|_| {},
            None,
            Arc::new(AtomicBool::new(false)),
            json!({"requests":[],"connections":[],"environments":[],"collections":[],"flows":[recursive.clone()]}),
            "flow".into(),
            recursive,
            json!({}),
        );
        assert_eq!(r["status"], "failed");
        assert!(r["error"].as_str().unwrap().contains("递归"));
    }
    #[test]
    fn variable_reference_uses_creator_name_and_connected_paths_extract_data() {
        let f = graph(
            vec![
                n("seed", "constant", json!({"value":{"body":{"price":5}}})),
                n(
                    "create",
                    "createVariable",
                    json!({"variableName":"renamed","inputPaths":{"in":"/body/price"}}),
                ),
                n(
                    "get",
                    "getVariable",
                    json!({"variableName":"old-name","variableRef":"create"}),
                ),
                n("out", "output", json!({})),
            ],
            vec![("seed", "out", "create", "in"), ("get", "out", "out", "in")],
        );
        let r = run(f);
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["output"], 5);
    }
    #[test]
    fn http_response_body_and_status_are_exposed_on_fail_port() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            let mut buf = [0; 2048];
            let _ = stream.read(&mut buf);
            stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"ok\":false}").unwrap();
        });
        let f = graph(
            vec![
                n(
                    "http",
                    "http",
                    json!({"request":{"id":"request","kind":"http","url":format!("http://{addr}"),"method":"GET"}}),
                ),
                n("out", "output", json!({})),
            ],
            vec![
                ("start", "out", "http", "in"),
                ("http", "fail", "out", "in"),
            ],
        );
        let r = run(f);
        thread.join().unwrap();
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["output"]["status"], 400);
        assert_eq!(r["output"]["body"]["ok"], false);
    }
    #[test]
    fn ports_route_only_selected_branch_and_or_preserves_false_and_null() {
        for truth in [true, false] {
            let f = graph(
                vec![
                    n(
                        "gate",
                        "if",
                        json!({"expression":if truth{"true"}else{"false"}}),
                    ),
                    n("yes", "evaluate", json!({"expression":"false"})),
                    n("no", "evaluate", json!({"expression":"null"})),
                    n("merge", "or", json!({})),
                    n("out", "output", json!({})),
                ],
                vec![
                    ("start", "out", "gate", "in"),
                    ("gate", "true", "yes", "in"),
                    ("gate", "false", "no", "in"),
                    ("yes", "out", "merge", "a"),
                    ("no", "out", "merge", "b"),
                    ("merge", "out", "out", "in"),
                ],
            );
            let r = run(f);
            assert_eq!(r["status"], "succeeded", "{r}");
            assert_eq!(r["data"], if truth { json!(false) } else { Value::Null });
            assert!(
                r["context"]["nodes"]
                    .get(if truth { "no" } else { "yes" })
                    .is_none()
            );
        }
    }
    #[test]
    fn or_uses_arrival_order_not_edge_order_and_does_not_wait() {
        let f = graph(
            vec![
                n("first", "string", json!({"value":"first"})),
                n("second", "string", json!({"value":"second"})),
                n("merge", "or", json!({})),
                n("out", "output", json!({})),
            ],
            vec![
                ("second", "out", "merge", "a"),
                ("first", "out", "merge", "b"),
                ("merge", "out", "out", "in"),
            ],
        );
        let r = run(f);
        assert_eq!(r["data"], "first", "{r}");
    }
    #[test]
    fn list_record_and_named_inputs_wait_for_all_connected_values() {
        let raw = "O'Brien\n\u{0} $HOME \\";
        let f = graph(
            vec![
                n("text", "string", json!({"value":raw})),
                n("num", "number", json!({"value":7})),
                n(
                    "record",
                    "record",
                    json!({"entries":[{"id":"a","key":"text","expression":"$unknown()"},{"id":"b","key":"number","expression":"null"}]}),
                ),
                n(
                    "eval",
                    "evaluate",
                    json!({"inputBindings":[{"id":"v","name":"record","expression":"$unknown()"}],"expression":"record"}),
                ),
                n("out", "output", json!({})),
            ],
            vec![
                ("text", "out", "record", "item:a"),
                ("num", "out", "record", "item:b"),
                ("record", "out", "eval", "var:v"),
                ("eval", "out", "out", "in"),
            ],
        );
        let r = run(f);
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["data"], json!({"text":raw,"number":7}));
    }
    #[test]
    fn foreach_collect_zero_and_nested_loops() {
        for values in [json!([]), json!([[1, 2], [], [3]])] {
            let f = graph(
                vec![
                    n("list", "constant", json!({"value":values})),
                    n("outer", "foreach", json!({})),
                    n("inner", "foreach", json!({})),
                    n("eval", "evaluate", json!({"expression":"input * 2"})),
                    n("inside", "collect", json!({})),
                    n("outside", "collect", json!({})),
                    n("out", "output", json!({})),
                ],
                vec![
                    ("list", "out", "outer", "list"),
                    ("outer", "item", "inner", "list"),
                    ("inner", "item", "eval", "in"),
                    ("eval", "out", "inside", "in"),
                    ("inside", "list", "outside", "in"),
                    ("outside", "list", "out", "in"),
                ],
            );
            let r = run(f);
            assert_eq!(r["status"], "succeeded", "{r}");
            assert_eq!(
                r["data"],
                if values.as_array().unwrap().is_empty() {
                    json!([])
                } else {
                    json!([[2.0, 4.0], [], [6.0]])
                }
            );
        }
    }
    #[test]
    fn repeat_and_validate_ports_preserve_order() {
        let f = graph(
            vec![
                n("count", "number", json!({"value":3})),
                n("loop", "repeat", json!({"expression":"$unknown()"})),
                n(
                    "validate",
                    "validate",
                    json!({"schema":{"type":"integer","minimum":1}}),
                ),
                n("pass", "evaluate", json!({"expression":"input * 10"})),
                n("merge", "or", json!({})),
                n("collect", "collect", json!({})),
                n("out", "output", json!({})),
            ],
            vec![
                ("count", "out", "loop", "count"),
                ("loop", "repeat", "validate", "in"),
                ("validate", "pass", "pass", "in"),
                ("pass", "out", "merge", "a"),
                ("validate", "fail", "merge", "b"),
                ("merge", "out", "collect", "in"),
                ("collect", "list", "out", "in"),
            ],
        );
        let r = run(f);
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["data"], json!([0, 10.0, 20.0]));
    }
    #[test]
    fn invalid_ports_duplicate_inputs_and_cycles_are_rejected() {
        let mut f = graph(
            vec![n("a", "evaluate", json!({})), n("b", "evaluate", json!({}))],
            vec![("start", "out", "a", "in"), ("a", "out", "b", "in")],
        );
        validate(&f).unwrap();
        f["edges"][0]["targetHandle"] = json!("missing");
        assert!(validate(&f).is_err());
        f["edges"][0]["targetHandle"] = json!("in");
        f["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source":"b","sourceHandle":"out","target":"a","targetHandle":"in"}));
        assert!(validate(&f).is_err());
    }
    #[test]
    fn transport_errors_route_to_fail_when_connected() {
        let f = graph(
            vec![
                n(
                    "http",
                    "http",
                    json!({"request":{"id":"request","kind":"http","url":"ftp://localhost","method":"GET"}}),
                ),
                n("out", "output", json!({})),
            ],
            vec![
                ("start", "out", "http", "in"),
                ("http", "fail", "out", "in"),
            ],
        );
        let r = run(f);
        assert_eq!(r["status"], "succeeded", "{r}");
        assert!(r["data"]["error"].is_string());
    }
    #[test]
    fn variable_nodes_publish_updates_and_null_values() {
        let f = graph(
            vec![
                n("null", "null", json!({})),
                n("create", "createVariable", json!({"variableName":"x"})),
                n("get", "getVariable", json!({"variableName":"x"})),
                n("out", "output", json!({})),
            ],
            vec![("null", "out", "create", "in"), ("get", "out", "out", "in")],
        );
        let r = run(f);
        assert_eq!(r["status"], "succeeded", "{r}");
        assert_eq!(r["output"], Value::Null);
        assert_eq!(r["context"]["nodes"]["get"]["status"], "succeeded");
    }
}
