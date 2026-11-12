use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::{Container, pack::Pack};
use serde_json::Value;

use crate::{
    campaign_actions,
    config::{ANCILLARIES, BASE_KEY, CAMPAIGN, CLASSES, Config, HAN, HEPU, SLOTS, Variant},
    doctrines,
    game::{self, Game, Localization, Row, add, add_loc, add_rows, db_path, field, raw, rows, set},
    portraits::{self, FounderArt},
    startpos::{self, ClassStart, Mode},
    trom, unlocks,
};

const BANDIT: &str = "3k_main_faction_zhang_yan";
const CLASS_TABLE: &str = "character_generation_template_game_mode_details";
pub const TERRITORY: &str = "Hold one commandery. Taking a settlement in another moves your base and abandons the old holdings.";
const PATRON: &str = "Choose your patron in diplomacy with a standalone offer of regular payments.";
const TABLES: [(&str, &str); 19] = [
    ("factions", "key"),
    ("names", "id"),
    ("character_generation_templates", "key"),
    (CLASS_TABLE, "character_generation_template"),
    ("frontend_factions", "faction"),
    (
        "frontend_faction_to_frontend_faction_leaders",
        "frontend_faction",
    ),
    ("frontend_faction_leaders", "key"),
    ("frontend_characters", "key"),
    ("building_chain_availabilities", "faction"),
    ("cai_personality_group_overrides", "faction_key"),
    ("campaign_group_member_criteria_factions", "faction"),
    ("campaign_map_attrition_faction_immunities", "faction"),
    ("campaign_stances_factions_junctions", "faction"),
    ("faction_agent_permitted_subtypes", "faction"),
    ("faction_banners", "key"),
    ("faction_civil_war_setups", "primary_faction"),
    ("faction_to_name_groups", "faction"),
    ("faction_uniform_colours", "faction_name"),
    ("unit_banners", "key"),
];

macro_rules! row {
    ($($tokens:tt)*) => { serde_json::json!({ $($tokens)* }).as_object().unwrap().clone() };
}
pub(crate) use row;

