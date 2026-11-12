use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::pack::Pack;

use crate::{
    config::{CAMPAIGN, Config, Variant},
    export::row,
    game::{Localization, Row, add_rows, field, record_id, rows_at, set},
    unlocks::Sources,
};

struct Chain {
    slug: &'static str,
    donor: &'static str,
    description: &'static str,
    tiers: &'static [&'static str],
}

const SECT_HALL: Chain = Chain {
    slug: "sect_hall",
    donor: "3k_district_government_religion",
    description: "A permanent gathering place for the sect. Each tier improves public order and peasantry income in the commandery.",
    tiers: &["Meeting Hall", "Hall of Vows", "Seat of the Sect"],
};
const CULT_MARKET: Chain = Chain {
    slug: "cult_market",
    donor: "3k_district_residential_bandits",
    description: "Fences buy plunder and put the proceeds back into the warbands. Each tier earns banditry income and increases local supply stockpiles.",
    tiers: &["Fences' Den", "Night Market", "Outlaw Exchange"],
};
const SECT_SHRINE: Chain = Chain {
    slug: "sect_shrine",
    donor: "hv_subtown_han_small_farm",
    description: "A minor-settlement attachment where villagers gather for rites and share provisions. Improves commandery public order and local food production.",
    tiers: &["Wayside Shrine", "Village Chapter"],
};
const CULT_STASH: Chain = Chain {
    slug: "cult_stash",
    donor: "hv_subtown_han_small_farm",
    description: "A minor-settlement attachment where outriders hide provisions and stolen goods. Adds banditry income and supply stockpiles.",
    tiers: &["Hidden Stash", "Smugglers' Cache"],
};

pub fn write(
    source: &Sources<'_>,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
) -> Result<()> {
    let (city, county) = match config.variant {
        Variant::Sect => (&SECT_HALL, &SECT_SHRINE),
        Variant::Cult => (&CULT_MARKET, &CULT_STASH),
    };
    let suffix = format!("{key}_founder_buildings");
    add_rows(
        pack,
        "building_chain_availability_set_ids",
        &suffix,
        vec![row! {"id": suffix}],
    )?;
    add_rows(
        pack,
        "building_chain_availabilities",
        &suffix,
        vec![row! {
            "set_id": suffix, "faction": key, "id": record_id(&suffix), "campaign": CAMPAIGN
        }],
    )?;
    let chains = std::iter::once((city, false)).chain(config.trom_compat.then_some((county, true)));
    for (chain, attachment) in chains {
        write_chain(
            source,
            pack,
            loc,
            chain,
            config.variant,
            key,
            &suffix,
            attachment,
        )?;
    }
    Ok(())
}

pub fn verify_availability(pack: &Pack) -> Result<()> {
    let records = |table| -> Result<Vec<Row>> {
        let prefix = format!("db/{table}_tables/");
        let mut records = Vec::new();
        for path in pack.files().keys().filter(|path| path.starts_with(&prefix)) {
            records.extend(rows_at(pack, path)?);
        }
        Ok(records)
    };
    let sets = records("building_chain_availability_set_ids")?;
    let registered: BTreeSet<_> = sets.iter().map(|row| field(row, "id")).collect();
    for row in records("building_chain_availability_sets")? {
        let id = field(&row, "id");
        ensure!(
            registered.contains(id),
            "Unregistered building availability set {id}"
        );
    }
    Ok(())
}

