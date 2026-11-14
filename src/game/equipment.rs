use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::{Container, pack::Pack};

use crate::{
    caab::Document,
    config::{LEGACY_EQUIPMENT, SLOTS},
    portraits::words,
    unlocks::Sources,
};

use super::{Game, Row, ceo::ceo_effects, field, localization};

pub type EquipmentCatalog = BTreeMap<String, Equipment>;

pub struct Equipment {
    pub slot: &'static str,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub icon_source: usize,
    pub rarity: String,
    pub stats: EquipmentStats,
    pub character_owned: bool,
    pub permission: Option<String>,
    classes: String,
    pub unavailable: Option<&'static str>,
}

impl Equipment {
    pub fn fits(&self, element: &str) -> bool {
        self.unavailable.is_none()
            && (self.classes.is_empty()
                || self.classes.split_ascii_whitespace().any(|c| c == element))
    }
}

pub struct EquipmentStats {
    pub effects: Vec<String>,
    pub battle: Vec<String>,
    pub records: Option<Vec<String>>,
}

impl Game {
    pub fn equipment(&self, trom: Option<[&Pack; 2]>) -> Result<EquipmentCatalog> {
        let [part_one, part_two] = trom.map_or([None, None], |[a, b]| [Some(a), Some(b)]);
        let doc = self.ceo_data(
            part_two
                .filter(|p| p.file(super::ceo::CEO_DATA, false).is_some())
                .or(part_one),
        )?;
        let mut loc = self.loc.clone();
        let mut labels = self.effect_labels();
        let packs = [Some(self.pack.as_ref()), part_one, part_two];
        for pack in [part_one, part_two].into_iter().flatten() {
            let mut tables = pack
                .files()
                .keys()
                .filter(|p| p.ends_with(".loc"))
                .collect::<Vec<_>>();
            tables.sort_unstable();
            for path in tables {
                for (key, value) in localization(pack, path)? {
                    if key.starts_with("ceo_nodes_") {
                        loc.insert(key, value);
                    } else if key.starts_with("effects_description_") {
                        labels.insert(key, value);
                    }
                }
            }
        }
        let sources = Sources::new(&self.pack, part_one);
        let effects = ceo_effects(&doc, &labels, |_| true);
        let mut variants = BTreeMap::<String, BTreeMap<String, Row>>::new();
        for row in sources.effective("ceos_to_equipment_variants", &["ceos_key", "game_mode"])? {
            let key = field(&row, "ceos_key");
            variants
                .entry(key.to_owned())
                .or_default()
                .insert(field(&row, "game_mode").to_owned(), row);
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

        let ids = doc
            .one(&doc.root, "CEO_TABLE_CREATE")?
            .groups
            .iter()
            .filter_map(|row| Some((doc.text(row.first()?)?, doc.number(row.get(1)?)? as usize)))
            .collect::<BTreeMap<_, _>>();
        let nodes = sections(&doc, "CEO_NODE_TABLE_POPULATE")?;
        let thresholds = sections(&doc, "CEO_THRESHOLD_TABLE_POPULATE")?;
        let permissions = equipment_permissions(&doc)?;
        let mut items = BTreeMap::new();
        for (key, data) in sections(&doc, "CEO_TABLE_POPULATE")? {
            let text = |field| data.get(field).and_then(|n| doc.text(n)).unwrap_or("");
            let category = text(0).strip_prefix("3k_main_ceo_category_ancillary_");
            let Some((slot, _)) = SLOTS.into_iter().find(|(slot, _)| Some(*slot) == category)
            else {
                continue;
            };
            let threshold = thresholds
                .get(text(13))
                .with_context(|| format!("Missing equipment node threshold for {key}"))?;
            let node = threshold
                .get(6)
                .and_then(|block| block.fields().first())
                .and_then(|section| section.fields().first())
                .and_then(|key| doc.text(key))
                .unwrap_or("");
            let presentation = |field| {
                nodes
                    .get(node)
                    .and_then(|row| row.get(field))
                    .and_then(|n| doc.text(n))
                    .unwrap_or("")
            };
            let modes = variants.get(key);
            let variant = |mode| modes.and_then(|modes| modes.get(mode).or_else(|| modes.get("")));
            let battle = variant("romance").map_or_else(Vec::new, |row| battle_stats(row, &tables));
            let records =
                variant("historical").map_or_else(Vec::new, |row| battle_stats(row, &tables));
            let character_owned = data.get(6).and_then(|field| doc.number(field)) == Some(0);
            let id = *ids.get(key).context("Missing equipment ID")?;
            let available_permissions = permissions
                .iter()
                .filter(|p| p.character == character_owned && p.includes(id))
                .map(|p| p.key)
                .collect::<Vec<_>>();
            let permission = character_owned
                .then(|| personal_permission(&available_permissions))
                .flatten()
                .map(str::to_owned);
            let class_access = class_permissions(slot, &available_permissions);
            let dynasties = key.starts_with("ep_dyn_");
            let alias = key.ends_with("_faction")
                && nodes
                    .get(node)
                    .and_then(|r| r.first())
                    .and_then(|n| doc.text(n))
                    .is_some_and(|owner| owner != key);
            let unavailable = if dynasties {
                Some("Dynasty Mode equipment")
            } else if alias {
                Some("Transfer variant. Choose the matching personal item.")
            } else if key == "3k_main_ancillary_accessory_the_emperor" || node.is_empty() {
                Some("Campaign event equipment")
            } else if LEGACY_EQUIPMENT.contains_key(key)
                || (!available_permissions.is_empty()
                    && (!character_owned || permission.is_some() || !class_access.is_empty()))
            {
                None
            } else {
                Some("Requires a different character or campaign setup")
            };
            let classes = LEGACY_EQUIPMENT
                .get(key)
                .map(|item| item.classes.as_str())
                .unwrap_or_else(|| {
                    if !class_access.is_empty() {
                        return class_access.as_str();
                    }
                    if slot != "weapon" {
                        return "";
                    }
                    let weapon = variant("romance")
                        .or_else(|| variant("historical"))
                        .map(|r| field(r, "primary_melee_weapon"))
                        .unwrap_or("");
                    if weapon.contains("staff") || key.contains("ball_mace") {
                        "wood"
                    } else if weapon.contains("_dw")
                        || (weapon.contains("_1h") && weapon.contains("axe"))
                    {
                        "earth metal"
                    } else if weapon.contains("_2h") || weapon.contains("short_ji") {
                        "wood fire"
                    } else if weapon.contains("_1h") {
                        "earth metal water"
                    } else {
                        ""
                    }
                });
            let icon = presentation(4).replace('\\', "/").to_ascii_lowercase();
            let icon = if icon.starts_with("ui/") {
                icon
            } else {
                format!("ui/ancillaries/{icon}")
            };
            let icon_source = packs
                .iter()
                .rposition(|p| p.is_some_and(|p| p.file(&icon, true).is_some()))
                .unwrap_or(0);
            let title = loc
                .get(presentation(2))
                .filter(|s| !s.is_empty())
                .cloned()
                .unwrap_or_else(|| {
                    let name = key
                        .split_once("ancillary_")
                        .or_else(|| key.split_once("ancilliary_"))
                        .map_or(key, |(_, name)| name);
                    words(name.strip_prefix(&format!("{slot}_")).unwrap_or(name))
                });
            let description = loc.get(presentation(3)).cloned().unwrap_or_default();
            items.insert(
                key.to_owned(),
                Equipment {
                    slot,
                    title,
                    description,
                    icon,
                    icon_source,
                    rarity: text(18).into(),
                    stats: EquipmentStats {
                        effects: effects
                            .get(node)
                            .or_else(|| effects.get(key))
                            .cloned()
                            .unwrap_or_default(),
                        records: (records != battle).then_some(records),
                        battle,
                    },
                    character_owned,
                    permission,
                    classes: classes.into(),
                    unavailable,
                },
            );
        }
        ensure!(!items.is_empty(), "The game equipment catalog is empty");
        Ok(items)
    }
}

fn sections<'a>(
    doc: &'a Document,
    table: &str,
) -> Result<BTreeMap<&'a str, &'a [crate::caab::Node]>> {
    Ok(doc
        .one(&doc.root, table)?
        .groups
        .iter()
        .filter_map(|row| Some((doc.text(row.first()?)?, row.get(1)?.fields())))
        .collect())
}

