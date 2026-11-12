use std::{mem::MaybeUninit, ptr};

use anyhow::{Context, Result, ensure};
use indexmap::IndexMap;
use liblzma_sys::{
    LZMA_BUF_ERROR, LZMA_FILTER_LZMA1, LZMA_OK, LZMA_VLI_UNKNOWN, lzma_filter, lzma_lzma_preset,
    lzma_options_lzma, lzma_raw_buffer_decode, lzma_raw_buffer_encode,
};
use sha2::{Digest, Sha256};

use crate::{
    config::{BASE_ART, BASE_KEY, CLASSES, Config},
    portraits::FounderArt,
};

// CA's documents contain both the explicit index and the original table order.
pub type Strings = IndexMap<u32, String>;
const DICTIONARY: u32 = 262_144;
// liblzma's extended LZMA1 filter accepts streams with a known output size and
// an optional end marker. The raw bindings do not expose these two C macros.
const LZMA_FILTER_LZMA1EXT: u64 = 0x4000_0000_0000_0002;
const LZMA1EXT_ALLOW_END_MARKER: u32 = 1;

#[derive(Clone, Copy)]
pub enum Mode {
    Historical,
    Romance,
}

impl Mode {
    pub const ALL: [Self; 2] = [Self::Historical, Self::Romance];

    pub fn key(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::Romance => "romance",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Historical => 0,
            Self::Romance => 1,
        }
    }

    pub fn compression_marker(self) -> usize {
        LAYOUTS[self.index()].compression
    }
}

struct Layout {
    checksum: &'static str,
    compression: usize,
    portrait: usize,
    template: usize,
    flag: usize,
    army: usize,
    horse: usize,
    permissions: [usize; 10],
    bandit: [usize; 5],
}

// Offsets are only used after verifying the exact BOB-built template.
const LAYOUTS: [Layout; 2] = [
    Layout {
        checksum: "b6c417be00662d2c50a00aab79fbadec8e9807a6da07ecf663c5c7e0690a25cd",
        compression: 60844,
        portrait: 2404828,
        template: 2404895,
        flag: 357277,
        army: 355299,
        horse: 5279514,
        permissions: [
            20, 60839, 75644, 2977482, 2977489, 3446183, 3758832, 3758838, 3761313, 3761315,
        ],
        bandit: [355073, 78551, 78560, 354289, 354295],
    },
    Layout {
        checksum: "82df164c2c8303ff15fd8539b603703017ec7c39547b506d548a71bfbe584151",
        compression: 60845,
        portrait: 2404565,
        template: 2404632,
        flag: 357196,
        army: 355220,
        horse: 5279255,
        permissions: [
            20, 60840, 75645, 2977219, 2977226, 3445920, 3758569, 3758575, 3761050, 3761052,
        ],
        bandit: [354994, 78552, 78561, 354210, 354216],
    },
];

pub fn uint(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .context("Truncated CAAB integer")?
            .try_into()?,
    ))
}

pub fn short(data: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .context("Truncated CAAB short integer")?
            .try_into()?,
    ))
}

pub fn sized(data: &[u8], at: usize) -> Result<(usize, usize)> {
    let mut value = 0_usize;
    for i in at..at + 5 {
        let byte = *data.get(i).context("Truncated CAAB size")?;
        value = (value << 7) | usize::from(byte & 127);
        if byte < 128 {
            ensure!(value <= u32::MAX as usize, "Oversized CAAB record");
            return Ok((value, i + 1));
        }
    }
    anyhow::bail!("Invalid CAAB record size")
}

pub fn size_field(value: usize, width: usize) -> Result<Vec<u8>> {
    ensure!(
        (1..=5).contains(&width) && value <= u32::MAX as usize && value < (1_usize << (7 * width)),
        "Record size cannot be encoded in {width} bytes"
    );
    Ok((0..width)
        .rev()
        .map(|i| ((value >> (7 * i)) as u8 & 127) | if i == 0 { 0 } else { 128 })
        .collect())
}

