use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::{Container, pack::Pack};

use crate::{
    config::{BASE_KEY, Config, Variant},
    doctrines,
    export::row,
    game::{Localization, Row, add_rows, field, localization, rows, rows_at, set},
};

pub struct Sources<'a> {
    game: &'a Pack,
    trom: Option<&'a Pack>,
}

impl<'a> Sources<'a> {
    pub fn new(game: &'a Pack, trom: Option<&'a Pack>) -> Self {
        Self { game, trom }
    }

    pub(crate) fn effective(&self, table: &str, keys: &[&str]) -> Result<Vec<Row>> {
        let prefix = format!("db/{table}_tables/");
        let paths = |pack: &Pack| {
            let mut paths = pack
                .files()
                .keys()
                .filter(|path| path.starts_with(&prefix))
                .cloned()
                .collect::<Vec<_>>();
            paths.sort();
            paths
        };
        let mut result = BTreeMap::new();
        let mut insert = |pack: &Pack, path: &str| -> Result<()> {
            for record in rows_at(pack, path)? {
                let id = keys
                    .iter()
                    .map(|key| record.get(*key).map(ToString::to_string))
                    .collect::<Vec<_>>();
                result.insert(id, record);
            }
            Ok(())
        };
        for path in paths(self.game) {
            // A DB file at the same virtual path replaces the retail file in TROM.
            if self
                .trom
                .is_none_or(|trom| trom.file(&path, false).is_none())
            {
                insert(self.game, &path)?;
            }
        }
        if let Some(trom) = self.trom {
            let mut overrides = paths(trom);
            // Apply replacement base files before TROM's separately named patches.
            overrides.sort_by_key(|path| !path.ends_with("/data__"));
            for path in overrides {
                insert(trom, &path)?;
            }
        }
        Ok(result.into_values().collect())
    }
}

fn tech(key: &str, variant: Variant, branch: usize, index: usize) -> String {
    format!("{key}_{}", doctrines::tree(variant)[branch][index].slug)
}

fn han_tier(technology: &str) -> Option<usize> {
    let key = technology.strip_prefix("3k_main_tech_")?;
    let (_, tier) = key.split_once("_tier")?;
    let tier = tier.as_bytes().first()?.checked_sub(b'0')?;
    (tier <= 3).then_some(tier.into())
}

fn recruitment_tech(key: &str, variant: Variant, gate: &str, unit: &str) -> Option<String> {
    Some(unit_reform(key, variant, han_tier(gate)?, unit))
}

fn unit_reform(key: &str, variant: Variant, tier: usize, unit: &str) -> String {
    let mounted = unit.contains("cavalry") || unit.contains("horse") || unit.contains("_lancer");
    let ranged = unit.contains("archer") || unit.contains("crossbow") || unit.contains("trebuchet");
    let index = match (tier, mounted, ranged) {
        (0 | 1, true, _) => 2,
        (0 | 1, _, true) => 4,
        (0, _, _) => 1,
        (1, _, _) => 3,
        (2, true, _) => 6,
        (2, _, true) => 8,
        (2, _, _) => 5,
        (_, true, _) => 9,
        (_, _, true) => 11,
        _ => 10,
    };
    tech(key, variant, 1, index)
}

struct BanditRecruit {
    unit: &'static str,
    cap: &'static str,
    reform: usize,
}

const VANILLA_BANDITS: [BanditRecruit; 3] = [
    BanditRecruit {
        unit: "3k_main_unit_water_archers",
        cap: "3k_dlc05_effect_unit_cap_water_archers",
        reform: 1,
    },
    BanditRecruit {
        unit: "3k_main_unit_metal_sabre_infantry",
        cap: "3k_dlc05_effect_unit_cap_metal_sabre_infantry",
        reform: 3,
    },
    BanditRecruit {
        unit: "3k_main_unit_wood_black_mountain_outlaws",
        cap: "3k_dlc05_effect_unit_cap_wood_black_mountain_outlaws",
        reform: 5,
    },
];

