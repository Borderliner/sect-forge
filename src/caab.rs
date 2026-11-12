//! Lossless editing of the CAAB records used by the campaign and CEO data.

use std::{collections::HashMap, str};

use anyhow::{Context, Result, bail, ensure};

use crate::startpos::{Strings, join, short, size_field, sized, split, uint};

const RECORD: u8 = 0x80;
const ARRAY: u8 = 0x40;
const EXTENDED: u8 = 0x20;
const WIDE: u8 = 0x0e;
const TEXT: u8 = 0x0f;
const UNSIGNED: [u8; 6] = [8, 0x14, 0x15, 0x16, 0x17, 0x18];

#[derive(Debug)]
pub struct Node {
    pub start: usize,
    end: usize,
    kind: u8,
    name: u16,
    version: u8,
    pub groups: Vec<Vec<Node>>,
}

impl Node {
    pub fn fields(&self) -> &[Node] {
        self.groups.first().map_or(&[], Vec::as_slice)
    }

    fn visit<'a>(&'a self, predicate: &impl Fn(&Node) -> bool, result: &mut Vec<&'a Node>) {
        if predicate(self) {
            result.push(self);
        }
        for node in self.groups.iter().flatten() {
            node.visit(predicate, result);
        }
    }
}

fn parse(data: &[u8], start: usize, name_count: usize, depth: usize) -> Result<Node> {
    ensure!(depth < 128, "CAAB nesting limit exceeded");
    let kind = *data.get(start).context("Truncated CAAB node")?;
    let mut at = start + 1;
    let mut node = Node {
        start,
        end: at,
        kind,
        name: 0,
        version: 0,
        groups: Vec::new(),
    };
    if kind & RECORD == 0 {
        node.end = match kind {
            0x12..=0x15 | 0x19 | 0x1d => at,
            1 | 2 | 6 | 0x16 | 0x1a | 0x23 => at + 1,
            3 | 7 | 0x10 | 0x17 | 0x1b | 0x24 => at + 2,
            0x18 | 0x1c => at + 3,
            4 | 8 | 0x0a | WIDE | TEXT | 0x21 | 0x25 => at + 4,
            5 | 9 | 0x0b | 0x0c => at + 8,
            0x0d => at + 12,
            0x41..=0x5d => {
                let (len, first) = sized(data, at)?;
                first + len
            }
            0x26 => {
                let count = usize::from(*data.get(at).context("Truncated opaque CAAB scalar")?);
                let end = at
                    + 1
                    + if count > 0 && count % 8 == 0 {
                        count
                    } else {
                        7
                    };
                end + usize::from(data.get(end) == Some(&0x9c))
            }
            _ => bail!("Unsupported CAAB node {kind:#04x} at {start}"),
        };
        ensure!(
            node.end <= data.len(),
            "CAAB scalar runs past the end of the document"
        );
        return Ok(node);
    }
    if depth == 0 || kind & EXTENDED != 0 {
        node.name = short(data, at)?;
        node.version = *data.get(at + 2).context("Truncated CAAB record header")?;
        at += 3;
    } else {
        node.name =
            (u16::from(kind & 1) << 8) | u16::from(*data.get(at).context("Truncated CAAB name")?);
        node.version = (kind >> 1) & 15;
        at += 1;
    }
    ensure!(
        (node.name as usize) < name_count,
        "Unknown CAAB record name"
    );
    let (len, after_size) = sized(data, at)?;
    let array = kind & ARRAY != 0;
    let (count, mut at) = if array {
        sized(data, after_size)?
    } else {
        (1, after_size)
    };
    let end = at + len;
    ensure!(
        end <= data.len() && (!array || count <= len),
        "Invalid CAAB record bounds"
    );
    for _ in 0..count {
        let group_end = if array {
            let (size, first) = sized(data, at)?;
            at = first;
            first + size
        } else {
            end
        };
        ensure!(group_end <= end, "Invalid CAAB group bounds");
        let mut fields = Vec::new();
        while at < group_end {
            let child = parse(data, at, name_count, depth + 1)?;
            at = child.end;
            fields.push(child);
        }
        ensure!(at == group_end, "CAAB field crossed a group boundary");
        node.groups.push(fields);
    }
    ensure!(at == end, "CAAB group count does not match record length");
    node.end = end;
    Ok(node)
}

