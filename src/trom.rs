//! Transplant the founder into TROM 5.2's dormant Wuwei faction.

use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use crate::{
    caab::{Document, Edit, Node, integer, remap},
    config::{Config, HEPU},
    startpos::{Mode, compression_record, open_world},
};

const MARKER: usize = 60442;
const DORMANT: &str = "3k_main_faction_wuwei";
const SHI_XIE: &str = "3k_main_faction_shi_xie";
pub const REFORMS: &str = "script/campaign/mod/hv_script_reforms.lua";

// Indices in CAMPAIGN_COMMAND_QUEUE_INDEX_TABLES for the supported TROM startpos.
const UNIT_CQI: usize = 4;
const FORCE_CQI: usize = 5;
const PERSISTENT_RETINUE_CQI: usize = 25;
const PERSISTENT_SLOT_CQI: usize = 26;
const DEPLOYED_RETINUE_CQI: usize = 27;
const DEPLOYED_SLOT_CQI: usize = 28;

pub fn packs(workshop: &[PathBuf]) -> Result<[PathBuf; 2]> {
    let find = |part, id, file| {
        workshop
            .iter()
            .find(|path| {
                path.file_name().is_some_and(|name| name == file)
                    && path
                        .parent()
                        .and_then(|folder| folder.file_name())
                        .is_some_and(|name| name == id)
            })
            .cloned()
            .with_context(|| {
                format!("Install TROM Part {part} from Steam Workshop to use TROM compatibility")
            })
    };
    Ok([
        find(1, "1977631962", "@_trom_mtu_hvo.pack")?,
        find(2, "1998760417", "@__trom_mtu_hvo_vfx.pack")?,
    ])
}

pub fn reforms(script: &[u8], key: &str) -> Result<Vec<u8>> {
    let condition = "return q_faction:subculture() == \"3k_main_chinese\"";
    let script = std::str::from_utf8(script)?;
    ensure!(
        script.matches(condition).count() == 2,
        "Unsupported TROM reform script"
    );
    Ok(script
        .replace(
            condition,
            &format!(
                "return q_faction:name() ~= \"{key}\" and {}",
                &condition[7..]
            ),
        )
        .into_bytes())
}

fn reserve(edit: &mut Edit<'_>, counters: &[Node], fields: &[usize], count: usize) -> Result<u32> {
    let next = fields
        .iter()
        .map(|index| edit.doc.id(&counters[*index]))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .context("No CAAB identifier counter")?;
    for field in fields {
        edit.replace_int(&counters[*field], next + u32::try_from(count)?);
    }
    Ok(next)
}

fn equipment(
    edit: &mut Edit<'_>,
    donor: &Document,
    counters: &[Node],
    char_id: u32,
    new_char: u32,
) -> Result<()> {
    let world = edit.doc;
    let managers = "CHARACTERS_CONNECTED_CEO_MANAGEMENT";
    let original = &donor
        .one(&donor.root, managers)?
        .groups
        .iter()
        .find(|group| donor.number(&group[0]) == Some(char_id))
        .context("Missing founder equipment manager")?[1];
    let manager_field = &donor.one(original, "CONNECTED_CEO_MANAGEMENT")?.fields()[0];
    let old_manager_id = donor.id(manager_field)?;
    let new_manager_id = world
        .find(&world.root, "CONNECTED_CEO_MANAGEMENT")
        .into_iter()
        .filter_map(|node| node.fields().first().and_then(|f| world.number(f)))
        .max()
        .context("Missing equipment managers")?
        + 1;
    edit.replace_int(&counters[36], new_manager_id + 1);
    let original_items = donor
        .one(&donor.root, "ALL_OWNED_CEOS")?
        .groups
        .iter()
        .filter(|group| donor.number(&group[1].fields()[1]) == Some(old_manager_id))
        .collect::<Vec<_>>();
    let items = world.one(&world.root, "ALL_OWNED_CEOS")?;
    let first = reserve(edit, counters, &[38], original_items.len())?;
    let mut ids = HashMap::new();
    for (index, group) in original_items.iter().enumerate() {
        let item = &group[1].fields()[0];
        ids.insert(donor.id(item)?, first + index as u32);
    }
    for group in original_items {
        let fields = group[1].fields();
        let changes = HashMap::from([
            (fields[0].start, integer(ids[&donor.id(&fields[0])?])),
            (fields[1].start, integer(new_manager_id)),
        ]);
        let mut bytes = Vec::new();
        for node in group {
            bytes.extend(edit.import(donor, node, &changes)?);
        }
        edit.append(items, bytes);
    }
    let mut changes = remap(donor, original, &ids);
    changes.insert(manager_field.start, integer(new_manager_id));
    let mut bytes = integer(new_char);
    bytes.extend(edit.import(donor, original, &changes)?);
    edit.append(world.one(&world.root, managers)?, bytes);
    Ok(())
}