pub fn split(data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Strings)> {
    ensure!(
        data.starts_with(&[0xca, 0xab, 0, 0]),
        "Expected a CAAB document"
    );
    let start = uint(data, 12)? as usize;
    ensure!(
        (16..data.len()).contains(&start),
        "Invalid CAAB string-table offset"
    );
    let names = short(data, start)?;
    let mut at = start + 2;
    for _ in 0..names {
        at += 2 + short(data, at)? as usize;
    }
    let wide = uint(data, at)?;
    at += 4;
    for _ in 0..wide {
        at += 6 + short(data, at)? as usize * 2;
    }
    let prefix_end = at;
    let count = uint(data, at)?;
    at += 4;
    let mut strings = Strings::new();
    for _ in 0..count {
        let len = short(data, at)? as usize;
        at += 2;
        let index = uint(data, at + len)?;
        let value = std::str::from_utf8(&data[at..at + len])?;
        ensure!(
            strings.insert(index, value.into()).is_none(),
            "Duplicate CAAB string index"
        );
        at += len + 4;
    }
    ensure!(at == data.len(), "Unexpected bytes after CAAB string table");
    Ok((
        data[..start].into(),
        data[start..prefix_end].into(),
        strings,
    ))
}

pub fn join(mut records: Vec<u8>, prefix: &[u8], strings: &Strings) -> Result<Vec<u8>> {
    let start = u32::try_from(records.len())?;
    records[12..16].copy_from_slice(&start.to_le_bytes());
    records.extend(prefix);
    records.extend(u32::try_from(strings.len())?.to_le_bytes());
    for (index, value) in strings {
        records.extend(u16::try_from(value.len())?.to_le_bytes());
        records.extend(value.as_bytes());
        records.extend(index.to_le_bytes());
    }
    Ok(records)
}

pub fn open_world(records: &[u8], marker: usize) -> Result<Vec<u8>> {
    let end = records
        .len()
        .checked_sub(15)
        .context("Truncated compression record")?;
    ensure!(
        records.get(marker) == Some(&0x46)
            && marker + 6 + sized(records, marker + 1)?.0 == end
            && records[end..].starts_with(&[0x80, 0x28, 0x0c, 0x08])
            && records.ends_with(&[0x46, 0x05, 0x5d, 0, 0, 4, 0]),
        "Unexpected campaign compression record"
    );
    let expected = uint(records, end + 4)?;
    ensure!(
        expected <= 128 * 1024 * 1024,
        "Campaign world exceeds the supported size"
    );
    let input = &records[marker + 6..end];
    let mut options = lzma_options()?;
    options.ext_flags = LZMA1EXT_ALLOW_END_MARKER;
    options.ext_size_low = expected;
    options.ext_size_high = 0;
    let filters = lzma_filters(LZMA_FILTER_LZMA1EXT, &mut options);
    let mut world = vec![0; expected as usize];
    let (mut read, mut written) = (0, 0);
    // The filter array, options, and both buffers live for the duration of the call.
    let status = unsafe {
        lzma_raw_buffer_decode(
            filters.as_ptr(),
            ptr::null(),
            input.as_ptr(),
            &mut read,
            input.len(),
            world.as_mut_ptr(),
            &mut written,
            world.len(),
        )
    };
    ensure!(
        status == LZMA_OK && read == input.len() && written == world.len(),
        "Could not decompress the campaign world (LZMA {status})"
    );
    Ok(world)
}

pub fn compression_record(outer: &[u8], world: &[u8]) -> Result<Vec<u8>> {
    let mut options = lzma_options()?;
    let filters = lzma_filters(LZMA_FILTER_LZMA1, &mut options);
    let mut payload = vec![0; world.len() + 1024];
    loop {
        let mut written = 0;
        // liblzma sees disjoint, bounded input and output buffers and a terminated filter list.
        let status = unsafe {
            lzma_raw_buffer_encode(
                filters.as_ptr(),
                ptr::null(),
                world.as_ptr(),
                world.len(),
                payload.as_mut_ptr(),
                &mut written,
                payload.len(),
            )
        };
        if status == LZMA_BUF_ERROR {
            payload.resize(payload.len() * 2, 0);
            continue;
        }
        ensure!(
            status == LZMA_OK,
            "Could not compress the campaign world (LZMA {status})"
        );
        payload.truncate(written);
        break;
    }
    let end = outer.len();
    let mut info = outer[end - 15..end - 11].to_vec();
    info.extend(u32::try_from(world.len())?.to_le_bytes());
    info.extend(&outer[end - 7..]);
    let mut result = size_field(payload.len() + 6 + info.len(), 5)?;
    result.push(0x46);
    result.extend(size_field(payload.len(), 5)?);
    result.extend(payload);
    result.extend(info);
    Ok(result)
}

fn lzma_options() -> Result<lzma_options_lzma> {
    let mut options = MaybeUninit::zeroed();
    // A valid preset initializes all required options. The reserved slots are zero-filled.
    let invalid = unsafe { lzma_lzma_preset(options.as_mut_ptr(), 6) };
    ensure!(invalid == 0, "Unsupported LZMA settings");
    let mut options = unsafe { options.assume_init() };
    options.dict_size = DICTIONARY;
    options.lc = 3;
    options.lp = 0;
    options.pb = 2;
    Ok(options)
}

