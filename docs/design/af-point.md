# 显示对焦点

状态：索尼已实现（Rust 解析 + 前端叠加）；理光 GR IIIx 等待样张。
起草：2026-10-05

## 需求

按键或点击查看照片的对焦点，默认关闭。用户机身：索尼 A7C II（ILCE-7CM2）为主，另有理光 GR IIIx。

## 能读到什么（索尼）

对焦点不在标准 EXIF 里，而在厂商私有的 MakerNote 中。只用不加密、不随机型变化的几个标签（加密的 0x94xx 块按机型改布局，不碰）：

| 标签   | 名称（ExifTool）    | 内容                                                                       |
| ------ | ------------------- | -------------------------------------------------------------------------- |
| 0x201B | FocusMode           | 0 = 手动对焦                                                               |
| 0x2027 | FocusLocation       | 图像宽、高、对焦点 x、y（存储方向，未按 Orientation 旋转；拿不到时为中心） |
| 0x2037 | FocusFrameSize      | 以 FocusLocation 为中心的框的宽、高，第三个值为 0 表示没有框               |
| 0x202A | FocalPlaneAFPoint\* | 合焦的相位对焦点（最多 15 个），首字节为 1 时可信                          |

- 用户 1635 张 A7C II JPG 与 ExifTool 逐张比对一致：1480 张有对焦框或对焦点，132 张手动对焦，23 张没有位置（对焦框标记为无效、也没有合焦点）。
- JPG 与 ARW 里的值逐字节相同；ARW 的 MakerNote 在文件开头约 5KB 处、长约 38KB，所以只读文件前 64KB。
- 0x202A 的坐标在一个铺满整张图的 640×480 网格里，而不是它自己声明的 640×428：800 张带合焦点的照片中，FocusLocation 换算到网格后全部落在整数格点上（例如 y = 260 → 260 × 4672 / 480 = 2530）。
- 眼部 / 人脸追踪时 FocusFrameSize 就是追踪框（眼睛小框、认不到眼睛时框头或身体）；E 卡口机身不写单独的人脸框。
- MakerNote 的偏移从 TIFF 头算起。编辑软件重写 EXIF 后可能移位：先核对标签类型、数量，坐标超出声明的图像尺寸就不画，宁缺勿错。读不到时如果有 RAW 再读 RAW（导出过的 JPG 常见）。
- 16:9 / 1:1 画幅、APS-C 裁切下 FocusLocation 的宽高是裁切后的尺寸（据 Focus-Points 插件），样张里没有，未实测。

理光 GR III / IIIx：MakerNote 是 Pentax 格式（`RICOH\0II` 头，偏移从 MakerNote 起算），ExifTool 的 AFInfoK3III（0x040C）给出对焦区域列表和是否合焦，FaceInfoK3III（0x040B）给出人脸和眼睛框。等用户提供样张后实测再做。

## 实现

Rust（`media/af_point/`，按厂商分文件）：

- `read_af_point(jpgPath, rawPath, source)` → `AfInfo`：`found`（存储方向的宽高、Orientation、对焦框、合焦点，单位都是存储图像的像素）/ `manualFocus` / `unavailable`。
- 先读 JPG；JPG 读不到或是纯 RAW 的预览图时读 RAW。用 `media/metadata/` 的头部缓存（一次读 64KB）+ TIFF IFD 读取器（与拍摄时间共用）。
- 与对比线约定：独立的模型和命令，不改 `ExifData` / `read_exif`。

前端：

- 换算是 `src/core/afPoint.ts` 的纯函数：存储方向的像素 → 按 EXIF Orientation 转正后的占比（0–1）→ 叠加层里的百分比位置。对焦框宽高最小 14px 屏幕像素，线宽固定。
- 叠加层的层级是 `main.css` 的 `--z-af-overlay`（35）：在照片自己的标签、标记角标和对比参数条（z-30）之上，所以角落的对焦框不会被盖住；在对比交换按钮、EXIF 面板（z-40）和顶栏、提示、对话框（z-50）之下。标记不接收点击。
- `AfOverlay.vue` 是一个和 `<img>` 同尺寸、同 transform（平移、旋转）、同过渡的框，标记用百分比放在里面，所以缩放、平移、旋转（`R`）都自动跟随。对比模式两侧各画各的，Survey 每格各画各的。
- `afPointStore` 按需读取并按照片缓存：开关打开时读当前图、对比基准图、往前预读 3 张、往后 1 张，Survey 读可见的几张。默认关闭时不读任何文件，也不在扫描时读。
- `P` 切换，顶栏视图工具组有按钮，偏好存 `sift.showAfPoint`，切换时提示一次。
- 打开时 `1` 以对焦点为中心放大到 100%（没有对焦点时照旧在光标处放大）。放大状态下切换照片，缩放倍率照旧保持，画面自动移到新照片的对焦点；旋转过的照片切换后照旧回到适应窗口。
- 没有对焦点时在图上方显示「手动对焦，没有对焦点」或「此照片没有对焦点信息」。

## 以后

- 理光 GR IIIx（AFInfoK3III / FaceInfoK3III）。
- 其他品牌按需：佳能 AFInfo2（原点在中心、EOS 的 y 轴向上）、尼康 AFInfo2（按版本）、富士 FocusPixel、OM / 松下（0–1 占比）。
