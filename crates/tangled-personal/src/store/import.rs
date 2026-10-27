//! Owner CAR admission is determined by the signed commit/MST closure.
use super::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use tangled::dagcbor::{self, Value as Cbor};
use tangled::http::GetHeadResponse;
use tangled::knot::StoredRecord;
use tangled::mst::Mst;

const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;
const MAX_CLOSURE_WORK: usize = 1_000_000;

impl Store {
    pub fn import(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err("Repository CAR exceeds 64 MiB".into());
        }
        let car = tangled::car::decode(bytes).map_err(err)?;
        if car.roots.len() != 1 {
            return Err("Backup must have exactly one signed repository root".into());
        }
        let root = car.roots[0].to_string();
        if car.get(&car.roots[0]).is_none() {
            return Err("Missing root commit".into());
        }
        let owner = self.did();
        let pk = tangled::identity::Did::parse(&owner).map_err(err)?.pubkey;
        let mut blocks = self.engine.blocks.clone();
        for (cid, bytes) in car.blocks {
            if self
                .engine
                .private_archive_erasure_cids
                .contains(&cid.to_string())
            {
                return Err("Repository contains erased private archive blocks".into());
            }
            blocks.insert(cid.to_string(), bytes);
        }
        let mut commits = Vec::new();
        let mut seen = HashSet::new();
        let mut next = Some(root.clone());
        let mut work = 0;
        while let Some(cid) = next.take() {
            budget(&mut work)?;
            if !seen.insert(cid.clone()) {
                return Err("Cyclic commit history".into());
            }
            let info = tangled::record::parse_commit(block(&blocks, &cid)?).map_err(err)?;
            if info.did != owner {
                return Err("Backup belongs to a different identity".into());
            }
            tangled::record::verify_commit(&pk, &info.unsigned, &info.sig).map_err(err)?;
            next = info.prev.clone();
            commits.push(info);
        }
        if let Some(head) = self.engine.heads.get(&owner) {
            if !seen.contains(&head.commit) {
                return Err("Backup would roll back or fork the current repository".into());
            }
        }
        let mut admitted = BTreeMap::new();
        let mut current = BTreeMap::new();
        // Validate historical trees too: their record blocks remain signed evidence.
        // No engine mutation or equal-root acceptance occurs until all are complete.
        for (index, commit) in commits.iter().enumerate() {
            let mappings = tree(&blocks, &commit.data, &mut work)?;
            for (key, cid) in &mappings {
                if !admitted.contains_key(cid) {
                    let record = verified_record(&blocks, cid, &owner, key)?;
                    admitted.insert(cid.clone(), record);
                } else {
                    let record = &admitted[cid];
                    if key != &tangled::sync::mst_key(&record.collection, &record.rkey) {
                        return Err("Signed MST record key does not match envelope".into());
                    }
                }
            }
            if index == 0 {
                current = mappings;
            }
        }
        let info = commits.first().ok_or("Missing signed root")?;
        let mut candidate = self.engine.clone();
        candidate.data_dir = None;
        candidate.blocks = blocks;
        candidate.stored.retain(|record| record.did != owner);
        candidate.tips.retain(|(did, _, _), _| did != &owner);
        candidate.records.retain(|(did, _, _), _| did != &owner);
        let mut mst = Mst::new();
        for (key, cid) in current {
            let record = &admitted[&cid];
            mst.insert(key.as_bytes(), Cid::parse(&cid).map_err(err)?)
                .map_err(err)?;
            candidate.tips.insert(
                (
                    owner.clone(),
                    record.collection.clone(),
                    record.rkey.clone(),
                ),
                cid,
            );
        }
        for record in admitted.into_values() {
            if record.collection == "sys.revocation" {
                if let Some(cid) = record.record["body"]["cid"].as_str() {
                    candidate.revoked.insert(cid.into(), ());
                }
            }
            candidate.stored.push(record);
        }
        candidate.msts.insert(owner.clone(), mst);
        candidate.heads.insert(
            owner.clone(),
            GetHeadResponse {
                did: owner.clone(),
                commit: root,
                rev: info.rev.clone(),
                knot: self
                    .engine
                    .heads
                    .get(&owner)
                    .map(|head| head.knot.clone())
                    .unwrap_or_else(|| format!("inproc://{owner}")),
                data: info.data.clone(),
            },
        );
        for (cid, bytes) in &candidate.blocks {
            if Cid::parse(cid).map_err(err)?.is_raw() {
                candidate
                    .blobs
                    .insert((owner.clone(), cid.clone()), bytes.clone());
            }
        }
        self.publish_candidate(candidate)
    }
}

