//! 운영체제 공통 진단. 계정 식별자, 메시지, 토큰은 수집하지 않는다.
use tauri::AppHandle;

pub fn collect(app: &AppHandle) -> String {
    format!(
        "똑띠왔어요 진단 리포트\n앱 버전: {}\n운영체제: {}\n아키텍처: {}\n{}메시지·토큰·계정 식별자는 수집하지 않습니다.\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crate::native_pet::diagnostics(app)
    )
}
