//! Device client for the OAuth/event relay. No Slack app secret in the binary.
use crate::connection::{Problem, Service};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
const RELAY: &str = match option_env!("SLACK_RELAY_URL") {
    Some(s) => s,
    None => "",
};
#[derive(Default)]
struct State {
    connected: bool,
    problem: Option<Problem>,
    busy: bool,
    generation: u64,
    account: String,
    status: String,
    filters: Value,
}
pub struct Slack(Mutex<State>);
fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("kr.co.plead.ddoktti-here", "slack-device-session")
        .map_err(|_| "자격 증명 저장소 오류".into())
}
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "네트워크 초기화 실패".into())
}
fn endpoint(path: &str) -> Result<String, String> {
    let u = url::Url::parse(RELAY).map_err(|_| "Slack 연결 서버 설정이 필요해요")?;
    if u.scheme() != "https"
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err("Slack 연결 서버는 HTTPS 주소여야 해요".into());
    }
    Ok(format!("{}{path}", RELAY.trim_end_matches('/')))
}
fn publish(app: &AppHandle) {
    let _ = app.emit("slack-state", slack_status(app.clone()));
}
pub fn init(app: &AppHandle) {
    app.manage(Slack(Mutex::new(State {
        status: "연결 안 됨".into(),
        ..Default::default()
    })));
}
#[tauri::command]
pub fn slack_status(app: AppHandle) -> Value {
    let state = app.state::<Slack>();
    let s = state.0.lock().unwrap();
    json!({"configured":endpoint("").is_ok(),"connected":s.connected,"reconnecting":s.problem==Some(Problem::Reconnecting),"busy":s.busy,"account":s.account,"status":s.status,"filters":s.filters})
}
fn connect(app: &AppHandle, generation: u64) -> Result<(), String> {
    let http = client()?;
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "인증 난수 오류")?;
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let response = http
        .post(endpoint("/v1/connect")?)
        .json(&json!({"challenge":challenge}))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| "Slack 연결 서버에 접속하지 못했어요")?;
    let data: Value = response.json().map_err(|_| "연결 응답 오류")?;
    let id = data["id"]
        .as_str()
        .filter(|s| {
            s.len() < 100
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or("연결 ID 오류")?;
    let login = url::Url::parse(data["loginUrl"].as_str().ok_or("로그인 주소 오류")?)
        .map_err(|_| "로그인 주소 오류")?;
    if login.origin() != url::Url::parse(RELAY).unwrap().origin() {
        return Err("로그인 서버가 일치하지 않아요".into());
    }
    app.opener()
        .open_url(login.as_str(), None::<&str>)
        .map_err(|_| "로그인 브라우저를 열지 못했어요")?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        {
            let state = app.state::<Slack>();
            if state.0.lock().unwrap().generation != generation {
                return Err("연결을 취소했어요".into());
            }
        }
        let r = http
            .post(endpoint(&format!("/v1/connect/{id}/claim"))?)
            .json(&json!({"verifier":verifier}))
            .send()
            .map_err(|_| "Slack 로그인 상태를 확인하지 못했어요")?;
        if r.status() == reqwest::StatusCode::ACCEPTED {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        }
        if !r.status().is_success() {
            return Err(
                "Slack 연결을 완료하지 못했어요. 권한 허용 또는 관리자 승인을 확인해 주세요."
                    .into(),
            );
        }
        let data: Value = r.json().map_err(|_| "연결 응답 오류")?;
        let token = data["sessionToken"].as_str().ok_or("인증 정보가 없어요")?;
        let state = app.state::<Slack>();
        let mut s = state.0.lock().unwrap();
        if s.generation != generation {
            let _ = http
                .delete(endpoint("/v1/session")?)
                .bearer_auth(token)
                .send();
            return Err("연결을 취소했어요".into());
        }
        credential()?
            .set_password(token)
            .map_err(|_| "인증 정보를 안전하게 저장하지 못했어요")?;
        s.connected = true;
        s.account = data["account"].as_str().unwrap_or("Slack").into();
        s.status = "연결됨".into();
        return Ok(());
    }
    Err("로그인 시간이 지났어요. 다시 연결해 주세요.".into())
}
#[tauri::command]
pub async fn slack_connect(app: AppHandle) -> Result<(), String> {
    let generation = {
        let state = app.state::<Slack>();
        let mut s = state.0.lock().unwrap();
        if s.busy {
            return Err("이미 연결 중이에요".into());
        }
        s.generation += 1;
        s.busy = true;
        s.problem = None;
        s.status = "브라우저에서 연결을 완료해 주세요".into();
        s.generation
    };
    crate::native_pet::connection_status(&app, Service::Slack, None);
    publish(&app);
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || connect(&worker, generation))
        .await
        .map_err(|_| "로그인 작업 오류".to_string())
        .and_then(|r| r);
    {
        let state = app.state::<Slack>();
        let mut s = state.0.lock().unwrap();
        if s.generation == generation {
            s.busy = false;
            if let Err(ref e) = result {
                s.status = e.clone();
            }
        }
    }
    publish(&app);
    result
}
#[tauri::command]
pub async fn slack_disconnect(app: AppHandle) -> Result<(), String> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        {
            let state = worker.state::<Slack>();
            let mut s = state.0.lock().unwrap();
            s.generation += 1;
            s.busy = false;
        }
        let token = credential()?.get_password().ok();
        // Delete remote device first; retain local credentials if unreachable so removal can be retried.
        if let Some(token) = token {
            client()?
                .delete(endpoint("/v1/session")?)
                .bearer_auth(token)
                .send()
                .and_then(|r| {
                    if r.status() == reqwest::StatusCode::UNAUTHORIZED {
                        Ok(r)
                    } else {
                        r.error_for_status()
                    }
                })
                .map_err(|_| "연결 해제 서버에 접속하지 못했어요. 다시 시도해 주세요")?;
        }
        {
            let state = worker.state::<Slack>();
            let mut s = state.0.lock().unwrap();
            match credential()?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => return Err("저장된 인증 정보 삭제 실패".into()),
            }
            s.generation += 1;
            s.busy = false;
            s.connected = false;
            s.problem = None;
            s.account.clear();
            s.status = "연결 안 됨".into();
        }
        {
            let state = worker.state::<crate::companion::Companion>();
            state
                .0
                .lock()
                .unwrap()
                .alerts
                .retain(|a| a["source"] != "slack");
        }
        crate::native_pet::connection_status(&worker, Service::Slack, None);
        crate::companion::emit(&worker);
        publish(&worker);
        Ok(())
    })
    .await
    .map_err(|_| "연결 해제 작업 오류".to_string())?
}
/// Quick reply or emoji reaction to a delivered Slack alert, sent as the user
/// through the relay. Sessions created before the write scopes existed get a
/// reconnect hint instead of a silent failure.
fn act(path: &str, payload: Value) -> Result<(), String> {
    let token = credential()?
        .get_password()
        .map_err(|_| "Slack에 연결해 주세요")?;
    let r = client()?
        .post(endpoint(path)?)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .map_err(|_| "오프라인 · 잠시 후 다시 시도해 주세요")?;
    match r.status().as_u16() {
        200 => Ok(()),
        403 => Err("답글·이모지를 보내려면 설정에서 Slack을 다시 연결해 주세요".into()),
        401 => Err("Slack에 다시 연결해 주세요".into()),
        404 => Err("이 알림에는 더 이상 답할 수 없어요".into()),
        400 => Err("보낼 내용을 확인해 주세요".into()),
        _ => Err("Slack에 보내지 못했어요. 잠시 후 다시 시도해 주세요".into()),
    }
}
#[tauri::command]
pub async fn slack_reply(id: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || act("/v1/reply", json!({"id":id,"text":text})))
        .await
        .map_err(|_| "답글 작업 오류".to_string())?
}
#[tauri::command]
pub async fn slack_react(id: String, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || act("/v1/react", json!({"id":id,"name":name})))
        .await
        .map_err(|_| "이모지 작업 오류".to_string())?
}
#[tauri::command]
pub async fn slack_filters(app: AppHandle, filters: Value) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let token = credential()?
            .get_password()
            .map_err(|_| "Slack에 연결해 주세요")?;
        client()?
            .patch(endpoint("/v1/filters")?)
            .bearer_auth(token)
            .json(&filters)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|_| "Slack 알림 설정 저장 실패")?;
        {
            let state = app.state::<Slack>();
            state.0.lock().unwrap().filters = filters;
        }
        publish(&app);
        Ok(())
    })
    .await
    .map_err(|_| "설정 작업 오류".to_string())?
}
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        if endpoint("").is_err() {
            return;
        }
        let mut ack = Vec::<String>::new();
        let mut generation = 0;
        let mut failures = 0u32;
        loop {
            let mut attempted = false;
            let result = (|| -> Result<(), String> {
                let token = match credential()?.get_password() {
                    Ok(t) => t,
                    Err(_) => return Ok(()),
                };
                let g = {
                    let state = app.state::<Slack>();
                    let s = state.0.lock().unwrap();
                    if s.busy || s.problem == Some(Problem::Authorization) {
                        return Ok(());
                    }
                    s.generation
                };
                if generation != g {
                    ack.clear();
                    generation = g;
                    failures = 0;
                }
                attempted = true;
                let http = client()?;
                let r = http
                    .get(endpoint("/v1/status")?)
                    .bearer_auth(&token)
                    .send()
                    .map_err(|_| "오프라인 · 다시 연결을 기다려요".to_string())?;
                if r.status() == reqwest::StatusCode::UNAUTHORIZED {
                    let state = app.state::<Slack>();
                    let mut s = state.0.lock().unwrap();
                    if s.generation == g {
                        s.connected = false;
                        s.status = "Slack에 다시 연결해 주세요".into();
                    }
                    return Err("Slack에 다시 연결해 주세요".into());
                }
                let info: Value = r
                    .error_for_status()
                    .map_err(|_| "연결 확인 실패")?
                    .json()
                    .map_err(|_| "연결 응답 오류")?;
                let data: Value = http
                    .post(endpoint("/v1/notifications")?)
                    .bearer_auth(token)
                    .json(&json!({"ack":ack}))
                    .send()
                    .and_then(|r| r.error_for_status())
                    .map_err(|_| "Slack 소식을 가져오지 못했어요")?
                    .json()
                    .map_err(|_| "알림 응답 오류")?;
                let state = app.state::<Slack>();
                let mut s = state.0.lock().unwrap();
                if s.generation != g {
                    return Ok(());
                }
                s.connected = true;
                s.problem = None;
                s.status = "연결됨".into();
                s.account = info["account"].as_str().unwrap_or("Slack").into();
                s.filters = info["filters"].clone();
                drop(s);
                ack.clear();
                if let Some(alerts) = data["alerts"].as_array() {
                    for alert in alerts.iter().take(50) {
                        if alert["source"] == "slack"
                            && alert["id"]
                                .as_str()
                                .is_some_and(|s| s.starts_with("slack:"))
                        {
                            crate::companion::push(&app, alert.clone())?;
                            ack.push(alert["id"].as_str().unwrap().into());
                        }
                    }
                }
                // Messages already read in Slack (on any device) need no bubble here.
                if let Some(read) = data["read"].as_array() {
                    for id in read.iter().filter_map(|v| v.as_str()).take(50) {
                        if id.starts_with("slack:") {
                            let _ = crate::companion::dismiss_alert(app.clone(), id.to_string(), None);
                        }
                    }
                }
                Ok(())
            })();
            let mut notice = None;
            if attempted {
                let state = app.state::<Slack>();
                let mut s = state.0.lock().unwrap();
                if s.generation == generation && !s.busy {
                    let problem = result.err().map(|e| Problem::from_error(&e));
                    s.problem = problem;
                    if let Some(p) = problem {
                        s.status = p.status(Service::Slack).into();
                        failures = (failures + 1).min(5);
                    } else {
                        failures = 0;
                    }
                    notice = Some(problem);
                }
            }
            if let Some(problem) = notice {
                crate::native_pet::connection_status(&app, Service::Slack, problem);
            }
            publish(&app);
            // Sleep in short intervals so reconnect/disconnect changes are respected.
            let delay = if failures == 0 {
                3
            } else {
                (3u64 << failures).min(60)
            };
            for _ in 0..delay {
                std::thread::sleep(Duration::from_secs(1));
                if app.state::<Slack>().0.lock().unwrap().generation != generation {
                    break;
                }
            }
        }
    });
}
