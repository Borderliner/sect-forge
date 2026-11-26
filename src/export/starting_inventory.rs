use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};

use crate::{
    caab::{Edit, integer},
    config::Config,
    game::EquipmentCatalog,
    trom,
};

/// Scripted awards respect world limits, including copies held by historical factions.
/// Reserve loose copies for the new court only when ordinary awards would hit that limit.
pub(super) fn prepare(
    bytes: Vec<u8>,
    config: &Config,
    items: &EquipmentCatalog,
) -> Result<Vec<u8>> {
    let mut requested = BTreeMap::new();
    for character in
        std::iter::once(&config.character).chain(config.officers.iter().map(|o| &o.character))
    {
        for key in character.starting_ancillaries.values() {
            let item = &items[key];
            if item.world_limit.is_some() {
                *requested.entry(key.as_str()).or_insert(0u32) += 1;
            }
        }
    }
    if requested.is_empty() {
        return Ok(bytes);
    }
    let (frontend, world) = trom::campaign_documents(&bytes)?;
    let faction = world.record("FACTION", 1, &config.key())?;
    let faction_id = world.id(&faction.fields()[0])?;
    let managers = world.one(&world.root, "FACTIONS_CONNECTED_CEO_MANAGEMENT")?;
    let manager = &managers
        .groups
        .iter()
        .find(|r| world.number(&r[0]) == Some(faction_id))
        .context("Missing starting faction inventory")?[1];
    let manager_id = world.id(&world.one(manager, "CONNECTED_CEO_MANAGEMENT")?.fields()[0])?;
    let owned = world.one(&world.root, "ALL_OWNED_CEOS")?;
    let mut existing = BTreeMap::<&str, (u32, u32)>::new();
    for r in &owned.groups {
        let Some(key) = world.text(&r[0]).filter(|k| requested.contains_key(k)) else {
            continue;
        };
        let fields = r[1].fields();
        let count = existing.entry(key).or_default();
        count.0 += 1;
        if world.number(&fields[1]) == Some(manager_id) && world.number(&fields[5]) == Some(0) {
            count.1 += 1;
        }
    }
    let mut reserve = Vec::new();
    for (key, count) in requested {
        let (world_count, available) = existing.get(key).copied().unwrap_or_default();
        let missing = count.saturating_sub(available);
        if missing != 0 && world_count + missing > items[key].world_limit.unwrap() {
            reserve.push((key, missing));
        }
    }
    if reserve.is_empty() {
        return Ok(bytes);
    }

    // A loose ancillary has no equipment-slot reference. Clone its record layout from
    // this campaign so Romance, Records and overhauls retain their own ESF definitions.
    let sample = owned
        .groups
        .iter()
        .find(|r| {
            r.len() == 3
                && world.text(&r[0]).is_some_and(|k| k.contains("_ancillary_"))
                && world.name(&r[2]) == "THRESHOLD_CEO"
                && r[1].fields().len() == 7
                && world.number(&r[1].fields()[5]) == Some(0)
        })
        .context("Missing loose ancillary record")?;
    let counter = &world
        .one(&world.root, "CAMPAIGN_COMMAND_QUEUE_INDEX_TABLES")?
        .fields()[trom::CEO_CQI];
    let mut next = world.id(counter)?;
    ensure!(
        owned
            .groups
            .iter()
            .all(|r| world.number(&r[1].fields()[0]).is_some_and(|id| id < next)),
        "Invalid starting equipment counter"
    );
    let pools = world.one(manager, "CEO_POOL_BLOCK")?;
    let list_layout = world
        .find(manager, "CEOS_BLOCK")
        .into_iter()
        .next()
        .context("Missing inventory list layout")?;
    let mut edit = Edit::new(&world);
    for (key, count) in reserve {
        let item = &items[key];
        ensure!(
            !item.initial_node.is_empty(),
            "Missing initial equipment node for {key}"
        );
        let category = format!("3k_main_ceo_category_ancillary_{}", item.slot);
        let pool = &pools
            .groups
            .iter()
            .find(|r| world.text(&r[0]) == Some(category.as_str()))
            .with_context(|| format!("Missing starting inventory for {}", item.slot))?[1];
        let map = world.one(pool, "CEOS_MAP_BLOCK")?;
        let mut ids = Vec::new();
        for _ in 0..count {
            let record = sample[1].fields();
            let overrides = HashMap::from([
                (record[0].start, integer(next)),
                (record[1].start, integer(manager_id)),
                (
                    sample[2].fields()[0].start,
                    edit.text_reference(&item.initial_node),
                ),
                (sample[2].fields()[1].start, integer(1)),
            ]);
            let mut row = edit.text_reference(key);
            row.extend(edit.import(&world, &sample[1], &overrides)?);
            row.extend(edit.import(&world, &sample[2], &overrides)?);
            edit.append(owned, row);
            ids.push(integer(next));
            next = next
                .checked_add(1)
                .context("Equipment identifier overflow")?;
        }
        if let Some(r) = map.groups.iter().find(|r| world.text(&r[0]) == Some(key)) {
            for id in ids {
                edit.append(&r[1], id);
            }
        } else {
            let mut row = edit.text_reference(key);
            row.extend(edit.array_like(list_layout, &ids)?);
            edit.append(map, row);
        }
    }
    edit.replace_int(counter, next);
    trom::finish_campaign(Edit::new(&frontend), edit)
}
