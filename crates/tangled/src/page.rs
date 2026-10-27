//! Page CRDT: RGA text + LWW title/blob, plus compact snapshots (spec 06).

use std::collections::{HashMap, HashSet};

use crate::cid::Cid;
use crate::constants::{PAGE_RGA_MAX_ATOMS, PAGE_TEXT_MAX_SCALARS, PAGE_TITLE_MAX_SCALARS};
use crate::error::{Error, Result};

const ROOT_OP: &str = "";

/// Atom identity `(op CID, i)`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AtomId {
    pub op: String,
    pub i: u64,
}

impl AtomId {
    pub fn key(&self) -> String {
        format!("{}:{}", self.op, self.i)
    }
}

#[derive(Clone, Debug)]
pub struct Atom {
    pub id: AtomId,
    pub parent: Option<AtomId>,
    pub ch: char,
    pub del: bool,
}

#[derive(Clone, Debug)]
pub struct LwwReg {
    pub value: Option<String>,
    pub ts: i128,
    pub cid: String,
}

impl Default for LwwReg {
    fn default() -> Self {
        Self {
            value: Some(String::new()),
            ts: -1,
            cid: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LivePage {
    pub title: String,
    pub text: String,
    pub blob: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PageBody {
    Tombstone,
    Ops(Vec<PageOp>),
    Compact(CompactBody),
}

#[derive(Clone, Debug)]
pub struct CompactBody {
    pub compact: u64,
    pub frontier: Vec<String>,
    /// `I(C)` CID set. Absent on pre-21.13 records: derive from `prev`-closure.
    pub included: Option<Vec<String>>,
    pub snapshot: Snapshot,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub atoms: Vec<SnapshotAtom>,
    pub title: LwwSnap,
    pub blob: Option<LwwSnap>,
}

#[derive(Clone, Debug)]
pub struct SnapshotAtom {
    pub id: AtomId,
    pub parent: Option<AtomId>,
    pub ch: String,
    pub del: bool,
}

#[derive(Clone, Debug)]
pub struct LwwSnap {
    pub value: Option<String>,
    pub ts: u64,
    pub cid: String,
}

#[derive(Clone, Debug)]
pub enum PageOp {
    Ins { after: Option<AtomId>, run: String },
    Del { ids: Vec<AtomId> },
    Lww { path: String, value: Option<String> },
}

#[derive(Clone, Debug)]
pub struct PageRecord {
    pub cid: String,
    pub ts: u64,
    pub prev: Vec<String>,
    pub vis: String,
    pub circle: Option<String>,
    pub epoch: Option<u64>,
    pub body: PageBody,
}

impl PageRecord {
    pub fn is_tombstone(&self) -> bool {
        matches!(self.body, PageBody::Tombstone)
    }

    pub fn is_compact(&self) -> bool {
        matches!(self.body, PageBody::Compact(_))
    }
}

/// Parse a page record from vector JSON (`unsigned` + `cid`).
pub fn record_from_vector(v: &serde_json::Value) -> Result<PageRecord> {
    let cid = v
        .get("cid")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::page("missing cid"))?
        .to_string();
    let unsigned = v.get("unsigned").unwrap_or(v);
    record_from_unsigned(&cid, unsigned)
}

pub fn record_from_unsigned(cid: &str, unsigned: &serde_json::Value) -> Result<PageRecord> {
    let ts = unsigned
        .get("ts")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| Error::page("missing ts"))?;
    let prev = unsigned
        .get("prev")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let vis = unsigned
        .get("vis")
        .and_then(|x| x.as_str())
        .unwrap_or("public")
        .to_string();
    let circle = unsigned
        .get("circle")
        .and_then(|x| x.as_str())
        .map(str::to_string);
    let epoch = unsigned.get("epoch").and_then(|x| x.as_u64());
    let body_v = unsigned.get("body");
    let body = match body_v {
        None | Some(serde_json::Value::Null) => PageBody::Tombstone,
        Some(b) if b.get("compact").and_then(|x| x.as_u64()) == Some(1) => {
            PageBody::Compact(parse_compact(b)?)
        }
        Some(b) => PageBody::Ops(parse_ops(b)?),
    };
    Ok(PageRecord {
        cid: cid.to_string(),
        ts,
        prev,
        vis,
        circle,
        epoch,
        body,
    })
}

fn parse_atom_id(v: &serde_json::Value, this_cid: Option<&str>) -> Result<AtomId> {
    let i = v
        .get("i")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| Error::page("atom-id missing i"))?;
    let op = match v.get("op").and_then(|x| x.as_str()) {
        Some(s) => s.to_string(),
        None => this_cid.unwrap_or("").to_string(),
    };
    Ok(AtomId { op, i })
}

fn parse_ops(body: &serde_json::Value) -> Result<Vec<PageOp>> {
    let ops = body
        .get("ops")
        .and_then(|x| x.as_array())
        .ok_or_else(|| Error::page("ops-body missing ops"))?;
    let mut out = Vec::new();
    for op in ops {
        let kind = op.get("op").and_then(|x| x.as_str()).unwrap_or("");
        match kind {
            "ins" => {
                let run = op
                    .get("run")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| Error::page("ins missing run"))?
                    .to_string();
                if run.is_empty() {
                    return Err(Error::page("ins run MUST contain a Unicode scalar"));
                }
                let after = match op.get("after") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(a) => Some(parse_atom_id(a, None)?),
                };
                out.push(PageOp::Ins { after, run });
            }
            "del" => {
                let ids = op
                    .get("ids")
                    .and_then(|x| x.as_array())
                    .ok_or_else(|| Error::page("del missing ids"))?;
                let mut parsed = Vec::new();
                for id in ids {
                    parsed.push(parse_atom_id(id, None)?);
                }
                out.push(PageOp::Del { ids: parsed });
            }
            "lww" => {
                let path = op
                    .get("path")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| Error::page("lww missing path"))?
                    .to_string();
                let value = match op.get("value") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(_) => return Err(Error::page("lww value must be text or null")),
                };
                out.push(PageOp::Lww { path, value });
            }
            _ => {
                // MUST ignore unknown ops on incremental bodies.
            }
        }
    }
    Ok(out)
}

