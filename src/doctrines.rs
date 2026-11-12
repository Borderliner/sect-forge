use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rpfm_lib::files::{Container, pack::Pack};
use uuid::Uuid;
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::{
    config::{CAMPAIGN, Config, Variant},
    export::{TERRITORY, bundle, dummy_effect, row},
    game::{Localization, add, add_rows, field, raw, rows, set},
};

pub struct Doctrine {
    pub slug: &'static str,
    name: &'static str,
    description: &'static str,
    gain: i32,
    effect: &'static str,
    value: i32,
}

const ORDER: &str = "3k_main_effect_province_public_order_technologies";
const FOOD: &str = "3k_main_effect_province_food_mod";
const INDUSTRY: &str = "3k_main_effect_economy_gdp_mod_industry";
const TRADE: &str = "3k_main_effect_economy_gdp_mod_commerce";
const PEASANTRY: &str = "3k_main_effect_economy_gdp_mod_peasantry";
const RECRUIT: &str = "3k_main_effect_force_all_campaign_recruitment_cost_all";
const SUPPLIES: &str = "3k_main_effect_force_military_supplies_from_reforms";
const MOVE: &str = "3k_main_effect_force_all_campaign_movement_range";
const REPLENISH: &str = "3k_main_effect_replenishment_technologies";
const BACKGROUND: &str = "3k_main_effect_economy_character_wealth_faction_background_income_mod";

const fn reform(
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    gain: i32,
    effect: &'static str,
    value: i32,
) -> Doctrine {
    Doctrine {
        slug,
        name,
        description,
        gain,
        effect,
        value,
    }
}

// Each scroll opens two paths and reunites them at its final reform.
const PARENTS: [&[usize]; 8] = [&[], &[0], &[0], &[1], &[2], &[3], &[4], &[5, 6]];
const STAGES: [i32; 8] = [0, 1, 1, 2, 2, 3, 3, 4];

