<p align="center">
  <h1 align="center">TokenSlim</h1>
  <p align="center">
    LLM 입력을 위한 고성능 Rust 토큰 압축 엔진。<br>
    플러그인 기반 · 50%–95% 토큰 절감 · AI 진단 내보내기 · CLI / Server / IDE / SDK
  </p>
</p>

<p align="center">
  <a href="https://github.com/nuoyazhizhou/tokenslim/actions/workflows/build-release.yml"><img src="https://img.shields.io/github/actions/workflow/status/nuoyazhizhou/tokenslim/build-release.yml?branch=main&logo=github&style=flat-square" alt="Build Status"></a>
  <a href="https://www.npmjs.com/package/tokenslim"><img src="https://img.shields.io/npm/v/tokenslim?logo=npm&style=flat-square" alt="npm version"></a>
  <a href="https://pypi.org/project/tokenslim/"><img src="https://img.shields.io/pypi/v/tokenslim?logo=python&style=flat-square" alt="PyPI version"></a>
  <a href="https://github.com/nuoyazhizhou/tokenslim/blob/main/LICENSE"><img src="https://img.shields.io/github/license/nuoyazhizhou/tokenslim?style=flat-square" alt="License"></a>
</p>

<p align="center">
  <a href="#tokenslim이란">TokenSlim이란?</a> ·
  <a href="#왜-tokenslim인가">왜 TokenSlim인가</a> ·
  <a href="#주요-기능">주요 기능</a> ·
  <a href="#설치">설치</a> ·
  <a href="#사용법">사용법</a> ·
  <a href="#플러그인">플러그인</a> ·
  <a href="#통합">통합</a> ·
  <a href="#라이선스">라이선스</a>
</p>

<p align="center">
  <a href="./README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.ja.md">日本語</a> · <strong>한국어</strong> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## TokenSlim이란?

TokenSlim은 Rust로 작성된 고성능 플러그인 기반 텍스트 압축 엔진입니다. 핵심 사명은 **LLM 입력의 토큰 비용을 극적으로 줄이는 것**이며, 길고 노이즈가 많은 실제 로그(빌드 파이프라인, CI 실행, 웹 액세스 로그, 데이터베이스 트레이스, 클라우드 로그, VCS 출력, 스택 트레이스 등)를 모델이 필요로 하는 진단 신호를 잃지 않고 LLM 컨텍스트 윈도우에 맞출 수 있게 합니다.

고도로 구조화되고 반복적인 입력(컴파일러 로그, 빌드 출력, CI 로그, 액세스 로그 등)에서 TokenSlim은 일반적으로 원본 정보의 100%를 보존하면서 **50%–90%** 의 감소를 제공합니다. LLM 소비 전용으로 설계된 **AI Export** 모드에서는 감소율이 **90%–95%** 에 달하며, 모델이 추론해야 하는 오류/경고 컨텍스트를 유지하는 문맥 인식 노이즈 제거를 수행합니다.

압축 외에도 TokenSlim에는 환경 진단 도구(`workspace`, `encoding`, `rule`, `env` 명령)가 함께 제공되어 OS, 셸, 코드 페이지, Python/Node/JDK 인코딩 구성을 자동 감지하고, mojibake 위험을 플래그하며, 실행 가능한 수정 사항을 출력합니다. 서브프로세스 디코딩 폴백 체인(UTF-8 우선, 코드 페이지 후보 다음)과 결합되어 다국어 혼합 환경에서도 안정성을 유지합니다.


## 작동 모습 보기

### 실제 일상적인 사용 — `tokenslim gain`

다음은 git 명령에서 몇 달간 매일 사용한 후의 `tokenslim gain` 모습입니다:

```
$ tokenslim gain

TokenSlim Cumulative Savings Report
====================================

Usage Statistics:
  Total runs:          7,244
  Input tokens:        13.2M
  Output tokens:       9.4M
  Tokens saved:        3.9M
  Overall compression: 29.3%

Estimated Savings:
  Tokens saved:        3,883,551 tokens
       claude-4.8:     $19.42 USD ($5.00/1M)
       gpt-5.5:        $19.42 USD ($5.00/1M)
       gemini-3.1-pro: $7.77 USD  ($2.00/1M)
```