fn lzma_filters(id: u64, options: &mut lzma_options_lzma) -> [lzma_filter; 2] {
    [
        lzma_filter {
            id,
            options: ptr::from_mut(options).cast(),
        },
        lzma_filter {
            id: LZMA_VLI_UNKNOWN,
            options: ptr::null_mut(),
        },
    ]
}

pub fn close_world(
    mut outer: Vec<u8>,
    names: &[u8],
    strings: &Strings,
    marker: usize,
    world: &[u8],
) -> Result<Vec<u8>> {
    let record = compression_record(&outer, world)?;
    outer.truncate(marker - 5);
    outer.extend(record);
    let size = size_field(outer.len() - 25, 5)?;
    outer[20..25].copy_from_slice(&size);
    join(outer, names, strings)
}

fn string_index(strings: &mut Strings, value: &str) -> u32 {
    if let Some((index, _)) = strings.iter().find(|(_, name)| *name == value) {
        return *index;
    }
    let index = strings.keys().max().map_or(0, |i| i + 1);
    strings.insert(index, value.into());
    index
}

fn reference(
    world: &mut [u8],
    strings: &mut Strings,
    at: usize,
    old: &str,
    new: &str,
) -> Result<()> {
    ensure!(
        world.get(at) == Some(&0x0f)
            && strings.get(&uint(world, at + 1)?).map(String::as_str) == Some(old),
        "Expected campaign reference to {old} at {at}"
    );
    world[at + 1..at + 5].copy_from_slice(&string_index(strings, new).to_le_bytes());
    Ok(())
}

fn grow(world: &mut [u8], offset: usize, delta: isize) -> Result<()> {
    let (old, end) = sized(world, offset)?;
    world[offset..end].copy_from_slice(&size_field(
        old.checked_add_signed(delta)
            .context("Invalid campaign record size")?,
        end - offset,
    )?);
    Ok(())
}

const CLASS_CEOS: [(isize, usize, &str, &str); 8] = [
    (0, 0, "ancillary_mount_brown_horse", "ancillary_brown_horse"),
    (
        41,
        66,
        "ancillary_weapon_two_handed_spear_common",
        "ancillary_two_handed_spear_common",
    ),
    (
        -27,
        132,
        "ancillary_armour_medium_armour_wood_and_fire_common",
        "ancillary_medium_armour_wood_and_fire_common",
    ),
    (
        177,
        198,
        "ceo_equipment_permissions_weapons_wood",
        "ceo_equipment_permissions_weapons_wood",
    ),
    (
        191,
        232,
        "ceo_equipment_permissions_armour_wood",
        "ceo_equipment_permissions_armour_wood",
    ),
    (150, 266, "ceo_class_wood", "ceo_node_class_wood_01"),
    (
        96,
        300,
        "ceo_career_generated_minister_wood",
        "ceo_node_career_generated_minister_wood_01",
    ),
    (
        123,
        505,
        "ceo_career_childhood_wood",
        "ceo_node_career_childhood_wood_01",
    ),
];

pub struct ClassStart {
    pub skill_set: String,
    pub mastery: String,
    pub ability: String,
    pub retinue: String,
}