const SECT: [[Doctrine; 8]; 3] = [
    [
        reform(
            "lay_teachers",
            "Lay Teachers",
            "Send teachers beyond your commandery to earn support.",
            1,
            ORDER,
            1,
        ),
        reform(
            "relief_kitchens",
            "Relief Kitchens",
            "Feed travellers and households through village kitchens.",
            0,
            FOOD,
            5,
        ),
        reform(
            "market_stewards",
            "Market Stewards",
            "Entrust the market wards to local congregations.",
            0,
            TRADE,
            8,
        ),
        reform(
            "village_commons",
            "Village Commons",
            "Pool tools and labour to sustain farms and settlements.",
            0,
            PEASANTRY,
            10,
        ),
        reform(
            "open_sanctuaries",
            "Open Sanctuaries",
            "Turn local sanctuaries into permanent civic institutions.",
            1,
            ORDER,
            2,
        ),
        reform(
            "communal_granaries",
            "Communal Granaries",
            "Keep larger grain reserves for a siege or failed harvest.",
            0,
            FOOD,
            10,
        ),
        reform(
            "chartered_workshops",
            "Chartered Workshops",
            "Finance experienced artisans and the town's advanced workshops.",
            0,
            INDUSTRY,
            12,
        ),
        reform(
            "living_community",
            "The Living Community",
            "Put the congregation's prosperity behind a lasting school.",
            2,
            ORDER,
            2,
        ),
    ],
    [
        reform(
            "safehouses",
            "Safehouse Network",
            "Give travelling messengers a secure place to rest.",
            1,
            SUPPLIES,
            3,
        ),
        reform(
            "village_levies",
            "Village Levies",
            "Train and arm dependable local recruits.",
            0,
            RECRUIT,
            -5,
        ),
        reform(
            "wardens",
            "Sanctuary Wardens",
            "Teach household guards to defend routes and settlements.",
            0,
            REPLENISH,
            2,
        ),
        reform(
            "retinue_drills",
            "Retinue Drills",
            "Standardise training for dedicated infantry and cavalry.",
            0,
            SUPPLIES,
            5,
        ),
        reform(
            "quartermasters",
            "Quartermaster Registers",
            "Stock district armouries and train their keepers.",
            0,
            RECRUIT,
            -5,
        ),
        reform(
            "veteran_companies",
            "Veteran Companies",
            "Organise veteran companies around experienced officers.",
            0,
            REPLENISH,
            3,
        ),
        reform(
            "sanctuary_armouries",
            "Sanctuary Armouries",
            "Maintain advanced equipment and guarded supply convoys.",
            0,
            SUPPLIES,
            7,
        ),
        reform(
            "oathbound_guard",
            "The Oathbound Guard",
            "Train the sect's most experienced retinues to hold the line.",
            0,
            RECRUIT,
            -6,
        ),
    ],
    [
        reform(
            "patrons",
            "Patronage Circles",
            "Organise households that can support a warlord abroad.",
            1,
            BACKGROUND,
            4,
        ),
        reform(
            "pilgrim_roads",
            "Pilgrim Roads",
            "Keep a network of waystations for pilgrims and escorts.",
            0,
            MOVE,
            3,
        ),
        reform(
            "merchant_letters",
            "Letters of Credit",
            "Let merchant houses honour each other's contracts.",
            0,
            TRADE,
            7,
        ),
        reform(
            "travelling_delegates",
            "Travelling Delegates",
            "Send delegates to keep distant schools in contact.",
            1,
            MOVE,
            4,
        ),
        reform(
            "provincial_brokers",
            "Provincial Brokers",
            "Use established markets to arrange long-distance trade.",
            0,
            TRADE,
            10,
        ),
        reform(
            "banner_subscriptions",
            "Banner Subscriptions",
            "Raise recurring donations to support your patron.",
            1,
            BACKGROUND,
            6,
        ),
        reform(
            "supply_contracts",
            "Supply Contracts",
            "Secure specialist materials through outside suppliers.",
            0,
            INDUSTRY,
            8,
        ),
        reform(
            "mandate_witnesses",
            "Witnesses to the Mandate",
            "Mobilise your supporters behind the patron's claim.",
            2,
            BACKGROUND,
            8,
        ),
    ],
];

