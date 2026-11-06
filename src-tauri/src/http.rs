use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub fn secret(id: &str) -> Result<String, String> {
    keyring::Entry::new("boostman", id)
        .map_err(|e| e.to_string())?
        .get_password()
        .map_err(|e| format!("无法读取凭据 {id}: {e}"))
}
pub fn timeout(v: &Value) -> u64 {
    v["timeoutMs"]
        .as_u64()
        .unwrap_or(30_000)
        .clamp(1, 3_600_000)
}
pub async fn cancellable<T>(
    cancel: &AtomicBool,
    ms: u64,
    task: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".into());
    }
    tokio::select! {r=tokio::time::timeout(Duration::from_millis(ms),task)=>r.map_err(|_|"执行超时".to_string())?,_=async{loop{if cancel.load(Ordering::SeqCst){break}tokio::time::sleep(Duration::from_millis(25)).await}}=>Err("已取消".into())}
}
pub async fn execute(
    request: &Value,
    jar: Arc<reqwest::cookie::Jar>,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    cancellable(cancel, timeout(request), send(request, jar)).await
}
async fn send(r: &Value, jar: Arc<reqwest::cookie::Jar>) -> Result<Value, String> {
    let start = Instant::now();
    let url = r["url"].as_str().ok_or("缺少 URL")?;
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if !["http", "https"].contains(&parsed.scheme()) {
        return Err("仅支持 HTTP(S)".into());
    }
    let client = reqwest::Client::builder()
        .cookie_provider(jar)
        .redirect(if r["followRedirects"].as_bool().unwrap_or(true) {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()
        .map_err(|e| e.to_string())?;
    let method = r["method"]
        .as_str()
        .unwrap_or("GET")
        .parse::<reqwest::Method>()
        .map_err(|e| e.to_string())?;
    let mut req = client.request(method, parsed);
    if let Some(pairs) = r["query"].as_array() {
        for p in pairs {
            if p["enabled"] != false {
                req = req.query(&[(
                    p["key"].as_str().unwrap_or(""),
                    p["value"].as_str().unwrap_or(""),
                )]);
            }
        }
    }
    if let Some(pairs) = r["headers"].as_array() {
        for p in pairs {
            if p["enabled"] != false {
                req = req.header(
                    p["key"].as_str().unwrap_or(""),
                    p["value"].as_str().unwrap_or(""),
                );
            }
        }
    }
    let auth = &r["auth"];
    match auth["type"].as_str().unwrap_or("none") {
        "basic" => {
            req = req.basic_auth(
                auth["username"].as_str().unwrap_or(""),
                Some(secret(
                    auth["secretId"].as_str().ok_or("Basic 缺少凭据引用")?,
                )?),
            )
        }
        "bearer" => {
            req = req.bearer_auth(secret(
                auth["secretId"].as_str().ok_or("Bearer 缺少凭据引用")?,
            )?)
        }
        "none" => {}
        _ => return Err("不支持的认证方式".into()),
    }
    let body = &r["body"];
    match body["type"].as_str().unwrap_or("none") {
        "none" => {}
        "json" => {
            let s = body["text"].as_str().unwrap_or("");
            serde_json::from_str::<Value>(s).map_err(|e| format!("JSON 请求体错误: {e}"))?;
            req = req
                .header("content-type", "application/json")
                .body(s.to_owned());
        }
        "text" => req = req.body(body["text"].as_str().unwrap_or("").to_owned()),
        "form" => {
            let pairs = body["fields"].as_array().ok_or("表单字段必须为数组")?;
            let fields: Vec<_> = pairs
                .iter()
                .filter(|p| p["enabled"] != false)
                .map(|p| {
                    (
                        p["key"].as_str().unwrap_or(""),
                        p["value"].as_str().unwrap_or(""),
                    )
                })
                .collect();
            req = req.form(&fields);
        }
        "file" => {
            req = req.body(
                tokio::fs::read(body["path"].as_str().ok_or("缺少文件路径")?)
                    .await
                    .map_err(|e| e.to_string())?,
            )
        }
        "multipart" => {
            let mut form = reqwest::multipart::Form::new();
            for p in body["fields"]
                .as_array()
                .ok_or("multipart 字段必须为数组")?
            {
                if p["enabled"] == false {
                    continue;
                }
                let key = p["key"].as_str().unwrap_or("").to_owned();
                if p["type"] == "file" {
                    let path = p["value"].as_str().ok_or("缺少文件路径")?;
                    let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
                    let name = std::path::Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    form = form.part(key, reqwest::multipart::Part::bytes(bytes).file_name(name));
                } else {
                    form = form.text(key, p["value"].as_str().unwrap_or("").to_owned());
                }
            }
            req = req.multipart(form);
        }
        _ => return Err("不支持的请求体类型".into()),
    }
    let mut response = req.send().await.map_err(|e| e.to_string())?;
    let code = response.status().as_u16();
    let status_text = response
        .status()
        .canonical_reason()
        .unwrap_or("")
        .to_owned();
    let final_url = response.url().to_string();
    let headers: Vec<Value> = response
        .headers()
        .iter()
        .map(|(k, v)| json!({"key":k.as_str(),"value":v.to_str().unwrap_or("[binary]")}))
        .collect();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 20 * 1024 * 1024 {
            return Err("响应超过首版 20MB 上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let (data, raw, encoding) = match std::str::from_utf8(&bytes) {
        Ok(s) => (
            serde_json::from_str::<Value>(s).unwrap_or(json!(s)),
            s.to_owned(),
            "utf8",
        ),
        Err(_) => (
            json!({"encoding":"base64","size":bytes.len()}),
            STANDARD.encode(&bytes),
            "base64",
        ),
    };
    Ok(
        json!({"status":"succeeded","data":data,"meta":{"statusCode":code,"statusText":status_text,"headers":headers,"url":final_url,"size":bytes.len(),"encoding":encoding},"raw":raw,"elapsedMs":start.elapsed().as_millis(),"error":null}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut data = Vec::new();
        let mut buf = [0; 2048];
        loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let size = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + size {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }
    #[tokio::test]
    async fn bodies_redirects_and_cookies() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let mut captures = vec![];
            for index in 0..5 {
                let (mut s, _) = server.accept().unwrap();
                captures.push(read_request(&mut s));
                if index == 0 {
                    s.write_all(b"HTTP/1.1 302 Found\r\nLocation: /target\r\nSet-Cookie: session=ok; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap()
                } else {
                    s.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .unwrap()
                }
            }
            captures
        });
        let jar: Arc<reqwest::cookie::Jar> = Arc::default();
        let flag = AtomicBool::new(false);
        let r = execute(
            &json!({"url":format!("http://{addr}/redirect")}),
            jar.clone(),
            &flag,
        )
        .await
        .unwrap();
        assert_eq!(r["data"], "ok");
        execute(&json!({"url":format!("http://{addr}/json"),"method":"POST","body":{"type":"json","text":"{\"x\":1}"}}),jar.clone(),&flag).await.unwrap();
        execute(&json!({"url":format!("http://{addr}/form"),"method":"POST","body":{"type":"form","fields":[{"key":"a","value":"hello world"}]}}),jar.clone(),&flag).await.unwrap();
        execute(&json!({"url":format!("http://{addr}/multi"),"method":"POST","body":{"type":"multipart","fields":[{"key":"a","value":"hello"}]}}),jar,&flag).await.unwrap();
        let got = thread.join().unwrap();
        assert!(got[1].starts_with("GET /target"));
        assert!(got[1].to_lowercase().contains("cookie: session=ok"));
        assert!(got[2].contains("{\"x\":1}"));
        assert!(got[3].contains("a=hello+world"));
        assert!(got[4].contains("name=\"a\""));
        assert!(got[4].contains("hello"));
    }
    #[tokio::test]
    async fn actual_http_timeout() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let _ = read_request(&mut s);
            std::thread::sleep(Duration::from_millis(150));
        });
        let err = execute(
            &json!({"url":format!("http://{addr}"),"timeoutMs":25}),
            Arc::default(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap_err();
        thread.join().unwrap();
        assert_eq!(err, "执行超时");
    }
    #[tokio::test]
    async fn response_and_status() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0; 2048];
            let _ = s.read(&mut b);
            s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 7\r\nConnection: close\r\n\r\n{\"a\":1}").unwrap();
        });
        let r = execute(
            &json!({"url":format!("http://{addr}")}),
            Arc::default(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(r["meta"]["statusCode"], 404);
        assert_eq!(r["data"]["a"], 1);
        assert_eq!(r["status"], "succeeded");
    }
    #[tokio::test]
    async fn cancellation() {
        let flag = AtomicBool::new(true);
        let r = cancellable(&flag, 1000, async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        })
        .await;
        assert_eq!(r.unwrap_err(), "已取消");
    }
}
