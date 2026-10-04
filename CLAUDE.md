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

提交前两个 check 都必须通过；CI（`.github/workflows/ci.yml`）跑的是同一套，但按改动路径跳过用不到的 job（只改前端/文档不跑 Rust，只改 Rust 不跑前端），PR 上 Rust 只跑 Windows（macOS 靠本地 `check:rust`），main 上两个平台都跑。依赖有 RustSec 漏洞时 CI 的 `audit` job 会失败（配置 `src-tauri/deny.toml`）。
Rust 的 clippy/test 需要 `dist/` 存在（`generate_context!` 会嵌入前端），首次先 `npm run build`。

## 架构

Rust（`src-tauri/src/`）：

| 模块        | 职责                                                                                                                                                                                                              | 依赖 Tauri |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `commands/` | IPC 薄层：`spawn_blocking` → 调用下层 → 错误转字符串                                                                                                                                                              | 是         |
| `catalog/`  | 扫描与配对：`pairing.rs`（纯规则）、`scan.rs`（读目录）；`progress.rs`（筛图进度）                                                                                                                                | 否         |
| `media/`    | 图像处理：`jpeg/`（字节级）、`raw_preview`、`thumbnail`、`exif`、`capture_time`（只读文件头取拍摄时间）、`cache_dirs`                                                                                             | 否         |
| `organize/` | 整理用户照片：`archive`（精选 → `精选/`，未标记 RAW → `RAW/`）、`delete` / `delete_batch`（进回收站，分块批量）、`no_trash`（没有回收站时经确认永久删除）、`export`（复制精选）、`transfer`（整组移动，失败回滚） | 否         |
| `os/`       | 交给系统：`reveal`（在文件管理器中显示）、`clipboard`（复制图片）、`trash_support`（卷有没有回收站）；路径只经参数/环境变量传入，不拼进脚本                                                                       | 否         |
| `models/`   | 与前端共享的 serde 结构体                                                                                                                                                                                         | 否         |
| `paths.rs`  | 路径安全：不覆盖的目标路径（一组 JPG/RAW/XMP 共用 `_N` 后缀）、`is_inside_dir`                                                                                                                                    | 否         |

- 新逻辑写进 `catalog/`、`media/`、`organize/` 或 `os/`，命令函数体只做参数转发。不要新建 `core/`：会遮蔽标准库 `core` crate。
- 写缓存的函数接收 `cache_dir: &Path` 参数，测试传临时目录；缓存目录名和路径只在 `media/cache_dirs.rs` 定义（`cache_dirs::path` / `ensure`）。
- 测试样本在代码里合成（`src/test_support.rs`：噪点 JPEG、最小 TIFF RAW），不提交二进制文件。

前端：

- `src/core/` 放纯 TS 规则（如 `review.ts`：标记切换、下一张、计数、分批），不引入 Vue/Pinia/Tauri；store 只管响应式状态并调用它。
- `src/services/tauriCommands.ts` 是前端**唯一**调用 `invoke()` 的地方。
- 与 Rust 共享的类型由 ts-rs 从 `src-tauri/src/models/` 生成到 `src/types/generated/`（不要手改，也不受 Prettier/ESLint 管）；改了 Rust 模型后运行 `npm run gen:types` 并一起提交，CI 会检查。`src/types/index.ts` 只负责转出，以及 `PhotoStatus` 常量（有编译期检查保证与 Rust 枚举一致）。
- Tauri 命令 reject 的是**字符串**（Rust `Result<_, String>`），前端用 `getErrorMessage()`（`src/utils/error.ts`）取错误信息，不要读 `e.message`。例外：整理类命令（`archive_photos`、`export_picks`）reject `OrganizeError`（`models/organize.rs`：错误类别、卡住的文件、英文原文、失败前已处理的照片数），前端用 `src/core/organizeError.ts` 的 `toOrganizeError()` 收窄、`describeFailure()` 转成中文说明和建议，在归档对话框的失败页（`ArchiveFailure.vue`）显示。`delete_pairs` 不因单组失败而 reject，而是返回逐组结果 `DeleteResult { trashed, failed[] }`，每个失败带自己的 `OrganizeError`。错误类别在 `organize/error.rs` 按 `io::ErrorKind` 和 trash 错误判定，新的错误来源要走这里而不是拼字符串。
- 照片只有三种状态：精选、删除、未标记（`PhotoStatus`）。`F`/`X` 切换标记，标记后前进一张、取消标记停在原地，每次改动都进撤销栈；`←`/`→`/`Space` 只导航、不改标记（规则在 `src/core/review.ts`）。
- 归档布局（`organize/archive.rs`）：精选的 JPG + RAW + XMP 移入 `精选/`（纯 RAW 另存同名 JPG 预览）；未标记的 JPG 留在原位，RAW 和 XMP 移入 `RAW/`；只在有文件要移入时才建文件夹。扫描总会读取 `RAW/`（与原位 JPG 配对）和旧布局的 `JPG/`（与 `RAW/` 配对），其他子文件夹不读。
- 删除是延迟的：标记删除只改前端状态，真正进回收站发生在归档时。顺序由 `src/core/archiveFlow.ts::runArchive` 编排：先用 `delete_pairs` 一次删掉所有标记删除的组（Rust 端按 50 组一块调用 `trash::delete_all`，某块失败再逐组逐文件重试，遇错继续），已删掉的组立即从会话移除（`sessionStore.dropPairs`），有失败就停在归档之前；文件已经不在算删除成功，所以失败后可以直接重试。
- 整理期间（归档、导出、删除）`viewStore.isOrganizing` 为真：对话框关不掉，「返回首页」无效；它同步给 Rust（`commands/organizing.rs`），Rust 拒绝关窗口（`CloseRequested`）和退出（`ExitRequested`，含 macOS 的 Cmd+Q），并发 `close-blocked` 事件让对话框提示。新增的整理操作都要包在 `ArchiveDialog.vue` 的 `organizing()` 里。归档成功后 `sessionStore.markArchived(folder)` 冻结会话（标记、撤销、重新开始、保存进度都不生效），文件夹路径在归档开始时就记下，清除的一定是这个文件夹的进度。
- 筛图进度按文件夹存在应用数据目录 `progress/<路径哈希>.json`（`catalog/progress.rs`），只存已标记的照片和当前位置（停在第一张不算），key 是文件路径（纯 RAW 用 `rawPath`，因为 pair `id` 每次扫描都会重新生成）。合并规则在 `src/core/progress.ts`；store 监听标记与位置变化后经 `utils/latestWriter.ts` 串行写入，既没有标记又停在第一张时删除文件；归档成功后由 `markArchived` 清除并冻结会话。旧版本写入的已不存在的状态（如 `skipped`）在 `catalog/progress.rs` 读取时丢弃，其余标记照常恢复。
- 排序：默认按拍摄时间（`src/core/sorting.ts`，无拍摄时间的排末尾），可切换为按文件名；排序方式是 viewer 偏好（localStorage），不进进度。撤销栈按 pair id 记录，重排不影响撤销。设计见 `docs/design/capture-time-and-bursts.md`。
- `source === 'rawPreview'` 表示纯 RAW：`jpgPath` 指向缓存目录里的临时预览图，不是用户文件。
- asset 协议（`convertFileSrc`）静态放行范围为空。用户目录的授权只来自用户操作：文件夹对话框和原生拖拽会由 Tauri 放行；`scan_folder` 先用 `catalog/folder_grants.rs::is_granted` 确认授权，再递归放行子目录并记入应用数据目录的 `granted-folders.json`，启动时恢复（「最近文件夹」靠它跨重启可用）。缓存目录在启动时放行。**不要**根据前端传入的任意路径放行。
- 缓存目录在启动和窗口销毁时同步清理（`commands/cache.rs`，目录名见 `media/cache_dirs.rs`）。

