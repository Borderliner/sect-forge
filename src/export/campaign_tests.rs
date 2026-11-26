use super::*;

#[test]
fn white_lotus_keeps_its_starting_city_when_migrated() -> Result<()> {
    let project = include_str!("../../configs/white_lotus.json");
    let current: Config = serde_json::from_str(project)?;
    assert_eq!(current.start_region, "3k_main_jianan_capital");
    let legacy: Config = serde_json::from_str(&project.replace("startRegion", "cityRegion"))?;
    let migrated = serde_json::to_value(legacy)?;
    assert_eq!(migrated["startRegion"], current.start_region);
    assert!(migrated.get("cityRegion").is_none());
    Ok(())
}

#[test]
#[ignore = "requires installed Three Kingdoms, TROM 5.2 and 190 Expanded packs"]
fn export_against_installed_campaigns() -> Result<()> {
    use crate::relationships::{Person, Relationship, Tie};
    let game = Game::open(&game::discover_game().context("Three Kingdoms installation")?)?;
    let out = tempfile::tempdir()?;
    let sample: Config = serde_json::from_str(include_str!("../../configs/white_lotus.json"))?;
    const SPEAR: &str = "3k_ironic_ancillary_weapon_pang_de_spear_unique";
    const PROGRESSION: &str = "script/campaign/_shared/3k_campaign_progression_extended.lua";
    const LIU_BEI: &str = "3k_main_template_historical_liu_bei_hero_earth";
    const GONGSUN_ZAN: &str = "3k_main_template_historical_gongsun_zan_hero_fire";
    let workshop = game::workshop_packs();
    for (trom_compat, expanded_compat) in [(false, false), (true, false), (false, true)] {
        let overlays = if trom_compat {
            trom::packs(&workshop)?.to_vec()
        } else if expanded_compat {
            expanded::packs(&workshop)?.to_vec()
        } else {
            Vec::new()
        };
        let roster = crate::relationships::Roster::read(&game, &overlays)?;
        let liu = roster
            .people
            .get(LIU_BEI)
            .context("Liu Bei must be selectable")?;
        assert_eq!(liu.name, "Liu Bei");
        assert!(liu.male && liu.age.is_some() && !liu.faction.is_empty());
        // 190 - 49 coincides with a donor identifier.
        let mut config = Config {
            founder_age: 49,
            officers: vec![crate::config::Officer {
                surname: "He".into(),
                forename: "Qing".into(),
                age: Some(19),
                character: crate::config::CharacterBuild {
                    subtype: "3k_general_water".into(),
                    art_set: "3k_main_art_set_historical_zheng_jiang_general".into(),
                    starting_career: Some("3k_main_ceo_career_historical_cao_pi".into()),
                    starting_traits: vec!["3k_main_ceo_trait_personality_ambitious".into()],
                    starting_ancillaries: BTreeMap::from([(
                        "follower".into(),
                        "3k_main_ancillary_follower_elite_trainer".into(),
                    )]),
                    ..Default::default()
                },
                legendary: true,
                attributes: BTreeMap::from([(Attribute::Cunning, 107)]),
                female: true,
            }],
            relationships: vec![
                Relationship {
                    first: Person::Founder,
                    tie: Tie::Parent,
                    second: Person::Officer(0),
                },
                Relationship {
                    first: Person::Officer(0),
                    tie: Tie::Friend,
                    second: Person::Historical(LIU_BEI.into()),
                },
                Relationship {
                    first: Person::Historical(GONGSUN_ZAN.into()),
                    tie: Tie::Nemesis,
                    second: Person::Historical(LIU_BEI.into()),
                },
                Relationship {
                    first: Person::Founder,
                    tie: Tie::Oathsworn,
                    second: Person::Officer(0),
                },
            ],
            trom_compat,
            expanded_compat,
            ..sample.clone()
        };
        if expanded_compat {
            config.start_region = "ironic_region_lelang_capital".into();
            config
                .starting_ancillaries
                .insert("weapon".into(), SPEAR.into());
        }
        const RED_HARE: &str = "3k_main_ancillary_mount_red_hare";
        config.character.art_set = "3k_main_art_set_historical_zhao_yun_general".into();
        config.officers[0]
            .starting_ancillaries
            .insert("mount".into(), RED_HARE.into());
        let key = config.key();
        let region = config.start_region.clone();
        let pack = game::open_pack(&build(&game, config, out.path(), |_| {})?)?;
        let rules =
            String::from_utf8(raw(&pack, &format!("script/campaign/mod/{key}_rules.lua"))?)?;
        assert!(rules.contains(&format!("local starting_region = \"{region}\"")));
        assert!(rules.contains(&format!(
            "{{\"{key}_leader\", \"parent\", \"{key}_officer_1\"}}"
        )));
        assert!(rules.contains(&format!("{{\"{key}_officer_1\", \"friend\", \"3k_main_template_historical_liu_bei_hero_earth\"}}")));
        assert!(rules.contains(&format!(
            "{{\"{GONGSUN_ZAN}\", \"nemesis\", \"{LIU_BEI}\"}}"
        )));
        assert!(rules.contains(&format!(
            "{{\"{key}_leader\", \"oathsworn\", \"{key}_officer_1\"}}"
        )));
        for (tie, sign) in [("oathsworn", "positive"), ("nemesis", "negative")] {
            let trigger = format!("{key}_bond_{tie}");
            let effects = game::rows_at(
                &pack,
                &db_path("character_relationship_trigger_effects", &trigger),
            )?;
            assert_eq!(effects.len(), 1);
            assert_eq!(field(&effects[0], "trigger"), trigger);
            assert_eq!(
                field(&effects[0], "effect"),
                format!("relationship_effect_permanent_{sign}_max")
            );
        }
        let officer_key = Officer::key(&key, 0);
        let officer_rows = game::rows_at(
            &pack,
            &db_path("character_generation_templates", &format!("{key}_officers")),
        )?;
        let officer = &officer_rows[0];
        assert_eq!(field(officer, "key"), officer_key);
        assert_eq!(officer["unique"], true);
        assert_eq!(officer["is_male"], false);
        assert_eq!(
            field(officer, "art_set_override"),
            format!("{officer_key}_portrait")
        );
        let art_rows = |name: &str| game::rows_at(&pack, &db_path("campaign_character_arts", name));
        assert_ne!(art_rows(&key)?[0]["id"], art_rows(&officer_key)?[0]["id"]);
        let loadout = rules
            .lines()
            .find(|line| line.contains(&format!(r#""{officer_key}", "3k_general_water""#)))
            .context("Officer loadout")?;
        for key in [
            "3k_main_ceo_trait_personality_ambitious",
            "3k_main_ceo_career_historical_cao_pi",
            "3k_main_ancillary_follower_elite_trainer",
            RED_HARE,
        ] {
            assert!(
                loadout.contains(key),
                "Missing {key} in the officer loadout"
            );
        }
        for mode in Mode::ALL {
            let bytes = raw(
                &pack,
                &format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key()),
            )?;
            let (_, world) = trom::campaign_documents(&bytes)?;
            let owned = world.one(&world.root, "ALL_OWNED_CEOS")?;
            let mounts: std::collections::BTreeSet<_> = owned
                .groups
                .iter()
                .filter(|r| world.text(&r[0]) == Some(RED_HARE))
                .map(|r| world.id(&r[1].fields()[0]))
                .collect::<Result<_>>()?;
            assert!(
                mounts.len() >= 2,
                "Preserve the historical mount and reserve an independent copy"
            );
            let attr = format!("{officer_key}_attributes_{}", mode.key());
            let rows = game::rows_at(&pack, &db_path("character_attributes", &attr))?;
            assert!(
                rows.iter()
                    .any(|r| field(r, "attribute_type") == "cunning" && r["base_value"] == 107)
            );
        }
        let ages = game::rows_at(
            &pack,
            &db_path(
                "character_generation_spawn_age_ranges",
                &format!("{key}_officers"),
            ),
        )?;
        assert!(
            ages.iter()
                .any(|r| field(r, "key") == format!("{key}_officer_1") && r["birth_year"] == 171)
        );
        if expanded_compat {
            assert!(rules.contains(SPEAR));
            let [_, characters] = expanded::packs(&game::workshop_packs())?;
            let native = String::from_utf8(raw(&game::open_pack(&characters)?, PROGRESSION)?)?;
            let generated = String::from_utf8(raw(&pack, PROGRESSION)?)?;
            let anchor = "local function get_progression_data(faction_key)";
            let (prefix, rest) = native
                .split_once(anchor)
                .context("Native progression lookup")?;
            let (patched, suffix) = generated
                .split_once(anchor)
                .context("Exported progression lookup")?;
            let addition = patched
                .strip_prefix(prefix)
                .context("Overhaul progression was replaced")?;
            assert!(addition.contains(&format!("point_keys[\"{key}\"]")));
            assert_eq!(suffix, rest);
        }
    }
    Ok(())
}
