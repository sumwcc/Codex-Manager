#건축학

이 문서에서는 CodexManager 현재 창고 구조, 실행 관계 및 릴리스 링크에 대해 설명합니다. 목표는 공동 작업자가 변경 사항이 어느 계층에 적용되어야 하는지 신속하게 결정하도록 돕는 것입니다.

## 1. 전체적인 형태

CodexManager은 두 가지 유형의 작동 모드로 구성됩니다.

1. 데스크탑 모드: Tauri 데스크탑 + 로컬 서비스 프로세스
2. Service 모드: 독립형 서비스 + 웹 UI, 서버와 함께 사용 가능, Docker 또는 데스크톱 환경 없이 사용 가능

통합 목표:

- 계정, 사용량, 플랫폼 키 관리
- 로컬 게이트웨이 기능 제공
- 对外兼容 OpenAI 风格入口，并适配多种上游协议

## 2. 디렉토리 구조와 책임

```text
.
├─ apps/                  # 前端与 Tauri 桌面端
│  ├─ src/                # Vite + 原生 JavaScript 前端
│  ├─ src-tauri/          # Tauri 桌面壳与原生命令桥接
│  ├─ tests/              # 前端 UI/结构测试
│  └─ dist/               # 前端构建产物
├─ crates/
│  ├─ core/               # 数据库迁移、存储基础、认证/用量底层能力
│  ├─ service/            # 本地 HTTP/RPC 服务、网关、协议适配、设置持久化
│  ├─ web/                # Web UI 服务壳，可嵌入前端静态资源
│  └─ start/              # Service 一键启动器（拉起 service + web）
├─ scripts/               # 本地构建、统一版本、测试探针、发布辅助脚本
├─ docker/                # Dockerfile 与 compose 配置
├─ assets/                # README 图片、Logo 等静态资源
└─ .github/workflows/     # CI / release workflow
```

## 3. 핵심 복합 도메인 항목 색인

### 3.1 프론트엔드 마스터 컨트롤 입구

- `apps/src/main.js`: 프런트엔드 스타트업 어셈블리 입구
- `apps/src/runtime/app-bootstrap.js`: 인터페이스 초기화 배열
- `apps/src/runtime/app-runtime.js`: 새로 고침 프로세스 및 런타임 조정
- `apps/src/settings/controller.js`：设置域门面，继续向子模块分发

### 3.2 데스크탑 셸 입구

- `apps/src-tauri/src/lib.rs`: Tauri 애플리케이션 어셈블리 항목
- `apps/src-tauri/src/settings_commands.rs`: 데스크탑 설정 브리지 명령
- `apps/src-tauri/src/service_runtime.rs`: 데스크탑 임베디드 서비스 수명주기
- `apps/src-tauri/src/rpc_client.rs`: 데스크톱 RPC 통화 인프라

### 3.3 서비스 게이트웨이 및 프로토콜 항목

- `crates/service/src/lib.rs`: 서비스 기본 입구 및 런타임 어셈블리
- `crates/service/src/http/`: HTTP 라우팅 항목
- `crates/service/src/rpc_dispatch/`: RPC 유통 입구
- `crates/service/src/gateway/mod.rs`: 게이트웨이 집계 항목
- `crates/service/src/gateway/observability/http_bridge.rs`: 요청 추적, 프로토콜 브리징, 로그 쓰기
- `crates/service/src/gateway/protocol_adapter/request_mapping.rs`: OpenAI/Codex 입력 매핑
- `crates/service/src/gateway/protocol_adapter/response_conversion.rs`: 비스트리밍 결과 총 변환 항목
- `crates/service/src/gateway/protocol_adapter/response_conversion/sse_conversion.rs`: 스트리밍 SSE 전환 항목
- `crates/service/src/gateway/protocol_adapter/response_conversion/openai_chat.rs`: OpenAI 채팅 결과 조정
- `crates/service/src/gateway/protocol_adapter/response_conversion/tool_mapping.rs`: 도구 이름 단축 및 복원

### 3.4 설정 및 실행 구성 항목

- `crates/service/src/app_settings/`: 지속성, 환경 변수 적용 범위, 런타임 동기화 설정
- `crates/service/src/web_access.rs`: Web 액세스 비밀번호 및 세션 토큰

