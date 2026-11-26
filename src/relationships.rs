use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use rpfm_lib::files::pack::Pack;
use serde::{Deserialize, Serialize};

use crate::{
    caab::{Document, Edit},
    config::{CAMPAIGN, Config, Officer},
    game::{self, Game, Localization},
    portraits::words,
    trom,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Person {
    Founder,
    Officer(usize),
    Historical(String),
}

impl Person {
    pub(crate) fn key(&self, config: &Config) -> String {
        match self {
            Self::Founder => format!("{}_leader", config.key()),
            Self::Officer(i) => Officer::key(&config.key(), *i),
            Self::Historical(k) => k.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tie {
    #[default]
    Spouse,
    Parent,
    Friend,
    Oathsworn,
    Rival,
    Nemesis,
}
impl Tie {
    pub(crate) fn is_social(self) -> bool {
        matches!(
            self,
            Self::Friend | Self::Oathsworn | Self::Rival | Self::Nemesis
        )
    }

    pub(crate) fn verb(self) -> &'static str {
        match self {
            Self::Spouse => "is married to",
            Self::Parent => "is a parent of",
            Self::Friend => "is friends with",
            Self::Oathsworn => "is oathsworn to",
            Self::Rival => "is a rival of",
            Self::Nemesis => "is the nemesis of",
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Spouse => "spouse",
            Self::Parent => "parent",
            Self::Friend => "friend",
            Self::Oathsworn => "oathsworn",
            Self::Rival => "rival",
            Self::Nemesis => "nemesis",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Relationship {
    pub first: Person,
    pub tie: Tie,
    pub second: Person,
}

pub(crate) fn write_bond_triggers(
    pack: &mut Pack,
    faction: &str,
    links: &[Relationship],
) -> Result<()> {
    use crate::export::row;
    for (tie, sign) in [(Tie::Oathsworn, "positive"), (Tie::Nemesis, "negative")] {
        if !links.iter().any(|link| link.tie == tie) {
            continue;
        }
        let key = format!("{faction}_bond_{}", tie.key());
        // Apply one maximum-strength memory directly to this pair on the first tick.
        // Multiple weaker memories can leave the pair at the intermediate tier.
        for (table, entry) in [
            ("character_relationship_trigger_sets", row! { "key": key }),
            (
                "character_relationship_trigger_set_items",
                row! {
                    "set": key, "target": "CHARACTER_ONLY", "trigger": key
                },
            ),
            (
                "character_relationship_triggers",
                row! {
                    "key": key, "conditions": "3k_main_relationship_condition_set_any",
                    "memory": format!("3k_main_relationship_memory_campaign_event_{sign}"),
                    "memory_relevancy_decay_per_turn": 5
                },
            ),
            (
                "character_relationship_trigger_effects",
                row! {
                    "trigger": key, "effect": format!("relationship_effect_permanent_{sign}_max")
                },
            ),
        ] {
            game::add_rows(pack, table, &key, vec![entry])?;
        }
    }
    Ok(())
}

/// The new-campaign script applies native memories. Clear the selected historical
/// pairs' stored scores and memories first, so older memories cannot cancel that choice.
pub(crate) fn prepare_startpos(bytes: Vec<u8>, links: &[Relationship]) -> Result<Vec<u8>> {
    let pairs: Vec<_> = links
        .iter()
        .filter_map(|link| match (&link.first, link.tie, &link.second) {
            (Person::Historical(a), tie, Person::Historical(b)) if tie.is_social() => Some((a, b)),
            _ => None,
        })
        .collect();
    if pairs.is_empty() {
        return Ok(bytes);
    }
    let (frontend, world) = trom::campaign_documents(&bytes)?;
    let characters = world.find(&world.root, "PERSISTENT_CHARACTER");
    let mut ids = HashMap::new();
    for character in characters {
        let fields = character.fields();
        if let Some(template) = fields.get(10).and_then(|field| world.text(field)) {
            ids.insert(template, world.id(&fields[0])?);
        }
    }
    let mut selected = HashSet::new();
    for (a, b) in pairs {
        let id = |key: &str| {
            ids.get(key)
                .copied()
                .with_context(|| format!("Missing historical character {key}"))
        };
        let mut pair = [id(a)?, id(b)?];
        pair.sort_unstable();
        selected.insert(pair);
    }
    let manager = world.one(&world.root, "CHARACTER_RELATIONSHIPS")?;
    let array = world.one(manager, "RELATIONSHIP")?;
    let mut edit = Edit::new(&world);
    let changed = edit.retain_groups(array, |row| {
        let mut pair = world.id_pair(row.get(4).context("Missing relationship members")?)?;
        pair.sort_unstable();
        Ok(!selected.contains(&pair))
    })?;
    if changed {
        trom::finish_campaign(Edit::new(&frontend), edit)
    } else {
        Ok(bytes)
    }
}

pub(crate) fn remove_officer(links: &mut Vec<Relationship>, index: usize) {
    links.retain(|r| r.first != Person::Officer(index) && r.second != Person::Officer(index));
    for link in links {
        for person in [&mut link.first, &mut link.second] {
            if let Person::Officer(i) = person
                && *i > index
            {
                *i -= 1;
            }
        }
    }
}

#[derive(Default, Clone)]
struct Family {
    parents: [Option<String>; 2],
    spouse: Option<String>,
}

pub(crate) struct Historical {
    pub name: String,
    pub faction: String,
    pub age: Option<u32>,
    pub male: bool,
}
pub(crate) struct Roster {
    pub people: BTreeMap<String, Historical>,
    labels: HashMap<String, String>,
    families: HashMap<String, Family>,
}

fn display_key(key: &str) -> String {
    let name = key
        .split_once("_historical_")
        .or_else(|| key.split_once("_ancestral_"))
        .or_else(|| key.split_once("_template_"))
        .map_or(key, |(_, s)| s);
    words(name.split("_hero").next().unwrap_or(name))
}

impl Roster {
    pub fn read(game: &Game, overlay_paths: &[std::path::PathBuf]) -> Result<Self> {
        let packs = overlay_paths
            .iter()
            .map(|p| game::open_pack(p))
            .collect::<Result<Vec<_>>>()?;
        let mut loc = game.relationship_labels()?;
        for pack in &packs {
            let mut paths = pack
                .files()
                .keys()
                .filter(|p| p.ends_with(".loc"))
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                loc.extend(
                    game::localization(pack, path)?
                        .into_iter()
                        .filter(|(k, _)| {
                            k.starts_with("names_name_") || k.starts_with("factions_screen_name_")
                        }),
                );
            }
        }
        // Vanilla exports use the bundled campaign as their starting point.
        let template;
        let base = if let Some(pack) = packs.first() {
            pack
        } else {
            template = game::template()?;
            &template
        };
        let path = format!("campaigns/{CAMPAIGN}/startpos_romance.esf");
        let (_, world) = trom::campaign_documents(&game::raw(base, &path)?)?;
        Self::from_world(&world, &loc)
    }

    pub(crate) fn from_world(w: &Document, loc: &Localization) -> Result<Self> {
        let chars = w.find(&w.root, "PERSISTENT_CHARACTER");
        let mut counts = HashMap::new();
        for c in &chars {
            if let Some(k) = c.fields().get(10).and_then(|n| w.text(n)) {
                *counts.entry(k).or_insert(0) += 1;
            }
        }
        let mut ids = HashMap::new();
        let mut people = BTreeMap::new();
        let mut labels = HashMap::new();
        let factions: HashMap<_, _> = w
            .find(&w.root, "FACTION")
            .into_iter()
            .filter_map(|f| Some((w.number(f.fields().first()?)?, w.text(f.fields().get(1)?)?)))
            .collect();
        for c in chars {
            let f = c.fields();
            let id = w.id(&f[0])?;
            let Some(template) = w.text(&f[10]) else {
                continue;
            };
            let key = if counts[template] == 1 {
                template.to_owned()
            } else {
                format!("#{id}")
            };
            ids.insert(id, key.clone());
            let names = f[3].fields();
            let name_part = |i: usize| {
                names
                    .get(i)?
                    .fields()
                    .first()
                    .and_then(|n| w.text(n))
                    .and_then(|k| loc.get(k))
                    .filter(|n| !n.is_empty())
            };
            let label = match (name_part(2), name_part(0)) {
                (Some(s), Some(g)) => format!("{s} {g}"),
                _ => display_key(template),
            };
            labels.insert(key.clone(), label.clone());
            if key.starts_with('#')
                || key.starts_with("sects_")
                || key.contains("_generic_")
                || key.contains("_ancestral_")
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !f.iter().any(|n| w.name(n) == "ACTIVE_CHARACTER")
            {
                continue;
            }
            let faction = w
                .number(&f[2])
                .and_then(|i| factions.get(&i).copied())
                .unwrap_or("");
            let faction = loc
                .get(&format!("factions_screen_name_{faction}"))
                .cloned()
                .unwrap_or_else(|| words(faction.split("_faction_").last().unwrap_or(faction)));
            let age = f[7]
                .fields()
                .first()
                .and_then(|n| w.number(n))
                .and_then(|y| 190u32.checked_sub(y));
            let male = f[4].fields().get(5).is_some_and(|n| w.raw(n) == [0x12]);
            people.insert(
                key,
                Historical {
                    name: label,
                    faction,
                    age,
                    male,
                },
            );
        }
        let members = w.find(&w.root, "FAMILY_MEMBER");
        let mut member_keys = HashMap::new();
        for m in &members {
            let f = m.fields();
            let id = w.id(&f[0])?;
            let key = ids
                .get(&w.id(&f[1])?)
                .cloned()
                .unwrap_or_else(|| format!("family#{id}"));
            member_keys.insert(id, key);
        }
        let mut families = HashMap::new();
        for m in members {
            let f = m.fields();
            let relative = |i: usize| {
                w.number(&f[i])
                    .filter(|id| *id != 0)
                    .and_then(|id| member_keys.get(&id).cloned())
            };
            families.insert(
                member_keys[&w.id(&f[0])?].clone(),
                Family {
                    parents: [relative(2), relative(3)],
                    spouse: relative(4),
                },
            );
        }
        Ok(Self {
            people,
            labels,
            families,
        })
    }

    pub fn family_info(&self, key: &str) -> String {
        let mut out = Vec::new();
        if let Some(f) = self.families.get(key) {
            for (title, r) in [
                ("Spouse", &f.spouse),
                ("Mother", &f.parents[0]),
                ("Father", &f.parents[1]),
            ] {
                if let Some(k) = r {
                    out.push(format!(
                        "{title} · {}",
                        self.labels.get(k).map(String::as_str).unwrap_or("Unknown")
                    ));
                }
            }
        }
        out.join("   ")
    }

    pub fn validate(&self, config: &Config, male: bool, links: &[Relationship]) -> Result<()> {
        let mut families = self.families.clone();
        let mut selected = BTreeSet::new();
        let mut parentage_changed = false;
        let details = |p: &Person| -> Result<(bool, Option<u32>)> {
            Ok(match p {
                Person::Founder => (male, Some(config.founder_age.into())),
                Person::Officer(i) => {
                    let o = config
                        .officers
                        .get(*i)
                        .context("A selected officer is missing.")?;
                    (!o.female, o.age.map(u32::from))
                }
                Person::Historical(k) => {
                    let h = self.people.get(k).with_context(|| {
                        format!("{} is unavailable in this campaign.", display_key(k))
                    })?;
                    (h.male, h.age)
                }
            })
        };
        for r in links {
            let (am, aa) = details(&r.first)?;
            let (bm, ba) = details(&r.second)?;
            let a = r.first.key(config);
            let b = r.second.key(config);
            ensure!(a != b, "Choose two different characters.");
            let pair = if a < b { (&a, &b) } else { (&b, &a) };
            match r.tie {
                Tie::Friend | Tie::Oathsworn | Tie::Rival | Tie::Nemesis => ensure!(
                    selected.insert((pair.0.clone(), pair.1.clone(), false)),
                    "This pair already has a starting social relationship. Remove it before choosing another."
                ),
                Tie::Spouse => {
                    ensure!(
                        selected.insert((pair.0.clone(), pair.1.clone(), true)),
                        "This marriage is already listed."
                    );
                    ensure!(
                        am != bm && aa.is_none_or(|n| n >= 18) && ba.is_none_or(|n| n >= 18),
                        "Choose an adult man and woman for a marriage."
                    );
                    for (k, other) in [(&a, &b), (&b, &a)] {
                        let f = families.entry(k.clone()).or_default();
                        ensure!(
                            f.spouse.is_none(),
                            "One of these characters already has a spouse."
                        );
                        f.spouse = Some(other.clone());
                    }
                }
                Tie::Parent => {
                    let (parent_age, child_age) = aa.zip(ba).context(
                        "Both ages must be known. Set each custom officer’s age under Starting officers.",
                    )?;
                    ensure!(
                        parent_age > child_age,
                        "The parent must be older than the child."
                    );
                    let slot = &mut families.entry(b.clone()).or_default().parents[usize::from(am)];
                    ensure!(
                        slot.is_none(),
                        "This child already has a parent in that family-tree slot."
                    );
                    *slot = Some(a.clone());
                    parentage_changed = true;
                    ensure!(
                        !ancestors(&a, &families).contains(&b),
                        "This link would create a cycle in the family tree."
                    );
                }
            }
        }
        for (a, family) in &families {
            let Some(b) = family.spouse.as_ref().filter(|b| a < *b) else {
                continue;
            };
            let existing = self.families.get(a).and_then(|f| f.spouse.as_ref()) == Some(b);
            if !existing || parentage_changed {
                ensure!(
                    !related(a, b, &families) || (existing && related(a, b, &self.families)),
                    "This would make relatives by descent a married couple."
                );
            }
        }
        Ok(())
    }
}

fn related(a: &str, b: &str, families: &HashMap<String, Family>) -> bool {
    let left = ancestors(a, families);
    let right = ancestors(b, families);
    left.contains(b) || right.contains(a) || !left.is_disjoint(&right)
}

fn ancestors(key: &str, families: &HashMap<String, Family>) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut todo = vec![key];
    while let Some(k) = todo.pop() {
        if let Some(f) = families.get(k) {
            for p in f.parents.iter().flatten() {
                if seen.insert(p.clone()) {
                    todo.push(p);
                }
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(first: Person, tie: Tie, second: Person) -> Relationship {
        Relationship { first, tie, second }
    }

    #[test]
    fn selecting_rivals_replaces_only_the_pairs_existing_social_history() -> Result<()> {
        use crate::startpos::Mode;
        const LIU: &str = "3k_main_template_historical_liu_bei_hero_earth";
        const ZAN: &str = "3k_main_template_historical_gongsun_zan_hero_fire";
        let choice = link(
            Person::Historical(ZAN.into()),
            Tie::Rival,
            Person::Historical(LIU.into()),
        );
        let template = game::template()?;
        for mode in Mode::ALL {
            let path = format!("campaigns/{CAMPAIGN}/startpos_{}.esf", mode.key());
            let bytes = game::raw(&template, &path)?;
            let (_, before) = trom::campaign_documents(&bytes)?;
            let id = |name: &str| {
                before.id(&before.record("PERSISTENT_CHARACTER", 10, name)?.fields()[0])
            };
            let mut selected = [id(LIU)?, id(ZAN)?];
            selected.sort_unstable();
            let mut expected = Vec::new();
            let rows = before.one(&before.root, "RELATIONSHIP")?;
            for row in &rows.groups {
                let mut ids = before.id_pair(&row[4])?;
                ids.sort_unstable();
                if ids == selected {
                    assert_eq!(
                        before.text(&row[1]),
                        Some("3k_main_relationship_positive_02")
                    );
                } else {
                    expected.push(
                        row.iter()
                            .flat_map(|n| before.raw(n).iter().copied())
                            .collect::<Vec<_>>(),
                    );
                }
            }
            assert_eq!(rows.groups.len(), expected.len() + 1);
            let result = prepare_startpos(bytes, std::slice::from_ref(&choice))?;
            let (_, after) = trom::campaign_documents(&result)?;
            let actual: Vec<Vec<u8>> = after
                .one(&after.root, "RELATIONSHIP")?
                .groups
                .iter()
                .map(|row| {
                    row.iter()
                        .flat_map(|n| after.raw(n).iter().copied())
                        .collect()
                })
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(
                before.raw(before.one(&before.root, "FAMILY_TREE")?),
                after.raw(after.one(&after.root, "FAMILY_TREE")?)
            );
        }
        Ok(())
    }

    #[test]
    fn deleting_an_officer_preserves_the_remaining_links() {
        use Person::*;
        let mut links = vec![
            link(Founder, Tie::Parent, Officer(1)),
            link(Officer(2), Tie::Friend, Officer(0)),
            link(Officer(1), Tie::Rival, Officer(2)),
        ];
        remove_officer(&mut links, 1);
        assert_eq!(links, [link(Officer(1), Tie::Friend, Officer(0))]);
    }

    #[test]
    fn a_marriage_cannot_conflict_with_existing_or_new_family_ties() {
        let config = Config {
            founder_age: 50,
            ..Config::default()
        };
        let daughter = Person::Historical("daughter".into());
        let mut roster = Roster {
            people: BTreeMap::from([(
                "daughter".into(),
                Historical {
                    name: "Daughter".into(),
                    faction: String::new(),
                    male: false,
                    age: Some(22),
                },
            )]),
            families: HashMap::new(),
            labels: HashMap::new(),
        };
        let marriage = link(Person::Founder, Tie::Spouse, daughter.clone());
        let parent = link(Person::Founder, Tie::Parent, daughter.clone());
        for links in [
            [marriage.clone(), parent.clone()],
            [parent.clone(), marriage.clone()],
        ] {
            assert!(roster.validate(&config, true, &links).is_err());
        }
        roster.families.insert(
            "daughter".into(),
            Family {
                parents: [None, Some("existing_father".into())],
                spouse: Some("existing_spouse".into()),
            },
        );
        assert!(roster.validate(&config, true, &[marriage]).is_err());
        assert!(roster.validate(&config, true, &[parent]).is_err());
        roster.families.get_mut("daughter").unwrap().parents = [None, None];
        roster.people.insert(
            "existing_spouse".into(),
            Historical {
                name: "Spouse".into(),
                faction: String::new(),
                male: true,
                age: Some(52),
            },
        );
        roster.families.insert(
            "existing_spouse".into(),
            Family {
                parents: [None, None],
                spouse: Some("daughter".into()),
            },
        );
        let conflicting_parent = link(
            Person::Historical("existing_spouse".into()),
            Tie::Parent,
            daughter.clone(),
        );
        assert!(
            roster
                .validate(&config, true, &[conflicting_parent])
                .is_err()
        );
        let social_ties = [Tie::Friend, Tie::Oathsworn, Tie::Rival, Tie::Nemesis];
        for tie in social_ties {
            let social = link(daughter.clone(), tie, Person::Founder);
            assert!(
                roster
                    .validate(&config, true, std::slice::from_ref(&social))
                    .is_ok()
            );
            for other in social_ties {
                let reversed = link(Person::Founder, other, daughter.clone());
                assert!(
                    roster
                        .validate(&config, true, &[social.clone(), reversed])
                        .is_err()
                );
            }
        }
    }
}
