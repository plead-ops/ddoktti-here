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

## 설치 중 PowerShell 실행 경로 (2026-10-01)

구버전은 Windows 알림을 읽기 위해 `PleadDdoktti` Sparse/MSIX 패키지와 자체 서명 인증서를 등록했다. `register-identity.ps1`에서 인증서가 없을 때 관리자 권한 PowerShell을 실행하고 패키지를 등록했으므로, 당시 설치 과정에는 별도 프로세스와 UAC 창이 필요했다. 현재 Slack OAuth 방식에서는 이 패키지와 인증서 등록이 필요하지 않으며 배포 리소스에서도 제거되어 있다.

설치 훅에는 구버전 패키지를 제거하는 PowerShell 명령이 남아 있었다. 이제 `$INSTDIR\register-identity.ps1`이 남아 있는 구버전 설치에만 이 정리를 실행한다. 새 설치와 정리가 끝난 설치에서는 PowerShell을 실행하지 않는다. 정리 명령은 `nsExec::ExecToLog`와 `-WindowStyle Hidden`으로 실행한다. 종료 코드 0일 때만 구버전 스크립트·MSIX·인증서 **파일**을 삭제하고, 실패하면 다음 설치의 재시도를 위해 남긴다. 시스템 인증서 저장소는 변경하지 않는다. 자동시작 설정은 NSIS가 레지스트리를 직접 수정하므로 셸이 필요 없다.

실제 사용자가 본 창이 현재 정리 명령에서 발생했는지는 화면 관찰로 확인하지 않았다. 업그레이드 과정에서 실행되는 **이전 버전 제거 프로그램**의 동작은 새 훅 변경으로 바뀌지 않는다. 또한 WebView2가 없는 Windows에서는 Tauri가 런타임 설치를 진행할 수 있다. 설정 화면과 알림 말풍선이 HTML이므로 WebView2는 여전히 필요하다.

변경한 훅은 NSIS 3.13으로 설치·제거 섹션을 함께 포함한 Unicode 테스트 설치 파일을 생성하여 문법과 매크로 확장을 검증했다. 추가로 Windows 11 ARM VM의 별도 테스트 폴더에서 정리 매크로를 실행했다. 구버전 파일이 없는 경우 종료 코드 0(221ms), 구버전 파일 4개를 놓은 경우 종료 코드 0(약 7.2초)과 해당 파일 전체 삭제를 확인했다. 실제 사용 중인 앱 설치와 자동시작 설정은 변경하지 않았다. 이는 구버전 패키지가 없는 환경의 조건 분기 검증이며, 실제 등록된 패키지 제거와 순간적인 콘솔 표시 여부를 입증하지는 않는다.

테스트 프로세스의 PATH에만 종료 코드 7을 반환하는 PowerShell 대체 실행 파일을 넣은 실패 검증도 통과했다. 신규 설치 분기는 대체 실행 파일을 호출하지 않았고, 구버전 정리 실패 분기는 파일 4개를 모두 보존하면서 앱 설치 자체는 정상 종료했다.

## 기능 개편 Windows 재검증 (2026-10-01)

동일 Windows 11 ARM VM에서 x64 격리 식별자 `kr.co.plead.ddoktti-here.native-smoke`로 확인했다. Rust 1.98.1과 cargo-xwin으로 빌드했으며, 실행 성능 측정은 `CARGO_PROFILE_DEV_OPT_LEVEL=2 CARGO_PROFILE_DEV_DEBUG=0`을 적용했다. 현재 사용 중인 0.1.12 설치는 테스트 중에만 종료하고 종료 후 다시 실행했다.

| 항목 | 결과 |
|---|---|
| Windows Rust 단위 테스트 | 53개 통과, 실패 0 |
| 네이티브 통합 실행 | 2회 모두 종료 코드 0, `passed=true` |
| 스케줄러 실측 | 59.6 / 59.5 ticks/s, 목표 60 |
| 졸 때 클릭 / 우클릭 메뉴 / 드래그 | `surprised` / `menu=true` / `dragging=true` 확인 |
| 타이머 설정 확인 | 실제 저장 명령 후 확인 동작 표시, 잘못된 값에는 표시 안 함 |
| 알림 전용 모드 | 타이머 확인 후 자동으로 다시 숨김 |
| 5배 크기 | 실제 네이티브 크기 500 이상, 렌더링 오류 없음 |
| 모니터 이동 및 유지 | VM의 단일 디스플레이에서 통과 |
| 모니터 간 자율 이동 | VM에 인접 디스플레이가 없어 통합 검사는 건너뜀. 관련 단위 테스트는 통과 |

최적화 전 디버그 빌드는 ARM의 x64 에뮬레이션에서 36.3~40.8 ticks/s였다. 별도 Rust 타이머만 측정하면 59.99 ticks/s여서 시스템 타이머 해상도 문제는 아니었고, 최적화 빌드에서 위 수치로 개선됐다. 졸기 입력 검증의 초기 실패는 앞 단계에서 시작한 창 접근 상태가 다음 동작을 덮는 테스트 격리 문제였다. 각 단계의 물리 상태를 초기화한 뒤 재검증하여 통과했다. 종료 시 WebView2의 기존 `Chrome_WidgetWin_0` 오류 1412 로그는 여전히 남지만 네이티브 오류 없이 정상 종료했다.

같은 날 줄 앵커 위치 수정 후, `UpdateLayeredWindow`에 화면 위치·크기·픽셀을 함께 전달하는 최신 Windows 코드와 재생성한 SVG로 다시 빌드했다. 실제 입력을 포함한 추가 실행 1회가 정상 종료했고 스케줄러는 **58.9 ticks/s**였다. 깜짝 깨기·메뉴·드래그·타이머 확인·자동 숨김·5배 크기 모두 통과했다. 이 과정에서 이전 테스트가 저장한 타이머의 만료 알림이 다음 실행에 섞이는 문제도 확인하여, 격리 테스트 시작 시 저장 상태와 알림을 초기화한 뒤 재검증했다. 이 검증은 최신 Windows 표시 API 경로의 실행 확인이며 줄 앵커를 프레임별 화면 캡처로 비교한 결과는 아니다.

## 설치 권한 경로 후속 확인

`nsis.installMode=currentUser`와 `webviewInstallMode={type:downloadBootstrapper,silent:true}`를 명시했다. 기존 기본값과 같은 정책이며 이것만으로 관측된 UAC 원인이 해결됐다고 판단하지 않는다. 앱 매니페스트는 `asInvoker`, 릴리스 실행 파일은 Windows GUI 서브시스템이다. 새 설치에는 구버전 PowerShell 정리가 실행되지 않는다. Tauri NSIS 템플릿의 currentUser 분기는 `RequestExecutionLevel user`이고 기존 버전 제거 프로그램을 별도 실행하므로 이전 설치 프로그램의 동작은 새 훅으로 바뀌지 않는다. WebView2는 없을 때만 설치한다. [Microsoft 배포 문서](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution#installing-the-runtime-as-per-machine-or-per-user)에 따르면 비승격 실행은 사용자별 설치를 지원한다. 이번 후속에서는 실제 설치 UI의 UAC/콘솔 재현 검사를 추가로 실행하지 않았으므로 모든 환경에서 팝업이 사라졌다고 보장하지 않는다.