## 用户照片安全（最高优先级）

- 删除用户文件**只能**用 `trash::delete` / `trash::delete_all`。`fs::remove_*` 只允许用于缓存目录里的临时文件和应用数据目录里 Sift 自己的进度文件。删缓存文件前先用 `paths::is_inside_dir` 确认确实在缓存目录内。例外只有以下四条：
  - （`organize/transfer.rs`）`move_file` 在 `rename` 失败（跨卷）时先 `fs::copy`，成功后才删除源文件——这是移动，不是删除。复制失败或源文件删不掉时，只要源文件还在，就把目标（本次占用的占位）清空成 0 字节，不留第二份。
  - （`organize/transfer.rs`）整组移动或复制结束后（成功或回滚后），删除**本组 `create_new` 创建、且确认仍是 0 字节普通文件**的占位（`remove_empty_placeholders`）：被放弃的后缀、没用上的目标、回滚时重新占用但没移回的原位置。已经放进用户文件的路径（哪怕用户照片本身是 0 字节）一律不碰；符号链接不碰。
  - 导出按组原子化（`organize/export.rs::copy_group`）：一组里某个文件复制失败时，本组已复制到目标文件夹的副本先用 `discard_copy` 清空（源文件还在才清空），再按上一条作为空占位删除，目标里不留半组。
  - **没有回收站的位置**（`organize/no_trash.rs::TrashOrRemove`）：只有当 `os/trash_support.rs` 判定文件所在的卷没有回收站（网络共享；Windows 上可移动的 FAT/exFAT），**并且**用户在本次归档中勾选了「永久删除」确认（`delete_pairs` 的 `permanent`，每次归档都要重新勾选）时，才对这些文件用 `fs::remove_file`。有回收站的卷永远走回收站，`permanent` 对它不起作用；检测失败按「有回收站」处理；没有确认时返回 `TrashUnsupported`，文件不动。新增删除入口必须经过 `TrashOrRemove`。
- 移动/复制**永不覆盖**已有文件：目标路径一律经过 `paths::unique_paths_sharing_stem()`（归档用 `_from` 版本）：一组 JPG/RAW/XMP 共用同一个 `_N` 后缀（即使分属不同文件夹；归档时留在原位的 JPG 也同步改名），重新扫描时仍能配对。
- 归档按组原子化（`organize/transfer.rs::move_group`）：一组里任何文件移动失败，已移走的文件按相反顺序移回原处（移回前先 `create_new` 占用原位置，原位置被别的文件占了就留在目标处并报告），然后停止归档。纯 RAW 的预览复制排在本组所有移动之后，所以永远不需要回滚复制。
- 修改 delete / archive / export / scan 时必须附带用临时目录跑的 Rust 测试。
- 命令接收的路径来自前端，视为不可信输入。
- 扫描不得静默丢弃照片：每个 JPG/RAW 要么进会话，要么进 `ScanResult.skipped`（界面会提示）。0 字节的照片文件不参与配对，记为 `SkipReason::Empty`。stem 不区分大小写（只折叠 ASCII，文件保留原名）；同 stem 有多个 JPG/RAW 时，按文件名自然序第一个 JPG 与第一个 RAW 配对，其余各自成为独立条目；每个 XMP 只挂在一个条目上，删除或归档一个条目不会带走别的条目的 sidecar（规则见 `catalog/pairing.rs`）。

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
