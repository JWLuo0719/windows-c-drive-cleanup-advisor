# windows-c-drive-cleanup-advisor「重构 vs 续研」评估报告

- **评估日期**：2026-09-25
- **评估对象**：`D:\Project\windows-c-drive-cleanup-advisor`（Windows C 盘只读清理顾问，当前版本 v0.2.0）
- **评估方法**：内部盘点（对源码/文档/CI/git 的只读核查）＋ 外部对标（SpaceSniffer 专项、磁盘分析器横向对比、清理工具安全模型、桌面工具技术栈趋势共四路调研）＋ 三方独立评审（重构派、续研派、混合派各出一份含 keyEvidence / verdict 可证伪条件的独立论证）＋ 终审复核（本评审对评委之间的争议点逐条回到源码核验）
- **评委完整性**：judges 数组为 3/3，**无缺失**。三派在「保留 IPC 契约、安全门禁、测试与发布证据链、不换技术栈」上完全收敛，分歧集中在**重建范围与节奏**，因此最终裁决的关键变量是"多快、按什么顺序做哪些定点重建"，而非"是否推倒"。
- **裁决结论**：**hybrid（混合路线）**——不推倒重来、也不原地惰性续研；以"契约冻结、每阶段可发布"的分阶段方式完成四处定点重建（状态机、分类规则表、两块巨石、扫描内核），并通过顺序纪律把 v0.3 写权限能力挡在这四者之后。

---

## 评估背景

本项目约 5,000 行源码，2026-08-10 发布 v0.2.0 便携版后停滞约一个月，现重新启动。决策问题是从"新项目视角"判断：推倒重构，还是在现有基础上继续开发。这个问题之所以值得认真回答，是因为三件事同时成立：

1. 项目已经产出了真实资产：已发布的 tag、SHA-256 校验、两份烟测报告、57 项测试、12 步发布门禁；
2. 项目存在已被实证的正确性缺陷与性能失信（烟测记录 Quick 扫描单目录卡 27% 达 4 分 48 秒，而 README 承诺 "usable in a few minutes"）；
3. v0.3 规划要引入本项目**第一个写权限能力**（allowlist 低风险清理），这是安全模型的分水岭。

因此本报告的判断框架不是"代码新不新"，而是：**哪些资产重写会丢、哪些缺陷不重建修不好、什么顺序能把两者同时拿到**。

---

## 一、项目现状速写（关键数字）

| 维度 | 数字（本次复核/盘点） |
|---|---|
| 源码规模 | 约 5,017 行（git 追踪 54 个文件）：TS/TSX 1,744、Rust 2,080、PowerShell 1,193 |
| Rust 后端 | `src-tauri/src/lib.rs` **2,077 行单文件**（生产约 1,332 + 内联测试 745），`main.rs` 3 行；6 个 Tauri 命令、1 个事件（scan-progress） |
| 前端 | `src/App.tsx` **861 行单组件**（10 useState/4 useRef/3 useEffect/5 useMemo）；`styles.css` 998 行、**0 个 CSS 变量、133 处硬编码色值**；`reportUtils.ts` 222 行 |
| 扫描内核 | `scripts/Scan-CDriveCleanupAdvisor.ps1` **415 行**单线程 PowerShell 顶层即执行脚本 |
| 脚本层 | 8 个 .ps1 共约 1,085 行；安全断言 Test-SafetyBoundary 25 处、扫描契约 Test-ScannerContract 16 处 |
| 测试 | **57 项**：Rust 29（我复核 `#[test]` 计数=29）＋ 前端 24（App 集成 13 + reportUtils 单元 11，我复核 `it(` 计数=13/11）＋ 4 个 PS 检查脚本；Rust **集成测试 0**、扫描器**逻辑级单测 0** |
| 门禁 | `Invoke-ProjectChecks.ps1` 12 步 fail-fast；CI 双 workflow（ci.yml 43 行 + verify.yml 24 行）均 push main + PR 双触发 |
| 安全配置 | capabilities 仅 `core:default`（我复核原文），无 shell 权限；CSP 收紧到 ipc 连接 |
| Git | **11 个提交**、2 个 tag（v0.1.0/v0.2.0），首提交 ec211ac 即 8,250 行（无法 bisect），末次提交 2026-08-10 |
| 性能实证 | Quick 全扫 **8 分 10 秒**（SMOKE_TEST_REPORT_2026-06-30，22:19:49→22:27:59，我复核原文）；`C:\common_attachment` 卡 27% 达 **4 分 48 秒**（SMOKE_TEST_REPORT_2026-08-12:31，我复核原文） |
| 报告留存 | `report/` 已堆积 7 个扫描目录（含 `drive-d-*`、`drive-e-*`）与临时产物，无轮转；单次报告 JSON 约 250KB + MD 约 34KB |
| 可视化 | package.json **无任何图表依赖**（我复核 grep 无 echarts/d3/recharts） |
| 对标研究 | `docs/research/github-research.md` 对 SpaceSniffer/WizTree/TreeSize **提及 0 次**（我复核 grep 计数=0） |

一句话画像：**一个安全模型与发布工程做得明显高于个人项目平均、但把三条主承载线（Rust 单文件、前端单组件、脚本扫描器）全部押在单点上的 v0.2 工具。**

---

## 二、内部盘点发现

### 2.1 Rust 后端与 IPC 边界（src-tauri）

**总体**：围绕只读 PowerShell 扫描器的一层薄 IPC 壳——6 个命令（start_scan/get_scan_status/cancel_scan/get_scan_report/load_latest_report/reveal_report）、后台线程编排子进程、逐行解析 `[WCDCA_PROGRESS]` 进度标记、把扫描器原始 JSON 富化为带中文分类建议的报告并持久化。目录遍历与大文件枚举等重活全在 415 行 PS 脚本里，Rust 负责任务状态机、分类启发式与缓存聚合。**"结构欠账大于算法难题"**。

**优点（有据）**
- 命令面极小且无破坏性：没有任何删除/移动用户文件的命令；capabilities 最小化、CSP 收紧。
- 扫描编排合理：`start_scan` 立即返回 scan_id，长扫描不阻塞 IPC；stdout 逐行解析转进度事件（lib.rs:315-455、1255-1302）。
- 单飞保护与终态防覆盖存在：`insert_queued_scan_task` 拒绝并发扫描（lib.rs:1166-1193）、`cancel_scan_task` 不覆盖已终态任务（lib.rs:1195-1220），均有测试。
- 报告富化测试覆盖好：29 个单测锁定系统路径必 blocked、推荐恒 cleanable=false、去重、聚合、80 条上限、BOM 解析等不变量。
- 输入处理谨慎：drive 单字母白名单、scan_id 内部 UUID 不可注入路径、子进程参数逐个传递不经 shell。

