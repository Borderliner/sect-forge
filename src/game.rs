use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use anyhow::{Context, Result, anyhow};
use rpfm_lib::{
    files::{
        Container, DecodeableExtraData, EncodeableExtraData, FileType, RFile, RFileDecoded, db::DB,
        loc::Loc, pack::Pack, table::DecodedData,
    },
    games::{
        GameInfo,
        pfh_file_type::PFHFileType,
        pfh_version::PFHVersion,
        supported_games::{KEY_THREE_KINGDOMS, SupportedGames},
    },
    schema::Schema,
};
use serde_json::{Map, Value};

mod traits;
pub use traits::{Trait, TraitCategory};

pub const MAP_IMAGE: &str = "campaign_maps/3k_dlc07_main_map/3k_main_minimap.png";
const CEO_LOC: &str = "text/db/ceo_nodes__.loc";
const STEAM_APP: u32 = 779340;

static SUPPORTED: LazyLock<SupportedGames> = LazyLock::new(SupportedGames::default);
static SCHEMA: LazyLock<Result<Schema, String>> = LazyLock::new(|| {
    ron::from_str(include_str!("data/schema_3k.ron")).map_err(|error| error.to_string())
});

pub type Row = Map<String, Value>;
pub type Localization = BTreeMap<String, String>;

fn info() -> &'static GameInfo {
    SUPPORTED.game(KEY_THREE_KINGDOMS).unwrap()
}

fn schema() -> Result<&'static Schema> {
    SCHEMA
        .as_ref()
        .map_err(|error| anyhow!("Bundled Three Kingdoms schema: {error}"))
}

pub fn discover_game() -> Option<PathBuf> {
    let steam = steamlocate::SteamDir::locate().ok()?;
    let (app, library) = steam.find_app(STEAM_APP).ok()??;
    Some(library.resolve_app_dir(&app))
}

pub fn workshop_packs() -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    if let Ok(steam) = steamlocate::SteamDir::locate()
        && let Ok(libraries) = steam.libraries()
    {
        for library in libraries.flatten() {
            let content = library
                .path()
                .join("steamapps")
                .join("workshop")
                .join("content")
                .join(STEAM_APP.to_string());
            for folder in read_dir(&content).filter(|path| path.is_dir()) {
                files.extend(read_dir(&folder).filter(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("pack"))
                }));
            }
        }
    }
    let mut files: Vec<_> = files.into_iter().collect();
    files.sort_by_cached_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    files
}

fn read_dir(path: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
}

pub struct Game {
    pub pack: Arc<Pack>,
    pub loc: Localization,
    local_en: PathBuf,
}

impl Game {
    pub fn open(path: &Path) -> Result<Self> {
        let pack = Pack::read_and_merge_ca_packs(info(), path)
            .with_context(|| format!("Read the Three Kingdoms game files in {}", path.display()))?;
        let local_en = path.join("data/local_en.pack");
        let loc = match localization(&pack, CEO_LOC) {
            Ok(loc) => loc,
            Err(_) => localization(&open_pack(&local_en)?, CEO_LOC)
                .context("Read the game's character and ancillary names")?,
        };
        Ok(Self {
            pack: Arc::new(pack),
            loc,
            local_en,
        })
    }
}

pub fn open_pack(path: &Path) -> Result<Pack> {
    Pack::read_and_merge(&[path.to_owned()], info(), true, false, true)
        .with_context(|| format!("Open pack {}", path.display()))
}

pub fn template() -> Result<Pack> {
    // The PFH5 reader resolves file offsets through a backing file even in eager mode.
    let mut template = tempfile::Builder::new()
        .prefix("sectforge-template-")
        .suffix(".pack")
        .tempfile()?;
    template.write_all(include_bytes!("data/hepu-template.pack"))?;
    template.flush()?;
    Pack::read_and_merge(&[template.path().to_owned()], info(), false, false, true)
        .context("Read the bundled Hepu campaign template")
}

pub fn new_pack(name: &str) -> Pack {
    let mut pack = Pack::new_with_name_and_version(name, PFHVersion::PFH5);
    pack.set_pfh_file_type(PFHFileType::Mod);
    pack
}

pub fn save(pack: &mut Pack, path: &Path) -> Result<()> {
    let mut extra = EncodeableExtraData::new_from_game_info(info());
    extra.set_regenerate_table_guid(false);
    pack.save(Some(path), info(), &Some(extra))
        .context("Save mod pack")
}

pub fn raw(pack: &Pack, path: &str) -> Result<Vec<u8>> {
    pack.file(path, true)
        .cloned()
        .with_context(|| format!("Pack is missing {path}"))?
        .encode(&None, false, false, true)?
        .with_context(|| format!("Read {path} from pack"))
}

pub fn add(pack: &mut Pack, path: &str, bytes: &[u8]) -> Result<()> {
    pack.insert(RFile::new_from_vec(bytes, FileType::Unknown, 0, path))?;
    Ok(())
}

pub fn db_path(table: &str, suffix: &str) -> String {
    format!("db/{table}_tables/{suffix}")
}

fn decoded(pack: &Pack, path: &str) -> Result<RFileDecoded> {
    let mut extra = DecodeableExtraData::default();
    extra.set_schema(Some(schema()?));
    extra.set_game_info(Some(info()));
    pack.file(path, true)
        .cloned()
        .with_context(|| format!("Pack is missing {path}"))?
        .decode(&Some(extra), false, true)
        .with_context(|| format!("Decode {path}"))?
        .with_context(|| format!("No decoded contents for {path}"))
}

