/**
 * Generate Tangled v1 test vectors. Deterministic: seed = BLAKE3("tangled-vector-seed-v1").
 */
import { writeFileSync } from "node:fs";
import { ed25519 } from "@noble/curves/ed25519.js";
import { x25519 } from "@noble/curves/ed25519.js";
import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex, hexToBytes, utf8ToBytes } from "@noble/hashes/utils.js";
import { encode as cborEncode } from "cborg";
import bs58 from "bs58";

const seed = blake3(utf8ToBytes("tangled-vector-seed-v1"));

function keypair(label) {
  const sk = blake3(utf8ToBytes(label), { dkLen: 32 });
  const pk = ed25519.getPublicKey(sk);
  return { sk, pk, b58: bs58.encode(pk), did: `did:sol:${bs58.encode(pk)}` };
}

function dagCbor(obj) {
  return cborEncode(obj, { float64: false });
}

function cidV1DagCbor(bytes) {
  const digest = blake3(bytes);
  // CIDv1 = 0x01 | 0x71 | 0x1e | 0x20 | digest
  const raw = new Uint8Array(4 + 32);
  raw[0] = 0x01;
  raw[1] = 0x71;
  raw[2] = 0x1e;
  raw[3] = 0x20;
  raw.set(digest, 4);
  // multibase base32 lower, no padding, prefix b
  return "b" + base32(raw);
}

function cidV1Raw(bytes) {
  const digest = blake3(bytes);
  const raw = new Uint8Array(4 + 32);
  raw[0] = 0x01;
  raw[1] = 0x55;
  raw[2] = 0x1e;
  raw[3] = 0x20;
  raw.set(digest, 4);
  return "b" + base32(raw);
}

const ALPH = "abcdefghijklmnopqrstuvwxyz234567";
function base32(bytes) {
  let bits = 0;
  let value = 0;
  let out = "";
  for (const b of bytes) {
    value = (value << 8) | b;
    bits += 8;
    while (bits >= 5) {
      out += ALPH[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) out += ALPH[(value << (5 - bits)) & 31];
  return out;
}

function offchainMessage(digest32) {
  const header = new Uint8Array([
    0xff, ...utf8ToBytes("solana offchain"),
  ]);
  const body = utf8ToBytes(bytesToHex(digest32));
  const msg = new Uint8Array(16 + 1 + 1 + 2 + 64);
  msg.set(header, 0);
  msg[16] = 0x00;
  msg[17] = 0x00;
  msg[18] = 64 & 0xff;
  msg[19] = 64 >> 8;
  msg.set(body, 20);
  return msg;
}

function signUnsigned(sk, unsigned) {
  const bytes = dagCbor(unsigned);
  const digest = blake3(bytes);
  const message = offchainMessage(digest);
  const sig = ed25519.sign(message, sk);
  return { bytes, digest, message, sig, hexSig: bytesToHex(sig) };
}

const alice = keypair("alice");
const device = keypair("device");
const bob = keypair("bob");

const schemaUnsigned = {
  v: 1,
  type: new Uint8Array(0), // placeholder filled after — schemas self-describe; use empty type cid bytes as 36-byte dummy? Better: omit circularity by using a well-known type CID of zeros-length schema.
};
// Use a fixed schema CID: hash of name
const schemaBody = { v: 1, name: "pub.tangled.post", fields: { text: { type: "string" } } };
const schemaBytes = dagCbor(schemaBody);
const schemaCid = cidV1DagCbor(schemaBytes);

const unsignedRecord = {
  v: 1,
  type: schemaCid,
  collection: "pub.tangled.post",
  rkey: "3jzfcijpjiz00",
  author: alice.did,
  ts: 1700000000000,
  prev: [],
  vis: "public",
  body: { text: "hello tangled" },
};

const signed = signUnsigned(alice.sk, unsignedRecord);
const record = { ...unsignedRecord, sig: signed.sig };
const recordBytes = dagCbor(record);
const recordCid = cidV1DagCbor(recordBytes);

writeFileSync(
  new URL("./unsigned-record.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Unsigned DAG-CBOR map of a public post. type is displayed as CID string; on the wire it is tag(42) CID bytes in production. This JSON vector uses the CID string for readability; hex_cbor is the canonical hash input used here (string type field).",
      unsigned: unsignedRecord,
      hex_cbor: bytesToHex(signed.bytes),
      blake3: bytesToHex(signed.digest),
    },
    null,
    2,
  ) + "\n",
);

writeFileSync(
  new URL("./solana-signature.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Ed25519 over Solana off-chain envelope of lowercase hex(BLAKE3(unsigned DAG-CBOR)).",
      secret_key_hex: bytesToHex(alice.sk),
      public_key_hex: bytesToHex(alice.pk),
      public_key_base58: alice.b58,
      did: alice.did,
      unsigned_cbor_hex: bytesToHex(signed.bytes),
      digest_hex: bytesToHex(signed.digest),
      envelope_hex: bytesToHex(signed.message),
      signature_hex: signed.hexSig,
      verify: true,
      negative: {
        comment: "Flipping the last signature byte MUST fail verify",
        signature_hex: signed.hexSig.slice(0, -2) + (signed.hexSig.endsWith("00") ? "01" : "00"),
        verify: false,
      },
    },
    null,
    2,
  ) + "\n",
);

