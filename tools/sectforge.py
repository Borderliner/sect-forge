"""Build a standalone Three Kingdoms mod from the 190 Hepu scenario."""

import argparse
import base64
from contextlib import contextmanager
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tempfile
import time
import unicodedata
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import uuid4
import zlib

from sect_startpos import BASE_ART, BASE_KEY, CAMPAIGN, LAYOUTS, customize


ROOT = Path(__file__).resolve().parent.parent
TEMPLATE = ROOT / "build" / "SectsCults_white_lotus.pack"
# The game rejected the working directory for this project (Sects&Cults) when
# loading a mod list. Keep the normal export path outside that directory.
OUTPUT = Path(os.getenv("LOCALAPPDATA") or Path.home()) / "SectForge" / "mods"
URL = "http://127.0.0.1:45127/mcp"
ART_SETS = "campaign_character_art_sets"
ARTS = "campaign_character_arts"
TABLE_KEYS = {
    "factions": "key",
    "names": "id",
    "character_generation_templates": "key",
    "character_generation_template_game_mode_details": "character_generation_template",
    "frontend_factions": "faction",
    "frontend_faction_to_frontend_faction_leaders": "frontend_faction",
    "frontend_faction_leaders": "key",
    "frontend_characters": "key",
    "building_chain_availabilities": "faction",
    "cai_personality_group_overrides": "faction_key",
    "campaign_group_member_criteria_factions": "faction",
    "campaign_map_attrition_faction_immunities": "faction",
    "campaign_stances_factions_junctions": "faction",
    "faction_agent_permitted_subtypes": "faction",
    "faction_banners": "key",
    "faction_civil_war_setups": "primary_faction",
    "faction_to_name_groups": "faction",
    "faction_uniform_colours": "faction_name",
    "unit_banners": "key",
}


class Rpfm:
    def __init__(self):
        self.session = None
        self.ids = itertools.count(1)
        try:
            self.connect()
        except (URLError, TimeoutError) as exc:
            if isinstance(exc, HTTPError):
                raise
            self.start_server()
        self.call("set_game", game="three_kingdoms")

    def start_server(self):
        exe = (os.getenv("SECTFORGE_RPFM") or shutil.which("rpfm_server.exe")
               or str(Path.home() / "Apps" / "RPFM" / "rpfm_server.exe"))
        if not Path(exe).is_file():
            raise ValueError("RPFM Server is required. Set SECTFORGE_RPFM to its executable path.")
        subprocess.Popen([exe], creationflags=subprocess.CREATE_NO_WINDOW,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(40):
            try:
                self.connect()
                return
            except (URLError, TimeoutError):
                time.sleep(0.25)
        raise ValueError("Could not connect to RPFM Server on port 45127")

    def request(self, method, params=None, notification=False):
        payload = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            payload["params"] = params
        if not notification:
            payload["id"] = next(self.ids)
        headers = {"Accept": "application/json, text/event-stream", "Content-Type": "application/json"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        request = Request(URL, data=json.dumps(payload).encode(), headers=headers, method="POST")
        with urlopen(request, timeout=120) as response:
            self.session = response.headers.get("Mcp-Session-Id", self.session)
            body = response.read().decode()
        if notification:
            return None
        frames = [line[6:] for line in body.splitlines() if line.startswith("data: {")]
        answer = json.loads(frames[-1] if frames else body)
        if "error" in answer:
            raise ValueError(answer["error"]["message"])
        return answer["result"]

    def connect(self):
        self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                    "clientInfo": {"name": "Sect Forge", "version": "1.0"}})
        self.request("notifications/initialized", notification=True)

    def call(self, tool_name, **arguments):
        try:
            result = self.request("tools/call", {"name": tool_name, "arguments": arguments})
            if result.get("isError"):
                raise ValueError(" ".join(item.get("text", "") for item in result["content"]))
            data = result.get("structuredContent", {})
            if tool_name == "wait_for_job":
                return data
            while data.get("state") in ("queued", "running"):
                data = self.call("wait_for_job", job=data["job"])
            if data.get("state") == "finished":
                return data["result"]
            if data.get("state") in ("failed", "cancelled"):
                raise ValueError(data.get("error", {}).get("message", "Job was cancelled"))
            return data
        except (ValueError, OSError) as exc:
            raise ValueError(f"RPFM {tool_name}: {exc}") from exc

    @contextmanager
    def pack(self, *, path=None, name=None):
        opened = (self.call("open_pack", paths=[str(path)]) if path is not None
                  else self.call("new_pack", name=name))
        try:
            yield {"pack": opened["key"]}
        finally:
            self.call("close_pack", pack=opened["key"])

    def files(self, source, prefix):
        offset = 0
        while True:
            page = self.call("list_files", source=source, path_prefix=prefix, offset=offset, limit=1000)
            for entry in page["files"]:
                yield entry["path"]
            offset += len(page["files"])
            if offset >= page["total"] or not page["files"]:
                return

    def rows(self, source, path, **filters):
        offset = 0
        while True:
            page = self.call("table_rows", file={"source": source, "path": path}, offset=offset, limit=1000,
                             filters=[{"column": key, "op": "equals", "value": str(value)}
                                      for key, value in filters.items()])
            for row in page["rows"]:
                yield dict(zip(page["columns"], row["values"]))
            offset += len(page["rows"])
            if offset >= page["total"] or not page["rows"]:
                return

    def raw(self, source, path):
        data = self.call("read_file", file={"source": source, "path": path}, format="raw")
        return base64.b64decode(data["base64"] if "base64" in data else data["contents"]["base64"])