> 💡 `tokenslim gain`은 실행하는 **모든 압축**을 추적하고 누적 절약량을 표시합니다. 위의 숫자는 한 개발자의 일상적인 워크플로우에서 나온 것입니다 — 팀의 절약량은 여기서부터 배가됩니다.

### 입력 유형에 따라 다른 압축률

모든 입력이 동일하게 압축되는 것은 아니며 — 이는 당연한 일입니다. 반복적이고 구조화된 로그는 git diff와 같이 정보가 조밀한 콘텐츠보다 훨씬 더 많이 압축됩니다:

<table>
<tr>
<th>입력 유형</th>
<th>일반적인 감소율</th>
<th>이유</th>
</tr>
<tr>
<td>🔨 빌드 로그 (cargo, gcc, gradle)</td>
<td align="center"><strong>70–95%</strong></td>
<td>엄청난 반복: 타임스탬프, 진행률 표시줄, 일상적인 출력</td>
</tr>
<tr>
<td>🌐 웹 액세스 로그 (Nginx, Apache)</td>
<td align="center"><strong>80–93%</strong></td>
<td>반복적인 구조: IP, 경로, 상태 코드, 사용자 에이전트</td>
</tr>
<tr>
<td>🤖 CI/CD 로그 (GitHub Actions, Jenkins)</td>
<td align="center"><strong>70–92%</strong></td>
<td>설정 단계, 의존성 설치, 상용구 출력</td>
</tr>
<tr>
<td>☁️ 클라우드 로그 (AWS, GCP, Azure)</td>
<td align="center"><strong>60–90%</strong></td>
<td>반복적인 필드 및 메타데이터가 있는 구조화된 JSON</td>
</tr>
<tr>
<td>🔀 VCS 출력 (git log, git diff)</td>
<td align="center"><strong>20–40%</strong></td>
<td>정보의 밀도가 높음; 제거할 중복성이 적음</td>
</tr>
</table>

> 전체 범위는 입력이 얼마나 반복적이고 구조화되어 있는지에 따라 **20–95%**입니다. `tokenslim gain`을 사용하여 시간이 지남에 따른 실제 절약량을 추적하세요.

**이전** — `git status` (22줄, ~680자):
```
$ git status
On branch master
Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        modified:   .gitignore
        modified:   src/core/dictionary_engine/test.rs
        modified:   src/plugins/cloud_log_plugin/test.rs

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   Cargo.toml
        modified:   resources/messages.zh-CN.json
        modified:   src/bin/tokenslim-server.rs
        modified:   src/core/plugin_config_loader/mod.rs

Untracked files:
  (use "git add <file>..." to include in what will be committed)
        tests/server_webui_e2e.rs
        webui/
```

**이후** — `tokenslim git status` (8줄, ~280자 — 동일한 정보, 손실 제로):
```
git status
BR:master
M .gitignore
M src/core/dictionary_engine/test.rs
M src/plugins/cloud_log_plugin/test.rs
M Cargo.toml
M resources/messages.zh-CN.json
M src/bin/tokenslim-server.rs
M src/core/plugin_config_loader/mod.rs
? tests/server_webui_e2e.rs
? webui/
```

> 모든 개발자는 하루에 수십 번 `git status`를 실행합니다. TokenSlim은 상용구 힌트를 제거하고 상태 마커를 통합하여 **~60% 적은 토큰**으로 동일한 정보를 제공합니다 — 그리고 이것은 수천 번의 LLM 상호 작용에 걸쳐 누적됩니다.
\n## 왜 TokenSlim인가?

### 1. 진짜 비용 절감
LLM API 비용은 입력 토큰 수에 의해 좌우됩니다. TokenSlim은 이를 50%–95% 줄입니다:

- **API 비용 절감** — 입력 토큰 50%–95% 감소.
- **문맥 인식 AI Export (`--ai-export`)** — 일상적인 줄을 제거하고 모델이 실제로 필요로 하는 오류/경고 윈도우를 유지. 노이즈가 많은 입력에서 환각을 줄임.
- **더 긴 유효 컨텍스트** — 동일한 컨텍스트 윈도우에서 더 많은 실제 신호.
- **더 빠른 프리필** — 입력이 짧을수록 일반적으로 모델 프리필이 빨라지고 TTFT가 낮아짐.