// Minimal MST: single leaf node
const mstNode = {
  l: null,
  e: [
    {
      p: 0,
      k: utf8ToBytes("pub.tangled.post/3jzfcijpjiz00"),
      v: recordCid,
      t: null,
    },
  ],
};
const mstBytes = dagCbor(mstNode);
const mstCid = cidV1DagCbor(mstBytes);

const unsignedCommit = {
  v: 1,
  did: alice.did,
  data: mstCid,
  rev: "3jzfcijpjiz00",
  prev: null,
};
const commitSigned = signUnsigned(alice.sk, unsignedCommit);
const commit = { ...unsignedCommit, sig: commitSigned.sig };

writeFileSync(
  new URL("./mst-proof.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Single-entry MST. Inclusion proof is the root node plus the record.",
      key: "pub.tangled.post/3jzfcijpjiz00",
      record_cid: recordCid,
      node: {
        l: null,
        e: [
          {
            p: 0,
            k_hex: bytesToHex(utf8ToBytes("pub.tangled.post/3jzfcijpjiz00")),
            v: recordCid,
            t: null,
          },
        ],
      },
      node_cbor_hex: bytesToHex(mstBytes),
      node_cid: mstCid,
      commit: {
        ...unsignedCommit,
        sig_hex: commitSigned.hexSig,
      },
      commit_cbor_unsigned_hex: bytesToHex(commitSigned.bytes),
    },
    null,
    2,
  ) + "\n",
);

const unsignedCap = {
  v: 1,
  iss: alice.did,
  aud: device.did,
  cmd: "repo/read",
  res: `tangled://${alice.did}/pub.tangled.chat`,
  nbf: 1700000000,
  exp: 1700000000 + 3600,
  pol: [],
  prf: [],
};
const capSigned = signUnsigned(alice.sk, unsignedCap);

writeFileSync(
  new URL("./capability-chain.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Root capability: identity → device, repo/read on one collection. prf empty.",
      issuer: alice.did,
      audience: device.did,
      unsigned: unsignedCap,
      unsigned_cbor_hex: bytesToHex(capSigned.bytes),
      signature_hex: capSigned.hexSig,
      attenuation_negative: {
        comment: "Child amplifying to tangled://did (parent prefix shorter) MUST fail",
        child_res: `tangled://${alice.did}`,
        valid: false,
      },
    },
    null,
    2,
  ) + "\n",
);

// HPKE sealed: we don't run full HPKE here; we record X25519 keys and a ChaCha20-Poly1305
// stand-in documenting the info string. Real HPKE enc/ct generated if x25519 shared secret
// is used as a simplified vector labeled "NOT RFC 9180 ciphertext — see hpke.mode".
const aliceAgrSk = blake3(utf8ToBytes("alice-agreement"), { dkLen: 32 });
const aliceAgrPk = x25519.getPublicKey(aliceAgrSk);
const bobAgrSk = blake3(utf8ToBytes("bob-agreement"), { dkLen: 32 });
const bobAgrPk = x25519.getPublicKey(bobAgrSk);
const shared = x25519.getSharedSecret(bobAgrSk, aliceAgrPk);

