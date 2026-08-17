pub mod transformers;
pub mod util;

use serde_json::Value;

pub use util::{get_project_id, MigrationResult};

pub const CURRENT_PROJECT_VERSION: u32 = 30;

pub fn detect_version(project: &Value) -> i64 {
    if let Some(version) = project.get("version").and_then(Value::as_i64) {
        return version;
    }
    let has_scenes = project
        .get("scenes")
        .and_then(Value::as_array)
        .is_some_and(|scenes| !scenes.is_empty());
    if has_scenes {
        1
    } else {
        0
    }
}

#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub from_version: i64,
    pub to_version: i64,
    pub applied: Vec<(i64, i64)>,
    pub stopped_reason: Option<String>,
}

fn apply(step: i64, project: Value, now_iso: &str) -> MigrationResult {
    match step {
        0 => transformers::v0_to_v1(project, now_iso),
        1 => transformers::v1_to_v2(project, now_iso),
        2 => transformers::v2_to_v3(project),
        3 => transformers::v3_to_v4(project),
        4 => transformers::v4_to_v5(project),
        5 => transformers::v5_to_v6(project),
        6 => transformers::v6_to_v7(project),
        7 => transformers::v7_to_v8(project),
        8 => transformers::v8_to_v9(project),
        9 => transformers::v9_to_v10(project),
        10 => transformers::v10_to_v11(project),
        11 => transformers::v11_to_v12(project),
        12 => transformers::v12_to_v13(project),
        13 => transformers::v13_to_v14(project),
        14 => transformers::v14_to_v15(project),
        15 => transformers::v15_to_v16(project),
        16 => transformers::v16_to_v17(project),
        17 => transformers::v17_to_v18(project),
        18 => transformers::v18_to_v19(project),
        19 => transformers::v19_to_v20(project),
        20 => transformers::v20_to_v21(project),
        21 => transformers::v21_to_v22(project),
        22 => transformers::v22_to_v23(project),
        23 => transformers::v23_to_v24(project),
        24 => transformers::v24_to_v25(project),
        25 => transformers::v25_to_v26(project),
        26 => transformers::v26_to_v27(project),
        27 => transformers::v27_to_v28(project),
        28 => transformers::v28_to_v29(project),
        29 => transformers::v29_to_v30(project),
        _ => MigrationResult::skipped(project, "no migration for this version"),
    }
}

pub fn migrate_to_current(project: Value, now_iso: &str) -> (Value, MigrationReport) {
    let mut current = detect_version(&project);
    let target = i64::from(CURRENT_PROJECT_VERSION);
    let mut report = MigrationReport {
        from_version: current,
        to_version: current,
        ..MigrationReport::default()
    };

    let mut document = project;
    while current < target {
        let result = apply(current, document, now_iso);
        document = result.project;
        if result.skipped {
            report.stopped_reason = result.reason;
            break;
        }
        report.applied.push((current, current + 1));
        current += 1;
        report.to_version = current;
    }

    (document, report)
}