fn parse_compact(body: &serde_json::Value) -> Result<CompactBody> {
    let compact = body.get("compact").and_then(|x| x.as_u64()).unwrap_or(0);
    let frontier = body
        .get("frontier")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let snap = body
        .get("snapshot")
        .ok_or_else(|| Error::page("compact missing snapshot"))?;
    let included = body.get("included").and_then(|x| x.as_array()).map(|a| {
        a.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    });
    Ok(CompactBody {
        compact,
        frontier,
        included,
        snapshot: parse_snapshot(snap)?,
    })
}

fn parse_snapshot(s: &serde_json::Value) -> Result<Snapshot> {
    let atoms_v = s
        .get("atoms")
        .and_then(|x| x.as_array())
        .ok_or_else(|| Error::page("snapshot missing atoms"))?;
    let mut atoms = Vec::new();
    for a in atoms_v {
        let id = parse_atom_id(
            a.get("id").ok_or_else(|| Error::page("atom missing id"))?,
            None,
        )?;
        let parent = match a.get("parent") {
            None | Some(serde_json::Value::Null) => None,
            Some(p) => Some(parse_atom_id(p, None)?),
        };
        let ch = a
            .get("ch")
            .and_then(|x| x.as_str())
            .ok_or_else(|| Error::page("atom missing ch"))?
            .to_string();
        let del = a.get("del").and_then(|x| x.as_bool()).unwrap_or(false);
        atoms.push(SnapshotAtom {
            id,
            parent,
            ch,
            del,
        });
    }
    let title = parse_lww_snap(
        s.get("title")
            .ok_or_else(|| Error::page("snapshot title"))?,
    )?;
    let blob = match s.get("blob") {
        None | Some(serde_json::Value::Null) => None,
        Some(b) => Some(parse_lww_snap(b)?),
    };
    Ok(Snapshot { atoms, title, blob })
}

fn parse_lww_snap(v: &serde_json::Value) -> Result<LwwSnap> {
    let ts = v.get("ts").and_then(|x| x.as_u64()).unwrap_or(0);
    let cid = v
        .get("cid")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let value = match v.get("value") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(Error::page("lww snap value")),
    };
    Ok(LwwSnap { value, ts, cid })
}

