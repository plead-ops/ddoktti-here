# macOS 배포 준비

유료 Apple Developer 계정으로 Developer ID 배포 서명과 공증을 진행한다. 투명 캐릭터 창에 macOS private API를 사용하므로 현재 배포 경로는 Mac App Store 대신 직접 배포 DMG다.

## 로컬 준비

1. Apple Developer에서 **Developer ID Application** 인증서를 발급하고 해당 개인키와 함께 로그인 Keychain에 설치한다. Apple Development 인증서와 구분한다.
2. `security find-identity -v -p codesigning`으로 유효한 Developer ID Application 신원을 확인한다.
3. Apple ID의 앱 전용 암호를 생성하거나 App Store Connect API 키로 공증 인증을 준비한다. 암호·인증서 개인키를 채팅이나 저장소에 넣지 않는다.
4. Keychain 인증서가 준비되면 Apple 서명 신원과 Team ID를 환경 변수로 전달해 Tauri 빌드를 실행한다. 서명된 앱에는 실제 Slack/Google 연결 배포 설정도 필요하다.

## GitHub Actions

`macos-build`를 수동 실행한다. 기본은 서명 없는 개발용 artifact이며, `signed=true`일 때 Developer ID 서명·공증 및 검증을 수행한다. 실행만으로 GitHub Release를 만들거나 외부에 공개하지 않는다.

저장소 Actions secrets:

- `APPLE_CERTIFICATE`: Developer ID Application 인증서와 개인키를 내보낸 `.p12`의 base64 값.
- `APPLE_CERTIFICATE_PASSWORD`: 위 파일 암호.
- `APPLE_SIGNING_IDENTITY`: `Developer ID Application: … (TEAMID)` 전체 이름.
- `APPLE_ID`, `APPLE_PASSWORD` (앱 전용 암호), `APPLE_TEAM_ID`: 공증 인증.
- `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`: Google Desktop OAuth client 구성.

Actions variable `SLACK_RELAY_URL`: 배포한 HTTPS Slack 인증·이벤트 중계 서버 주소.

Apple Silicon과 Intel을 포함하는 universal app/DMG를 만든다. 이 workflow는 아직 실제 계정으로 실행·검증하지 않았다. 서명 빌드는 인증서 설치 및 공증 서버 접근이 필요하다.

## 배포 전 검증

`codesign --verify`, `stapler validate`, `spctl --assess`를 통과하고 실제 DMG 설치에서 계정 연결, 트레이, 자동 시작, 투명 창, 클릭 통과, 다중 모니터/Spaces, 드래그·착지를 확인한다.

현재 Windows `latest.json`에는 Mac 업데이트 항목이 없다. 이번 수동 Mac 빌드는 업데이터 artifact 생성을 끄며, 정식 공동 릴리즈 전에 Mac용 서명된 `.app.tar.gz`와 `darwin-aarch64` / `darwin-x86_64` 업데이트 항목을 추가해야 한다. Apple 코드 서명과 Tauri 업데이트 서명은 별개의 키다.

공식 문서: https://v2.tauri.app/distribute/sign/macos/ 및 https://v2.tauri.app/reference/config/#macosprivateapi