fn write_chain(
    source: &Sources<'_>,
    pack: &mut Pack,
    loc: &mut Localization,
    chain: &Chain,
    variant: Variant,
    key: &str,
    availability: &str,
    attachment: bool,
) -> Result<()> {
    let id = format!("{key}_{}", chain.slug);
    let mut chain_row = source
        .effective("building_chains", &["key"])?
        .into_iter()
        .find(|row| field(row, "key") == chain.donor)
        .with_context(|| format!("Missing building chain {}", chain.donor))?;
    let donor_superchain = field(&chain_row, "building_superchain");
    let mut superchain = source
        .effective("building_superchains", &["key"])?
        .into_iter()
        .find(|row| field(row, "key") == donor_superchain)
        .with_context(|| format!("Missing superchain for {}", chain.donor))?;
    let slot_table = "slot_template_to_building_superchain_junctions";
    let slot_templates = source.effective(slot_table, &["id"])?
        .into_iter().filter(|row| field(row, "building_superchain") == donor_superchain)
        .map(|row| {
            let template = field(&row, "slot_template");
            row! {"building_superchain": id, "slot_template": template, "id": record_id(&format!("{id}/{template}"))}
        }).collect::<Vec<_>>();
    anyhow::ensure!(
        !slot_templates.is_empty(),
        "No slot templates for {}",
        chain.donor
    );
    add_rows(pack, slot_table, &id, slot_templates)?;
    set(&mut chain_row, "key", id.as_str());
    set(&mut chain_row, "building_superchain", id.as_str());
    set(&mut superchain, "key", id.as_str());
    for field in ["chain_tooltip", "encyclopedia_name"] {
        loc.insert(
            format!("building_chains_{field}_{id}"),
            chain.tiers[0].into(),
        );
    }
    add_rows(pack, "building_chains", &id, vec![chain_row])?;
    add_rows(pack, "building_superchains", &id, vec![superchain])?;
    add_rows(
        pack,
        "building_chain_availability_sets",
        &id,
        vec![row! {"id": availability, "building_chain": id}],
    )?;

    let sets = source.effective(
        "building_set_to_building_junctions",
        &["building_chain", "building_level", "building_set"],
    )?;
    let sets = sets
        .into_iter()
        .filter(|row| {
            field(row, "building_chain") == chain.donor
                && field(row, "building_level").is_empty()
                && field(row, "building_set") != chain.donor
        })
        .map(|mut row| {
            set(&mut row, "building_chain", id.as_str());
            row
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(
        !sets.is_empty(),
        "No construction slots for {}",
        chain.donor
    );
    add_rows(pack, "building_set_to_building_junctions", &id, sets)?;

    let mut donors = source
        .effective("building_levels", &["level_name"])?
        .into_iter()
        .filter(|row| field(row, "chain") == chain.donor)
        .collect::<Vec<_>>();
    donors.sort_by_key(|row| row["level"].as_i64());
    let variants = source.effective(
        "building_culture_variants",
        &["building", "culture", "subculture", "faction"],
    )?;
    let prefabs = source.effective(
        "building_levels_campaign_building_slot_groups",
        &["building_level", "group_key"],
    )?;
    let mut levels = Vec::new();
    let mut artwork = Vec::new();
    let mut layout = Vec::new();
    let mut upgrade = Vec::new();
    let mut downgrade = Vec::new();
    let mut slots = Vec::new();
    let mut groups = Vec::new();
    let mut members = Vec::new();
    let mut criteria = Vec::new();
    let mut effects = Vec::new();
    anyhow::ensure!(
        donors.len() >= chain.tiers.len(),
        "Missing levels for {}",
        chain.donor
    );
    for (index, (mut level, title)) in donors.into_iter().zip(chain.tiers).enumerate() {
        let tier = index as i32 + 1;
        let level_id = format!("{id}_{tier}");
        let original = field(&level, "level_name");
        let mut icon = variants
            .iter()
            .find(|row| {
                field(row, "building") == original
                    && ["culture", "subculture", "faction"]
                        .iter()
                        .all(|name| field(row, name).is_empty())
            })
            .cloned()
            .with_context(|| format!("Missing building icon for {original}"))?;
        for prefab in prefabs
            .iter()
            .filter(|row| field(row, "building_level") == original)
        {
            let mut prefab = prefab.clone();
            set(&mut prefab, "building_level", level_id.as_str());
            slots.push(prefab);
        }
        set(&mut icon, "building", level_id.as_str());
        set(&mut icon, "description", level_id.as_str());
        set(&mut icon, "short_description", level_id.as_str());
        set(&mut level, "level_name", level_id.as_str());
        set(&mut level, "chain", id.as_str());
        set(&mut level, "faction_unique", true);
        set(&mut level, "can_convert", false);
        set(
            &mut level,
            "primary_slot_building_building_level_requirement",
            if attachment { tier } else { tier * 2 - 1 },
        );
        let cost = if attachment { 450 * tier } else { 850 * tier };
        set(&mut level, "create_cost", cost);
        set(&mut level, "cost_to_construct_instantly", cost);
        set(&mut level, "upkeep_cost", 15 * tier);
        loc.extend([
            (
                format!("building_culture_variants_name_{level_id}"),
                (*title).into(),
            ),
            (
                format!("building_short_description_texts_short_description_{level_id}"),
                chain.description.into(),
            ),
            (
                format!("building_description_texts_long_description_{level_id}"),
                chain.description.into(),
            ),
        ]);
        levels.push(level);
        artwork.push(icon);
        layout.push(row! {"building_level": level_id, "pos_x": 0, "pos_y": 5 - tier});
        if index > 0 {
            let previous = format!("{id}_{index}");
            upgrade.push(row! {"from": previous, "to": level_id});
            downgrade.push(row! {"from": level_id, "to": previous});
        }
        let member = format!("{level_id}_member");
        groups.push(row! {"id": level_id});
        members.push(row! {"id": member, "group": level_id, "priority": 1.0});
        criteria.push(row! {"member": member, "building_level": level_id});
        effects.extend(level_effects(&level_id, variant, attachment, tier));
    }
    for (table, rows) in [
        ("building_levels", levels),
        ("building_culture_variants", artwork),
        ("building_level_ui_positions", layout),
        ("building_upgrades_junction", upgrade),
        ("building_downgrade_junctions", downgrade),
        ("building_levels_campaign_building_slot_groups", slots),
        ("campaign_groups", groups),
        ("campaign_group_members", members),
        ("campaign_group_member_criteria_building_levels", criteria),
        ("building_effects_junction", effects),
    ] {
        add_rows(pack, table, &id, rows)?;
    }
    Ok(())
}

fn level_effects(group: &str, variant: Variant, attachment: bool, tier: i32) -> [Row; 2] {
    let order = "3k_main_effect_province_public_order_buildings";
    let peasantry = "3k_main_effect_economy_gdp_mod_peasantry";
    let food = "3k_main_effect_province_food_abs_farming";
    let banditry = "3k_dlc05_effect_economy_gdp_abs_banditry";
    let supplies =
        "3k_main_effect_province_supply_stockpile_abs_increase_from_residential_infrastructure";
    let province = "province_to_province_own_unseen";
    let local = "region_to_region_own_provincewide_unseen";
    let building = "building_to_building_own";
    match (variant, attachment) {
        (Variant::Sect, false) => [(order, province, tier * 2), (peasantry, local, tier * 8)],
        (Variant::Sect, true) => [
            (order, province, tier),
            (food, "region_to_region_own_unseen_province_pool_view_hidden", tier),
        ],
        (Variant::Cult, false) => [(banditry, building, tier * 70), (supplies, "region_to_province_own_unseen", tier * 3)],
        (Variant::Cult, true) => [(banditry, building, tier * 25), (supplies, "region_to_province_own_unseen", tier)],
    }.map(|(effect, scope, value)| row! {"campaign_group": group, "effect": effect, "effect_scope": scope,
        "value": value, "value_damaged": value / 2, "value_ruined": 0, "value_resource_permission_lost": 0})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::new_pack;

    #[test]
    fn building_availability_requires_a_registered_set() -> Result<()> {
        let mut pack = new_pack("fixture.pack");
        add_rows(
            &mut pack,
            "building_chain_availability_sets",
            "fixture",
            vec![row! {"id": "custom_set", "building_chain": "custom_hall"}],
        )?;
        let error = verify_availability(&pack).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Unregistered building availability set custom_set")
        );
        add_rows(
            &mut pack,
            "building_chain_availability_set_ids",
            "fixture",
            vec![row! {"id": "custom_set"}],
        )?;
        verify_availability(&pack)
    }
}