const CULT: [[Doctrine; 8]; 3] = [
    [
        reform(
            "lookouts",
            "Roadside Lookouts",
            "Find unguarded roads and weak approaches to new targets.",
            0,
            MOVE,
            3,
        ),
        reform(
            "ransom_ledgers",
            "Ransom Ledgers",
            "Keep records of saleable goods taken from new commandery seats.",
            0,
            TRADE,
            6,
        ),
        reform(
            "pack_trains",
            "Hidden Pack Trains",
            "Keep raiders provisioned between settlements.",
            0,
            SUPPLIES,
            4,
        ),
        reform(
            "river_crossings",
            "Night Crossings",
            "Lead warbands across rivers by routes known to smugglers.",
            0,
            MOVE,
            4,
        ),
        reform(
            "ravine_camps",
            "Ravine Camps",
            "Rest wounded raiders under cover after an assault.",
            0,
            REPLENISH,
            2,
        ),
        reform(
            "siege_crews",
            "Siege Crews",
            "Keep equipment and experienced hands ready for the next assault.",
            0,
            RECRUIT,
            -6,
        ),
        reform(
            "forced_marches",
            "Forced Marches",
            "Move between commandery seats before their allies arrive.",
            0,
            MOVE,
            5,
        ),
        reform(
            "rolling_plunder",
            "Rolling Plunder",
            "Maintain a reserve from every new seat your warband sacks.",
            0,
            SUPPLIES,
            6,
        ),
    ],
    [
        reform(
            "gang_oaths",
            "Gang Oaths",
            "Organise new followers into reliable warbands.",
            0,
            RECRUIT,
            -4,
        ),
        reform(
            "outlaw_drills",
            "Outlaw Drills",
            "Drill recruits in fighting as an organised retinue.",
            0,
            SUPPLIES,
            4,
        ),
        reform(
            "hidden_infirmaries",
            "Hidden Infirmaries",
            "Keep healers and clean bedding behind the front.",
            0,
            REPLENISH,
            2,
        ),
        reform(
            "warband_companies",
            "Warband Companies",
            "Maintain standing companies drawn from several gangs.",
            0,
            RECRUIT,
            -5,
        ),
        reform(
            "black_forges",
            "Black Forges",
            "Concentrate smiths and armourers at your stronghold.",
            0,
            INDUSTRY,
            9,
        ),
        reform(
            "hardened_companies",
            "Hardened Companies",
            "Give seasoned fighters a permanent place in the warband.",
            0,
            REPLENISH,
            3,
        ),
        reform(
            "outlaw_armouries",
            "Outlaw Armouries",
            "Maintain advanced workshops for your assault companies.",
            0,
            SUPPLIES,
            7,
        ),
        reform(
            "red_banners",
            "The Red Banners",
            "Muster elite troops under the banner of the strongest company.",
            0,
            RECRUIT,
            -6,
        ),
    ],
    [
        reform(
            "fences",
            "Market Fences",
            "Keep local buyers ready to take captured goods.",
            0,
            TRADE,
            6,
        ),
        reform(
            "tribute_houses",
            "Tribute Houses",
            "Collect regular protection payments at the stronghold.",
            0,
            BACKGROUND,
            5,
        ),
        reform(
            "hidden_storehouses",
            "Hidden Storehouses",
            "Keep a reserve to outlast sieges and poor harvests.",
            0,
            FOOD,
            5,
        ),
        reform(
            "night_markets",
            "Night Markets",
            "Connect hidden markets to the road network.",
            0,
            TRADE,
            8,
        ),
        reform(
            "stockades",
            "Stockade Settlements",
            "Organise markets, workshops and farms around defended outposts.",
            0,
            ORDER,
            2,
        ),
        reform(
            "outlaw_mint",
            "Outlaw Mint",
            "Pay warbands from a permanent treasury.",
            0,
            BACKGROUND,
            7,
        ),
        reform(
            "smuggler_contracts",
            "Smuggler Contracts",
            "Arrange outside supplies for specialist workshops.",
            0,
            INDUSTRY,
            8,
        ),
        reform(
            "sanctuary_of_thieves",
            "Sanctuary of Thieves",
            "Turn the stronghold into a regional refuge for outlaws.",
            0,
            ORDER,
            3,
        ),
    ],
];

pub fn tree(variant: Variant) -> &'static [[Doctrine; 8]; 3] {
    match variant {
        Variant::Sect => &SECT,
        Variant::Cult => &CULT,
    }
}

pub fn gains(variant: Variant) -> String {
    tree(variant)
        .iter()
        .flatten()
        .filter(|node| node.gain > 0)
        .map(|node| format!("{} = {}", node.slug, node.gain * 3))
        .collect::<Vec<_>>()
        .join(", ")
}

fn effect_scope(effect: &str) -> &'static str {
    match effect {
        ORDER => "faction_to_province_own_unseen",
        FOOD | INDUSTRY | TRADE | PEASANTRY => "faction_to_region_own_unseen",
        SUPPLIES | MOVE | REPLENISH => "faction_to_force_own_unseen",
        RECRUIT => "faction_to_character_own",
        _ => "faction_to_faction_own",
    }
}

const HAN_DONORS: [&str; 3] = [
    "3k_main_tech_wood_tier0_resettlement_incentives",
    "3k_main_tech_fire_tier0_regional_commissioners",
    "3k_main_tech_water_tier0_foreign_envoys",
];
const BANDIT_DONORS: [&str; 3] = [
    "3k_dlc05_tech_bandit_network_bing_yanmen",
    "3k_dlc05_tech_bandit_network_bing_xihe",
    "3k_dlc05_tech_bandit_network_bing_shangdang",
];