fn included_cids(c: &CompactBody, records: &[PageRecord], compact_cid: &str) -> Vec<String> {
    if let Some(inc) = &c.included {
        return inc.clone();
    }
    let mut v: Vec<String> = prev_closure(records, compact_cid).into_iter().collect();
    v.sort();
    v
}

fn verify_compacts(records: &[PageRecord]) -> Result<()> {
    for r in records {
        let PageBody::Compact(c) = &r.body else {
            continue;
        };
        if c.compact != 1 {
            continue;
        }
        if let Some(inc) = &c.included {
            let mut sorted = inc.clone();
            sorted.sort();
            if sorted != *inc {
                return Err(Error::page("compact included MUST be sorted"));
            }
        }
        let ic_cids = included_cids(c, records, &r.cid);
        let mut ic = Vec::with_capacity(ic_cids.len());
        for cid in &ic_cids {
            match records.iter().find(|x| x.cid == *cid) {
                Some(rec) => ic.push(rec.clone()),
                None => {
                    return Err(Error::page(
                        "compact I(C) CID missing or unverifiable as a folded page op",
                    ));
                }
            }
        }
        let replayed = apply_unchecked(&ic)?;
        let claimed = apply_unchecked(std::slice::from_ref(r))?;
        if replayed.text != claimed.text
            || replayed.title != claimed.title
            || replayed.blob != claimed.blob
        {
            return Err(Error::page("lying compact: snapshot != Apply(I(C))"));
        }
    }
    Ok(())
}

fn causal_closure(
    records: &[PageRecord],
    starts: impl IntoIterator<Item = String>,
) -> HashSet<String> {
    let by_cid: HashMap<&str, &PageRecord> = records.iter().map(|r| (r.cid.as_str(), r)).collect();
    let mut seen = HashSet::new();
    let mut pending: Vec<String> = starts.into_iter().collect();
    while let Some(cid) = pending.pop() {
        if seen.insert(cid.clone()) {
            if let Some(record) = by_cid.get(cid.as_str()) {
                pending.extend(record.prev.iter().cloned());
            }
        }
    }
    seen
}

fn prev_closure(records: &[PageRecord], start: &str) -> HashSet<String> {
    let mut seen = causal_closure(records, [start.to_string()]);
    seen.remove(start);
    seen
}

fn consider_lww(reg: &mut LwwReg, ts: i128, cid: &str, value: Option<String>) {
    if ts > reg.ts || (ts == reg.ts && cid >= reg.cid.as_str()) {
        *reg = LwwReg {
            value,
            ts,
            cid: cid.to_string(),
        };
    }
}

fn cmp_atom_desc(a: &AtomId, b: &AtomId) -> std::cmp::Ordering {
    b.op.cmp(&a.op).then_with(|| b.i.cmp(&a.i))
}

/// Apply set `S`. Order of `records` must not affect the live value.
pub fn apply(records: &[PageRecord]) -> Result<LivePage> {
    verify_compacts(records)?;
    apply_unchecked(records)
}