fn patch_class(
    world: &mut Vec<u8>,
    strings: &mut Strings,
    mode: Mode,
    element: &str,
    class: &ClassStart,
) -> Result<()> {
    let layout = &LAYOUTS[mode.index()];
    let role = match mode {
        Mode::Romance => "hero",
        Mode::Historical => "general",
    };
    let old_unit = format!("3k_main_{role}_wood_generic");
    let new_unit = format!("3k_main_{role}_{element}_generic");
    let old_retinue = format!("3k_main_{role}_generic_wood");
    let old_ability = match mode {
        Mode::Romance => "3k_main_skill_special_ability_wood_binding_fury",
        Mode::Historical => "3k_main_skill_bodyguard_wood_wood_alt_1",
    };
    let subtype = format!("3k_general_{element}");
    let art = layout.portrait;
    for (at, old, new) in [
        (art - 6, CLASSES[0].0, subtype.as_str()),
        (art + 62, CLASSES[0].0, subtype.as_str()),
        (
            art + 251,
            "3k_main_skillset_generic_minister_wood",
            &class.skill_set,
        ),
        (
            art + 266,
            "3k_main_skill_mastery_wood_3_patience_mlvl_3",
            &class.mastery,
        ),
        (art + 279, old_ability, &class.ability),
        (art + 725, &old_retinue, &class.retinue),
        (art + 730, &old_retinue, &class.retinue),
        (art + 759, &old_unit, &new_unit),
        (layout.army, &old_unit, &new_unit),
        (layout.army + 115, &old_unit, &new_unit),
    ] {
        reference(world, strings, at, old, new)?;
    }

    let (horse, weapon, armour, permissions): (_, _, _, &[&str]) = match element {
        "fire" => (
            "red_horse",
            "two_handed_spear_common",
            "medium_armour_wood_and_fire_common",
            &[
                "main_axe_two_handed",
                "main_spear_two_handed",
                "main_spear_two_handed_long",
            ],
        ),
        "earth" => (
            "white_horse",
            "double_edged_sword_common",
            "light_armour_earth_metal_and_water_common",
            &[
                "main_axe_dual",
                "main_axe_one_handed",
                "main_sword_dual",
                "main_sword_one_handed",
            ],
        ),
        "metal" => (
            "grey_horse",
            "double_edged_sword_common",
            "light_armour_earth_metal_and_water_common",
            &[
                "ytr_mace_dual",
                "main_axe_dual",
                "main_axe_one_handed",
                "main_sword_dual",
                "main_sword_one_handed",
            ],
        ),
        "water" => (
            "black_horse",
            "single_edged_sword_common",
            "strategist_light_armour_water_common",
            &["main_sword_one_handed"],
        ),
        _ => anyhow::bail!("Unexpected class element {element}"),
    };
    let rename = |key: &str| {
        let renamed = match key {
            "ancillary_mount_brown_horse" => format!("ancillary_mount_{horse}"),
            "ancillary_brown_horse" => format!("ancillary_{horse}"),
            "ancillary_weapon_two_handed_spear_common" => format!("ancillary_weapon_{weapon}"),
            "ancillary_two_handed_spear_common" => format!("ancillary_{weapon}"),
            "ancillary_armour_medium_armour_wood_and_fire_common" => {
                format!("ancillary_armour_{armour}")
            }
            "ancillary_medium_armour_wood_and_fire_common" => format!("ancillary_{armour}"),
            _ => key.replace("_wood", &format!("_{element}")),
        };
        format!("3k_main_{renamed}")
    };
    for (manager, owned, key, node) in CLASS_CEOS {
        for (at, old) in [
            ((art + 1355524).checked_add_signed(manager).unwrap(), key),
            (layout.horse + owned, key),
            (layout.horse + owned + 27, node),
        ] {
            reference(world, strings, at, &format!("3k_main_{old}"), &rename(old))?;
        }
    }

    let size = layout.permissions[9];
    ensure!(
        world[size - 1..size + 1] == [0x4f, 76],
        "Could not locate founder equipment permissions"
    );
    let start = size + 1;
    let end = start + 76;
    let mut bytes = Vec::new();
    for offset in (start..end).step_by(4) {
        let index = uint(world, offset)?;
        let key = &strings[&index];
        if !key.contains("_ancillary_armour_character_")
            && !key.contains("_ancillary_weapon_character_")
        {
            bytes.extend(index.to_le_bytes());
        }
    }
    ensure!(
        bytes.len() == 13 * 4,
        "Unexpected founder equipment permissions"
    );
    bytes.extend(
        string_index(
            strings,
            &format!("3k_main_ceo_permissions_ancillary_armour_character_{element}_enable"),
        )
        .to_le_bytes(),
    );
    for permission in permissions {
        let (culture, weapon) = permission.split_once('_').unwrap();
        bytes.extend(
            string_index(
                strings,
                &format!("3k_{culture}_ceo_permissions_ancillary_weapon_character_{weapon}_enable"),
            )
            .to_le_bytes(),
        );
    }
    for offset in layout.permissions {
        grow(world, offset, bytes.len() as isize - 76)?;
    }
    world.splice(start..end, bytes);
    Ok(())
}

