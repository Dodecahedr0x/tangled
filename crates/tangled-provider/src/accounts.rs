//! Email accounts for the hosted knot. Seeds stay on the provider disk.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rusqlite::{params, Connection};

use crate::AccountError;

pub struct NewAccount {
    pub email: String,
    pub display_name: String,
    pub did: String,
    pub ed_seed: [u8; 32],
    pub x_seed: [u8; 32],
}

#[derive(Clone)]
pub struct Account {
    pub id: i64,
    pub display_name: String,
    pub did: String,
    pub ed_seed: [u8; 32],
    pub x_seed: [u8; 32],
}

pub fn migrate(conn: &Connection) -> Result<(), AccountError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY,
            email TEXT NOT NULL UNIQUE COLLATE NOCASE,
            password_hash TEXT NOT NULL,
            display_name TEXT NOT NULL,
            did TEXT NOT NULL UNIQUE,
            ed_seed BLOB NOT NULL,
            x_seed BLOB NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY,
            account_id INTEGER NOT NULL,
            exp INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS moves (
            code TEXT PRIMARY KEY,
            account_id INTEGER NOT NULL,
            exp INTEGER NOT NULL,
            used INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS incoming (
            code TEXT NOT NULL,
            knot TEXT NOT NULL,
            ed_seed BLOB NOT NULL,
            x_seed BLOB NOT NULL,
            PRIMARY KEY (code, knot)
        );",
    )
    .map_err(store)?;
    if let Err(e) = conn.execute(
        "ALTER TABLE accounts ADD COLUMN custodial INTEGER NOT NULL DEFAULT 1",
        [],
    ) {
        if !e.to_string().to_lowercase().contains("duplicate") {
            return Err(store(e));
        }
    }
    Ok(())
}

pub fn insert_account(
    conn: &Connection,
    account: &NewAccount,
    password: &str,
    now: i64,
) -> Result<i64, AccountError> {
    let hash = hash_password(password)?;
    conn.execute(
        "INSERT INTO accounts (email, password_hash, display_name, did, ed_seed, x_seed, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            account.email,
            hash,
            account.display_name,
            account.did,
            account.ed_seed.as_slice(),
            account.x_seed.as_slice(),
            now,
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AccountError::EmailTaken
        } else {
            store(e)
        }
    })?;
    Ok(conn.last_insert_rowid())
}

pub fn login(conn: &Connection, email: &str, password: &str) -> Result<Account, AccountError> {
    let account = by_email(conn, email)?.ok_or(AccountError::BadLogin)?;
    let hash: String = conn
        .query_row(
            "SELECT password_hash FROM accounts WHERE id = ?1",
            [account.id],
            |row| row.get(0),
        )
        .map_err(store)?;
    let parsed = PasswordHash::new(&hash).map_err(|e| AccountError::Store(e.to_string()))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AccountError::BadLogin)?;
    Ok(account)
}

pub fn by_email(conn: &Connection, email: &str) -> Result<Option<Account>, AccountError> {
    match conn.query_row(
        "SELECT id, display_name, did, ed_seed, x_seed FROM accounts WHERE email = ?1",
        [email],
        row_account,
    ) {
        Ok(account) => Ok(Some(account)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(store(e)),
    }
}

pub fn by_id(conn: &Connection, id: i64) -> Result<Option<Account>, AccountError> {
    match conn.query_row(
        "SELECT id, display_name, did, ed_seed, x_seed FROM accounts WHERE id = ?1",
        [id],
        row_account,
    ) {
        Ok(account) => Ok(Some(account)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(store(e)),
    }
}

pub fn all_seeds(conn: &Connection) -> Result<Vec<Account>, AccountError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, display_name, did, ed_seed, x_seed FROM accounts WHERE custodial = 1 ORDER BY id",
        )
        .map_err(store)?;
    let rows = stmt
        .query_map([], row_account)
        .map_err(store)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(store)?;
    Ok(rows)
}

pub fn names(conn: &Connection) -> Result<Vec<(String, String)>, AccountError> {
    let mut stmt = conn
        .prepare("SELECT did, display_name FROM accounts")
        .map_err(store)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(store)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(store)?;
    Ok(rows)
}

pub fn insert_session(
    conn: &Connection,
    token: &str,
    account_id: i64,
    exp: i64,
) -> Result<(), AccountError> {
    conn.execute(
        "INSERT INTO sessions (token, account_id, exp) VALUES (?1, ?2, ?3)",
        params![token, account_id, exp],
    )
    .map_err(store)?;
    Ok(())
}