fn apply_unchecked(records: &[PageRecord]) -> Result<LivePage> {
    let superseded = causal_closure(records, records.iter().flat_map(|r| r.prev.iter().cloned()));
    let heads: Vec<&PageRecord> = records
        .iter()
        .filter(|r| !superseded.contains(&r.cid))
        .collect();
    if !heads.is_empty() && heads.iter().all(|h| h.is_tombstone()) {
        return Ok(LivePage {
            title: String::new(),
            text: String::new(),
            blob: None,
        });
    }

    let non_tomb: Vec<&PageRecord> = records.iter().filter(|r| !r.is_tombstone()).collect();

    // Invalid compact: compact != 1, empty/unsorted frontier, etc.
    let mut invalid_compact_head = false;
    for r in &non_tomb {
        if let PageBody::Compact(c) = &r.body {
            let invalid = c.compact != 1
                || c.frontier.is_empty()
                || {
                    let mut s = c.frontier.clone();
                    s.sort();
                    s != c.frontier
                }
                || {
                    let mut prev_sorted = r.prev.clone();
                    let mut fr = c.frontier.clone();
                    prev_sorted.sort();
                    fr.sort();
                    prev_sorted != fr
                };
            if invalid && heads.iter().any(|h| h.cid == r.cid) {
                invalid_compact_head = true;
            }
        }
    }
    if invalid_compact_head {
        return Err(Error::page(
            "invalid compact is a causal head; refuse to materialize",
        ));
    }

    let mut covered = HashSet::new();
    for r in &non_tomb {
        if let PageBody::Compact(c) = &r.body {
            if c.compact != 1 {
                continue;
            }
            for cid in included_cids(c, records, &r.cid) {
                covered.insert(cid);
            }
        }
    }

    let mut atoms: HashMap<String, Atom> = HashMap::new();
    let mut title = LwwReg {
        value: Some(String::new()),
        ts: -1,
        cid: String::new(),
    };
    let mut blob = LwwReg {
        value: None,
        ts: -1,
        cid: String::new(),
    };

    for r in &non_tomb {
        let PageBody::Compact(c) = &r.body else {
            continue;
        };
        if c.compact != 1 {
            continue;
        }
        for sa in &c.snapshot.atoms {
            let chars: Vec<char> = sa.ch.chars().collect();
            if chars.len() != 1 {
                return Err(Error::page(
                    "snapshot ch MUST be exactly one Unicode scalar",
                ));
            }
            let key = sa.id.key();
            if let Some(existing) = atoms.get_mut(&key) {
                if existing.parent != sa.parent || existing.ch != chars[0] {
                    return Err(Error::page("compact snapshot atom parent/scalar mismatch"));
                }
                existing.del |= sa.del;
            } else {
                atoms.insert(
                    key,
                    Atom {
                        id: sa.id.clone(),
                        parent: sa.parent.clone(),
                        ch: chars[0],
                        del: sa.del,
                    },
                );
            }
        }
        consider_lww(
            &mut title,
            i128::from(c.snapshot.title.ts),
            &c.snapshot.title.cid,
            c.snapshot.title.value.clone(),
        );
        if let Some(b) = &c.snapshot.blob {
            consider_lww(&mut blob, i128::from(b.ts), &b.cid, b.value.clone());
        }
    }

    let incremental: Vec<&PageRecord> = non_tomb
        .iter()
        .copied()
        .filter(|r| matches!(r.body, PageBody::Ops(_)) && !covered.contains(&r.cid))
        .collect();

    for rec in &incremental {
        let PageBody::Ops(ops) = &rec.body else {
            continue;
        };
        let mut i = 0u64;
        for op in ops {
            let PageOp::Ins { after, run } = op else {
                continue;
            };
            let mut parent = after.clone();
            // Invalid after (missing atom, except earlier index in this record) → ignore run.
            if let Some(a) = parent.as_mut() {
                if a.op.is_empty() {
                    a.op = rec.cid.clone();
                }
            }
            let after_ok = match &parent {
                None => true,
                Some(a) if a.op == rec.cid && a.i < i => true,
                Some(a) => atoms.contains_key(&a.key()),
            };
            if !after_ok {
                continue;
            }
            for ch in run.chars() {
                let id = AtomId {
                    op: rec.cid.clone(),
                    i,
                };
                let key = id.key();
                atoms.entry(key).or_insert_with(|| Atom {
                    id: id.clone(),
                    parent: parent.clone(),
                    ch,
                    del: false,
                });
                parent = Some(id);
                i += 1;
            }
        }
    }

    for rec in &incremental {
        let PageBody::Ops(ops) = &rec.body else {
            continue;
        };
        let mut last_lww: HashMap<String, Option<String>> = HashMap::new();
        for op in ops {
            match op {
                PageOp::Del { ids } => {
                    for id in ids {
                        let resolved = if id.op.is_empty() {
                            AtomId {
                                op: rec.cid.clone(),
                                i: id.i,
                            }
                        } else {
                            id.clone()
                        };
                        if let Some(a) = atoms.get_mut(&resolved.key()) {
                            a.del = true;
                        }
                    }
                }
                PageOp::Lww { path, value } => {
                    last_lww.insert(path.clone(), value.clone());
                }
                PageOp::Ins { .. } => {}
            }
        }
        for (path, value) in last_lww {
            match path.as_str() {
                "title" => consider_lww(&mut title, i128::from(rec.ts), &rec.cid, value),
                "blob" => consider_lww(&mut blob, i128::from(rec.ts), &rec.cid, value),
                _ => {}
            }
        }
    }

    if atoms.len() > PAGE_RGA_MAX_ATOMS {
        return Err(Error::page("PAGE_RGA_MAX_ATOMS exceeded"));
    }

    let mut children: HashMap<String, Vec<AtomId>> = HashMap::new();
    for a in atoms.values() {
        let pk = match &a.parent {
            None => AtomId {
                op: ROOT_OP.into(),
                i: 0,
            }
            .key(),
            Some(p) => p.key(),
        };
        children.entry(pk).or_default().push(a.id.clone());
    }
    for arr in children.values_mut() {
        arr.sort_by(cmp_atom_desc);
    }

    let mut text = String::new();
    let root_key = AtomId {
        op: ROOT_OP.into(),
        i: 0,
    }
    .key();
    let mut pending: Vec<AtomId> = children
        .get(&root_key)
        .into_iter()
        .flatten()
        .rev()
        .cloned()
        .collect();
    let mut visited = HashSet::new();
    while let Some(id) = pending.pop() {
        let key = id.key();
        if !visited.insert(key.clone()) {
            continue;
        }
        if let Some(atom) = atoms.get(&key) {
            if !atom.del {
                text.push(atom.ch);
            }
            if let Some(kids) = children.get(&key) {
                pending.extend(kids.iter().rev().cloned());
            }
        }
    }

    if text.chars().count() > PAGE_TEXT_MAX_SCALARS {
        return Err(Error::page("PAGE_TEXT_MAX_SCALARS exceeded"));
    }
    let title_s = title.value.clone().unwrap_or_default();
    if title_s.chars().count() > PAGE_TITLE_MAX_SCALARS {
        return Err(Error::page("title exceeds 512 scalars"));
    }

    Ok(LivePage {
        title: title_s,
        text,
        blob: blob.value,
    })
}

