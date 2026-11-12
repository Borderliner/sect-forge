use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    sync::LazyLock,
};

use anyhow::{Context, Result};
use regex::Regex;
use rpfm_lib::files::pack::Pack;
use sha2::{Digest, Sha256};

use crate::{
    config::ANCILLARIES,
    game::{self, Row, add, add_rows, field, raw, set},
};

const ART_SETS: &str = "campaign_character_art_sets";
const ARTS: &str = "campaign_character_arts";
const FEMALE_MODEL: &str = "3k_main_art_set_generic_female_03_general_earth";
// Exact artwork keys whose female general records are present in the MTU/TROM
// roster. AW's image-only aliases omit the gender marker used by other cards.
const FEMALE_FACE_CARDS: &[&str] = &[
    "3k_main_hero_special_earth_wang_lanlan",
    "3k_main_hero_special_earth_zhang_yanyan",
    "3k_main_hero_special_water_wang_baolan",
    "3k_main_hero_special_water_wang_fangyue",
    "sad_han_ruilan_earth",
    "sad_han_wanwan_metal",
    "sad_huang_anbao_water",
    "sad_huang_yingxiu_metal",
    "sad_kong_yurui_water",
    "sad_liu_jiejie_earth",
    "sad_liu_min_water",
    "sad_liu_pingming_metal",
    "sad_shi_jiejin_water",
    "sad_shi_mengjuan_water",
    "sad_wang_baolan_water",
    "sad_wang_fangyue_water",
    "sad_ze_ningqin_metal",
    "sad_zhang_liyan_earth",
    "sad_zhang_zhumeng_metal",
];
const PORTRAIT_STILLS: &[&str] = &[
    "halfbody_large",
    "halfbody_small",
    "unitcards",
    "bobbleheads",
    "mini",
];
const SIZES: &[&str] = &[
    "halfbody_large",
    "fullbody_large",
    "large",
    "halfbody_small",
    "unitcards",
    "bobbleheads",
    "boobleheads",
    "mini",
    "faces",
    "nanman",
];
type Identity = (String, Option<String>);
type Images = BTreeMap<Identity, Vec<(usize, String)>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaceArt {
    Registered,
    Layered,
    Standalone,
}

static COMPOSITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?:faces/)?([^/]+)/)?(large_panel|small_panel)/(norm|happy|angry)/(norm|normal|happy|angry|noanim|face)\.png$").unwrap()
});
static PORTRAIT_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:3k_(?:main|dlc\d+|ytr)_|(?:mh|lb|sad|aw|x)_)(?:(?:.*?_)?hero_)?(?:special_|generic_)?").unwrap()
});

#[derive(Clone, Debug)]
pub struct Portrait {
    pub key: String,
    pub label: String,
    pub search: String,
    pub gender: &'static str,
    pub from_mod: bool,
    pub can_export: bool,
    pub game_paths: Vec<String>,
    pub mod_paths: Vec<String>,
    art_set: Option<Row>,
    art: Option<Row>,
    face_art: FaceArt,
}

pub struct FounderArt {
    pub key: String,
    pub is_male: bool,
}