fn budget(work: &mut usize) -> Result<()> {
    *work += 1;
    if *work > MAX_CLOSURE_WORK {
        return Err("Repository closure exceeds traversal limit".into());
    }
    Ok(())
}

fn block<'a>(blocks: &'a HashMap<String, Vec<u8>>, cid: &str) -> Result<&'a [u8]> {
    let parsed = Cid::parse(cid).map_err(err)?;
    let bytes = blocks
        .get(cid)
        .ok_or_else(|| format!("Incomplete repository closure: missing {cid}"))?;
    if !parsed.is_dag_cbor() || Cid::dag_cbor(bytes) != parsed {
        return Err("Repository closure block CID mismatch".into());
    }
    Ok(bytes)
}

fn link(value: Option<&Cbor>) -> Result<Option<String>> {
    match value {
        Some(Cbor::Null) => Ok(None),
        Some(Cbor::Text(cid)) => {
            Cid::parse(cid).map_err(err)?;
            Ok(Some(cid.clone()))
        }
        Some(Cbor::Cid(cid)) => Ok(Some(cid.to_string())),
        _ => Err("Invalid signed MST link".into()),
    }
}

// Each frame has exclusive key bounds. Iteration avoids recursion on hostile trees.
fn tree(
    blocks: &HashMap<String, Vec<u8>>,
    root: &str,
    work: &mut usize,
) -> Result<BTreeMap<String, String>> {
    let mut stack = vec![(root.to_string(), None::<Vec<u8>>, None::<Vec<u8>>)];
    let mut visited = HashSet::new();
    let mut mappings = BTreeMap::new();
    while let Some((cid, lower, upper)) = stack.pop() {
        budget(work)?;
        if !visited.insert(cid.clone()) {
            return Err("Repeated or cyclic signed MST node".into());
        }
        let node = dagcbor::decode(block(blocks, &cid)?).map_err(err)?;
        let Some(Cbor::Array(entries)) = node.get("e") else {
            return Err("Signed MST node is missing entries".into());
        };
        let mut previous = Vec::new();
        let mut decoded = Vec::new();
        for entry in entries {
            budget(work)?;
            let Some(Cbor::Unsigned(prefix)) = entry.get("p") else {
                return Err("Invalid signed MST prefix".into());
            };
            let prefix = usize::try_from(*prefix).map_err(err)?;
            let Some(Cbor::Bytes(suffix)) = entry.get("k") else {
                return Err("Invalid signed MST key".into());
            };
            let mut key = previous
                .get(..prefix)
                .ok_or("Invalid signed MST prefix length")?
                .to_vec();
            key.extend_from_slice(suffix);
            if key.len() > 513
                || key.contains(&0)
                || (!previous.is_empty() && key <= previous)
                || lower.as_ref().is_some_and(|bound| key <= *bound)
                || upper.as_ref().is_some_and(|bound| key >= *bound)
            {
                return Err("Invalid signed MST key ordering or bounds".into());
            }
            let value = link(entry.get("v"))?.ok_or("Missing signed MST record CID")?;
            let text_key = String::from_utf8(key.clone()).map_err(err)?;
            if mappings.insert(text_key, value).is_some() {
                return Err("Duplicate signed MST key".into());
            }
            decoded.push((key.clone(), link(entry.get("t"))?));
            previous = key;
        }
        if let Some(left) = link(node.get("l"))? {
            stack.push((
                left,
                lower,
                decoded
                    .first()
                    .map(|(key, _)| key.clone())
                    .or_else(|| upper.clone()),
            ));
        }
        for index in 0..decoded.len() {
            if let Some(right) = &decoded[index].1 {
                stack.push((
                    right.clone(),
                    Some(decoded[index].0.clone()),
                    decoded
                        .get(index + 1)
                        .map(|(key, _)| key.clone())
                        .or_else(|| upper.clone()),
                ));
            }
        }
    }
    Ok(mappings)
}

