use std::{collections::BTreeMap, path::PathBuf, sync::LazyLock};

use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

pub const BASE_KEY: &str = "sects_white_lotus";
pub const BASE_ART: &str = "3k_main_art_set_generic_male_48_general_wood";
pub const MIN_FOUNDER_AGE: u8 = 18;
pub const MAX_FOUNDER_AGE: u8 = 60;
pub const HEPU: &str = "3k_main_hepu_capital";
pub const HAN: &str = "3k_main_faction_han_empire";
pub const CAMPAIGN: &str = "3k_main_campaign_map";
pub const SLOTS: [(&str, &str); 5] = [
    ("weapon", "Weapons"),
    ("armour", "Armour"),
    ("mount", "Mounts"),
    ("accessory", "Accessories"),
    ("follower", "Followers"),
];
pub const CLASSES: [(&str, &str, &str, u32); 5] = [
    ("3k_general_wood", "Champion", "green", 0x85aa78),
    ("3k_general_fire", "Vanguard", "red", 0xd58173),
    ("3k_general_earth", "Commander", "yellow", 0xd6ad65),
    ("3k_general_metal", "Sentinel", "purple", 0xb19dc8),
    ("3k_general_water", "Strategist", "blue", 0x7aaac4),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    #[default]
    Sect,
    Cult,
}

impl Variant {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sect => "Sect",
            Self::Cult => "Cult",
        }
    }

    pub fn objective(self) -> &'static str {
        match self {
            Self::Sect => "Win when your chosen patron claims an imperial throne.",
            Self::Cult => "Sack and withdraw from 30 different commandery cities.",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub sect_name: String,
    pub forename: String,
    pub surname: String,
    #[serde(default = "default_founder_age")]
    pub founder_age: u8,
    pub primary_colour: String,
    pub secondary_colour: String,
    #[serde(default)]
    pub faction_key: Option<String>,
    #[serde(default)]
    pub art_set: String,
    #[serde(default)]
    pub portrait_pack: Option<PathBuf>,
    #[serde(default = "default_subtype")]
    pub subtype: String,
    #[serde(default)]
    pub starting_traits: Vec<String>,
    #[serde(default)]
    pub starting_ancillaries: BTreeMap<String, String>,
    #[serde(default = "default_region")]
    pub start_region: String,
    #[serde(default = "enabled")]
    pub sect_mode: bool,
    #[serde(default)]
    pub trom_compat: bool,
    #[serde(default)]
    pub variant: Variant,
}

fn default_founder_age() -> u8 {
    MIN_FOUNDER_AGE
}

fn default_subtype() -> String {
    CLASSES[0].0.into()
}
fn default_region() -> String {
    HEPU.into()
}
fn enabled() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sect_name: "White Lotus".into(),
            forename: "Lian".into(),
            surname: "Shen".into(),
            founder_age: default_founder_age(),
            primary_colour: "A34434".into(),
            secondary_colour: "D3AD6C".into(),
            faction_key: None,
            art_set: BASE_ART.into(),
            portrait_pack: None,
            subtype: default_subtype(),
            starting_traits: Vec::new(),
            starting_ancillaries: BTreeMap::new(),
            start_region: default_region(),
            sect_mode: true,
            trom_compat: false,
            variant: Variant::Sect,
        }
    }
}

static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new("[^a-z0-9]+").unwrap());
static FACTION_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\Asects_[a-z0-9][a-z0-9_]{0,47}\z").unwrap());
static ART_SET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A[A-Za-z0-9_]{1,160}\z").unwrap());
static TRAIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A3k_main_ceo_trait_(?:personality|physical)_[a-z0-9_]+\z").unwrap()
});

impl Config {
    pub fn key(&self) -> String {
        if let Some(key) = &self.faction_key
            && !key.is_empty()
        {
            return key.clone();
        }
        let ascii = self
            .sect_name
            .trim()
            .nfkd()
            .filter(char::is_ascii)
            .collect::<String>()
            .to_ascii_lowercase();
        let normalized = SLUG.replace_all(&ascii, "_");
        let slug = normalized.trim_matches('_');
        if slug.is_empty() {
            return format!(
                "sects_{}",
                &hex::encode(Sha256::digest(self.sect_name.trim().as_bytes()))[..12]
            );
        }
        format!("sects_{}", slug[..slug.len().min(42)].trim_end_matches('_'))
    }

    pub fn filename(&self) -> String {
        let key = self.key();
        if self.trom_compat {
            format!("!{key}_trom.pack")
        } else {
            format!("{key}.pack")
        }
    }

    pub fn element(&self) -> &str {
        self.subtype
            .strip_prefix("3k_general_")
            .unwrap_or(&self.subtype)
    }

