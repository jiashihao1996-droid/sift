#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# Sift Release Script
# 用法: npm run release -- [--dry-run] <version>
#   例: npm run release -- 1.2.0
#
# 在最新的 main 上新建 release/v<version>，同步版本号、把 CHANGELOG 的
# [Unreleased] 落到新版本，只提交这几个文件，推送后开发布 PR（automerge）。
# PR 的 CI 全绿后 automerge 合并，并派发 release.yml：等 main 上这个提交的
# CI 通过后打 tag、构建 draft release。开 PR 前先在仓库目录外的临时 worktree
# 里 npm ci 并跑 npm run check 和 npm run check:rust，失败就中止。
#
#   --dry-run  做到本地提交和检查为止，打印改动后还原，不推送、不开 PR
# ============================================================

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info()  { echo -e "${CYAN}[info]${NC} $*"; }
ok()    { echo -e "${GREEN}[ok]${NC} $*"; }
warn()  { echo -e "${YELLOW}[warn]${NC} $*"; }
error() { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }

# 发布提交只包含这些文件；release.yml 按提交标题找发布提交
RELEASE_FILES=(
  package.json
  package-lock.json
  src-tauri/tauri.conf.json
  src-tauri/Cargo.toml
  src-tauri/Cargo.lock
  CHANGELOG.md
)

# Windows MSI 的版本号上限（tauri-bundler msi/mod.rs）
MSI_MAX_MAJOR_MINOR=255
MSI_MAX_PATCH=65535

usage() {
  echo ""
  echo -e "  ${CYAN}Sift Release Script${NC}"
  echo ""
  echo "  用法: npm run release -- [--dry-run] <version>"
  echo "  例如: npm run release -- 1.2.0"
  echo ""
  exit 1
}

# ---- 参数 ----
DRY_RUN=false
VERSION=""
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    -*) usage ;;
    *) [[ -z "$VERSION" ]] || usage; VERSION="${arg#v}" ;;
  esac
done
[[ -n "$VERSION" ]] || usage

TAG="v${VERSION}"
BRANCH="release/${TAG}"

