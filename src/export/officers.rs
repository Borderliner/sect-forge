use anyhow::{Context, Result};
use rpfm_lib::files::pack::Pack;

use super::CLASS_TABLE;
use crate::{
    config::{Config, Officer},
    game::{Localization, add_rows, field, rows, set},
};

pub(super) fn write(
    retail: &Pack,
    template: &Pack,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    faction: &str,
) -> Result<()> {
    if config.officers.is_empty() {
        return Ok(());
    }
    let officer_templates = "character_generation_templates";
    let generation = rows(retail, officer_templates)?;
    let modes = rows(retail, CLASS_TABLE)?;
    let base_names = rows(template, "names")?;
    let given = base_names
        .iter()
        .find(|row| row["id"].as_i64() == Some(1900000001) || field(row, "id") == "1900000001")
        .context("Missing given name template")?;
    let family = base_names
        .iter()
        .find(|row| row["id"].as_i64() == Some(1900000002) || field(row, "id") == "1900000002")
        .context("Missing family name template")?;
    let mut new_names = Vec::new();
    let mut new_templates = Vec::new();
    let mut ages = Vec::new();
    let mut new_modes = Vec::new();
    for (index, officer) in config.officers.iter().enumerate() {
        let key = Officer::key(faction, index);
        let element = officer.subtype.trim_start_matches("3k_general_");
        let male = format!("3k_main_template_generic_{element}_minister_normal_m_hero");
        let female = format!("3k_main_template_generic_{element}_minister_normal_f_hero");
        let donor = generation
            .iter()
            .find(|row| officer.female && field(row, "key") == female)
            .or_else(|| generation.iter().find(|row| field(row, "key") == male))
            .with_context(|| format!("Missing generic officer for {element}"))?;
        let given_id = 1_900_000_200 + 2 * index as u32;
        for (offset, (base, value)) in [(given, &officer.forename), (family, &officer.surname)]
            .into_iter()
            .enumerate()
        {
            let id = given_id + offset as u32;
            let mut row = base.clone();
            set(&mut row, "id", id);
            set(&mut row, "name", value.as_str());
            new_names.push(row);
            loc.insert(format!("names_name_{id}"), value.clone());
        }
        let mut row = donor.clone();
        set(&mut row, "key", key.as_str());
        set(&mut row, "forename", given_id);
        set(&mut row, "family_name", given_id + 1);
        set(&mut row, "clan_name", serde_json::Value::Null);
        set(&mut row, "other_name", serde_json::Value::Null);
        set(&mut row, "weight", 0);
        set(&mut row, "is_male", !officer.female);
        set(&mut row, "unique", true);
        set(&mut row, "can_be_born", false);
        if let Some(age) = officer.age {
            set(&mut row, "spawn_age_range", key.as_str());
            ages.push(super::row! {
                "key": key, "has_valid_birth_year": true, "birth_year": 190 - i32::from(age),
                "min_age_for_birth_year": 0, "max_age_for_birth_year": 1000,
                "min_random_age": age, "max_random_age": age, "min_spawn_year": 190,
                "max_spawn_year": 190, "min_spawn_year_round": 0, "max_spawn_year_round": 999
            });
        }
        if officer.female && field(donor, "key") == male {
            let voice = if element == "wood" {
                "vo_actor_group_hero_Zheng_Jiang"
            } else {
                "vo_actor_group_hero_Lady_Sun"
            };
            set(&mut row, "voiceover_actor", voice);
        }
        new_templates.push(row);
        for mode in ["historical", "romance"] {
            let mut row = modes
                .iter()
                .find(|row| {
                    field(row, "character_generation_template") == field(donor, "key")
                        && field(row, "game_mode") == mode
                })
                .with_context(|| format!("Missing {mode} officer template for {element}"))?
                .clone();
            set(&mut row, "character_generation_template", key.as_str());
            set(&mut row, "min_rank", 0);
            set(&mut row, "max_rank", 0);
            new_modes.push(row);
        }
    }
    let suffix = format!("{faction}_officers");
    add_rows(pack, "character_generation_spawn_age_ranges", &suffix, ages)?;
    add_rows(pack, "names", &suffix, new_names)?;
    add_rows(pack, officer_templates, &suffix, new_templates)?;
    add_rows(pack, CLASS_TABLE, &suffix, new_modes)
}
