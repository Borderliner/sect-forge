use anyhow::{Context, Result, ensure};

use super::SHI_XIE;
use crate::{
    caab::{Edit, Node},
    config::{HAN, HEPU, Settlement},
};

pub(super) fn place(
    edit: &mut Edit<'_>,
    faction: &Node,
    key: &str,
    site: &Settlement,
) -> Result<()> {
    let doc = edit.doc;
    let region = doc.record("REGION", 1, &site.key)?;
    let fields = region.fields();
    let region_id = doc.id(&fields[0])?;
    let faction_id = doc.id(&faction.fields()[0])?;
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
            doc.id(&owner.fields()[0])? == doc.id(&fields[7])?,
            "Starting settlement owner differs from the campaign roster"
        );
        let count = &owner.fields()[if site.key == HEPU { 32 } else { 30 }];
        let remaining = doc
            .id(count)?
            .checked_sub(1)
            .context("Missing region count")?;
        edit.replace_int(count, remaining);
    }
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
