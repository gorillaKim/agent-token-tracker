# 🚀 ATK 데스크톱 앱 릴리즈 및 자동 업데이트(Auto-Updater) 설정 가이드

Tauri v2의 공식 `updater` 플러그인은 업데이트 파일의 변조 방지를 위해 **서명 검증(minisign)**을 필수로 요구합니다. 
GitHub Actions를 통해 앱을 빌드하고 자동 업데이트가 원활하게 작동하도록 하려면 아래의 설정 단계를 완료해야 합니다.

---

## 🔑 1. 업데이트 서명 키 생성 (Minisign)

Tauri는 업데이트 파일 검증에 `minisign`을 사용합니다. 로컬 터미널에서 다음 명령을 실행하여 키 쌍을 생성합니다.

```bash
# minisign이 설치되어 있지 않은 경우 설치 (macOS)
brew install minisign

# 키 쌍 생성 (패스워드 입력 요구 시 빈 값으로 엔터를 치면 자동 빌드 파이프라인에서 편리하게 쓸 수 있습니다)
minisign -G
```

실행 결과 다음 두 파일이 생성됩니다.
- `minisign.pub`: 공개키 (앱 내부에 내장하여 검증에 사용)
- `minisign.key`: 개인키 (GitHub Action에서 빌드 후 파일 서명에 사용)

### 설정 반영:
1. **공개키 등록 (`tauri.conf.json`)**:
   `minisign.pub` 파일의 내용(예: `RW...`)을 복사하여 `src-tauri/tauri.conf.json` 파일의 `plugins.updater.pubkey` 필드에 입력합니다. (이미 설정된 임시 키를 본인의 공개키로 교체해 주세요)
   ```json
   "plugins": {
     "updater": {
       "pubkey": "본인의_minisign.pub_내용_문자열",
       "endpoints": [
         "https://github.com/gorillaKim/agent-token-tracker/releases/latest/download/latest.json"
       ]
     }
   }
   ```

2. **개인키 등록 (GitHub Secrets)**:
   GitHub 저장소 설정(Settings) -> **Secrets and variables** -> **Actions**로 이동하여 다음 시크릿을 생성합니다.
   - `TAURI_SIGNING_PRIVATE_KEY`: `minisign.key` 파일의 전체 내용 붙여넣기

---

## 🤖 2. GitHub Actions 자동 빌드 워크플로우 구성

GitHub에 소스코드를 푸시하거나 버전을 태깅(예: `v0.2.1`)했을 때 자동으로 빌드하고 릴리즈를 생성하기 위해 `.github/workflows/release.yml` 파일을 작성합니다.

저장소 루트에 `.github/workflows/release.yml` 파일을 만들고 아래 코드를 저장합니다.

```yaml
name: "Publish Release"

on:
  push:
    tags:
      - "v*"

jobs:
  publish-tauri:
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: "macos-latest" # Apple Silicon & Intel macOS 지원

    runs-on: ${{ matrix.platform }}
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-size: '20'
          cache: 'npm'
          cache-dependency-path: 'frontend/package-lock.json'

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Install dependencies
        run: |
          npm --prefix frontend install

      - name: Build and Publish with Tauri Action
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Agent Token Tracker ${{ github.ref_name }}"
          releaseBody: "ATK 데스크톱 앱 최신 릴리즈 버전입니다."
          releaseDraft: false
          prerelease: false
          projectPath: "."
```

---

## 🎯 3. 실제 자동 업데이트가 작동하는 흐름

1. **버전 수정 및 태깅**:
   - `src-tauri/tauri.conf.json`의 `"version"` 필드를 `0.2.1`로 올립니다.
   - Git 커밋 후 태그를 생성해 푸시합니다:
     ```bash
     git add .
     git commit -m "bump: version v0.2.1"
     git tag v0.2.1
     git push origin main --tags
     ```

2. **자동 빌드 및 업로드**:
   - GitHub Actions가 트리거되어 macOS 빌드를 시작합니다.
   - 빌드가 완료되면 GitHub Release에 `Agent Token Tracker_0.2.1_aarch64.dmg`, `Agent Token Tracker.app.tar.gz`, `Agent Token Tracker.app.tar.gz.sig`, 그리고 가장 중요한 **`latest.json`** 파일이 자동으로 생성되어 업로드됩니다.

3. **앱내 감지**:
   - 앱이 실행되어 업데이트를 체크할 때, `releases/latest/download/latest.json` 주소를 요청하여 저장된 버전(`0.2.1`)과 현재 앱 버전(`0.2.0`)을 비교합니다.
   - 새 버전이 더 높은 것을 감지하면 `latest.json` 안에 적힌 `Agent Token Tracker.app.tar.gz` 다운로드 URL과 `.sig` 서명을 토대로 다운로드를 수행하고 서명을 검증한 뒤 안전하게 설치합니다.