const SECT_TIERS: [(i32, i32); 3] = [(25, 10), (75, 25), (150, 50)];
const CULT_TIERS: [(i32, i32); 3] = [(10, 5), (25, 10), (50, 20)];

fn influence_tiers(variant: Variant) -> [(i32, i32); 3] {
    match variant {
        Variant::Sect => SECT_TIERS,
        Variant::Cult => CULT_TIERS,
    }
}

pub fn influence_thresholds(variant: Variant) -> String {
    influence_tiers(variant)
        .map(|(threshold, _)| threshold.to_string())
        .join(", ")
}
const TECH_TREE: &str = "ui/tech_trees/tech_tree.twui.xml";

pub const BANDIT_TREE: &str = "ui/tech_trees/tech_tree_3k_dlc05_subculture_bandits.twui.xml";

pub fn write(
    game: &Pack,
    trom: Option<&Pack>,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
) -> Result<()> {
    let suffix = format!("{key}_doctrines");
    let cult = config.variant == Variant::Cult;
    if !cult {
        let sets = rows(game, "technology_node_sets")?
            .into_iter()
            .filter(|row| {
                field(row, "campaign_key") == CAMPAIGN
                    && field(row, "subculture") == "3k_main_chinese"
            })
            .map(|mut row| {
                let id = format!("{key}_{}", field(&row, "technology_category"));
                set(&mut row, "key", id);
                set(&mut row, "faction_key", key);
                row
            })
            .collect();
        add_rows(pack, "technology_node_sets", &suffix, sets)?;
    }
    let techs = rows(game, "technologies")?;
    let nodes = rows(game, "technology_nodes")?;
    let donors = if cult { BANDIT_DONORS } else { HAN_DONORS };
    let mut technologies = Vec::new();
    let mut technology_nodes = Vec::new();
    let mut links = Vec::new();
    let mut junctions = Vec::new();
    for (branch, scroll) in tree(config.variant).iter().enumerate() {
        let donor = donors[branch];
        let template = techs
            .iter()
            .find(|row| field(row, "key") == donor)
            .with_context(|| format!("Missing technology donor {donor}"))?;
        let node_template = nodes
            .iter()
            .find(|row| {
                field(row, "technology_key") == donor && field(row, "campaign_key").is_empty()
            })
            .with_context(|| format!("Missing research node donor {donor}"))?;
        let set_key = if cult {
            field(node_template, "technology_node_set").to_owned()
        } else {
            format!("{key}_3k_main_tech_cat_wood")
        };
        for (index, doctrine) in scroll.iter().enumerate() {
            let name = format!("{key}_{}", doctrine.slug);
            let mut tech = template.clone();
            set(&mut tech, "key", name.as_str());
            let uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, name.as_bytes());
            let unique = i32::from_le_bytes(uuid.as_bytes()[..4].try_into().unwrap()) & i32::MAX;
            set(&mut tech, "unique_index", unique);
            technologies.push(tech);
            let mut node = node_template.clone();
            for (column, value) in [
                ("key", name.as_str()),
                ("technology_key", &name),
                ("faction_key", key),
                ("technology_node_set", &set_key),
            ] {
                set(&mut node, column, value);
            }
            set(&mut node, "tier", STAGES[index]);
            set(&mut node, "indent", index as i32);
            set(
                &mut node,
                "research_points_required",
                if cult { 250 + 100 * STAGES[index] } else { 0 },
            );
            set(
                &mut node,
                "cooldown_rounds_until_next_research",
                if cult { 0 } else { 3 },
            );
            set(
                &mut node,
                "required_num_parents",
                PARENTS[index].len() as i32,
            );
            set(
                &mut node,
                "can_meet_parent_requirement_with_children",
                false,
            );
            technology_nodes.push(node);
            for parent in PARENTS[index] {
                links.push(row!{"parent_key": format!("{key}_{}", scroll[*parent].slug), "child_key": name,
                    "initial_descent_tiers": 0, "parent_link_position": 0, "child_link_position": 0,
                    "parent_link_position_offset": 0.0, "child_link_position_offset": 0.0});
            }
            junctions.push(row! {"technology": name, "effect": doctrine.effect,
            "effect_scope": effect_scope(doctrine.effect), "value": doctrine.value});
            if doctrine.gain > 0 {
                junctions.push(
                    row! {"technology": name, "effect": format!("{key}_influence_growth"),
                    "effect_scope": "faction_to_faction_own", "value": doctrine.gain * 3},
                );
            }
            let mut description = doctrine.description.to_owned();
            if doctrine.gain > 0 {
                description.push_str(&format!(
                    " Grants +{} Sect Influence after research.",
                    doctrine.gain * 3
                ));
            }
            if let Some(extra) = match doctrine.slug {
                "ransom_ledgers" => {
                    Some("Gain 500 extra income for each newly sacked commandery seat.")
                }
                "rolling_plunder" => {
                    Some("Gain another 750 income for each newly sacked commandery seat.")
                }
                _ => None,
            } {
                description.push(' ');
                description.push_str(extra);
            }
            loc.insert(
                format!("technologies_onscreen_name_{name}"),
                doctrine.name.into(),
            );
            loc.insert(
                format!("technologies_short_description_{name}"),
                description.clone(),
            );
            loc.insert(format!("technologies_long_description_{name}"), description);
            let mask = format!("ui/campaign ui/technology_masks/{donor}.png");
            if game.file(&mask, true).is_some() {
                add(
                    pack,
                    &format!("ui/campaign ui/technology_masks/{name}.png"),
                    &raw(game, &mask)?,
                )?;
            }
        }
    }
    for (table, records) in [
        ("technologies", technologies),
        ("technology_nodes", technology_nodes),
        ("technology_node_links", links),
        ("technology_effects_junction", junctions),
    ] {
        add_rows(pack, table, &suffix, records)?;
    }
    let path = if cult { BANDIT_TREE } else { TECH_TREE };
    let source = trom
        .filter(|pack| cult && pack.file(path, false).is_some())
        .unwrap_or(game);
    add(
        pack,
        path,
        &layout(&raw(source, path)?, key, config.variant, &donors)?,
    )?;
    influence(game, pack, loc, config, key, &suffix)?;
    Ok(())
}