**技术债清单（★＝本次终审逐行复核确认；☆＝来自内部盘点、未逐一复核）**
1. ★ `lib.rs:1132-1160 update_task_status` **无终态守卫**：`status.phase = phase.to_string()` 在 1145 行无条件执行，只有写入终态时才记 completed_at；`is_terminal_phase()` 函数（1162-1164）存在却没被它使用。stdout 读取线程"检查未取消→写入 running"之间插入 cancel 即把 cancelled 覆盖回 running，随后 worker 在 lib.rs:412 读到 running，**取消被静默撤销**。这是"取消可信度"级别的缺陷。
2. ★ PID 在子进程 spawn 之后才登记（lib.rs:361-373）；queued/未登记窗口内取消拿不到 PID，旧 PowerShell 继续跑完，而任务已进终态又允许发起新扫描，可能双扫描并发。
3. ★ `child.wait()`（lib.rs:404）**无超时**：脚本卡死时状态永远停在 running，只能靠人工取消兜底。
4. ★（更正盘点）所谓"stderr 管道 64KB 死锁"**不成立**：lib.rs:396-402 在 wait 之前就 spawn 独立线程并发 `read_to_string` 消费 stderr。**内部盘点与续研派均收录了这条错误**（详见 §5.4 点名）。
5. ★ 分类安全模型不自洽：`is_system_managed`（lib.rs:1021-1034）硬编码 `c:\pagefile.sys / c:\windows / c:\$recycle.bin / c:\recovery` 等精确路径，而 `normalize_drive`（457+）接受任意盘符、`start_scan` 不强制 C。**修正一处评委表述**：其中 `contains("\\windows\\winsxs")` 等 6 条子串规则对任意盘符同样生效，故"非 C 盘分类完全失效"言过其实——真正失效的是**精确路径规则**（`D:\pagefile.sys`、`D:\Windows` 根、`D:\$recycle.bin`、`D:\Recovery` 不会标 blocked）。前端写死 `drive:"C"`（App.tsx:363，我复核）使 UI 主路径不触发该缺口，但 `report/` 下实际存在 `drive-d-*`、`drive-e-*` 目录，证明**缺口被真实触发过**，不是纯理论。
6. ★ 宽泛子串匹配 + 顺序耦合：`is_low_risk_cache` 的 `contains("\\update")`（1048）、`is_app_managed` 的 `contains("\\qq")`（1055）等会误命中任意路径片段，分类结果依赖固定 if 顺序，规则表与中文文案硬耦合。
7. ☆ 任务表与 reports 目录无限增长：AppState HashMap 永不裁剪且每项持有完整 ScanReport；`latest_enriched_report_path`（531-560）每次全量遍历所有报告目录。
8. ☆ 扫描脚本定位靠 6 个候选路径猜测（`resource_scanner_candidates`，471-517）；`resolve_powershell`（624-640，我复核函数体）每次扫描前额外起一个 pwsh 探测子进程，结果不缓存。
9. ☆ AppError 绝大多数走 `Message(String)`，前端拿不到结构化错误码；`lock().ok()?` 静默吞锁中毒（1142）。
10. ☆ 测试结构：1333-2077 全为内联单测；`run_scan_worker` 端到端、取消全链路、并发单飞、进程编排**零覆盖**，无 tests/ 集成测试目录。

### 2.2 前端（src/）

**总体**：以单组件为核心的 Tauri + React 18 + TS strict 只读扫描台，主流程清晰，长扫描陪伴体验是亮点；报告可视化形态原始（纯卡片列表）。

**优点（有据）**
- 类型安全扎实：strict、类型集中 types.ts、IPC 封装仅 26 行，组件内几乎无 any。
- 测试质量高于项目平均：24 个用例特意覆盖真实难点——stale 进度事件过滤、取消后迟到轮询响应、扫描停滞 30s 陪伴文案、卸载时清理轮询与 listen dispose（App.test.tsx:371-461）。**这是隐性资产，拆分时最易丢失。**
- 长扫描 UX 细致：分阶段描述、焦点/为什么慢/下一步三栏、轮换 tips、已用时与阶段计时、15s 判停滞、30s 活动记录、5 条活动流——明显优于同类单一进度条。
- 安全边界在 UI 表达完整：安全账本五项、结果自检健康面板 5 项、每条建议带 manualSteps/置信度/risk。
- 纯逻辑初步分层：reportUtils.ts 222 行 + 11 单测。

**技术债清单（★＝终审复核）**
1. ★ 报告可视化**整层缺失**：无 treemap、无占比图、无目录树 drill-down、无磁盘容量仪表（对比全线 treemap 的竞品差距是层级的，不是缺一个组件）。
2. ★ `App.tsx` 861 行单组件：UI（约 8 个 section）与扫描会话编排（runScan/pollStatus/loadCompletedReport/stopScan）耦合，任何新面板都只能继续加状态。
3. ★ 四连重置逐字重复：`clearPollTimer(); activeScanIdRef.current=null; ...` 在全文件出现 **8 处**（312/320/341/386/398/410/427/479，我复核计数），漏改一处即状态残留。
4. ★ 协议耦合：`App.tsx:135 message.includes("重点目录")` 把后端中文文案当协议信号；`scanStageDescription/scanStageInsight` 两套百分比阈值（<12/<18/<65/<74/<88）平行维护，与 PS 脚本魔法数三处隐式同步，无共享契约。
5. ★ `scanProfiles` quick/deep 的 `topCount:30`、`largeFileMb:200` **完全相同**（59-72，我复核），"快速/完整"实际差异只剩 scanMode 参数与文案。
6. ☆ 事件推送 + 1200ms 轮询双通道并存，completed/failed 处理在监听回调与 pollStatus 两处重复，理解成本高。
7. ☆ listen() 清理竞态：cleanup 执行时 promise 未 resolve 则 dispose 永不调用（StrictMode 双挂载有重复监听风险）。
8. ☆ 无运行时校验：types.ts 的 schemaVersion 前端从未读取，旧报告兼容无版本检查；`extractErrorPath` 用正则反解后端错误文案格式。
9. ☆ 工程配套：无 ESLint/Prettier/lint 脚本、vitest 无覆盖率配置、styles.css 998 行 0 变量。
10. ☆ 测试与文案强耦合：大量 `getByRole` + 精确中文断言，文案迭代会连锁改测试。

### 2.3 脚本、CI 与发布工程

**总体**：**整个项目最成熟、证据链最完整的部分**，应原样续研；短板集中在扫描性能架构、可测试性与 CI 覆盖缝隙三处。