fn verified_record(
    blocks: &HashMap<String, Vec<u8>>,
    cid: &str,
    owner: &str,
    key: &str,
) -> Result<StoredRecord> {
    let cbor = dagcbor::decode(block(blocks, cid)?).map_err(err)?;
    let signature: [u8; 64] = match cbor.get("sig") {
        Some(Cbor::Bytes(bytes)) => bytes.as_slice().try_into().map_err(err)?,
        _ => return Err("Signed MST record has no signature bytes".into()),
    };
    // Preserve IPLD values in unknown extension data; only the envelope signature
    // uses the display convention expected by Store clients.
    let mut record = dagcbor::to_json(&cbor);
    let object = record
        .as_object_mut()
        .ok_or("Signed MST record must be an object")?;
    object.remove("sig");
    tangled::record::validate_envelope(&record).map_err(err)?;
    let pk = tangled::record::signer_pubkey(&record).map_err(err)?;
    tangled::record::verify_json_record(&pk, &record, &signature).map_err(err)?;
    if record["author"].as_str() != Some(owner) {
        return Err("Signed MST record author does not match owner".into());
    }
    let collection = record["collection"]
        .as_str()
        .ok_or("Missing record collection")?
        .to_string();
    let rkey = record["rkey"]
        .as_str()
        .ok_or("Missing record key")?
        .to_string();
    if tangled::sync::mst_key(&collection, &rkey) != key {
        return Err("Signed MST record key does not match envelope".into());
    }
    let vis = record["vis"]
        .as_str()
        .ok_or("Missing visibility")?
        .to_string();
    let circle = record["circle"].as_str().map(str::to_string);
    let deleted = record["body"].is_null();
    record["sig"] = json!(hex::encode(signature));
    Ok(StoredRecord {
        did: owner.into(),
        collection,
        rkey,
        vis,
        cid: cid.into(),
        record,
        circle,
        deleted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clone_empty(source: &Store) -> Store {
        let (ed, x) = source.identity.to_seeds();
        let identity = Identity::from_seeds(ed, x);
        let mut engine = KnotEngine::hosting(&identity.did_string());
        engine.register_identity(&identity);
        Store {
            engine,
            identity,
            path: None,
            _lock: None,
        }
    }

    #[test]
    fn unrepresentable_filesystem_key_collision_rejects_without_changing_live_data() {
        let mut source = Store::memory();
        source
            .put("pub.tangled.note", "a:b", json!({"text":"first"}), "cap")
            .unwrap();
        source
            .put("pub.tangled.note", "a_b", json!({"text":"second"}), "cap")
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (ed, x) = source.identity.to_seeds();
        std::fs::write(
            dir.path().join("identity.json"),
            serde_json::to_vec(&json!({"ed25519":hex::encode(ed),"x25519":hex::encode(x)}))
                .unwrap(),
        )
        .unwrap();
        let mut target = Store::open(dir.path()).unwrap();
        assert!(target.import(&source.export().unwrap()).is_err());
        assert!(target.list(None).unwrap().is_empty());
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert!(reopened.list(None).unwrap().is_empty());
    }

    #[test]
    fn authoritative_root_key_removal_survives_reopen() {
        let mut source = Store::memory();
        source
            .put(
                "pub.tangled.note",
                "one",
                json!({"text":"historical"}),
                "cap",
            )
            .unwrap();
        let old_bytes = source.export().unwrap();
        let mut car = tangled::car::decode(&old_bytes).unwrap();
        let empty = Mst::new();
        let data = empty.root_cid_string_cids().unwrap();
        car.blocks.extend(empty.block_pairs_string_cids().unwrap());
        let unsigned = tangled::record::unsigned_commit(
            &source.did(),
            &data,
            "0000000000200",
            Some(&car.roots[0]),
        );
        let commit =
            tangled::record::seal_commit(source.identity.signing_key(), &unsigned).unwrap();
        car.roots = vec![commit.cid.clone()];
        car.blocks.push((commit.cid, commit.signed_cbor));
        let dir = tempfile::tempdir().unwrap();
        let (ed, x) = source.identity.to_seeds();
        std::fs::write(
            dir.path().join("identity.json"),
            serde_json::to_vec(&json!({"ed25519":hex::encode(ed),"x25519":hex::encode(x)}))
                .unwrap(),
        )
        .unwrap();
        let mut target = Store::open(dir.path()).unwrap();
        target.import(&old_bytes).unwrap();
        target.import(&tangled::car::encode(&car).unwrap()).unwrap();
        assert!(target.list(None).unwrap().is_empty());
        drop(target);
        let mut reopened = Store::open(dir.path()).unwrap();
        assert!(reopened.list(None).unwrap().is_empty());
        assert_eq!(
            reopened.engine.stored.len(),
            1,
            "signed history is retained"
        );
    }

    #[test]
    fn multilevel_trees_and_unknown_ipld_extensions_survive_reordered_import() {
        let mut source = Store::memory();
        for index in 0..80 {
            source.put("org.example.extension", &format!("key-{index}"), json!({"title":format!("row {index}"),"extension":{"bytes":{"/":{"bytes":"AAEC"}},"nested":{"keep":true}}}), "cap").unwrap();
        }
        let mut car = tangled::car::decode(&source.export().unwrap()).unwrap();
        car.blocks.reverse();
        let mut target = clone_empty(&source);
        target.import(&tangled::car::encode(&car).unwrap()).unwrap();
        assert_eq!(target.list(None).unwrap(), source.list(None).unwrap());
        let old = target.engine.heads[&target.did()].clone();
        target
            .put(
                "org.example.extension",
                "after-import",
                json!({"text":"signed successor"}),
                "cap",
            )
            .unwrap();
        let new = &target.engine.heads[&target.did()];
        let commit = tangled::record::parse_commit(&target.engine.blocks[&new.commit]).unwrap();
        assert_eq!(commit.prev.as_deref(), Some(old.commit.as_str()));
        assert!(new.rev > old.rev);
        let mut roundtrip = clone_empty(&source);
        roundtrip.import(&target.export().unwrap()).unwrap();
        assert_eq!(roundtrip.list(None).unwrap().len(), 81);
    }

    #[test]
    fn same_root_does_not_bypass_missing_closure_validation() {
        let mut source = Store::memory();
        let record = source
            .put("pub.tangled.note", "one", json!({"text":"body"}), "cap")
            .unwrap();
        let bytes = source.export().unwrap();
        let mut car = tangled::car::decode(&bytes).unwrap();
        let mut target = clone_empty(&source);
        target.import(&bytes).unwrap();
        target.engine.blocks.remove(record["cid"].as_str().unwrap());
        car.blocks.retain(|(cid, _)| car.roots.contains(cid));
        assert!(target.import(&tangled::car::encode(&car).unwrap()).is_err());
        target.import(&bytes).unwrap();
        assert_eq!(
            target.get("pub.tangled.note", "one").unwrap()["cid"],
            record["cid"]
        );
    }

    #[test]
    fn same_root_import_repairs_missing_materialization_and_raw_blobs() {
        let mut source = Store::memory();
        let blob = source.put_blob(b"article text", "text/plain").unwrap();
        let current = source
            .put(
                "pub.tangled.note",
                "one",
                json!({"text":"latest","blob":blob}),
                "cap",
            )
            .unwrap();
        let bytes = source.export().unwrap();
        let mut target = clone_empty(&source);
        target.import(&bytes).unwrap();
        target.engine.stored.clear();
        target.engine.tips.clear();
        target.engine.blocks.remove(&blob);
        target.engine.blobs.clear();
        target.import(&bytes).unwrap();
        assert_eq!(
            target.get("pub.tangled.note", "one").unwrap()["cid"],
            current["cid"]
        );
        assert_eq!(target.get_blob(&blob).unwrap(), b"article text");
    }

    #[test]
    fn uncommitted_revocation_block_has_no_revocation_side_effect() {
        let mut source = Store::memory();
        source
            .put(
                "pub.tangled.note",
                "one",
                json!({"text":"committed"}),
                "cap",
            )
            .unwrap();
        let mut car = tangled::car::decode(&source.export().unwrap()).unwrap();
        let revoked_cid = Cid::raw(b"capability").to_string();
        source
            .put("sys.revocation", "rogue", json!({"cid":revoked_cid}), "cap")
            .unwrap();
        let rogue = source
            .engine
            .stored
            .iter()
            .find(|record| record.collection == "sys.revocation")
            .unwrap();
        car.blocks.push((
            Cid::parse(&rogue.cid).unwrap(),
            source.engine.blocks[&rogue.cid].clone(),
        ));
        let mut target = clone_empty(&source);
        target.import(&tangled::car::encode(&car).unwrap()).unwrap();
        assert!(!target.engine.revoked.contains_key(&revoked_cid));
        assert!(target.get("sys.revocation", "rogue").is_err());
        assert!(
            target.engine.blocks.contains_key(&rogue.cid),
            "opaque unknown blocks remain retainable"
        );
    }

    #[test]
    fn rejects_wrong_mst_key_and_invalid_record_signature() {
        let mut source = Store::memory();
        source
            .put("pub.tangled.note", "one", json!({"text":"body"}), "cap")
            .unwrap();
        let bytes = source.export().unwrap();
        let original = tangled::car::decode(&bytes).unwrap();
        let record = source.engine.stored[0].clone();
        for invalid_signature in [false, true] {
            let mut car = original.clone();
            let mut cid = Cid::parse(&record.cid).unwrap();
            if invalid_signature {
                let mut bad = dagcbor::decode(&source.engine.blocks[&record.cid]).unwrap();
                bad.insert("sig", Cbor::Bytes(vec![0; 64])).unwrap();
                let bad = dagcbor::encode(&bad).unwrap();
                cid = Cid::dag_cbor(&bad);
                car.blocks.push((cid.clone(), bad));
            }
            let key = if invalid_signature {
                "pub.tangled.note/one"
            } else {
                "pub.tangled.note/wrong"
            };
            let mut mst = Mst::new();
            mst.insert(key.as_bytes(), cid).unwrap();
            let data = mst.root_cid_string_cids().unwrap();
            car.blocks.extend(mst.block_pairs_string_cids().unwrap());
            let unsigned =
                tangled::record::unsigned_commit(&source.did(), &data, "0000000000100", None);
            let commit =
                tangled::record::seal_commit(source.identity.signing_key(), &unsigned).unwrap();
            car.roots = vec![commit.cid.clone()];
            car.blocks.push((commit.cid, commit.signed_cbor));
            let mut target = clone_empty(&source);
            assert!(target.import(&tangled::car::encode(&car).unwrap()).is_err());
            assert!(target.engine.heads.is_empty());
        }
    }
}
