use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rpfm_lib::files::{Container, pack::Pack};

use crate::{caab::Document, config::CLASSES};

use super::{Game, Localization, localization, open_pack, raw};

const CEO_DATA: &str = "campaigns/ceo_data.ccd";
const EFFECT_LOC: &str = "text/db/effects__.loc";

impl Game {
    pub(super) fn ceo_data(&self, overlay: Option<&Pack>) -> Result<Document> {
        let pack = overlay
            .filter(|pack| pack.file(CEO_DATA, false).is_some())
            .unwrap_or(&self.pack);
        Document::read(&raw(pack, CEO_DATA)?)
            .context("Read the game's character and equipment data")
    }

    pub(super) fn effect_labels(&self) -> Localization {
        localization(&self.pack, EFFECT_LOC)
            .or_else(|_| localization(&open_pack(&self.local_en)?, EFFECT_LOC))
            .unwrap_or_default()
    }
}

pub(super) fn ceo_effects<'a>(
    doc: &'a Document,
    labels: &Localization,
    include: impl Fn(&str) -> bool,
) -> BTreeMap<&'a str, Vec<String>> {
    let mut result = BTreeMap::new();
    for table in doc.find(&doc.root, "EFFECTS_LIST_TABLE_POPULATE") {
        for group in &table.groups {
            let Some(key) = group
                .first()
                .and_then(|field| doc.text(field))
                .filter(|key| include(key))
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
    if label.contains("{{tr:3k_general_type_") {
        for (class, name, _, _) in CLASSES {
            let element = class.trim_start_matches("3k_general_");
            label = label.replace(&format!("{{{{tr:3k_general_type_{element}}}}}"), name);
        }
    }
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
