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

Apple Silicon과 Intel을 포함하는 universal app/DMG를 만든다. 2026-09-30에 Developer ID Application 인증서와 개인키 일치를 확인하고 암호화된 `.p12`를 생성해 서명·공증용 GitHub Secrets 6개를 등록했다. `notarytool history`로 Apple 공증 인증을 확인했으며, 첫 서명 빌드는 https://github.com/plead-ops/ddoktti-here/actions/runs/36684643026 에서 수행했다. v0.1.12 정식 빌드의 공증 ID `25e88534-7c31-4f60-956c-8f7543dc9fb5`는 Accepted이며, DMG 복사 및 업데이트 아카이브 압축 해제 후 codesign·stapler·Gatekeeper·Universal 아키텍처 검사도 통과했다. 로컬 개인키는 저장소 밖에 보관하며, CI runner의 Keychain에는 빌드 시 인증서를 가져온다.

## 배포 전 검증

`codesign --verify`, `stapler validate`, `spctl --assess`를 통과하고 실제 DMG 설치에서 계정 연결, 트레이, 자동 시작, 투명 창, 클릭 통과, 다중 모니터/Spaces, 드래그·착지를 확인한다.

수동 `macos-build`는 설치 시험용이고 자동 업데이트 릴리즈는 아래 통합 workflow를 사용한다.

공식 문서: https://v2.tauri.app/distribute/sign/macos/ 및 https://v2.tauri.app/reference/config/#macosprivateapi

## 통합 릴리즈 (0.1.12부터)

`desktop-release`는 `v*` 태그를 푸시하면 Windows NSIS와 macOS Universal 앱/DMG를 함께 빌드한다. 두 플랫폼 테스트·빌드가 모두 성공해야 게시 작업이 실행된다. macOS는 Developer ID 서명, Apple 공증, codesign·stapler·Gatekeeper 및 두 아키텍처 포함 여부를 검사한다. Windows는 이전 0.1.9 설치본에서 새 설치본으로 교체되는지 검사한다.

Mac `.app.tar.gz`와 Windows 설치 파일의 업데이터 서명을 앱에 포함된 기존 공개키로 검증하고, `windows-x86_64`·`darwin-aarch64`·`darwin-x86_64` 세 항목을 하나의 `latest.json`으로 병합하여 같은 릴리즈에 게시한다. Apple 코드 서명과 Tauri 업데이트 서명은 별개의 키다. `workflow_dispatch`는 빌드·검증된 파일만 아티팩트로 올리며 공개 릴리즈를 만들지 않는다. main push에는 `desktop-check`가 양쪽 OS의 빌드·테스트를 자동 실행한다.

릴리즈 시 package.json들, Cargo.toml/Cargo.lock, tauri.conf.json 버전을 함께 올리고 `docs/releases/v버전.md`를 작성한다. 버전 태그와 앱 버전이 다르면 패키징이 실패한다. 앱은 자동 업데이트 설정이 켜져 있으면 시작 시 확인하고 다운로드·설치·재시작한다. 실행 중 주기적인 확인은 아직 없다.

2026-09-30 첫 인증서 검증: OpenSSL 기본 PKCS#12 형식이 Keychain 가져오기에서 실패하여 3DES PBE와 SHA-1 MAC의 호환 형식으로 변환했다. 임시 Keychain 가져오기 검증 후 CI Secret을 교체했다. 시험 빌드의 공증 ID `fd777a41-67b8-458e-a878-4ba354942fa2`는 Accepted이며 codesign·stapler·Gatekeeper 검증도 통과했다. 기존 0.1.9 시험 작업은 정리 단계에서 취소되었으며 정식 배포본은 아니다.

배포 전 DMG 설치 검사에서 자동 생성된 한글 `.icns` 파일명의 NFC/NFD 변환으로 서명이 깨지는 것을 발견했다. `icons/icon.icns`를 명시하고 DMG 복사 및 업데이터 압축 해제 후 서명·공증·버전을 검사한다. v0.1.10 태그의 빌드는 공개 전에 중단했다. v0.1.11은 공증까지 완료했으나 아키텍처 검사 명령의 인자 순서 오류로 게시되지 않았다. `lipo <binary> -verify_arch arm64 x86_64`로 수정한 릴리즈는 v0.1.12다.

## v0.1.12 공개 및 자동 업데이트 실측 (2026-09-30)

- 통합 CI https://github.com/plead-ops/ddoktti-here/actions/runs/36690378082 의 Windows·Mac·게시 작업이 모두 성공했다.
- 공개 릴리즈 https://github.com/plead-ops/ddoktti-here/releases/tag/v0.1.12 에 설치 파일, 업데이트 패키지·서명, 세 플랫폼을 포함한 `latest.json`이 게시되었다.
- Windows 11 ARM VM에서 공개 0.1.9 x64 설치본을 실행하자 앱 자체 업데이터가 0.1.12로 설치하고 재시작했다. 실행 파일 ProductVersion 0.1.12, 새 프로세스 응답, 설치 프로세스 종료를 확인했다. 별도로 Windows x64 CI에서도 0.1.9 → 0.1.12 설치 교체가 통과했다. Windows Authenticode 서명 검증을 의미하지 않는다.
- Apple Silicon Mac에서 서명·공증된 0.1.9 시험 설치본을 실행하자 앱 자체 업데이터가 0.1.12로 설치하고 재시작했다. 업데이트 후 버전, 설정 창 응답, codesign·stapler·Gatekeeper를 확인했다. Intel 실행은 실기기에서 시험하지 않았고 Universal 바이너리의 두 아키텍처 포함을 검사했다.
- 0.1.9 Mac 시험본은 한글 아이콘 이름의 NFC/NFD 차이를 바로잡아 유효한 기존 서명을 복구한 뒤 사용했다. 새 0.1.12 패키지는 ASCII `icon.icns`를 사용하며 이런 수동 조정이 필요 없다.
- 이 검증은 설치·업데이트 경로 검증이며 Slack 실제 메시지 수신이나 Google OAuth 공개 심사 완료를 의미하지 않는다.