pub fn words(key: &str) -> String {
    key.split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .unwrap()
                .to_uppercase()
                .chain(chars)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn label(key: &str) -> String {
    let name = key.split_once("_art_set_").map_or(key, |(_, name)| name);
    let name = name
        .strip_prefix("historical_")
        .and_then(|name| name.strip_suffix("_general"))
        .unwrap_or(name);
    let bandit = if key.contains("_bandit_") {
        " · Bandits"
    } else {
        ""
    };
    format!("{}{bandit}", words(name))
}

fn mod_images(pack: &Pack) -> Images {
    let mut images = Images::new();
    for path in pack.files().keys() {
        let lower = path.to_ascii_lowercase();
        let Some(relative) = lower.strip_prefix("ui/characters/") else {
            continue;
        };
        if !relative.ends_with(".png") {
            continue;
        }
        let (folder, card, rank) = if let Some((folder, tail)) = relative.split_once("/stills/") {
            let (size, _) = tail.split_once('/').unwrap_or((tail, ""));
            let card = tail.rsplit('/').next().unwrap().trim_end_matches(".png");
            let rank = if let Some(index) = SIZES.iter().position(|entry| *entry == size) {
                index * 2 + usize::from(tail.contains("/faces/") || size == "faces") * 8
            } else if !tail.contains('/') && folder.rsplit('/').next() == Some(card) {
                18
            } else {
                continue;
            };
            (
                folder.split("/composites/faces/").next().unwrap(),
                Some(card),
                rank,
            )
        } else if let Some((folder, tail)) = relative.split_once("/composites/") {
            let Some(captures) = COMPOSITE.captures(tail) else {
                continue;
            };
            let image = match &captures[4] {
                "normal" => "norm",
                image => image,
            };
            if !matches!(image, "noanim" | "face") && &captures[3] != image {
                continue;
            }
            let rank = if &captures[2] == "large_panel" { 1 } else { 3 }
                + usize::from(&captures[3] != "norm") * 5
                + usize::from(&captures[4] == "face") * 20;
            (
                folder,
                captures.get(1).map(|capture| capture.as_str()),
                rank,
            )
        } else {
            continue;
        };
        if card.is_some_and(|name| {
            name.contains("ancillary") || name.starts_with("armour_") || name.starts_with("armor_")
        }) {
            continue;
        }
        images
            .entry((folder.into(), card.map(str::to_owned)))
            .or_default()
            .push((rank, path.clone()));
    }
    let without_card = images
        .keys()
        .filter(|(_, card)| card.is_none())
        .cloned()
        .collect::<Vec<_>>();
    for identity in without_card {
        let folder = &identity.0;
        let own_card = folder.rsplit('/').next().unwrap();
        let cards = images
            .keys()
            .filter(|(key, card)| key == folder && card.is_some())
            .collect::<Vec<_>>();
        let matching = cards
            .iter()
            .find(|(_, card)| card.as_deref() == Some(own_card))
            .copied()
            .or_else(|| (cards.len() == 1).then(|| cards[0]))
            .cloned();
        if let Some(matching) = matching {
            let additional = images.remove(&identity).unwrap();
            images.get_mut(&matching).unwrap().extend(additional);
        }
    }
    for variants in images.values_mut() {
        variants.sort_by(|a, b| (a.0, a.1.len(), &a.1).cmp(&(b.0, b.1.len(), &b.1)));
    }
    images
}

pub fn catalog(game: &Pack, source: Option<&Pack>) -> Result<Vec<Portrait>> {
    let mut sets: BTreeMap<_, _> = game::rows(game, ART_SETS)?
        .into_iter()
        .map(|row| (field(&row, "art_set_id").to_owned(), row))
        .collect();
    let mut arts: BTreeMap<_, _> = game::rows(game, ARTS)?
        .into_iter()
        .map(|row| (row["id"].to_string(), row))
        .collect();
    let mut changed = BTreeSet::new();
    let images = source.map(mod_images).unwrap_or_default();
    if let Some(source) = source {
        for (table, key, target) in [(ART_SETS, "art_set_id", &mut sets), (ARTS, "id", &mut arts)] {
            let prefix = format!("db/{table}_tables/");
            let mut paths = source
                .files()
                .keys()
                .filter(|path| path.starts_with(&prefix))
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                // Some portrait packs include only images. Keep them browsable if a DB is unsupported.
                let Ok(rows) = game::rows_at(source, path) else {
                    continue;
                };
                for row in rows {
                    let id = if key == "id" {
                        row[key].to_string()
                    } else {
                        field(&row, key).to_owned()
                    };
                    changed.insert(field(&row, "art_set_id").to_owned());
                    target.insert(id, row);
                }
            }
        }
    }
    let mut rows: Vec<_> = arts.into_values().collect();
    rows.sort_by_key(|row| (row["level"].as_i64(), row["age"].as_i64()));
    let mut portraits: BTreeMap<String, Portrait> = BTreeMap::new();
    let mut claimed = BTreeSet::new();
    for art in rows {
        let key = field(&art, "art_set_id").to_owned();
        let Some(art_set) = sets.get(&key) else {
            continue;
        };
        let folder = field(&art, "portrait").trim_matches('/');
        let card = field(&art, "card");
        if field(art_set, "agent_type") != "general"
            || !art["has_come_of_age"].as_bool().unwrap_or(false)
            || folder.is_empty()
            || card.is_empty()
        {
            continue;
        }
        let folder_lower = folder.to_ascii_lowercase();
        let card_lower = card.to_ascii_lowercase();
        let mut identities = vec![
            (folder_lower.clone(), Some(card_lower.clone())),
            (folder_lower.clone(), None),
        ];
        if let Some((parent, name)) = folder_lower.rsplit_once('/')
            && name == card_lower
        {
            identities.push((parent.into(), Some(card_lower)));
        }
        let identity = identities
            .into_iter()
            .find(|candidate| images.contains_key(candidate));
        if portraits
            .get(&key)
            .is_some_and(|previous| identity.is_none() || !previous.mod_paths.is_empty())
        {
            continue;
        }
        let mut mod_paths = Vec::new();
        if let Some(identity) = &identity {
            mod_paths.extend(images[identity].iter().map(|(_, path)| path.clone()));
            claimed.insert(identity.clone());
        }
        let root = format!("ui/characters/{folder}");
        let mut game_paths = vec![format!("{root}/composites/small_panel/norm/norm.png")];
        for face in ["", "faces/"] {
            for size in [
                "halfbody_large",
                "halfbody_small",
                "unitcards",
                "bobbleheads",
            ] {
                game_paths.push(format!("{root}/stills/{size}/{face}{card}.png"));
            }
        }
        game_paths.push(format!("{root}/composites/small_panel/norm/noanim.png"));
        let Some(male) = art_set["is_male"].as_bool() else {
            continue;
        };
        portraits.insert(
            key.clone(),
            Portrait {
                label: label(&key),
                from_mod: changed.contains(&key) || identity.is_some(),
                key,
                search: format!("{folder}/{card}"),
                gender: if male { "Male" } else { "Female" },
                can_export: true,
                game_paths,
                mod_paths,
                art_set: Some(art_set.clone()),
                art: Some(art),
                face_art: FaceArt::Registered,
            },
        );
    }
    for ((folder, card), paths) in &images {
        if claimed.contains(&(folder.clone(), card.clone())) {
            continue;
        }
        let name = card
            .as_deref()
            .unwrap_or_else(|| folder.rsplit('/').next().unwrap());
        let search = format!("{folder}/{name}");
        let tokens = search
            .split(|c: char| !c.is_ascii_alphanumeric())
            .collect::<BTreeSet<_>>();
        let gender = if FEMALE_FACE_CARDS.contains(&name)
            || ["female", "lady", "empress"]
                .iter()
                .any(|word| tokens.contains(word))
        {
            "Female"
        } else if tokens.contains("male") {
            "Male"
        } else {
            "Unknown"
        };
        let key = format!(
            "asset_{}",
            &hex::encode(Sha256::digest(search.as_bytes()))[..20]
        );
        let mut paths = paths
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        let donor = (gender == "Female")
            .then(|| {
                portraits.values().find(|portrait| {
                    portrait.face_art == FaceArt::Registered
                        && portrait.gender == gender
                        && portrait.art.as_ref().is_some_and(|art| {
                            field(art, "portrait")
                                .trim_matches('/')
                                .eq_ignore_ascii_case(folder)
                        })
                })
            })
            .flatten();
        if donor.is_some()
            && let Some(number) = face_number(name)
        {
            // Some Workshop packs zero-pad the stills' face number but not the animated layers.
            let stills = paths.iter().any(|path| path.contains("/stills/"));
            let composites = paths.iter().any(|path| path.contains("/composites/"));
            for ((other_folder, other_card), variants) in &images {
                if other_folder == folder
                    && other_card
                        .as_deref()
                        .filter(|other| *other != name)
                        .and_then(face_number)
                        == Some(number)
                {
                    paths.extend(
                        variants
                            .iter()
                            .filter(|(_, path)| {
                                (!stills && path.contains("/stills/"))
                                    || (!composites && path.contains("/composites/"))
                            })
                            .map(|(_, path)| path.clone()),
                    );
                }
            }
        }
        let donor = donor.filter(|_| {
            paths
                .iter()
                .any(|path| path.contains("/stills/") && path.contains("/faces/"))
        });
        let mut face_art = if donor.is_some() {
            FaceArt::Layered
        } else {
            FaceArt::Registered
        };
        let (mut art_set, mut art) = donor.map_or((None, None), |portrait| {
            (portrait.art_set.clone(), portrait.art.clone())
        });
        let root = format!("ui/characters/{folder}/");
        if gender == "Female"
            && !["child", "baby", "armour", "armours", "armor", "armors"]
                .iter()
                .any(|token| tokens.contains(token))
            && let Some(model) = portraits.get(FEMALE_MODEL)
        {
            let standalone_face = has_complete_face_art(&root, &paths);
            let still = art.is_none()
                && paths.iter().any(|path| {
                    let path = path.to_ascii_lowercase();
                    path.contains("/stills/halfbody_large/") && !path.contains("/faces/")
                });
            if standalone_face || still {
                // Keep the source artwork and supply the missing campaign model.
                art_set = model.art_set.clone();
                art = model.art.clone();
                if let Some(art) = &mut art {
                    set(art, "portrait", format!("{folder}/"));
                    set(art, "card", name);
                    set(art, "fallback_composite_display", "EMPTY");
                }
                if standalone_face {
                    face_art = FaceArt::Standalone;
                }
            }
        }
        portraits.insert(
            key.clone(),
            Portrait {
                key,
                label: words(&PORTRAIT_PREFIX.replace(name, "")),
                gender,
                search,
                from_mod: true,
                can_export: art.is_some(),
                game_paths: Vec::new(),
                mod_paths: paths,
                art_set,
                art,
                face_art,
            },
        );
    }
    let mut result: Vec<_> = portraits
        .into_values()
        .filter(|portrait| source.is_none() || portrait.from_mod)
        .collect();
    result.sort_by_cached_key(|portrait| (portrait.label.to_lowercase(), portrait.key.clone()));
    Ok(result)
}

