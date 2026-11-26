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
    let game = Game::open(&game::discover_game().context("Three Kingdoms installation")?)?;
    let out = tempfile::tempdir()?;
    let sample: Config = serde_json::from_str(include_str!("../../configs/white_lotus.json"))?;
    const SPEAR: &str = "3k_ironic_ancillary_weapon_pang_de_spear_unique";
    const PROGRESSION: &str = "script/campaign/_shared/3k_campaign_progression_extended.lua";
    for (trom_compat, expanded_compat) in [(false, false), (true, false), (false, true)] {
        // 190 - 49 coincides with a donor identifier.
        let mut config = Config {
            founder_age: 49,
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
