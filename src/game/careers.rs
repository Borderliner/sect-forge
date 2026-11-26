use std::collections::HashMap;

use anyhow::Result;
use rpfm_lib::files::{Container, pack::Pack};

use super::{Game, ceo::ceo_effects, localization};

pub struct Career {
    pub key: String,
    pub title: String,
    pub description: String,
    pub effects: Vec<String>,
}

impl Game {
    pub(crate) fn expanded_careers(
        &self,
        regions: &Pack,
        characters: &Pack,
    ) -> Result<Vec<Career>> {
        let source = [characters, regions];
        self.careers(&source)
    }

    pub(crate) fn careers(&self, overlays: &[&Pack]) -> Result<Vec<Career>> {
        let overlay = overlays
            .iter()
            .find(|pack| pack.file(super::ceo::CEO_DATA, false).is_some());
        let doc = self.ceo_data(overlay.copied())?;
        let mut loc = self.loc.clone();
        let mut effects_loc = self.effect_labels();
        for pack in overlays.iter().rev() {
            for path in pack.files().keys().filter(|path| path.ends_with(".loc")) {
                let Ok(entries) = localization(pack, path) else {
                    continue;
                };
                for (key, value) in entries {
                    if key.starts_with("ceo_nodes_") {
                        loc.insert(key, value);
                    } else if key.starts_with("effects_description_") {
                        effects_loc.insert(key, value);
                    }
                }
            }
        }
        let groups = |name| -> Result<HashMap<_, _>> {
            Ok(doc
                .one(&doc.root, name)?
                .groups
                .iter()
                .filter_map(|group| Some((doc.text(group.first()?)?, group.get(1)?)))
                .collect())
        };
        let thresholds = groups("CEO_THRESHOLD_TABLE_POPULATE")?;
        let nodes = groups("CEO_NODE_TABLE_POPULATE")?;
        let effects = ceo_effects(&doc, &effects_loc, |_| true);
        let mut careers = Vec::new();
        for (key, record) in groups("CEO_TABLE_POPULATE")? {
            let fields = record.fields();
            if fields.first().and_then(|field| doc.text(field))
                != Some("3k_main_ceo_category_career")
            {
                continue;
            }
            let Some(threshold) = fields
                .get(13)
                .and_then(|field| doc.text(field))
                .and_then(|key| thresholds.get(key))
            else {
                continue;
            };
            let Some(node_key) = doc
                .find(threshold, "CEO_THRESHOLD_NODE_SECTION")
                .first()
                .and_then(|section| section.fields().first())
                .and_then(|value| doc.text(value))
            else {
                continue;
            };
            let Some(node) = nodes.get(node_key) else {
                continue;
            };
            let fields = node.fields();
            let Some(title) = fields
                .get(2)
                .and_then(|field| doc.text(field))
                .and_then(|key| loc.get(key))
                .filter(|title| !title.trim().is_empty())
            else {
                continue;
            };
            let description = fields
                .get(3)
                .and_then(|field| doc.text(field))
                .and_then(|key| loc.get(key))
                .cloned()
                .unwrap_or_default();
            let effect_key = fields
                .first()
                .and_then(|field| doc.text(field))
                .unwrap_or(node_key);
            careers.push(Career {
                key: key.into(),
                title: title.clone(),
                description,
                effects: effects.get(effect_key).cloned().unwrap_or_default(),
            });
        }
        careers.sort_by_cached_key(|career| (career.title.to_lowercase(), career.key.clone()));
        Ok(careers)
    }
}
