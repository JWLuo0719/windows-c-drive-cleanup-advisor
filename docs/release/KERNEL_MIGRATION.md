# 内核迁移注记（Phase 2：扫描内核单层替换）

日期：2026-09-26
范围：扫描内核从「PowerShell 单内核」迁移为「原生 Rust 内核默认 + PowerShell 兜底」双内核。
本文档记录两内核之间的**口径差异**、实测证据与回退指引。安全边界不变：两内核都只读，不删除、不移动、不修改任何用户文件。

## 1. 架构摘要

| 项 | 值 |
| --- | --- |
| 默认内核 | 原生 Rust（`src-tauri/src/kernel.rs`，进程内单次遍历） |
| 兜底内核 | PowerShell（`scripts/Scan-CDriveCleanupAdvisor.ps1`，双遍历） |
| 切换方式 | 环境变量 `WCDCA_SCAN_KERNEL=powershell` 显式选择兜底；缺省即原生 |
| 测试入口 | `src-tauri/src/bin/wcdca-scan.rs`（A/B 等价断言与性能基准专用，不随产品打包） |
| 进度协议 | 形状完全保持（`[WCDCA_PROGRESS] {percent}\|{code}` 三方常量表、心跳码语义不变） |
| 取消传播 | 原生经进度回调返回 `false` 协作式取消；PS 兜底仍走 taskkill 进程树 |

单遍历合并：原生内核用一次全盘遍历同时产出顶层行与大文件候选（PS 为 Get-LocalTreeSize 与 Get-LargeFiles 两次独立遍历）；deep 深挖仍为独立锚点遍历，与 PS 一致。

## 2. 口径差异表

行为一致、但实现取舍相关的项（差异均已在 A/B 断言或测试中显式看护）：

| 项 | PS 兜底内核 | 原生内核 | 影响与处理 |
| --- | --- | --- | --- |
| 硬链接 | 每个链接按全量计入 | 同左（按链接计数） | 一致。曾计划按 file_index 去重，但 std `MetadataExt::file_index` 需 unstable feature `windows_by_handle`，放弃；A/B 断言 `0.06\|2\|1\|0` 看护两内核 Files=2、SizeGB=0.06 |
| scanErrors 计数 | 快扫两遍（TreeSize* + LargeFiles*），同一失败路径最多记 2 次 | 单次遍历，每路径记 1 次 | A/B 断言实测 2:1（`Test-ScannerContract.ps1` 硬断言） |
| scanErrors stage 名 | TreeSizeEnumerate / TreeSizeEntry / LargeFilesEnumerate / LargeFilesEntry | 只有 TreeSizeEnumerate / TreeSizeEntry（大文件收集合并进主遍历） | 原生不产生 LargeFiles* stage；报告消费者不得按 stage 文本推断遍历轮次 |
| scanErrorTotal 字段 | JSON 无此字段（列表上限 200，截断后无从得知总数） | JSON 有 `scanErrorTotal`（截断前总数）+ `scanErrorLimit` | 富化解析兼容缺省（缺省取列表长度）；Phase 4 脚本卫生给 PS 补总数 |
| SizeGB 舍入 | `[math]::Round(x, 2)`（banker's rounding，.xx5 取偶） | `(x*100).round()/100`（half-away-from-zero） | .xx5 边界可差 0.01 GB；A/B 种子尺寸避开边界（0.01～0.06 区间内无 .xx5） |
| 行排序 | `Sort-Object SizeGB -Descending`，平手次序不保证 | 精确字节降序，平手按路径升序（确定性） | top-N 截断在 SizeGB 平手时可能取到不同行；A/B 种子用唯一 SizeGB 规避，UI 不得依赖平手次序 |
| 大文件 LastWriteTime | .NET DateTime 序列化字符串 | Unix 毫秒整数，缺失为 null | 表示法不同，语义同一时刻；A/B 不比此项 |
| pagefile | WMI `Win32_PageFileUsage`（Name/AllocatedBaseSize/CurrentUsage/PeakUsage） | 容量（AllocatedBaseSize 换算 MB），CurrentUsage/PeakUsage 为 null | 峰值不经 WMI；UI 需容忍 null |
| computer.AutomaticManagedPagefile | WMI 真实值 | null | 原生不读 WMI；UI 需容忍 null |
| hibernation | C:\hiberfil.sys 路径 + SizeGB | 同左 | 一致 |
| DISM 分析 | 管理员时 `Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore`，输出 -join 换行 | 同参数同只读命令，OEM 码页解码复用扫描器行解码（UTF-8 优先 GBK 兜底） | 一致；DISM 阶段结构性静默沿用（超时保底） |

