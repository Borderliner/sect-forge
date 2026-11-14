use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::{Container, pack::Pack};

use crate::{
    caab::{Document, Edit},
    config::{CAMPAIGN, Warlord, WarlordFamily},
    game::{Row, add, add_rows_like, field, raw, record_id, row_keys, rows_at, set},
    startpos::{compression_record, open_world},
};

pub struct Playstyle {
    pub faction: Row,
    pub leader: Row,
    pub group: String,
}

pub struct Sources<'a> {
    retail: &'a Pack,
    overlay: Option<&'a Pack>,
}

impl<'a> Sources<'a> {
    pub fn new(retail: &'a Pack, overlay: Option<&'a Pack>) -> Self {
        Self { retail, overlay }
    }

    fn file(&self, path: &str) -> &'a Pack {
        self.overlay
            .filter(|pack| pack.file(path, false).is_some())
            .unwrap_or(self.retail)
    }

    fn tables(&self, table: &str) -> Vec<(String, &'a Pack)> {
        let prefix = format!("db/{table}_tables/");
        self.retail
            .files()
            .keys()
            .chain(
                self.overlay
                    .into_iter()
                    .flat_map(|pack| pack.files().keys()),
            )
            .filter(|path| path.starts_with(&prefix))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|path| (path.clone(), self.file(path)))
            .collect()
    }

    fn rows(&self, table: &str) -> Result<Vec<Row>> {
        let mut effective = BTreeMap::new();
        for (path, source) in self.tables(table) {
            if crate::game::table_is_empty(source, &path)? {
                continue;
            }
            let keys = row_keys(source, &path)?;
            for row in rows_at(source, &path)? {
                let id = if keys.is_empty() {
                    row.values().map(ToString::to_string).collect::<Vec<_>>()
                } else {
                    keys.iter()
                        .map(|key| row[key].to_string())
                        .collect::<Vec<_>>()
                };
                effective.insert(id, row);
            }
        }
        Ok(effective.into_values().collect())
    }

    pub fn playstyle(&self, donor: &Warlord) -> Result<Playstyle> {
        let find = |table: &str, column: &str, id: &str| -> Result<Row> {
            self.rows(table)?
                .into_iter()
                .find(|row| field(row, column) == id)
                .with_context(|| format!("Missing {id} in {table}"))
        };
        let faction = find("factions", "key", &donor.faction)?;
        let junction = self
            .rows("frontend_faction_to_frontend_faction_leaders")?
            .into_iter()
            .find(|row| {
                field(row, "frontend_faction") == donor.faction
                    && field(row, "campaign_key") == CAMPAIGN
            })
            .map(Ok)
            .unwrap_or_else(|| {
                Sources::new(self.retail, None)
                    .rows("frontend_faction_to_frontend_faction_leaders")?
                    .into_iter()
                    .find(|row| {
                        field(row, "frontend_faction") == donor.faction
                            && field(row, "campaign_key") == CAMPAIGN
                    })
                    .with_context(|| format!("{} has no 190 CE entry in the game data", donor.name))
            })?;
        let leader = find(
            "frontend_faction_leaders",
            "key",
            field(&junction, "frontend_faction_leader"),
        )?;
        let groups = self.rows("faction_to_faction_groups_junctions")?;
        let group = groups
            .iter()
            .find(|row| {
                field(row, "faction_key") == donor.faction
                    && field(row, "optional_campaign_key") == CAMPAIGN
            })
            .or_else(|| {
                groups.iter().find(|row| {
                    field(row, "faction_key") == donor.faction
                        && field(row, "optional_campaign_key").is_empty()
                })
            })
            .map(|row| field(row, "faction_group_key").to_owned())
            .with_context(|| format!("Missing faction group for {}", donor.name))?;
        Ok(Playstyle {
            faction,
            leader,
            group,
        })
    }

    fn copy_faction_rows(&self, output: &mut Pack, donor: &Warlord, key: &str) -> Result<()> {
        let suffix = format!("zz_{key}_warlord");
        let donor_sep = format!("{}_separatists", donor.faction);
        let match_faction = |value: &str| -> bool { value == donor.faction || value == donor_sep };
        let new_faction = |value: &str| -> String {
            if value == donor_sep {
                format!("{key}_separatists")
            } else {
                key.into()
            }
        };
        for (table, column) in [
            ("building_chain_availabilities", "faction"),
            ("campaign_map_attrition_faction_immunities", "faction"),
            ("campaign_stances_factions_junctions", "faction"),
            ("faction_agent_permitted_subtypes", "faction"),
            ("faction_to_name_groups", "faction"),
            ("technology_node_sets", "faction_key"),
            ("ui_primary_faction_pooled_resources", "faction"),
        ] {
            let mut rows: Vec<_> = self
                .rows(table)?
                .into_iter()
                .filter(|row| match_faction(field(row, column)))
                .collect();
            if rows.is_empty() {
                continue;
            }
            for (i, row) in rows.iter_mut().enumerate() {
                let faction = new_faction(field(row, column));
                set(row, column, faction);
                if table == "building_chain_availabilities" {
                    let id = record_id(&format!("{key}_warlord_building_{i}"));
                    set(row, "id", id.to_string());
                }
                if table == "technology_node_sets" {
                    set(row, "key", format!("{key}_technology_{i}"));
                }
            }
            let (path, source) = self
                .tables(table)
                .into_iter()
                .next()
                .context("Missing source table")?;
            add_rows_like(output, table, &suffix, rows, source, &path)?;
        }
        let criteria = "campaign_group_member_criteria_factions";
        let all_criteria = self.rows(criteria)?;
        let mut members = BTreeMap::new();
        for row in &all_criteria {
            let member = field(row, "member");
            if match_faction(field(row, "faction"))
                && ![
                    "cosmetic",
                    "mission",
                    "campaign_effect_bundle",
                    "grants_line_of_sight",
                    "faction_defection",
                ]
                .iter()
                .any(|part| member.contains(part))
            {
                let index = members.len();
                members
                    .entry(member.to_owned())
                    .or_insert_with(|| format!("{key}_warlord_{index}"));
            }
        }
        for table in self
            .retail
            .files()
            .keys()
            .chain(
                self.overlay
                    .into_iter()
                    .flat_map(|pack| pack.files().keys()),
            )
            .filter_map(|path| {
                path.strip_prefix("db/")?
                    .split_once("_tables/")
                    .map(|(table, _)| table)
            })
            .filter(|table| {
                *table == "campaign_group_members"
                    || table.starts_with("campaign_group_member_criteria_")
            })
            .collect::<BTreeSet<_>>()
        {
            let column = if table == "campaign_group_members" {
                "id"
            } else {
                "member"
            };
            let rows: Vec<_> = self
                .rows(table)?
                .into_iter()
                .filter_map(|mut row| {
                    let member = field(&row, column);
                    let mapped = members.get(member)?;
                    if table == criteria {
                        let faction = field(&row, "faction");
                        if match_faction(faction) {
                            let faction = new_faction(faction);
                            set(&mut row, "faction", faction);
                        } else if all_criteria.iter().any(|other| {
                            field(other, "member") == member
                                && field(other, "context") == field(&row, "context")
                                && match_faction(field(other, "faction"))
                        }) {
                            return None;
                        }
                    }
                    set(&mut row, column, mapped.as_str());
                    Some(row)
                })
                .collect();
            if !rows.is_empty() {
                let mut tables = self.tables(table).into_iter();
                let (mut path, mut source) =
                    tables.next().context("Missing campaign group table")?;
                while crate::game::table_is_empty(source, &path)? {
                    (path, source) = tables
                        .next()
                        .context("Missing populated campaign group table")?;
                }
                add_rows_like(output, table, &suffix, rows, source, &path)?;
            }
        }
        Ok(())
    }

    pub fn write(&self, output: &mut Pack, donor: &Warlord, key: &str) -> Result<()> {
        self.copy_faction_rows(output, donor, key)?;
        self.write_scripts(output, donor, key)?;
        self.write_campaign_ui(output, donor, key)
    }

    pub fn validate_pack(output: &Pack, native: &Playstyle, key: &str) -> Result<()> {
        let path = format!("db/factions_tables/{key}");
        let faction = rows_at(output, &path)?
            .into_iter()
            .find(|row| field(row, "key") == key)
            .context("The exported warlord faction is missing")?;
        for column in [
            "subculture",
            "feature_forest",
            "campaign_unit_permission_group",
        ] {
            ensure!(
                faction.get(column) == native.faction.get(column),
                "The exported warlord has a different {column} from the source faction"
            );
        }
        Ok(())
    }

    fn write_campaign_ui(&self, output: &mut Pack, donor: &Warlord, key: &str) -> Result<()> {
        let source_key = regex::escape(&donor.faction);
        let predicate = regex::Regex::new(&format!(
            r"(?P<lhs>(?:[\w.]*FactionRecordContext\.Key|\bkey))\s*(?P<op>==|!=)\s*(?P<q>&quot;|'){}(?:&quot;|')",
            source_key
        ))?;
        for panel in [
            "campaign_hud_faction_header",
            "faction_collections_panel",
            "family_tree",
            "army_id",
            "character_panel",
            "units_panel",
            "faction_summary_panel",
            "diplomacy",
            "diplomacy_manipulation_panel",
            "diplomacy_popup_panel",
            "diplomacy_factions_panel",
            "undercover_network_panel",
        ] {
            let path = format!("ui/campaign ui/{panel}.twui.xml");
            let source = self.file(&path);
            if source.file(&path, true).is_none() {
                continue;
            }
            let original = String::from_utf8(raw(source, &path)?)?;
            let patched = predicate.replace_all(&original, |matched: &regex::Captures<'_>| {
                let lhs = &matched["lhs"];
                let op = &matched["op"];
                let quote = &matched["q"];
                let join = if op == "==" { "||" } else { "&amp;&amp;" };
                format!("({} {join} {lhs} {op} {quote}{key}{quote})", &matched[0])
            });
            let patched = if panel == "campaign_hud_faction_header"
                && donor.faction == "3k_dlc04_faction_prince_liu_chong"
            {
                let start = patched
                    .find("id=\"3k_dlc04_faction_prince_liu_chong\"")
                    .context("Missing Trophy Hall button")?;
                let end = start
                    + patched[start..]
                        .find("</_3k_dlc04_faction_prince_liu_chong>")
                        .context("Missing Trophy Hall component")?;
                let from = "PlayersFaction.FactionRecordContext.Key == self.Id";
                let component = patched[start..end].replace(
                    from,
                    &format!(
                        "({from} || PlayersFaction.FactionRecordContext.Key == &quot;{key}&quot;)"
                    ),
                );
                format!("{}{component}{}", &patched[..start], &patched[end..]).into()
            } else {
                patched
            };
            if patched != original {
                add(output, &path, patched.as_bytes())?;
            }
        }
        Ok(())
    }

    fn write_scripts(&self, output: &mut Pack, donor: &Warlord, key: &str) -> Result<()> {
        const SCRIPTS: &[(&str, &[(&str, &str)])] = &[
            (
                "3k_main_faction_cao_cao",
                &[("dlc07_faction_cao_cao_schemes", "cao_cao_schemes")],
            ),
            (
                "3k_main_faction_yuan_shao",
                &[
                    (
                        "dlc07_faction_yuan_shao_resource_manager",
                        "yuan_shao_features",
                    ),
                    (
                        "dlc07_faction_yuan_shao_captain_armoury",
                        "yuan_shao_captain_armoury",
                    ),
                ],
            ),
            (
                "3k_dlc04_faction_prince_liu_chong",
                &[("dlc04_faction_ceos_liu_chong", "lc_faction_ceos")],
            ),
            (
                "3k_main_faction_liu_yan",
                &[("dlc07_faction_liu_yan_resource_manager", "liu_yan_features")],
            ),
            (
                "3k_main_faction_shi_xie",
                &[(
                    "dlc06_faction_shi_xie_resource_manager",
                    "shi_xie_resource_manager",
                )],
            ),
            (
                "3k_dlc05_faction_white_tiger_yan",
                &[(
                    "3k_dlc05_campaign_diplomacy_faction_specific_treaties",
                    "faction_specific_treaties",
                )],
            ),
            (
                "3k_dlc06_faction_nanman_king_meng_huo",
                &[(
                    "dlc06_faction_nanman_king_meng_huo_features",
                    "king_meng_huo_features",
                )],
            ),
            (
                "3k_dlc06_faction_nanman_king_mulu",
                &[(
                    "dlc06_faction_nanman_king_mulu_features",
                    "king_mulu_features",
                )],
            ),
            (
                "3k_dlc06_faction_nanman_king_shamoke",
                &[("dlc06_faction_nanman_king_shamoke_features", "")],
            ),
            (
                "3k_dlc06_faction_nanman_lady_zhurong",
                &[(
                    "dlc06_faction_nanman_lady_zhurong_feature",
                    "lady_zhurong_features",
                )],
            ),
        ];
        let scripts = SCRIPTS
            .iter()
            .find(|(faction, _)| *faction == donor.faction)
            .map_or(&[][..], |(_, scripts)| *scripts);
        for (index, (file, singleton)) in scripts.iter().enumerate() {
            let path = format!("script/campaign/_shared/{file}.lua");
            let mut script = String::from_utf8(raw(self.file(&path), &path)?)?;
            let original = format!("\"{}\"", donor.faction);
            ensure!(
                script.contains(&original),
                "Could not bind {file} to your faction"
            );
            script = script.replace(&original, &format!("\"{key}\""));
            if *singleton == "shi_xie_resource_manager" {
                script = script.replace(
                    "\"3k_main_template_ancestral_shi_ci\"",
                    &format!("\"{key}_leader\""),
                );
                // The ancestral family appears only in Shi Xie's historical opening.
                script = script.replace("self:add_narrative_listeners()", "");
            }
            let local = if singleton.is_empty() {
                String::new()
            } else {
                format!("local {singleton}\n")
            };
            let wrapper = include_str!("script/warlord_scope.lua").replace("__FACTION_KEY__", key);
            add(
                output,
                &format!("script/campaign/mod/{key}_warlord_{index}.lua"),
                format!("{wrapper}\n{local}{script}").as_bytes(),
            )?;
        }
        if donor.faction == "3k_main_faction_liu_bei" {
            let path = "script/campaign/mod/path_to_unity_faction_ceos_liu_bei.lua";
            if let Some(trom) = self.overlay.filter(|pack| pack.file(path, false).is_some()) {
                let script = String::from_utf8(raw(trom, path)?)?
                    .replace("\"3k_main_faction_liu_bei\"", &format!("\"{key}\""));
                let wrapper =
                    include_str!("script/warlord_scope.lua").replace("__FACTION_KEY__", key);
                add(
                    output,
                    &format!("script/campaign/mod/{key}_warlord_unity.lua"),
                    format!("{wrapper}\nlocal rt_faction_ceos, extra_unlock_groups\n{script}")
                        .as_bytes(),
                )?;
            }
        }
        Ok(())
    }
}