writeFileSync(
  new URL("./hpke-sealed.json", import.meta.url),
  JSON.stringify(
    {
      comment: "X25519 key agreement fixtures for HPKE DHKEM(X25519). Implementations MUST run RFC 9180 base mode with info tangled.hpke.record.v1. shared_secret_hex is the raw X25519 shared secret (DHKEM input), not the HPKE key.",
      recipient_agreement_sk_hex: bytesToHex(aliceAgrSk),
      recipient_agreement_pk_hex: bytesToHex(aliceAgrPk),
      sender_ephemeral_sk_hex: bytesToHex(bobAgrSk),
      sender_ephemeral_pk_hex: bytesToHex(bobAgrPk),
      x25519_shared_hex: bytesToHex(shared),
      hpke: {
        kem_id: "0x0020",
        kdf_id: "0x0001",
        aead_id: "0x0003",
        info: "tangled.hpke.record.v1",
        mode: "base",
      },
      plaintext_hex: bytesToHex(utf8ToBytes("secret body")),
    },
    null,
    2,
  ) + "\n",
);

const epochSecret0 = blake3(utf8ToBytes("epoch-0"));
const epochSecret1 = blake3(utf8ToBytes("epoch-1-from-mls-exporter"));

const members = Array.from({ length: 16 }, (_, i) => keypair(`member-${i}`));
const newMember = keypair("member-16");
const offline = members[3];

writeFileSync(
  new URL("./mls-welcome-commit.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Wrapping-only. payload_hex is not RFC 9420 TLS bytes (not byte-stable across MLS stacks). Ciphersuite 0x0001, exporter label tangled.epoch, context uint64be(N). Leaf HPKE is Tangled agreement; Welcome sealed to init_key ≠ leaf. Live bytes: cargo test -p tangled --lib mls::",
      ciphersuite: "0x0001",
      application_id: "tangled.circle.v1",
      exporter_label: "tangled.epoch",
      exporter_context_epoch_1_hex: bytesToHex(new Uint8Array([0, 0, 0, 0, 0, 0, 0, 1])),
      exporter_length: 32,
      epoch_secret_1_hex: bytesToHex(epochSecret1),
      welcome_record: {
        type: "welcome",
        epoch: 1,
        member: offline.did,
        payload_hex: bytesToHex(blake3(utf8ToBytes("synthetic-mls-welcome-offline"))),
      },
      commit_record: {
        type: "commit",
        epoch: 1,
        payload_hex: bytesToHex(blake3(utf8ToBytes("synthetic-mls-commit-grow"))),
      },
    },
    null,
    2,
  ) + "\n",
);

writeFileSync(
  new URL("./circle-grow-dual-stack.json", import.meta.url),
  JSON.stringify(
    {
      comment: "16 members on HPKE, 17th add triggers MLS. Circle id unchanged. One member offline. Pre-migration records still decrypt with epoch_secret[0]. distributor dual then mls. Never-downgrade: shrink would stay mls.",
      threshold: 16,
      circle_id: "3jzfcijpjiz0c",
      owner: members[0].did,
      members_before: members.map((m) => m.did),
      new_member: newMember.did,
      offline_member: offline.did,
      epoch_before: 0,
      distributor_before: "hpke",
      epoch_secret_0_hex: bytesToHex(epochSecret0),
      epoch_after: 1,
      distributor_after_transition: "dual",
      distributor_after_skew: "mls",
      epoch_secret_1_hex: bytesToHex(epochSecret1),
      circle_id_after: "3jzfcijpjiz0c",
      client_api_flag_flipped: false,
      history_share: false,
      pre_migration_record: {
        epoch: 0,
        decrypts_after_grow: true,
        aead: "ChaCha20-Poly1305",
        plaintext: "hello before grow",
      },
      dual_stack_max_skew_epochs: 8,
      dual_stack_max_skew_seconds: 259200,
      shrink_20_to_5: { distributor: "mls", never_downgrade: true },
    },
    null,
    2,
  ) + "\n",
);

writeFileSync(
  new URL("./handle-dns.json", import.meta.url),
  JSON.stringify(
    {
      handle: "alice.example.com",
      txt_name: "_tangled.alice.example.com",
      txt_values: [
        alice.did,
        `head=${cidV1DagCbor(commitSigned.bytes)}`,
      ],
      well_known: `https://alice.example.com/.well-known/tangled-did`,
      alsoKnownAs_must_include: "alice.example.com",
      one_sided_reject: true,
    },
    null,
    2,
  ) + "\n",
);