pub fn build(game: &Game, config: Config, output: &Path, report: impl Fn(&str)) -> Result<PathBuf> {
    let config = config.validated()?;
    let key = config.key();
    let filename = config.filename();
    let cult = config.variant == Variant::Cult;
    let mut dependencies = BTreeSet::new();
    let mut pack = game::new_pack(&filename);
    let template = game::template()?;
    let class = if config.subtype != CLASSES[0].0 {
        Some(class_details(&game.pack, config.element())?)
    } else {
        None
    };
    let technology = if cult {
        Some(bandit_network(&game.pack, &config.start_region)?)
    } else {
        None
    };
    if !config.starting_traits.is_empty() {
        let available = game.traits()?;
        for key in &config.starting_traits {
            ensure!(
                available.iter().any(|item| item.key == *key),
                "Unknown starting trait {key}"
            );
        }
    }
    report("Preparing founder artwork");
    let portrait_pack = config
        .portrait_pack
        .as_ref()
        .map(|path| {
            ensure!(
                path.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pack")),
                "Choose a portrait .pack file"
            );
            dependencies.insert(
                path.file_name()
                    .context("Invalid portrait pack name")?
                    .to_string_lossy()
                    .into_owned(),
            );
            game::open_pack(path)
        })
        .transpose()?;
    let portraits = portraits::catalog(&game.pack, portrait_pack.as_ref())?;
    let portrait = portraits
        .iter()
        .find(|item| item.key == config.art_set)
        .with_context(|| {
            format!(
                "Portrait {} is missing from the chosen source",
                config.art_set
            )
        })?;
    ensure!(
        portrait.can_export,
        "This portrait is preview-only. Choose a portrait with compatible campaign artwork."
    );
    let art = portraits::export(
        &game.pack,
        portrait_pack.as_ref(),
        &mut pack,
        &key,
        portrait,
        config.element(),
    )?;
    let trom_pack = if config.trom_compat {
        report("Reading TROM 5.2 campaign");
        let parts = trom::packs(&game::workshop_packs())?;
        dependencies.extend(
            parts
                .iter()
                .map(|part| part.file_name().unwrap().to_string_lossy().into_owned()),
        );
        Some(game::open_pack(&parts[0])?)
    } else {
        None
    };

    for mode in Mode::ALL {
        report(&format!("Writing the {} campaign start", mode.key()));
        let path = format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key());
        let class_start = class.as_ref().map(|details| &details[mode.key()].1);
        let mut generated = startpos::customize(
            &raw(&template, &path)?,
            mode,
            &config,
            &key,
            &art,
            class_start,
            technology.as_deref(),
        )
        .with_context(|| format!("Customize {} start position", mode.key()))?;
        if let Some(trom) = &trom_pack {
            generated = trom::merge(&raw(trom, &path)?, &generated, mode, &key, &config)
                .with_context(|| format!("Merge the {} TROM campaign", mode.key()))?;
        }
        add(&mut pack, &path, &generated)?;
    }

    report("Writing faction, doctrines, and victory rules");
    let unlocks = unlocks::Sources::new(&game.pack, trom_pack.as_ref());
    let permission_group =
        unlocks::write_recruitment(&unlocks, &mut pack, &template, &config, &key)?;
    faction_rows(
        &game.pack,
        &template,
        &mut pack,
        &config,
        &key,
        &art,
        class.as_ref(),
        permission_group.as_deref(),
    )?;
    let mut loc = localization(&config, &key);
    variant(&game.pack, &mut pack, &mut loc, &config, &key)?;
    campaign_actions::write(&mut pack, &mut loc, config.variant, &key)?;
    doctrines::write(
        &game.pack,
        trom_pack.as_ref(),
        &mut pack,
        &mut loc,
        &config,
        &key,
    )?;
    unlocks::write_buildings(&unlocks, &mut pack, &mut loc, &config, &key)?;
    if !cult {
        if let Some(trom) = &trom_pack {
            add(
                &mut pack,
                trom::REFORMS,
                &trom::reforms(&raw(trom, trom::REFORMS)?, &key)?,
            )?;
        }
    }
    scripts(&mut pack, &config, &key)?;
    add_loc(&mut pack, &format!("text/db/{key}.loc"), loc)?;
    pack.set_dependencies(dependencies.into_iter().map(|name| (true, name)).collect());

    report("Verifying the mod pack");
    fs::create_dir_all(output)?;
    let stage = tempfile::Builder::new()
        .prefix(".sectforge-")
        .tempdir_in(output)?;
    let staged = stage.path().join(&filename);
    game::save(&mut pack, &staged)?;
    let saved = game::open_pack(&staged)?;
    verify(&pack, &saved)?;
    drop(saved);
    let published = output.join(&filename);
    // The staged file and output are on the same volume.
    tempfile::TempPath::try_from_path(staged)?
        .persist(&published)
        .with_context(|| {
            format!(
                "Publish {}. Close any application holding the pack and retry.",
                published.display()
            )
        })?;
    Ok(published)
}

fn verify(original: &Pack, saved: &Pack) -> Result<()> {
    ensure!(
        original.dependencies() == saved.dependencies(),
        "Saved pack dependencies differ from the export"
    );
    for path in original.files().keys() {
        ensure!(
            saved.file(path, false).is_some(),
            "Saved pack is missing {path}"
        );
    }
    for mode in Mode::ALL {
        let path = format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key());
        ensure!(
            raw(saved, &path)? == raw(original, &path)?,
            "Saved start position differs for {}",
            mode.key()
        );
    }
    Ok(())
}

type Classes = BTreeMap<&'static str, (Row, ClassStart)>;

