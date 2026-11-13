use anyhow::Result;
use rpfm_lib::files::pack::Pack;

use crate::{
    config::Variant,
    export::{bundle, row},
    game::{Localization, add, add_rows, raw, record_id},
};

struct Choice {
    label: &'static str,
    description: &'static str,
}

const fn choice(label: &'static str, description: &'static str) -> Choice {
    Choice { label, description }
}

struct Dilemma {
    slug: &'static str,
    title: &'static str,
    description: &'static str,
    image: &'static str,
    choices: &'static [Choice],
}

const SECT: &[Dilemma] = &[
    Dilemma {
        slug: "assembly",
        title: "An Assembly of Households",
        description: "Followers have brought offerings to your seat. Decide whether to collect them for the treasury or put your own coin into a new congregation.",
        image: "3k_event_gift",
        choices: &[
            choice("Collect donations", "Add 900 to your treasury."),
            choice(
                "Endow a chapter",
                "Pay 1,000 to gain 6 Influence now and +1 Influence every three turns from this chapter. Maximum five chapters.",
            ),
            choice(
                "Close the assembly",
                "Leave the offerings with the households.",
            ),
        ],
    },
    Dilemma {
        slug: "choose_patron",
        title: "Choose a Patron",
        description: "A warlord has accepted your offer of regular payments. You may sponsor the recipient of that offer. If your chosen patron becomes Emperor or Empress, your sect wins the campaign.",
        image: "3k_event_diplomacy_neutral",
        choices: &[
            choice(
                "Sponsor this warlord",
                "Name the recipient of the payment agreement as your sole patron, replacing any previous patron.",
            ),
            choice(
                "Keep the payment agreement only",
                "Leave your patronage unchanged.",
            ),
        ],
    },
    Dilemma {
        slug: "assembly_donations",
        title: "Offerings from the Households",
        description: "Followers have brought offerings to your seat. Collect them or leave the money with the households.",
        image: "3k_event_gift",
        choices: &[
            choice("Collect donations", "Add 900 to your treasury."),
            choice(
                "Close the assembly",
                "Leave the offerings with the households.",
            ),
        ],
    },
    Dilemma {
        slug: "patron_dispatch",
        title: "A Dispatch from Your Patron",
        description: "Your patron's envoys ask whether your followers can furnish money for the war effort.",
        image: "3k_event_diplomacy_neutral",
        choices: &[
            choice(
                "Send a war chest",
                "Transfer 1,500 from your treasury to the warlord you currently sponsor.",
            ),
            choice(
                "Retain the treasury",
                "Decline the additional contribution.",
            ),
        ],
    },
    Dilemma {
        slug: "household_relief",
        title: "Households in Need",
        description: "Crop failures have left several villages without enough grain. Families that once supplied your gatherings are asking for help.",
        image: "3k_event_food_good",
        choices: &[
            choice(
                "Buy grain for the villages",
                "Pay 600 to prevent the loss of 5 Sect Influence.",
            ),
            choice("Keep the treasury closed", "Lose 5 Sect Influence."),
        ],
    },
    Dilemma {
        slug: "household_relief_unfunded",
        title: "Households in Need",
        description: "Crop failures have left several villages without enough grain. Your treasury cannot cover the relief, and some families are withdrawing their support.",
        image: "3k_event_food_good",
        choices: &[choice("Acknowledge the loss", "Lose 5 Sect Influence.")],
    },
];
const CULT: &[Dilemma] = &[Dilemma {
    slug: "plunder_contract",
    title: "A Fence's Contract",
    description: "The market fences offer to buy the next haul at a premium. Their messengers make your movements harder to conceal.",
    image: "3k_captured_settlement_small_sack_withdraw",
    choices: &[
        choice(
            "Take the contract",
            "Gain 2,500 if you sack and withdraw from a new commandery seat within five turns. While the contract is active, lose 10% campaign movement. Letting it expire costs 5 Cult Influence.",
        ),
        choice("Keep a low profile", "Continue raiding on your own terms."),
    ],
}];

pub fn write(
    game: &Pack,
    pack: &mut Pack,
    loc: &mut Localization,
    variant: Variant,
    key: &str,
) -> Result<()> {
    let dilemmas = match variant {
        Variant::Sect => SECT,
        Variant::Cult => CULT,
    };
    let suffix = format!("{key}_campaign_actions");
    let mut definitions = Vec::new();
    let mut choices = Vec::new();
    let mut options = Vec::new();
    let mut payloads = Vec::new();
    let mut payload_text = Vec::new();
    for dilemma in dilemmas {
        let key = format!("{key}_{}", dilemma.slug);
        add(
            pack,
            &format!("ui/eventpics/{key}.png"),
            &raw(game, &format!("ui/eventpics/{}.png", dilemma.image))?,
        )?;
        definitions.push(row! {"key": key, "generate": false, "localised_description": dilemma.description,
            "localised_title": dilemma.title, "ui_image": key, "event_category": "faction", "prioritized": true,
            "sound_popup_override": "UI_CAM_EVENT_Dilemma"});
        loc.insert(
            format!("dilemmas_localised_title_{key}"),
            dilemma.title.into(),
        );
        loc.insert(
            format!("dilemmas_localised_description_{key}"),
            dilemma.description.into(),
        );
        options.push(row! {"dilemma_key": key, "id": record_id(&key),
        "option_key": "GEN_TARGET_NONE", "target": "default"});
        for (choice, option) in dilemma.choices.iter().zip(["FIRST", "SECOND", "THIRD"]) {
            choices.push(row! {"dilemma_key": key, "choice_key": option});
            let component = format!("{key}_{}", option.to_ascii_lowercase());
            payloads.push(row! {"dilemma_key": key, "choice_key": option,
            "id": record_id(&format!("{key}/{option}")), "payload_key": "TEXT_DISPLAY", "target_key": "default",
            "value": format!("LOOKUP[{component}]")});
            payload_text.push(row! {"component": component, "icon": "ui/campaign ui/effect_bundles/flag.png", "sort_order": 250});
            loc.insert(
                format!("campaign_payload_ui_details_description_{component}"),
                choice.description.into(),
            );
            for (field, text) in [("label", choice.label), ("title", choice.description)] {
                loc.insert(
                    format!(
                        "cdir_events_dilemma_choice_details_localised_choice_{field}_{key}{option}"
                    ),
                    text.into(),
                );
            }
        }
    }
    for (table, records) in [
        ("dilemmas", definitions),
        ("cdir_events_dilemma_choice_details", choices),
        ("cdir_events_dilemma_option_junctions", options),
        ("cdir_events_dilemma_payloads", payloads),
        ("campaign_payload_ui_details", payload_text),
    ] {
        add_rows(pack, table, &suffix, records)?;
    }
    if variant == Variant::Cult {
        let heat = format!("{key}_heat");
        add_rows(pack, "effect_bundles", &suffix, vec![bundle(&heat)])?;
        add_rows(
            pack,
            "effect_bundles_to_effects_junctions",
            &suffix,
            vec![row! {
                "effect_bundle_key": heat, "effect_key": "3k_main_effect_force_all_campaign_movement_range",
                "effect_scope": "faction_to_force_own_unseen", "value": -10, "advancement_stage": "start_turn_completed"
            }],
        )?;
        loc.insert(
            format!("effect_bundles_localised_title_{heat}"),
            "Heat from the Fence's Contract".into(),
        );
        loc.insert(format!("effect_bundles_localised_description_{heat}"),
            "The contract's messengers draw attention to the warband. Campaign movement is reduced by 10% while the contract is active.".into());
    }
    Ok(())
}