fn face_number(card: &str) -> Option<(&str, u32)> {
    let (prefix, number) = card.rsplit_once("_face_")?;
    Some((prefix, number.parse().ok()?))
}

fn relative_art_path<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let (folder, relative) = path.split_at_checked(root.len())?;
    folder.eq_ignore_ascii_case(root).then_some(relative)
}

fn has_complete_face_art(root: &str, paths: &[String]) -> bool {
    // Artwork packs can store full character images in face slots. Require
    // both campaign panels and the corresponding full-size portrait and card.
    let relative: Vec<_> = paths
        .iter()
        .filter_map(|path| relative_art_path(path, root).map(str::to_ascii_lowercase))
        .collect();
    ["halfbody_large", "unitcards"].into_iter().all(|size| {
        let prefix = format!("stills/{size}/faces/");
        relative.iter().any(|path| path.starts_with(&prefix))
    }) && ["large_panel", "small_panel"].into_iter().all(|panel| {
        let suffix = format!("/{panel}/norm/noanim.png");
        relative
            .iter()
            .any(|path| path.starts_with("composites/faces/") && path.ends_with(&suffix))
    })
}

pub fn export(
    game: &Pack,
    source: Option<&Pack>,
    output: &mut Pack,
    key: &str,
    portrait: &Portrait,
    element: &str,
) -> Result<FounderArt> {
    let mut art_set = portrait
        .art_set
        .clone()
        .context("Missing portrait art set")?;
    let is_male = art_set["is_male"]
        .as_bool()
        .context("Missing portrait gender")?;
    let mut art = portrait.art.clone().context("Missing portrait artwork")?;
    let uniform = field(&art, "uniform");
    let replacement_uniform = (!is_male && (uniform.is_empty() || uniform.contains("_hero_skin_")))
        .then(|| match element {
            // The generic Wood and Fire armour families have no female body meshes.
            "wood" => Some("3k_main_zheng_jiang"),
            "fire" => Some("3k_main_lady_sun"),
            _ if !uniform.starts_with("3k_main_hero_skin_female_") => {
                Some("3k_main_hero_skin_female_03")
            }
            _ => None,
        })
        .flatten();
    if let Some(uniform) = replacement_uniform {
        set(&mut art, "uniform", uniform);
    }
    let card = field(&art, "card");
    let donor = format!(
        "ui/characters/{}/",
        field(&art, "portrait").trim_matches('/')
    );
    let art_key = format!("{key}_portrait");
    if portrait.key.starts_with("asset_") {
        // Generated art is selected explicitly and can be used with any founder class.
        set(&mut art_set, "is_custom", true);
        for column in ["agent_subtype", "subculture", "faction"] {
            set(&mut art_set, column, "");
        }
    }
    let root = format!("ui/characters/{art_key}/");
    if let Some(source) = source {
        // Some packs put complete portraits in the face layer and blank out the armour.
        let full_portrait = portrait.face_art == FaceArt::Standalone
            || (!is_male
                && portrait
                    .mod_paths
                    .iter()
                    .any(|path| path.contains("/stills/") && path.contains("/faces/"))
                && has_transparent_armour(source, &donor, &art));
        if portrait.face_art == FaceArt::Layered || full_portrait {
            if full_portrait {
                blank_armour_layers(game, output, &root, element, is_male)?;
            }
            export_face(
                source,
                output,
                &donor,
                full_portrait.then_some(root.as_str()),
                &art_key,
                &portrait.mod_paths,
            )?;
            if full_portrait {
                // The founder belongs to a Han faction. Export the complete artwork in
                // the same layout as a unique Han character's stills and composites.
                set(&mut art_set, "culture", "3k_main_chinese");
                set(&mut art, "portrait", format!("{art_key}/"));
                set(&mut art, "fallback_composite_display", "EMPTY");
            }
            set(&mut art, "card", art_key.as_str());
            write_art(output, key, &art_key, art_set, art)?;
            return Ok(FounderArt {
                key: art_key,
                is_male,
            });
        }
    }
    let mut files = BTreeMap::new();
    for pack in [Some(game), source].into_iter().flatten() {
        for path in pack.files().keys() {
            if let Some(relative) = path.strip_prefix(&donor) {
                files.insert(relative, pack);
            }
        }
    }
    let card_file = format!("/{card}.png");
    if !files.keys().any(|path| {
        path.starts_with("stills/") && !path.contains("/faces/") && path.ends_with(&card_file)
    }) {
        if replacement_uniform.is_some() {
            // This portrait still uses its source's layered images.
            write_art(output, key, &art_key, art_set, art)?;
            return Ok(FounderArt {
                key: art_key,
                is_male,
            });
        }
        return Ok(FounderArt {
            key: portrait.key.clone(),
            is_male,
        });
    }
    blank_armour_layers(game, output, &root, element, is_male)?;
    // Put the standalone image above a transparent armour layer for each equipment variant.
    for (path, provider) in files {
        if path.starts_with("stills/") && path.ends_with(&card_file) {
            let asset = raw(provider, &format!("{donor}{path}"))?;
            let renamed = format!(
                "{}{art_key}.png",
                path.strip_suffix(&format!("{card}.png")).unwrap()
            );
            add(output, &format!("{root}{renamed}"), &asset)?;
            if !path.contains("/faces/")
                && let Some((folder, name)) = renamed["stills/".len()..].split_once('/')
            {
                add(
                    output,
                    &format!("{root}stills/{folder}/faces/{name}"),
                    &asset,
                )?;
            }
        } else if let Some(parts) =
            COMPOSITE.captures(path.strip_prefix("composites/").unwrap_or(""))
            && parts.get(1).is_none()
            && parts[4] != *"face"
        {
            let asset = raw(provider, &format!("{donor}{path}"))?;
            let destination = if let Some(prefix) = path.strip_suffix("/normal.png") {
                format!("{root}{prefix}/norm.png")
            } else {
                format!("{root}{path}")
            };
            add(output, &destination, &asset)?;
            add(
                output,
                &format!(
                    "{root}composites/faces/{art_key}/{}/{}/face.png",
                    &parts[2], &parts[3]
                ),
                &asset,
            )?;
        }
    }
    set(&mut art, "portrait", format!("{art_key}/"));
    set(&mut art, "card", art_key.as_str());
    write_art(output, key, &art_key, art_set, art)?;
    Ok(FounderArt {
        key: art_key,
        is_male,
    })
}