pub struct Document {
    pub data: Vec<u8>,
    prefix: Vec<u8>,
    pub strings: Strings,
    wide: Strings,
    names: Vec<String>,
    pub root: Node,
}

impl Document {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let (data, prefix, strings) = split(bytes)?;
        let count = short(&prefix, 0)?;
        let mut names = Vec::with_capacity(count.into());
        let mut at = 2;
        for _ in 0..count {
            let len = short(&prefix, at)? as usize;
            at += 2;
            names.push(
                str::from_utf8(prefix.get(at..at + len).context("Truncated CAAB name")?)?
                    .to_owned(),
            );
            at += len;
        }
        let count = uint(&prefix, at)?;
        at += 4;
        let mut wide = Strings::new();
        for _ in 0..count {
            let len = short(&prefix, at)? as usize * 2;
            at += 2;
            let id = uint(&prefix, at + len)?;
            let units = prefix[at..at + len]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            wide.insert(id, String::from_utf16_lossy(&units));
            at += len + 4;
        }
        let root = parse(&data, 16, names.len(), 0)?;
        ensure!(root.end == data.len(), "Unexpected data after CAAB root");
        Ok(Self {
            data,
            prefix,
            strings,
            wide,
            names,
            root,
        })
    }

    pub fn name<'a>(&'a self, node: &Node) -> &'a str {
        if node.kind & RECORD != 0 {
            &self.names[node.name as usize]
        } else {
            ""
        }
    }

    pub fn raw(&self, node: &Node) -> &[u8] {
        &self.data[node.start..node.end]
    }

    pub fn text(&self, node: &Node) -> Option<&str> {
        let table = match node.kind {
            TEXT => &self.strings,
            WIDE => &self.wide,
            _ => return None,
        };
        table
            .get(&uint(&self.data, node.start + 1).ok()?)
            .map(String::as_str)
    }

    pub fn number(&self, node: &Node) -> Option<u32> {
        let bytes = &self.raw(node)[1..];
        match node.kind {
            0x14 | 0x15 => Some((node.kind - 0x14).into()),
            6 | 0x16 => Some(bytes[0].into()),
            7 | 0x17 => Some(u16::from_le_bytes([bytes[0], bytes[1]]).into()),
            0x18 => Some(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]])),
            8 => uint(bytes, 0).ok(),
            _ => None,
        }
    }

    pub fn float(&self, node: &Node) -> Option<f32> {
        match self.raw(node) {
            [0x0a, a, b, c, d] => Some(f32::from_le_bytes([*a, *b, *c, *d])),
            [0x1d] => Some(0.),
            _ => None,
        }
    }

    pub fn id(&self, node: &Node) -> Result<u32> {
        self.number(node)
            .with_context(|| format!("Expected a CAAB identifier at {}", node.start))
    }

    pub fn find<'a>(&'a self, node: &'a Node, name: &str) -> Vec<&'a Node> {
        let mut found = Vec::new();
        node.visit(&|node| self.name(node) == name, &mut found);
        found
    }

    pub fn one<'a>(&'a self, node: &'a Node, name: &str) -> Result<&'a Node> {
        let nodes = self.find(node, name);
        ensure!(
            nodes.len() == 1,
            "Expected one {name} record, found {}",
            nodes.len()
        );
        Ok(nodes[0])
    }

    pub fn record(&self, name: &str, column: usize, key: &str) -> Result<&Node> {
        self.find(&self.root, name)
            .into_iter()
            .find(|node| node.fields().get(column).and_then(|f| self.text(f)) == Some(key))
            .with_context(|| format!("Missing {name} record for {key}"))
    }

    pub fn group(&self, name: &str, column: usize, key: &str) -> Result<&[Node]> {
        self.one(&self.root, name)?
            .groups
            .iter()
            .find(|group| group.get(column).and_then(|f| self.text(f)) == Some(key))
            .map(Vec::as_slice)
            .with_context(|| format!("Missing {name} entry for {key}"))
    }
}