**优点（有据）**
- 分层契约清晰：PS 只读遍历 → Rust 富化 → React 展示；`[WCDCA_PROGRESS]` 行协议 + Markdown + JSON 双输出解耦。
- verify 门禁完整 fail-fast：12 步覆盖版本契约、安全边界、扫描契约、前端测试+构建、npm audit、cargo test/check、Tauri release 构建、便携打包、SHA-256、解压后 subst 虚拟盘运行时冒烟。
- 安全边界是**可执行断言**而非口头承诺：Test-SafetyBoundary.ps1 静态禁删改类命令黑名单、禁 shell 插件与 shell 权限、强制 Rust 以固定参数启动扫描器、校验 CSP 与资源随包。
- 便携发布链自洽可复验：打包（zip 必需条目）、checksum、解压后重跑扫描冒烟三重校验；两份 GUI 烟测报告保存了扫描 ID、SHA-256、耗时与 WebView 控制台 0 error 等真实证据。
- 运行时细节到位：4 秒进度心跳、取消用 taskkill /T /F、重解析点全程跳过、scanErrors 限流 200、BOM 显式剥离兼容 PS5.1/pwsh。

**技术债清单（★＝终审复核）**
1. ★ Quick/Deep 名不副实：两者唯一区别是是否传 `-SkipCommonRoots`（`include_common_roots = options.scan_mode == "deep"`，lib.rs 154/618，我复核 start_scan 函数体），实际是 2-3 次全盘遍历的叠加。
2. ★ 扫描性能数量级短板：单线程逐条目 GetAttributes + FileInfo（约 3 次系统调用/条目）、`Get-LocalTreeSize` 与 `Get-LargeFiles` 两套同构遍历重复下钻；烟测两处实证（8 分 10 秒 / 4 分 48 秒卡 27%）。
3. ☆ 扫描脚本 415 行参数块后立即执行主流程，无法 dot-source 做单元测试；唯一覆盖是重型 subst E2E。
4. ☆ 全局 `$ErrorActionPreference="SilentlyContinue"`（脚本第 10 行）+ 尾部无条件 `'[OK] Report written'`：写文件失败也可能报成功。
5. ☆ scanErrors 硬上限 200：两份烟测的"200 条不可读"恰好等于上限，指标饱和失去区分度。
6. ☆ 进度魔法数（12/18/34/62/65/73/74/82/88/94）在 PS 脚本、lib.rs:1255、前端三处隐式耦合。
7. ★ 冒烟测试与真实路径不一致：`Test-ScannerContract.ps1:53`、`Test-PortablePackage.ps1:168` 硬编码 powershell.exe，而 `resolve_powershell()` 优先 pwsh（CI 的 windows-latest 预装 pwsh）——**被测路径 ≠ 运行路径**。
8. ★ CI 覆盖窄于本地门禁：verify.yml 实测以 `-SkipTauriBuild -SkipPackage -SkipChecksum` 运行（我复核原文），Tauri 构建/打包/校验和只在发布者本机执行；双 workflow 重复触发、无 Rust 缓存、ci.yml 无 permissions 块。
9. ☆ checksum 格式 `SHA256 <hash> <file>` 非 sha256sum 兼容，且最近两次提交（364862a、681c476）都在修它——"改一处漏一处"已有实证。
10. ☆ 打包清单双处维护（New-PortablePackage.ps1:47-59 ↔ Test-PortablePackage.ps1:67-79）+ 依赖 Tauri `_up_` 内部资源布局命名，AGENT.md 自己承认。

### 2.4 产品定位、文档与测试全景

**优点（有据）**
- 定位在全文档链条上高度一致：README/AGENT/USER-GUIDE/SKILL/RELEASE_NOTES 均重复"只读、cleanable 恒 false、系统项 blocked、无上传"，未发现互相矛盾的安全承诺。
- 证据留痕文化成熟：docs/release 7 份文件、docs/reviews 结构化复盘 JSON（记录 scanId 竞态根因与复用验证）。
- 已建立许可证纪律：WinDirStat(gpl2)/BleachBit(gpl3)/Czkawka(gpl3)/SquirrelDisk(agpl3) 只借思路不借代码，本项目 MIT 独立实现。
- 针对长扫描停滞的治理完整（阶段计时、陪伴文案、心跳、活动流）且有测试。

**技术债清单**
1. ★ 核心承诺与实测冲突：README.md:38 承诺 Quick "usable in a few minutes on typical machines"（我复核原文），而两份烟测分别记录 8 分 10 秒全扫与单目录 4 分 48 秒卡顿未完成——**自家烟测证伪自家承诺**，且 PRODUCT_PLAN 无对应性能优化条目。
2. ★ 对标调研缺口：github-research.md 无 SpaceSniffer/WizTree/TreeSize 条目（grep=0），偏清理脚本调研，无功能/许可证/可视化对比矩阵。
3. ☆ 测试环境单一：仅一台 Windows 11 Home 中文机（C 盘 2.8% 剩余），RELEASE_NOTES 却声明支持 Win10/Win11——Win10、非管理员、域环境、非 NTFS 全无证据。
4. ☆ 分类知识四处重复维护：lib.rs 规则、README "What This App Never Deletes"、SKILL.md、references/windows-cleanup-heuristics.md，无单一事实源，v0.3 扩 allowlist 时极易漂移。
5. ☆ 文档职责重叠与命名陷阱：RELEASE_CHECKLIST(v0.1) 与 RELEASE_CHECKLIST_0.2.0 并存；根目录 untracked 的 `AGENTS.md` 与 `AGENT.md` 一字之差（git status 可见 `?? AGENTS.md`）。
6. ☆ git 历史过粗：11 提交、首提交即 8,250 行，无法 bisect。
7. ☆ 双语成本：UI/报告硬编码 zh-CN、文档全英文，无 i18n 层。
8. ☆ 天花板（来自安全模型调研，需正视）：只读模型无法兑现"空间回来了"的时刻，价值让渡给 cleanmgr/存储感知；且与 Windows 原生"清理建议 + 存储感知"正面竞争。

---

## 三、外部对标

### 3.1 SpaceSniffer 专节

**基本事实**：意大利独立开发者 Umberto Uderzo（uderzo.it）产品，2009-04-18 首发，treemap 概念源自 Shneiderman 并声明获授权。许可为 **freeWare（接受捐赠）但闭源**：Disclaimer.txt 明确"个人/教育/非营利/商业使用均免费"同时写明 "source code is not free"、禁止逆向与改包。当前最新版 **2.2.0.27**（2026-03-25）。技术栈为 **Delphi / Object Pascal + VCL**（对官方 x64 exe 静态检查：System.Generics.Collections、Vcl.Forms 等单元名、FileVersion 2.2.0.27），渲染从 VCL/GDI 演进到 DirectDraw。

