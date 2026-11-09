#[cfg(test)]
mod database_tests;
mod db_config;
mod engine;
mod flow_blocks;
mod fql;
mod http;
mod oracle_adapter;
mod request_folders;
mod scripts;
mod sql;
mod sql_placeholders;
mod store;
#[cfg(test)]
mod tags_workflow_tests;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};
pub struct RunState {
    cancelled: Arc<AtomicBool>,
    snapshot: Value,
}
pub struct AppState {
    pool: sqlx::SqlitePool,
    runs: Mutex<HashMap<String, RunState>>,
}
#[tauri::command]
async fn load_workspace(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    store::load(&state.pool).await
}
#[tauri::command]
async fn save_workspace(workspace: Value, state: tauri::State<'_, AppState>) -> Result<(), String> {
    if state
        .runs
        .lock()
        .map_err(|e| e.to_string())?
        .values()
        .any(|r| r.snapshot["status"] == "running")
    {
        return Err("运行期间不能修改工作区".into());
    }
    store::save(&state.pool, &workspace).await
}
#[tauri::command]
fn set_secret(id: String, secret: String) -> Result<(), String> {
    keyring::Entry::new("boostman", &id)
        .map_err(|e| e.to_string())?
        .set_password(&secret)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_secret(id: String) -> Result<(), String> {
    keyring::Entry::new("boostman", &id)
        .map_err(|e| e.to_string())?
        .delete_credential()
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn test_connection(connection: Value) -> Result<(), String> {
    sql::test(&connection).await
}
#[tauri::command]
async fn start_run(
    kind: String,
    target: Value,
    options: Value,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    if !["request", "collection", "flow"].contains(&kind.as_str()) {
        return Err("未知运行类型".into());
    }
    let workspace = store::load(&state.pool).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut runs = state.runs.lock().map_err(|e| e.to_string())?;
        if runs.values().any(|r| r.snapshot["status"] == "running") {
            return Err("已有运行任务，请等待完成或取消".into());
        }
        if runs.len() >= 50 {
            runs.retain(|_, r| r.snapshot["status"] == "running");
        }
        runs.insert(id.clone(), RunState { cancelled: cancel.clone(), snapshot: json!({"runId":id,"status":"running","sequence":0,"events":[],"result":null}) });
    }
    let run_id = id.clone();
    std::thread::spawn(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine::execute(
                &|event| publish(&app, &run_id, event),
                Some(app.state::<AppState>().pool.clone()),
                cancel,
                workspace,
                kind,
                target,
                options,
            )
        }));
        let result = match outcome {
            Ok(v) => v,
            Err(_) => json!({"status":"failed","error":"执行线程异常退出"}),
        };
        publish(
            &app,
            &run_id,
            json!({"type":"finished","status":result["status"],"result":result}),
        );
    });
    Ok(id)
}
#[tauri::command]
fn cancel_run(run_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .runs
        .lock()
        .map_err(|e| e.to_string())?
        .get(&run_id)
        .ok_or("运行不存在")?
        .cancelled
        .store(true, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
fn get_run(run_id: String, state: tauri::State<'_, AppState>) -> Result<Value, String> {
    Ok(state
        .runs
        .lock()
        .map_err(|e| e.to_string())?
        .get(&run_id)
        .ok_or("运行不存在（重启后运行快照不会保留）")?
        .snapshot
        .clone())
}
pub fn publish(app: &tauri::AppHandle, id: &str, mut event: Value) {
    let state = app.state::<AppState>();
    if let Ok(mut runs) = state.runs.lock() {
        if let Some(run) = runs.get_mut(id) {
            let seq = run.snapshot["sequence"].as_u64().unwrap_or(0) + 1;
            event["runId"] = json!(id);
            event["sequence"] = json!(seq);
            run.snapshot["sequence"] = json!(seq);
            if event["type"] == "finished" {
                run.snapshot["status"] = event["status"].clone();
                run.snapshot["result"] = event["result"].clone();
            }
            if let Some(events) = run.snapshot["events"].as_array_mut() {
                if events.len() >= 2000 {
                    events.remove(0);
                }
                events.push(event.clone());
            }
            let _ = app.emit("run-event", &event);
        }
    }
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let path = app.path().app_data_dir()?;
            std::fs::create_dir_all(&path)?;
            let pool = tauri::async_runtime::block_on(store::open(&path.join("workspace.sqlite")))?;
            app.manage(AppState {
                pool,
                runs: Mutex::new(HashMap::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_workspace,
            save_workspace,
            set_secret,
            delete_secret,
            test_connection,
            start_run,
            cancel_run,
            get_run
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 Boostman");
}
#[cfg(all(test, target_os = "windows"))]
mod credential_tests {
    #[test]
    fn system_credential_round_trip() {
        let id = format!("boostman-test-{}", uuid::Uuid::new_v4());
        super::set_secret(id.clone(), "temporary-test-value".into()).unwrap();
        let got = crate::http::secret(&id);
        super::delete_secret(id).unwrap();
        assert_eq!(got.unwrap(), "temporary-test-value");
    }
}