const nonce = blake3(utf8ToBytes("binding-nonce")).slice(0, 16);
const bindingMsg = utf8ToBytes(`tangled-binding:v1:${alice.did}:${bytesToHex(nonce)}`);
const nostrSk = blake3(utf8ToBytes("nostr-sk"), { dkLen: 32 });
// Schnorr not implemented here; store the message bytes implementations must sign with BIP-340.
writeFileSync(
  new URL("./binding-nostr.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Nostr binding. foreignSig is BIP-340 Schnorr over SHA-256(tangled-binding:v1:did:sol:…:noncehex). nostr_sk_hex is a fixture; produce sig with a BIP-340 library. attested MUST be false.",
      tangled: alice.did,
      protocol: "nostr",
      nostr_sk_hex: bytesToHex(nostrSk),
      nonce_hex: bytesToHex(nonce),
      message_utf8: `tangled-binding:v1:${alice.did}:${bytesToHex(nonce)}`,
      message_hex: bytesToHex(bindingMsg),
      attested: false,
    },
    null,
    2,
  ) + "\n",
);

const circleRkey = "3jzfcijpjiz0c";
const circleRes = `tangled://circle/${alice.did}/${circleRkey}`;

const circleReadValid = {
  v: 1,
  iss: alice.did,
  aud: device.did,
  cmd: "circle/read",
  res: circleRes,
  nbf: 1700000000,
  exp: 1700000000 + 3600,
  pol: [],
  prf: [],
};
const circleReadValidSigned = signUnsigned(alice.sk, circleReadValid);

const circleReadWrongIss = {
  ...circleReadValid,
  iss: bob.did,
};
const circleReadWrongIssSigned = signUnsigned(bob.sk, circleReadWrongIss);

const circleReadBadRes = {
  ...circleReadValid,
  res: `tangled://${alice.did}/sys.circle/${circleRkey}`,
};
const circleReadBadResSigned = signUnsigned(alice.sk, circleReadBadRes);

const siblingRead = {
  v: 1,
  iss: alice.did,
  aud: device.did,
  cmd: "repo/read",
  res: `tangled://${alice.did}/pub.tangled.chat`,
  nbf: 1700000000,
  exp: 1700000000 + 3600,
  pol: [],
  prf: [],
};
const siblingReadSigned = signUnsigned(alice.sk, siblingRead);

writeFileSync(
  new URL("./circle-read-auth.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Authorization vectors for circle/read. Signatures use the Solana off-chain envelope over BLAKE3(unsigned DAG-CBOR), same keys as solana-signature.json / capability-chain.json.",
      owner: alice.did,
      member: device.did,
      outsider: bob.did,
      circle_rkey: circleRkey,
      cases: [
        {
          id: "valid-membership-grant",
          accept: true,
          unsigned: circleReadValid,
          unsigned_cbor_hex: bytesToHex(circleReadValidSigned.bytes),
          signature_hex: circleReadValidSigned.hexSig,
          reason: "Root iss is the circle owner; cmd circle/read; res is tangled://circle/<owner>/<rkey>",
        },
        {
          id: "wrong-issuer",
          accept: false,
          unsigned: circleReadWrongIss,
          unsigned_cbor_hex: bytesToHex(circleReadWrongIssSigned.bytes),
          signature_hex: circleReadWrongIssSigned.hexSig,
          reason: "Root iss is not the circle owner and is not a cap/issue delegate in that owner sys/id",
        },
        {
          id: "resource-not-circle-uri",
          accept: false,
          unsigned: circleReadBadRes,
          unsigned_cbor_hex: bytesToHex(circleReadBadResSigned.bytes),
          signature_hex: circleReadBadResSigned.hexSig,
          reason: "circle/read res MUST be tangled://circle/<owner>/<rkey>, not a did collection path",
        },
        {
          id: "sibling-collection-leak",
          accept: false,
          unsigned: siblingRead,
          unsigned_cbor_hex: bytesToHex(siblingReadSigned.bytes),
          signature_hex: siblingReadSigned.hexSig,
          reason: "repo/read on pub.tangled.chat MUST NOT authorize circle/read or getRepoAuth of sibling collections",
          must_not_authorize: ["circle/read", "pub.tangled.post", "sys.circle"],
        },
      ],
    },
    null,
    2,
  ) + "\n",
);

const blobCid = cidV1Raw(utf8ToBytes("getblob-auth-fixture-bytes"));