const TROM_BANDITS: [BanditRecruit; 3] = [
    BanditRecruit {
        unit: "hv_unit_bandits_heroic_outlaws",
        cap: "hv_effect_unit_cap_bandits_heroic_outlaws",
        reform: 1,
    },
    BanditRecruit {
        unit: "hv_unit_bandits_elite_blood_seekers",
        cap: "hv_effect_unit_cap_bandits_elite_blood_seekers",
        reform: 5,
    },
    BanditRecruit {
        unit: "hv_unit_bandits_elite_cunning_monkeys",
        cap: "hv_effect_unit_cap_bandits_elite_cunning_monkeys",
        reform: 7,
    },
];

pub fn write_recruitment(
    source: &Sources<'_>,
    pack: &mut Pack,
    template: &Pack,
    config: &Config,
    key: &str,
) -> Result<Option<String>> {
    let donor = if config.variant == Variant::Cult {
        source
            .effective("factions", &["key"])?
            .into_iter()
            .find(|row| field(row, "key") == "3k_main_faction_zhang_yan")
    } else {
        rows(template, "factions")?
            .into_iter()
            .find(|row| field(row, "key") == BASE_KEY)
    }
    .context("Missing recruitment donor faction")?;
    let donor_group = field(&donor, "campaign_unit_permission_group");
    let memberships = source.effective(
        "campaign_unit_permission_group_to_permission_junctions",
        &["permission", "permission_group"],
    )?;
    let permissions = memberships
        .iter()
        .filter(|row| field(row, "permission_group") == donor_group)
        .map(|row| field(row, "permission"))
        .collect::<BTreeSet<_>>();
    ensure!(
        !permissions.is_empty(),
        "No recruitment permissions for {donor_group}"
    );
    let requirements = source.effective("campaign_unit_requirements", &["key"])?;
    let by_key = requirements
        .iter()
        .map(|row| (field(row, "key"), row))
        .collect::<BTreeMap<_, _>>();
    let links = source.effective(
        "campaign_unit_permission_requirements",
        &["permission", "unit", "requirement", "is_restriction"],
    )?;
    let mut replacements = BTreeMap::new();
    let mut alternatives = BTreeMap::new();
    let mut caps = BTreeMap::new();
    let mut changed = BTreeSet::new();
    for link in &links {
        let permission = field(link, "permission");
        if !permissions.contains(permission) || link["is_restriction"].as_bool() == Some(true) {
            continue;
        }
        let requirement = field(link, "requirement");
        let Some(original) = by_key.get(requirement) else {
            continue;
        };
        let unit = field(link, "unit");
        let mut record = (*original).clone();
        for column in ["tech1", "tech2"] {
            if let Some(new) = recruitment_tech(key, config.variant, field(original, column), unit)
            {
                set(&mut record, column, new);
            }
        }
        if &record != *original {
            let id = format!("{key}_{requirement}_{unit}");
            set(&mut record, "key", id.as_str());
            replacements.insert((permission, unit, requirement), (id, record));
            changed.insert(permission);
            continue;
        }
        let mut alternative = (*original).clone();
        if config.variant == Variant::Sect
            && config.sect_mode
            && permission == "hv_permission_elite_basic"
        {
            let tier = match field(original, "ceo") {
                "hv_ceo_faction_rank_unlock_advanced" => 2,
                "hv_ceo_faction_rank_unlock_elite" => 3,
                _ => continue,
            };
            set(&mut alternative, "ceo", "");
            set(
                &mut alternative,
                "tech1",
                unit_reform(key, config.variant, tier, unit),
            );
        } else if config.variant == Variant::Cult
            && matches!(
                permission,
                "3k_dlc05_unit_permission_bandit_network" | "hv_permission_bandits_basic"
            )
            && field(original, "tech1").contains("tech_bandit_network_")
            && field(original, "tech2").is_empty()
        {
            let recruits = if config.trom_compat {
                &TROM_BANDITS
            } else {
                &VANILLA_BANDITS
            };
            let Some(recruit) = recruits.iter().find(|recruit| recruit.unit == unit) else {
                continue;
            };
            let reform = tech(key, config.variant, 1, recruit.reform);
            set(&mut alternative, "tech1", reform.as_str());
            caps.insert(recruit.cap, reform);
        } else {
            continue;
        }
        let id = format!("{key}_alternative_{requirement}_{unit}");
        set(&mut alternative, "key", id.as_str());
        let mut alternative_link = link.clone();
        set(
            &mut alternative_link,
            "permission",
            format!("{key}_{permission}"),
        );
        set(&mut alternative_link, "requirement", id.as_str());
        alternatives.insert((permission, unit, id), (alternative, alternative_link));
        changed.insert(permission);
    }
    if changed.is_empty() {
        return Ok(None);
    }

    let group = format!("{key}_recruitment");
    let permission_key = |permission: &str| format!("{key}_{permission}");
    let group_links = permissions
        .iter()
        .map(|&permission| {
            let permission = if changed.contains(permission) {
                permission_key(permission)
            } else {
                permission.into()
            };
            row! {"permission_group": group, "permission": permission}
        })
        .collect();
    let permission_rows = changed
        .iter()
        .map(|permission| row! {"key": permission_key(permission)})
        .collect();
    let mut permission_links = Vec::new();
    for original in &links {
        let permission = field(original, "permission");
        if !changed.contains(permission) {
            continue;
        }
        let replacement = replacements.get(&(
            permission,
            field(original, "unit"),
            field(original, "requirement"),
        ));
        let new_permission = permission_key(permission);
        let mut link = original.clone();
        if let Some((id, _)) = replacement
            && link["is_restriction"].as_bool() != Some(true)
        {
            set(&mut link, "requirement", id.as_str());
        }
        set(&mut link, "permission", new_permission);
        permission_links.push(link);
    }
    // Multiple permission rows can reference the same unit and requirement.
    let mut records = replacements
        .into_values()
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect::<Vec<_>>();
    for (record, link) in alternatives.into_values() {
        records.push(record);
        permission_links.push(link);
    }
    let cap_effects = caps.into_iter().map(|(effect, technology)| {
        row! {"technology": technology, "effect": effect, "effect_scope": "faction_to_faction_own_unseen", "value": 1}
    }).collect();
    for (table, rows) in [
        ("campaign_unit_permission_groups", vec![row! {"key": group}]),
        ("campaign_unit_permissions", permission_rows),
        (
            "campaign_unit_permission_group_to_permission_junctions",
            group_links,
        ),
        ("campaign_unit_permission_requirements", permission_links),
        ("campaign_unit_requirements", records),
        ("technology_effects_junction", cap_effects),
    ] {
        add_rows(pack, table, &group, rows)?;
    }
    Ok(Some(group))
}