/// Build a compact snapshot from apply(`I(C)`).
pub fn snapshot_of(records: &[PageRecord]) -> Result<Snapshot> {
    let live = apply(records)?;
    // Re-run a private apply that keeps atoms — duplicate the apply internals via
    // a second pass: reconstruct from live is insufficient (need atom table).
    // Call apply_full.
    let state = apply_full(records)?;
    let mut atoms: Vec<SnapshotAtom> = state
        .atoms
        .values()
        .map(|a| SnapshotAtom {
            id: a.id.clone(),
            parent: a.parent.clone(),
            ch: a.ch.to_string(),
            del: a.del,
        })
        .collect();
    atoms.sort_by(|a, b| a.id.op.cmp(&b.id.op).then_with(|| a.id.i.cmp(&b.id.i)));
    let title = LwwSnap {
        value: Some(live.title),
        ts: state.title.ts.max(0) as u64,
        cid: state.title.cid,
    };
    let blob = if state.blob.ts < 0 {
        None
    } else {
        Some(LwwSnap {
            value: live.blob,
            ts: state.blob.ts as u64,
            cid: state.blob.cid,
        })
    };
    Ok(Snapshot { atoms, title, blob })
}

struct FullState {
    atoms: HashMap<String, Atom>,
    title: LwwReg,
    blob: LwwReg,
}