### 2. 산업 등급 성능
- **제로 카피 파이프라인** — Rust `Cow<'a, str>`, `rayon` 병렬 블록 처리, `Bump` 아레나 할당 위에 구축. 산업 등급 로그 100MB를 **약 250ms**, 약 400MB/s 처리량으로 처리.
- **결정론적 글로벌 재정렬** — 스트리밍 빌드 대상 추적기가 `make -jN` / `Ninja`가 생성한 순서가 맞지 않는 인터리빙을 수정. 동일한 병렬 빌드 두 번이 항상 동일한 오류 스택 순서를 생성.
- **사이드카 모드** — 고처리량 REST API 서버, 시작 오버헤드 없이 IDE / CI / Agent 워크플로에 임베드 가능.

### 3. 데이터 기반 추출
- **래딕스 트라이 경로 추출** — TokenSlim은 줄별로 슬라이스하지 않습니다. 100MB의 입력을 스캔한 후 프로젝트 전체 래딕스 트라이를 메모리에 구축하고 핫 브랜치(가중치 > 10)에서만 디렉토리 사전(`$D`)을 출력하여 파편화된 토큰을 제거.
- **의미론적 마커** — Android, iOS, GCC, MSVC, 링커를 위한 환경 인식 치환.
- **전체 빌드 생태계 감지** — C/C++, Rust, Go, Java, Android, iOS/Xcode, MSVC, Swift 및 주요 링커, 문맥 인식 접기와 오류 중복 제거 포함.

## 주요 기능

- **3가지 런타임**
  - **CLI** — 스크립트 가능한 배치 처리
  - **Server** — 전체 생태계 통합을 위한 장기 실행 REST API
  - **SDK** — Java, Python(PyO3), Node.js
- **플러그인 생태계** (실제 LLM 트래픽을 지배하는 입력을 다루는 60+ 플러그인)
  - **모바일** — `android_gradle`, `xcode_log`
  - **일반 개발** — `gcc_log`, `java_stack`, `python_traceback`, `dotnet`, `rust_go`, `maven`, `gradle`, `node_error`, `nodejs`, `php_ruby`, `unity_unreal`
  - **구조화된 데이터** — `json`, `yaml`, `xml_html`, `ndjson`, `protobuf`
  - **빌드 산출물** — `artifact_summary` (SARIF / JUnit XML), 테스트 상태, SARIF level/rule/location/tool의 의미론적 보존
  - **클라우드 및 운영** — `cloud_log` (AWS / GCP / Azure / Alibaba / OCI / Tencent / Huawei / Cloudflare), `web_log` (Nginx / Apache / ingress / Envoy / CloudFront / IIS / ALB / Cloudflare), `db_log` (PostgreSQL / MySQL / MongoDB / Redis), `syslog`
  - **CI/CD** — `ci_log` (GitHub Actions / GitLab CI / Jenkins / Azure Pipelines / CircleCI / Buildkite / 로컬 `act` / TeamCity / Travis CI)
  - **VCS** — git / svn / hg / p4 / cvs / bzr / fossil / darcs용 통합 `vcs_plugin`, 추가로 `git_diff`, `smart_code` (AST 레벨), `smart_path`
- **환경 진단** — `workspace`, `encoding`, `rule`, `env` 서브명령이 mojibake 위험을 감지하고 수정 레시피를 출력.
- **AI 네이티브 출력 모드**
  - `--ai-export` — 문맥 인식 노이즈 제거, 오류/경고 윈도우 유지
  - `--ai-signal` — 손실이 있지만 고신호, 의사 결정에 가장 관련 있는 필드 보존
- **플러그인 인트로스펙션** — `tokenslim explain-plugin`과 `tokenslim run --explain-route`가 라우트 선택, 폴백, 신뢰도, 대안을 설명하고 감사용 오분류를 재생.

## 설치

### 소스에서 빌드 (Rust toolchain ≥ 1.75)

```bash
git clone https://github.com/nuoyazhizhou/tokenslim.git
cd tokenslim
cargo build --release
```

