# 네이티브 캐릭터 렌더링

캐릭터는 브라우저를 포함하지 않는 `pet-native` 투명 창에 직접 그린다. 설정·계정 연결·첫 실행 화면과 알림 말풍선·메뉴는 기존 HTML UI를 유지한다. 앱 전체에서 WebView를 제거하는 변경은 아니다.

## 책임과 파일

- `src-tauri/src/native_pet/mod.rs`: 자율 행동, 클릭·깨우기·드래그, 표시 모드, 네이티브 창과 HTML 팝업의 상태 동기화.
- `native_pet/physics.rs`: 창 가림·지지면, 이동·등반·점프·낙하·착지. 좌표는 모니터 작업영역 기준 논리 좌표다.
- `native_pet/art.rs`: 승인된 SVG 경로를 resvg로 화면 배율에 맞게 래스터화하고 줄을 합성한다. 캐릭터의 가로·세로 배율은 같다.
- `native_pet/platform.rs`: macOS의 CALayer/CGImage, Windows의 UpdateLayeredWindow. premultiplied RGBA를 사용하며 Windows 전송 시 BGRA로 변환한다.
- `src/overlay.ts`: 알림·메뉴·반응 말풍선만 처리한다. 캐릭터 아틀라스, requestAnimationFrame, 이동 IPC를 사용하지 않는다.
- `src/pet-surfaces.ts`, `pet-vector.ts`, `pet-rope.ts`: 개발용 브라우저 미리보기. 실제 앱의 이동 엔진은 Rust다.

Rust/JavaScript의 Tauri 및 updater는 2.12 계열로 맞춘다. Tauri의 WebView 없는 WindowBuilder API를 사용하기 위해 `unstable` feature를 활성화한다. 의존성 갱신 때 실제 앱 빌드도 실행해 양쪽 버전의 호환성을 검사한다.

## 보폭과 재생 속도

`src/pet-gaits.json`이 Rust와 미리보기의 공통 기준이다. 400×260 원화 좌표에서 걷기 한 주기(8프레임, 1.04초)의 이동 거리는 88, 달리기 한 주기(4프레임, 0.46초)는 112다. 승인된 짧은 다리의 보폭에 맞춘 조정값으로, 실제 이동 시 미끄러짐을 비교 검토하며 수정한다.

속도는 `원화 보폭 × 캐릭터 크기 / 260 ÷ 한 주기 시간 × 속도 설정`으로 계산한다. 기본 크기 180에서는 걷기 약 59, 달리기 약 169 논리 px/s다. 이동한 실제 수평 거리를 누적해 프레임 진행 시간을 구하므로 멈추거나 경계에 막히면 제자리에서 계속 걷지 않는다. 창 자체의 이동과 드래그 거리는 걸음에 더하지 않는다. 행동 지속 시간은 별도로 관리하며 몸을 상하로 튕기거나 늘리지 않는다.

## 이동과 줄

등반은 0.8초 주기로 손 뻗기 → 당기기 → 다시 잡기를 반복한다. 앞뒤 1/4 구간은 높이를 유지하고 가운데 1/2 구간에 부드럽게 상승한다. 프레임 순서는 1 → 0 → 2 → 3이며 양쪽 방향에서 같은 타이밍을 사용한다. 창 꼭대기에 오르면 줄을 회수한다.

줄은 실제 창 모서리를 고정점으로 사용하고, 손 위치는 `assets/concepts/pet-climb-hand-guides.json`의 원본 좌표에서 변환한다. SVG 재생성 시 `pet-climb-hands.json`도 함께 생성한다. 새 원본으로 바꾸면 손 좌표를 다시 검토한다. 줄은 클릭 영역에 포함하지 않는다.

창이 이동하거나 크기가 바뀌면 고정점과 캐릭터를 함께 이동한다. 창이 닫히거나 가려지면 줄을 놓고 낙하한다. 드래그와 동작 줄이기도 줄을 해제한다. 공중에서 알림이 도착하면 안전하게 착지한 다음 말풍선을 보여준다. 준비 중인 이동 점프는 알림이 오면 취소한다.