fn class_details(game: &Pack, element: &str) -> Result<Classes> {
    let key = format!("3k_main_template_generic_{element}_minister_normal_m_hero");
    let donors = rows(game, CLASS_TABLE)?
        .into_iter()
        .filter(|row| field(row, "character_generation_template") == key)
        .collect::<Vec<_>>();
    let romance = donors
        .iter()
        .find(|row| field(row, "game_mode") == "romance")
        .context("Missing Romance class donor")?;
    let skill_set = field(romance, "skill_set_override");
    let skills = rows(game, "character_skill_nodes")?
        .into_iter()
        .filter(|row| {
            field(row, "character_skill_node_set_key") == skill_set
                && row["points_on_creation"].as_i64() == Some(1)
        })
        .collect::<Vec<_>>();
    let skill = |mode| -> Result<String> {
        let mut entries = skills.iter().filter(|row| field(row, "game_mode") == mode);
        let first = entries
            .next()
            .with_context(|| format!("Missing starting skill for {element} {mode}"))?;
        ensure!(
            entries.next().is_none(),
            "Unexpected starting skill list for {element} {mode}"
        );
        Ok(field(first, "character_skill_key").into())
    };
    let mut classes = Classes::new();
    for mode in Mode::ALL {
        let donor = donors
            .iter()
            .find(|row| field(row, "game_mode") == mode.key())
            .context("Missing starting class data")?;
        let subset: Row = [
            "initial_ceos",
            "retinue",
            "attribute_set",
            "skill_set_override",
        ]
        .into_iter()
        .filter_map(|key| donor.get(key).map(|value| (key.into(), value.clone())))
        .collect();
        classes.insert(
            mode.key(),
            (
                subset,
                ClassStart {
                    skill_set: field(donor, "skill_set_override").into(),
                    mastery: skill("")?,
                    ability: skill(mode.key())?,
                    retinue: field(donor, "retinue").into(),
                },
            ),
        );
    }
    Ok(classes)
}

fn bandit_network(game: &Pack, region: &str) -> Result<String> {
    let region_name = region
        .trim_start_matches("3k_main_")
        .split("_resource_")
        .next()
        .unwrap()
        .trim_end_matches("_capital");
    let suffix = format!(
        "_{}",
        if region_name == "yizhou_island" {
            "tongan"
        } else {
            region_name
        }
    );
    let techs = rows(game, "technology_nodes")?
        .into_iter()
        .filter_map(|row| {
            let key = field(&row, "technology_key");
            (key.starts_with("3k_dlc05_tech_bandit_network_") && key.ends_with(&suffix))
                .then(|| key.to_owned())
        })
        .collect::<Vec<_>>();
    ensure!(
        techs.len() == 1,
        "Could not find Bandit Network research for {region}"
    );
    Ok(techs.into_iter().next().unwrap())
}

