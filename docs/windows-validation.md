# Windows VM 검증 기록

검증일: 2026-09-30.

## 환경

- VMware Fusion 25의 첫 번째 Windows 11 ARM VM.
- Windows 11 Pro 10.0.26100 ARM64, WebView2 148.0.3967.96.
- Rust 1.98.1, cargo-xwin 0.23.1로 Mac에서 `x86_64-pc-windows-msvc` 디버그 실행 파일을 빌드하고 VM의 x64 에뮬레이션으로 실행했다.
- 실행 검증은 관리자 권한이 없는 PowerShell에서 진행했다.
- VM의 C: 여유 공간이 처음에는 0바이트였다. 정상 종료 후 테스트용 20GB 디스크를 추가해 `E:` / `DdokttiTest`로 사용했다. 기존 디스크는 포맷하거나 파티션을 변경하지 않았다. 재부팅 후 C: 여유 공간은 약 3.3GB였다.
- 호스트에는 교차 빌드용 lld·rustup을 설치했다. Homebrew 의존성 갱신 후 기존 Rust와 LLVM의 호환성도 확인하고 Rust 1.98.1로 맞췄다. 임시 rustup 도구 체인은 별도 경로를 사용하며 셸 시작 설정은 변경하지 않았다.
- 테스트 실행 파일·검증 스크립트는 VM의 `E:\ddoktti-test`에 남겼다. 검증 후 임시 HTTP 통신을 종료하고 VM을 일시 중단했다.

## 결과

| 확인 항목 | 결과 |
| --- | --- |
| Windows x64 실행 파일 빌드 | 통과 |
| Windows에서 Rust 단위 테스트 실행 | 31개 통과 |
| 앱 반복 실행·종료 | 수정 후 3회 연속 통과, 종료 코드 0 |
| 네이티브 렌더링 | `native_window=true`, `webviews=2`, 각 동작 `error=None` |
| 걷기·달리기·졸기·간지럽히기 샘플 | 실행 확인 |
| Slack·일정·타이머·스트레칭 샘플 알림 | 실행 확인, HTML 말풍선 화면 확인 |
| 졸기 중 실제 마우스 클릭 | `mode=surprised` 확인 |
| 실제 우클릭 | `menu=true` 확인 |
| 실제 드래그 | `dragging=true` 확인 |
| 저장한 Windows 검증 스크립트 | `-Repeat 1 -TestInput` 통과 |
| 같은 수정 후 macOS 실행 | 네이티브 실행 검증 통과 |
| 최종 일반 식별자 빌드 | Windows x64·macOS 디버그 빌드 통과, 설치 패키지는 생성하지 않음 |

## 발견하고 수정한 문제

1. **시작 시 상태 등록 경쟁:** WebView2가 Rust `setup`의 상태 등록 전에 IPC를 전달해 `Companion` 조회가 패닉을 일으켰다. WebView 자동 생성을 끄고 상태를 등록한 뒤 생성하도록 바꿨다. 네이티브 실행 루프도 WebView 준비가 끝난 뒤 시작한다.
2. **교차 빌드의 Windows 매니페스트 누락:** `build.rs`의 `cfg(windows)`는 빌드 호스트를 검사한다. `CARGO_CFG_TARGET_OS`를 검사하도록 변경해 Mac에서 Windows용으로 빌드할 때도 DPI·UTF-8 매니페스트를 포함한다. 결과 실행 파일에 `PerMonitorV2, PerMonitor`가 들어 있음을 확인했다.
3. **LLVM 리소스 컴파일 오류:** 매니페스트 주석의 한글이 `llvm-rc`의 narrow string 처리에서 오류를 일으켰다. 주석을 ASCII로 변경했다.

## 재현

Mac 교차 빌드에는 Rust Windows 타깃, cargo-xwin, clang-cl, llvm-rc, lld-link가 필요하다. 해당 실행 파일들이 PATH에 있는 상태에서 저장소 루트에서 실행한다.

```sh
rustup target add x86_64-pc-windows-msvc
pnpm --filter @ddoktti/desktop tauri build --debug --no-bundle \
  --runner cargo-xwin --target x86_64-pc-windows-msvc \
  --config '{"identifier":"kr.co.plead.ddoktti-here.native-smoke","bundle":{"createUpdaterArtifacts":false}}'
```

`apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/debug/ddoktti-here.exe`와 `scripts/check-native-windows.ps1`을 테스트용 Windows로 옮긴 뒤 실행한다.

```powershell
& .\check-native-windows.ps1 -ExePath .\ddoktti-here.exe -Repeat 3 -TestInput
```

`-TestInput`은 실제 커서를 조작한다. 다른 똑띠 프로세스가 없는 테스트 데스크톱을 사용한다. 테스트 빌드는 `.native-smoke` 식별자와 `--native-smoke` 실행 인자가 함께 있어야 샘플 동작을 실행하며 실제 계정 수신 스케줄러는 시작하지 않는다. 정상 배포용 실행 파일과 구분한다.

## 남은 검증 범위

- Intel/AMD Windows 실기기와 ARM64 네이티브 바이너리. 이번 결과는 ARM Windows의 x64 에뮬레이션 결과다.
- 혼합 DPI 다중 모니터, 모니터 연결 해제, 장시간 사용, 잠금·절전 복귀, 전체 화면 앱과의 실제 조합.
- 실제 Slack·Google OAuth 및 서비스 수신, 설치·업데이트·서명.
- 종료 시 WebView2의 `Failed to unregister class Chrome_WidgetWin_0. Error = 1412` 로그가 남는다. 확인한 실행은 모두 종료 코드 0으로 종료됐고 네이티브 렌더링 오류는 없었지만, 이 WebView2 종료 로그의 원인까지 해결한 것은 아니다.

참고: [Tauri Windows 교차 빌드](https://v2.tauri.app/distribute/windows-installer/), [cargo-xwin](https://github.com/rust-cross/cargo-xwin).
