use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    caab::Document,
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
    Rival,
}
impl Tie {
    pub(crate) fn verb(self) -> &'static str {
        match self {
            Self::Spouse => "is married to",
            Self::Parent => "is a parent of",
            Self::Friend => "is friends with",
            Self::Rival => "is a rival of",
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Spouse => "spouse",
            Self::Parent => "parent",
            Self::Friend => "friend",
            Self::Rival => "rival",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Relationship {
    pub first: Person,
    pub tie: Tie,
    pub second: Person,
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
                Tie::Friend | Tie::Rival => ensure!(
                    selected.insert((pair.0.clone(), pair.1.clone(), false)),
                    "This pair already has a friendship or rivalry."
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
        let social = link(daughter.clone(), Tie::Friend, Person::Founder);
        assert!(
            roster
                .validate(&config, true, std::slice::from_ref(&social))
                .is_ok()
        );
        assert!(
            roster
                .validate(
                    &config,
                    true,
                    &[social, link(Person::Founder, Tie::Rival, daughter)]
                )
                .is_err()
        );
    }
}