fn apply_full(records: &[PageRecord]) -> Result<FullState> {
    // Same as apply but returns internals. Keep in sync with apply() — for snapshot
    // generation we re-apply without materializing text bounds as errors after.
    let live_check = apply(records)?;
    let _ = live_check;
    // Walk using apply's covered/union logic by calling apply and also storing
    // via a duplicated inner. Simpler: serialize through apply then... we need atoms.
    // Duplicate the core loop:
    let mut covered = HashSet::new();
    for r in records {
        if let PageBody::Compact(c) = &r.body {
            for cid in included_cids(c, records, &r.cid) {
                covered.insert(cid);
            }
        }
    }
    let mut atoms: HashMap<String, Atom> = HashMap::new();
    let mut title = LwwReg::default();
    let mut blob = LwwReg {
        value: None,
        ts: -1,
        cid: String::new(),
    };
    for r in records {
        if let PageBody::Compact(c) = &r.body {
            if c.compact != 1 {
                continue;
            }
            for sa in &c.snapshot.atoms {
                let ch = sa.ch.chars().next().unwrap_or('\u{fffd}');
                atoms.insert(
                    sa.id.key(),
                    Atom {
                        id: sa.id.clone(),
                        parent: sa.parent.clone(),
                        ch,
                        del: sa.del,
                    },
                );
            }
            consider_lww(
                &mut title,
                i128::from(c.snapshot.title.ts),
                &c.snapshot.title.cid,
                c.snapshot.title.value.clone(),
            );
            if let Some(b) = &c.snapshot.blob {
                consider_lww(&mut blob, i128::from(b.ts), &b.cid, b.value.clone());
            }
        }
    }
    let incremental: Vec<&PageRecord> = records
        .iter()
        .filter(|r| matches!(r.body, PageBody::Ops(_)) && !covered.contains(&r.cid))
        .collect();
    for rec in &incremental {
        let PageBody::Ops(ops) = &rec.body else {
            continue;
        };
        let mut i = 0u64;
        for op in ops {
            if let PageOp::Ins { after, run } = op {
                let mut parent = after.clone();
                for ch in run.chars() {
                    let id = AtomId {
                        op: rec.cid.clone(),
                        i,
                    };
                    atoms.entry(id.key()).or_insert(Atom {
                        id: id.clone(),
                        parent: parent.clone(),
                        ch,
                        del: false,
                    });
                    parent = Some(id);
                    i += 1;
                }
            }
        }
    }
    for rec in &incremental {
        let PageBody::Ops(ops) = &rec.body else {
            continue;
        };
        let mut last_lww: HashMap<String, Option<String>> = HashMap::new();
        for op in ops {
            match op {
                PageOp::Del { ids } => {
                    for id in ids {
                        if let Some(a) = atoms.get_mut(&id.key()) {
                            a.del = true;
                        }
                    }
                }
                PageOp::Lww { path, value } => {
                    last_lww.insert(path.clone(), value.clone());
                }
                PageOp::Ins { .. } => {}
            }
        }
        for (path, value) in last_lww {
            if path == "title" {
                consider_lww(&mut title, i128::from(rec.ts), &rec.cid, value);
            } else if path == "blob" {
                consider_lww(&mut blob, i128::from(rec.ts), &rec.cid, value);
            }
        }
    }
    Ok(FullState { atoms, title, blob })
}

/// Sorted frontier of causal heads (CID text).
pub fn frontier_of(records: &[PageRecord]) -> Vec<String> {
    let superseded = causal_closure(records, records.iter().flat_map(|r| r.prev.iter().cloned()));
    let mut f: Vec<String> = records
        .iter()
        .filter(|r| !superseded.contains(&r.cid) && !r.is_tombstone())
        .map(|r| r.cid.clone())
        .collect();
    f.sort();
    f
}

pub fn cid_text_ord(a: &Cid, b: &Cid) -> std::cmp::Ordering {
    a.to_string_multibase().cmp(&b.to_string_multibase())
}