def db(table, suffix="data__"):
    return f"db/{table}_tables/{suffix}"


def label_art(key):
    name = re.sub(r"^.*?_art_set_", "", key)
    match = re.fullmatch(r"historical_(.+)_general", name)
    display = (match[1] if match else name).replace("_", " ").title()
    return display + " · Bandits" if "_bandit_" in key else display


def portrait_catalog(api, source=None, vanilla=None):
    """Resolve adult male art sets, with an optional mod overriding vanilla assets."""
    if vanilla is None:
        vanilla = (
            {r["art_set_id"]: r for r in api.rows("game_files", db(ART_SETS))},
            {r["id"]: r for r in api.rows("game_files", db(ARTS))},
            {},
        )
    sets, art, mod_images = (dict(part) for part in vanilla)
    changed = set()
    if source:
        for path in api.files(source, f"db/{ART_SETS}_tables/"):
            for row in api.rows(source, path):
                sets[row["art_set_id"]] = row
                changed.add(row["art_set_id"])
        for path in api.files(source, f"db/{ARTS}_tables/"):
            for row in api.rows(source, path):
                art[row["id"]] = row
                changed.add(row["art_set_id"])
        mod_images.update((path.lower(), path) for path in api.files(source, "ui/characters/") if path.endswith(".png"))
    results = {}
    for row in sorted(art.values(), key=lambda row: (row["level"], row["age"])):
        key = row["art_set_id"]
        art_set = sets.get(key, {})
        if (key in results or art_set.get("agent_type") != "general" or not art_set.get("is_male")
                or not row["has_come_of_age"] or not row["portrait"] or not row["card"]):
            continue
        folder, card = row["portrait"].strip("/"), row["card"]
        root = f"ui/characters/{folder}"
        candidates = [f"{root}/composites/small_panel/norm/norm.png"]
        candidates.extend(f"{root}/stills/{size}/{card}.png" for size in ("halfbody_large", "halfbody_small", "unitcards", "bobbleheads"))
        candidates.extend(f"{root}/stills/{size}/faces/{card}.png" for size in ("halfbody_small", "unitcards", "bobbleheads"))
        candidates.append(f"{root}/composites/small_panel/norm/noanim.png")
        override = next((mod_images[path.lower()] for path in candidates if path.lower() in mod_images), None)
        results[key] = {"key": key, "label": label_art(key), "paths": candidates,
                        "art_set_row": art_set, "art_row": row,
                        "override": override, "from_mod": key in changed or override is not None}
    return results, vanilla