fn has_transparent_armour(source: &Pack, donor: &str, art: &Row) -> bool {
    let armour = field(art, "fallback_composite_display");
    let Ok(bytes) = raw(source, &format!("{donor}stills/unitcards/{armour}.png")) else {
        return false;
    };
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    let Some(size) = reader
        .output_buffer_size()
        .filter(|size| *size <= 1024 * 1024)
    else {
        return false;
    };
    let mut pixels = vec![0; size];
    let Ok(info) = reader.next_frame(&mut pixels) else {
        return false;
    };
    matches!(
        info.color_type,
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
    ) && pixels[..info.buffer_size()]
        .chunks_exact(info.color_type.samples())
        .all(|pixel| pixel.last() == Some(&0))
}

fn blank_armour_layers(
    game: &Pack,
    output: &mut Pack,
    root: &str,
    element: &str,
    is_male: bool,
) -> Result<()> {
    let mut folders = vec![element];
    if element == "water" {
        // Strategist tunics and Water armour use different portrait directories.
        folders.push("water_strategist");
    }
    let genders: &[&str] = if is_male {
        &["male"]
    } else {
        &["female", "male"]
    };
    let prefixes: Vec<_> = folders
        .iter()
        .flat_map(|folder| {
            genders
                .iter()
                .map(move |gender| format!("ui/characters/{folder}/{gender}/"))
        })
        .collect();
    let mut layers = BTreeSet::new();
    for path in game.files().keys() {
        if let Some(suffix) = prefixes.iter().find_map(|prefix| path.strip_prefix(prefix))
            && suffix.contains("_ancillary_armour_")
            && suffix.ends_with(".png")
        {
            layers.insert(suffix.to_owned());
            if !is_male {
                layers.insert(
                    suffix
                        .replace("_male.png", "_female.png")
                        .replace("_male/", "_female/"),
                );
            }
        }
    }
    if !is_male {
        // Wood and Fire use shared equipment even though vanilla has no generic women in those classes.
        for (key, _) in ANCILLARIES
            .iter()
            .filter(|(_, item)| item.slot == "armour" && item.fits(element))
        {
            for name in [key.clone(), format!("{key}_female")] {
                for size in PORTRAIT_STILLS {
                    layers.insert(format!("stills/{size}/{name}.png"));
                    layers.insert(format!("stills/{size}/large/{name}.png"));
                }
                for panel in ["large_panel", "small_panel"] {
                    for mood in ["norm", "happy", "angry"] {
                        layers.insert(format!("composites/{name}/{panel}/{mood}/{mood}.png"));
                    }
                }
            }
        }
    }
    for suffix in layers {
        add(
            output,
            &format!("{root}{suffix}"),
            include_bytes!("data/blank.png"),
        )?;
    }
    Ok(())
}

