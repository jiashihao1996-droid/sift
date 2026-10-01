# Sift — 项目约定

RAW + JPG 筛图桌面应用。Tauri 2（Rust）+ Vue 3 + Pinia + Tailwind。
从 sankigan/Sift fork 而来，**已独立演进，不再同步上游**。

## 常用命令

```bash
npm run tauri dev       # 开发（前端 + Rust）
npm run check           # typecheck + lint + format:check + vitest
npm run check:rust      # cargo fmt --check + clippy -D warnings + cargo test
npm run format          # Prettier 写入；Rust 用 cd src-tauri && cargo fmt
npm run release -- 1.2.0
```

提交前两个 check 都必须通过；CI（`.github/workflows/ci.yml`）跑的是同一套。
Rust 的 clippy/test 需要 `dist/` 存在（`generate_context!` 会嵌入前端），首次先 `npm run build`。

## 架构

- `src/services/tauriCommands.ts` 是前端**唯一**调用 `invoke()` 的地方。
- `src/types/index.ts` 与 `src-tauri/src/models/photo.rs` 手工保持一致；改一边必须改另一边。
- Tauri 命令 reject 的是**字符串**（Rust `Result<_, String>`），前端用 `getErrorMessage()`（`src/utils/error.ts`）取错误信息，不要读 `e.message`。
- 删除是延迟的：标记删除只改前端状态，真正进回收站发生在归档对话框（`ArchiveDialog.vue` → `delete_pair`）。
- `source === 'rawPreview'` 表示纯 RAW：`jpgPath` 指向缓存目录里的临时预览图，不是用户文件。
- 缓存目录（`sift-thumbnails`、`sift-raw-previews`）在启动和窗口销毁时同步清理（`commands/cache.rs`）。

## 用户照片安全（最高优先级）

- 删除用户文件**只能**用 `trash::delete`。`fs::remove_*` 只允许用于缓存目录里的临时文件。
- 移动/复制**永不覆盖**已有文件：目标路径一律经过 `unique_path()`（`commands/archive.rs`）。
- 修改 delete / archive / export / scan 时必须附带用临时目录跑的 Rust 测试。
- 命令接收的路径来自前端，视为不可信输入。

## 代码规范

- 前端：Prettier（无分号、单引号、100 列）+ ESLint（`eslint.config.js`）。禁止 `any`，外部输入用 `unknown` 再收窄。
- Rust：`cargo fmt` + `clippy -D warnings`。需要保留手工排版时用 `#[rustfmt::skip]`（例：`utils/file_types.rs`）。
- 单文件 > 400 行就考虑按职责拆分，800 行是硬上限。当前最大：`utils/raw_preview.rs`（~550）。
- 纯逻辑与框架分离：可测试的逻辑不要写进 Vue 组件或 `#[tauri::command]` 函数体。
- 测试文件与源码同目录：`foo.ts` ↔ `foo.test.ts`；Rust 用文件内 `#[cfg(test)] mod tests`。

## 版本与发布

- 版本号唯一来源是 `package.json`；`scripts/sync-version.mjs` 同步到 lock、`tauri.conf.json`、`Cargo.toml`。CI 用 `--check` 校验。
- 每个面向用户的改动都更新 `CHANGELOG.md` 的 `[Unreleased]`。
- 发布：`npm run release -- x.y.z` → 推 tag → `release.yml` 构建 draft release。

## Git

- Conventional Commits，带 scope：`viewer` `scan` `raw` `archive` `export` `ipc` `release` `ci`。
- 格式化/重构与行为改动分开提交。
- main 只经 PR 合入，CI 绿了再合。

## 已知待办

- `assetProtocol.scope` 为 `/**`，webview 可读整盘；应改为运行时只放行所选目录和缓存目录。
- `export.rs` 用 `fs::copy` 直接写入 `target.join(filename)`，会覆盖导出目录中的同名文件。
- `ArchiveDialog.vue` 内层 `<Transition name="modal">` 从不切换，弹窗动画可能从未生效。