struct EquipmentPermission<'a> {
    key: &'a str,
    character: bool,
    items: &'a [u8],
}

impl EquipmentPermission<'_> {
    fn includes(&self, id: usize) -> bool {
        self.items
            .get(id / 8)
            .is_some_and(|byte| byte & (1 << (id % 8)) != 0)
    }
}

fn personal_permission<'a>(permissions: &[&'a str]) -> Option<&'a str> {
    permissions.iter().copied().find(|key| {
        (key.contains("_character_specific_") && !key.ends_with("_specific_enable"))
            || key.contains("_armour_nanman_")
    })
}

fn class_permissions(slot: &str, permissions: &[&str]) -> String {
    [
        (
            "earth",
            &[
                "axe_dual",
                "axe_one_handed",
                "sword_dual",
                "sword_one_handed",
            ][..],
        ),
        (
            "metal",
            &[
                "mace_dual",
                "axe_dual",
                "axe_one_handed",
                "sword_dual",
                "sword_one_handed",
            ][..],
        ),
        ("water", &["sword_one_handed"][..]),
        (
            "wood",
            &[
                "axe_two_handed",
                "spear_two_handed",
                "spear_two_handed_long",
                "staff_two_handed",
                "mace_two_handed",
            ][..],
        ),
        (
            "fire",
            &[
                "axe_two_handed",
                "spear_two_handed",
                "spear_two_handed_long",
            ][..],
        ),
    ]
    .into_iter()
    .filter(|(element, types)| match slot {
        "armour" => permissions.iter().any(|key| {
            key.strip_prefix("3k_main_ceo_permissions_ancillary_armour_character_")
                .and_then(|key| key.strip_suffix("_enable"))
                == Some(*element)
        }),
        "weapon" => permissions.iter().any(|key| {
            key.split_once("_weapon_character_")
                .and_then(|(_, suffix)| suffix.strip_suffix("_enable"))
                .is_some_and(|kind| types.contains(&kind))
        }),
        _ => false,
    })
    .map(|(element, _)| element)
    .collect::<Vec<_>>()
    .join(" ")
}

fn equipment_permissions(doc: &Document) -> Result<Vec<EquipmentPermission<'_>>> {
    let mut result = Vec::new();
    for (key, data) in sections(doc, "SCRIPTED_PERMISSIONS_TABLE_POPULATE")? {
        // The permission starts with its inventory location: character (0) or faction (1).
        let location = match data.first().and_then(|n| doc.number(n)) {
            Some(location @ (0 | 1)) => location,
            _ => continue,
        };
        let Some(locations) = data.get(2) else {
            continue;
        };
        if let Some(items) = locations
            .groups
            .iter()
            .find(|row| row.first().and_then(|n| doc.number(n)) == Some(location))
            .and_then(|row| row.get(1))
            .and_then(|slot| slot.fields().first())
            .and_then(|bitset| doc.bitset(bitset))
        {
            result.push(EquipmentPermission {
                key,
                character: location == 0,
                items,
            });
        }
    }
    Ok(result)
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