**版本时间线（官网 release 日期直读）**：1.x 系列 2009–2016 → **约 8 年半停更** → 2025-05-21 复活（2.0.1.4，x64 + 图形引擎完全重写 + "代码大多重写"）→ 2.0.3.12（2025-09，High DPI + Unicode 补课）→ 2.1.0.21（2026-01，修复 hacked SNF 加载漏洞）→ 2.2.0.27（2026-03）。**结论：不是死项目，而是 2025 年起进入密集迭代期的复活项目。**

**实现与 UX**：非 MFT 直读，而是传统目录遍历 + smart caching engine（边扫边显示、扫描未完成即可导航、多视图共享一次扫描、文件系统变化闪烁提示）；treemap 单击展开/双击 zoom-to-fit/浏览器式前进后退；**极强的组合过滤 DSL**（`*.jpg`、`>2years`、`|` 排除、`:red` 标签、`;` 连接）；四色标签、右键调用系统菜单、命令行 scan/load/save/export；配置只写 XML 不写注册表。

**弱点（有据）**：① 2016→2025 停更 8 年半，High DPI/Unicode 拖到 2025 才补且反复修 DPI 回归；② 官方 release notes 自证缺陷类型：ADS 扫描崩溃、reparse point 潜在死循环、treemap 浮点误差、离线 OneDrive 文件性能差、**SNF 快照加载漏洞**、UPX/无签名致杀软误报；③ 非官方镜像仓库 redtrillix/SpaceSniffer（2039★）用户 issue：app hang、Defender 隔离、右键卡住；④ SuperUser：OneDrive 目录管理员也读不到、与资源管理器已用空间口径不一致出现 "Unknown (not yet scanned)"；⑤ API 遍历天然慢于 MFT 直读的 WizTree；⑥ **中文搜索被 space-sniffer.cn 等 SEO 镜像站占据、排在官网之前，冒名下载风险真实存在**；⑦ 闭源禁止逆向，社区无法贡献。

**对本项目的直接启示（采纳）**：
- **演进先例**：SpaceSniffer 自己在 2.0.1.4 只重写图形引擎与底层、保留全部 UX 资产与产品形态——"换最弱一层、保留资产"是同领域成熟产品的既有先例，直接支持混合路线。
- **不照抄**：实时 FS 监控闪烁、ADS、右键菜单、拖放、二进制快照、meta-commands 属"可视化探索器"功能树，对一次性只读报告价值低且扩大攻击面。
- **值得借鉴**：过滤 DSL 即时重算（同一数据集切换视图无需重扫）、边扫边可浏览、变化高亮（只读版可做"比上次报告变大"的 diff 闪烁）、四色标签（可映射为已确认/待处理/忽略/危险）。
- **避坑**：Unicode/高 DPI 必须架构层一次做对（SpaceSniffer 拖 9 年）；权限不足/OneDrive/长路径要有明确"未扫描"状态而非静默少算；报告口径差异要在 UI 解释；本地快照/报告文件要做完整性校验（SNF 漏洞教训）；**SpaceSniffer 2.2.0.27 官方移除了"实验性并发目录扫描，因为没有收益"——说明并发扫目录收益不普适，本项目对"并行遍历能提速多少"必须实测而非想当然。**
- **定位**：SpaceSniffer 是"可视化探索 + 就地操作"，本项目是"只读顾问"。treemap 值得做（发现层），但不引入删除/拖放，否则撞上它的主场并稀释只读安全边界。
- **分发**：闭源致其中文分发被镜像站占领；本项目开源 + 可审计 + 校验和便携包是差异化卖点，应明写。

**调研局限**：社区舆情（Reddit/HN/论坛）因网络策略全程未能取证；Delphi/VCL 与版本时间线来自 exe 静态检查 + 官网直读（可信度高）；百万文件实际耗时、长路径支持、treemap 具体布局算法未验证。

### 3.2 磁盘分析器横向对比表

| 工具 | 许可/价格 | 技术栈 | 扫描方式 | 可视化 | 状态（2026-09） | 对本项目含义 |
|---|---|---|---|---|---|---|
| **本项目** | MIT，便携未签名 | Tauri 2 + React + Rust + PS 脚本 | PS 单线程目录递归（8 分 10 秒基线） | **仅卡片列表，0 图表** | v0.2.0，停滞一个月重启 | 只读顾问定位，发现层与速度是短板 |
| SpaceSniffer 2.2.0.27 | freeware **闭源** | Delphi/VCL + DirectDraw | 目录遍历 + smart cache（边扫边看） | treemap（钻取/缩放/过滤 DSL） | 2025 起密集迭代 | 演进先例（只换图形引擎）；发现层标杆 |
| WizTree | 个人免费/商用收费，闭源 | 未核实（官网宣称） | **NTFS MFT 直读**（需管理员），4.34s vs 老 WinDirStat 3min20s（厂商自测） | treemap + CSV/MFT 导出 | 持续更新（含 macOS 版） | 速度天花板；商用许可；不必正面对抗 |
| WinDirStat | **GPL-2.0** 4126★ | C++/MFC | 新版已自行解析 MFT（FinderNtfs.cpp）+ 多线程 | treemap（KDirStat/SequoiaView 风格） | 活跃（v2.8.8 beta 2026-09-25） | 开源 treemap 参考实现；只借思路 |
| TreeSize | 商业闭源，Free/Pro 分档 | C++ | MFT 快扫 + 32 线程遍历回退，扫描中可浏览 | 树 + 饼图 + treemap | 25 年以上 | Storage Growth（索引对比）是增量分析范式 |
| MangoDisk | **GPL-3.0** 3278★ | **Tauri 2 + Rust（同栈）** | 原生侧扫描 | 存储分析 | 当日仍有提交 | 同栈可行性的最强背书；走"清理+删除一体" |
| ncdu-win-qt | MIT 85★ | C++17/Qt6 | MFT 直读多线程 | 列表 + squarified treemap | 新秀 | 轻量对照 |
| TreeMap-Disk-Visualizer | MIT 800★ | Electron + TS | 跨平台 | treemap | 2026-06 新建 | Web treemap 头部新秀 |
| Windows 原生（存储感知/清理建议） | 内置免费 | 系统组件 | 系统级 | 分类卡片 | 持续维护 | **免费正面竞争者**；分类占位会被原生替代 |

关键读数：**没有任何成熟磁盘工具用脚本语言做主扫描器**（WinDirStat=C++、TreeSize=C++、gdu=Go、dua/dirstat-rs=Rust、WizTree/新版 WinDirStat=自行 MFT 解析）；treemap 是桌面端绝对主流形态；WizTree 的 46x 数据是厂商自测且对象是 2014 年代老版 WinDirStat，**不可直接外推**。