fn write_art(
    output: &mut Pack,
    key: &str,
    art_key: &str,
    mut art_set: Row,
    mut art: Row,
) -> Result<()> {
    set(&mut art_set, "art_set_id", art_key);
    set(&mut art, "id", 1900001400);
    set(&mut art, "art_set_id", art_key);
    add_rows(output, ART_SETS, key, vec![art_set])?;
    add_rows(output, ARTS, key, vec![art])
}

fn export_face(
    source: &Pack,
    output: &mut Pack,
    donor: &str,
    standalone: Option<&str>,
    card: &str,
    paths: &[String],
) -> Result<()> {
    let root = standalone.unwrap_or(donor);
    let mut stills = BTreeMap::new();
    let mut composites = BTreeMap::new();
    for path in paths {
        let Some(relative) = relative_art_path(path, donor).map(str::to_ascii_lowercase) else {
            continue;
        };
        if relative.starts_with("stills/") && relative.contains("/faces/") {
            let (folder, _) = relative.rsplit_once('/').unwrap();
            stills.entry(folder.to_owned()).or_insert(path);
        } else if let Some(face) = relative.strip_prefix("composites/faces/") {
            let (_, suffix) = face.split_once('/').context("Invalid animated face path")?;
            composites.entry(suffix.to_owned()).or_insert(path);
        }
    }
    let default = stills
        .get("stills/halfbody_large/faces")
        .or_else(|| stills.values().next())
        .context("Missing portrait stills")?;
    for size in PORTRAIT_STILLS {
        let folder = format!("stills/{size}/faces");
        let large = format!("{folder}/large");
        for (target, alternate) in [(&folder, &large), (&large, &folder)] {
            let path = stills
                .get(target.as_str())
                .or_else(|| stills.get(alternate.as_str()))
                .unwrap_or(default);
            let image = raw(source, path)?;
            add(output, &format!("{root}{target}/{card}.png"), &image)?;
            if standalone.is_some() {
                // Equipped characters also request layered stills, even for unique art sets.
                let suffix = if target == &large { "/large" } else { "" };
                add(
                    output,
                    &format!("{root}stills/{size}{suffix}/{card}.png"),
                    &image,
                )?;
            }
        }
    }
    for (panel, size) in [
        ("large_panel", "halfbody_large"),
        ("small_panel", "halfbody_small"),
    ] {
        let source_mood = |mood: &str| {
            composites
                .get(&format!("{panel}/{mood}/face.png"))
                .or_else(|| {
                    if standalone.is_none() {
                        return None;
                    }
                    ["noanim", mood, "normal"].into_iter().find_map(|filename| {
                        composites.get(&format!("{panel}/{mood}/{filename}.png"))
                    })
                })
        };
        let still = format!("stills/{size}/faces");
        for mood in ["norm", "happy", "angry"] {
            let suffix = format!("{panel}/{mood}/face.png");
            let path = source_mood(mood)
                .or_else(|| source_mood("norm"))
                .or_else(|| stills.get(still.as_str()))
                .unwrap_or(default);
            let image = raw(source, path)?;
            add(
                output,
                &format!("{root}composites/faces/{card}/{suffix}"),
                &image,
            )?;
            if standalone.is_some() {
                let image = standalone_composite(&image, mood)
                    .with_context(|| format!("Prepare {panel} portrait from {path}"))?;
                add(
                    output,
                    &format!("{root}composites/{panel}/{mood}/{mood}.png"),
                    &image,
                )?;
            }
        }
    }
    Ok(())
}

