use std::{borrow::Cow, sync::LazyLock};

use regex::Regex;

use crate::config::Variant;

const MALE: &str = include_str!("../data/names/male_given.txt");
const FEMALE: &str = include_str!("../data/names/female_given.txt");
const FAMILY: &str = include_str!("../data/names/family.txt");
const SECT: &str = include_str!("../data/names/sects.txt");
const CULT: &str = include_str!("../data/names/cults.txt");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

impl Gender {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "Male" => Some(Self::Male),
            "Female" => Some(Self::Female),
            _ => None,
        }
    }

    fn title(self, (male, female): (&'static str, &'static str)) -> &'static str {
        match self {
            Self::Male => male,
            Self::Female => female,
        }
    }
}

// Store one form of each generated title in the pools. Convert whole words
// here so custom names, punctuation and capitalization work the same way.
const TITLES: &[(&str, &str)] = &[
    ("Brotherhood", "Sisterhood"),
    ("Brothers", "Sisters"),
    ("Brother", "Sister"),
    ("Sons", "Daughters"),
    ("Son", "Daughter"),
    ("Fathers", "Mothers"),
    ("Father", "Mother"),
    ("Patriarchs", "Matriarchs"),
    ("Patriarch", "Matriarch"),
    ("Lords", "Ladies"),
    ("Lord", "Lady"),
    ("Kings", "Queens"),
    ("King", "Queen"),
    ("Princes", "Princesses"),
    ("Prince", "Princess"),
    ("Monks", "Nuns"),
    ("Monk", "Nun"),
];

static MASCULINE: LazyLock<Regex> = LazyLock::new(|| title_pattern(Gender::Male));
static FEMININE: LazyLock<Regex> = LazyLock::new(|| title_pattern(Gender::Female));

fn title_pattern(gender: Gender) -> Regex {
    let words = TITLES
        .iter()
        .map(|&pair| gender.title(pair))
        .collect::<Vec<_>>()
        .join("|");
    Regex::new(&format!(r"\b(?i-u:{words})\b")).expect("The title list contains only words")
}

pub fn faction_title(name: &str, gender: Gender) -> Cow<'_, str> {
    let (pattern, from) = match gender {
        Gender::Male => (&*FEMININE, Gender::Female),
        Gender::Female => (&*MASCULINE, Gender::Male),
    };
    pattern.replace_all(name, |captures: &regex::Captures<'_>| {
        let word = &captures[0];
        let pair = TITLES
            .iter()
            .copied()
            .find(|&pair| from.title(pair).eq_ignore_ascii_case(word))
            .expect("The title pattern was built from the same list");
        let replacement = gender.title(pair);
        if word.bytes().all(|letter| letter.is_ascii_uppercase()) {
            replacement.to_ascii_uppercase()
        } else if word.as_bytes()[0].is_ascii_lowercase() {
            replacement.to_ascii_lowercase()
        } else {
            replacement.to_owned()
        }
    })
}

pub fn given_name(gender: Gender, current: &str) -> &'static str {
    let pool = match gender {
        Gender::Male => MALE,
        Gender::Female => FEMALE,
    };
    reroll(pool.lines(), current)
}

pub fn family_name(current: &str) -> &'static str {
    reroll(FAMILY.lines(), current)
}

pub fn faction_name(variant: Variant, gender: Option<Gender>, current: &str) -> String {
    let pool = match variant {
        Variant::Sect => SECT,
        Variant::Cult => CULT,
    };
    let current = faction_title(current, Gender::Male);
    let options = pool
        .lines()
        .filter(|name| gender.is_some() || !MASCULINE.is_match(name));
    let name = reroll(options, &current);
    gender.map_or_else(
        || name.to_owned(),
        |gender| faction_title(name, gender).into_owned(),
    )
}

fn reroll(pool: impl Iterator<Item = &'static str>, current: &str) -> &'static str {
    let current = current.trim();
    let candidates = pool
        .filter(|name| !name.eq_ignore_ascii_case(current))
        .collect::<Vec<_>>();
    fastrand::choice(candidates).expect("The bundled pool has more than one name")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::config::Config;

    #[test]
    fn name_pools_are_large_and_have_unique_valid_entries() {
        for pool in [MALE, FEMALE, FAMILY, SECT, CULT] {
            let mut names = HashSet::new();
            for name in pool.lines() {
                assert_eq!(name, name.trim());
                assert!((1..=64).contains(&name.chars().count()));
                assert!(!name.chars().any(char::is_control));
                assert!(
                    names.insert(name.to_ascii_lowercase()),
                    "Duplicate name {name}"
                );
            }
            assert!(names.len() >= 100);
        }
        let mut faction_keys = HashSet::new();
        for gender in [Gender::Male, Gender::Female] {
            faction_keys.clear();
            for name in SECT.lines().chain(CULT.lines()) {
                let config = Config {
                    sect_name: faction_title(name, gender).into_owned(),
                    ..Config::default()
                };
                let config = config.validated().unwrap();
                assert!(
                    faction_keys.insert(config.key()),
                    "Duplicate faction key for {name}"
                );
            }
        }
    }

    #[test]
    fn rerolls_use_the_selected_pool_and_replace_the_current_name() {
        for (pool, roll) in [
            (MALE, given_name_male as fn(&str) -> &'static str),
            (FEMALE, given_name_female),
            (FAMILY, family_name),
        ] {
            let mut previous = pool.lines().next().unwrap();
            for _ in 0..64 {
                let result = roll(&previous.to_ascii_lowercase());
                assert!(pool.lines().any(|name| name == result));
                assert_ne!(previous, result);
                previous = result;
            }
        }
    }

    #[test]
    fn faction_rerolls_match_both_variant_and_gender() {
        for (pool, variant) in [(SECT, Variant::Sect), (CULT, Variant::Cult)] {
            for gender in [None, Some(Gender::Male), Some(Gender::Female)] {
                let mut previous = String::from("Pale Crow Sisterhood");
                for _ in 0..64 {
                    let result = faction_name(variant, gender, &previous);
                    let base_name = faction_title(&result, Gender::Male);
                    assert!(pool.lines().any(|name| name == base_name));
                    assert!(!previous.eq_ignore_ascii_case(&result));
                    match gender {
                        Some(Gender::Male) => assert!(!FEMININE.is_match(&result)),
                        Some(Gender::Female) => assert!(!MASCULINE.is_match(&result)),
                        None => {
                            assert!(!MASCULINE.is_match(&result) && !FEMININE.is_match(&result))
                        }
                    }
                    previous = result;
                }
            }
        }
    }

    #[test]
    fn gendered_titles_change_in_both_directions() {
        for &(male, female) in TITLES {
            assert_eq!(faction_title(male, Gender::Female), female);
            assert_eq!(faction_title(female, Gender::Male), male);
        }
        assert_eq!(
            faction_title("Pale Crow BROTHERHOOD & the king's guard", Gender::Female),
            "Pale Crow SISTERHOOD & the queen's guard"
        );
        assert_eq!(
            faction_title("Kingfisher Society / Brotherhoods", Gender::Female),
            "Kingfisher Society / Brotherhoods"
        );
    }

    fn given_name_male(current: &str) -> &'static str {
        given_name(Gender::Male, current)
    }
    fn given_name_female(current: &str) -> &'static str {
        given_name(Gender::Female, current)
    }
}
