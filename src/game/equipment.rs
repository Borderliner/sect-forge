use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rpfm_lib::files::pack::Pack;

use crate::{config::ANCILLARIES, unlocks::Sources};

use super::{Game, Row, ceo::ceo_effects, field};

pub type EquipmentCatalog = BTreeMap<String, EquipmentStats>;

pub struct EquipmentStats {
    pub effects: Vec<String>,
    pub battle: Vec<String>,
    pub records: Option<Vec<String>>,
}

impl Game {
    pub fn equipment_stats(&self, trom: Option<&Pack>) -> Result<EquipmentCatalog> {
        let doc = self.ceo_data(trom)?;
        let mut effects = ceo_effects(&doc, &self.effect_labels(), |key| {
            ANCILLARIES.contains_key(key)
        });
        let sources = Sources::new(&self.pack, trom);
        let mut variants = BTreeMap::<String, BTreeMap<String, Row>>::new();
        for row in sources.effective("ceos_to_equipment_variants", &["ceos_key", "game_mode"])? {
            let key = field(&row, "ceos_key");
            if ANCILLARIES.contains_key(key) {
                variants
                    .entry(key.to_owned())
                    .or_default()
                    .insert(field(&row, "game_mode").to_owned(), row);
            }
        }

        let mut tables = BTreeMap::new();
        for name in [
            "melee_weapons",
            "missile_weapons",
            "projectiles",
            "projectiles_explosions",
            "unit_armour_types",
            "unit_shield_types",
            "mounts",
            "battle_entities",
        ] {
            let rows = sources
                .effective(name, &["key"])
                .with_context(|| format!("Read equipment data from {name}"))?;
            tables.insert(
                name,
                rows.into_iter()
                    .map(|row| (field(&row, "key").to_owned(), row))
                    .collect(),
            );
        }

        Ok(ANCILLARIES
            .keys()
            .map(|key| {
                let modes = variants.get(key);
                let stats_for = |mode| {
                    modes
                        .and_then(|modes| modes.get(mode).or_else(|| modes.get("")))
                        .map_or_else(Vec::new, |row| battle_stats(row, &tables))
                };
                let battle = stats_for("romance");
                let records = stats_for("historical");
                let stats = EquipmentStats {
                    effects: effects.remove(key.as_str()).unwrap_or_default(),
                    records: (records != battle).then_some(records),
                    battle,
                };
                (key.clone(), stats)
            })
            .collect())
    }
}

fn battle_stats(variant: &Row, tables: &BTreeMap<&str, BTreeMap<String, Row>>) -> Vec<String> {
    let mut stats = Vec::new();
    let find = |table: &str, row: &Row, key: &str| tables[table].get(field(row, key));
    if let Some(weapon) = find("melee_weapons", variant, "primary_melee_weapon") {
        add_stats(
            &mut stats,
            weapon,
            &[
                ("damage", "Melee damage", ""),
                ("ap_damage", "Armour-piercing melee damage", ""),
                ("melee_attack_interval", "Attack interval", " s"),
            ],
        );
    }
    if let Some(projectile) = find("missile_weapons", variant, "primary_missile_weapon")
        .and_then(|weapon| find("projectiles", weapon, "default_projectile"))
    {
        add_stats(
            &mut stats,
            projectile,
            &[
                ("damage", "Ranged damage", ""),
                ("ap_damage", "Armour-piercing ranged damage", ""),
                ("effective_range", "Range", " m"),
                ("base_reload_time", "Reload time", " s"),
            ],
        );
        if let Some(explosion) = find("projectiles_explosions", projectile, "explosion_type") {
            add_stats(
                &mut stats,
                explosion,
                &[
                    ("detonation_damage", "Explosive damage", ""),
                    (
                        "detonation_damage_ap",
                        "Armour-piercing explosive damage",
                        "",
                    ),
                ],
            );
        }
    }
    if let Some(armour) = find("unit_armour_types", variant, "armour") {
        add_stats(&mut stats, armour, &[("armour_value", "Armour", "")]);
    }
    if let Some(shield) = find("unit_shield_types", variant, "shield") {
        add_stats(
            &mut stats,
            shield,
            &[
                ("shield_defence_value", "Shield defence", ""),
                ("shield_armour_value", "Shield armour", ""),
                ("missile_block_chance", "Missile block chance", "%"),
            ],
        );
    }
    if let Some(entity) =
        find("mounts", variant, "mount").and_then(|mount| find("battle_entities", mount, "entity"))
    {
        add_stats(
            &mut stats,
            entity,
            &[
                ("run_speed", "Mount run speed", " m/s"),
                ("mass", "Mount mass", ""),
            ],
        );
    }
    stats
}

fn add_stats(output: &mut Vec<String>, row: &Row, fields: &[(&str, &str, &str)]) {
    for &(column, label, units) in fields {
        if let Some(value) = row.get(column).and_then(|value| value.as_f64()) {
            let text = format!("{value:.2}");
            let text = text.trim_end_matches('0').trim_end_matches('.');
            output.push(format!("{label} {text}{units}"));
        }
    }
}