writeFileSync(
  new URL("./getblob-auth.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Authorization policy for tangled.sync.getBlob. No new crypto: knots decide serve-without-cap vs require-cap from referencing records in the DID repo. blob_cid is a fixture CID (hash of a label), not required to exist as stored bytes.",
      did: alice.did,
      blob_cid: blobCid,
      cases: [
        {
          id: "public-ref-serve",
          expect: "serve-without-cap",
          records: [
            {
              collection: "pub.tangled.media",
              rkey: "3jzfcijpjiz0m",
              vis: "public",
              deleted: false,
              references_blob: true,
            },
          ],
        },
        {
          id: "sealed-only-require-cap",
          expect: "require-cap",
          records: [
            {
              collection: "pub.tangled.media",
              rkey: "3jzfcijpjiz0s",
              vis: "sealed",
              deleted: false,
              references_blob: true,
              circle: `${alice.did}#${circleRkey}`,
            },
          ],
        },
        {
          id: "deleted-public-require-cap",
          expect: "require-cap",
          records: [
            {
              collection: "pub.tangled.media",
              rkey: "3jzfcijpjiz0d",
              vis: "public",
              deleted: true,
              references_blob: true,
              note: "Tombstone (body null) is not a live public reference",
            },
          ],
        },
        {
          id: "public-and-sealed-serve",
          expect: "serve-without-cap",
          records: [
            {
              collection: "pub.tangled.media",
              rkey: "3jzfcijpjiz0p",
              vis: "public",
              deleted: false,
              references_blob: true,
            },
            {
              collection: "pub.tangled.media",
              rkey: "3jzfcijpjiz0q",
              vis: "sealed",
              deleted: false,
              references_blob: true,
            },
          ],
          note: "Same CID referenced from a live public record is public, even if a sealed record also references it",
        },
      ],
    },
    null,
    2,
  ) + "\n",
);

function atomKey(id) {
  return `${id.op}:${id.i}`;
}

function cmpAtomIdDesc(a, b) {
  if (a.op !== b.op) return a.op < b.op ? 1 : -1;
  return b.i - a.i;
}

function cmpAtomIdAsc(a, b) {
  if (a.op !== b.op) return a.op < b.op ? -1 : 1;
  return a.i - b.i;
}

const PAGE_ROOT = { op: "", i: 0 };

function recordPrev(rec) {
  return rec.prev ?? rec.unsigned?.prev ?? [];
}

function includedCids(records, compactCid) {
  const byCid = new Map(records.map((r) => [r.cid, r]));
  const seen = new Set();
  function walk(cid) {
    if (seen.has(cid)) return;
    seen.add(cid);
    const rec = byCid.get(cid);
    if (!rec) return;
    for (const p of recordPrev(rec)) walk(p);
  }
  walk(compactCid);
  seen.delete(compactCid);
  return seen;
}

function considerLww(lww, path, ts, cid, value) {
  const cur = lww[path];
  if (ts > cur.ts || (ts === cur.ts && cid >= cur.cid)) {
    lww[path] = { value, ts, cid };
  }
}