### 3.3 清理工具安全模型对标

- **成熟形态共性**：预览/dry-run 先行（BleachBit Preview、MangoDisk 默认只读扫描 + CLI 非交互必须 --yes、Windows 清理建议先看后清）；风险分级 + 默认不勾选 + 弱匹配不勾选附理由（MangoDisk risk/verified_buildable/evidence 字段、BHUninstaller 审查面板不可跳过）；删除通道分级（BHUninstaller **永不直接删除、一律进回收站**、blocklist 在删除时刻复判；WindowsClear 不删而迁+回滚）；规则库外置可审计（BleachBit CleanerML/Winapp2.ini、Dism++ Data.xml、MangoDisk 带 schema_version 的 TOML）；操作留痕（MangoDisk Operation History）。
- **删除即攻击面（硬证据）**：CCleaner CVE-2017-20201（供应链投毒）、**CVE-2025-3025**（清理功能不安全删除致本地提权到 SYSTEM）、**CVE-2026-12410**（卸载删除时跟随 symlink/junction 提权）；**连微软 cleanmgr 都有 CVE-2025-21420**（2025-02-11）。
- **对本项目的含义**：只读模型"结构上不可能删错文件"是可承接不敢用清理软件的个人用户、受管企业设备的**结构性护城河**；v0.3 引入写能力时必须照抄的模式是：入参只收 reportId+candidateIds、执行前三重复检（含拒绝 reparse 目标）、回收站优先、blocklist 删除时刻复判、action-log 审计、plan-first 与 dry-run 共用执行计划——**这些全部需要模块边界，2077 行单文件承载不了**。
- **天花板要正视**：价值让渡（只读不产生"空间回来了"时刻）、原生功能竞争、中文搜索词被镜像站/商业工具占据、v1.0 才计划签名（分发信任）。

### 3.4 技术栈与可视化趋势

- **Tauri v2 处于成熟维护期**：v2.0.0（2024-10-02）→ v2.11.6（2026-09-19），v3.0.0-alpha 刚起步；MangoDisk（3278★）证明"磁盘清理 + Tauri 2"是被市场验证的方向。**迁 Electron 在系统工具赛道无收益，迁 WinUI/WPF/C++ 等于整体重写，均无证据支持。**
- **扫描执行层是本栈唯一三雷区**：PowerShell 作为核心扫描载体同时踩中性能（本机实测 powershell 启动约 0.66-0.73s/次，3 次样本单机）、可用性（Windows 默认仅 5.1，pwsh 7 需另装；执行策略官方明言"不是安全边界"）、安全（AMSI 脚本内容扫描；真实误报 issue：warp 脚本被 Kaspersky 判 Trojan、openclaw 的 -EncodedCommand 触发 Defender 误报）。**结论：Rust 进程内扫描是第一优先级的渐进重构点，PowerShell 降级为可选/兜底。**
- **Rust 生态成熟可承接**：walkdir 2.5（6.42 亿下载）、ignore、rayon 1.12（5.61 亿）、windows crate 0.62（3.37 亿）、mft crate v0.7.0（2025-12，纯 safe Rust 解析 MFT 快照）。MFT 直读可作后续可选管理员模式（难点在"获取 MFT"而非"解析 MFT"）。
- **可视化无需重做**：ECharts 6.1（内置 treemap）、d3-hierarchy 3.1.2、@visx/hierarchy、react-window/tanstack-virtual 均活跃；WebView2 即 Chromium，库兼容性等同 Chrome；大数据量靠"聚合 + 分层下钻 + 只渲染当前视图"，与是否重构无关。
- **工程化参照（MangoDisk）**：pnpm workspace + Cargo workspace + src-tauri/{crates,plugins,capabilities} + vitest/eslint + cross-platform CI——即"规则库独立 crate、capabilities 最小权限、默认只读 + 操作留痕"是同栈头部项目的现行做法。

---

## 四、三种路线对比表

| 维度 | 推倒重构（重构派） | 继续开发（续研派） | **混合路线（本裁决）** |
|---|---|---|---|
| 核心动作 | 拆掉 lib.rs / App.tsx / PS 扫描内核三块地基重建，保留 IPC 契约、报告 schema 与安全门禁 | 小步重构（拆文件 + 修 bug + 收紧规则）先行，扫描性能走"单层替换"，可视化与对标补课并行 | 契约冻结下分 5 阶段定点重建：止血 → 拆分 → 换内核 → 发现层 → 写能力，每阶段可发布 |
| 成本 | 最高：约 3,000-3,500 行重写，4-6 周零功能时间盒 | 最低且分散：每周有功能产出 | 中高：累计约 6-9 周，但拆成独立可回退的阶段 |
| 主要风险 | 单人项目 + 11 提交历史下弃坑概率高；半途状态（安全断言改一半、双内核并存）比不改更糟；性能收益（<60s）未经实测 | 债务复利；**v0.3 写能力可能在结构门槛前被提前**（最危险单点）；性能失信持续 | 边界腐蚀回潮（拆完又往单文件塞）、阶段收尾靠纪律无机制约束、双内核口径回归 |
| 主要收益 | 状态机/分类/内核一次到位；安全断言升级为行为级测试 | 资产零风险保全、快速补可视化 | 兼得：保全四类不可复制资产 + 完成四处必要重建 + 每步可验证可回退 |
| 适用条件 | 有 ≥4 周连续投入、能保证阶段收尾、契约全程冻结 | 能严格执行"写能力前置门槛"顺序纪律（但其 requiredWork 前四项实质就是重构） | 契约冻结 + 每阶段 clean-install verify 全绿 + 顺序不可颠倒 |
| 与证据吻合度 | 中：三个缺陷判断正确，但集中时间盒与性能假设未被证据支撑 | 中低：低估真实缺陷与写能力门槛，且 keyEvidence 含已被证伪条目 | **高：三派收敛项全部吸收，分歧项（节奏）用基准闸门与顺序纪律裁决** |

**三方收敛的"保留清单"（谁上台都不动）**：6 个 IPC 命令名与 camelCase 字段、scan-progress 事件、`[WCDCA_PROGRESS]` 行协议、schemaVersion 0.1.0 旧报告可读、12 步 verify 骨架与 subst 契约测试、57 项测试语义、Tauri v2 + React 技术栈、只读定位与 Windows 领域知识、v0.2.0 发布证据链。

---

## 五、最终建议与理由

### 裁决：**hybrid（混合路线）**

### 5.1 为什么不是"纯续研（continue）"

