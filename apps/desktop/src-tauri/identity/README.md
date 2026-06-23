# identity/ — Sparse Package(패키지 ID 부여)

이 폴더는 비패키지 Win32 앱(똑띠왔어요)에 **패키지 ID**를 부여하기 위한 것이다.
목적: Windows `UserNotificationListener`(슬랙 알림 읽기) 권한이 정상 동작하게 하는 것.
비패키지 앱은 권한 동의창을 못 띄워 알림 캡처가 조용히 실패하기 때문.

자세한 배경/결정은 커밋 히스토리와 `v0.1.6` 릴리즈 참고.

## 파일

| 파일 | 설명 | 커밋? |
|---|---|---|
| `AppxManifest.xml` | 신원 패키지 매니페스트. `userNotificationListener`(uap3) 선언. `__VERSION__`은 CI가 치환 | ✅ |
| `register-identity.ps1` | 설치 시: 인증서 신뢰 + `Add-AppxPackage`(per-user) | ✅ |
| `unregister-identity.ps1` | 제거 시: 패키지 등록 해제 | ✅ |
| `ddoktti-cert.cer` | **공개** 인증서(신뢰 등록용). 비밀 아님 | ✅ |
| `ddoktti-identity.msix` | placeholder. CI가 빌드/서명한 실제 MSIX로 교체 | ✅(자리표시) |

## 서명 인증서 (자체 서명) — ⚠️ 기억용 메모

- 방식: **자체 서명 + 설치 시 UAC 1회**(A1). 첫 설치 때만 머신 루트에 신뢰 등록, 이후 업데이트는 조용함.
- 인증서 **Subject = `CN=Plead`** — **절대 바꾸지 말 것.**
  `app.manifest`의 `<msix publisher="CN=Plead">` 및 `AppxManifest.xml`의 `Identity Publisher`와 정확히 일치해야 함.

### 개인키(pfx) 보관 위치
- **로컬 전용**: `~/ddoktti-msix-key-backup/` (이 저장소 **밖**, 커밋 안 됨)
  - `ddoktti-codesign.pfx`(개인키+인증서), `pfx-password.txt`(비번), `key.pem`/`cert.pem`
- **CI 운영용**: GitHub Actions 시크릿 `MSIX_CERT_PFX_BASE64`, `MSIX_CERT_PASSWORD`
  - ⚠️ 시크릿은 **write-only(다시 못 꺼냄)** → 백업이 아님. 복구는 로컬 pfx로만 가능.
- `.gitignore`에 `*.pfx *.pem *.key` 등록(이중 안전망). pfx/개인키는 **절대 커밋 금지**.

### 키를 잃어버리면
재발급 가능하나 thumbprint가 바뀜 → 기존 사용자 PC가 **다음 업데이트 때 UAC로 한 번 더 재신뢰**해야 함.
순서: 새 인증서 생성(Subject 그대로 `CN=Plead`) → GitHub 시크릿 2개 교체 → `ddoktti-cert.cer` 교체 커밋 → 릴리즈.

## 배포 확장 시
자체 서명은 어느 PC서든 UAC/인증서 신뢰가 필요하다. 더 넓게 배포할 거면
**Azure Trusted Signing**(무관리자·무프롬프트)으로 전환 권장.