function applyPage(records) {
  const atoms = new Map();
  const lww = {
    title: { value: "", ts: -1, cid: "" },
    blob: { value: null, ts: -1, cid: "" },
  };

  const covered = new Set();
  for (const rec of records) {
    if (rec.body?.compact === 1) {
      for (const cid of includedCids(records, rec.cid)) covered.add(cid);
    }
  }

  for (const rec of records) {
    if (rec.body?.compact !== 1) continue;
    const snap = rec.body.snapshot;
    for (const atom of snap.atoms) {
      const parentKey =
        atom.parent == null ? atomKey(PAGE_ROOT) : atomKey(atom.parent);
      atoms.set(atomKey(atom.id), {
        id: { op: atom.id.op, i: atom.id.i },
        parentKey,
        parent: atom.parent == null ? null : { op: atom.parent.op, i: atom.parent.i },
        ch: atom.ch,
        del: atom.del,
      });
    }
    considerLww(lww, "title", snap.title.ts, snap.title.cid, snap.title.value);
    if (snap.blob != null) {
      considerLww(lww, "blob", snap.blob.ts, snap.blob.cid, snap.blob.value);
    }
  }

  const incremental = records.filter(
    (rec) => rec.body != null && rec.body.compact !== 1 && !covered.has(rec.cid),
  );

  for (const rec of incremental) {
    const cid = rec.cid;
    let i = 0;
    for (const op of rec.body.ops || []) {
      if (op.op !== "ins") continue;
      let parentKey;
      if (op.after == null) parentKey = atomKey(PAGE_ROOT);
      else if (op.after.op == null) parentKey = atomKey({ op: cid, i: op.after.i });
      else parentKey = atomKey(op.after);
      let parentId =
        op.after == null
          ? null
          : op.after.op == null
            ? { op: cid, i: op.after.i }
            : { op: op.after.op, i: op.after.i };
      for (const ch of [...op.run]) {
        const id = { op: cid, i };
        const key = atomKey(id);
        if (!atoms.has(key)) {
          atoms.set(key, { id, parentKey, parent: parentId, ch, del: false });
        }
        parentId = id;
        parentKey = key;
        i += 1;
      }
    }
  }

  for (const rec of incremental) {
    for (const op of rec.body.ops || []) {
      if (op.op === "del") {
        for (const id of op.ids) {
          const a = atoms.get(atomKey(id.op == null ? { op: rec.cid, i: id.i } : id));
          if (a) a.del = true;
        }
      } else if (op.op === "lww") {
        considerLww(lww, op.path, rec.ts, rec.cid, op.value);
      }
    }
  }

  const children = new Map();
  for (const a of atoms.values()) {
    if (!children.has(a.parentKey)) children.set(a.parentKey, []);
    children.get(a.parentKey).push(a);
  }
  for (const arr of children.values()) {
    arr.sort((x, y) => cmpAtomIdDesc(x.id, y.id));
  }

  let text = "";
  function walk(parentKey) {
    for (const a of children.get(parentKey) || []) {
      if (!a.del) text += a.ch;
      walk(atomKey(a.id));
    }
  }
  walk(atomKey(PAGE_ROOT));
  return {
    title: lww.title.value,
    text,
    blob: lww.blob.value,
    atoms,
    lww,
  };
}

function liveOf(state) {
  return { title: state.title, text: state.text, blob: state.blob };
}

function snapshotFromState(state) {
  const atoms = [...state.atoms.values()].sort((a, b) => cmpAtomIdAsc(a.id, b.id));
  return {
    atoms: atoms.map((a) => ({
      id: { op: a.id.op, i: a.id.i },
      parent: a.parent,
      ch: a.ch,
      del: a.del,
    })),
    title: {
      value: state.lww.title.value,
      ts: state.lww.title.ts,
      cid: state.lww.title.cid,
    },
    blob:
      state.lww.blob.ts < 0
        ? null
        : {
            value: state.lww.blob.value,
            ts: state.lww.blob.ts,
            cid: state.lww.blob.cid,
          },
  };
}

const pageSchemaBody = {
  v: 1,
  name: "pub.tangled.page",
  fields: { ops: { type: "array" } },
};
const pageSchemaCid = cidV1DagCbor(dagCbor(pageSchemaBody));

function signPage(label, ts, prev, ops) {
  const unsigned = {
    v: 1,
    type: pageSchemaCid,
    collection: "pub.tangled.page",
    rkey: "Home",
    author: alice.did,
    ts,
    prev,
    vis: "public",
    body: { ops },
  };
  const signedRec = signUnsigned(alice.sk, unsigned);
  const bytes = dagCbor({ ...unsigned, sig: signedRec.sig });
  return {
    label,
    cid: cidV1DagCbor(bytes),
    ts,
    prev,
    body: unsigned.body,
    unsigned,
    signature_hex: signedRec.hexSig,
    unsigned_cbor_hex: bytesToHex(signedRec.bytes),
  };
}

const pageR0 = signPage("R0", 1700000000000, [], [
  { op: "lww", path: "title", value: "Home" },
  { op: "ins", after: null, run: "Hello" },
]);
const afterO = { op: pageR0.cid, i: 4 };
const pageA = signPage("A", 1700000001000, [pageR0.cid], [
  { op: "ins", after: afterO, run: " Alice" },
]);
const pageB = signPage("B", 1700000002000, [pageR0.cid], [
  { op: "ins", after: afterO, run: " Bob" },
]);

const applyAB = applyPage([pageR0, pageA, pageB]);
const applyBA = applyPage([pageR0, pageB, pageA]);
if (applyAB.title !== applyBA.title || applyAB.text !== applyBA.text) {
  throw new Error(`page CRDT orders diverged: ${JSON.stringify(liveOf(applyAB))} vs ${JSON.stringify(liveOf(applyBA))}`);
}

