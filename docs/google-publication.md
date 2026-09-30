# Google OAuth 공개 배포 준비

Desktop OAuth는 사용자 PC가 Google에 직접 인증하는 방식입니다. 이 방식 자체와 공개 앱의 브랜딩·권한 검증은 별개입니다. OAuth 클라이언트 발급과 빌드 값 등록만으로 공개 배포 심사가 끝나는 것은 아닙니다.

## 공개 페이지

아래 주소에 공개 사이트를 배포하고 HTTPS 응답을 확인했습니다. 웹사이트 소스는 `plead-ops/ddoktti-here-website` 저장소에서 관리하며 Dokploy가 main 변경 시 자동 배포합니다. Google Search Console 도메인 소유권 확인과 Google 재심사는 아직 완료하지 않았습니다.

| Google Auth Platform 항목 | 게시 주소 |
|---|---|
| 애플리케이션 홈페이지 | `https://ddoktti-here.plead.co.kr/` |
| 개인정보처리방침 | `https://ddoktti-here.plead.co.kr/privacy` |
| 서비스 약관 | `https://ddoktti-here.plead.co.kr/terms` |
| 승인된 도메인 | `plead.co.kr` |

홈페이지에는 똑띠왔어요의 기능, Google 데이터 이용 목적, 운영·문의 정보와 개인정보처리방침·약관 링크를 표시합니다. 페이지는 로그인 없이 누구나 읽을 수 있어야 합니다. 정책 원문은 저장소의 `PRIVACY.md`, `TERMS.md`입니다. GitHub 링크는 문서 열람에 사용할 수 있지만 `github.com`의 도메인 소유권 확인을 대신하지 않습니다.

## 심사 전 확인

1. 공개 페이지를 실제로 게시하고 모든 링크를 확인합니다.
2. OAuth 프로젝트 Owner 또는 Editor 계정으로 Google Search Console에서 도메인 소유권을 확인합니다. 도메인 속성 방식은 DNS TXT 검증을 사용합니다.
3. Google Auth Platform의 승인된 도메인과 홈페이지·정책 링크를 실제 게시 주소로 변경합니다. 앱 이름·로고·운영 정보가 사이트와 일치하도록 합니다.
4. 이번 반려 안내에 따라 소유권 확인 후 24시간을 기다린 뒤 재신청합니다.
5. Verification Center에서 브랜딩 검증과 요청 범위에 필요한 데이터 액세스 검증을 진행합니다. 캘린더 목록·일정 읽기 권한의 필요성과 실제 사용자 승인→조회→알림 흐름을 설명할 수 있어야 합니다. 콘솔에서 요구하는 시연 영상·추가 자료를 제출합니다.
6. 테스트 계정에서 실제 로그인·일정 조회·연결 해제를 확인하고 공개 배포 상태를 검토합니다. Testing 상태의 Calendar 갱신 토큰은 7일 후 만료될 수 있습니다. Production 전환과 검증 승인은 별개입니다.

Google의 승인 여부를 보장할 수는 없습니다. 설치형 OAuth를 사용한다는 이유로 공개 배포가 금지되는 것은 아니며, 이 절차 때문에 Google 인증을 중계 서버 방식으로 바꿀 필요도 없습니다.

참고: [브랜드 검증 요구사항](https://support.google.com/cloud/answer/13464321?hl=en), [설치형 OAuth](https://developers.google.com/identity/protocols/oauth2/native-app), [갱신 토큰 만료](https://developers.google.com/identity/protocols/oauth2#expiration).
