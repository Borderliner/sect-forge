use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::pack::Pack;

use crate::{
    caab::Document,
    config::{CAMPAIGN, HAN, Settlement},
    game::{self, raw},
    startpos::{Mode, place_frontend_marker},
    trom,
};

const REGION_PACK: (&str, &str) = ("3240150507", "!!190_expanded_region.pack");
const CHARACTER_PACK: (&str, &str) = ("2875547086", "!!190_expanded_with_characters.pack");
const REBELS: &str = "3k_dlc04_faction_rebels";

pub(crate) fn packs(workshop: &[PathBuf]) -> Result<[PathBuf; 2]> {
    let find = |(id, file), title| {
        workshop
            .iter()
            .find(|path| path.ends_with(std::path::Path::new(id).join(file)))
            .cloned()
            .with_context(|| format!("Install 190 Expanded {title} from Steam Workshop."))
    };
    Ok([
        find(REGION_PACK, "Extra Regions")?,
        find(CHARACTER_PACK, "Factions & Characters")?,
    ])
}

fn path(mode: Mode) -> String {
    format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key())
}

fn ownership(world: &Document) -> Result<HashMap<&str, &str>> {
    world
        .one(&world.root, "REGION_OWNERSHIPS")?
        .groups
        .iter()
        .map(|row| {
            let [key, owner] = row.as_slice() else {
                anyhow::bail!("Unexpected 190 Expanded ownership record");
            };
            Ok((
                world.text(key).context("Missing region key")?,
                world.text(owner).context("Missing region owner")?,
            ))
        })
        .collect()
}

pub(crate) struct CampaignMap {
    pub sites: Vec<Settlement>,
    pub faction_positions: HashMap<String, (f32, f32)>,
}

pub(crate) fn catalog(pack: &Pack) -> Result<CampaignMap> {
    let (frontend, romance) = trom::campaign_documents(&raw(pack, &path(Mode::Romance))?)?;
    let (_, records) = trom::campaign_documents(&raw(pack, &path(Mode::Historical))?)?;
    let romance_owners = ownership(&romance)?;
    let records_owners = ownership(&records)?;
    let map_data = "campaign_maps/3k_dlc07_main_map/map_data.esf";
    let map = Document::read(&raw(pack, map_data)?)?;
    let names = game::localization(pack, "text/db/regions.loc")?;
    let region_positions: HashMap<_, _> = map
        .find(&map.root, "REGION_DATA")
        .into_iter()
        .filter_map(|entry| Some((map.text(entry.fields().get(1)?)?, entry)))
        .collect();
    let han = romance.record("FACTION", 1, HAN)?;
    let army = han
        .fields()
        .iter()
        .position(|node| romance.name(node) == "ARMY_ARRAY")
        .context("Missing Han Empire army record")?;
    let han_capital = romance.id(&han.fields()[army + 4])?;
    let mut sites = Vec::new();
    for region in romance.find(&romance.root, "REGION") {
        let fields = region.fields();
        let Some(key) = fields.get(1).and_then(|node| romance.text(node)) else {
            continue;
        };
        let Some(&owner) = romance_owners.get(key) else {
            continue;
        };
        if !matches!(owner, HAN | "rebels" | REBELS)
            || records_owners.get(key) != Some(&owner)
            || romance.id(&fields[0])? == han_capital && owner == HAN
        {
            continue;
        }
        let Ok(town) = romance.one(region, "SETTLEMENT") else {
            continue;
        };
        let Some(entry) = region_positions.get(key) else {
            continue;
        };
        let position = map.one(entry, "SETTLEMENT_INFO")?.fields();
        let x = u16::try_from(map.id(&position[1])?)?;
        let y = u16::try_from(map.id(&position[2])?)?;
        if x >= 892 || y >= 703 {
            continue;
        }
        let Some(name) = names.get(&format!("regions_onscreen_{key}")) else {
            continue;
        };
        let buildings = if romance.raw(&fields[19]) == [0x12] {
            let old = romance
                .text(&town.fields()[9])
                .context("Missing settlement building")?;
            let Some(stem) = old.strip_suffix("_0") else {
                continue;
            };
            Some((old.to_owned(), format!("{stem}_1"), [0, 0]))
        } else {
            None
        };
        sites.push(Settlement {
            key: key.into(),
            name: name.clone(),
            x,
            y,
            county: key.contains("_resource_"),
            han: owner == HAN,
            buildings,
            owner: Some(owner.into()),
        });
    }
    sites.sort_by(|a, b| (&a.name, &a.key).cmp(&(&b.name, &b.key)));
    ensure!(
        !sites.is_empty(),
        "No eligible 190 Expanded starting settlements were found."
    );
    let faction_positions = frontend
        .one(&frontend.root, "FACTION_INFOS")?
        .groups
        .iter()
        .filter_map(|row| {
            let key = frontend.text(row.first()?)?;
            let [0x4c, 8, xa, xb, xc, xd, ya, yb, yc, yd] = frontend.raw(row.get(11)?) else {
                return None;
            };
            let x = f32::from_le_bytes([*xa, *xb, *xc, *xd]);
            let y = f32::from_le_bytes([*ya, *yb, *yc, *yd]);
            (x > 0. && x <= 1. && y > 0. && y <= 1.).then(|| (key.to_owned(), (x, y)))
        })
        .collect();
    Ok(CampaignMap {
        sites,
        faction_positions,
    })
}

pub(crate) fn merge(
    pack: &Pack,
    generated: &[u8],
    mode: Mode,
    key: &str,
    site: &Settlement,
) -> Result<Vec<u8>> {
    let donor = place_frontend_marker(generated.to_vec(), key, site)?;
    trom::transplant_campaign(&raw(pack, &path(mode))?, &donor, key, site)
}