fn building_tech(key: &str, variant: Variant, chain: &str, level: i64) -> String {
    let industry = chain.contains("artisan") || chain.starts_with("3k_resource_metal");
    let military = chain.contains("_military_") || chain.contains("gang_recruitment");
    let commerce = chain.contains("_market_") || chain.starts_with("3k_resource_water");
    let district = chain.starts_with("3k_district_");
    let index = match (level, industry || military || commerce) {
        (0..=1, false) => 1,
        (0..=1, true) => 2,
        (2, false) => 3,
        (2, true) => 4,
        (_, false) => 5,
        (_, true) => 6,
    };
    let branch = if military {
        1
    } else if variant == Variant::Cult || commerce || (district && chain.contains("logistics")) {
        2
    } else {
        0
    };
    tech(key, variant, branch, index)
}

fn write_supply(
    source: &Sources<'_>,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
    originals: BTreeMap<String, Row>,
) -> Result<()> {
    if originals.is_empty() {
        return Ok(());
    }
    let suffix = format!("{key}_supply");
    let supplied = |original: &str| format!("{key}_supply_{original}");
    let mut levels = Vec::new();
    let mut gates = Vec::new();
    for (name, original) in &originals {
        let id = supplied(name);
        let mut level = original.clone();
        set(&mut level, "level_name", id.as_str());
        for field in ["create_cost", "cost_to_construct_instantly"] {
            let cost = original[field].as_i64().unwrap_or_default();
            // Outside contractors are a permanent paid construction alternative.
            set(&mut level, field, cost + (cost / 4).max(250));
        }
        if !field(original, "resource_requirement").is_empty() {
            set(&mut level, "resource_requirement", "");
        }
        let tier = original["level"].as_i64().unwrap_or_default();
        let reform = building_tech(key, config.variant, field(original, "chain"), tier);
        gates.push(row! {"building_level_key": id, "technology_key": reform});
        levels.push(level);
    }
    add_rows(pack, "building_levels", &suffix, levels)?;
    add_rows(
        pack,
        "building_level_required_technology_junctions",
        &suffix,
        gates,
    )?;

    let upgrades = source.effective("building_upgrades_junction", &["from", "to"])?;
    let downgrades = source.effective("building_downgrade_junctions", &["from"])?;
    let mut supplied_upgrades = Vec::new();
    let mut supplied_downgrades = Vec::new();
    for row in upgrades {
        let from = field(&row, "from");
        let to = field(&row, "to");
        if originals.contains_key(to) {
            let to = supplied(to);
            supplied_upgrades.push(row! {"from": from, "to": to});
            if originals.contains_key(from) {
                supplied_upgrades.push(row! {"from": supplied(from), "to": to});
            }
        }
        if originals.contains_key(from) {
            supplied_upgrades.push(row! {"from": supplied(from), "to": to});
        }
    }
    for row in downgrades {
        let from = field(&row, "from");
        let to = field(&row, "to");
        if originals.contains_key(from) {
            supplied_downgrades.push(row! {"from": supplied(from), "to": to});
        }
    }
    add_rows(
        pack,
        "building_upgrades_junction",
        &suffix,
        supplied_upgrades,
    )?;
    add_rows(
        pack,
        "building_downgrade_junctions",
        &suffix,
        supplied_downgrades,
    )?;

    for (table, column, keys) in [
        (
            "building_level_ui_positions",
            "building_level",
            &["building_level"][..],
        ),
        (
            "building_set_to_building_junctions",
            "building_level",
            &["building_chain", "building_level", "building_set"],
        ),
        (
            "building_levels_campaign_building_slot_groups",
            "building_level",
            &["building_level", "group_key"],
        ),
        (
            "building_level_garrison_retinues",
            "building_level",
            &["building_level", "retinue_chain", "subculture"],
        ),
        (
            "faction_resources_building_levels_permissions_junctions",
            "building_level_record",
            &["building_level_record", "faction_resource_id_flag"],
        ),
    ] {
        let records = source
            .effective(table, keys)?
            .into_iter()
            .filter_map(|mut row| {
                let original = field(&row, column);
                if !originals.contains_key(original) {
                    return None;
                }
                let id = supplied(original);
                set(&mut row, column, id);
                Some(row)
            })
            .collect();
        add_rows(pack, table, &suffix, records)?;
    }

    let table = "building_culture_variants";
    let keys = ["building", "culture", "subculture", "faction"];
    let strings = "text/db/building_culture_variants__.loc";
    let names = localization(source.game, strings).unwrap_or_default();
    let records = source
        .effective(table, &keys)?
        .into_iter()
        .filter_map(|mut row| {
            let original = field(&row, "building");
            if !originals.contains_key(original) || !field(&row, "faction").is_empty() {
                return None;
            }
            let culture = field(&row, "culture");
            let subculture = field(&row, "subculture");
            let id = supplied(original);
            let donor = format!("building_culture_variants_name_{original}{culture}{subculture}");
            let label = names.get(&donor).cloned().unwrap_or_else(|| {
                original
                    .trim_start_matches("3k_district_")
                    .trim_start_matches("3k_resource_")
                    .split('_')
                    .filter(|part| !part.is_empty())
                    .map(|part| {
                        let mut chars = part.chars();
                        format!("{}{}", chars.next().unwrap().to_uppercase(), chars.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            loc.insert(
                format!("building_culture_variants_name_{id}{culture}{subculture}"),
                format!("{label} (Contracted)"),
            );
            set(&mut row, "building", id);
            Some(row)
        })
        .collect();
    add_rows(pack, table, &suffix, records)?;

    let members = source.effective("campaign_group_members", &["id"])?;
    let criteria = source.effective(
        "campaign_group_member_criteria_building_levels",
        &["member"],
    )?;
    let mut member_ids = BTreeMap::new();
    let mut building_criteria = Vec::new();
    for mut criterion in criteria {
        let original = field(&criterion, "building_level");
        if !originals.contains_key(original) {
            continue;
        }
        let id = supplied(original);
        let member = field(&criterion, "member").to_owned();
        let new_member = member_ids
            .entry(member.clone())
            .or_insert_with(|| format!("{key}_supply_{member}"));
        set(&mut criterion, "building_level", id);
        set(&mut criterion, "member", new_member.as_str());
        building_criteria.push(criterion);
    }
    let new_members = members
        .into_iter()
        .filter_map(|mut row| {
            let new = member_ids.get(field(&row, "id"))?;
            set(&mut row, "id", new.as_str());
            Some(row)
        })
        .collect();
    add_rows(pack, "campaign_group_members", &suffix, new_members)?;
    add_rows(
        pack,
        "campaign_group_member_criteria_building_levels",
        &suffix,
        building_criteria,
    )?;
    for (table, keys) in [
        (
            "campaign_group_member_criteria_factions",
            &["member", "context"][..],
        ),
        (
            "campaign_group_member_criteria_subcultures",
            &["member", "context"],
        ),
        ("campaign_group_member_criteria_campaigns", &["member"]),
        (
            "campaign_group_member_criteria_faction_resources",
            &["member"],
        ),
    ] {
        let records = source
            .effective(table, keys)?
            .into_iter()
            .filter_map(|mut row| {
                let new = member_ids.get(field(&row, "member"))?;
                set(&mut row, "member", new.as_str());
                Some(row)
            })
            .collect();
        add_rows(pack, table, &suffix, records)?;
    }
    Ok(())
}

pub fn write_buildings(
    source: &Sources<'_>,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
) -> Result<()> {
    let suffix = format!("{key}_building_reforms");
    let sets: &[&str] = if config.variant == Variant::Cult {
        &["3k_dlc05_bandit"]
    } else {
        &["3k_main", "3k_main_default", "3k_main_han_chinese_shared"]
    };
    let chains = source
        .effective(
            "building_chain_availability_sets",
            &["id", "building_chain"],
        )?
        .into_iter()
        .filter(|row| sets.contains(&field(row, "id")))
        .map(|row| field(&row, "building_chain").to_owned())
        .collect::<BTreeSet<_>>();
    let tech_gates = source.effective(
        "building_level_required_technology_junctions",
        &["building_level_key", "technology_key"],
    )?;
    let levels_with_tech = tech_gates
        .iter()
        .filter(|row| {
            let gate = field(row, "technology_key");
            han_tier(gate).is_some()
        })
        .map(|row| field(row, "building_level_key"))
        .collect::<BTreeSet<_>>();
    let resources = source.effective(
        "faction_resources_building_levels_requirements_junctions",
        &["building_level_record"],
    )?;
    let resource_levels = resources
        .iter()
        .map(|row| field(row, "building_level_record"))
        .collect::<BTreeSet<_>>();
    let mut gates = Vec::new();
    let mut contracted = BTreeMap::new();
    for building in source.effective("building_levels", &["level_name"])? {
        let level = field(&building, "level_name");
        let chain = field(&building, "chain");
        if !chains.contains(chain)
            || building["faction_unique"].as_bool() == Some(true)
            || !field(&building, "religion_requirement").is_empty()
            || (!chain.starts_with("3k_district_") && !chain.starts_with("3k_resource_"))
        {
            continue;
        }
        if levels_with_tech.contains(level) {
            let tier = building["level"].as_i64().unwrap_or_default();
            gates.push(row! {"building_level_key": level, "technology_key": building_tech(key, config.variant, chain, tier)});
        }
        // Contracted levels attach to normal upgrade paths and inherit settlement-tier and slot rules.
        if config.sect_mode
            && (resource_levels.contains(level)
                || !field(&building, "resource_requirement").is_empty())
        {
            contracted.insert(level.to_owned(), building);
        }
    }
    add_rows(
        pack,
        "building_level_required_technology_junctions",
        &suffix,
        gates,
    )?;
    write_supply(source, pack, loc, config, key, contracted)
}