    pub fn settlement(&self) -> Option<&'static Settlement> {
        SETTLEMENTS
            .iter()
            .find(|site| site.key == self.start_region)
    }

    pub fn validated(mut self) -> Result<Self> {
        ensure!(
            (MIN_FOUNDER_AGE..=MAX_FOUNDER_AGE).contains(&self.founder_age),
            "Founder age must be between {MIN_FOUNDER_AGE} and {MAX_FOUNDER_AGE}."
        );
        for (label, value) in [
            ("Faction name", &mut self.sect_name),
            ("Given name", &mut self.forename),
            ("Family name", &mut self.surname),
        ] {
            *value = value.trim().to_owned();
            ensure!(
                (1..=64).contains(&value.chars().count()) && !value.chars().any(char::is_control),
                "{label} must contain 1 to 64 printable characters."
            );
        }
        for value in [&mut self.primary_colour, &mut self.secondary_colour] {
            *value = value.trim().trim_start_matches('#').to_ascii_uppercase();
            ensure!(
                value.len() == 6 && value.bytes().all(|b| b.is_ascii_hexdigit()),
                "Both faction colours must use six hexadecimal digits."
            );
        }
        let key = self.key();
        ensure!(
            FACTION_KEY.is_match(&key) && !key.ends_with("_separatists"),
            "Invalid faction key {key}."
        );
        self.faction_key = Some(key);
        ensure!(
            CLASSES.iter().any(|class| class.0 == self.subtype),
            "Choose a founder class."
        );
        if self.art_set.is_empty() {
            self.art_set = BASE_ART.into();
        }
        ensure!(ART_SET.is_match(&self.art_set), "Invalid portrait art set.");
        ensure!(
            self.settlement().is_some(),
            "Choose an eligible starting settlement."
        );
        ensure!(
            self.starting_traits.len() <= 3,
            "Choose up to three traits."
        );
        let mut unique = std::collections::HashSet::new();
        ensure!(
            self.starting_traits
                .iter()
                .all(|key| TRAIT.is_match(key) && unique.insert(key)),
            "Choose each eligible trait once."
        );
        ensure!(
            self.starting_traits
                .iter()
                .filter(|key| key.contains("_physical_"))
                .count()
                <= 1,
            "Choose at most one physical trait."
        );
        if let Some(key) = self.starting_ancillaries.remove("extra") {
            let item = ANCILLARIES
                .get(&key)
                .context("Unknown legacy accessory or follower.")?;
            ensure!(
                matches!(item.slot, "accessory" | "follower"),
                "Invalid legacy extra slot."
            );
            self.starting_ancillaries
                .entry(item.slot.into())
                .or_insert(key);
        }
        for (slot, key) in &self.starting_ancillaries {
            let item = ANCILLARIES
                .get(key)
                .with_context(|| format!("Unknown starting item {key}."))?;
            ensure!(
                item.slot == slot && item.fits(self.element()),
                "Choose an eligible starting item for {slot}."
            );
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
pub struct Settlement {
    pub key: String,
    pub name: String,
    pub x: u16,
    pub y: u16,
    pub county: bool,
    pub han: bool,
    pub buildings: Option<(String, String, [usize; 2])>,
}

impl Settlement {
    pub fn owner_label(&self) -> &'static str {
        if self.key == HEPU {
            "Hepu scenario"
        } else if self.han {
            "Han Empire"
        } else {
            "Unoccupied"
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Warlord {
    pub name: String,
    pub x: u16,
    pub y: u16,
    pub primary: String,
    pub secondary: String,
    pub portrait: String,
}

pub static SETTLEMENTS: LazyLock<Vec<Settlement>> = LazyLock::new(|| {
    let mut sites: Vec<Settlement> =
        serde_json::from_str(include_str!("data/settlements.json")).unwrap();
    sites.sort_by(|a, b| a.name.cmp(&b.name));
    sites
});
pub static WARLORDS: LazyLock<Vec<Warlord>> =
    LazyLock::new(|| serde_json::from_str(include_str!("data/warlords.json")).unwrap());

#[derive(Debug)]
pub struct Ancillary {
    pub slot: &'static str,
    pub node: String,
    pub icon: String,
    pub rarity: String,
    classes: String,
    pub character_owned: bool,
    pub permission: Option<String>,
}

impl Ancillary {
    pub fn fits(&self, element: &str) -> bool {
        self.classes.is_empty()
            || self
                .classes
                .split_ascii_whitespace()
                .any(|class| class == element)
    }
}

pub static ANCILLARIES: LazyLock<BTreeMap<String, Ancillary>> = LazyLock::new(|| {
    let groups: BTreeMap<String, Vec<Vec<serde_json::Value>>> =
        serde_json::from_str(include_str!("starting_ancillaries.json")).unwrap();
    let mut items = BTreeMap::new();
    for (slot, _) in SLOTS {
        for row in &groups[slot] {
            let field = |i: usize| row[i].as_str().unwrap().to_owned();
            items.insert(
                field(0),
                Ancillary {
                    slot,
                    node: field(1),
                    icon: format!("ui/ancillaries/{}", field(2)),
                    rarity: field(3),
                    classes: field(4),
                    character_owned: row
                        .get(5)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    permission: row
                        .get(6)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                },
            );
        }
    }
    items
});

pub fn output_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("SectForge").join("mods")
}