pub fn account_for_session(
    conn: &Connection,
    token: &str,
    now: i64,
) -> Result<Option<Account>, AccountError> {
    let found: Result<(i64, i64), rusqlite::Error> = conn.query_row(
        "SELECT account_id, exp FROM sessions WHERE token = ?1",
        [token],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    let (id, exp) = match found {
        Ok(pair) => pair,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(store(e)),
    };
    if exp <= now {
        let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", [token]);
        return Ok(None);
    }
    by_id(conn, id)
}

pub fn signing_allowed(conn: &Connection, did: &str) -> Result<bool, AccountError> {
    match conn.query_row(
        "SELECT custodial FROM accounts WHERE did = ?1",
        [did],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(v) => Ok(v != 0),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
        Err(e) => Err(store(e)),
    }
}

pub fn email_of(conn: &Connection, id: i64) -> Result<String, AccountError> {
    conn.query_row("SELECT email FROM accounts WHERE id = ?1", [id], |row| {
        row.get(0)
    })
    .map_err(store)
}

pub fn by_did(conn: &Connection, did: &str) -> Result<Option<Account>, AccountError> {
    match conn.query_row(
        "SELECT id, display_name, did, ed_seed, x_seed FROM accounts WHERE did = ?1 AND custodial = 1",
        [did],
        row_account,
    ) {
        Ok(account) => Ok(Some(account)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(store(e)),
    }
}

/// One live code per account. Replaces any unused code.
pub fn start_move(
    conn: &Connection,
    account_id: i64,
    code: &str,
    exp: i64,
) -> Result<(), AccountError> {
    conn.execute(
        "DELETE FROM moves WHERE account_id = ?1 AND used = 0",
        [account_id],
    )
    .map_err(store)?;
    conn.execute(
        "INSERT INTO moves (code, account_id, exp, used) VALUES (?1, ?2, ?3, 0)",
        params![code, account_id, exp],
    )
    .map_err(store)?;
    Ok(())
}

pub fn active_code(
    conn: &Connection,
    account_id: i64,
    now: i64,
) -> Result<Option<String>, AccountError> {
    match conn.query_row(
        "SELECT code FROM moves WHERE account_id = ?1 AND used = 0 AND exp > ?2 ORDER BY exp DESC LIMIT 1",
        params![account_id, now],
        |row| row.get(0),
    ) {
        Ok(code) => Ok(Some(code)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(store(e)),
    }
}

pub struct MoveAccount {
    pub account: Account,
    pub email: String,
    pub used: bool,
}

pub fn move_account(
    conn: &Connection,
    code: &str,
    now: i64,
) -> Result<Option<MoveAccount>, AccountError> {
    let found: Result<(i64, i64, i64), rusqlite::Error> = conn.query_row(
        "SELECT account_id, exp, used FROM moves WHERE code = ?1",
        [code],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );
    let (id, exp, used) = match found {
        Ok(row) => row,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(store(e)),
    };
    if used == 0 && exp <= now {
        return Ok(None);
    }
    let account = by_id(conn, id)?.ok_or_else(|| AccountError::Store("missing account".into()))?;
    let email = email_of(conn, id)?;
    Ok(Some(MoveAccount {
        account,
        email,
        used: used != 0,
    }))
}

pub fn wipe_custodial(conn: &Connection, account_id: i64, code: &str) -> Result<(), AccountError> {
    let zeros = [0u8; 32];
    conn.execute(
        "UPDATE accounts SET ed_seed = ?1, x_seed = ?1, custodial = 0 WHERE id = ?2",
        params![zeros.as_slice(), account_id],
    )
    .map_err(store)?;
    conn.execute("DELETE FROM sessions WHERE account_id = ?1", [account_id])
        .map_err(store)?;
    conn.execute("UPDATE moves SET used = 1 WHERE code = ?1", [code])
        .map_err(store)?;
    Ok(())
}

pub type SeedPair = ([u8; 32], [u8; 32]);

pub fn incoming_key(
    conn: &Connection,
    code: &str,
    knot: &str,
) -> Result<Option<SeedPair>, AccountError> {
    let found = conn.query_row(
        "SELECT ed_seed, x_seed FROM incoming WHERE code = ?1 AND knot = ?2",
        params![code, knot],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
    );
    match found {
        Ok((ed, x)) => {
            let ed: [u8; 32] = ed
                .try_into()
                .map_err(|_| AccountError::Store("incoming ed seed".into()))?;
            let x: [u8; 32] = x
                .try_into()
                .map_err(|_| AccountError::Store("incoming x seed".into()))?;
            Ok(Some((ed, x)))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(store(e)),
    }
}

pub fn save_incoming(
    conn: &Connection,
    code: &str,
    knot: &str,
    ed: &[u8; 32],
    x: &[u8; 32],
) -> Result<(), AccountError> {
    conn.execute(
        "INSERT INTO incoming (code, knot, ed_seed, x_seed) VALUES (?1, ?2, ?3, ?4)",
        params![code, knot, ed.as_slice(), x.as_slice()],
    )
    .map_err(store)?;
    Ok(())
}

pub fn delete_incoming(conn: &Connection, code: &str, knot: &str) -> Result<(), AccountError> {
    conn.execute(
        "DELETE FROM incoming WHERE code = ?1 AND knot = ?2",
        params![code, knot],
    )
    .map_err(store)?;
    Ok(())
}

pub fn delete_session(conn: &Connection, token: &str) -> Result<(), AccountError> {
    conn.execute("DELETE FROM sessions WHERE token = ?1", [token])
        .map_err(store)?;
    Ok(())
}

fn row_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
    let ed: Vec<u8> = row.get(3)?;
    let x: Vec<u8> = row.get(4)?;
    Ok(Account {
        id: row.get(0)?,
        display_name: row.get(1)?,
        did: row.get(2)?,
        ed_seed: ed.try_into().map_err(|_| {
            rusqlite::Error::InvalidColumnType(3, "ed_seed".into(), rusqlite::types::Type::Blob)
        })?,
        x_seed: x.try_into().map_err(|_| {
            rusqlite::Error::InvalidColumnType(4, "x_seed".into(), rusqlite::types::Type::Blob)
        })?,
    })
}

fn hash_password(password: &str) -> Result<String, AccountError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AccountError::Store(e.to_string()))
}

fn store(e: rusqlite::Error) -> AccountError {
    AccountError::Store(e.to_string())
}