## 4. 달리기 관계

### 4.1 데스크탑 모드

데스크탑 모드는 다음 부분으로 구성됩니다.

- `apps/src/`: 프런트엔드 UI
- `apps/src-tauri/`: 데스크탑 셸
- `crates/service/`: 현지 서비스

실행 방법:

1. 사용자가 데스크톱 응용 프로그램을 시작합니다.
2. Tauri 壳负责窗口、托盘、更新、单实例、设置桥接等桌面行为。
3. 데스크탑은 RPC 또는 로컬 주소를 통해 `codexmanager-service`과 통신합니다.
4. 프런트엔드 UI에는 계정, 사용량, 요청 로그, 설정 등의 페이지가 표시됩니다.

### 4.2 Service 모드

Service 패턴은 다음 바이너리로 구성됩니다.

- `codexmanager-service`
- `codexmanager-web`
- `codexmanager-start`

책임:

- `codexmanager-service`: 계정 관리, 게이트웨이 전달, 요청 로그, 지속성 설정 및 RPC/HTTP 인터페이스를 제공하는 핵심 서비스 프로세스입니다.
- `codexmanager-web`: Web UI 서비스 셸, 프런트 엔드 페이지와 프록시를 로컬 서비스에 직접 제공할 수 있습니다.
- `codexmanager-start`: 서비스와 웹을 동시에 실행하는 패키지 게시용 원클릭 실행 프로그램입니다.

## 5. 모듈 책임

### 5.1 `apps/src/`

주로 다음을 담당합니다.

- 页面渲染
- 사용자 상호작용
- 현황관리
- 로컬 API 호출 / Tauri 명령
- 설정 페이지 및 계정 페이지의 프런트엔드 로직

### 5.2 `apps/src-tauri/`

주로 다음을 담당합니다.

- Tauri 애플리케이션 시작
- 단일 인스턴스 제어
- 시스템 트레이 및 창 이벤트
- 데스크탑 업데이트 및 설치 프로그램 동작
- 프런트 엔드 작업을 서비스/로컬 런타임에 연결

### 5.3 `crates/core/`

주로 다음을 담당합니다.

- SQLite 마이그레이션
- 스토리지 기본 기능
- 인증/사용 등 핵심 기본 로직
- 可被 service 复用的数据访问能力

### 5.4 `crates/service/`

主要负责：

- HTTP / RPC 入口
- 계정, 사용량, API Key 관리
- 로컬 게이트웨이 기능
- 프로토콜 적응 및 업스트림 전달
- 요청 로깅 및 지속성 설정
- 런타임 구성 동기화

주요 하위 디렉터리:

- `src/gateway/`: 게이트웨이, 프로토콜 적응, 스트리밍 및 비스트리밍 변환
- `src/http/`: HTTP 라우팅 항목
- `src/rpc_dispatch/`: RPC 배포
- `src/account/`, `src/apikey/`, `src/requestlog/`, `src/usage/`: 도메인 논리

### 5.5 `crates/web/`

주로 다음을 담당합니다.

- Web UI 정적 리소스 제공
- 서비스에 마운트 또는 프록시
- 可选把 `apps/dist` 内嵌到二进制，形成单文件发布物

### 5.6 `crates/start/`

주로 다음을 담당합니다.

- Service 릴리스 패키지에서 보다 직접적인 시작 항목 제공
- 서비스와 웹의 라이프사이클 조정

## 6. 데이터 및 구성

### 6.1 데이터베이스

현재 프로젝트에서는 SQLite을 사용합니다.
데이터베이스 마이그레이션 위치는 다음과 같습니다.

- `crates/core/migrations/`

데이터베이스는 계정을 저장할 뿐만 아니라 다음을 가정합니다.

- API 키
- 요청 로그
- 토큰 통계
- 앱 설정

### 6.2 실행 구성

주요 구성 소스는 다음과 같습니다.

- 환경 변수 `CODEXMANAGER_*`
- `.env` / `codexmanager.env` 애플리케이션 실행 디렉터리
- `app_settings` 지속성 테이블
- 데스크톱 설정 페이지

현재 계약:

- 시작 전에 적용되어야 하는 구성은 환경 변수 계층에 유지됩니다.
- 런타임 조정 가능 구성은 먼저 설정 페이지 + `app_settings`를 통해 관리됩니다.
- 설정 변경 사항이 데스크톱, 프런트엔드, 서비스 전반에 경계 없이 분산되어서는 안 됩니다.

## 7. 요청 링크 개요

일반적인 요청 링크는 다음과 같습니다.

1. 클라이언트 또는 UI가 요청을 시작합니다.
2. 요청은 `crates/service`의 HTTP/RPC 계층으로 들어갑니다.
3. 게이트웨이 모듈은 전달 전략, 계좌 번호, 헤더 전략, 업스트림 프록시 등을 결정합니다.
4. 프로토콜 적응 계층은 다음 처리를 담당합니다.
   - `/v1/chat/completions`
   - `/v1/responses`
   - 스트리밍 SSE
   - 비스트리밍 JSON
   - `tool_calls` / 도구 매핑 및 집계
5. 결과는 요청 로그와 통계에 다시 기록된 다음 호출자에게 반환됩니다.

## 8. 링크 구축 및 게시

### 8.1 로컬 개발 및 빌드

프런트 엔드:

- `pnpm -C apps run dev`
- `pnpm -C apps run build`
- `pnpm -C apps run check`

녹:

- `cargo test --workspace`
- `cargo build -p codexmanager-service --release`
- `cargo build -p codexmanager-web --release`
- `cargo build -p codexmanager-start --release`

데스크탑:

- `scripts/rebuild.ps1`
- `scripts/rebuild-linux.sh`
- `scripts/rebuild-macos.sh`

### 8.2 버전 관리

버전은 현재 루트 작업공간에서 균일하게 유지관리됩니다.

- `[workspace.package].version`의 루트 `Cargo.toml`

데스크탑에서 추가 동기화:

- `apps/src-tauri/Cargo.toml`
- `apps/src-tauri/tauri.conf.json`

통합 수정 항목:

- `scripts/bump-version.ps1`

### 8.3 GitHub 릴리스

주요 출판 입구:

- `.github/workflows/release-all.yml`

책임:

- Windows / macOS / Linux 데스크탑 제품 구축
- 빌드 버전 Service 아티팩트
- GitHub 릴리스 첨부 파일 업로드
- 태그/`prerelease` 입력을 기준으로 릴리스 유형 결정

## 9. 현재의 구조적 위험

현재 창고는 다음 문제에 중점을 두어야 합니다.

1. `apps/src-tauri/src/lib.rs` 여전히 두껍고 데스크톱 셸 어셈블리와 명령 구현을 여전히 분해해야 합니다.
2. `crates/service/src/lib.rs` 구성, 런타임 동기화 및 부작용 경계가 충분히 명확하지 않습니다.
3. `crates/service/src/gateway/protocol_adapter/response_conversion.rs` 호환되는 브랜치가 많고 회귀 위험이 높습니다.
4. `.github/workflows/release-all.yml` 여전히 긴 다중 플랫폼 논리에는 지속성 제약이 필요합니다.

## 10. 제안된 변경 사항

구조적 오염을 줄이기 위해서는 다음 원칙에 따라 새로운 수요를 목표로 삼아야 합니다.

- 새로운 페이지 또는 프런트 엔드 상호 작용: 우선 순위는 `apps/src/views/`, `apps/src/services/`, `apps/src/ui/`에 해당합니다.
- 새로운 데스크탑 기능: 모든 모듈을 `lib.rs`에 계속 집어넣는 대신 `apps/src-tauri/src/`에 해당하는 독립형 모듈의 우선순위를 지정하세요.
- 새로운 설정 항목: 먼저 환경 변수, 영구 구성 또는 런타임 상태에 속하는지 확인합니다.
- 새로운 프로토콜과 호환: 게이트웨이/프로토콜 어댑터 서브모듈에 우선순위를 두어야 하며 조건부 분기를 순서 없이 계속해서 스택하지 마십시오.
- 새로운 릴리스 로직: 스크립트 그리기 또는 단계 재사용에 우선순위를 두고 세 가지 플랫폼에서 세 번 수정을 반복하지 않습니다.