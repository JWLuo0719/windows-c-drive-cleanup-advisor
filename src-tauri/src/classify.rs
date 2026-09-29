use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recommendation {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) size_gb: f64,
    pub(crate) category: String,
    pub(crate) risk: String,
    pub(crate) confidence: f64,
    pub(crate) reason: String,
    pub(crate) manual_steps: Vec<String>,
    pub(crate) cleanable: bool,
    pub(crate) blocked_reason: Option<String>,
    pub(crate) cleanup_method: Option<String>,
    pub(crate) requires_app_closed: bool,
    pub(crate) source: String,
}

pub(crate) fn aggregate_repeated_cache_recommendations(
    recommendations: Vec<Recommendation>,
) -> Vec<Recommendation> {
    let existing_paths = recommendations
        .iter()
        .map(|item| item.path.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut groups: HashMap<String, (String, Vec<Recommendation>)> = HashMap::new();
    let mut suppressed_child_paths = HashSet::new();

    for item in &recommendations {
        if item.category != "low-risk-cache" {
            continue;
        }
        let Some(root) = cache_review_root(&item.path) else {
            continue;
        };
        let root_key = root.to_ascii_lowercase();
        let item_key = item.path.to_ascii_lowercase();
        if root_key == item_key {
            continue;
        }
        if existing_paths.contains(&root_key) {
            suppressed_child_paths.insert(item_key);
            continue;
        }
        groups
            .entry(root_key)
            .or_insert_with(|| (root, Vec::new()))
            .1
            .push(item.clone());
    }

    let grouped_child_paths = groups
        .values()
        .filter(|(_, items)| items.len() > 1)
        .flat_map(|(_, items)| items.iter().map(|item| item.path.to_ascii_lowercase()))
        .collect::<HashSet<_>>();

    let mut output = recommendations
        .into_iter()
        .filter(|item| {
            let key = item.path.to_ascii_lowercase();
            !suppressed_child_paths.contains(&key) && !grouped_child_paths.contains(&key)
        })
        .collect::<Vec<_>>();

    for (_, (root, items)) in groups {
        if items.len() <= 1 {
            continue;
        }
        output.push(aggregate_cache_group(root, items));
    }

    output
}

fn aggregate_cache_group(root: String, items: Vec<Recommendation>) -> Recommendation {
    let size_gb = items.iter().map(|item| item.size_gb).sum::<f64>();
    let count = items.len();
    let mut confidence = items
        .iter()
        .map(|item| item.confidence)
        .sum::<f64>()
        / count as f64;
    confidence = confidence.clamp(0.0, 0.9);

    Recommendation {
        id: Uuid::new_v4().to_string(),
        path: root,
        size_gb,
        category: "low-risk-cache".to_string(),
        risk: "low".to_string(),
        confidence,
        reason: format!(
            "同一缓存目录下发现 {count} 个较大的缓存文件，已合并为目录级候选，便于一次性人工复核。"
        ),
        manual_steps: vec![
            "先关闭相关应用。".to_string(),
            "打开该缓存目录，按大小或修改时间复核这些文件。".to_string(),
            "优先使用应用自带清理入口；v0.2 仍不执行删除。".to_string(),
        ],
        cleanable: false,
        blocked_reason: Some("内置清理仍后移到 v0.3 白名单流程。".to_string()),
        cleanup_method: Some("manual-cache-review".to_string()),
        requires_app_closed: true,
        source: "heuristic".to_string(),
    }
}

fn cache_review_root(path: &str) -> Option<String> {
    let normalized = path.replace('/', "\\");
    let lower = normalized.to_ascii_lowercase();
    for marker in [
        "\\dxcache\\",
        "\\glcache\\",
        "\\gpucache\\",
        "\\code cache\\",
        "\\npm-cache\\",
        "\\ms-playwright\\",
        "\\cachedextensionvsixs\\",
        "\\pip\\cache\\",
        "\\.gradle\\caches\\",
        "\\autoupdate\\download\\",
        "\\cache\\",
    ] {
        if let Some(index) = lower.find(marker) {
            let root_end = index + marker.len() - 1;
            return Some(normalized[..root_end].trim_end_matches('\\').to_string());
        }
    }
    None
}

pub(crate) fn compare_recommendations_for_review(
    left: &Recommendation,
    right: &Recommendation,
) -> std::cmp::Ordering {
    let category_delta = recommendation_category_rank(&left.category)
        .cmp(&recommendation_category_rank(&right.category));
    if category_delta != std::cmp::Ordering::Equal {
        return category_delta;
    }

    let left_is_root = is_drive_root_summary(&left.path);
    let right_is_root = is_drive_root_summary(&right.path);
    if left_is_root != right_is_root {
        return if left_is_root {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        };
    }

    right
        .size_gb
        .partial_cmp(&left.size_gb)
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn recommendation_category_rank(category: &str) -> u8 {
    match category {
        "low-risk-cache" => 0,
        "app-managed" => 1,
        "user-data" => 2,
        "uninstall-or-migrate" => 3,
        "system-managed" => 4,
        _ => 5,
    }
}

fn is_drive_root_summary(path: &str) -> bool {
    let normalized = path.trim_end_matches('\\');
    if normalized.len() == 2 {
        let mut chars = normalized.chars();
        let Some(drive) = chars.next() else {
            return false;
        };
        return drive.is_ascii_alphabetic() && chars.next() == Some(':');
    }
    let mut chars = normalized.chars();
    let Some(drive) = chars.next() else {
        return false;
    };
    if !drive.is_ascii_alphabetic() || chars.next() != Some(':') {
        return false;
    }
    if chars.next() != Some('\\') {
        return normalized.len() == 2;
    }
    !chars.any(|ch| ch == '\\')
}

pub(crate) fn classify_recommendation(
    path: &str,
    size_gb: f64,
    source_name: &str,
) -> Recommendation {
    let lower = path.to_ascii_lowercase();
    let id = Uuid::new_v4().to_string();

    if is_system_managed(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "system-managed".to_string(),
            risk: "blocked".to_string(),
            confidence: 0.96,
            reason: "这是由 Windows 管理的位置。手动删除可能影响系统修复、更新、启动或回滚能力。"
                .to_string(),
            manual_steps: vec![
                "仅使用 Windows 设置、磁盘清理、DISM 或官方说明中的系统工具处理。".to_string(),
                "不要直接删除这个路径。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("系统托管路径，不提供自动清理。".to_string()),
            cleanup_method: Some("manual-windows-tool".to_string()),
            requires_app_closed: false,
            source: source_name.to_string(),
        };
    }

    if is_low_risk_cache(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "low-risk-cache".to_string(),
            risk: "low".to_string(),
            confidence: 0.82,
            reason: "这看起来像缓存或更新包目录，相关应用通常会在后续使用中重新生成。".to_string(),
            manual_steps: vec![
                "先关闭相关应用。".to_string(),
                "优先使用应用内清理入口；如需手动处理，请先确认内容。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("内置清理已后移到 v0.3 白名单流程。".to_string()),
            cleanup_method: Some("manual-cache-review".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    if is_app_managed(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "app-managed".to_string(),
            risk: "medium".to_string(),
            confidence: 0.85,
            reason: "这看起来是应用托管数据。聊天软件、网盘和编辑器可能在这里保存数据库、索引或本地副本。".to_string(),
            manual_steps: vec![
                "优先使用应用自带的存储管理功能。".to_string(),
                "手动删除前请先备份或逐项确认。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("应用托管数据需要用户人工确认。".to_string()),
            cleanup_method: Some("app-storage-manager".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    if is_uninstall_or_migrate(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "uninstall-or-migrate".to_string(),
            risk: "high".to_string(),
            confidence: 0.78,
            reason: "这看起来像已安装软件、SDK、组件包或大型应用目录。".to_string(),
            manual_steps: vec![
                "请使用 Windows 应用设置或厂商卸载器处理。".to_string(),
                "迁移项目、SDK 或工具缓存前，先确认依赖它们的工具不会受影响。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("已安装软件不应直接删除。".to_string()),
            cleanup_method: Some("uninstall-or-migrate".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    Recommendation {
        id,
        path: path.to_string(),
        size_gb,
        category: "user-data".to_string(),
        risk: "medium".to_string(),
        confidence: 0.7,
        reason: "这是用户可见或类似项目的数据，可能包含下载、文档、媒体、源码或导出文件。"
            .to_string(),
        manual_steps: vec![
            "打开文件夹并按大小排序。".to_string(),
            "确认不再需要后，再移动、归档或删除。".to_string(),
        ],
        cleanable: false,
        blocked_reason: Some("用户数据需要明确的人工确认。".to_string()),
        cleanup_method: Some("manual-review".to_string()),
        requires_app_closed: false,
        source: source_name.to_string(),
    }
}

// ==== 分类规则声明式数据表 ====
// 显式优先级：classify_recommendation 按下列顺序短路匹配，先命中者胜：
//   system-managed → low-risk-cache → app-managed → uninstall-or-migrate → user-data（兜底）
// 匹配方式（MatchKind）：
// - RootExact     盘根精确条目；盘符参数化后与实际盘符拼接（不写死 c:\，任意盘符一致生效）。
// - RootTree      盘根子树：等于该名或位于其下。
// - SegmentSeq    完整目录段序列：只在段边界命中，不吞段内前缀（收紧误命中，如 update / qq）。
// - SegmentPrefix 目录段前缀：等价 v0.2 的 contains("\marker")，其余标记保持原语义。
// 规则变更纪律：收紧标记必须补误命中反例单测（见 tests 模块）。

/// 分类短路顺序（与 classify_recommendation 的 if 链同步，单测锁定）。
/// 运行时镜像是 classify_recommendation 的 if 链；本表是契约数据源，由单测与文档一致性断言消费。
#[allow(dead_code)]
pub(crate) const CATEGORY_PRIORITY: &[&str] = &[
    "system-managed",
    "low-risk-cache",
    "app-managed",
    "uninstall-or-migrate",
    "user-data",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MatchKind {
    RootExact,
    RootTree,
    SegmentSeq,
    SegmentPrefix,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PathRule {
    pub(crate) kind: MatchKind,
    pub(crate) marker: &'static str,
}

/// 系统托管规则：命中即 blocked，永远不提供清理建议。
pub(crate) const SYSTEM_MANAGED_RULES: &[PathRule] = &[
    PathRule { kind: MatchKind::RootExact, marker: "pagefile.sys" },
    PathRule { kind: MatchKind::RootExact, marker: "swapfile.sys" },
    PathRule { kind: MatchKind::RootExact, marker: "hiberfil.sys" },
    PathRule { kind: MatchKind::RootExact, marker: "recovery" },
    PathRule { kind: MatchKind::RootExact, marker: "$recycle.bin" },
    PathRule { kind: MatchKind::RootTree, marker: "windows" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "windows\\winsxs" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "windows\\installer" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "windows\\system32" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "windows\\servicing" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "system volume information" },
];

/// 四处清单一致性测试用：SYSTEM_MANAGED_RULES 的文档展示名（大小写不敏感包含校验），
/// 顺序与规则表一一对应，README / SKILL / references 三份文档必须全部提及。
/// system-managed 文档展示名清单：与 SYSTEM_MANAGED_RULES 一一对应，由四处清单一致性单测消费。
#[allow(dead_code)]
pub(crate) const SYSTEM_MANAGED_DOC_TOKENS: &[&str] = &[
    "pagefile.sys",
    "swapfile.sys",
    "hiberfil.sys",
    "recovery",
    "$recycle.bin",
    "windows",
    "winsxs",
    "installer",
    "system32",
    "servicing",
    "system volume information",
];

/// 低风险缓存规则。`update` 已收紧为 SegmentSeq：
/// v0.2 的 contains("\update") 会把 update-history、updated 等用户目录误标为可弃缓存
/// （误标缓存是最危险的误命中方向），现仅命中字面名为 update 的目录段。
pub(crate) const LOW_RISK_CACHE_RULES: &[PathRule] = &[
    PathRule { kind: MatchKind::SegmentPrefix, marker: "cache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "code cache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "gpucache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "dxcache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "glcache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "npm-cache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "pip\\cache" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: ".gradle\\caches" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "ms-playwright" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "cachedextensionvsixs" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "autoupdate\\download" },
    PathRule { kind: MatchKind::SegmentSeq, marker: "update" },
];

/// 应用托管规则。`qq` 已收紧为 SegmentSeq：
/// v0.2 的 contains("\qq") 会把 qq-backup、qqmusic-exports 等用户目录误标为应用托管，
/// 现仅命中字面名为 qq 的目录段；品牌数据目录（WeChat Files 等）仍由前缀语义覆盖。
pub(crate) const APP_MANAGED_RULES: &[PathRule] = &[
    PathRule { kind: MatchKind::SegmentPrefix, marker: "tencent" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "wechat" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "wxwork" },
    PathRule { kind: MatchKind::SegmentSeq, marker: "qq" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "kingsoft" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "wps" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "onedrive" },
];

/// 卸载/迁移规则。
pub(crate) const UNINSTALL_OR_MIGRATE_RULES: &[PathRule] = &[
    PathRule { kind: MatchKind::SegmentPrefix, marker: "program files" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "program files (x86)" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "programdata" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "appdata\\local\\programs" },
    PathRule { kind: MatchKind::SegmentPrefix, marker: "appdata\\local\\packages" },
    PathRule { kind: MatchKind::SegmentSeq, marker: "ext4.vhdx" },
];

/// 返回盘符根之后的路径体（`c:\windows` → `windows`）；无 `x:\` 形式则返回 None。
fn strip_drive_root(lower: &str) -> Option<&str> {
    let bytes = lower.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
    {
        Some(&lower[3..])
    } else {
        None
    }
}

fn rule_matches(rule: &PathRule, lower: &str) -> bool {
    // 段边界匹配基底：首尾补 `\`，使段标记不会吞掉段内前缀。
    let padded = format!("\\{}\\", lower.trim_matches('\\'));
    match rule.kind {
        MatchKind::RootExact => strip_drive_root(lower) == Some(rule.marker),
        MatchKind::RootTree => match strip_drive_root(lower) {
            Some(rest) => rest == rule.marker || rest.starts_with(&format!("{}\\", rule.marker)),
            None => false,
        },
        MatchKind::SegmentSeq => padded.contains(&format!("\\{}\\", rule.marker)),
        // 与 v0.2 的 contains("\marker") 语义一致（marker 可为多段序列）。
        MatchKind::SegmentPrefix => padded.contains(&format!("\\{}", rule.marker)),
    }
}

fn matches_any(lower: &str, rules: &[PathRule]) -> bool {
    rules.iter().any(|rule| rule_matches(rule, lower))
}

fn is_system_managed(lower: &str) -> bool {
    matches_any(lower, SYSTEM_MANAGED_RULES)
}

fn is_low_risk_cache(lower: &str) -> bool {
    matches_any(lower, LOW_RISK_CACHE_RULES)
}

fn is_app_managed(lower: &str) -> bool {
    matches_any(lower, APP_MANAGED_RULES)
}

fn is_uninstall_or_migrate(lower: &str) -> bool {
    matches_any(lower, UNINSTALL_OR_MIGRATE_RULES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_managed_recommendation_is_blocked_and_not_cleanable() {
        let recommendation = classify_recommendation("C:\\Windows\\WinSxS", 18.4, "scanner");

        assert_eq!(recommendation.category, "system-managed");
        assert_eq!(recommendation.risk, "blocked");
        assert!(!recommendation.cleanable);
        assert_eq!(
            recommendation.blocked_reason.as_deref(),
            Some("系统托管路径，不提供自动清理。")
        );
    }

    #[test]
    fn root_windows_directory_is_system_managed() {
        let recommendation = classify_recommendation("C:\\Windows", 35.8, "scanner");

        assert_eq!(recommendation.category, "system-managed");
        assert_eq!(recommendation.risk, "blocked");
        assert!(!recommendation.cleanable);
    }

    #[test]
    fn cache_recommendation_stays_manual_until_allowlist_release() {
        let recommendation = classify_recommendation(
            "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache",
            2.1,
            "scanner",
        );

        assert_eq!(recommendation.category, "low-risk-cache");
        assert_eq!(recommendation.risk, "low");
        assert!(!recommendation.cleanable);
        assert_eq!(
            recommendation.blocked_reason.as_deref(),
            Some("内置清理已后移到 v0.3 白名单流程。")
        );
    }

    #[test]
    fn recommendation_sort_prioritizes_specific_review_paths() {
        let mut recommendations = vec![
            classify_recommendation("C:\\Users", 74.0, "scanner"),
            classify_recommendation("C:\\Windows", 35.0, "scanner"),
            classify_recommendation(
                "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache",
                3.2,
                "heuristic",
            ),
            classify_recommendation("C:\\Program Files\\Vendor\\tool.dll", 18.0, "heuristic"),
            classify_recommendation("C:\\Users\\me\\Downloads\\archive.zip", 5.0, "heuristic"),
        ];

        recommendations.sort_by(compare_recommendations_for_review);

        let ordered_paths = recommendations
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered_paths,
            vec![
                "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache",
                "C:\\Users\\me\\Downloads\\archive.zip",
                "C:\\Users",
                "C:\\Program Files\\Vendor\\tool.dll",
                "C:\\Windows",
            ]
        );
    }

    #[test]
    fn drive_root_summary_detection_handles_common_root_forms() {
        assert!(is_drive_root_summary("C:\\"));
        assert!(is_drive_root_summary("C:\\Users"));
        assert!(is_drive_root_summary("D:\\Program Files"));
        assert!(!is_drive_root_summary("C:\\Users\\me\\Downloads"));
        assert!(!is_drive_root_summary("\\\\server\\share"));
    }

    #[test]
    fn system_managed_rules_are_drive_parameterized() {
        // 盘符参数化：同一相对规则对任意盘符生效，不再写死 c:\。
        for path in [
            "D:\\Windows",
            "E:\\pagefile.sys",
            "F:\\Recovery",
            "G:\\data\\windows\\winsxs",
        ] {
            let recommendation = classify_recommendation(path, 1.0, "scanner");
            assert_eq!(recommendation.category, "system-managed", "{path}");
        }
        // 反例：盘根精确条目不在盘根时不命中（与 v0.2 语义一致）。
        let nested = classify_recommendation("C:\\other\\pagefile.sys", 1.0, "scanner");
        assert_ne!(nested.category, "system-managed");
    }

    #[test]
    fn cache_rules_miss_update_like_user_directories() {
        // 反例：`update` 已收紧为完整目录段标记，段内前缀/相近名不得误标缓存。
        // 误标缓存是最危险的误命中方向（用户可能当垃圾清掉）。
        for path in [
            "C:\\Users\\me\\Documents\\update-notes",
            "C:\\data\\updates-archive",
            "C:\\work\\updated-crashdumps",
            "C:\\Users\\me\\Desktop\\update 2024",
            "C:\\projects\\updater-config",
        ] {
            let recommendation = classify_recommendation(path, 1.0, "heuristic");
            assert_ne!(recommendation.category, "low-risk-cache", "{path}");
        }
        // 正例回归：字面名为 update 的目录段仍按缓存候选处理。
        let literal = classify_recommendation(
            "C:\\Users\\me\\AppData\\Local\\SomeApp\\Update",
            1.0,
            "heuristic",
        );
        assert_eq!(literal.category, "low-risk-cache");
    }

    #[test]
    fn app_rules_miss_qq_like_user_directories() {
        // 反例：`qq` 已收紧为完整目录段标记，qq-backup 等用户目录不得误标应用托管。
        for path in [
            "C:\\Users\\me\\Documents\\qq-backup",
            "C:\\data\\qqmusic-exports",
            "C:\\work\\share\\qq2024",
        ] {
            let recommendation = classify_recommendation(path, 1.0, "heuristic");
            assert_ne!(recommendation.category, "app-managed", "{path}");
        }
        // 正例回归：品牌数据目录仍由前缀语义覆盖，字面 qq 段仍命中。
        for path in [
            "C:\\Users\\me\\Documents\\WeChat Files",
            "C:\\Users\\me\\Documents\\Tencent Files",
            "C:\\data\\share\\qq",
        ] {
            let recommendation = classify_recommendation(path, 1.0, "heuristic");
            assert_eq!(recommendation.category, "app-managed", "{path}");
        }
    }

    #[test]
    fn classification_priority_short_circuits_by_table_order() {
        // 显式优先级：同一路径命中多类标记时，按 CATEGORY_PRIORITY 顺序短路。
        assert_eq!(CATEGORY_PRIORITY.len(), 5);
        assert_eq!(CATEGORY_PRIORITY[0], "system-managed");
        // system > cache：update 缓存标记不得越过系统托管。
        let system_first = classify_recommendation("C:\\Windows\\WinSxS\\update", 1.0, "heuristic");
        assert_eq!(system_first.category, "system-managed");
        // cache > app：Tencent 下的缓存目录按缓存处理。
        let cache_before_app = classify_recommendation(
            "C:\\Users\\me\\AppData\\Roaming\\Tencent\\Cache",
            1.0,
            "heuristic",
        );
        assert_eq!(cache_before_app.category, "low-risk-cache");
        // app > uninstall：Program Files 下品牌目录仍按应用托管语义给出复核指引。
        let app_in_program_files =
            classify_recommendation("C:\\Program Files\\Tencent\\WeChat", 1.0, "heuristic");
        assert_eq!(app_in_program_files.category, "app-managed");
        // uninstall > user-data 兜底。
        let uninstall =
            classify_recommendation("C:\\Program Files\\Vendor\\tool", 1.0, "heuristic");
        assert_eq!(uninstall.category, "uninstall-or-migrate");
    }

    #[test]
    fn system_managed_list_is_documented_consistently() {
        // 四处清单一致性（反向校验）：classify.rs 规则表的系统托管条目
        // 必须在 README / SKILL / references 三份文档的清单中出现，防文档漂移。
        assert_eq!(
            SYSTEM_MANAGED_DOC_TOKENS.len(),
            SYSTEM_MANAGED_RULES.len(),
            "文档展示名与规则表必须一一对应"
        );
        let docs = [
            ("README.md", include_str!("../../README.md")),
            ("SKILL.md", include_str!("../../SKILL.md")),
            (
                "references/windows-cleanup-heuristics.md",
                include_str!("../../references/windows-cleanup-heuristics.md"),
            ),
        ];
        for token in SYSTEM_MANAGED_DOC_TOKENS {
            for (doc_name, text) in docs {
                let haystack = text.to_ascii_lowercase();
                assert!(
                    haystack.contains(&token.to_ascii_lowercase()),
                    "{doc_name} 缺少系统托管条目: {token}"
                );
            }
        }
    }
}