pub fn customize(
    data: &[u8],
    mode: Mode,
    config: &Config,
    key: &str,
    art: &FounderArt,
    class: Option<&ClassStart>,
    technology: Option<&str>,
) -> Result<Vec<u8>> {
    let layout = &LAYOUTS[mode.index()];
    ensure!(
        hex::encode(Sha256::digest(data)) == layout.checksum,
        "The {} start differs from the supported Hepu template",
        mode.key()
    );
    let (outer, names, mut strings) = split(data)?;
    let (mut world, world_names, mut world_strings) =
        split(&open_world(&outer, layout.compression)?)?;
    let leader = format!("{BASE_KEY}_leader");
    ensure!(
        world[layout.template] == 0x0f
            && world_strings[&uint(&world, layout.template + 1)?] == leader,
        "Could not locate the campaign founder"
    );
    for table in [&mut strings, &mut world_strings] {
        for name in table.values_mut() {
            if name.contains(BASE_KEY) {
                *name = name.replace(BASE_KEY, key);
            }
        }
    }
    reference(
        &mut world,
        &mut world_strings,
        layout.portrait,
        BASE_ART,
        &art.key,
    )?;
    // CHARACTER_ART_SET_INFO stores is_male immediately before the art override.
    let gender = &mut world[layout.portrait - 1];
    ensure!(*gender == 0x12, "Could not locate the founder's gender");
    if !art.is_male {
        *gender = 0x13;
    }
    if let Some((old, new, offsets)) = config.settlement().and_then(|site| site.buildings.as_ref())
    {
        let shift = if matches!(mode, Mode::Historical) {
            232
        } else {
            0
        };
        for offset in offsets {
            reference(&mut world, &mut world_strings, offset + shift, old, new)?;
        }
    }
    if let Some(class) = class {
        patch_class(
            &mut world,
            &mut world_strings,
            mode,
            config.element(),
            class,
        )?;
    }

    let channels = layout.flag + 7;
    ensure!(
        world[layout.flag - 1..layout.flag + 2] == [0x82, 0x9f, 0x17]
            && world[channels..channels + 18]
                .iter()
                .step_by(2)
                .all(|byte| *byte == 6),
        "Could not locate faction colours"
    );
    let colours = hex::decode(format!(
        "{}{}{}",
        config.primary_colour, config.secondary_colour, config.primary_colour
    ))?;
    for (index, colour) in colours.into_iter().enumerate() {
        world[channels + index * 2 + 1] = colour;
    }

    if let Some(technology) = technology {
        let manager = layout.bandit[0];
        let array = manager + 8;
        ensure!(
            world[manager..manager + 4] == [0x80, 0x5d, 0x18, 0x0f]
                && world_strings[&uint(&world, manager + 4)?] == key
                && world[array..array + 4] == [0xc0, 0x5e, 0, 0],
            "Could not locate starting research"
        );
        let mut entry = vec![7, 1, 6, 0x0f];
        entry.extend(string_index(&mut world_strings, technology).to_le_bytes());
        entry.push(0x12);
        for offset in layout.permissions[..3]
            .iter()
            .copied()
            .chain(layout.bandit[1..].iter().copied())
            .chain([manager + 2])
        {
            grow(&mut world, offset, 7)?;
        }
        world.splice(array + 2..array + 4, entry);
    }
    close_world(
        outer,
        &names,
        &strings,
        layout.compression,
        &join(world, &world_names, &world_strings)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{caab::Document, config::CAMPAIGN, game};

    #[test]
    fn female_founder_uses_female_art_in_both_campaign_modes() -> Result<()> {
        let pack = game::template()?;
        let config = Config::default();
        let key = config.key();
        let art = FounderArt {
            key: "3k_main_art_set_historical_zheng_jiang_general".into(),
            is_male: false,
        };
        for mode in Mode::ALL {
            let bytes = game::raw(
                &pack,
                &format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key()),
            )?;
            let generated = customize(&bytes, mode, &config, &key, &art, None, None)?;
            let (outer, _, _) = split(&generated)?;
            let world = Document::read(&open_world(&outer, mode.compression_marker())?)?;
            let founder = world.record("PERSISTENT_CHARACTER", 10, &format!("{key}_leader"))?;
            let appearance = world.one(founder, "CHARACTER_ART_SET_INFO")?.fields();
            assert_eq!(world.raw(&appearance[5]), [0x13]);
            assert_eq!(world.text(&appearance[6]), Some(art.key.as_str()));
        }
        Ok(())
    }

    #[test]
    fn campaign_world_round_trip_preserves_string_table_order() -> Result<()> {
        let pack = game::template()?;
        for mode in Mode::ALL {
            let bytes = game::raw(
                &pack,
                &format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key()),
            )?;
            let (outer, _, _) = split(&bytes)?;
            let world = open_world(&outer, mode.compression_marker())?;
            let (records, names, strings) = split(&world)?;
            assert!(
                join(records, &names, &strings)? == world,
                "{} string table order",
                mode.key()
            );
        }
        Ok(())
    }
}