路径展示形一致（`Z:\DataRoot` 等，非 verbatim）；原生内部系统调用走 `\\?\` verbatim 长路径形。

## 3. 性能闸门与实测证据

闸门定义（AGENT.md / 评估报告）：原生内核须 **快一个数量级（≥10x）** 且 Quick 扫描绝对时长 **< 60s**；未达标则保留 PS 内核并重定位产品。

实测（2026-09-26，本机 Windows 11 Home China 10.0.26200，**release 构建**，与产品打包同优化级别）：

```text
# 种子树 A/B（scripts/Measure-ScanPerformance.ps1，默认 300 目录 × 16 文件 + 大文件候选）
[PERF] seed-tree dirs=300 files=4806 native=0.02s ps=2.81s speedup=144.7x gates=10x,30s
[OK] Seed-tree performance gate passed

# 真实 C 盘 Quick（scripts/Measure-ScanPerformance.ps1 -RealDrive -Drive C）
[PERF] real-drive quick drive=C:\ native=35.7s budget=60s
[OK] Real-drive performance gate passed
```

对照记录：同一真实盘 Quick 在 debug 构建（`cargo build`，无优化）下为 110s——性能基准一度误用 debug 产物并触发 60s 预算失败，已改为强制 `cargo build --release` 后通过。基线参照（v0.2 PS 内核历史实测）：全盘扫描 8 分 10 秒、单目录 4 分 48 秒；原生内核 Quick 35.7s 较历史全盘基线快约 13 倍，且满足 Quick < 60s 的绝对验收预算。

闸门结论：**双条件均达标**（种子树 ≥10x 数量级：144.7x；真实盘 Quick 绝对时长：35.7s < 60s）。按 AGENT.md 裁决保留原生内核为默认，无需产品重定位。

命令实际输出保留于本文档；未在其他机器/系统版本上重跑（见第 5 节）。

## 4. 验证证据（Phase 2 收口）

| 检查 | 结果 |
| --- | --- |
| `cargo test`（src-tauri） | 66 passed; 0 failed（含 kernel 13 项：遍历计数、大文件阈值、重解析跳过、硬链接链接计数、不可读路径、错误上限、进度冻结次序、取消传播、JSON 属性名、Markdown 冻结文案、深挖截断、标记行格式） |
| `scripts/Test-SafetyBoundary.ps1` | 通过（含双内核新断言：进程启动面白名单仅 Dism.exe、原生内核不接 PS 启动面、非测试区无删除/改名 API、默认内核原生、PS 兜底 opt-in、wcdca-scan 不打包） |
| `scripts/Test-ScannerContract.ps1` | 通过（三方协议表静态一致 + subst E2E + 双内核 A/B：top 行逐字段、大文件行、硬链接、junction Skipped=1、scanErrors 2:1、scanErrorTotal 截断语义） |
| `scripts/Measure-ScanPerformance.ps1` | 通过（release 构建：种子树 144.7x ≥ 10x 闸门；真实盘 Quick 35.7s < 60s 预算，见第 3 节实测输出） |

## 5. 未测试环境与限制

- 仅在本机 Windows 11（PS 5.1、NTFS、管理员+普通用户两种 token 下的提权检测）验证；Windows 10、ReFS、非 NTFS 网络盘未测。
- 硬链接按链接计数的口径对「同一文件多个链接」的大盘会重复计量——两内核一致，且为只读报告的安全侧行为（宁多勿漏）。
- 原生内核的并行 worker 在网络映射盘上的表现未测（subst 与本机 NTFS 已测）。
- 真实盘实测依赖本机当时的目录规模与缓存状态，仅作量级证据，不是跨机承诺。

## 6. 回退指引

设置环境变量后重启应用（或直接运行脚本）即回退 PowerShell 兜底内核：

```powershell
$env:WCDCA_SCAN_KERNEL = "powershell"
```

回退后行为与 v0.2.x 完全一致（PS 双遍历 + 本表左列口径）。安全边界测试对兜底启动面的原断言（固定参数启动、捆绑资源解析、debug-only 源树回退）持续看护该路径。
