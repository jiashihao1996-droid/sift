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

| 模块        | 职责                                                                                                       | 依赖 Tauri |
| ----------- | ---------------------------------------------------------------------------------------------------------- | ---------- |
| `commands/` | IPC 薄层：`spawn_blocking` → 调用下层 → 错误转字符串                                                       | 是         |
| `catalog/`  | 扫描与配对：`pairing.rs`（纯规则）、`scan.rs`（读目录）；`progress.rs`（筛图进度）                         | 否         |
| `media/`    | 图像处理：`jpeg/`（字节级）、`raw_preview`、`thumbnail`、`exif`、`cache_dirs`                              | 否         |
| `organize/` | 整理用户照片：`archive`（精选 → `精选/`，未标记 RAW → `RAW/`）、`delete`（进回收站）、`export`（复制精选） | 否         |
| `models/`   | 与前端共享的 serde 结构体                                                                                  | 否         |
| `paths.rs`  | 路径安全：不覆盖的目标路径（一组 JPG/RAW/XMP 共用 `_N` 后缀）、`is_inside_dir`                             | 否         |

- 新逻辑写进 `catalog/`、`media/` 或 `organize/`，命令函数体只做参数转发。不要新建 `core/`：会遮蔽标准库 `core` crate。
- 写缓存的函数接收 `cache_dir: &Path` 参数，测试传临时目录；缓存目录名和路径只在 `media/cache_dirs.rs` 定义（`cache_dirs::path` / `ensure`）。
- 测试样本在代码里合成（`src/test_support.rs`：噪点 JPEG、最小 TIFF RAW），不提交二进制文件。

前端：

- `src/core/` 放纯 TS 规则（如 `review.ts`：标记切换、下一张、计数、分批），不引入 Vue/Pinia/Tauri；store 只管响应式状态并调用它。
- `src/services/tauriCommands.ts` 是前端**唯一**调用 `invoke()` 的地方。
- 与 Rust 共享的类型由 ts-rs 从 `src-tauri/src/models/` 生成到 `src/types/generated/`（不要手改，也不受 Prettier/ESLint 管）；改了 Rust 模型后运行 `npm run gen:types` 并一起提交，CI 会检查。`src/types/index.ts` 只负责转出，以及 `PhotoStatus` 常量（有编译期检查保证与 Rust 枚举一致）。
- Tauri 命令 reject 的是**字符串**（Rust `Result<_, String>`），前端用 `getErrorMessage()`（`src/utils/error.ts`）取错误信息，不要读 `e.message`。
- 照片只有三种状态：精选、删除、未标记（`PhotoStatus`）。`F`/`X` 切换标记，标记后前进一张、取消标记停在原地，每次改动都进撤销栈；`←`/`→`/`Space` 只导航、不改标记（规则在 `src/core/review.ts`）。
- 归档布局（`organize/archive.rs`）：精选的 JPG + RAW + XMP 移入 `精选/`（纯 RAW 另存同名 JPG 预览）；未标记的 JPG 留在原位，RAW 和 XMP 移入 `RAW/`；只在有文件要移入时才建文件夹。扫描总会读取 `RAW/`（与原位 JPG 配对）和旧布局的 `JPG/`（与 `RAW/` 配对），其他子文件夹不读。
- 删除是延迟的：标记删除只改前端状态，真正进回收站发生在归档对话框（`ArchiveDialog.vue` → `delete_pair`）。
- 筛图进度按文件夹存在应用数据目录 `progress/<路径哈希>.json`（`catalog/progress.rs`），只存已标记的照片和当前位置（停在第一张不算），key 是文件路径（纯 RAW 用 `rawPath`，因为 pair `id` 每次扫描都会重新生成）。合并规则在 `src/core/progress.ts`；store 监听标记与位置变化后经 `utils/latestWriter.ts` 串行写入，既没有标记又停在第一张时删除文件；归档成功后清除。旧版本写入的已不存在的状态（如 `skipped`）在 `catalog/progress.rs` 读取时丢弃，其余标记照常恢复。
- `source === 'rawPreview'` 表示纯 RAW：`jpgPath` 指向缓存目录里的临时预览图，不是用户文件。
- asset 协议（`convertFileSrc`）静态放行范围为空。用户目录的授权只来自用户操作：文件夹对话框和原生拖拽会由 Tauri 放行；`scan_folder` 先用 `catalog/folder_grants.rs::is_granted` 确认授权，再递归放行子目录并记入应用数据目录的 `granted-folders.json`，启动时恢复（「最近文件夹」靠它跨重启可用）。缓存目录在启动时放行。**不要**根据前端传入的任意路径放行。
- 缓存目录在启动和窗口销毁时同步清理（`commands/cache.rs`，目录名见 `media/cache_dirs.rs`）。

## 用户照片安全（最高优先级）

- 删除用户文件**只能**用 `trash::delete`。`fs::remove_*` 只允许用于缓存目录里的临时文件和应用数据目录里 Sift 自己的进度文件。唯一例外：`organize/archive.rs` 的 `move_file` 在 `rename` 失败（跨卷）时先 `fs::copy`，成功后才删除源文件——这是移动，不是删除。删缓存文件前先用 `paths::is_inside_dir` 确认确实在缓存目录内。
- 移动/复制**永不覆盖**已有文件：目标路径一律经过 `paths::unique_paths_sharing_stem()`（归档用 `_from` 版本）：一组 JPG/RAW/XMP 共用同一个 `_N` 后缀（即使分属不同文件夹；归档时留在原位的 JPG 也同步改名），重新扫描时仍能配对。
- 修改 delete / archive / export / scan 时必须附带用临时目录跑的 Rust 测试。
- 命令接收的路径来自前端，视为不可信输入。
- 扫描不得静默丢弃照片：每个 JPG/RAW 要么进会话，要么进 `ScanResult.skipped`（界面会提示）。stem 不区分大小写（只折叠 ASCII，文件保留原名）；同 stem 有多个 JPG/RAW 时，按文件名自然序第一个 JPG 与第一个 RAW 配对，其余各自成为独立条目；每个 XMP 只挂在一个条目上，删除或归档一个条目不会带走别的条目的 sidecar（规则见 `catalog/pairing.rs`）。

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
- 给 PR 打 `automerge` 标签，CI 全绿后由 `.github/workflows/automerge.yml` 自动 rebase 合并并删除分支（分支无法 rebase 时——通常是合并过 main 来解决冲突——改为 squash 合并）。改动 `.github/workflows/` 的 PR 不能自动合并（`GITHUB_TOKEN` 无权合并 workflow 变更），需手动合并。