fn founder(
    edit: &mut Edit<'_>,
    donor: &Document,
    faction: &Node,
    origin: &Node,
    key: &str,
) -> Result<()> {
    let world = edit.doc;
    let character = donor.record("PERSISTENT_CHARACTER", 10, &format!("{key}_leader"))?;
    let char_id = donor.id(&character.fields()[0])?;
    let army = donor.one(origin, "ARMY_ARRAY")?;
    let force = donor.one(army, "MILITARY_FORCE")?;
    let force_id = donor.id(&force.fields()[0])?;
    let units = donor.find(force, "UNIT");
    let retinue = donor.one(force, "MILITARY_FORCE_RETINUE")?.fields();
    let slots = &donor
        .one(character, "CHARACTER_PERSISTENT_RETINUE_SLOTS")?
        .groups;
    let counters = world
        .one(&world.root, "CAMPAIGN_COMMAND_QUEUE_INDEX_TABLES")?
        .fields();
    let new_char = reserve(edit, counters, &[0, 21], 1)?;
    let new_army = reserve(edit, counters, &[FORCE_CQI], 1)?;
    let new_unit = reserve(edit, counters, &[UNIT_CQI], units.len())?;
    let new_retinue = reserve(edit, counters, &[DEPLOYED_RETINUE_CQI], 1)?;
    let new_persistent_retinue = reserve(edit, counters, &[PERSISTENT_RETINUE_CQI], 1)?;
    let new_slot = reserve(edit, counters, &[DEPLOYED_SLOT_CQI], slots.len())?;
    let new_persistent_slot = reserve(edit, counters, &[PERSISTENT_SLOT_CQI], slots.len())?;
    let mut ids = HashMap::from([
        (char_id, new_char),
        (force_id, new_army),
        (donor.id(&retinue[0])?, new_retinue),
        (donor.id(&retinue[1])?, new_persistent_retinue),
    ]);
    for (index, node) in units.iter().enumerate() {
        ids.insert(donor.id(&node.fields()[0])?, new_unit + index as u32);
    }
    let deployed_slots: HashMap<_, _> = donor
        .find(force, "MILITARY_FORCE_RETINUE_SLOT")
        .into_iter()
        .map(|slot| Ok((donor.id(&slot.fields()[0])?, slot)))
        .collect::<Result<_>>()?;
    ensure!(
        deployed_slots.len() == slots.len(),
        "Founder retinue slots differ"
    );
    let mut persistent_slots = HashMap::new();
    let mut force_slots = HashMap::new();
    for (index, group) in slots.iter().enumerate() {
        let fields = group[0].fields();
        let deployed_id = donor.id(&fields[1])?;
        let deployed = deployed_slots
            .get(&deployed_id)
            .context("Missing deployed founder retinue slot")?
            .fields();
        ensure!(
            donor.id(&fields[0])? == donor.id(&deployed[1])?,
            "Founder retinue slot references differ"
        );
        let persistent_id = integer(new_persistent_slot + u32::try_from(index)?);
        persistent_slots.insert(fields[0].start, persistent_id.clone());
        force_slots.insert(deployed[1].start, persistent_id);
        ids.insert(deployed_id, new_slot + u32::try_from(index)?);
    }
    let mut army_changes = remap(donor, army, &ids);
    army_changes.extend(force_slots);
    let imported = edit.import(donor, army, &army_changes)?;
    edit.replace(world.one(faction, "ARMY_ARRAY")?, imported);
    let fields = faction.fields();
    let mut shroud = vec![0x12];
    shroud.extend(edit.import(
        donor,
        donor.one(origin, "CAMPAIGN_SHROUD")?,
        &HashMap::new(),
    )?);
    edit.replace(&fields[15], shroud);
    let mut missions = edit.import(
        donor,
        donor.one(origin, "CAMPAIGN_MISSION_MANAGER")?,
        &HashMap::new(),
    )?;
    missions.extend(world.raw(&fields[7]));
    edit.replace(&fields[7], missions);
    // The dormant faction has an extra string after two zero capital references.
    edit.replace(&fields[18], Vec::new());
    edit.replace(&fields[22], vec![0x13]);
    edit.replace_int(&fields[31], 1);
    edit.replace_int(&fields[60], new_char);

    let old_post = donor.id(&donor.one(character, "ACTIVE_CHARACTER")?.fields()[11])?;
    let post = world
        .find(world.one(faction, "GOVERNMENT")?, "CHARACTER_POST")
        .into_iter()
        .find(|post| world.text(&post.fields()[1]) == Some("faction_leader"))
        .context("Missing faction leader post")?;
    ids.insert(old_post, world.id(&post.fields()[0])?);
    ids.insert(donor.id(&origin.fields()[0])?, world.id(&fields[0])?);
    let mut character_changes = remap(donor, character, &ids);
    character_changes.extend(persistent_slots);
    let imported = edit.import(donor, character, &character_changes)?;
    edit.append(world.one(&world.root, "CHARACTERS")?, imported);

    let family = world.one(&world.root, "FAMILY_TREE")?.fields();
    let member = donor
        .find(&donor.root, "FAMILY_MEMBER")
        .into_iter()
        .find(|node| donor.number(&node.fields()[1]) == Some(char_id))
        .context("Missing founder family record")?;
    edit.replace_int(&family[0], world.id(&family[0])? + 1);
    let last = family
        .iter()
        .rev()
        .find(|node| world.name(node) == "FAMILY_MEMBER")
        .context("Missing family tree")?;
    let mut record = world.raw(last).to_vec();
    record.extend(edit.import(
        donor,
        member,
        &remap(donor, member, &HashMap::from([(char_id, new_char)])),
    )?);
    edit.replace(last, record);
    equipment(edit, donor, counters, char_id, new_char)
}