## 입력·설정

- 클릭: 간지럽히기. 졸고 있으며 알림이 없으면 놀라서 깨어난다.
- 우클릭: HTML 메뉴. 공중에서는 착지 후 표시한다.
- 드래그: 옷자락에 매달린 프레임을 재생하며 이동한다. 상주 모드는 놓은 자리에서 물리 동작을 계속하고, 알림 전용 모드는 놓은 위치를 저장한다.
- 알림·메뉴의 hover 중에는 새 자율 이동을 시작하지 않는다.
- 동작 줄이기는 정지 포즈를 사용하되 클릭 반응의 종료 시간은 계속 흐른다.
- 화면 배율 변경 시 해당 해상도의 캐시를 사용한다. 모니터가 제거되면 사용 가능한 모니터로 복구한다.
- 네이티브 창의 입력 통과는 실제 캐릭터 영역에만 해제한다. Windows에서는 WS_EX_LAYERED를 유지하면서 WS_EX_TRANSPARENT만 변경한다.

## 비용 제어

주 실행 루프는 약 30Hz이고 UI 스레드 작업은 한 번에 하나만 예약한다. 창 목록은 150ms 간격으로 갱신한다. 같은 그림·배치일 때 다시 그리지 않고, 같은 창 위치·크기·입력 통과 상태를 반복 설정하지 않는다. SVG 프레임 비트맵 캐시는 크기별 최대 16개다.

`native_pet_metrics`는 렌더러 종류, 그린 횟수, 가동 시간과 캐시 제한을 반환한다. 실제 팝업에서는 약 4MB의 개발 미리보기 아틀라스 JS를 로딩하지 않는다. 이는 로딩 경로 확인 결과이며 CPU·메모리가 특정 비율로 줄었다는 벤치마크 결과는 아니다.

## 검증

```sh
pnpm typecheck
pnpm build
node --test tests/pet-gait.test.mjs tests/pet-vector.test.mjs tests/pet-behaviors.test.mjs tests/pet-layout.test.mjs tests/pet-surfaces.test.mjs
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
node scripts/check-ui-browser.mjs
```

브라우저 검증은 Chrome이 필요하며 `CHROME_PATH`로 경로를 지정할 수 있다. 캡처는 OS 임시 폴더의 `ddoktti-ui-review`에 저장한다.

macOS/Windows의 독립된 개발용 실행 검증:

```sh
pnpm tauri build --debug --no-bundle --config '{"identifier":"kr.co.plead.ddoktti-here.native-smoke","bundle":{"createUpdaterArtifacts":false}}'
apps/desktop/src-tauri/target/debug/ddoktti-here --native-smoke
```

Windows에서는 마지막 실행 파일에 `.exe`를 붙인다. 테스트 식별자와 실행 인자가 모두 있어야 동작하며 릴리즈 빌드에는 테스트 모드가 없다. 계정 수신 서비스를 시작하지 않고 별도 설정 공간에서 걷기·달리기·졸기·클릭 반응·Slack/Calendar/타이머/스트레칭 샘플 알림을 재생한 뒤 종료한다. 실제 서비스 수신 검증을 대신하지 않는다.

Windows의 독립된 테스트 데스크톱/VM에서는 다음 명령으로 반복 실행과 실제 마우스 입력을 검사할 수 있다. `-TestInput`은 커서를 움직이고 버튼을 누르므로 검증 중에는 다른 작업을 하지 않는다. 종료 코드, 네이티브 창 존재, 렌더링 결과, 졸기 클릭 후 놀라기, 우클릭 메뉴, 드래그 상태를 검사한다. 로그는 출력된 임시 폴더에 남긴다.

```powershell
& .\scripts\check-native-windows.ps1 -ExePath .\apps\desktop\src-tauri\target\debug\ddoktti-here.exe -Repeat 3 -TestInput
```