/// Knot admission for a compact write (spec 06). Not CRDT validity.
pub fn compact_admission(
    included_new_ops: usize,
    interval_from_prev_compact_ms: Option<u64>,
) -> Result<()> {
    use crate::constants::{PAGE_COMPACT_MIN_INTERVAL_MS, PAGE_COMPACT_MIN_OPS};
    if included_new_ops < PAGE_COMPACT_MIN_OPS {
        return Err(Error::Http {
            status: 429,
            error: "RateLimited".into(),
        });
    }
    if let Some(dt) = interval_from_prev_compact_ms {
        if dt < PAGE_COMPACT_MIN_INTERVAL_MS {
            return Err(Error::Http {
                status: 429,
                error: "RateLimited".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_page() {
        let live = apply(&[]).unwrap();
        assert_eq!(live.text, "");
    }

    #[test]
    fn tombstoned_atoms_remain_in_rga() {
        let r0 = PageRecord {
            cid: "bafyr0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ts: 1,
            prev: vec![],
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Ops(vec![PageOp::Ins {
                after: None,
                run: "ab".into(),
            }]),
        };
        let r1 = PageRecord {
            cid: "bafyr1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ts: 2,
            prev: vec![r0.cid.clone()],
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Ops(vec![PageOp::Del {
                ids: vec![
                    AtomId {
                        op: r0.cid.clone(),
                        i: 0,
                    },
                    AtomId {
                        op: r0.cid.clone(),
                        i: 1,
                    },
                ],
            }]),
        };
        let live = apply(&[r0.clone(), r1.clone()]).unwrap();
        assert_eq!(live.text, "");
        let snap = snapshot_of(&[r0, r1]).unwrap();
        assert_eq!(snap.atoms.len(), 2);
        assert!(snap.atoms.iter().all(|a| a.del));
    }

    #[test]
    fn compact_admission_min_ops_and_interval() {
        use crate::constants::{PAGE_COMPACT_MIN_INTERVAL_MS, PAGE_COMPACT_MIN_OPS};
        assert!(compact_admission(PAGE_COMPACT_MIN_OPS - 1, None).is_err());
        assert!(compact_admission(PAGE_COMPACT_MIN_OPS, None).is_ok());
        assert!(
            compact_admission(PAGE_COMPACT_MIN_OPS, Some(PAGE_COMPACT_MIN_INTERVAL_MS - 1))
                .is_err()
        );
        assert!(
            compact_admission(PAGE_COMPACT_MIN_OPS, Some(PAGE_COMPACT_MIN_INTERVAL_MS)).is_ok()
        );
    }

    fn rec_ops(cid: &str, ts: u64, prev: &[&str], run: &str) -> PageRecord {
        PageRecord {
            cid: cid.into(),
            ts,
            prev: prev.iter().map(|s| (*s).to_string()).collect(),
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Ops(vec![PageOp::Ins {
                after: None,
                run: run.into(),
            }]),
        }
    }

    fn rec_compact(cid: &str, ts: u64, folded: &[PageRecord], snapshot: Snapshot) -> PageRecord {
        let mut included: Vec<String> = folded.iter().map(|r| r.cid.clone()).collect();
        included.sort();
        let frontier = frontier_of(folded);
        PageRecord {
            cid: cid.into(),
            ts,
            prev: frontier.clone(),
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Compact(CompactBody {
                compact: 1,
                frontier,
                included: Some(included),
                snapshot,
            }),
        }
    }

    #[test]
    fn compact_missing_included_block_is_rejected() {
        let op = rec_ops("bafya", 1, &[], "H");
        let snap = snapshot_of(&[op]).unwrap();
        let compact = rec_compact("bafyk", 10, &[], snap);
        let PageBody::Compact(c) = &compact.body else {
            panic!("compact");
        };
        assert_eq!(c.included.as_ref().map(Vec::len), Some(0));
        let dangling = PageRecord {
            cid: "bafyk".into(),
            ts: 10,
            prev: vec!["bafya".into()],
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Compact(CompactBody {
                compact: 1,
                frontier: vec!["bafya".into()],
                included: Some(vec!["bafya".into()]),
                snapshot: snapshot_of(&[rec_ops("bafya", 1, &[], "H")]).unwrap(),
            }),
        };
        assert!(apply(&[dangling]).is_err());
    }

    #[test]
    fn compact_with_fetchable_included_is_accepted() {
        let op = rec_ops("bafya", 1, &[], "H");
        let snap = snapshot_of(std::slice::from_ref(&op)).unwrap();
        let compact = rec_compact("bafyk", 10, std::slice::from_ref(&op), snap);
        let concurrent = PageRecord {
            cid: "bafyl".into(),
            ts: 11,
            prev: vec!["bafya".into()],
            vis: "public".into(),
            circle: None,
            epoch: None,
            body: PageBody::Ops(vec![PageOp::Ins {
                after: Some(AtomId {
                    op: "bafya".into(),
                    i: 0,
                }),
                run: "i".into(),
            }]),
        };
        let live = apply(&[op, compact, concurrent]).unwrap();
        assert_eq!(live.text, "Hi");
    }

    #[test]
    fn lying_compact_snapshot_is_rejected() {
        let op = rec_ops("bafya", 1, &[], "H");
        let mut snap = snapshot_of(std::slice::from_ref(&op)).unwrap();
        snap.atoms[0].ch = "X".into();
        let compact = rec_compact("bafyk", 10, std::slice::from_ref(&op), snap);
        assert!(apply(&[op, compact]).is_err());
    }
}