fn influence(
    game: &Pack,
    pack: &mut Pack,
    loc: &mut Localization,
    config: &Config,
    key: &str,
    suffix: &str,
) -> Result<()> {
    let resource = format!("{key}_influence");
    let factor = format!("{key}_teachings");
    let setbacks = format!("{key}_setbacks");
    let group = format!("{key}_influence_group");
    let member = format!("{group}_member");
    let mut effect = dummy_effect(game)?;
    set(&mut effect, "effect", format!("{key}_influence_growth"));
    let mut bundles = Vec::new();
    let mut tiers = Vec::new();
    let cult = config.variant == Variant::Cult;
    let (name, income_effect, income_scope, income_label) = if cult {
        (
            "Cult Influence",
            "3k_main_effect_force_all_campaign_loot_income",
            "faction_to_character_own_unseen",
            "settlement looting income",
        )
    } else {
        (
            "Sect Influence",
            "3k_main_effect_economy_character_wealth_faction_background_income_mod",
            "faction_to_faction_own",
            "background income",
        )
    };
    for (threshold, bonus) in influence_tiers(config.variant) {
        let id = format!("{resource}_{threshold}");
        bundles.push(bundle(&id));
        tiers.push(row! {"effect_bundle_key": id, "effect_key": income_effect,
        "effect_scope": income_scope, "value": bonus, "advancement_stage": "start_turn_completed"});
        loc.insert(
            format!("effect_bundles_localised_title_{id}"),
            format!("{name} · {threshold}+"),
        );
        loc.insert(
            format!("effect_bundles_localised_description_{id}"),
            format!("+{bonus}% {income_label}."),
        );
    }
    let territory = if config.sect_mode {
        format!("\n\n{TERRITORY} Influence and doctrines follow the faction.")
    } else {
        String::new()
    };
    let description = if cult {
        "Gain 5 Influence for each newly sacked commandery seat. Lose 1 each turn and 5 when a fence's contract expires. At 10 / 25 / 50 Influence, gain +5% / +10% / +20% settlement looting income."
    } else {
        "Gain 1 Influence per turn, plus income from Meeting Halls and one-time gains from doctrines and chapters. Lose 6 if another faction captures one of your settlements, 10 if your patron falls, and 5 if households go without relief. At 25 / 75 / 150 Influence, gain +10% / +25% / +50% background income."
    };
    loc.extend([
        (
            format!("pooled_resources_display_name_{resource}"),
            name.into(),
        ),
        (
            format!("pooled_resources_description_{resource}"),
            format!("{description}{territory}"),
        ),
        (
            format!("pooled_resources_positive_factors_display_name_{resource}"),
            "Influence gained".into(),
        ),
        (
            format!("pooled_resources_negative_factors_display_name_{resource}"),
            "Influence lost".into(),
        ),
        (
            format!("pooled_resource_factors_display_name_positive_{factor}"),
            if cult {
                "Plunder"
            } else {
                "Teachings and chapters"
            }
            .into(),
        ),
        (
            format!("pooled_resource_factors_display_name_negative_{setbacks}"),
            "Setbacks".into(),
        ),
        (
            format!("effects_description_{key}_influence_growth"),
            "+%n Sect Influence on completion".into(),
        ),
    ]);
    for (table, rows) in [
        (
            "pooled_resources",
            vec![
                row! {"key": resource, "default_factor": factor, "persistent_factors": false,
                "ai_ignored": false, "minimum": 0, "maximum": 1000, "overflow_behaviour": "DISCARD", "reset_policy": "NONE",
                "icon_path": "ui/campaign ui/effect_bundles/technology.png", "ui_value_scale_factor": 1.0, "income_policy": "ECONOMICS_UPDATE", "shared": false},
            ],
        ),
        ("pooled_resource_factors", vec![row! {"key": factor}, row! {"key": setbacks}]),
        (
            "pooled_resource_factor_junctions",
            [factor, setbacks].map(|factor| {
                row! {"key": format!("{factor}_junction"), "factor": factor, "resource": resource,
                "minimum": -1000, "maximum": 1000, "ai_can_propose": false, "is_recurring_factor": false}
            }).to_vec(),
        ),
        ("campaign_groups", vec![row! {"id": group}]),
        (
            "campaign_group_members",
            vec![row! {"id": member, "group": group, "priority": 1.0}],
        ),
        (
            "campaign_group_member_criteria_factions",
            vec![row! {"member": member, "faction": key, "context": "owner"}],
        ),
        (
            "campaign_group_member_criteria_pooled_resource_scopes",
            vec![row! {"member": member, "scope": "FACTION"}],
        ),
        (
            "campaign_group_pooled_resources",
            vec![row! {"campaign_group": group, "resource": resource, "initial_amount": 0}],
        ),
        (
            "ui_primary_faction_pooled_resources",
            vec![
                row! {"faction": key, "optional_campaign_key": CAMPAIGN, "pooled_resource": resource},
            ],
        ),
        ("effects", vec![effect]),
        ("effect_bundles", bundles),
        ("effect_bundles_to_effects_junctions", tiers),
    ] {
        add_rows(pack, table, suffix, rows)?;
    }
    Ok(())
}