def settings(config):
    values = dict(config)
    for field in ("sectName", "forename", "surname"):
        value = str(values.get(field) or "").strip()
        if not value or len(value) > 64 or any(ord(char) < 32 for char in value):
            raise ValueError(f"{field} must contain 1 to 64 characters")
        values[field] = value
    for field in ("primaryColour", "secondaryColour"):
        value = str(values.get(field, "")).strip().removeprefix("#").upper()
        if not re.fullmatch(r"[0-9A-F]{6}", value):
            raise ValueError(f"{field} must be a six-digit hex colour")
        values[field] = value
    if values.get("isMale", True) is not True or values.get("subtype", "3k_general_wood") != "3k_general_wood":
        raise ValueError("The Hepu scenario uses a male Wood-class leader")
    slug = unicodedata.normalize("NFKD", values["sectName"]).encode("ascii", "ignore").decode().lower()
    slug = re.sub(r"[^a-z0-9]+", "_", slug).strip("_")
    if not slug:
        from hashlib import sha256
        slug = sha256(values["sectName"].encode()).hexdigest()[:12]
    key = values.get("factionKey") or "sects_" + slug[:42].rstrip("_")
    if not re.fullmatch(r"sects_[a-z0-9][a-z0-9_]{0,47}", key) or key.endswith("_separatists"):
        raise ValueError("Faction key must begin with sects_ and use lowercase ASCII letters, digits or underscores")
    values["factionKey"] = key
    values["artSet"] = values.get("artSet") or BASE_ART
    if not re.fullmatch(r"[A-Za-z0-9_]{1,160}", values["artSet"]):
        raise ValueError("Invalid portrait art-set key")
    return values


def make_rows(api, source, config):
    new_key = config["factionKey"]
    for table, column in TABLE_KEYS.items():
        targets = {
            "names": ("1900000001", "1900000002"),
            "character_generation_templates": (BASE_KEY + "_leader",),
            "character_generation_template_game_mode_details": (BASE_KEY + "_leader",),
            "frontend_characters": (BASE_KEY + "_leader_faction_leader",),
            "frontend_faction_leaders": (BASE_KEY + "_ruler",),
            "frontend_factions": (BASE_KEY,),
            "frontend_faction_to_frontend_faction_leaders": (BASE_KEY,),
            "faction_civil_war_setups": (BASE_KEY,),
        }.get(table, (BASE_KEY, BASE_KEY + "_separatists"))
        rows = []
        for target in targets:
            rows.extend(api.rows(source, db(table), **{column: target}))
        minimum = len(targets) if table in ("names", "factions") else 0
        if table.startswith("frontend_") or table == "character_generation_templates":
            minimum = 1
        if table == "character_generation_template_game_mode_details":
            minimum = 2
        if len(rows) < minimum:
            raise ValueError(f"The scenario template is missing required {table} rows")
        for row in rows:
            for name, value in row.items():
                if isinstance(value, str) and BASE_KEY in value:
                    row[name] = value.replace(BASE_KEY, new_key)
            if table == "factions":
                stem = f"{new_key}_" + ("sep_" if row["key"].endswith("_separatists") else "")
                row.update({name: stem + name for name in ("screen_name", "screen_adjective", "attack_desc", "defend_desc")})
                row.update(primary_colour_hex=config["primaryColour"], secondary_colour_hex=config["secondaryColour"],
                           uniform_colour_hex=config["primaryColour"], card_colour_hex=config["primaryColour"])
            if table == "character_generation_templates":
                row["art_set_override"] = config["artSet"]
            if table == "frontend_faction_leaders":
                row.update(video="3k_main_faction_intro_wood", loading_screen_intro_video="3k_main_faction_intro_wood")
        if rows:
            yield table, rows


def localization(config):
    key, name = config["factionKey"], config["sectName"]
    rows = {
        "names_name_1900000001": config["forename"], "names_name_1900000002": config["surname"],
        "start_pos_factions_description_1900000000": name, "start_pos_factions_long_description_1900000000": name,
        "frontend_faction_leaders_localised_subtitle_" + key + "_ruler": name,
        "frontend_faction_leaders_localised_description_" + key + "_ruler": f"{name} begins in Hepu in 190 CE.",
    }
    for faction, display, stem in [(key, name, key + "_"),
                                    (key + "_separatists", name + " Separatists", key + "_sep_")]:
        rows.update({f"factions_screen_name_{faction}": display, f"factions_screen_adjective_{faction}": display,
                     f"factions_screen_name_when_rebels_{faction}": f"{display} Renegades",
                     stem + "screen_name": display, stem + "screen_adjective": display,
                     stem + "attack_desc": f"You are attacking a {display} force.",
                     stem + "defend_desc": f"You are defending against a {display} force."})
    rows["start_pos_factions_description_1900000050"] = name + " Separatists"
    rows["start_pos_factions_long_description_1900000050"] = name + " Separatists"
    return [{"key": loc_key, "text": text, "tooltip": False} for loc_key, text in rows.items()]