validate_version() {
  if [[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-[0-9A-Za-z.-]+$ ]]; then
    error "不支持预发布版本号: '$VERSION'
  Windows 的 MSI 只接受纯数字的预发布号（会变成第四段版本号，安装器比较新旧时忽略这一段），
  release.yml 也总是按正式版发布。要先给少数人试用，请用正式版本号发 draft release，不要公开。"
  fi
  if ! [[ "$VERSION" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    error "版本号格式不合法: '$VERSION'（需要 x.y.z，如 1.2.0）"
  fi
  if (( BASH_REMATCH[1] > MSI_MAX_MAJOR_MINOR || BASH_REMATCH[2] > MSI_MAX_MAJOR_MINOR ||
        BASH_REMATCH[3] > MSI_MAX_PATCH )); then
    error "版本号超出 Windows MSI 的范围: '$VERSION'（主、次版本号 ≤ ${MSI_MAX_MAJOR_MINOR}，补丁号 ≤ ${MSI_MAX_PATCH}）"
  fi
}

# origin 的 OWNER/REPO；这是 fork，裸 gh 会默认指向上游
origin_repo() {
  git remote get-url origin | sed -E 's#\.git$##; s#^.*[:/]([^/]+/[^/]+)$#\1#'
}

check_preconditions() {
  [[ -z "$(git status --porcelain)" ]] || {
    git status --short
    error "工作区不干净。发布提交只包含版本号和 CHANGELOG，请先提交或清理其他改动。"
  }

  info "拉取 origin/main ..."
  git fetch --quiet origin main
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] ||
    error "当前不在最新的 main 上。请先运行: git switch main && git pull --ff-only"

  if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null ||
     [[ -n "$(git ls-remote --tags origin "refs/tags/$TAG")" ]]; then
    error "Tag '$TAG' 已存在！请使用一个新版本号。"
  fi
  if git rev-parse -q --verify "refs/heads/$BRANCH" >/dev/null ||
     [[ -n "$(git ls-remote --heads origin "$BRANCH")" ]]; then
    error "分支 '$BRANCH' 已存在（本地或远端）。如果是上次失败留下的，请先删除。"
  fi

  awk '/^## \[Unreleased\]$/ { f = 1; next } /^## \[/ { f = 0 } f && /^- / { n++ } END { exit !n }' \
    CHANGELOG.md || error "CHANGELOG.md 的 [Unreleased] 下没有条目，没有可发布的内容。"

  if [[ "$DRY_RUN" == false ]]; then
    gh auth status >/dev/null 2>&1 || error "gh 未登录，请先运行: gh auth login"
  fi
}

# [Unreleased] 的内容落到新版本小节，上面留一个空的 [Unreleased]
update_changelog() {
  local tmp
  tmp="$(mktemp)"
  awk -v heading="## [${VERSION}] - $(date +%Y-%m-%d)" '
    $0 == "## [Unreleased]" { print; print ""; print heading; next }
    { print }
  ' CHANGELOG.md >"$tmp"
  mv "$tmp" CHANGELOG.md
}

# 新版本小节的正文，用作 PR 描述
release_notes() {
  awk -v heading="## [${VERSION}]" '
    index($0, heading) == 1 { f = 1; next }
    /^## \[/ { f = 0 }
    f && (NF || started) { started = 1; print }
  ' CHANGELOG.md
}

bump_files() {
  info "更新版本号为 ${VERSION} ..."
  node scripts/sync-version.mjs "$VERSION"

  # 只刷新 Cargo.lock 里 sift 自己的版本行，不编译、不动其他依赖
  info "同步 Cargo.lock ..."
  (cd src-tauri && cargo update --workspace --quiet) ||
    error "cargo update --workspace 失败，Cargo.lock 未更新"
  grep -A1 '^name = "sift"$' src-tauri/Cargo.lock | grep -qx "version = \"${VERSION}\"" ||
    error "Cargo.lock 里 sift 的版本没有更新为 ${VERSION}"
  node scripts/sync-version.mjs --check

  info "更新 CHANGELOG.md ..."
  update_changelog

  local unexpected
  unexpected="$(git status --porcelain | awk '{ print $2 }' | grep -vxF -f <(printf '%s\n' "${RELEASE_FILES[@]}") || true)"
  [[ -z "$unexpected" ]] || error "以下文件被意外改动，已中止：
${unexpected}"
}

# 结束时（成功、失败或 dry-run）删掉检查用的 worktree，回到起点，删掉本地发布分支；
# 推送过的分支留在远端
START_REF=""
BRANCH_CREATED=false
CHECK_DIR=""
restore() {
  [[ -z "$CHECK_DIR" ]] || git worktree remove --force "$CHECK_DIR" || true
  [[ "$BRANCH_CREATED" == true ]] || return 0
  git reset --quiet --hard
  git checkout --quiet "$START_REF"
  git branch --quiet -D "$BRANCH"
}

# 在发布提交上跑和 CI 相同的检查：Actions 不可用时发布也不会绕过检查。
# 放在仓库目录外的临时 worktree 里，从 npm ci 开始：本机 node_modules 过期，
# 或 Node 往上层目录找到别的 node_modules，都不会让缺依赖的代码蒙混过关
run_checks() {
  local root tmp="${TMPDIR:-/tmp}"
  root="$(pwd)"
  CHECK_DIR="$(mktemp -d "${tmp%/}/sift-release-check.XXXXXX")"
  git worktree add --quiet --detach "$CHECK_DIR" HEAD
  (
    cd "$CHECK_DIR"
    info "本地检查（${CHECK_DIR}）: npm ci ..."
    npm ci --silent --no-audit --no-fund || error "npm ci 失败，已中止发布。"
    # check:rust 只需要 dist/ 存在（generate_context! 会嵌入它），与 CI 一样用桩代替 npm run build
    mkdir -p dist && echo '<!doctype html><title>Sift</title>' >dist/index.html

    info "本地检查: npm run check ..."
    npm run --silent check || error "npm run check 失败，已中止发布。"
    # 共用仓库的 target/，依赖不必重新编译
    info "本地检查: npm run check:rust ..."
    CARGO_TARGET_DIR="${root}/src-tauri/target" npm run --silent check:rust ||
      error "npm run check:rust 失败，已中止发布。"

    # cargo test 会重新生成 src/types/generated，有变化说明 main 上的类型过期了
    [[ -z "$(git status --porcelain)" ]] || {
      git status --short
      error "检查改动了工作区（多半是生成的 TS 类型过期，先在 main 上修好），已中止发布。"
    }
  )
  ok "本地检查全部通过"
}

# shellcheck disable=SC2016 # 反引号是 Markdown，不是命令替换
pr_body() {
  printf '发布 %s。\n\n%s\n\n' "$TAG" "$(release_notes)"
  printf '## 本地检查\n\n发布脚本在仓库目录外的临时 worktree 里，对本提交（%s）运行 `npm ci`、`npm run check` 和 `npm run check:rust`，全部通过。\n\n' \
    "$(git rev-parse --short HEAD)"
  printf '## 合并之后\n\n'
  printf 'CI 全绿后 automerge 会合并本 PR，并派发 `release.yml`：等 main 上这个提交的 CI 通过后打 tag `%s`、构建 draft release。' "$TAG"
  printf '如果手动合并，请运行：\n\n```bash\ngh workflow run release.yml --repo %s -f version=%s\n```\n' "$(origin_repo)" "$VERSION"
}

open_pr() {
  local repo
  repo="$(origin_repo)"
  git push --quiet -u origin "$BRANCH"
  ok "已推送 ${BRANCH}"

  gh pr create --repo "$repo" --base main --head "$BRANCH" --label automerge \
    --title "chore(release): ${TAG}" --body "$(pr_body)" ||
    error "开 PR 失败。分支已推送，可以手动运行: gh pr create --repo ${repo} --head ${BRANCH} --label automerge"
}

main() {
  validate_version
  cd "$(git rev-parse --show-toplevel)"
  info "项目根目录: $(pwd)"
  check_preconditions

  START_REF="$(git symbolic-ref --quiet --short HEAD || git rev-parse HEAD)"
  git switch --quiet -c "$BRANCH"
  BRANCH_CREATED=true
  trap restore EXIT

  bump_files
  git add -- "${RELEASE_FILES[@]}"
  git commit --quiet -m "chore(release): ${TAG}"
  run_checks

  echo ""
  echo -e "${CYAN}════════════════════════════════════════${NC}"
  echo -e "  版本:  ${GREEN}${VERSION}${NC}    分支: ${GREEN}${BRANCH}${NC}"
  echo -e "${CYAN}════════════════════════════════════════${NC}"
  git show --stat --format='  %s' HEAD
  echo ""
  echo -e "  ${YELLOW}PR 描述:${NC}"
  pr_body | sed 's/^/    /'
  echo ""

  if [[ "$DRY_RUN" == true ]]; then
    ok "dry-run：以上改动不会推送，已还原到 ${START_REF}。"
    return
  fi

  read -rp "$(echo -e "${YELLOW}推送并开发布 PR？(y/n): ${NC}")" CONFIRM
  if [[ "$CONFIRM" != "y" && "$CONFIRM" != "Y" ]]; then
    info "已取消，改动已还原。"
    return
  fi

  open_pr
  echo ""
  ok "发布 PR 已创建。CI 全绿后自动合并，随后 release.yml 打 tag ${TAG} 并构建 draft release。"
}

main
