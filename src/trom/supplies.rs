use anyhow::{Context, Result};
use rpfm_lib::files::{Container, pack::Pack};
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::game::{add, raw};

const TOTALS: &str = concat!(
    "CcoRTMoraleBreakdown(Name:Loc(\"campaign_morale_title_tooltip\"), ",
    "Description:Loc(\"morale_description\"), Total:{RoundFloat(PredictedMorale - CurrentMorale)}, ",
    "CurrentValue:{RoundFloat(CurrentMorale)}, ValueMax:{100})",
);

pub fn write_tooltips(game: &Pack, trom: &Pack, output: &mut Pack, faction: &str) -> Result<()> {
    for panel in [
        "army_id",
        "army_id_mid",
        "units_panel",
        "context_list_armies",
    ] {
        let path = format!("ui/campaign ui/{panel}.twui.xml");
        let source = if trom.file(&path, false).is_some() {
            trom
        } else {
            game
        };
        let bytes = raw(source, &path)?;
        let mut root = Element::parse(bytes.as_slice()).with_context(|| format!("Read {path}"))?;
        if replace_supplies_tooltip(&mut root, faction) {
            let mut patched = Vec::new();
            root.write_with_config(&mut patched, EmitterConfig::new().perform_indent(false))?;
            add(output, &path, &patched)?;
        }
    }
    Ok(())
}

fn replace_supplies_tooltip(element: &mut Element, faction: &str) -> bool {
    if element.name == "callback_with_context"
        && element.attributes.get("callback_id").map(String::as_str)
            == Some("ContextCustomTooltipStore")
        && element
            .attributes
            .get("context_object_id")
            .map(String::as_str)
            == Some("CcoCampaignMilitaryForceMorale")
        && let Some(function) = element.attributes.get_mut("context_function_id")
        && function.trim() == "SuppliesTooltip"
    {
        // The native breakdown can dereference a prestige key as a string.
        *function = format!(
            "GetIfElse(PlayersFaction.FactionRecordContext.Key == \"{faction}\", {TOTALS}, SuppliesTooltip)"
        );
        return true;
    }

    let mut changed = false;
    for child in element
        .children
        .iter_mut()
        .filter_map(XMLNode::as_mut_element)
    {
        changed |= replace_supplies_tooltip(child, faction);
    }
    changed
}