const winnerFirst = pageA.cid > pageB.cid ? " Alice Bob" : " Bob Alice";
const expectedText = "Hello" + winnerFirst;
if (applyAB.text !== expectedText) {
  throw new Error(`page CRDT text ${JSON.stringify(applyAB.text)} != ${JSON.stringify(expectedText)}`);
}

function pageRecordJson(id, rec) {
  return {
    id,
    cid: rec.cid,
    unsigned: rec.unsigned,
    signature_hex: rec.signature_hex,
    unsigned_cbor_hex: rec.unsigned_cbor_hex,
  };
}

writeFileSync(
  new URL("./page-crdt-concurrent.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Two concurrent ins ops on pub.tangled.page after the same atom, neither in the other's prev. Apply(R0,A,B) MUST equal Apply(R0,B,A). Sibling order is atom-id descending (CID text, then i). Each op is a signed record; op-id is the record CID.",
      algorithm: "rga-text + lww-register title/blob",
      collection: "pub.tangled.page",
      rkey: "Home",
      author: alice.did,
      schema_cid: pageSchemaCid,
      records: [
        pageRecordJson("R0", pageR0),
        pageRecordJson("A", pageA),
        pageRecordJson("B", pageB),
      ],
      apply_set: ["R0", "A", "B"],
      orders: [
        { order: ["R0", "A", "B"], live: liveOf(applyAB) },
        { order: ["R0", "B", "A"], live: liveOf(applyBA) },
      ],
      live: liveOf(applyAB),
      orders_equal: true,
      sibling_rule: "atom-id descending (multibase base32 CID text, then i)",
      greater_cid: pageA.cid > pageB.cid ? "A" : "B",
    },
    null,
    2,
  ) + "\n",
);

const frontier = [pageA.cid, pageB.cid].sort();
const compactSnapshot = snapshotFromState(applyAB);
function signPageBody(label, ts, prev, body) {
  const unsigned = {
    v: 1,
    type: pageSchemaCid,
    collection: "pub.tangled.page",
    rkey: "Home",
    author: alice.did,
    ts,
    prev,
    vis: "public",
    body,
  };
  const signedRec = signUnsigned(alice.sk, unsigned);
  const bytes = dagCbor({ ...unsigned, sig: signedRec.sig });
  return {
    label,
    cid: cidV1DagCbor(bytes),
    ts,
    prev,
    body: unsigned.body,
    unsigned,
    signature_hex: signedRec.hexSig,
    unsigned_cbor_hex: bytesToHex(signedRec.bytes),
  };
}

const included = [pageR0.cid, pageA.cid, pageB.cid].sort();
const pageK = signPageBody("K", 1700000003000, frontier, {
  compact: 1,
  frontier,
  included,
  snapshot: compactSnapshot,
});
const pageL = signPage("L", 1700000004000, frontier, [
  { op: "ins", after: afterO, run: "!" },
]);

const applyK = applyPage([pageK]);
const applyKL = applyPage([pageK, pageL]);
const applyLK = applyPage([pageL, pageK]);
const applyFullL = applyPage([pageR0, pageA, pageB, pageL]);
const applyAll = applyPage([pageR0, pageA, pageB, pageK, pageL]);

function sameLive(a, b) {
  return a.title === b.title && a.text === b.text && a.blob === b.blob;
}
if (!sameLive(applyK, applyAB)) {
  throw new Error(`compact snapshot diverged from R0,A,B: ${JSON.stringify(liveOf(applyK))} vs ${JSON.stringify(liveOf(applyAB))}`);
}
if (!sameLive(applyKL, applyFullL) || !sameLive(applyLK, applyFullL) || !sameLive(applyAll, applyFullL)) {
  throw new Error(
    `compact+late did not converge: K+L=${JSON.stringify(liveOf(applyKL))} L+K=${JSON.stringify(liveOf(applyLK))} hist+L=${JSON.stringify(liveOf(applyFullL))} all=${JSON.stringify(liveOf(applyAll))}`,
  );
}

const lateConcurrent =
  !frontier.includes(pageL.cid) &&
  !pageL.prev.includes(pageK.cid) &&
  !pageK.prev.includes(pageL.cid);

