# Sift — 项目约定

RAW + JPG 筛图桌面应用。Tauri 2（Rust）+ Vue 3 + Pinia + Tailwind。
从 sankigan/Sift fork 而来，**已独立演进，不再同步上游**。

## 常用命令

```bash
npm run tauri dev       # 开发（前端 + Rust）
npm run check           # typecheck + lint + format:check + vitest
npm run check:rust      # cargo fmt --check + clippy -D warnings + cargo test
SIFT_RAW_SAMPLES=~/raws npm run test:raw-samples   # 可选：用本地真实 RAW 验证预览提取
npm run format          # Prettier 写入；Rust 用 cd src-tauri && cargo fmt
npm run release -- 1.2.0
```

提交前两个 check 都必须通过；CI（`.github/workflows/ci.yml`）跑的是同一套。
Rust 的 clippy/test 需要 `dist/` 存在（`generate_context!` 会嵌入前端），首次先 `npm run build`。

## 架构

Rust（`src-tauri/src/`）：

| 模块        | 职责                                                                           | 依赖 Tauri |
| ----------- | ------------------------------------------------------------------------------ | ---------- |
| `commands/` | IPC 薄层：`spawn_blocking` → 调用下层 → 错误转字符串                           | 是         |
| `catalog/`  | 扫描与配对：`pairing.rs`（纯规则）、`scan.rs`（读目录）                        | 否         |
| `media/`    | 图像处理：`jpeg/`（字节级）、`raw_preview`、`thumbnail`、`exif`、`cache_dirs`  | 否         |
| `models/`   | 与前端共享的 serde 结构体                                                      | 否         |
| `paths.rs`  | 路径安全：不覆盖的目标路径（一组 JPG/RAW/XMP 共用 `_N` 后缀）、`is_inside_dir` | 否         |

- 新逻辑写进 `catalog/` 或 `media/`，命令函数体只做参数转发。不要新建 `core/`：会遮蔽标准库 `core` crate。
- 写缓存的函数接收 `cache_dir: &Path` 参数，测试传临时目录；缓存目录名和路径只在 `media/cache_dirs.rs` 定义（`cache_dirs::path` / `ensure`）。
- 测试样本在代码里合成（`src/test_support.rs`：噪点 JPEG、最小 TIFF RAW），不提交二进制文件。

前端：

- `src/core/` 放纯 TS 规则（如 `review.ts`：标记切换、下一张、计数、分批），不引入 Vue/Pinia/Tauri；store 只管响应式状态并调用它。
- `src/services/tauriCommands.ts` 是前端**唯一**调用 `invoke()` 的地方。
- `src/types/index.ts` 与 `src-tauri/src/models/photo.rs` 手工保持一致；改一边必须改另一边。
- Tauri 命令 reject 的是**字符串**（Rust `Result<_, String>`），前端用 `getErrorMessage()`（`src/utils/error.ts`）取错误信息，不要读 `e.message`。
- 删除是延迟的：标记删除只改前端状态，真正进回收站发生在归档对话框（`ArchiveDialog.vue` → `delete_pair`）。
- `source === 'rawPreview'` 表示纯 RAW：`jpgPath` 指向缓存目录里的临时预览图，不是用户文件。
- 缓存目录在启动和窗口销毁时同步清理（`commands/cache.rs`，目录名见 `media/cache_dirs.rs`）。

## 用户照片安全（最高优先级）

- 删除用户文件**只能**用 `trash::delete`。`fs::remove_*` 只允许用于缓存目录里的临时文件。唯一例外：`archive.rs::move_file` 在 `rename` 失败（跨卷）时先 `fs::copy`，成功后才删除源文件——这是移动，不是删除。删缓存文件前先用 `paths::is_inside_dir` 确认确实在缓存目录内。
- 移动/复制**永不覆盖**已有文件：目标路径一律经过 `paths::unique_paths_sharing_stem()`：一组 JPG/RAW/XMP 共用同一个 `_N` 后缀（即使分属 `JPG/` 和 `RAW/` 两个文件夹），重新扫描时仍能配对。
- 修改 delete / archive / export / scan 时必须附带用临时目录跑的 Rust 测试。
- 命令接收的路径来自前端，视为不可信输入。

## 代码规范

- 前端：Prettier（无分号、单引号、100 列）+ ESLint（`eslint.config.js`）。禁止 `any`，外部输入用 `unknown` 再收窄。
- Rust：`cargo fmt` + `clippy -D warnings`。需要保留手工排版时用 `#[rustfmt::skip]`（例：`catalog/file_types.rs`）。
- 单文件 > 400 行就考虑按职责拆分，800 行是硬上限。
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
- 给 PR 打 `automerge` 标签，CI 全绿后由 `.github/workflows/automerge.yml` 自动 rebase 合并并删除分支。改动 `.github/workflows/` 的 PR 不能自动合并（`GITHUB_TOKEN` 无权合并 workflow 变更），需手动合并。

## 已知待办

配对规则的现状（`catalog/pairing.rs` 测试里有记录，改动前先想清楚是否算 bug）：

- stem 区分大小写：`img_1.jpg` 与 `IMG_1.CR3` 不配对。
- 同 stem 同类型有两个文件（`IMG_1.jpg` + `IMG_1.jpeg`，或 `.CR3` + `.DNG`）时只保留后扫到的一个。
- 提取不出内嵌预览的 RAW-only 文件会被静默排除在会话之外。

筛图交互的现状（`src/stores/sessionStore.test.ts` 有记录）：

- 取消标记（再按一次同一个键）不进撤销栈；此时 Ctrl+Z 撤销的是更早一次已记录的标记。
- 回绕到前面的未处理照片时，切换动画方向仍是 `left`。

其他：

- `assetProtocol.scope` 为 `/**`，webview 可读整盘；应改为运行时只放行所选目录和缓存目录。