// Transfer religion and rank state while retaining the new faction's identity, army and land.
pub fn startpos(bytes: &[u8], generated: &[u8], donor: &Warlord, key: &str) -> Result<Vec<u8>> {
    let source = Document::read(bytes)?;
    let target = Document::read(generated)?;
    let source_marker = source.one(&source.root, "COMPRESSED_DATA")?.fields()[0].start;
    let container = target.one(&target.root, "COMPRESSED_DATA")?;
    let marker = container.fields()[0].start;
    let source = Document::read(&open_world(&source.data, source_marker)?)?;
    let destination = Document::read(&open_world(&target.data, marker)?)?;
    let origin = source.record("FACTION", 1, &donor.faction)?;
    let faction = destination.record("FACTION", 1, key)?;
    destination.record("FACTION", 1, &donor.faction)?;
    let mut edit = Edit::new(&destination);
    for kind in [
        "FACTION_RELIGION_MANAGER",
        "PROGRESSION_LEVEL",
        "FACTION_FEATURE_SET_MANAGER",
        "FACTION_TECHNOLOGY_MANAGER",
    ] {
        if kind == "FACTION_TECHNOLOGY_MANAGER" && donor.family == WarlordFamily::Bandit {
            continue;
        }
        let from = source.one(origin, kind)?;
        let to = destination.one(faction, kind)?;
        let overrides = if kind == "FACTION_TECHNOLOGY_MANAGER" {
            HashMap::from([(from.fields()[0].start, edit.text_reference(key))])
        } else {
            HashMap::new()
        };
        let bytes = edit.import(&source, from, &overrides)?;
        edit.replace(to, bytes);
    }
    let mut outer = Edit::new(&target);
    let mut replacement = target.data[container.start..marker - 5].to_vec();
    replacement.extend(compression_record(&target.data, &edit.finish()?)?);
    outer.replace(container, replacement);
    outer.finish()
}