바이너리는 `./target/release/tokenslim` (Windows에서는 `tokenslim.exe`)에 위치합니다.

### 미리 빌드된 바이너리

[Releases](https://github.com/nuoyazhizhou/tokenslim/releases) 페이지에서 다운로드.

### 설정 (선택)

모든 런타임 설정은 환경 변수를 통해 이루어집니다. [`.env.example`](./.env.example)을 `.env`로 복사하고 로컬 값을 채우세요. `.env`는 기본적으로 git에서 무시됩니다. 추적되는 것은 예제 템플릿뿐입니다.

대부분의 사용자는 `RUST_LOG=info` (상세 추적은 `debug`)만 필요합니다. LLM 감사 관련 변수(`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`)는 `scripts/audit_*.py --llm-audit`을 실행할 때만 필요합니다. 이것이 없으면 감사는 lint 전용 모드로 저하됩니다.

### 에디터 / IDE 통합

- **VS Code** — `vscode-extension/` 참조
- **Chrome** — `chrome-extension/` 참조
- **JetBrains** — `jetbrains-plugin/` 참조


#### Web UI

사이드카에는 대화형 압축 및 실시간 로그 테일링을 위한 단일 페이지 UI가 내장되어 있습니다. 모든 프론트엔드 정적 리소스는 **바이너리 실행 파일에 직접 컴파일되어 있습니다**. npm이나 pip를 통해 설치하더라도 어떤 디렉토리에서든 추가 설정 없이 바로 실행할 수 있습니다.

![TokenSlim Web UI — 홈 (zh-CN)](docs/webui-screenshots/01-home-zh.png)

##### 실행

```bash
# 모든 디렉토리에서 실행 (내장된 Web UI 자동 제공)
tokenslim-server

# 프론트엔드 개발 모드 (핫 리로드를 위해 물리적 디렉토리에서 제공)
TOKENSLIM_WEBUI_DIR=./webui tokenslim-server

# 포트 및 바인딩 주소 선택
TOKENSLIM_PORT=10086 TOKENSLIM_HOST=127.0.0.1 tokenslim-server

# 로컬 테스트 시 인증 비활성화 (기본값: 환경 변수가 설정되지 않은 경우 꺼짐)
# TOKENSLIM_API_KEY=changeme tokenslim-server
```

### SDK

- **Node.js / TypeScript** — `npm i tokenslim` (소스: [`packages/sdk-nodejs/`](./packages/sdk-nodejs/))
- **Python** — [`sdk/python/tokenslim_sdk.py`](./sdk/python/tokenslim_sdk.py) (단일 파일 클라이언트)
- **Java 11+** — [`sdk/java/TokenSlimClient.java`](./sdk/java/TokenSlimClient.java)

> 📖 [5분 Quickstart](./docs/guides/QUICKSTART.md) · [SDK 사용 가이드](./docs/guides/SDK_USAGE.md) · [사용자 가이드](./docs/guides/USER_GUIDE.md)

## 사용법

### CLI

```bash
# 빌드 로그 압축
tokenslim -i build.log -o output.json --reorder

# AI 친화적 노이즈 제거 진단 리포트
tokenslim decompress -i output.json -o ai_report.txt --ai-export

# 고신호 손실 모드 (오류 윈도우 + 핵심 메타데이터 보존)
tokenslim decompress -i output.json -o ai_signal.txt --ai-signal

# 정적 규칙 검증 (단일 파일)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule/sample_fixture.log \
  --verify-expected tests/fixtures/static_rule/sample_expected.txt

# 정적 규칙 검증 (배치, 디렉토리 모드)
tokenslim --verify-rule tests/fixtures/static_rule/sample_rule.toml \
  --verify-fixture tests/fixtures/static_rule \
  --verify-expected tests/fixtures/static_rule

# 프로젝트 부트스트랩 및 셸 훅
tokenslim init
tokenslim workspace
tokenslim --dry-run workspace --inject
tokenslim workspace --inject
tokenslim hooks install
tokenslim hooks status
tokenslim hooks uninstall
```

### Server (사이드카)

```bash
tokenslim-server
# 127.0.0.1:<port>에서 리슨, /health, /compress, /decompress 제공
```


#### Web UI

사이드카에는 대화형 압축 및 실시간 로그 테일링을 위한 단일 페이지 UI가 내장되어 있습니다. 모든 프론트엔드 정적 리소스는 **바이너리 실행 파일에 직접 컴파일되어 있습니다**. npm이나 pip를 통해 설치하더라도 어떤 디렉토리에서든 추가 설정 없이 바로 실행할 수 있습니다.

![TokenSlim Web UI — 홈 (zh-CN)](docs/webui-screenshots/01-home-zh.png)

##### 실행

```bash
# 모든 디렉토리에서 실행 (내장된 Web UI 자동 제공)
tokenslim-server

# 프론트엔드 개발 모드 (핫 리로드를 위해 물리적 디렉토리에서 제공)
TOKENSLIM_WEBUI_DIR=./webui tokenslim-server

# 포트 및 바인딩 주소 선택
TOKENSLIM_PORT=10086 TOKENSLIM_HOST=127.0.0.1 tokenslim-server

# 로컬 테스트 시 인증 비활성화 (기본값: 환경 변수가 설정되지 않은 경우 꺼짐)
# TOKENSLIM_API_KEY=changeme tokenslim-server
```

#### Docker

```bash
# 공식 이미지 (멀티 아키텍처: linux/amd64 + linux/arm64)
docker run -d -p 10086:10086 ghcr.io/nuoyazhizhou/tokenslim:latest

# API Key 인증 포함
docker run -d -p 10086:10086 -e TOKENSLIM_API_KEY=my-secret ghcr.io/nuoyazhizhou/tokenslim:latest

# JWT 인증 모드
docker run -d -p 10086:10086 \
  -e TOKENSLIM_AUTH_MODE=jwt \
  -e TOKENSLIM_JWT_SECRET=my-secret \
  -e TOKENSLIM_API_KEY=my-key \
  ghcr.io/nuoyazhizhou/tokenslim:latest
```

#### JWT 인증

TokenSlim 서버는 세 가지 인증 모드를 지원합니다:

| 모드 | 설명 |
|---|---|
| `static` (기본값) | 기존 API Key를 `Authorization: Bearer <key>`로 전달 |
| `jwt` | API Key로 JWT 토큰을 교환 (`POST /auth/token`), 이후 요청에서 JWT 사용 |
| `none` | 인증 없음 (개발 환경 전용) |

```bash
# API Key로 JWT 토큰 획득
curl -X POST http://127.0.0.1:10086/auth/token \
  -H "Authorization: Bearer YOUR_API_KEY"
# {"token":"eyJ...","expires_in":3600,"token_type":"Bearer"}

# 만료 전 갱신
curl -X POST http://127.0.0.1:10086/auth/refresh \
  -H "Authorization: Bearer YOUR_CURRENT_JWT"
```

#### WebSocket 양방향 압축 채널

`/ws/compress` 엔드포인트는 영구적인 양방향 채널을 제공합니다:

- **Binary 프레임** → 원본 데이터 → 압축 후 Binary 프레임으로 반환
- **Text 프레임** → JSON 제어 명령:
  - `{"action":"flush"}` — 즉시 압축하고 버퍼 비우기
  - `{"action":"reset"}` — 버퍼 비우고 세션 초기화
  - `{"plugin":"<name>"}` — 압축 플러그인 전환

#### 플러그인 설정 관리

```bash
tokenslim config plugin status                       # 모든 플러그인 상태 확인
tokenslim config plugin disable gcc_log_plugin       # 플러그인 비활성화
tokenslim config plugin enable gcc_log_plugin        # 플러그인 활성화
tokenslim config plugin list-params gcc_log_plugin   # 설정 가능한 매개변수 확인
tokenslim config plugin set gcc_log_plugin convert_timestamps false
tokenslim config plugin reset                        # 모든 플러그인 설정 초기화
```

### SDK

```python
# Python
from tokenslim import compress, decompress
compressed = compress(open("build.log").read())
print(decompress(compressed, mode="ai-export"))
```

```javascript
// Node.js
const { compress, decompress } = require("tokenslim");
const compressed = compress(fs.readFileSync("build.log", "utf8"));
console.log(decompress(compressed, { mode: "ai-export" }));
```

```java
// Java
TokenSlimClient client = new TokenSlimClient("http://127.0.0.1:8080");
String compressed = client.compress(logText);
String report = client.decompress(compressed, "ai-export");
```

## 로그 재정렬 (Log Reordering)

![Log Reordering: BEFORE vs AFTER](docs/webui-screenshots/reorder-before-after.png)

`make -jN` / `ninja` / Bazel / MSBuild 같은 병렬 빌드 도구는 여러 타깃의 로그를 **비결정적으로** 인터리브하여 출력하므로, Diff / 캐시 / 회귀 비교가 모두 깨집니다. TokenSlim 은 **결정적 전역 재정렬기**를 내장하여 활성 빌드 타깃을 스트리밍으로 추적하고, 타깃별 안정된 순서로 줄을 다시 정렬합니다.

```bash
# 내장: --reorder 플래그는 재정렬기를 강제하고 직렬 모드로 폴백
tokenslim -i build.log -o output.json --reorder

# 독립 도구: 순수한 log→log diff (Jenkins / CI 환경용)
cargo build --release --bin log_reorder
./target/release/log_reorder -i messy_build.log -o sorted_build.log --deterministic -n -p
#   --deterministic  : 모듈 / 빌드 타깃별로 줄을 그룹화
#   -n  (--normalize) : 플래그 순서를 정렬하고 메모리 주소와 랜덤 해시를 마스킹
#   -p  (--shorten-paths) : /home/userA/workspace/... 를 마지막 3 세그먼트로 축소
```

동일한 엔진은 `POST /compress` (요청 필드 `reorder: true`), WebUI 의 "재정렬 활성화" 체크박스, Python / Node SDK 에서도 노출됩니다.

## 플러그인

TokenSlim은 실제 LLM 트래픽을 지배하는 입력을 다루는 **60+ 플러그인** 과 함께 제공됩니다. 각 플러그인은 데이터 기반(`config/plugins/` 아래의 JSON / TOML 설정)이고 디스패치는 라우트 기반이므로, 새 소스 형식 추가는 대부분의 경우 설정 변경만으로 가능합니다.

전체 레지스트리는 [`config/plugins/`](./config/plugins/)에서 확인하거나 다음을 실행하세요:

```bash
tokenslim plugins list
tokenslim explain-plugin --explain-command "cargo build"
```

## 통합

| 표면        | 경로                                            | 상태   |
| ----------- | ----------------------------------------------- | ------ |
| CLI         | `src/bin/tokenslim-server.rs`, `src/cli/`       | Stable |
| REST Server | `src/bin/tokenslim-server.rs`                   | Stable |
| VS Code     | `vscode-extension/`                             | Stable |
| Chrome      | `chrome-extension/`                             | Stable |
| JetBrains   | `jetbrains-plugin/`                             | Stable |
| Python SDK  | `crates/tokenslim-py/`                          | Stable |
| Node.js SDK | `packages/sdk-nodejs/` (npm: `tokenslim@0.1.0`) | Stable |
| Java SDK    | `sdk/java/`                                     | Stable |

## 아키텍처

TokenSlim은 계층화된 파이프라인을 따릅니다:

1. **라우트 디스패처** — 명령 / 콘텐츠 시그니처로 플러그인을 선택.
2. **플러그인 체인** — 각 플러그인이 추출, 접기, 의미론적 치환을 소유.
3. **압축 코어** — 래딕스 트라이 경로 추출, 사전 레이어링, 글로벌 중복 제거.
4. **재수화** — 라운드트립 안전, 압축된 형식에서 원본 입력을 완전히 복구 가능.
5. **AI Export / Signal** — LLM 소비를 위한 문맥 인식 후처리.

전체 설계는 `docs/development/ARCHITECTURE.md`를 참조하세요.

## 🛡️ AI Agent 거버넌스 및 안티 드리프트 샌드박스 (AI Agent Governance & Anti-Drift Sandbox)

자율적인 AI 코드 생성에서 가장 해결하기 어려운 문제는 **"코딩 에이전트(Coding Agent)가 코드를 작성하면서 자기만족적인 모의 테스트(Mock Test)를 자작극으로 작성하는 것(가비지 인, 가비지 아웃)을 방지하는 것"**과 **"이후의 리팩토링 과정에서 내재된 타겟 드리프트(Target Drift, 동작 퇴행)가 소리 없이 유입되는 것을 방지하는 것"**입니다.

**10만 줄 이상의 핵심 소스 코드(105k+ LOC), 60개 이상의 플러그인, 1000개 이상의 물리적 테스트 케이스**로 구성된 복잡한 생태계에서, TokenSlim이 극도의 견고함을 유지하는 것은 수동 디버깅 덕분이 아닙니다. AI 코드 생성 동작을 제어하는 자동화된 폐루프형 **품질 샌드박스(Quality Sandbox) 체계**를 통해서입니다.

1. **의도 추출 및 코드 문서 주입 ([`extract_plugin_design.py`](scripts/extract_plugin_design.py))**: 파서 소스 코드를 스캔하고, LLM을 활용하여 핵심 설계 계약(`design_intent`/`keep_signals`)을 추출한 후, **이를 `mod.rs` 소스 코드 헤더에 모듈 수준의 `//!` 문서 주석으로 자동 역주입**합니다! 이를 통해 향후의 AI 및 인간 개발자가 가장 권위 있는 설계 경계(Source of Truth)를 준수하도록 강제합니다.
2. **다국어 자동 번역 동기화 ([`translate_messages_fields.py`](scripts/translate_messages_fields.py))**: 중국어와 영어 번역 파일을 교차 검증하고, 누락된 항목을 LLM 번역을 통해 자동으로 정렬하여 다국어 지원에서 필드가 누락되는 것을 방지합니다.
3. **물리적 케이스 정적 품질 감사 ([`audit_sample_case_quality.py`](scripts/audit_sample_case_quality.py))**: 초심 판사(Micro-Judge). 추출된 계약을 기반으로 LLM이 객관적인 제3자로서 물리적 원시 로그 케이스의 현실성과 커버리지를 감사하여 AI가 임의로 조작한 케이스를 걸러냅니다. 또한 샘플 폴더를 스캔하여 **등록되지 않은 새로운 물리적 로그가 발견되면 `showcase.rs`에 추가할 코드 줄을 자동으로 출력**합니다.
4. **압축 충실도 중관 감사 ([`audit_case_metrics.py`](scripts/audit_case_metrics.py))**: 종심 판사(Meso-Judge). **`showcase.rs` 선언**, **`samples/` 물리적 파일**, 그리고 **`target/` 생성 보고서**의 삼방향 정렬을 강력하게 검증합니다(유령 케이스 배제). G1-G4 결정론적 게이트를 집행하여(치명적인 에러 신호와 명령 앵커가 절대 유실되지 않도록 보장), 1단계에서 주입된 설계 계약과 비교해 압축 충실도를 교차 체크합니다.
5. **상태 동결 및 퇴행 방지 (State Freeze)**: 감사를 통과하면 압축 결과가 SHA256 해시값으로 잠깁니다. 향후 AI 코딩 변경이 기존 동작을 깨뜨려 출력 해시가 일치하지 않으면, CI/CD 파이프라인은 **즉시 릴리스를 거부하고 차단**하여 무의식적인 동작 드리프트를 방지합니다.
6. **글로벌 헬스 거버넌스 ([`audit_all_case_metrics.py`](scripts/audit_all_case_metrics.py))**: 거시 판사(Macro-Judge). CI 관문에서 `--fail-on-any-failure`를 통해 60개 이상의 모든 플러그인에 대한 감사를 병렬/직렬로 오케스트레이션하고, 글로벌 건강 대시보드(`audit_health.md`)를 작성하여 최종 품질 확인을 완료합니다.

## 기여

기여를 환영합니다. 더 큰 변경 사항은 먼저 Issue를 열어 논의해 주세요. 작은 수정과 새 플러그인 설정은 바로 PR로 가도 됩니다.

```bash
# 테스트 실행
cargo test

# 샘플로 실행
tokenslim -i samples/web_log_plugin/case_001_access.log -o out.json --reorder
```

## 라이선스

[MIT](./LICENSE)
