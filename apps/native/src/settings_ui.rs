use std::collections::HashSet;

use cutix_project::model::{Attribution, Project, ProjectSettings};
use watermark::{create_default_watermark, TWatermark};

pub fn read_watermark(settings: &ProjectSettings) -> TWatermark {
    settings
        .watermark
        .as_ref()
        .and_then(|value| serde_json::from_value::<TWatermark>(value.clone()).ok())
        .unwrap_or_else(create_default_watermark)
}

pub fn write_watermark(settings: &mut ProjectSettings, watermark: &TWatermark) {
    settings.watermark = serde_json::to_value(watermark).ok();
}

pub fn collect_attributions(project: &Project) -> Vec<Attribution> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Attribution> = Vec::new();
    for entry in project.attributions.iter().flatten() {
        if entry.title.trim().is_empty()
            && entry.creator.trim().is_empty()
            && entry.license.trim().is_empty()
        {
            continue;
        }
        if seen.insert(entry.id.clone()) {
            out.push(entry.clone());
        }
    }
    out
}

pub fn attribution_credit_line(entry: &Attribution) -> String {
    match &entry.provider {
        Some(provider) if !provider.is_empty() && !entry.license.is_empty() => {
            format!("{} · {}", entry.license, provider)
        }
        Some(provider) if !provider.is_empty() => provider.clone(),
        _ => entry.license.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::Attribution;

    fn sound_attribution(id: &str, title: &str, creator: &str, license: &str) -> Attribution {
        Attribution {
            id: id.to_string(),
            title: title.to_string(),
            creator: creator.to_string(),
            license: license.to_string(),
            license_url: Some(format!(
                "https://creativecommons.org/licenses/{}/4.0/",
                license.to_lowercase().replace("cc ", "").replace(' ', "-")
            )),
            source_url: Some(format!("https://freesound.org/s/{id}/")),
            provider: Some("Freesound".to_string()),
            added_at: format!("2026-01-0{id}T00:00:00Z"),
        }
    }

    fn project_with(attributions: Option<Vec<Attribution>>) -> Project {
        let mut project = Project::new("test", "2026-01-01T00:00:00.000Z".to_owned());
        project.attributions = attributions;
        project
    }

    #[test]
    fn lists_exactly_the_two_cc_sounds_with_their_author_and_licence() {
        let one = sound_attribution("1", "Rainfall Loop", "fieldrec", "CC BY 4.0");
        let two = sound_attribution("2", "Analog Pad", "synthwave", "CC BY-SA 4.0");
        let project = project_with(Some(vec![one.clone(), two.clone()]));

        let collected = collect_attributions(&project);
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].title, "Rainfall Loop");
        assert_eq!(collected[0].creator, "fieldrec");
        assert_eq!(collected[0].license, "CC BY 4.0");
        assert_eq!(collected[1].title, "Analog Pad");
        assert_eq!(collected[1].creator, "synthwave");
        assert_eq!(collected[1].license, "CC BY-SA 4.0");
    }

    #[test]
    fn de_duplicates_the_same_asset_added_twice() {
        let one = sound_attribution("1", "Rainfall Loop", "fieldrec", "CC BY 4.0");
        let project = project_with(Some(vec![one.clone(), one.clone()]));
        assert_eq!(collect_attributions(&project).len(), 1);
    }

    #[test]
    fn a_project_without_credited_assets_lists_nothing() {
        assert!(collect_attributions(&project_with(None)).is_empty());
    }

    #[test]
    fn watermark_round_trips_through_the_settings_slot() {
        let mut mark = create_default_watermark();
        mark.enabled = true;
        mark.size = 0.42;
        mark.anchor = watermark::WatermarkAnchor::TopLeft;

        let mut project = Project::new("test", "2026-01-01T00:00:00.000Z".to_owned());
        write_watermark(&mut project.settings, &mark);
        assert_eq!(read_watermark(&project.settings), mark);
    }
}