설정 파일의 두 WebView는 `create: false`로 두고 `setup`에서 상태 등록 → 네이티브 창 준비 → WebView 생성 → 배치 및 실행 루프 시작 순서로 생성한다. Windows WebView2는 다른 창을 생성하는 중에도 IPC를 처리하므로 상태 등록 전에 HTML을 로딩하면 `state() called before manage()`로 종료될 수 있다. 이 순서를 유지한다.

2026-09-30 확인: macOS 네이티브 실행 및 렌더링 오류 검사, Rust 이동/아트 테스트, 브라우저 팝업·개발 미리보기 검증. Windows 11 ARM VM에서 x64 실행 파일의 단위 테스트 31개, 반복 실행 및 마우스 입력 검증도 통과했다. [Windows 검증 기록](windows-validation.md)에 환경과 제한을 기록했다. 혼합 DPI 다중 모니터, Spaces·절전·잠금 및 실제 계정 수신은 아직 검증하지 않았다. Windows와 macOS CI에서 Rust 테스트와 프런트엔드 검사를 실행하도록 구성했다.

## API 참고

- [Apple CALayer contents](https://developer.apple.com/documentation/quartzcore/calayer/contents): CGImage 표시와 view/layer 상호작용. 직접 관리하는 layer-hosting view로 표시한다.
- [Apple contentsScale](https://developer.apple.com/documentation/quartzcore/calayer/contentsscale): 화면 배율에 맞는 픽셀 크기.
- [Microsoft UpdateLayeredWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow): 픽셀별 알파를 사용하는 네이티브 창 갱신.

## 다중 모니터 좌표와 Slack 포커스

macOS에서는 화면 전역 좌표는 논리 포인트이고, Tao 커서 API는 주 모니터 배율로 환산한 픽셀을 반환한다. 커서·드래그 시작점·히트 영역은 논리 포인트로 맞추고, 모니터별 물리 픽셀은 해당 모니터 배율로 변환한다. 창 위치·크기는 macOS에 LogicalPosition/LogicalSize로 전달하여 이전 화면 배율의 영향을 피한다. 렌더링은 목적지 배율의 픽셀 해상도를 유지한다. Windows는 전역 물리 픽셀 좌표를 유지한다.

배치 직후 창 위치를 읽으면 비동기 이동 이전 값이 돌아올 수 있으므로 마지막 요청한 모니터와 발 위치를 물리 엔진의 기준으로 사용한다. smoke 검사는 연결된 각 화면에서 이동 후 위치 유지와 걷기를 확인한다.

Slack 포커스 시 새 Slack 알림은 성공 처리하여 중계 서버에 ACK하며, 이미 표시 중인 Slack 알림도 닫는다. 다른 종류의 알림은 유지한다. macOS 공식 Slack은 `com.tinyspeck.slackmacgap`으로 확인한다. Windows는 `slack.exe` 또는 지원 브라우저의 활성 창 제목 중 독립적인 `Slack`/`슬랙` 구간으로 식별한다. PWA와 활성 Slack 탭을 지원하며, 제목만으로 판별하는 브라우저 방식은 URL 검증이 아니므로 제목이 같은 다른 페이지와 완벽히 구분할 수 없다. 창 제목은 저장·전송하지 않는다.

### 입력 감시와 UI 잠금 순서

백그라운드 입력 감시에서 Tauri 모니터/창 API를 호출하면 UI 스레드 응답을 동기적으로 기다릴 수 있다. 따라서 `Companion` 상태 잠금을 잡은 상태에서는 이 API를 호출하지 않는다. 2026-09-30 멈춤 사례는 입력 감시가 상태 잠금을 잡고 `primary_monitor`를 기다리는 동안 UI의 native tick이 같은 잠금을 기다린 교착 상태였다. 프로세스 스택 샘플로 확인했으며, 창 조회를 모두 잠금 밖에서 끝낸 뒤 히트 영역 판정에만 잠금을 사용하도록 수정했다.

네이티브 smoke는 실제 `companion::start` 입력/포커스 감시를 함께 실행한다. UI가 멈춰 검사가 무한 대기하지 않도록 60초 watchdog이 실패 종료한다. 계정 연결 서비스는 시작하지 않는다.