pub fn integer(value: u32) -> Vec<u8> {
    let little = value.to_le_bytes();
    match value {
        0..=1 => vec![0x14 + value as u8],
        2..=0xff => vec![0x16, little[0]],
        0x100..=0xffff => vec![0x17, little[0], little[1]],
        0x10000..=0xffffff => vec![0x18, little[2], little[1], little[0]],
        _ => [vec![8], little.to_vec()].concat(),
    }
}

pub fn remap(doc: &Document, node: &Node, ids: &HashMap<u32, u32>) -> HashMap<usize, Vec<u8>> {
    let mut found = Vec::new();
    node.visit(&|node| UNSIGNED.contains(&node.kind), &mut found);
    found
        .into_iter()
        .filter_map(|node| Some((node.start, integer(*ids.get(&doc.number(node)?)?))))
        .collect()
}

pub struct Edit<'a> {
    pub doc: &'a Document,
    patches: HashMap<usize, Vec<u8>>,
    groups: HashMap<usize, Vec<Vec<u8>>>,
    strings: Strings,
    name_ids: HashMap<&'a str, u16>,
    wide_ids: HashMap<&'a str, u32>,
    text_ids: HashMap<String, u32>,
    next_text: u32,
}

impl<'a> Edit<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            patches: HashMap::new(),
            groups: HashMap::new(),
            strings: doc.strings.clone(),
            name_ids: doc
                .names
                .iter()
                .enumerate()
                .map(|(i, name)| (name.as_str(), i as u16))
                .collect(),
            wide_ids: doc
                .wide
                .iter()
                .map(|(i, name)| (name.as_str(), *i))
                .collect(),
            text_ids: doc
                .strings
                .iter()
                .rev()
                .map(|(i, name)| (name.clone(), *i))
                .collect(),
            next_text: doc.strings.keys().max().map_or(0, |id| id + 1),
        }
    }

    fn text_id(&mut self, value: &str) -> u32 {
        if let Some(index) = self.text_ids.get(value) {
            return *index;
        }
        let index = self.next_text;
        self.next_text += 1;
        self.strings.insert(index, value.into());
        self.text_ids.insert(value.into(), index);
        index
    }

    pub fn rename(&mut self, from: &str, to: &str, excluded: &str) {
        for text in self.strings.values_mut() {
            if !text.contains(excluded) && text.contains(from) {
                *text = text.replace(from, to);
            }
        }
        self.text_ids = self
            .strings
            .iter()
            .rev()
            .map(|(i, name)| (name.clone(), *i))
            .collect();
    }

    pub fn replace(&mut self, node: &Node, bytes: Vec<u8>) {
        self.patches.insert(node.start, bytes);
    }

    pub fn replace_int(&mut self, node: &Node, value: u32) {
        self.replace(node, integer(value));
    }

    pub fn replace_text(&mut self, node: &Node, value: &str) {
        let mut bytes = vec![TEXT];
        bytes.extend(self.text_id(value).to_le_bytes());
        self.replace(node, bytes);
    }

    pub fn append(&mut self, array: &Node, bytes: Vec<u8>) {
        self.groups.entry(array.start).or_default().push(bytes);
    }

    pub fn import(
        &mut self,
        source: &Document,
        node: &Node,
        overrides: &HashMap<usize, Vec<u8>>,
    ) -> Result<Vec<u8>> {
        if let Some(bytes) = overrides.get(&node.start) {
            return Ok(bytes.clone());
        }
        if node.kind & RECORD != 0 {
            let name = source.name(node);
            let id = *self
                .name_ids
                .get(name)
                .with_context(|| format!("Missing CAAB record type {name}"))?;
            let flags = RECORD | (node.kind & ARRAY);
            let header = if id < 512 && node.version < 16 {
                vec![flags | node.version << 1 | (id >> 8) as u8, id as u8]
            } else {
                vec![flags | EXTENDED, id as u8, (id >> 8) as u8, node.version]
            };
            let mut groups = Vec::new();
            for group in &node.groups {
                let mut bytes = Vec::new();
                for field in group {
                    bytes.extend(self.import(source, field, overrides)?);
                }
                groups.push(bytes);
            }
            return encode(header, flags & ARRAY != 0, &groups, 1, 1, &[]);
        }
        if matches!(node.kind, TEXT | WIDE) {
            let value = source.text(node).context("Missing CAAB string")?;
            let id = if node.kind == TEXT {
                self.text_id(value)
            } else {
                *self.wide_ids.get(value).context("Unknown UTF-16 string")?
            };
            let mut bytes = vec![node.kind];
            bytes.extend(id.to_le_bytes());
            return Ok(bytes);
        }
        if matches!(node.kind, 0x4e | 0x4f) {
            let (size, start) = sized(&source.data, node.start + 1)?;
            ensure!(size % 4 == 0, "Malformed CAAB string array");
            let mut bytes = source.data[node.start..start].to_vec();
            let wide = node.kind == 0x4e;
            let strings = if wide { &source.wide } else { &source.strings };
            for at in (start..node.end).step_by(4) {
                let value = strings
                    .get(&uint(&source.data, at)?)
                    .context("Missing CAAB array string")?;
                let id = if wide {
                    *self
                        .wide_ids
                        .get(value.as_str())
                        .context("Unknown UTF-16 array string")?
                } else {
                    self.text_id(value)
                };
                bytes.extend(id.to_le_bytes());
            }
            return Ok(bytes);
        }
        Ok(source.raw(node).to_vec())
    }

    fn render(&self, node: &Node) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.patches.get(&node.start) {
            return Ok(Some(bytes.clone()));
        }
        if node.kind & RECORD == 0 {
            return Ok(None);
        }
        let mut changed = self.groups.contains_key(&node.start);
        let mut rendered = Vec::with_capacity(node.groups.len());
        for group in &node.groups {
            let mut values = Vec::with_capacity(group.len());
            for field in group {
                let bytes = self.render(field)?;
                changed |= bytes.is_some();
                values.push(bytes);
            }
            rendered.push(values);
        }
        if !changed {
            return Ok(None);
        }
        let data = &self.doc.data;
        let header_end = node.start
            + if node.start == 16 || node.kind & EXTENDED != 0 {
                4
            } else {
                2
            };
        let (_, after_size) = sized(data, header_end)?;
        let array = node.kind & ARRAY != 0;
        let after_count = if array {
            sized(data, after_size)?.1
        } else {
            after_size
        };
        let mut cursor = after_count;
        let mut widths = Vec::new();
        let mut groups = Vec::new();
        for (group, values) in node.groups.iter().zip(rendered) {
            if array {
                let (len, at) = sized(data, cursor)?;
                widths.push(at - cursor);
                cursor = at + len;
            }
            let mut bytes = Vec::new();
            for (field, value) in group.iter().zip(values) {
                bytes.extend(value.as_deref().unwrap_or_else(|| self.doc.raw(field)));
            }
            groups.push(bytes);
        }
        if let Some(added) = self.groups.get(&node.start) {
            groups.extend(added.iter().cloned());
        }
        Ok(Some(encode(
            data[node.start..header_end].to_vec(),
            array,
            &groups,
            after_size - header_end,
            after_count - after_size,
            &widths,
        )?))
    }

    pub fn finish(self) -> Result<Vec<u8>> {
        let records = match self.render(&self.doc.root)? {
            Some(root) => [&self.doc.data[..16], &root].concat(),
            None => self.doc.data.clone(),
        };
        join(records, &self.doc.prefix, &self.strings)
    }
}

fn append_size(bytes: &mut Vec<u8>, size: usize, min: usize) -> Result<()> {
    let bits = usize::BITS - size.leading_zeros();
    bytes.extend(size_field(
        size,
        (bits.div_ceil(7) as usize).max(min).max(1),
    )?);
    Ok(())
}

fn encode(
    mut header: Vec<u8>,
    array: bool,
    groups: &[Vec<u8>],
    size_width: usize,
    count_width: usize,
    widths: &[usize],
) -> Result<Vec<u8>> {
    if array {
        let mut body = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            append_size(&mut body, group.len(), widths.get(i).copied().unwrap_or(1))?;
            body.extend(group);
        }
        append_size(&mut header, body.len(), size_width)?;
        append_size(&mut header, groups.len(), count_width)?;
        header.extend(body);
    } else {
        let body = groups.first().context("Missing CAAB record body")?;
        append_size(&mut header, body.len(), size_width)?;
        header.extend(body);
    }
    Ok(header)
}
