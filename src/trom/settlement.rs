use std::collections::HashMap;

use anyhow::{Context, Result, ensure};

use super::{SHI_XIE, reserve};
use crate::{
    caab::{Document, Edit, Node, integer},
    config::{HAN, HEPU, Settlement},
    startpos::{size_field, sized},
};

const PROVINCE_MANAGER_CQI: usize = 18;
const MERCENARY_POOL_CQI: usize = 33;
const PROVINCE_MANAGERS: &str = "FACTION_PROVINCE_MANAGER_ARRAY";

fn regions<'a>(doc: &'a Document, manager: &Node) -> Result<&'a [u8]> {
    let bytes = doc.raw(&manager.fields()[2]);
    let (length, start) = sized(bytes, 1)?;
    ensure!(
        bytes[0] == 0x56 && bytes.len() == start + length,
        "Invalid province region list"
    );
    Ok(&bytes[start..])
}

fn region_list(regions: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = vec![0x56];
    bytes.extend(size_field(regions.len(), 1)?);
    bytes.extend(regions);
    Ok(bytes)
}

fn capital<'a>(doc: &Document, manager: &'a Node) -> &'a Node {
    let fields = manager.fields();
    &fields[10 + usize::from(doc.raw(&fields[4]) == [0x12])]
}

pub(super) fn place_province(
    edit: &mut Edit<'_>,
    donor: &Document,
    donor_faction: u32,
    region_id: u32,
    previous_owner: u32,
    faction_id: u32,
) -> Result<()> {
    let world = edit.doc;
    let region = u8::try_from(region_id)?;
    let mut target = None;
    for array in world.find(&world.root, PROVINCE_MANAGERS) {
        for group in &array.groups {
            let manager = &group[0];
            if regions(world, manager)?.contains(&region) {
                ensure!(
                    target.is_none(),
                    "Duplicate province ownership for region {region}"
                );
                target = Some((array, manager));
            }
        }
    }
    let (array, original) = target.context("Missing starting settlement's province manager")?;
    ensure!(
        world.id(&original.fields()[1])? == previous_owner,
        "Starting settlement's province manager has a different owner"
    );
    let hepu = &donor.group("PROVINCE_ARRAY", 0, "3k_main_province_hepu")?[1];
    let template = donor
        .one(hepu, PROVINCE_MANAGERS)?
        .groups
        .iter()
        .map(|group| &group[0])
        .find(|manager| donor.number(&manager.fields()[1]) == Some(donor_faction))
        .context("Missing founder's province manager in the Hepu template")?;

    let remaining: Vec<_> = regions(world, original)?
        .iter()
        .copied()
        .filter(|id| *id != region)
        .collect();
    let (manager_id, pool_id) = if let Some(&other_region) = remaining.last() {
        let counters = world
            .one(&world.root, "CAMPAIGN_COMMAND_QUEUE_INDEX_TABLES")?
            .fields();
        edit.replace(&original.fields()[2], region_list(&remaining)?);
        let original_capital = capital(world, original);
        if world.id(original_capital)? == region_id {
            edit.replace_int(original_capital, u32::from(other_region));
        }
        (
            reserve(edit, counters, &[PROVINCE_MANAGER_CQI], 1)?,
            reserve(edit, counters, &[MERCENARY_POOL_CQI], 1)?,
        )
    } else {
        (
            world.id(&original.fields()[0])?,
            world.id(&world.one(original, "MERCENARY_RETINUE_POOL")?.fields()[0])?,
        )
    };
    let manager = edit.import(
        donor,
        template,
        &HashMap::from([
            (template.fields()[0].start, integer(manager_id)),
            (template.fields()[1].start, integer(faction_id)),
            (template.fields()[2].start, region_list(&[region])?),
            (
                donor.one(template, "RECRUITMENT_MANAGER")?.fields()[0].start,
                integer(manager_id),
            ),
            (capital(donor, template).start, integer(region_id)),
            (
                donor.one(template, "MERCENARY_RETINUE_POOL")?.fields()[0].start,
                integer(pool_id),
            ),
        ]),
    )?;
    if remaining.is_empty() {
        edit.replace(original, manager);
    } else {
        edit.append(array, manager);
    }
    Ok(())
}

pub(super) fn place(
    edit: &mut Edit<'_>,
    faction: &Node,
    donor: &Document,
    origin: &Node,
    key: &str,
    site: &Settlement,
) -> Result<()> {
    let doc = edit.doc;
    let region = doc.record("REGION", 1, &site.key)?;
    let fields = region.fields();
    let region_id = doc.id(&fields[0])?;
    let faction_id = doc.id(&faction.fields()[0])?;
    let previous_owner_id = doc.id(&fields[7])?;
    let previous_owner = if site.key == HEPU {
        SHI_XIE
    } else if site.han {
        HAN
    } else {
        "rebels"
    };
    let roster = doc.group("REGION_OWNERSHIPS", 0, &site.key)?;
    ensure!(
        doc.text(&roster[1]) == Some(previous_owner),
        "{} has a different owner in TROM's 190 CE campaign",
        site.name
    );
    if previous_owner != "rebels" {
        let owner = doc.record("FACTION", 1, previous_owner)?;
        ensure!(
            doc.id(&owner.fields()[0])? == previous_owner_id,
            "Starting settlement owner differs from the campaign roster"
        );
        let count = &owner.fields()[if site.key == HEPU { 32 } else { 30 }];
        let remaining = doc
            .id(count)?
            .checked_sub(1)
            .context("Missing region count")?;
        edit.replace_int(count, remaining);
    }
    place_province(
        edit,
        donor,
        donor.id(&origin.fields()[0])?,
        region_id,
        previous_owner_id,
        faction_id,
    )?;
    edit.replace_int(&faction.fields()[16], region_id);
    edit.replace_int(&faction.fields()[17], region_id);
    edit.replace_int(&fields[7], faction_id);

    let mut converted = 0;
    for building in doc.find(region, "BUILDING_BASE") {
        let fields = building.fields();
        edit.replace_text(&fields[4], key);
        if let Some((old, new, _)) = &site.buildings
            && doc.text(&fields[1]) == Some(old)
        {
            edit.replace_text(&fields[1], new);
            converted += 1;
        }
    }
    if let Some((old, new, _)) = &site.buildings {
        let town = doc.one(region, "SETTLEMENT")?.fields();
        ensure!(
            converted == 1 && doc.text(&town[9]) == Some(old) && doc.raw(&fields[19]) == [0x12],
            "Unexpected abandoned settlement layout for {}",
            site.name
        );
        edit.replace_text(&town[9], new);
        edit.replace(&fields[19], vec![0x13]);
        edit.replace(&town[3], vec![0x1a, 3]);
        for slot in doc.find(region, "REGION_SLOT") {
            if doc.raw(&slot.fields()[13]) == [0x19] {
                edit.replace(&slot.fields()[13], vec![0x1a, 3]);
            }
        }
    }
    Ok(())
}
