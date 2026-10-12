//! 운영체제 공통 진단. 계정 식별자, 메시지, 토큰은 수집하지 않는다.
use tauri::{AppHandle, Manager};

/// Why the character may be off screen: the welcome, the mode and the hide rules.
fn visibility(app: &AppHandle) -> String {
    let on = |b: bool| if b { "켜짐" } else { "꺼짐" };
    let state = app.state::<crate::companion::Companion>();
    let s = state.0.lock().unwrap();
    let p = &s.saved.preferences;
    format!(
        "캐릭터 표시: 첫 설정 완료 {} · 항상 함께하기 {} · 전체 화면 감지 {} · 발표 감지 {} · 숨김 {} · 오버레이 준비 {}\n",
        on(p.onboarded),
        on(p.resident),
        on(s.fullscreen),
        on(s.presenting),
        on(s.hidden()),
        on(s.ready)
    )
}

pub fn collect(app: &AppHandle) -> String {
    format!(
        "똑띠왔어요 진단 리포트\n앱 버전: {}\n운영체제: {}\n아키텍처: {}\n{}{}메시지·토큰·계정 식별자는 수집하지 않습니다.\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        visibility(app),
        crate::native_pet::diagnostics(app)
    )
}