pub fn localization(pack: &Pack, path: &str) -> Result<Localization> {
    let RFileDecoded::Loc(loc) = decoded(pack, path)? else {
        anyhow::bail!("{path} is not localization");
    };
    Ok(loc
        .data()
        .iter()
        .map(|row| {
            (
                row[0].data_to_string().into_owned(),
                row[1].data_to_string().into_owned(),
            )
        })
        .collect())
}

pub fn add_loc(pack: &mut Pack, path: &str, rows: Localization) -> Result<()> {
    let mut loc = Loc::new();
    let data = rows
        .into_iter()
        .map(|(key, value)| {
            vec![
                DecodedData::StringU16(key),
                DecodedData::StringU16(value),
                DecodedData::Boolean(false),
            ]
        })
        .collect::<Vec<_>>();
    loc.set_data(&data)?;
    pack.insert(RFile::new_from_decoded(&RFileDecoded::Loc(loc), 0, path))?;
    Ok(())
}

fn value(cell: &DecodedData) -> Value {
    use DecodedData::*;
    match cell {
        Boolean(value) => Value::from(*value),
        F32(value) => Value::from(*value),
        F64(value) => Value::from(*value),
        I16(value) | OptionalI16(value) => Value::from(*value),
        I32(value) | OptionalI32(value) => Value::from(*value),
        I64(value) | OptionalI64(value) => Value::from(*value),
        SequenceU16(value) | SequenceU32(value) => Value::from(value.clone()),
        _ => Value::from(cell.data_to_string().into_owned()),
    }
}

pub fn rows(pack: &Pack, table: &str) -> Result<Vec<Row>> {
    rows_at(pack, &db_path(table, "data__"))
}

pub fn rows_at(pack: &Pack, path: &str) -> Result<Vec<Row>> {
    let RFileDecoded::DB(table) = decoded(pack, path)? else {
        anyhow::bail!("{path} is not a database table");
    };
    let fields = table.definition().fields_processed();
    Ok(table
        .data()
        .iter()
        .map(|row| {
            fields
                .iter()
                .zip(row)
                .map(|(field, cell)| (field.name().to_owned(), value(cell)))
                .collect()
        })
        .collect())
}

pub fn add_rows(pack: &mut Pack, table: &str, suffix: &str, rows: Vec<Row>) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let name = format!("{table}_tables");
    let schema = schema()?;
    // The shared schema also includes Assembly Kit and later, unused definitions.
    let campaign_version = match table {
        "faction_uniform_colours"
        | "pooled_resource_factors"
        | "campaign_group_pooled_resources"
        | "cdir_events_dilemma_option_junctions"
        | "cdir_events_dilemma_payloads" => Some(0),
        "dilemmas" => Some(1),
        "political_parties" => Some(6),
        _ => None,
    };
    let definition = schema
        .definitions_by_table_name(&name)
        .and_then(|versions| {
            versions
                .iter()
                .filter(|def| campaign_version.is_none_or(|version| *def.version() == version))
                .max_by_key(|def| def.version())
        })
        .with_context(|| format!("No database schema for {table}"))?;
    let mut db = DB::new(definition, None, &name);
    let columns: BTreeMap<_, _> = definition
        .fields_processed()
        .iter()
        .enumerate()
        .map(|(i, field)| (field.name(), (i, field.field_type())))
        .collect();
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        let mut record = db.new_row();
        for (field, value) in row {
            let Some((index, kind)) = columns.get(field.as_str()) else {
                continue;
            };
            let source = match value {
                Value::Bool(flag) => DecodedData::Boolean(flag),
                Value::Number(ref number) => {
                    if let Some(integer) = number.as_i64() {
                        DecodedData::I64(integer)
                    } else {
                        DecodedData::F64(number.as_f64().unwrap_or_default())
                    }
                }
                Value::String(text) => DecodedData::StringU8(text),
                Value::Array(bytes) => {
                    let bytes = bytes
                        .iter()
                        .map(|value| value.as_u64().unwrap_or_default() as u8)
                        .collect();
                    record[*index] = match record[*index] {
                        DecodedData::SequenceU16(_) => DecodedData::SequenceU16(bytes),
                        DecodedData::SequenceU32(_) => DecodedData::SequenceU32(bytes),
                        _ => anyhow::bail!("Unexpected nested record in {table}.{field}"),
                    };
                    continue;
                }
                _ => continue,
            };
            record[*index] = source
                .convert_between_types(kind)
                .with_context(|| format!("Encode {table}.{field}"))?;
        }
        records.push(record);
    }
    db.set_data(&records)?;
    pack.insert(RFile::new_from_decoded(
        &RFileDecoded::DB(db),
        0,
        &db_path(table, suffix),
    ))?;
    Ok(())
}

pub fn field<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or_default()
}

pub fn set(row: &mut Row, key: &str, value: impl Into<Value>) {
    row.insert(key.into(), value.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_tables_use_retail_db_versions() -> Result<()> {
        let mut pack = new_pack("versions.pack");
        let extra = Some(EncodeableExtraData::new_from_game_info(info()));
        for (table, version) in [
            ("faction_uniform_colours", 0),
            ("political_parties", 6),
            ("pooled_resource_factors", 0),
            ("campaign_group_pooled_resources", 0),
            ("dilemmas", 1),
            ("cdir_events_dilemma_choice_details", 2),
            ("cdir_events_dilemma_option_junctions", 0),
            ("cdir_events_dilemma_payloads", 0),
        ] {
            add_rows(&mut pack, table, "fixture", vec![Row::new()])?;
            let mut file = pack
                .file(&db_path(table, "fixture"), false)
                .unwrap()
                .clone();
            let bytes = file.encode(&extra, false, false, true)?.unwrap();
            let (written, _, _, _) = DB::read_header(&mut std::io::Cursor::new(bytes))?;
            assert_eq!(written, version, "{table}");
        }
        Ok(())
    }
}
