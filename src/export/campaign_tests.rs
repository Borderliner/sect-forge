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
                subtype: "3k_general_water".into(),
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
                    first: Person::Historical(
                        "3k_main_template_historical_gongsun_zan_hero_fire".into(),
                    ),
                    tie: Tie::Rival,
                    second: Person::Historical(LIU_BEI.into()),
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