def insert_rows(api, pack, path, kind, rows):
    api.call("create_file", pack=pack, path=path, kind=kind)
    result = api.call("edit_table", pack=pack, path=path, edits=[{"op": "insert", "values": row} for row in rows])
    if result["row_count"] != len(rows):
        raise ValueError(f"Unexpected row count in {path}")


def blank_portrait_layer():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\0" * 5)) + chunk(b"IEND", b""))


def add_portrait(api, source, pack, stage, faction_key, portrait):
    """Provide generic portrait layers for a selected portrait drawn as a single image."""
    art = portrait["art_row"]
    card = art["card"]
    donor_root = "ui/characters/" + art["portrait"].strip("/") + "/"
    files = {path.removeprefix(donor_root): "game_files" for path in api.files("game_files", donor_root)}
    if source:
        files.update((path.removeprefix(donor_root), source) for path in api.files(source, donor_root))
    # A generic face already supports compositing with the leader's equipped armour.
    if not any(path.startswith("stills/") and "/faces/" not in path and path.endswith(f"/{card}.png")
               for path in files):
        return portrait["key"]

    art_key = faction_key + "_portrait"
    art_root = "ui/characters/" + art_key + "/"
    paths = {}
    blank = blank_portrait_layer()
    generic_root = "ui/characters/wood/male/"
    for path in api.files("game_files", generic_root):
        if "3k_main_ancillary_armour_" in path and path.endswith(".png"):
            paths[art_root + path.removeprefix(generic_root)] = blank

    for path, provider in files.items():
        if path.startswith("stills/") and path.endswith(f"/{card}.png"):
            asset = api.raw(provider, donor_root + path)
            renamed = path.removesuffix(card + ".png") + art_key + ".png"
            paths[art_root + renamed] = asset
            if "/faces/" not in path:
                folder, variant = renamed.split("/", 2)[1:]
                paths[f"{art_root}stills/{folder}/faces/{variant}"] = asset
        elif re.fullmatch(r"composites/(small_panel|large_panel)/(norm|angry|happy)/[^/]+\.png", path):
            asset = api.raw(provider, donor_root + path)
            paths[art_root + path] = asset
            _, panel, mood, _ = path.split("/")
            paths[f"{art_root}composites/faces/{art_key}/{panel}/{mood}/face.png"] = asset

    disk_paths = []
    for index, data in enumerate(paths.values()):
        file = stage / f"portrait-{index}.png"
        file.write_bytes(data)
        disk_paths.append(str(file))
    api.call("add_files_from_disk", pack=pack, paths=disk_paths, pack_paths=list(paths))
    sets_row = dict(portrait["art_set_row"], art_set_id=art_key)
    arts_row = dict(art, id=1900001400, art_set_id=art_key, portrait=art_key + "/", card=art_key)
    for table, row in ((ART_SETS, sets_row), (ARTS, arts_row)):
        insert_rows(api, pack, db(table, faction_key), {"type": "db", "table_name": table + "_tables"}, [row])
    return art_key


@contextmanager
def staging_folder(parent):
    stage = parent / (".sectforge-" + uuid4().hex)
    stage.mkdir()
    try:
        yield stage
    finally:
        shutil.rmtree(stage)