1. **已确认的正确性缺陷不允许"原地不动"**：我逐行核实 `update_task_status`（lib.rs:1132-1160）对 phase 无条件写入、`is_terminal_phase` 存在却未被用于守卫——取消可被静默撤销是**复现路径清晰的并发正确性问题**，不修就是把"取消没生效"的用户信任问题带进 v0.3。
2. **v0.3 写能力是硬门槛**：删除动作即提权攻击面（三条 CVE 实证），把 allowlist 清理堆进混装七类职责的 2077 行单文件，等于把安全回归面集中到一个文件。"先拆边界再放写能力"不是审美偏好，是安全顺序——而这条顺序本身就要求先做结构性工作。
3. **性能债无法在 PowerShell 内增量偿还**：8 分 10 秒全扫、单目录卡 4 分 48 秒、Quick/Deep 仅差一个 flag 导致同一子树遍历 2-3 次、行业无一用脚本语言做主扫描器——调参补不了数量级差距，必须换遍历实现（这是单层替换，但性质是重建）。
4. **发现层是整层缺失**：在 861 行单组件 + 0 变量 CSS 上直接堆 treemap/搜索/历史对比，债务随时间复利，半年后再拆的成本显著高于现在。

### 5.2 为什么不是"纯重构（refactor）"

1. **重构派自己的 verdict 已把范围收缩成混合路线**：其保留清单（IPC 契约、报告格式、安全门禁资产、57 项测试、verify 骨架、subst 契约测试）与混合派"保留"清单几乎逐项重合；两者实质分歧只剩**节奏**（4-6 周集中 vs 分阶段可发布）。它主张的"重建地基、保留房产证"在操作定义上就是分阶段定点重建。
2. **集中时间盒与执行史冲突**：全史 11 提交、首提交即 8,250 行（无法 bisect）、刚经历一个月停滞的单人项目，4-6 周零功能窗口的弃坑概率是真实风险；重构派把"停滞刚重启、无新增用户"只算作利好（迁移面小），未同时计入弃坑概率。
3. **核心收益假设未经实测**：Rust 化把 Quick 压到 <60s 是工程推断——调研自认无第三方 PowerShell vs Rust 遍历基准，且 SpaceSniffer 官方因"没有收益"移除过并发扫描。集中重建若基准闸门不达标，主要投入换不来承诺兑现，而且回退成本比"单阶段可回退"高得多。
4. **重写解决不了真正的三个用户痛点**：treemap 缺失、Win10/非管理员/域环境零证据、性能基准缺失——这些是新增投入，不是旧代码问题；重构挤占它们就是用"重构"掩盖"没做产品"。
5. **盘点本身含错误项**（stderr 死锁经我复核证伪），说明必须"核验一条、施工一条"，分阶段小步恰好容纳这种修正，集中施工则会按错误清单浪费工时。

### 5.3 为什么混合更优（不是骑墙）

**这不是取中间值，而是证据本身是分层的**：在"保留什么"上三派证据一边倒（安全门禁、契约、测试、发布证据链、技术栈全部保留）；在"必须重建什么"上也一边倒（状态机守卫、分类规则参数化、两块巨石、扫描内核四处）；唯一分叉的是"多快做完"。混合路线把前者冻结为契约、把后者裁决为"带基准闸门与回退条件的分阶段顺序"，并对三派各自的可证伪条件全部采纳：

- 采纳重构派：性能必须可量化验收、安全断言必须升级不删弱、写能力前必须有模块边界；
- 采纳续研派：契约不破、每阶段 verify 全绿、资产不丢、不要求一次赌注；
- 采纳混合派：每阶段可发布、顺序不可颠倒、先修竞态再拆结构再换内核最后开写能力。

**路线的可证伪条件**：① Phase 2 性能闸门若两个迭代周期内达不到 README 承诺，则保留 PS 内核并把产品重定位为"慢但流式可浏览"（此时仅阶段 0-1 的正确性/结构收益成立，内核替换判为不划算）；② 若任何阶段导致契约测试大面积失效且无法语义等价重建，立即停在当前阶段回退；③ 若实际可投入时间 < 4 周，砍掉 Phase 3/4，只做三件最小正确动作：终态守卫、分类 drive 参数化、建立 Quick 耗时基准。

### 5.4 证据缺口与评委论证中的可疑之处（点名，不抹平）

**评委论证可疑之处：**

1. **续研派 keyEvidence 第 6 条错误**：把"stderr 管道死锁（wait 后才读）"列为已确认 bug。我读源码证伪：lib.rs:396-402 在 `child.wait()` **之前**已 spawn 独立线程并发 `read_to_string` 消费 stderr，不存在 64KB 管道死锁。内部盘点 JSON 的 weaknesses 同样收录了这条错误。重构派与混合派都做了诚实更正，**唯独续研派照单全收**——说明评委存在复制盘点未核验内容的风险，其"约 4 处函数级改动即可修完已知 bug"的成本估计也因此偏乐观（它同时低估了拆分时同步改写 Test-SafetyBoundary 8 条 Rust 源码断言的工作量）。
2. **重构派"非 C 盘分类失效"表述过重**：`is_system_managed` 中 `contains("\\windows\\winsxs")` 等 6 条子串规则对任意盘符生效，真正失效的是 `c:\pagefile.sys`/`c:\windows` 根/`c:\$recycle.bin`/`c:\recovery` 这类精确路径规则；且前端写死 `drive:"C"` 使 UI 主路径不触发。但 `report/` 下存在 `drive-d-*`、`drive-e-*` 目录证明缺口被真实触发过——**结论方向成立、程度表述需打折**。
3. **重构派"Rust 化 <60s / 快一个数量级"是目标不是证据**：无第三方遍历基准支撑，SpaceSniffer 移除并发扫描的先例还提示并发收益不普适。本报告已把**可复现耗时基准设为 Phase 2 验收闸门并给出回退条件**。
4. **WizTree 46x/22x 数据不可外推**：厂商自测、对比对象是 2014 年代老版 WinDirStat（新版已有 MFT 快扫），横向表引用时已标注。
5. **8 分 10 秒与 4 分 48 秒是同机同目录样本**：两条数据（2026-06-30 与 2026-08-12）都围绕同一病理目录 `C:\common_attachment`、同一台机器，n=2 手工烟测；"失信于 README 承诺"结论成立，但"典型机器"的耗时分布**未知**。
6. **重构派与混合派立场高度重合**（见 5.2），三派分歧实为节奏之争；据此选择混合不是折中惯性，而是收敛项已由证据单方面决定。
7. **续研派把自己的 requiredWork 前四项称为"小步"**，但拆分 lib.rs + 同步改安全断言 + 参数化分类 + 加保留策略，在本项目里已牵动发布门禁，标签比内容更保守。
8. **混合派成本估计未量化**：它要求"每阶段过含 Tauri 构建+打包+解压冒烟的 12 步 verify"，但 verify.yml 实测恰恰跳过这三步，本地全量 verify 的真实耗时未记录在案——本报告把"记录 verify 真实输出与耗时"写入 Phase 0 收尾要求。
9. **内部盘点 refactorImpact 的迁移面估算（6 脚本+2 workflow+3 文档+tauri.conf.json）未经施工验证**，应视为量级参考而非承诺。
10. **judges 数组完整（3/3）**，无缺失影响。

