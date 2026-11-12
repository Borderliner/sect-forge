use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rpfm_lib::files::Container;

use crate::caab::Document;

use super::{Game, Localization, localization, open_pack, raw};

const EFFECT_LOC: &str = "text/db/effects__.loc";
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
        let doc = Document::read(&raw(&self.pack, "campaigns/ceo_data.ccd")?)
            .context("Read the game's trait data")?;
        let effect_loc = localization(&self.pack, EFFECT_LOC)
            .or_else(|_| localization(&open_pack(&self.local_en)?, EFFECT_LOC))
            .unwrap_or_default();
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
        let mut effects = trait_effects(&doc, &effect_loc);
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

fn trait_effects<'a>(doc: &'a Document, labels: &Localization) -> BTreeMap<&'a str, Vec<String>> {
    let mut result = BTreeMap::new();
    for table in doc.find(&doc.root, "EFFECTS_LIST_TABLE_POPULATE") {
        for group in &table.groups {
            let Some(key) = group
                .first()
                .and_then(|field| doc.text(field))
                .filter(|key| key.starts_with(NODE_PREFIX))
            else {
                continue;
            };
            let Some(section) = group.get(1) else {
                continue;
            };
            let mut effects = Vec::new();
            for block in doc.find(section, "EFFECT_INFOS_BLOCK") {
                for row in &block.groups {
                    let [effect, value, scope, ..] = row.as_slice() else {
                        continue;
                    };
                    let (Some(effect), Some(value), Some(scope)) =
                        (doc.text(effect), doc.float(value), doc.text(scope))
                    else {
                        continue;
                    };
                    if let Some(label) = labels
                        .get(&format!("effects_description_{effect}"))
                        .and_then(|label| format_effect(label, value, scope))
                        && !effects.contains(&label)
                    {
                        effects.push(label);
                    }
                }
            }
            result.insert(key, effects);
        }
    }
    result
}

fn format_effect(template: &str, value: f32, scope: &str) -> Option<String> {
    let template = template.trim();
    if template.is_empty() || template.starts_with("[HIDDEN]") {
        return None;
    }
    let mut label = template
        .replace("[[b]]", "")
        .replace("[[/b]]", "")
        .replace("%+n", &format!("{value:+}"))
        .replace("%n", &value.to_string())
        .replace("{{tr:military_supplies}}", "military supplies")
        .replace("{{tr:public_order}}", "public order");
    let context = match scope {
        "character_to_force_own" => "own army",
        "character_to_force_own_when_commanding" => "when commanding",
        "character_to_character_own_retinue" => "own retinue",
        "character_to_character_own_forcewide" => "characters in the same army",
        "character_to_character_regionwide" => "characters in the same region",
        "character_to_character_own_factionwide" | "character_to_faction_unseen" => "faction-wide",
        "character_to_province_own_when_governor"
        | "character_to_region_own_provincewide_when_governor" => "administered commandery",
        "character_to_force_own_provincewide_when_governor" => {
            "friendly armies in the administered commandery"
        }
        "character_to_force_enemy_provincewide_when_governor" => {
            "enemy armies in the administered commandery"
        }
        "character_to_force_regionwide_own" => "friendly armies in the same region",
        "character_to_force_regionwide_enemy" => "enemy armies in the same region",
        "character_minister_to_faction"
        | "character_minister_to_character_factionwide"
        | "character_minister_to_region_factionwide"
        | "character_minister_to_province_factionwide" => "faction-wide as minister",
        _ => "",
    };
    if !context.is_empty() {
        label.push_str(" · ");
        label.push_str(context);
    }
    Some(label)
}

#[cfg(test)]
mod tests {
    use super::format_effect;

    #[test]
    fn trait_effects_expand_values_and_keep_their_campaign_scope() {
        assert_eq!(
            format_effect(
                "[[b]]%+n%[[/b]] income from all sources",
                5.,
                "character_to_region_own_provincewide_when_governor"
            )
            .as_deref(),
            Some("+5% income from all sources · administered commandery")
        );
        assert_eq!(
            format_effect(
                "[[b]]%+n[[/b]] {{tr:public_order}}",
                -2.,
                "character_to_character_own"
            )
            .as_deref(),
            Some("-2 public order")
        );
        assert_eq!(
            format_effect("[HIDDEN]", 5., "character_to_character_own"),
            None
        );
    }
}