writeFileSync(
  new URL("./page-crdt-compact.json", import.meta.url),
  JSON.stringify(
    {
      comment: "Concurrent ins A and B, then compact K whose frontier includes both and whose included set I(C) is R0,A,B. Late op L has the same prev as K (not K itself), so L is concurrent with K. Apply(R0,A,B,K) MUST equal Apply(R0,A,B). Apply(R0,A,B,K,L) MUST equal Apply(R0,A,B,L). A reader that has L MUST merge L on top of K’s snapshot; K is stale relative to L, not invalid. Apply(K) alone MUST fail: I(C) blocks are required.",
      algorithm: "page compact snapshot + rga-text + lww-register",
      collection: "pub.tangled.page",
      rkey: "Home",
      author: alice.did,
      schema_cid: pageSchemaCid,
      records: [
        pageRecordJson("R0", pageR0),
        pageRecordJson("A", pageA),
        pageRecordJson("B", pageB),
        pageRecordJson("K", pageK),
        pageRecordJson("L", pageL),
      ],
      compact_id: "K",
      frontier_ids: ["A", "B"],
      late_id: "L",
      concurrent_with_compact: lateConcurrent,
      stale_vs_extra_ops: ["L"],
      included_ids: ["R0", "A", "B"],
      apply_compact_with_included: liveOf(applyAB),
      apply_history: liveOf(applyAB),
      apply_compact_plus_late: liveOf(applyFullL),
      apply_late_plus_compact: liveOf(applyFullL),
      apply_full_history_plus_late: liveOf(applyFullL),
      orders: [
        { order: ["R0", "A", "B", "K"], live: liveOf(applyAB) },
        { order: ["R0", "A", "B", "K", "L"], live: liveOf(applyFullL) },
        { order: ["L", "R0", "A", "B", "K"], live: liveOf(applyFullL) },
        { order: ["R0", "A", "B", "L"], live: liveOf(applyFullL) },
      ],
      live: liveOf(applyKL),
      converges: true,
      sibling_rule: "atom-id descending (multibase base32 CID text, then i)",
    },
    null,
    2,
  ) + "\n",
);

writeFileSync(
  new URL("./README.md", import.meta.url),
  `# Test vectors

Generated by \`node generate.mjs\` (deterministic seed \`BLAKE3("tangled-vector-seed-v1")\`).

| File | Checks |
| --- | --- |
| unsigned-record.json | DAG-CBOR + BLAKE3 of unsigned record |
| solana-signature.json | Off-chain envelope + Ed25519 verify / negative |
| mst-proof.json | Single-leaf MST CID + commit signature |
| capability-chain.json | Root cap + attenuation negative |
| hpke-sealed.json | X25519 DHKEM inputs + HPKE ciphersuite |
| mls-welcome-commit.json | Ciphersuite, exporter label, wrapping records (payload_hex is wrapping-only; RFC 9420 bytes from OpenMLS in \`mls\` tests) |
| circle-grow-dual-stack.json | Threshold grow, offline member, never-downgrade |
| handle-dns.json | TXT + bidirectional alsoKnownAs (SNS layout is spec 13, not this file) |
| binding-nostr.json | Binding message bytes for BIP-340 |
| circle-read-auth.json | circle/read accept/reject (wrong issuer, non-circle URI, sibling leak) |
| getblob-auth.json | getBlob serve-without-cap vs require-cap |
| httpsig-ed25519.json | RFC 9421 ed25519 + Content-Digest covered components |
| page-crdt-concurrent.json | Concurrent page ins ops; both apply orders equal |
| page-crdt-compact.json | Compact I(C) is R0,A,B; late op concurrent with the compact still applies |

MLS Welcome/Commit \`payload_hex\` values are wrapping-only stand-ins, not RFC 9420 TLS bytes (those are not byte-stable across MLS libraries). A conforming MLS stack MUST produce payloads that export \`epoch_secret_1_hex\` with label \`tangled.epoch\` and 8-byte big-endian epoch context. Leaf HPKE is the Tangled X25519 \`agreement\` key; Welcome is sealed to \`init_key\`. The Tangled record wrapping around those payloads is normative. Live RFC 9420 coverage is \`cargo test -p tangled --lib mls::\`. OpenMLS Welcome hex is not frozen (not deterministic).
`,
);

console.log("wrote vectors for", alice.did);