**证据缺口（外部调研共同局限）：**

- 四路调研的 WebSearch/WebFetch 均被网络策略阻断，Bing 中文出口结果被 SEO 镜像站污染；**Reddit/HN/论坛社区舆情完全未取证**（SpaceSniffer 性能与兼容抱怨、清理工具信任问题缺系统性数据）。
- SpaceSniffer 源码与许可未确认（闭源 freeware、禁逆向）；"Delphi+VCL"来自 exe 静态字符串；treemap 布局算法、百万文件实测耗时、>260 字符长路径、官方中文界面均未验证。
- WizTree 实现语言未核实；RidNac 未找到现存仓库；WinDirStat "MFT 直读"是从 FinderNtfs.cpp 源码结构推断，读取方式未逐行核实。
- 无 PowerShell vs Rust vs MFT 的第三方可引用基准（相关判断属工程经验推断）；本地 powershell 启动计时仅 3 次单机样本。
- **未测试环境**：Windows 10、非管理员/提权路径、域环境、非 NTFS/UNC/网络盘、pwsh 7 分支（CI 冒烟硬编码 powershell.exe）、WebView2 最新 runtime 回归——全部零证据；CI 的 windows-latest 是 Server 系，不等于普通用户 Win10/Win11。
- **无 GUI 自动化**：真实 exe 的启动/扫描/取消/报告打开依赖手工 checklist（仅 2 次人工 smoke 记录）。
- 本终审未复核全部 1,332 行生产代码：已逐行核验争议点（终态守卫、stderr 线程、PID 时序、wait 超时、分类函数、安全断言、烟测数字、CI 配置、测试计数），其余盘点条目标注 ☆ 者为采信未复核。

---

## 六、分阶段路线图

> 总原则（全阶段不可违反）：**契约冻结**（6 命令名、camelCase 字段、scan-progress 事件、`[WCDCA_PROGRESS]` 行协议、schemaVersion 0.1.0 旧报告可读）；**每阶段以 clean-install `npm run verify` 全绿收尾**，安全断言与结构拆分同步改写、语义等价、绝不删弱；真实输出写入既有发布/状态文档；**顺序不可颠倒**（止血 → 拆分 → 换内核 → 发现层 → 写能力）。

### Phase 0 — 正确性止血（不动结构）｜约 1 周

**目标**：让"取消"与"超时"可靠，清掉无界增长，为拆分建立干净基线；本阶段不碰 Test-SafetyBoundary 断言的任何源码字符串。

- `src-tauri/src/lib.rs:1132-1160`：`update_task_status` 增加终态守卫（复用 1162-1164 的 `is_terminal_phase`），已终态任务拒绝任何 phase 写回，根治"取消被 running 覆盖"。
- `lib.rs:361-373`：PID 登记时序重构——spawn 前二次校验 phase 非 cancelled、spawn 后立即登记 PID，消除 queued 窗口孤儿扫描与双扫描并发。
- `lib.rs:404`：`child.wait()` 加超时/看门狗（超时后 taskkill /T /F 并置 failed）。
- `lib.rs:19-22、531-560、1166-1193`：AppState 与 `app_data/reports/` 加"保留最近 N 个"环形淘汰 + latest 索引，替换全量遍历。
- 新增 `src-tauri/tests/`：取消全链路（queued/running 窗口、迟到进度事件不得覆盖终态）、并发单飞、`run_scan_worker` 端到端——当前覆盖为 0。
- 记录本轮 verify 真实输出与耗时到既有状态文档（同时为 Phase 4 的 CI 分层提供基线数据）。

### Phase 1 — 结构拆分与契约固化｜1.5-2 周

**目标**：拆掉两块巨石、分类规则数据化、进度协议契约化；行为不变、契约不破、断言同步改写不删弱。

- `lib.rs` 2,077 行拆为 `commands / tasks / scanner / classify / report / errors` 模块（6 命令与序列化字段原样）。
- `scripts/Test-SafetyBoundary.ps1:84-92` 的 8 条 lib.rs 字面量断言（`Command::new(&shell)`、`.arg("-NoProfile")` 等，我已复核原文）改为按模块目录扫描的语义断言，覆盖面不减；黑名单从仅扫描脚本扩展到其余 7 个 ps1。
- `lib.rs:1021-1068` 分类规则声明式数据表化：`is_system_managed` 以本次 scan 的 drive 参数化（或盘符白名单收紧为仅 C）；`contains("\\update")`/`contains("\\qq")` 改前缀/标记 + 显式优先级，补误命中反例单测；测试反向校验 lib.rs/README/SKILL/references 四处清单一致性。
- 进度协议共享契约：`[WCDCA_PROGRESS]` 百分比与阶段码固化为常量表（PS 脚本 / `lib.rs:1255` / `App.tsx:115-184` 三处对齐），消除 `App.tsx:135 message.includes("重点目录")` 文案耦合，`Test-ScannerContract.ps1` 扩展断言。
- `src/App.tsx` 拆 `useScanSession` hook（收敛 8 处四连重置）+ ScanCompanion/SummaryPane/RecommendationList 展示组件；**App.test.tsx 13 用例先跑通再迁移**；顺带修 listen() 清理竞态、收敛 quick/deep 形同虚设的配置。
- 补 ESLint/Prettier 与 vitest 覆盖率门槛。

### Phase 2 — 扫描内核单层替换（最大单项）｜2-3 周

**目标**：用 Rust 并行遍历替换 PS 遍历内核，行协议与 JSON/Markdown 契约不变，用可复现基准做验收闸门。