def build(config, output=OUTPUT, progress=print):
    config = settings(config)
    key, art = config["factionKey"], config["artSet"]
    api = Rpfm()
    dependencies = []
    progress("Checking the selected portrait")
    mod_path = config.get("portraitPack")
    if mod_path:
        mod_path = Path(mod_path).expanduser().resolve(strict=True)
        if mod_path.suffix.lower() != ".pack":
            raise ValueError("Select an RPFM-readable .pack for the portrait source")
        with api.pack(path=mod_path) as source:
            portraits, _ = portrait_catalog(api, source)
        if art not in portraits or not portraits[art]["from_mod"]:
            raise ValueError(f"{art} is not an available portrait in {mod_path.name}")
        dependencies = [{"name": mod_path.name, "enabled": True}]
    else:
        portraits, _ = portrait_catalog(api)
        if art not in portraits:
            raise ValueError(f"Unknown or incompatible vanilla portrait: {art}")

    output.mkdir(parents=True, exist_ok=True)
    filename = f"{key}.pack"
    target = output / filename
    # A separately running RPFM server also needs write access to the build stage.
    with staging_folder(Path(tempfile.gettempdir())) as stage:
        with api.pack(path=TEMPLATE) as template, api.pack(name=filename) as destination:
            pack = destination["pack"]
            progress("Preparing the campaign portrait")
            if mod_path:
                with api.pack(path=mod_path) as source:
                    art = add_portrait(api, source, pack, stage, key, portraits[art])
            else:
                art = add_portrait(api, None, pack, stage, key, portraits[art])
            config["artSet"] = art
            for mode in LAYOUTS:
                progress(f"Writing the {mode.title()} campaign start")
                name = f"startpos_{mode}.esf"
                path = f"campaigns/{CAMPAIGN}/{name}"
                baseline = api.raw(template, path)
                file = stage / name
                file.write_bytes(customize(baseline, mode, key, art, config["primaryColour"], config["secondaryColour"]))
                api.call("add_files_from_disk", pack=pack, paths=[str(file)], pack_paths=[path])
            progress("Wiring faction, leader and campaign selection")
            for table, rows in make_rows(api, template, config):
                insert_rows(api, pack, db(table, key), {"type": "db", "table_name": table + "_tables"}, rows)
            groups = "faction_to_faction_groups_junctions"
            insert_rows(api, pack, db(groups, key), {"type": "db", "table_name": groups + "_tables"}, [
                {"faction_group_key": "3k_the_coalition", "faction_key": key, "optional_campaign_key": CAMPAIGN},
            ])
            # The Start Campaign handler resolves the selected leader to a political party.
            for table, rows in {
                "political_parties": [
                    {"key": key + "_ruler", "playable": True, "effect_bundle": "3k_main_dummy_effect_bundle", "initial_power": 60},
                    {"key": key + "_council", "playable": False, "effect_bundle": "3k_main_dummy_effect_bundle", "initial_power": 40},
                ],
                "faction_political_parties_junctions": [
                    {"faction_key": key, "political_party_key": key + "_ruler"},
                    {"faction_key": key + "_separatists", "political_party_key": key + "_council"},
                ],
                "political_parties_frontend_leaders_junctions": [
                    {"frontend_leader": key + "_ruler", "political_party": key + "_ruler"},
                ],
            }.items():
                insert_rows(api, pack, db(table, key), {"type": "db", "table_name": table + "_tables"}, rows)
            insert_rows(api, pack, f"text/db/{key}.loc", {"type": "loc"}, localization(config))
            api.call("update_pack", pack=pack, pack_type="Mod", dependencies=dependencies)
            api.call("save_pack", pack=pack, path=str(stage / filename), disable_uuid_regeneration=True)
        progress("Verifying the saved pack")
        with api.pack(path=stage / filename) as saved:
            files = set(api.files(saved, ""))
            expected = {f"campaigns/{CAMPAIGN}/startpos_{mode}.esf" for mode in LAYOUTS}
            expected.update(db(table, key) for table in ("factions", "frontend_characters", "frontend_factions",
                            "frontend_faction_leaders", "frontend_faction_to_frontend_faction_leaders",
                            "character_generation_templates", "character_generation_template_game_mode_details",
                            "faction_to_faction_groups_junctions", "names", "political_parties",
                            "faction_political_parties_junctions", "political_parties_frontend_leaders_junctions"))
            expected.add(f"text/db/{key}.loc")
            if not expected <= files:
                raise ValueError("Saved pack is missing required files: " + ", ".join(sorted(expected - files)))
        with staging_folder(output) as publish:
            shutil.copyfile(stage / filename, publish / filename)
            try:
                os.replace(publish / filename, target)
            except PermissionError as exc:
                if os.name == "nt" and exc.winerror in (5, 32) and target.exists():
                    raise ValueError(f"Could not replace {filename}. Close Three Kingdoms or any pack editor holding it and retry.") from exc
                raise
    return target


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args()
    try:
        config = json.loads(args.config.read_text(encoding="utf-8-sig"))
        print(build(config, args.output.resolve(), progress=lambda message: print(message, flush=True)))
    except (ValueError, OSError, KeyError) as exc:
        parser.exit(1, f"Build failed: {exc}\n")


if __name__ == "__main__":
    main()
