use rquickjs::{Context, Function, Runtime};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub struct Host {
    pub runtime: Arc<tokio::runtime::Runtime>,
    pub jar: Arc<reqwest::cookie::Jar>,
    pub cancel: Arc<AtomicBool>,
}
pub fn run(sources: &[String], seed: Value, host: &Host, timeout_ms: u64) -> Result<Value, String> {
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(32 * 1024 * 1024);
    rt.set_max_stack_size(512 * 1024);
    let start = Instant::now();
    let cancel = host.cancel.clone();
    rt.set_interrupt_handler(Some(Box::new(move || {
        cancel.load(Ordering::SeqCst) || start.elapsed() > Duration::from_millis(timeout_ms)
    })));
    let context = Context::full(&rt).map_err(|e| e.to_string())?;
    let runtime = host.runtime.clone();
    let jar = host.jar.clone();
    let cancel = host.cancel.clone();
    context.with(|ctx|->Result<(),String>{
        let host_fn=Function::new(ctx.clone(),move|source:String|->String{
            let result=(||{let mut request:Value=serde_json::from_str(&source).map_err(|e|e.to_string())?;
                let remaining=timeout_ms.saturating_sub(start.elapsed().as_millis() as u64);
                if remaining==0{return Err("脚本超时".to_string())}
                request["timeoutMs"]=json!(crate::http::timeout(&request).min(remaining));
                runtime.block_on(crate::http::execute(&request,jar.clone(),&cancel))})();
            match result{Ok(v)=>json!({"ok":true,"response":v}),Err(e)=>json!({"ok":false,"error":e})}.to_string()
        }).map_err(|e|e.to_string())?;
        ctx.globals().set("__hostSend",host_fn).map_err(|e|e.to_string())?;
        ctx.eval::<(),_>(format!("globalThis.__seed = {};",seed)).map_err(|e|e.to_string())?;
        ctx.eval::<(),_>(include_str!("scripts.js")).map_err(|e|e.to_string())?;
        ctx.eval::<(),_>(format!("globalThis.__sources = {}; globalThis.__done=false; globalThis.__scriptError=null; (async()=>{{ for (const source of __sources) {{ await (new AsyncFunction(source))(); while (__pending.length) await Promise.all(__pending.splice(0)); }} }})().then(()=>{{__done=true}},e=>{{__scriptError=String(e) + String(e.stack||'');__done=true}});",json!(sources))).map_err(|e|e.to_string())?;Ok(())
    })?;
    while rt.is_job_pending() {
        rt.execute_pending_job()
            .map_err(|_| "JavaScript 异步任务失败".to_string())?;
        if start.elapsed() > Duration::from_millis(timeout_ms) || host.cancel.load(Ordering::SeqCst)
        {
            return Err("脚本超时或已取消".into());
        }
    }
    context.with(|ctx|{
        let raw:String=ctx.eval("JSON.stringify({done:__done,error:__scriptError,request:__exportRequest(),locals:__seed.locals,environment:__seed.environment,collection:__seed.collection,tests:__tests,logs:__logs})").map_err(|e|format!("脚本超时或异常: {e}"))?;
        let value:Value=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
        if value["done"]!=true{return Err("脚本包含未完成的异步任务".into())}Ok(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Host {
        Host {
            runtime: Arc::new(tokio::runtime::Runtime::new().unwrap()),
            jar: Arc::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
    fn seed() -> Value {
        json!({"request":{"id":"req","name":"test","url":"http://localhost/","method":"GET","headers":[],"body":{"type":"none"}},"response":{"meta":{"statusCode":200,"headers":[],"encoding":"utf8"},"raw":"{\"ok\":true}","elapsedMs":1},"locals":{},"environment":{"x":1},"collection":{"x":0},"iteration":{"x":2}})
    }
    #[test]
    fn variables_request_and_assertions() {
        let result=run(&[r#"pm.expect(pm.variables.get('x')).to.equal(2);pm.variables.set('x',3);pm.environment.set('token','abc');pm.request.url.update('http://localhost/new');pm.request.headers.upsert({key:'X-Test',value:'ok'});pm.test('json',()=>pm.expect(pm.response.json()).to.deep.equal({ok:true}));pm.test('status',()=>pm.response.to.have.status(200));pm.test('failed',()=>pm.expect(1).to.equal(2));"#.into()],seed(),&host(),5000).unwrap();
        assert_eq!(result["locals"]["x"], 3);
        assert_eq!(result["environment"]["token"], "abc");
        assert_eq!(result["request"]["url"], "http://localhost/new");
        assert_eq!(result["tests"][0]["passed"], true);
        assert_eq!(result["tests"][2]["passed"], false);
    }
    #[test]
    fn interruption_and_unknown_api() {
        let h = host();
        assert!(run(&["while(true){}".into()], seed(), &h, 30).is_err());
        let err = run(&["pm.require('external')".into()], seed(), &h, 5000).unwrap();
        assert!(err["error"].as_str().unwrap().contains("不支持"), "{err}");
    }
    #[test]
    fn asynchronous_send_request() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let join = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut s, _) = server.accept().unwrap();
                let mut b = [0; 2048];
                let _ = s.read(&mut b);
                s.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\n{\"a\":1}",
                )
                .unwrap();
            }
        });
        let code = format!(
            "const r = await pm.sendRequest('http://{addr}');pm.variables.set('a',r.json().a);pm.sendRequest('http://{addr}',(err,r)=>{{if(err)throw err;pm.environment.set('b',r.json().a)}});"
        );
        let result = run(&[code], seed(), &host(), 5000).unwrap();
        join.join().unwrap();
        assert_eq!(result["locals"]["a"], 1);
        assert_eq!(result["environment"]["b"], 1);
    }
    #[test]
    fn failed_script_preserves_explicit_changes() {
        let result = run(
            &["pm.environment.set('saved',42);throw Error('failure')".into()],
            seed(),
            &host(),
            5000,
        )
        .unwrap();
        assert_eq!(result["environment"]["saved"], 42);
        assert!(result["error"].as_str().unwrap().contains("failure"));
    }
    #[test]
    fn chai_style_property_and_object_equality() {
        let result=run(&["pm.test('object',()=>pm.expect({a:1,b:2}).to.deep.equal({b:2,a:1}));pm.test('property',()=>pm.expect({a:1}).to.have.property('a').that.equals(1));".into()],seed(),&host(),5000).unwrap();
        assert!(
            result["tests"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["passed"] == true),
            "{result}"
        );
    }
}
