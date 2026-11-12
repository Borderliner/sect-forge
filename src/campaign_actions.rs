use anyhow::Result;
use rpfm_lib::files::pack::Pack;
use uuid::Uuid;

use crate::{
    config::Variant,
    export::{bundle, row},
    game::{Localization, add_rows},
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
    choices: &'static [Choice],
}

const SECT: &[Dilemma] = &[
    Dilemma {
        slug: "assembly",
        title: "An Assembly of Households",
        description: "Followers have brought offerings to your seat. Decide whether to collect them for the treasury or put your own coin into a new congregation.",
        choices: &[
            choice("Collect donations", "Add 900 to your treasury."),
            choice(
                "Endow a chapter",
                "Pay 1,000 to establish a chapter. Gain 6 Influence and +1 Influence per turn. Up to five chapters.",
            ),
            choice(
                "Close the assembly",
                "Leave the offerings with the households.",
            ),
        ],
    },
    Dilemma {
        slug: "assembly_donations",
        title: "Offerings from the Households",
        description: "Followers have brought offerings to your seat. Collect them or leave the money with the households.",
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
        choices: &[
            choice(
                "Send a war chest",
                "Transfer 1,500 from your treasury to the warlord you currently sponsor.",
            ),
            choice(
                "Retain the treasury",
                "Continue the existing diplomatic payment agreement.",
            ),
        ],
    },
];
const CULT: &[Dilemma] = &[Dilemma {
    slug: "plunder_contract",
    title: "A Fence's Contract",
    description: "The market fences offer to buy the next haul at a premium. Their messengers make your movements harder to conceal.",
    choices: &[
        choice(
            "Take the contract",
            "Gain 2,500 if you sack and withdraw from a new commandery seat within five turns. Your armies lose 10% campaign movement for the duration of the contract.",
        ),
        choice("Keep a low profile", "Continue raiding on your own terms."),
    ],
}];

fn id(value: &str) -> i32 {
    let hash = Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes());
    i32::from_le_bytes(hash.as_bytes()[..4].try_into().unwrap()) & i32::MAX
}

pub fn write(pack: &mut Pack, loc: &mut Localization, variant: Variant, key: &str) -> Result<()> {
    let dilemmas = match variant {
        Variant::Sect => SECT,
        Variant::Cult => CULT,
    };
    let suffix = format!("{key}_campaign_actions");
    let mut definitions = Vec::new();
    let mut choices = Vec::new();
    let mut options = Vec::new();
    let mut payloads = Vec::new();
    for dilemma in dilemmas {
        let key = format!("{key}_{}", dilemma.slug);
        definitions.push(row! {"key": key, "generate": false, "localised_description": dilemma.description,
            "localised_title": dilemma.title, "ui_image": "3k_event_gift", "event_category": "faction",
            "sound_popup_override": "UI_CAM_EVENT_Dilemma"});
        options.push(row! {"dilemma_key": key, "id": id(&key),
        "option_key": "GEN_TARGET_NONE", "target": "default"});
        for (choice, option) in dilemma.choices.iter().zip(["FIRST", "SECOND", "THIRD"]) {
            choices.push(row! {"dilemma_key": key, "choice_key": option});
            payloads.push(row! {"dilemma_key": key, "choice_key": option,
            "id": id(&format!("{key}/{option}")), "payload_key": "DUMMY", "target_key": "default"});
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