fn roster(edit: &mut Edit<'_>, donor: &Document, key: &str, in_world: bool) -> Result<()> {
    let doc = edit.doc;
    edit.rename(DORMANT, key, "flags");
    for node in doc.find(&doc.root, "CAMPAIGN_PLAYER_SETUP") {
        let fields = node.fields();
        if doc.text(&fields[1]) == Some(DORMANT) {
            edit.replace(&fields[3], vec![0x12]);
            if in_world && node.start > MARKER {
                edit.replace(&fields[9], vec![0x12]);
            }
        }
    }
    let info = doc.group("FACTION_INFOS", 0, DORMANT)?;
    let source = donor.group("FACTION_INFOS", 0, key)?;
    edit.replace(&info[2], vec![0x12]);
    for index in [3, 4, 5, 6, 9, 11] {
        let bytes = edit.import(donor, &source[index], &HashMap::new())?;
        edit.replace(&info[index], bytes);
    }
    let hepu = doc.group("REGION_OWNERSHIPS", 0, HEPU)?;
    edit.replace_text(&hepu[1], key);
    Ok(())
}

fn transfer_hepu(edit: &mut Edit<'_>, faction: &Node, key: &str, config: &Config) -> Result<()> {
    let doc = edit.doc;
    let hepu = doc.record("REGION", 1, HEPU)?;
    let region = doc.id(&hepu.fields()[0])?;
    edit.replace_int(&faction.fields()[16], region);
    edit.replace_int(&faction.fields()[17], region);
    edit.replace_int(&hepu.fields()[7], doc.id(&faction.fields()[0])?);
    for node in doc.find(hepu, "BUILDING_BASE") {
        edit.replace_text(&node.fields()[4], key);
    }
    let shi = doc.record("FACTION", 1, SHI_XIE)?;
    edit.replace_int(&shi.fields()[32], doc.id(&shi.fields()[32])? - 1);
    if let Some((old, new, _)) = config.settlement().and_then(|site| site.buildings.as_ref()) {
        let target = doc.record("REGION", 1, &config.start_region)?;
        for name in ["BUILDING_BASE", "SETTLEMENT"] {
            for node in doc.find(target, name) {
                for field in node.fields() {
                    if doc.text(field) == Some(old) {
                        edit.replace_text(field, new);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn merge(
    trom: &[u8],
    generated: &[u8],
    mode: Mode,
    key: &str,
    config: &Config,
) -> Result<Vec<u8>> {
    let checksum = match mode {
        Mode::Historical => "b6d293edbf1be2d87f888b84eb97abfe13f667b25e6ac7431084448fc75bec81",
        Mode::Romance => "89076b06fe0fcb7273c2dd4e362cbf0e31efb699b55e561f4b821721274976eb",
    };
    ensure!(
        hex::encode(Sha256::digest(trom)) == checksum,
        "The {} start position differs from the supported TROM 5.2 release",
        mode.key()
    );
    let frontend = Document::read(trom)?;
    let donor_frontend = Document::read(generated)?;
    let world = Document::read(&open_world(&frontend.data, MARKER)?)?;
    let donor = Document::read(&open_world(
        &donor_frontend.data,
        mode.compression_marker(),
    )?)?;
    let mut frontend_edit = Edit::new(&frontend);
    let mut edit = Edit::new(&world);
    roster(&mut frontend_edit, &donor_frontend, key, false)?;
    roster(&mut edit, &donor_frontend, key, true)?;
    let faction = world.record("FACTION", 1, DORMANT)?;
    let origin = donor.record("FACTION", 1, key)?;
    for name in ["FACTION_FLAG_AND_COLOURS", "FACTION_TECHNOLOGY_MANAGER"] {
        let bytes = edit.import(&donor, donor.one(origin, name)?, &HashMap::new())?;
        edit.replace(world.one(faction, name)?, bytes);
    }
    transfer_hepu(&mut edit, faction, key, config)?;
    founder(&mut edit, &donor, faction, origin, key)?;
    let container = frontend.one(&frontend.root, "COMPRESSED_DATA")?;
    let mut bytes = frontend.data[container.start..container.fields()[0].start - 5].to_vec();
    bytes.extend(compression_record(&frontend.data, &edit.finish()?)?);
    frontend_edit.replace(container, bytes);
    frontend_edit.finish()
}