fn walk(element: &Element) -> impl Iterator<Item = &Element> {
    fn collect<'a>(element: &'a Element, output: &mut Vec<&'a Element>) {
        output.push(element);
        for child in element.children.iter().filter_map(XMLNode::as_element) {
            collect(child, output);
        }
    }
    let mut elements = Vec::new();
    collect(element, &mut elements);
    elements.into_iter()
}

fn find_mut<'a>(element: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    if element.name == name {
        return Some(element);
    }
    element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
        .find_map(|child| find_mut(child, name))
}

fn rewrite(element: &mut Element, ids: &BTreeMap<String, String>, donor: &str, technology: &str) {
    element.name = element.name.replace(donor, technology);
    for value in element.attributes.values_mut() {
        *value = ids
            .get(value)
            .cloned()
            .unwrap_or_else(|| value.replace(donor, technology));
    }
    for child in element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
    {
        rewrite(child, ids, donor, technology);
    }
}

fn layout(bytes: &[u8], key: &str, variant: Variant, donors: &[&str; 3]) -> Result<Vec<u8>> {
    let cult = variant == Variant::Cult;
    let holder = if cult {
        "bandit_tech_tree_nodes_holder"
    } else {
        "tech_tree_component"
    };
    let mut root = Element::parse(bytes).context("Read vanilla research layout")?;
    let components = root
        .get_child("components")
        .context("Missing research components")?;
    let definitions = components
        .children
        .iter()
        .filter_map(XMLNode::as_element)
        .filter_map(|node| Some((node.attributes.get("this")?, node)))
        .collect::<BTreeMap<_, _>>();
    let hierarchy = root
        .get_child("hierarchy")
        .context("Missing research hierarchy")?;
    let tree_holder = walk(hierarchy)
        .find(|node| node.name == holder)
        .context("Missing research tree")?;
    let mut new_nodes = Vec::new();
    let mut new_definitions = Vec::new();
    for (branch, scroll) in tree(variant).iter().enumerate() {
        let donor = donors[branch];
        for (index, doctrine) in scroll.iter().enumerate() {
            let technology = format!("{key}_{}", doctrine.slug);
            let mut node = tree_holder
                .get_child(format!("_{donor}").as_str())
                .context("Missing research node")?
                .clone();
            let mut copies = Vec::new();
            for member in walk(&node) {
                let id = member
                    .attributes
                    .get("this")
                    .context("Missing research node identifier")?;
                copies.push(
                    (*definitions
                        .get(id)
                        .context("Missing research node definition")?)
                    .clone(),
                );
            }
            let mut ids = BTreeMap::new();
            for block in &copies {
                for element in walk(block) {
                    for field in ["this", "uniqueguid"] {
                        if let Some(old) = element.attributes.get(field).filter(|id| !id.is_empty())
                        {
                            ids.entry(old.clone()).or_insert_with(|| {
                                let id = Uuid::new_v5(
                                    &Uuid::NAMESPACE_URL,
                                    format!("sectforge/{technology}/{old}").as_bytes(),
                                )
                                .to_string()
                                .to_uppercase();
                                format!("{}{}", &id[..18], &id[19..])
                            });
                        }
                    }
                }
            }
            rewrite(&mut node, &ids, donor, &technology);
            new_nodes.push(XMLNode::Element(node));
            for mut copy in copies {
                rewrite(&mut copy, &ids, donor, &technology);
                if copy.attributes.get("id") == Some(&technology) {
                    let fork = [0, -1, 1, -1, 1, -1, 1, 0][index];
                    let (x, y) = if cult {
                        // Keep the new branches clear of native locations on both maps.
                        let (x, y) = [(40, 55), (0, 475), (0, 980)][branch];
                        (x + STAGES[index] * 55, y + fork * 30)
                    } else {
                        (
                            470 + STAGES[index] * 156,
                            265 + branch as i32 * 175 + fork * 45,
                        )
                    };
                    copy.attributes
                        .insert("offset".into(), format!("{x}.00,{y}.00"));
                }
                new_definitions.push(XMLNode::Element(copy));
            }
        }
    }
    find_mut(root.get_mut_child("hierarchy").unwrap(), holder)
        .unwrap()
        .children
        .extend(new_nodes);
    root.get_mut_child("components")
        .unwrap()
        .children
        .extend(new_definitions);
    let mut output = Vec::new();
    root.write_with_config(&mut output, EmitterConfig::new().perform_indent(false))?;
    Ok(output)
}
