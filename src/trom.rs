//! Transplant the founder into TROM 5.2's dormant Wuwei faction.

use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

use crate::{
    caab::{Document, Edit, Node, integer, remap},
    config::{Config, HEPU, Settlement},
    startpos::{Mode, compression_record, map_position, open_world},
};

mod settlement;
mod supplies;

pub use supplies::write_tooltips as write_supplies_tooltips;

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
const CEO_MANAGER_CQI: usize = 36;
const CEO_CQI: usize = 38;
const CEO_EQUIPMENT_SLOT_CQI: usize = 39;
const VANILLA_STARTING_INFANTRY: &str = "3k_main_unit_wood_ji_militia";
const TROM_STARTING_INFANTRY: &str = "hv_unit_militia_ji_infantry";

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
    edit.replace_int(&counters[CEO_MANAGER_CQI], new_manager_id + 1);
    let original_items = donor
        .one(&donor.root, "ALL_OWNED_CEOS")?
        .groups
        .iter()
        .filter(|group| donor.number(&group[1].fields()[1]) == Some(old_manager_id))
        .collect::<Vec<_>>();
    let items = world.one(&world.root, "ALL_OWNED_CEOS")?;
    let first = reserve(edit, counters, &[CEO_CQI], original_items.len())?;
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
    // Equipment slots have their own CQI space. Donor slot IDs already belong
    // to other TROM characters, even when the associated CEO IDs are unique.
    let slots = donor.find(original, "CEO_EQUIPMENT_SLOT");
    let first_slot = reserve(edit, counters, &[CEO_EQUIPMENT_SLOT_CQI], slots.len())?;
    for (index, slot) in slots.iter().enumerate() {
        changes.insert(
            slot.fields()[0].start,
            integer(first_slot + u32::try_from(index)?),
        );
    }
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
    site: &Settlement,
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
    for unit in &units {
        for field in unit
            .fields()
            .iter()
            .filter(|field| donor.text(field) == Some(VANILLA_STARTING_INFANTRY))
        {
            army_changes.insert(field.start, edit.text_reference(TROM_STARTING_INFANTRY));
        }
    }
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
    for slot in donor.find(character, "PERSISTENT_RETINUE_SLOT_UNIT_INFO") {
        let unit = &slot.fields()[0];
        if donor.text(unit) == Some(VANILLA_STARTING_INFANTRY) {
            character_changes.insert(unit.start, edit.text_reference(TROM_STARTING_INFANTRY));
        }
    }
    for slot in donor.find(character, "CHARACTER_PERSISTENT_RETINUE_SLOT_RECRUITMENT") {
        let [tree, node, subtree] = slot.fields() else {
            anyhow::bail!("Unexpected founder recruitment slot");
        };
        let recruit = match donor.text(node) {
            Some("3k_main_wood_halberd_militia") => TROM_STARTING_INFANTRY,
            Some("node_empty") => "node_empty",
            _ => anyhow::bail!("Unexpected founder recruitment node"),
        };
        for (field, value) in [
            (tree, "hv_retinue_tree_all"),
            (node, recruit),
            (subtree, "hv_retinue_subtree_all"),
        ] {
            character_changes.insert(field.start, edit.text_reference(value));
        }
    }
    if site.key != HEPU {
        let position = donor.one(character, "LOCOMOTABLE")?.fields();
        let mut display = vec![0x0c];
        let (x, y) = map_position(site);
        display.extend((x * 595.1).to_le_bytes());
        display.extend((y * 541.7862).to_le_bytes());
        character_changes.insert(position[0].start, display);
        for (field, coordinate) in [(&position[1], site.x), (&position[2], site.y)] {
            let mut bytes = vec![7];
            bytes.extend(coordinate.to_le_bytes());
            character_changes.insert(field.start, bytes);
        }
    }
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

fn roster(
    edit: &mut Edit<'_>,
    donor: &Document,
    key: &str,
    region: &str,
    in_world: bool,
) -> Result<()> {
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
    let ownership = doc.group("REGION_OWNERSHIPS", 0, region)?;
    edit.replace_text(&ownership[1], key);
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
        donor_frontend
            .one(&donor_frontend.root, "COMPRESSED_DATA")?
            .fields()[0]
            .start,
    )?)?;
    let mut frontend_edit = Edit::new(&frontend);
    let mut edit = Edit::new(&world);
    let site = config.settlement().context("Missing starting settlement")?;
    roster(&mut frontend_edit, &donor_frontend, key, &site.key, false)?;
    roster(&mut edit, &donor_frontend, key, &site.key, true)?;
    let faction = world.record("FACTION", 1, DORMANT)?;
    let origin = donor.record("FACTION", 1, key)?;
    for name in ["FACTION_FLAG_AND_COLOURS", "FACTION_TECHNOLOGY_MANAGER"] {
        let bytes = edit.import(&donor, donor.one(origin, name)?, &HashMap::new())?;
        edit.replace(world.one(faction, name)?, bytes);
    }
    settlement::place(&mut edit, faction, &donor, origin, key, site)?;
    founder(&mut edit, &donor, faction, origin, key, site)?;
    let container = frontend.one(&frontend.root, "COMPRESSED_DATA")?;
    let mut bytes = frontend.data[container.start..container.fields()[0].start - 5].to_vec();
    bytes.extend(compression_record(&frontend.data, &edit.finish()?)?);
    frontend_edit.replace(container, bytes);
    frontend_edit.finish()
}