fn faction_rows(
    game: &Pack,
    template: &Pack,
    pack: &mut Pack,
    config: &Config,
    key: &str,
    art: &FounderArt,
    class: Option<&Classes>,
    permission_group: Option<&str>,
) -> Result<()> {
    let cult = config.variant == Variant::Cult;
    let donor = if cult {
        Some(
            rows(game, "factions")?
                .into_iter()
                .find(|row| field(row, "key") == BANDIT)
                .context("Missing Bandit faction donor")?,
        )
    } else {
        None
    };
    let element = config.element();
    let separatists = format!("{BASE_KEY}_separatists");
    for (table, column) in TABLES {
        if cult
            && matches!(
                table,
                "building_chain_availabilities" | "campaign_group_member_criteria_factions"
            )
        {
            continue;
        }
        let targets = match table {
            "names" => vec!["1900000001".into(), "1900000002".into()],
            "character_generation_templates" | CLASS_TABLE => vec![format!("{BASE_KEY}_leader")],
            "frontend_characters" => vec![format!("{BASE_KEY}_leader_faction_leader")],
            "frontend_faction_leaders" => vec![format!("{BASE_KEY}_ruler")],
            "frontend_factions"
            | "frontend_faction_to_frontend_faction_leaders"
            | "faction_civil_war_setups" => vec![BASE_KEY.into()],
            _ => vec![BASE_KEY.into(), separatists.clone()],
        };
        let minimum = match table {
            "names" | "factions" | CLASS_TABLE => 2,
            "character_generation_templates" => 1,
            name if name.starts_with("frontend_") => 1,
            _ => 0,
        };
        let path = db_path(table, "data__");
        let mut subset = if template.file(&path, true).is_some() {
            rows(template, table)?
                .into_iter()
                .filter(|row| {
                    let value = &row[column];
                    value.as_str().map_or_else(
                        || targets.contains(&value.to_string()),
                        |value| targets.iter().any(|target| target == value),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        ensure!(
            subset.len() >= minimum,
            "The template is missing {table} rows"
        );
        if cult && table == "campaign_stances_factions_junctions" {
            let stances: BTreeSet<_> = subset
                .iter()
                .filter(|row| field(row, "faction") == BASE_KEY)
                .map(|row| field(row, "stance").to_owned())
                .collect();
            for mut row in rows(game, table)? {
                if field(&row, "faction") == BANDIT && !stances.contains(field(&row, "stance")) {
                    set(&mut row, "faction", BASE_KEY);
                    subset.push(row);
                }
            }
        }
        for row in &mut subset {
            for value in row.values_mut() {
                if let Value::String(value) = value
                    && value.contains(BASE_KEY)
                {
                    *value = value.replace(BASE_KEY, key);
                }
            }
            match table {
                "factions" => {
                    if let Some(donor) = &donor {
                        for name in [
                            "subculture",
                            "ui_skin",
                            "military_group",
                            "campaign_unit_permission_group",
                            "feature_forest",
                            "text_replacement_key",
                        ] {
                            set(row, name, donor[name].clone());
                        }
                    }
                    if field(row, "key") == key
                        && let Some(group) = permission_group
                    {
                        set(row, "campaign_unit_permission_group", group);
                    }
                    let stem = if field(row, "key").ends_with("_separatists") {
                        format!("{key}_sep_")
                    } else {
                        format!("{key}_")
                    };
                    for name in [
                        "screen_name",
                        "screen_adjective",
                        "attack_desc",
                        "defend_desc",
                    ] {
                        set(row, name, format!("{stem}{name}"));
                    }
                    for name in [
                        "primary_colour_hex",
                        "uniform_colour_hex",
                        "card_colour_hex",
                    ] {
                        set(row, name, config.primary_colour.as_str());
                    }
                    set(
                        row,
                        "secondary_colour_hex",
                        config.secondary_colour.as_str(),
                    );
                }
                "character_generation_templates" => {
                    set(row, "art_set_override", art.key.as_str());
                    set(row, "is_male", art.is_male);
                    set(row, "subtype", config.subtype.as_str());
                    let voice = if art.is_male {
                        format!("vo_actor_group_generic_any_male_{element}_general")
                    } else {
                        // Vanilla uses these actor groups for female Champions and Vanguards.
                        match element {
                            "wood" => "vo_actor_group_hero_Zheng_Jiang".into(),
                            "fire" => "vo_actor_group_hero_Lady_Sun".into(),
                            _ => format!("vo_actor_group_generic_female_{element}_general"),
                        }
                    };
                    set(row, "voiceover_actor", voice);
                }
                CLASS_TABLE => {
                    if let Some(class) = class.and_then(|class| class.get(field(row, "game_mode")))
                    {
                        row.extend(class.0.clone());
                    }
                }
                "frontend_characters" => {
                    set(
                        row,
                        "optional_specialisation",
                        format!("{key}_specialisation"),
                    );
                    set(
                        row,
                        "optional_general_type",
                        format!(
                            "3k_main_hero_{}",
                            if element == "water" {
                                "strategist_water"
                            } else {
                                element
                            }
                        ),
                    );
                }
                "frontend_factions" => set(row, "playstyle", key),
                "frontend_faction_leaders" => {
                    if cult {
                        set(row, "starting_progression_level", "3k_dlc05_bandit_1");
                    }
                    let colour = CLASSES
                        .iter()
                        .find(|class| class.0 == config.subtype)
                        .unwrap()
                        .2;
                    set(row, "video", "3k_main_faction_intro_wood");
                    set(
                        row,
                        "loading_screen_intro_video",
                        "3k_main_faction_intro_wood",
                    );
                    set(
                        row,
                        "victory_screen_image",
                        format!("ui/post_campaign_background/campaign_victory_{colour}.png"),
                    );
                    if config.start_region != HEPU {
                        let site = config.settlement().unwrap();
                        set(row, "override_force_location_x", site.x);
                        set(row, "override_force_location_y", site.y);
                    }
                }
                _ => {}
            }
        }
        add_rows(pack, table, key, subset)?;
    }
    let ruler = format!("{key}_ruler");
    let council = format!("{key}_council");
    add_rows(
        pack,
        "faction_to_faction_groups_junctions",
        key,
        vec![row! {
            "faction_group_key": if cult { "3k_bandits" } else { "3k_the_coalition" },
            "faction_key": key, "optional_campaign_key": CAMPAIGN
        }],
    )?;
    add_rows(
        pack,
        "political_parties",
        key,
        vec![
            row! {"key": ruler, "playable": true, "effect_bundle": "3k_main_dummy_effect_bundle", "initial_power": 60},
            row! {"key": council, "playable": false, "effect_bundle": "3k_main_dummy_effect_bundle", "initial_power": 40},
        ],
    )?;
    add_rows(
        pack,
        "faction_political_parties_junctions",
        key,
        vec![
            row! {"faction_key": key, "political_party_key": ruler},
            row! {"faction_key": format!("{key}_separatists"), "political_party_key": council},
        ],
    )?;
    add_rows(
        pack,
        "political_parties_frontend_leaders_junctions",
        key,
        vec![row! {"frontend_leader": ruler, "political_party": ruler}],
    )
}

fn localization(config: &Config, key: &str) -> Localization {
    let name = &config.sect_name;
    let site = &config.settlement().unwrap().name;
    let mut loc = BTreeMap::from([
        ("names_name_1900000001".into(), config.forename.clone()),
        ("names_name_1900000002".into(), config.surname.clone()),
        (
            "start_pos_factions_description_1900000000".into(),
            name.clone(),
        ),
        (
            "start_pos_factions_long_description_1900000000".into(),
            name.clone(),
        ),
        (
            "start_pos_factions_description_1900000050".into(),
            format!("{name} Separatists"),
        ),
        (
            "start_pos_factions_long_description_1900000050".into(),
            format!("{name} Separatists"),
        ),
        (
            format!("frontend_faction_leaders_localised_subtitle_{key}_ruler"),
            name.clone(),
        ),
        (
            format!("frontend_faction_leaders_localised_description_{key}_ruler"),
            format!("{name} begins in {site} in 190 CE."),
        ),
    ]);
    for (faction, display, stem) in [
        (key.into(), name.clone(), format!("{key}_")),
        (
            format!("{key}_separatists"),
            format!("{name} Separatists"),
            format!("{key}_sep_"),
        ),
    ] {
        loc.extend([
            (format!("factions_screen_name_{faction}"), display.clone()),
            (
                format!("factions_screen_adjective_{faction}"),
                display.clone(),
            ),
            (
                format!("factions_screen_name_when_rebels_{faction}"),
                format!("{display} Renegades"),
            ),
            (format!("{stem}screen_name"), display.clone()),
            (format!("{stem}screen_adjective"), display.clone()),
            (
                format!("{stem}attack_desc"),
                format!("You are attacking a {display} force."),
            ),
            (
                format!("{stem}defend_desc"),
                format!("You are defending against a {display} force."),
            ),
        ]);
    }
    loc
}

pub fn dummy_effect(game: &Pack) -> Result<Row> {
    rows(game, "effects")?
        .into_iter()
        .find(|row| field(row, "effect") == "3k_main_dummy_effect_background_income")
        .context("Missing vanilla effect template")
}

pub fn bundle(key: &str) -> Row {
    row! {"key": key, "localised_title": "", "localised_description": "", "ui_icon": "technology.png",
    "bundle_target": "faction", "icon_supplement_colour_type": "positive", "show_in_generic_bundle_display": true}
}

fn variant(
    game: &Pack,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
) -> Result<()> {
    let preview = format!("{key}_specialisation");
    let suffix = format!("{key}_variant");
    let mut descriptions = match config.variant {
        Variant::Sect => vec![
            "Research doctrines to grow Sect Influence and increase follower donations.",
            PATRON,
            config.variant.objective(),
        ],
        Variant::Cult => vec![
            "Research warband reforms in the Bandit Network and take contracts for new raids.",
            config.variant.objective(),
        ],
    };
    if config.sect_mode {
        descriptions.push(TERRITORY);
    }
    let dummy = dummy_effect(game)?;
    let mut effects = Vec::new();
    let mut generalised = Vec::new();
    let mut junctions = Vec::new();
    for (index, description) in descriptions.into_iter().enumerate() {
        let effect = format!("{preview}_{}", index + 1);
        let mut row = dummy.clone();
        set(&mut row, "effect", effect.as_str());
        set(&mut row, "generalised_description", effect.as_str());
        set(&mut row, "icon", "technology.png");
        set(&mut row, "priority", ((index + 1) * 10) as u32);
        effects.push(row);
        generalised.push(row! {"key": effect});
        junctions.push(row!{"effect_bundle_key": preview, "effect_key": effect, "effect_scope": "faction_to_faction_own_unseen",
            "value": 0, "advancement_stage": "start_turn_completed"});
        loc.insert(
            format!("effects_generalised_descriptions_localised_description_{effect}"),
            format!("• {description}"),
        );
    }
    let title = match config.variant {
        Variant::Sect => {
            loc.extend([
                (format!("event_feed_strings_text_{key}_patron_title"), "Patron chosen".into()),
                (format!("event_feed_strings_text_{key}_patron_detail"), "Your payment agreement has pledged your support.".into()),
                (format!("event_feed_strings_text_{key}_patron_body"), "You will win if this warlord claims an imperial throne. A standalone offer of regular payments to another eligible warlord will switch your patron.".into()),
            ]);
            "Crown a Warlord"
        }
        Variant::Cult => {
            loc.insert(
                format!("event_feed_strings_text_{key}_sack_title"),
                "City sacked".into(),
            );
            for count in 1..30 {
                loc.insert(
                    format!("event_feed_strings_text_{key}_sack_{count}"),
                    format!("{count} of 30 cities sacked."),
                );
            }
            "Sack 30 Cities"
        }
    };
    loc.extend([
        (
            format!("frontend_faction_leader_playstyles_localised_title_{key}"),
            format!("{} · {title}", config.variant.label()),
        ),
        (
            format!("effect_bundles_localised_title_{preview}"),
            title.into(),
        ),
        (
            format!("effect_bundles_localised_description_{preview}"),
            config.variant.objective().into(),
        ),
    ]);
    for (table, rows) in [
        ("effects", effects),
        ("effects_generalised_descriptions", generalised),
        (
            "frontend_faction_leader_playstyles",
            vec![row! {"key": key}],
        ),
        ("effect_bundles", vec![bundle(&preview)]),
        ("effect_bundles_to_effects_junctions", junctions),
    ] {
        add_rows(pack, table, &suffix, rows)?;
    }
    Ok(())
}

fn scripts(pack: &mut Pack, config: &Config, key: &str) -> Result<()> {
    let site = config.settlement().unwrap();
    let traits = config
        .starting_traits
        .iter()
        .map(|key| format!("\"{key}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let equipment = SLOTS
        .iter()
        .filter_map(|(slot, _)| {
            config.starting_ancillaries.get(*slot).map(|key| {
                format!(
                    "{{\"{key}\", \"3k_main_ceo_category_ancillary_{slot}\", {}}}",
                    ANCILLARIES[key].character_owned
                )
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut rules = include_str!("script/sect_rules.lua").to_owned();
    let gains = doctrines::gains(config.variant);
    for (marker, value) in [
        ("__FACTION_KEY__", key),
        ("__STARTING_TRAITS__", traits.as_str()),
        ("__STARTING_ANCILLARIES__", equipment.as_str()),
        ("__START_REGION__", site.key.as_str()),
        ("__START_OWNER__", if site.han { HAN } else { "rebels" }),
        (
            "__START_BUILDING__",
            site.buildings.as_ref().map_or("", |(_, new, _)| new),
        ),
        (
            "__SECT_MODE__",
            if config.sect_mode { "true" } else { "false" },
        ),
        (
            "__IS_CULT__",
            if config.variant == Variant::Cult {
                "true"
            } else {
                "false"
            },
        ),
        ("__DOCTRINE_GAINS__", gains.as_str()),
        ("__INFLUENCE_TIERS__", "25, 75, 150"),
    ] {
        rules = rules.replace(marker, value);
    }
    add(
        pack,
        &format!("script/campaign/mod/{key}_rules.lua"),
        rules.as_bytes(),
    )?;
    let victory = include_str!("script/sect_victory.lua")
        .replace("__FACTION_KEY__", key)
        .replace(
            "__IS_CULT__",
            if config.variant == Variant::Cult {
                "true"
            } else {
                "false"
            },
        )
        .replace("__CULT_SACK_TARGET__", "30")
        .replace("__HAN_EMPIRE__", HAN);
    add(
        pack,
        &format!("script/campaign/mod/{key}_victory.lua"),
        victory.as_bytes(),
    )
}
