//! Read-only automatic multi-calendar integration. Tokens never leave the native process.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::TcpListener,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

const CLIENT_ID: &str = match option_env!("GOOGLE_CLIENT_ID") {
    Some(v) => v,
    None => "",
};
const CLIENT_SECRET: &str = match option_env!("GOOGLE_CLIENT_SECRET") {
    Some(v) => v,
    None => "",
};
#[derive(Clone, Serialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    pub url: String,
    pub meeting_url: String,
}
#[derive(Default)]
pub struct State {
    connected: bool,
    busy: bool,
    generation: u64,
    status: String,
    last_sync: u64,
    events: Vec<Event>,
    fired: HashMap<String, u64>,
    known: HashMap<String, String>,
}
pub struct Calendar(Mutex<State>);
fn credential() -> Result<keyring::Entry, String> {
    keyring::Entry::new("kr.co.plead.ddoktti-here", "google-calendar-refresh")
        .map_err(|_| "OS 자격 증명 저장소를 열 수 없어요".into())
}
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "네트워크 초기화 실패".into())
}
pub fn init(app: &AppHandle) {
    let fired = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|p| std::fs::read(p.join("calendar-delivered.json")).ok())
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default();
    let known = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|p| std::fs::read(p.join("calendar-occurrences.json")).ok())
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or_default();
    app.manage(Calendar(Mutex::new(State {
        status: "연결 안 됨".into(),
        fired,
        known,
        ..Default::default()
    })));
}
fn publish(app: &AppHandle) {
    let _ = app.emit("calendar-state", calendar_status(app.clone()));
}
#[tauri::command]
pub fn calendar_status(app: AppHandle) -> Value {
    let state = app.state::<Calendar>();
    let s = state.0.lock().unwrap();
    json!({"configured":!CLIENT_ID.is_empty(),"connected":s.connected,"busy":s.busy,"status":s.status,"last_sync":s.last_sync,"events":s.events})
}
pub fn in_meeting(app: &AppHandle, t: u64) -> bool {
    let state = app.state::<Calendar>();
    let s = state.0.lock().unwrap();
    s.connected && s.events.iter().any(|e| e.start <= t && e.end > t)
}
fn random() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| "인증 난수 생성 실패")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn oauth(app: &AppHandle, generation: u64) -> Result<(), String> {
    if CLIENT_ID.is_empty() {
        return Err("이 빌드는 Google 연결 설정이 필요해요. 배포 관리자에게 문의해 주세요.".into());
    }
    let http = client()?;
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|_| "로그인 응답 포트를 열 수 없어요")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "로그인 응답 준비 실패")?;
    let redirect = format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener
            .local_addr()
            .map_err(|_| "로그인 주소 확인 실패")?
            .port()
    );
    let state = random()?;
    let verifier = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    url.query_pairs_mut().extend_pairs([
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        (
            "scope",
            "https://www.googleapis.com/auth/calendar.events.readonly https://www.googleapis.com/auth/calendar.calendarlist.readonly",
        ),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", state.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]);
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|_| "로그인 브라우저를 열 수 없어요")?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let code = loop {
        if Instant::now() > deadline {
            return Err("로그인 시간이 지났어요. 다시 연결해 주세요.".into());
        }
        {
            let s = app.state::<Calendar>();
            if s.0.lock().unwrap().generation != generation {
                return Err("연결을 취소했어요".into());
            }
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buffer = [0u8; 8192];
                let n = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..n]);
                let line = request.lines().next().unwrap_or("");
                let mut parts = line.split_whitespace();
                let method = parts.next();
                let path = parts.next().unwrap_or("");
                let callback = url::Url::parse(&format!("http://localhost{path}")).ok();
                let params: HashMap<String, String> = callback
                    .as_ref()
                    .map(|u| u.query_pairs().into_owned().collect())
                    .unwrap_or_default();
                if method != Some("GET")
                    || callback.as_ref().map(|u| u.path()) != Some("/oauth/callback")
                    || params.get("state") != Some(&state)
                {
                    let _=stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
                    continue;
                }
                let html="<!doctype html><meta charset=utf-8><title>똑띠</title><p>로그인 응답을 받았어요. 똑띠 설정 화면에서 연결 결과를 확인해 주세요.</p>";
                let response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",html.len());
                let _ = stream.write_all(response.as_bytes());
                if params.contains_key("error") {
                    return Err("Google 연결을 허용하지 않았어요".into());
                }
                break params.get("code").cloned().ok_or("로그인 코드가 없어요")?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(_) => return Err("로그인 응답을 받지 못했어요".into()),
        }
    };
    let mut form = vec![
        ("client_id", CLIENT_ID),
        ("code", code.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier.as_str()),
    ];
    if !CLIENT_SECRET.is_empty() {
        form.push(("client_secret", CLIENT_SECRET));
    }
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .map_err(|_| "Google 인증 서버에 연결할 수 없어요")?;
    if !response.status().is_success() {
        return Err("Google 인증이 완료되지 않았어요. 앱의 OAuth 설정을 확인해 주세요.".into());
    }
    let token: Value = response.json().map_err(|_| "인증 응답을 읽을 수 없어요")?;
    let refresh = token["refresh_token"]
        .as_str()
        .ok_or("오프라인 접근 권한을 받지 못했어요. 다시 연결해 주세요.")?;
    let state = app.state::<Calendar>();
    let mut s = state.0.lock().unwrap();
    if s.generation != generation {
        return Err("연결을 취소했어요".into());
    }
    credential()?
        .set_password(refresh)
        .map_err(|_| "인증 정보를 OS에 안전하게 저장하지 못했어요")?;
    s.connected = true;
    s.events.clear();
    s.last_sync = 0;
    s.status = "연결됨 · 일정 동기화 중".into();
    Ok(())
}
#[tauri::command]
pub async fn calendar_connect(app: AppHandle) -> Result<(), String> {
    let generation = {
        let state = app.state::<Calendar>();
        let mut s = state.0.lock().unwrap();
        if s.busy {
            return Err("이미 연결 중이에요".into());
        }
        s.generation += 1;
        s.busy = true;
        s.status = "브라우저에서 Google 연결을 완료해 주세요".into();
        s.generation
    };
    publish(&app);
    let worker = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || oauth(&worker, generation))
        .await
        .map_err(|_| "로그인 작업이 중단됐어요".to_string())
        .and_then(|v| v);
    {
        let state = app.state::<Calendar>();
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
pub fn calendar_disconnect(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<Calendar>();
        let mut s = state.0.lock().unwrap();
        match credential()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => return Err("저장된 인증 정보를 삭제하지 못했어요".into()),
        }
        s.generation += 1;
        s.busy = false;
        s.connected = false;
        s.events.clear();
        s.fired.clear();
        s.known.clear();
        s.last_sync = 0;
        s.status = "연결 안 됨".into();
        if let Ok(dir) = app.path().app_config_dir() {
            let _ = std::fs::remove_file(dir.join("calendar-delivered.json"));
            let _ = std::fs::remove_file(dir.join("calendar-occurrences.json"));
        }
    }
    {
        let state = app.state::<crate::companion::Companion>();
        let mut s = state.0.lock().unwrap();
        s.alerts.retain(|a| a["source"] != "calendar");
        s.snoozed.retain(|_, v| v.1["source"] != "calendar");
    }
    crate::companion::emit(&app);
    publish(&app);
    Ok(())
}
#[tauri::command]
pub fn calendar_refresh(app: AppHandle) {
    let state = app.state::<Calendar>();
    let mut s = state.0.lock().unwrap();
    s.last_sync = 0;
    s.generation += 1;
}
#[derive(Clone)]
struct CalendarSource {
    id: String,
    primary: bool,
}
fn calendar_sources(items: &[Value]) -> Result<(String, Vec<CalendarSource>), String> {
    let identity = items
        .iter()
        .find(|c| c["primary"] == true && c["deleted"] != true)
        .and_then(|c| c["id"].as_str())
        .ok_or("계정 정보를 확인하지 못했어요. Google에 다시 연결해 주세요.")?
        .to_lowercase();
    let sources = items
        .iter()
        .filter(|c| {
            c["deleted"] != true
                && matches!(
                    c["accessRole"].as_str(),
                    Some("owner" | "writer" | "writerWithoutPrivateAccess" | "reader")
                )
        })
        .filter_map(|c| {
            Some(CalendarSource {
                id: c["id"].as_str()?.into(),
                primary: c["primary"] == true,
            })
        })
        .collect();
    Ok((identity, sources))
}
fn instant(v: &Value) -> Option<u64> {
    let t = DateTime::parse_from_rfc3339(v.as_str()?).ok()?.timestamp();
    u64::try_from(t).ok()
}
fn is_me(person: &Value, source: &CalendarSource, identity: &str) -> bool {
    person["email"]
        .as_str()
        .is_some_and(|email| email.eq_ignore_ascii_case(identity))
        || (source.primary && person["self"] == true)
}
fn occurrence_key(v: &Value, source: &CalendarSource, identity: &str) -> Option<String> {
    let uid = v["iCalUID"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| v["id"].as_str().map(|id| format!("{}:{id}", source.id)))?;
    // originalStartTime identifies a recurring instance even after it is moved.
    let occurrence = if v.get("originalStartTime").is_some() {
        instant(&v["originalStartTime"]["dateTime"])
            .map(|t| t.to_string())
            .or_else(|| v["originalStartTime"]["date"].as_str().map(str::to_owned))?
    } else if v.get("recurringEventId").is_some() {
        return None;
    } else {
        "single".into()
    };
    Some(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(identity, uid, occurrence)).ok()?)
    ))
}
fn parse_event(v: &Value, source: &CalendarSource, identity: &str) -> Option<Event> {
    if v["status"] == "cancelled"
        || v.get("eventType")
            .and_then(Value::as_str)
            .is_some_and(|t| !matches!(t, "default" | "fromGmail"))
    {
        return None;
    }
    let attendees = v["attendees"].as_array();
    let mine = attendees.and_then(|a| a.iter().find(|p| is_me(p, source, identity)));
    if mine.is_some_and(|p| p["responseStatus"] == "declined") {
        return None;
    }
    let created_by_me = is_me(&v["creator"], source, identity);
    let organizer = is_me(&v["organizer"], source, identity)
        || (created_by_me && v["organizer"]["email"].as_str() == Some(source.id.as_str()));
    let attending = mine.is_some_and(|a| {
        matches!(
            a["responseStatus"].as_str(),
            Some("accepted" | "tentative" | "needsAction")
        )
    });
    let personal = attendees.is_none_or(Vec::is_empty)
        && v["attendeesOmitted"] != true
        && (source.primary || created_by_me);
    if !organizer && !attending && !personal {
        return None;
    }
    let start = instant(&v["start"]["dateTime"])?;
    let end = instant(&v["end"]["dateTime"])?;
    if end <= start {
        return None;
    }
    let meeting = v["conferenceData"]["entryPoints"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["entryPointType"] == "video"))
        .and_then(|p| p["uri"].as_str())
        .or_else(|| v["hangoutLink"].as_str())
        .unwrap_or("");
    Some(Event {
        id: occurrence_key(v, source, identity)?,
        title: v["summary"]
            .as_str()
            .unwrap_or("예정된 일정")
            .chars()
            .take(300)
            .collect(),
        start,
        end,
        url: v["htmlLink"].as_str().unwrap_or("").into(),
        meeting_url: meeting.into(),
    })
}
// Retain rejected copies too: a primary-calendar decline/cancellation must not
// be resurrected by a stale shared-calendar copy. Prefer the user's own copy.
fn source_key(v: &Value, source: &CalendarSource, identity: &str) -> Option<String> {
    Some(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(identity, &source.id, v["id"].as_str()?)).ok()?)
    ))
}
fn merge_events(
    copies: Vec<(CalendarSource, Vec<Value>)>,
    identity: &str,
    known: &mut HashMap<String, String>,
) -> Vec<Event> {
    let mut refreshed = HashMap::new();
    let mut occurrences: HashMap<String, ((bool, i64), Option<Event>)> = HashMap::new();
    for (source, items) in copies {
        for v in items {
            let local = source_key(&v, &source, identity);
            let key = if v["iCalUID"].as_str().is_none() {
                local
                    .as_ref()
                    .and_then(|key| known.get(key).cloned())
                    .or_else(|| occurrence_key(&v, &source, identity))
            } else {
                occurrence_key(&v, &source, identity)
            };
            let Some(key) = key else { continue };
            if let Some(local) = local {
                refreshed.insert(local, key.clone());
            }
            let rank = (
                source.primary,
                DateTime::parse_from_rfc3339(v["updated"].as_str().unwrap_or(""))
                    .map(|d| d.timestamp())
                    .unwrap_or(0),
            );
            let entry = occurrences
                .entry(key)
                .or_insert_with(|| (rank, parse_event(&v, &source, identity)));
            if rank > entry.0 {
                *entry = (rank, parse_event(&v, &source, identity));
            }
        }
    }
    *known = refreshed;
    let mut events = occurrences
        .into_values()
        .filter_map(|(_, event)| event)
        .collect::<Vec<_>>();
    events.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    events
}
fn read_pages(
    mut request: impl FnMut(&str) -> Result<Value, String>,
) -> Result<Vec<Value>, String> {
    let mut items = Vec::new();
    let mut page = String::new();
    let mut seen = HashSet::new();
    for _ in 0..100 {
        let data = request(&page)?;
        if let Some(values) = data["items"].as_array() {
            items.extend(values.iter().cloned());
        }
        match data["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
            Some(next) if seen.insert(next.to_string()) => page = next.into(),
            Some(_) => return Err("Google 동기화 페이지가 반복돼요. 다시 시도해 주세요.".into()),
            None => return Ok(items),
        }
    }
    Err("일정이 너무 많아 동기화를 완료하지 못했어요".into())
}
struct Fetched {
    known: HashMap<String, String>,
    events: Vec<Event>,
    calendars: usize,
    skipped: usize,
}
fn fetch(mut known: HashMap<String, String>) -> Result<Fetched, String> {
    let token = credential()?
        .get_password()
        .map_err(|_| "Google에 다시 연결해 주세요")?;
    let http = client()?;
    let mut form = vec![
        ("client_id", CLIENT_ID),
        ("refresh_token", token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if !CLIENT_SECRET.is_empty() {
        form.push(("client_secret", CLIENT_SECRET));
    }
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .map_err(|_| "오프라인 · 연결을 기다리고 있어요")?;
    if !response.status().is_success() {
        return Err("인증 갱신 실패 · Google에 다시 연결해 주세요".into());
    }
    let token: Value = response.json().map_err(|_| "Google 인증 응답 오류")?;
    let access = token["access_token"]
        .as_str()
        .ok_or("Google 인증 응답 오류")?;
    // Re-enumerate every sync, including hidden/unselected calendars. User-added
    // calendars enter automatically; removed calendars disappear from the next snapshot.
    let listed = read_pages(|page| {
        let response = http
            .get("https://www.googleapis.com/calendar/v3/users/me/calendarList")
            .bearer_auth(access)
            .query(&[
                ("showHidden", "true"),
                ("maxResults", "250"),
                ("pageToken", page),
            ])
            .send()
            .map_err(|_| "캘린더 목록을 불러오지 못했어요")?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(
                "캘린더 목록 접근 권한이 없어요. 계정 연결 시 캘린더 조회 권한을 허용해 주세요."
                    .into(),
            );
        }
        if !response.status().is_success() {
            return Err(format!(
                "캘린더 목록 동기화 실패 ({})",
                response.status().as_u16()
            ));
        }
        response.json().map_err(|_| "캘린더 목록 응답 오류".into())
    })?;
    let (identity, sources) = calendar_sources(&listed)?;
    let now = Utc::now();
    let from = (now - chrono::Duration::minutes(5)).to_rfc3339();
    let to = (now + chrono::Duration::days(7)).to_rfc3339();
    let mut copies = Vec::new();
    let mut skipped = 0;
    for source in sources {
        let mut endpoint =
            url::Url::parse("https://www.googleapis.com/calendar/v3/calendars/").unwrap();
        endpoint
            .path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .push(&source.id)
            .push("events");
        let mut denied = false;
        let events = read_pages(|page| {
            let response = http
                .get(endpoint.clone())
                .bearer_auth(access)
                .query(&[
                    ("singleEvents", "true"),
                    ("showDeleted", "true"),
                    ("showHiddenInvitations", "true"),
                    ("timeMin", from.as_str()),
                    ("timeMax", to.as_str()),
                    ("maxResults", "2500"),
                    ("pageToken", page),
                ])
                .send()
                .map_err(|_| "오프라인 · 마지막 동기화 일정을 사용해요")?;
            let status = response.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                denied = true;
                return Ok(json!({"items":[]}));
            }
            if status == reqwest::StatusCode::FORBIDDEN {
                let error: Value = response.json().unwrap_or(Value::Null);
                let permission = error["error"]["errors"].as_array().is_some_and(|errors| {
                    errors.iter().any(|e| {
                        matches!(
                            e["reason"].as_str(),
                            Some("forbidden" | "insufficientPermissions")
                        )
                    })
                });
                if permission {
                    denied = true;
                    return Ok(json!({"items":[]}));
                }
                return Err("Google 조회 제한 · 잠시 후 다시 시도해요".into());
            }
            if !status.is_success() {
                return Err(format!(
                    "일정 동기화 실패 ({}) · 잠시 후 다시 시도해요",
                    status.as_u16()
                ));
            }
            response.json().map_err(|_| "일정 응답 오류".into())
        })?;
        if denied {
            skipped += 1;
        } else {
            copies.push((source, events));
        }
    }
    let calendars = copies.len();
    Ok(Fetched {
        events: merge_events(copies, &identity, &mut known),
        known,
        calendars,
        skipped,
    })
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        // Credential store access must not block app setup or UI.
        {
            let connected = credential()
                .and_then(|c| c.get_password().map_err(|_| "없음".into()))
                .is_ok();
            let state = app.state::<Calendar>();
            let mut s = state.0.lock().unwrap();
            s.connected = connected;
            if connected {
                s.status = "연결됨 · 동기화 중".into();
            }
        }
        let mut next_attempt = 0;
        let mut failures = 0u32;
        let mut last_generation = 0;
        loop {
            let t = crate::companion::now();
            let (connected, busy, generation, last_sync) = {
                let state = app.state::<Calendar>();
                let s = state.0.lock().unwrap();
                (s.connected, s.busy, s.generation, s.last_sync)
            };
            if generation != last_generation {
                last_generation = generation;
                next_attempt = 0;
                failures = 0;
            }
            if connected
                && !busy
                && t >= next_attempt
                && (last_sync == 0 || t.saturating_sub(last_sync) >= 60)
            {
                let known = app.state::<Calendar>().0.lock().unwrap().known.clone();
                let result = fetch(known);
                {
                    let state = app.state::<Calendar>();
                    let mut s = state.0.lock().unwrap();
                    if s.generation == generation {
                        match result {
                            Ok(fetched) => {
                                s.events = fetched.events;
                                s.known = fetched.known;
                                if let Ok(dir) = app.path().app_config_dir() {
                                    let _ = std::fs::write(
                                        dir.join("calendar-occurrences.json"),
                                        serde_json::to_vec(&s.known).unwrap_or_default(),
                                    );
                                }
                                s.last_sync = t;
                                s.status = format!(
                                    "연결됨 · 캘린더 {}개 자동 확인{}",
                                    fetched.calendars,
                                    if fetched.skipped > 0 {
                                        format!(" · 접근 불가 {}개 제외", fetched.skipped)
                                    } else {
                                        String::new()
                                    }
                                );
                                failures = 0;
                            }
                            Err(e) => {
                                s.status = e;
                                failures = (failures + 1).min(5);
                            }
                        }
                    }
                }
                next_attempt = t + if failures == 0 {
                    5
                } else {
                    30 * (1u64 << failures)
                };
                publish(&app);
            }
            let prefs = {
                let state = app.state::<crate::companion::Companion>();
                let p = state.0.lock().unwrap().saved.preferences.clone();
                p
            };
            let stale = {
                let state = app.state::<Calendar>();
                let s = state.0.lock().unwrap();
                s.generation != generation || s.busy
            };
            if stale {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            let t = crate::companion::now();
            let mut due = Vec::new();
            let valid;
            let event_snapshot;
            {
                let state = app.state::<Calendar>();
                let mut s = state.0.lock().unwrap();
                event_snapshot = s.events.clone();
                valid = s
                    .events
                    .iter()
                    .map(|e| format!("calendar:{}", e.id))
                    .collect::<Vec<_>>();
                if s.connected && prefs.quiet_until <= t {
                    for e in &s.events {
                        let id = format!("calendar:{}", e.id);
                        if reminder_due(
                            e.start,
                            e.end,
                            prefs.calendar_minutes,
                            t,
                            prefs.quiet_until,
                        ) && !s.fired.contains_key(&id)
                        {
                            due.push(json!({"id":id,"source":"calendar","trigger":"calendar","title":if t<e.start{"곧 일정이 시작돼요"}else{"일정이 시작됐어요"},"body":e.title,"startsAt":e.start,"endsAt":e.end,"expiresAt":e.end,"deepLink":e.url,"meetingUrl":e.meeting_url}));
                        }
                    }
                    for alert in &due {
                        s.fired.insert(alert["id"].as_str().unwrap().into(), t);
                    }
                    s.fired.retain(|_, at| t.saturating_sub(*at) < 30 * 86400);
                    if !due.is_empty() {
                        if let Ok(dir) = app.path().app_config_dir() {
                            let _ = std::fs::write(
                                dir.join("calendar-delivered.json"),
                                serde_json::to_vec(&s.fired).unwrap_or_default(),
                            );
                        }
                    }
                }
            }
            // Remove cancelled/rescheduled occurrences, including snoozed copies.
            let changed = {
                let state = app.state::<crate::companion::Companion>();
                let mut s = state.0.lock().unwrap();
                let n = s.alerts.len();
                s.alerts
                    .retain(|a| a["source"] != "calendar" || valid.iter().any(|id| a["id"] == *id));
                s.snoozed.retain(|_, (_, a)| {
                    a["source"] != "calendar" || valid.iter().any(|id| a["id"] == *id)
                });
                let mut updated = false;
                for a in s.alerts.iter_mut() {
                    if a["source"] == "calendar" {
                        if let Some(e) = event_snapshot
                            .iter()
                            .find(|e| a["id"] == format!("calendar:{}", e.id))
                        {
                            updated |= a["body"] != e.title
                                || a["startsAt"] != e.start
                                || a["meetingUrl"] != e.meeting_url;
                            a["body"] = json!(e.title);
                            a["startsAt"] = json!(e.start);
                            a["endsAt"] = json!(e.end);
                            a["expiresAt"] = json!(e.end);
                            a["deepLink"] = json!(e.url);
                            a["meetingUrl"] = json!(e.meeting_url);
                        }
                    }
                }
                for (_, a) in s.snoozed.values_mut() {
                    if a["source"] == "calendar" {
                        if let Some(e) = event_snapshot
                            .iter()
                            .find(|e| a["id"] == format!("calendar:{}", e.id))
                        {
                            a["body"] = json!(e.title);
                            a["startsAt"] = json!(e.start);
                            a["endsAt"] = json!(e.end);
                            a["expiresAt"] = json!(e.end);
                            a["deepLink"] = json!(e.url);
                            a["meetingUrl"] = json!(e.meeting_url);
                        }
                    }
                }
                n != s.alerts.len() || updated
            };
            if changed {
                crate::companion::emit(&app);
            }
            {
                let state = app.state::<Calendar>();
                let s = state.0.lock().unwrap();
                if s.connected && s.generation == generation {
                    for alert in due {
                        let _ = crate::companion::push(&app, alert);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> CalendarSource {
        CalendarSource {
            id: "me@example.com".into(),
            primary: true,
        }
    }
    fn event() -> Value {
        json!({"id":"test","start":{"dateTime":"2026-09-29T14:00:00+09:00"},"end":{"dateTime":"2026-09-29T15:00:00+09:00"},"attendees":[{"self":true,"responseStatus":"accepted"}]})
    }
    #[test]
    fn filters_attendance_and_all_day() {
        let mut e = event();
        assert!(parse_event(&e, &source(), "me@example.com").is_some());
        e["attendees"][0]["responseStatus"] = json!("declined");
        assert!(parse_event(&e, &source(), "me@example.com").is_none());
        e["attendees"][0]["responseStatus"] = json!("needsAction");
        assert!(parse_event(&e, &source(), "me@example.com").is_some());
        e["start"] = json!({"date":"2026-09-29"});
        assert!(parse_event(&e, &source(), "me@example.com").is_none());
    }
    #[test]
    fn respects_offsets() {
        let e = parse_event(&event(), &source(), "me@example.com").unwrap();
        assert_eq!(e.end - e.start, 3600);
        assert_eq!(
            DateTime::from_timestamp(e.start as i64, 0)
                .unwrap()
                .to_rfc3339(),
            "2026-09-29T05:00:00+00:00"
        );
    }
}

// Unread calendar alerts suppressed by the user may be offered again only while
// the normal upcoming/start grace window remains valid.
pub(crate) fn defer_unread(app: &AppHandle, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let state = app.state::<Calendar>();
    let mut s = state.0.lock().unwrap();
    for id in ids {
        s.fired.remove(id);
    }
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = std::fs::write(
            dir.join("calendar-delivered.json"),
            serde_json::to_vec(&s.fired).unwrap_or_default(),
        );
    }
}

fn reminder_due(start: u64, end: u64, minutes: u64, t: u64, quiet_until: u64) -> bool {
    t >= quiet_until
        && t >= start.saturating_sub(minutes * 60)
        && t < end
        && t < start.saturating_add(300)
}
#[cfg(test)]
mod quiet_tests {
    use super::*;
    #[test]
    fn only_valid_meetings_return_after_quiet() {
        assert!(!reminder_due(1000, 3000, 5, 800, 900));
        assert!(reminder_due(1000, 3000, 5, 900, 900));
        assert!(!reminder_due(1000, 3000, 5, 1400, 1400));
        assert!(!reminder_due(1000, 1100, 5, 1150, 1100));
    }
}

#[cfg(test)]
mod multi_calendar_tests {
    use super::*;
    const ME: &str = "me@example.com";
    fn source(primary: bool) -> CalendarSource {
        CalendarSource {
            id: if primary {
                ME
            } else {
                "team@group.calendar.google.com"
            }
            .into(),
            primary,
        }
    }
    fn event() -> Value {
        json!({"id":"copy1","iCalUID":"unique@google.com","summary":"회의","updated":"2026-09-30T00:00:00Z","start":{"dateTime":"2026-09-30T14:00:00+09:00"},"end":{"dateTime":"2026-09-30T15:00:00+09:00"},"attendees":[{"email":ME,"responseStatus":"needsAction"}]})
    }
    fn merge(copies: Vec<(CalendarSource, Vec<Value>)>) -> Vec<Event> {
        merge_events(copies, ME, &mut HashMap::new())
    }
    #[test]
    fn pending_tentative_accepted_in_other_calendars() {
        for response in ["needsAction", "tentative", "accepted"] {
            let mut e = event();
            e["attendees"][0]["responseStatus"] = json!(response);
            assert!(parse_event(&e, &source(false), ME).is_some());
        }
    }
    #[test]
    fn shared_self_is_not_current_user() {
        let mut e = event();
        e["attendees"] =
            json!([{"self":true,"email":"other@example.com","responseStatus":"accepted"}]);
        e["organizer"] = json!({"self":true,"email":"team@group.calendar.google.com"});
        assert!(parse_event(&e, &source(false), ME).is_none());
    }
    #[test]
    fn organizer_and_personal_events() {
        let mut e = event();
        e["attendees"] = json!([]);
        assert!(parse_event(&e, &source(true), ME).is_some());
        assert!(parse_event(&e, &source(false), ME).is_none());
        e["creator"] = json!({"email":ME});
        assert!(parse_event(&e, &source(false), ME).is_some());
        e["creator"] = Value::Null;
        e["attendees"] = json!([{"email":"other@example.com","responseStatus":"accepted"}]);
        e["organizer"] = json!({"email":ME.to_uppercase()});
        assert!(parse_event(&e, &source(false), ME).is_some());
    }
    #[test]
    fn hidden_guest_list_is_not_personal() {
        let mut e = event();
        e["attendees"] = Value::Null;
        e["attendeesOmitted"] = json!(true);
        assert!(parse_event(&e, &source(true), ME).is_none());
    }
    #[test]
    fn same_invitation_across_calendars_only_once() {
        let e = event();
        let mut copy = e.clone();
        copy["id"] = json!("different-id");
        assert_eq!(
            merge(vec![(source(true), vec![e]), (source(false), vec![copy])]).len(),
            1
        );
    }
    #[test]
    fn same_title_different_events_stay_separate() {
        let e = event();
        let mut other = e.clone();
        other["iCalUID"] = json!("different");
        assert_eq!(merge(vec![(source(true), vec![e, other])]).len(), 2);
    }
    #[test]
    fn recurring_occurrences_are_separate_but_moves_share_identity() {
        let mut e = event();
        e["recurringEventId"] = json!("series");
        e["originalStartTime"] = e["start"].clone();
        let key = occurrence_key(&e, &source(true), ME);
        let mut moved = e.clone();
        moved["start"] = json!({"dateTime":"2026-09-30T15:00:00+09:00"});
        moved["end"] = json!({"dateTime":"2026-09-30T16:00:00+09:00"});
        assert_eq!(key, occurrence_key(&moved, &source(true), ME));
        let mut next = e.clone();
        next["originalStartTime"] = json!({"dateTime":"2026-10-01T14:00:00+09:00"});
        assert_ne!(key, occurrence_key(&next, &source(true), ME));
        let mut utc = e.clone();
        utc["originalStartTime"] = json!({"dateTime":"2026-09-30T05:00:00Z"});
        assert_eq!(key, occurrence_key(&utc, &source(true), ME));
    }
    #[test]
    fn declined_primary_copy_wins_over_stale_shared_copy() {
        let shared = event();
        let mut own = shared.clone();
        own["attendees"][0]["responseStatus"] = json!("declined");
        assert!(merge(vec![
            (source(false), vec![shared]),
            (source(true), vec![own])
        ])
        .is_empty());
    }
    #[test]
    fn cancellation_with_only_id_uses_previous_identity() {
        let mut known = HashMap::new();
        let own = event();
        let shared = event();
        assert_eq!(
            merge_events(
                vec![
                    (source(true), vec![own]),
                    (source(false), vec![shared.clone()])
                ],
                ME,
                &mut known
            )
            .len(),
            1
        );
        assert!(merge_events(
            vec![
                (
                    source(true),
                    vec![json!({"id":"copy1","status":"cancelled"})]
                ),
                (source(false), vec![shared])
            ],
            ME,
            &mut known
        )
        .is_empty());
    }
    #[test]
    fn source_list_refresh_includes_new_and_hidden_calendars() {
        let primary = json!({"id":ME,"primary":true,"accessRole":"owner"});
        let (_, before) = calendar_sources(&[primary.clone()]).unwrap();
        assert_eq!(before.len(), 1);
        let (_, after) = calendar_sources(&[
            primary,
            json!({"id":"new","accessRole":"reader","hidden":true,"selected":false}),
            json!({"id":"busy-only","accessRole":"freeBusyReader"}),
            json!({"id":"deleted","deleted":true,"accessRole":"owner"}),
        ])
        .unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[1].id, "new");
    }
    #[test]
    fn pagination_reads_empty_intermediate_pages_and_propagates_failures() {
        let mut calls = Vec::new();
        let items = read_pages(|token| {
            calls.push(token.to_string());
            Ok(match token {
                "" => json!({"items":[1],"nextPageToken":"two"}),
                "two" => json!({"nextPageToken":"three"}),
                _ => json!({"items":[2]}),
            })
        })
        .unwrap();
        assert_eq!(items, vec![json!(1), json!(2)]);
        assert_eq!(calls.len(), 3);
        assert!(read_pages(|_| Ok(json!({"nextPageToken":"same"}))).is_err());
        assert!(read_pages(|_| Err("offline".into())).is_err());
    }
    #[test]
    fn fallback_ids_are_scoped_and_accounts_do_not_collide() {
        let mut e = event();
        e.as_object_mut().unwrap().remove("iCalUID");
        assert_ne!(
            occurrence_key(&e, &source(true), ME),
            occurrence_key(&e, &source(false), ME)
        );
        assert_ne!(
            occurrence_key(&e, &source(true), ME),
            occurrence_key(&e, &source(true), "other@example.com")
        );
    }
}
