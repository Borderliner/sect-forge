"""Customize the two CAAB start positions in the original White Lotus pack.

The template already has its own faction, leader, diplomacy and settlement.
Keep record sizes and references stable and edit only the template's identity.
"""

import hashlib
import lzma
import struct


BASE_KEY = "sects_white_lotus"
BASE_ART = "3k_main_art_set_generic_male_48_general_wood"
CAMPAIGN = "3k_main_campaign_map"

# Offsets in the decompressed files, pinned to the exact BOB-built inputs.
LAYOUTS = {
    "historical": (
        "b6c417be00662d2c50a00aab79fbadec8e9807a6da07ecf663c5c7e0690a25cd",
        60844, 2404828, 2404895, 357277,
    ),
    "romance": (
        "82df164c2c8303ff15fd8539b603703017ec7c39547b506d548a71bfbe584151",
        60845, 2404565, 2404632, 357196,
    ),
}
LZMA_FILTERS = [dict(id=lzma.FILTER_LZMA1, dict_size=262144, lc=3, lp=0, pb=2)]


def uint(data, offset):
    return struct.unpack_from("<I", data, offset)[0]


def size_field(value):
    if not 0 <= value <= 0xFFFFFFFF:
        raise ValueError("Start position size is outside the CAAB format")
    return bytes(((value >> shift) & 127) | 128 for shift in (28, 21, 14, 7)) + bytes([value & 127])


def read_size(data, offset):
    value = 0
    for byte in data[offset:offset + 5]:
        value = (value << 7) | (byte & 127)
        if byte < 128:
            return value
    raise ValueError("Invalid CAAB size field")


def split_tables(data):
    """Return the record data, unchanged name/UTF-16 tables and UTF-8 strings."""
    offset = uint(data, 12)
    count = struct.unpack_from("<H", data, offset)[0]
    end = offset + 2
    for _ in range(count):
        end += 2 + struct.unpack_from("<H", data, end)[0]
    count = uint(data, end)
    end += 4
    for _ in range(count):
        end += 6 + 2 * struct.unpack_from("<H", data, end)[0]
    prefix = data[offset:end]
    count = uint(data, end)
    end += 4
    strings = {}
    for _ in range(count):
        length = struct.unpack_from("<H", data, end)[0]
        end += 2
        strings[uint(data, end + length)] = data[end:end + length].decode("utf-8")
        end += length + 4
    if end != len(data) or len(strings) != count:
        raise ValueError("Unexpected CAAB string table")
    return bytearray(data[:offset]), prefix, strings


def write_tables(records, prefix, strings):
    struct.pack_into("<I", records, 12, len(records))
    result = records + prefix + struct.pack("<I", len(strings))
    for index, text in strings.items():
        encoded = text.encode("utf-8")
        result += struct.pack("<H", len(encoded)) + encoded + struct.pack("<I", index)
    return bytes(result)


def open_world(records, marker):
    end = len(records) - 15
    if (records[:4] != b"\xca\xab\x00\x00"
            or records[marker] != 0x46 or marker + 6 + read_size(records, marker + 1) != end
            or records[end:end + 4] != b"\x80\x28\x0c\x08"
            or records[-7:] != b"\x46\x05\x5d\x00\x00\x04\x00"):
        raise ValueError("Unexpected campaign compression record")
    expected = uint(records, end + 4)
    decoder = lzma.LZMADecompressor(format=lzma.FORMAT_RAW, filters=LZMA_FILTERS)
    world = decoder.decompress(records[marker + 6:end], max_length=expected)
    if len(world) != expected:
        raise ValueError("Campaign world did not decompress to its expected length")
    return world


def customize(data, mode, faction_key, art_set, primary, secondary):
    checksum, marker, portrait_ref, template_ref, flag_record = LAYOUTS[mode]
    if hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError(f"The {mode} start position differs from the supported Hepu template")

    outer, outer_names, outer_strings = split_tables(data)
    world, world_names, world_strings = split_tables(open_world(outer, marker))
    if (world[portrait_ref] != 0x0F or world_strings[uint(world, portrait_ref + 1)] != BASE_ART
            or world[template_ref] != 0x0F or world_strings[uint(world, template_ref + 1)] != BASE_KEY + "_leader"):
        raise ValueError(f"Could not locate the {mode} campaign leader")

    for strings in (outer_strings, world_strings):
        for index, value in strings.items():
            if BASE_KEY in value:
                strings[index] = value.replace(BASE_KEY, faction_key)

    # The baseline art also belongs to a vanilla general. Change this reference alone.
    portrait_index = next((i for i, key in world_strings.items() if key == art_set), None)
    if portrait_index is None:
        portrait_index = max(world_strings) + 1
        world_strings[portrait_index] = art_set
    struct.pack_into("<I", world, portrait_ref + 1, portrait_index)

    # FACTION_FLAG_AND_COLOURS: flag string, followed by nine typed bytes (RGB x 3).
    channels = flag_record + 7
    if world[flag_record - 1:flag_record + 2] != b"\x82\x9f\x17" or world[channels:channels + 18:2] != b"\x06" * 9:
        raise ValueError(f"Could not locate the {mode} campaign faction colours")
    world[channels + 1:channels + 18:2] = bytes.fromhex(primary + secondary + primary)

    new_world = write_tables(world, world_names, world_strings)
    payload = lzma.compress(new_world, format=lzma.FORMAT_RAW, filters=LZMA_FILTERS)
    # The compressed data record is the final child of the root.
    info = outer[-15:-11] + struct.pack("<I", len(new_world)) + outer[-7:]
    outer[marker - 5:] = size_field(len(payload) + 6 + len(info)) + b"\x46" + size_field(len(payload)) + payload + info
    outer[20:25] = size_field(len(outer) - 25)
    return write_tables(outer, outer_names, outer_strings)