- 新增 Rust 遍历模块（walkdir/rayon/windows crate）：并行遍历、`\\?\` 长路径、硬链接去重、跳过 reparse、ACCESS_DENIED 不中止而入 scanErrors、可取消保进度。
- PS 扫描器降级为可选兜底/校验路径；Test-SafetyBoundary 中 8 条"Rust 必须以固定参数启动 PowerShell"断言按新架构重定义。
- `Test-ScannerContract.ps1` subst E2E 改为双内核 A/B 等价断言（目录大小、硬链接、reparse 计数、scanErrors 口径差异显式文档化，防"建议怎么变了"）。
- **性能闸门**：建立可复现 Quick 基准（固定种子目录 + 真实 C 盘对照）。基线 8 分 10 秒 / 单目录 4 分 48 秒@27%。验收：参考机 Quick < 60s 或较基线快一个数量级；**未达标即保留 PS 内核并重定位产品为"慢但流式可浏览"，只保留 Phase 0-1 成果**。
- 冒烟去 powershell.exe 硬编码，覆盖 `resolve_powershell()` pwsh 优先分支（Test-ScannerContract.ps1:53、Test-PortablePackage.ps1:168）。
- 打包面同步：tauri.conf.json bundle.resources、New-PortablePackage.ps1:17/47、Test-PortablePackage.ps1:67-79、release checklist、AGENT.md；重跑 SHA-256 与烟测证据。

### Phase 3 — 报告发现层（补 SpaceSniffer 差距）｜1.5-2 周

- 只读 treemap（d3-hierarchy squarify 或 ECharts 6，聚合 + 分层下钻 + canvas），卡片列表保留；新依赖过许可证/体积审查。
- 搜索/路径过滤、大小/风险/置信度排序（扩展 `reportUtils.ts` + 单测）。
- `styles.css` 令牌化（998 行 → CSS 变量，消 133 处硬编码色值）。
- 历史报告列表与多次扫描对比（当前只能"载入最近一次"）。
- 明确不做：拖放、右键删除、实时 FS 闪烁、ADS、二进制快照（SpaceSniffer 主场，稀释只读定位）。

### Phase 4 — 工程保障与对标补课（可与 2/3 并行）｜约 1 周

- CI：合并 ci.yml/verify.yml 双触发、加 rust-cache、ci.yml 补 permissions；verify.yml 补回被 `-SkipTauriBuild -SkipPackage -SkipChecksum` 跳过的构建/打包/校验和（或明确分层写入 AGENT.md），让 checksum 类回归（已两次返修）在 PR 阶段可发现。
- `New-ReleaseChecksum.ps1:44` 改 sha256sum -c 兼容格式。
- `docs/research/github-research.md` 补 SpaceSniffer/WizTree/TreeSize/MangoDisk 对标矩阵（现为 0 覆盖）。
- 文档收敛：README/USER-GUIDE/AGENT/SKILL 重复段落、双 release checklist、untracked `AGENTS.md` vs `AGENT.md`、分类清单单一事实源。
- 脚本卫生：抽共享模块（Get-FreeDriveLetter、Get-Sha256Hex 各两处重复）、去全局 `SilentlyContinue` + 无条件 `[OK]`、scanErrors 200 上限改带总数截断。

### Phase 5 — v0.3 写权限清理（硬门槛：Phase 0-2 全绿后）｜2-3 周

- 入参只收 reportId + candidateIds，拒绝 UI 直传路径；capability 白名单扩展。
- 执行前三重校验（路径规范化、句柄身份复检、拒绝 reparse 目标）+ blocklist 删除时刻复判（复用 Phase 1 数据化 system-managed 清单）。
- 回收站优先；plan-first dry-run 与执行共用计划；action-log 审计；Test-SafetyBoundary 扩展负向测试（删除类 API 不可达）。
- 默认普通用户权限，仅 allowlist 系统类别走独立小提权助手。

**总量级**：Phase 0-4 约 6-9 周（单人全职当量，含并行），Phase 5 另计 2-3 周；每个 Phase 独立可交付、可回退。

---

## 七、风险与未验证事项

### 路线风险

1. **半途而废**（最大风险）：混合路线的正确性依赖"每阶段收尾"，而仓库全史 11 提交、曾停滞一个月。**缓解**：Phase 0-1 优先交付（即使后续停摆也已拿到正确性与结构收益）；把阶段验收写进 AGENT.md 成为门禁而非口头纪律。
2. **边界腐蚀回潮**：拆分后新功能又往单文件塞；"临时" PS 兜底内核永久化——现成先例就是冒烟硬编码 powershell.exe 而运行时优先 pwsh，**测试与真实路径已经不一致**。缓解：Test-SafetyBoundary 改为按目录扫描 + CI 断言模块行数上限。
3. **安全网静默失效**：Test-SafetyBoundary 是源码字面量断言，拆分时最隐蔽的失败模式是为过门禁把断言改松。缓解：断言改写必须语义等价并在 PR 中逐条 diff 说明，禁止删弱。
4. **口径回归**：Rust 与 PS 遍历在硬链接/reparse/scanErrors 统计上的语义差异会让升级前后报告数字变化。缓解：Phase 2 的 subst A/B 等价断言 + 显式差异文档。
5. **测试资产在拆分中丢失**：App.test.tsx 13 例锁住的 stale 事件/迟到轮询/卸载清理是隐性资产。缓解：先跑通再迁移，迁移前后用例数只增不减。
6. **性能闸门不达标**：已设回退条件（保留 PS 内核 + 重定位），避免"用重构掩盖没做产品"。
7. **范围蔓延**：treemap → 交互过滤 → 顺手加删除，逐步滑向 SpaceSniffer/WinDirStat 主场。缓解：Phase 5 单独门槛，Phase 3 明确不做清单。
8. **产品级天花板**（重构解决不了）：只读模型的价值让渡、与 Windows 原生清理建议竞争、"C 盘清理"搜索词被镜像站占据、v1.0 才有签名——这些需要产品与分发投入，已在对标章节点名。

### 未验证事项与证据缺口（汇总）

- 调研全程 WebSearch/WebFetch 被网络策略阻断；Bing 受中国区出口影响；**社区舆情未取证**。
- SpaceSniffer 源码/许可/布局算法/百万文件实测/长路径支持未验证；WizTree 实现语言未核实；RidNac 未找到；WinDirStat MFT 结论为源码结构推断。
- 无第三方 PowerShell vs Rust vs MFT 遍历基准；powershell 启动计时 3 次单机样本。
- 性能证据 n=2 且同机同病理目录；"典型机器"分布未知。
- **未测试环境**：Win10、非管理员、域环境、非 NTFS/UNC、pwsh 7 分支（CI 硬编码 powershell.exe）、WebView2 最新 runtime；CI 为 Server 系。
- 无 GUI 自动化 e2e；Rust 集成测试 0；PS 扫描器逻辑级单测 0。
- git 11 提交无法 bisect；内部盘点 refactorImpact 迁移面为估算。
- 评委侧：续研派含 1 条已被证伪的 keyEvidence（stderr 死锁）；重构派 1 条表述过重（非 C 盘分类失效）+ 1 条未实测收益假设（<60s）；三派共同引用的性能基线为单机 n=2；judges 3/3 完整无缺失。

---

*本报告为只读评估产物，未修改项目任何文件；所有"我复核"标注的行号与数字均来自本次对源码/文档的直接读取。*
