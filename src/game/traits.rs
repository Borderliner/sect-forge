use std::collections::BTreeMap;

use anyhow::Result;
use rpfm_lib::files::Container;

use super::{Game, ceo::ceo_effects};

const NODE_PREFIX: &str = "3k_main_ceo_node_trait_";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TraitCategory {
    Resolve,
    Cunning,
    Instinct,
    Authority,
    Expertise,
    Good,
    Bad,
}

impl TraitCategory {
    fn from_icon(icon: &str) -> Option<Self> {
        match icon.rsplit_once('_')?.1 {
            "wood.png" => Some(Self::Resolve),
            "water.png" => Some(Self::Cunning),
            "fire.png" => Some(Self::Instinct),
            "earth.png" => Some(Self::Authority),
            "metal.png" => Some(Self::Expertise),
            "good.png" => Some(Self::Good),
            "bad.png" => Some(Self::Bad),
            _ => None,
        }
    }
}

pub struct Trait {
    pub key: String,
    pub title: String,
    pub description: String,
    pub physical: bool,
    pub icon: Option<String>,
    pub category: Option<TraitCategory>,
    pub effects: Vec<String>,
}

impl Game {
    pub fn traits(&self) -> Result<Vec<Trait>> {
        let doc = self.ceo_data(None)?;
        let mut icons = BTreeMap::new();
        for node in doc.find(&doc.root, "CEO_NODE_DATA_SECTION") {
            for group in &node.groups {
                if let (Some(key), Some(path)) = (
                    group.first().and_then(|field| doc.text(field)),
                    group.get(4).and_then(|field| doc.text(field)),
                ) && key.starts_with(NODE_PREFIX)
                {
                    icons.insert(key, path.to_ascii_lowercase());
                }
            }
        }
        let mut effects = ceo_effects(&doc, &self.effect_labels(), |key| {
            key.starts_with(NODE_PREFIX)
        });
        let mut traits = Vec::new();
        for (loc_key, title) in &self.loc {
            let Some(node) = loc_key.strip_prefix("ceo_nodes_title_") else {
                continue;
            };
            let Some(kind) = node
                .strip_prefix(NODE_PREFIX)
                .and_then(|key| key.strip_suffix("_01"))
            else {
                continue;
            };
            if title.is_empty()
                || !(kind.starts_with("personality_") || kind.starts_with("physical_"))
            {
                continue;
            }
            let icon = icons.get(node);
            traits.push(Trait {
                key: format!("3k_main_ceo_trait_{kind}"),
                title: title.clone(),
                description: self
                    .loc
                    .get(&format!("ceo_nodes_description_{node}"))
                    .cloned()
                    .unwrap_or_default(),
                physical: kind.starts_with("physical_"),
                icon: icon
                    .filter(|path| self.pack.file(path, true).is_some())
                    .cloned(),
                category: icon.and_then(|path| TraitCategory::from_icon(path)),
                effects: effects.remove(node).unwrap_or_default(),
            });
        }
        traits.sort_by_cached_key(|item| item.title.to_lowercase());
        Ok(traits)
    }
}