fn standalone_composite(image: &[u8], mood: &str) -> Result<Vec<u8>> {
    let mut decoder = png::Decoder::new(Cursor::new(image));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![
        0;
        reader
            .output_buffer_size()
            .context("Portrait is too large")?
    ];
    let info = reader.next_frame(&mut pixels)?;

    // The source face layer can describe several moods in one file. The unique
    // portrait format stores one mood and its placement per composite.
    let placement = reader
        .info()
        .uncompressed_latin1_text
        .iter()
        .filter(|text| text.keyword == "Comment")
        .flat_map(|text| text.text.lines())
        .filter_map(|line| line.trim().strip_prefix("[type:")?.split_once(';'))
        .min_by_key(|(kind, _)| match *kind {
            kind if kind == mood => 0,
            "norm" => 1,
            _ => 2,
        })
        .map(|(_, placement)| placement)
        .unwrap_or("x:0;y:80;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]");

    let mut result = Vec::new();
    let mut encoder = png::Encoder::new(&mut result, info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    encoder.add_text_chunk("Comment".into(), format!("[type:{mood};{placement}"))?;
    encoder
        .write_header()?
        .write_image_data(&pixels[..info.buffer_size()])?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::row;

    #[test]
    fn artwork_only_face_pack_supports_standalone_and_equipped_portraits() -> Result<()> {
        let mut game = game::new_pack("vanilla.pack");
        add_rows(
            &mut game,
            ART_SETS,
            "data__",
            vec![row! {
                "art_set_id": FEMALE_MODEL, "is_male": false, "agent_type": "general",
                "culture": "3k_main_chinese",
            }],
        )?;
        add_rows(
            &mut game,
            ARTS,
            "data__",
            vec![row! {
                "id": 1, "art_set_id": FEMALE_MODEL, "has_come_of_age": true,
                "portrait": "earth/female/", "card": "vanilla_female",
                "uniform": "3k_main_hero_skin_female_03",
            }],
        )?;
        let armour = "stills/unitcards/large/3k_main_ancillary_armour_strategist_light_armour_water_common_female.png";
        add(
            &mut game,
            &format!("ui/characters/water_strategist/female/{armour}"),
            b"vanilla armour artwork",
        )?;
        let mut composite = Vec::new();
        let mut encoder = png::Encoder::new(&mut composite, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.add_text_chunk(
            "Comment".into(),
            "[type:norm;x:12;y:34;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]".into(),
        )?;
        encoder
            .write_header()?
            .write_image_data(&[80, 40, 15, 255])?;
        let mut source = game::new_pack("addon.pack");
        let card = include_bytes!("data/blank.png");
        for size in ["halfbody_large", "unitcards"] {
            add(
                &mut source,
                &format!("ui/characters/ADD_UNIQUE/stills/{size}/faces/SAD_shi_mengjuan_water.png"),
                card,
            )?;
        }
        for panel in ["large_panel", "small_panel"] {
            add(
                &mut source,
                &format!(
                    "ui/characters/ADD_UNIQUE/composites/faces/SAD_shi_mengjuan_water/{panel}/norm/noanim.png"
                ),
                &composite,
            )?;
        }
        add(
            &mut source,
            "ui/characters/ADD_UNIQUE/stills/unitcards/faces/SAD_lady_incomplete_water.png",
            card,
        )?;
        let portraits = catalog(&game, Some(&source))?;
        let portrait = portraits
            .iter()
            .find(|item| item.label == "Shi Mengjuan Water")
            .unwrap();
        let incomplete = portraits
            .iter()
            .find(|item| item.label == "Lady Incomplete Water")
            .unwrap();
        assert!(portrait.can_export);
        assert_eq!(portrait.gender, "Female");
        assert!(!incomplete.can_export);
        let mut output = game::new_pack("output.pack");
        let art = export(
            &game,
            Some(&source),
            &mut output,
            "founder",
            portrait,
            "water",
        )?;
        assert!(!art.is_male);
        let rows = game::rows_at(&output, &game::db_path(ARTS, "founder"))?;
        assert_eq!(field(&rows[0], "portrait"), "founder_portrait/");
        assert_eq!(field(&rows[0], "uniform"), "3k_main_hero_skin_female_03");
        assert_eq!(field(&rows[0], "fallback_composite_display"), "EMPTY");
        let root = "ui/characters/founder_portrait";
        for folder in ["", "large/", "faces/", "faces/large/"] {
            assert_eq!(
                raw(
                    &output,
                    &format!("{root}/stills/unitcards/{folder}founder_portrait.png")
                )?,
                card
            );
        }
        assert_eq!(
            raw(&output, &format!("{root}/{armour}"))?,
            include_bytes!("data/blank.png")
        );
        let image = raw(
            &output,
            &format!("{root}/composites/large_panel/happy/happy.png"),
        )?;
        let reader = png::Decoder::new(Cursor::new(image)).read_info()?;
        assert_eq!(
            reader.info().uncompressed_latin1_text[0].text,
            "[type:happy;x:12;y:34;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]"
        );
        Ok(())
    }

    #[test]
    fn water_armour_layers_include_strategist_tunics() -> Result<()> {
        let root = "ui/characters/founder_portrait/";
        let still =
            "stills/unitcards/large/3k_main_ancillary_armour_light_tunic_unique_water_female.png";
        let composite = "composites/ep_ancillary_armour_light_tunic_unique_water_female/small_panel/norm/norm.png";
        let regular = "stills/unitcards/3k_main_ancillary_armour_light_leather_partial_refined_shared_female.png";
        let mut game = game::new_pack("vanilla.pack");
        for (folder, asset) in [
            ("water_strategist", still),
            ("water_strategist", composite),
            ("water", regular),
        ] {
            add(
                &mut game,
                &format!("ui/characters/{folder}/female/{asset}"),
                b"armour",
            )?;
        }
        let mut output = game::new_pack("output.pack");
        blank_armour_layers(&game, &mut output, root, "water", false)?;
        for asset in [still, composite, regular] {
            assert_eq!(
                raw(&output, &format!("{root}{asset}"))?,
                include_bytes!("data/blank.png")
            );
        }
        Ok(())
    }

    #[test]
    fn complete_face_art_exports_as_a_unique_portrait() -> Result<()> {
        let donor = "ui/characters/nanman/female/";
        let root = "ui/characters/founder_portrait/";
        let still = format!("{donor}stills/unitcards/faces/example.png");
        let composite = format!("{donor}composites/faces/example/large_panel/norm/face.png");
        let mut portrait = Vec::new();
        let mut encoder = png::Encoder::new(&mut portrait, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.add_text_chunk(
            "Comment".into(),
            "[type:norm;x:0;y:80;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]\n\
             [type:happy;x:9;y:42;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]"
                .into(),
        )?;
        encoder
            .write_header()?
            .write_image_data(&[80, 40, 15, 255])?;

        let mut source = game::new_pack("source.pack");
        add(&mut source, &still, &portrait)?;
        add(&mut source, &composite, &portrait)?;
        let mut output = game::new_pack("output.pack");
        export_face(
            &source,
            &mut output,
            donor,
            Some(root),
            "founder_portrait",
            &[still, composite],
        )?;

        assert_eq!(output.files().len(), PORTRAIT_STILLS.len() * 4 + 12);
        let card = raw(
            &output,
            &format!("{root}stills/unitcards/founder_portrait.png"),
        )?;
        assert_eq!(card, portrait);
        assert_eq!(
            raw(
                &output,
                &format!("{root}composites/faces/founder_portrait/large_panel/norm/face.png"),
            )?,
            portrait
        );
        let image = raw(
            &output,
            &format!("{root}composites/large_panel/happy/happy.png"),
        )?;
        let reader = png::Decoder::new(Cursor::new(image)).read_info()?;
        let comments = &reader.info().uncompressed_latin1_text;
        assert_eq!(comments.len(), 1);
        assert_eq!(
            comments[0].text,
            "[type:happy;x:9;y:42;z-order:0;pivot_x:0.5000;pivot_y:0.5000;]"
        );
        Ok(())
    }
}
