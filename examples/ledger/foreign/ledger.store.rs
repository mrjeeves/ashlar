//! Foreign persistence shim for the `ledger` example: the datastore is a
//! real SQLite database file on disk. Ashlar *names* the operations
//! (`record`, `recent`, `total`); the SQL lives here, across the `foreign`
//! boundary (reference §9.10) — SQL is the persistence peer of CSS, named
//! in Ashlar and defined outside it (ADR-0014).
//!
//! Built as a std-only Rust `cdylib` that links the system `libsqlite3`
//! over the C ABI: no crate, no change to the Ashlar runtime. Compile with
//!   rustc --edition 2021 --crate-type cdylib -l sqlite3 \
//!         -o foreign/ledger.store.so foreign/ledger.store.rs
//!
//! ABI: `char* name(const char* args_json)` — arguments arrive as a JSON
//! array, the return is JSON, shape-checked at the call site by the
//! runtime. JSON parsing and shaping are handed to SQLite's built-in JSON1
//! functions, so this shim stays small and never hand-rolls JSON.
//!
//! The database location is a deployment fact, never Ashlar source (B5):
//! `ASHLAR_LEDGER_DB` if set, else a per-process file under the temp dir.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

#[link(name = "sqlite3")]
extern "C" {
    fn sqlite3_open(path: *const c_char, db: *mut *mut c_void) -> c_int;
    fn sqlite3_close(db: *mut c_void) -> c_int;
    fn sqlite3_exec(
        db: *mut c_void,
        sql: *const c_char,
        cb: *const c_void,
        arg: *mut c_void,
        err: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_prepare_v2(
        db: *mut c_void,
        sql: *const c_char,
        n: c_int,
        stmt: *mut *mut c_void,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_bind_text(
        stmt: *mut c_void,
        idx: c_int,
        text: *const c_char,
        n: c_int,
        destr: *const c_void,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut c_void) -> c_int;
    fn sqlite3_column_text(stmt: *mut c_void, col: c_int) -> *const u8;
    fn sqlite3_column_double(stmt: *mut c_void, col: c_int) -> f64;
    fn sqlite3_finalize(stmt: *mut c_void) -> c_int;
}

const SQLITE_ROW: c_int = 100;

// SQLITE_TRANSIENT (= (void*)-1): tells SQLite to copy the bound text.
fn transient() -> *const c_void {
    (-1isize) as *const c_void
}

fn db_path() -> CString {
    let p = std::env::var("ASHLAR_LEDGER_DB").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join(format!("ashlar-ledger-{}.db", std::process::id()))
            .to_string_lossy()
            .into_owned()
    });
    CString::new(p).unwrap()
}

// Open the database and ensure the table exists.
unsafe fn open() -> *mut c_void {
    let mut db: *mut c_void = std::ptr::null_mut();
    sqlite3_open(db_path().as_ptr(), &mut db);
    let ddl = CString::new(
        "CREATE TABLE IF NOT EXISTS entries(\
           id INTEGER PRIMARY KEY AUTOINCREMENT, who TEXT, note TEXT, amount REAL)",
    )
    .unwrap();
    sqlite3_exec(
        db,
        ddl.as_ptr(),
        std::ptr::null(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    db
}

fn ret(s: String) -> *mut c_char {
    CString::new(s).unwrap().into_raw()
}

/// `record(["who","note",amount])` — insert one row; returns `true`.
#[no_mangle]
pub extern "C" fn record(args: *const c_char) -> *mut c_char {
    let a = unsafe { CStr::from_ptr(args) }.to_string_lossy().into_owned();
    unsafe {
        let db = open();
        let sql = CString::new(
            "INSERT INTO entries(who,note,amount) VALUES(\
               json_extract(?1,'$[0]'), json_extract(?1,'$[1]'), json_extract(?1,'$[2]'))",
        )
        .unwrap();
        let mut st: *mut c_void = std::ptr::null_mut();
        sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut st, std::ptr::null_mut());
        let ca = CString::new(a).unwrap();
        sqlite3_bind_text(st, 1, ca.as_ptr(), -1, transient());
        sqlite3_step(st);
        sqlite3_finalize(st);
        sqlite3_close(db);
    }
    ret("true".to_string())
}

/// `recent([])` — every row, newest first, as a JSON array of
/// `{who,note,amount}`. SQLite's JSON1 shapes the rows to fit `[Entry]`.
#[no_mangle]
pub extern "C" fn recent(_args: *const c_char) -> *mut c_char {
    unsafe {
        let db = open();
        let sql = CString::new(
            "SELECT coalesce(json_group_array(json_object(\
               'who',who,'note',note,'amount',amount)),'[]') \
             FROM (SELECT who,note,amount FROM entries ORDER BY id DESC)",
        )
        .unwrap();
        let mut st: *mut c_void = std::ptr::null_mut();
        sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut st, std::ptr::null_mut());
        let out = if sqlite3_step(st) == SQLITE_ROW {
            let p = sqlite3_column_text(st, 0);
            if p.is_null() {
                "[]".to_string()
            } else {
                CStr::from_ptr(p as *const c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        } else {
            "[]".to_string()
        };
        sqlite3_finalize(st);
        sqlite3_close(db);
        ret(out)
    }
}

/// `total([])` — the running sum of every amount, as a JSON number.
#[no_mangle]
pub extern "C" fn total(_args: *const c_char) -> *mut c_char {
    unsafe {
        let db = open();
        let sql = CString::new("SELECT coalesce(sum(amount),0) FROM entries").unwrap();
        let mut st: *mut c_void = std::ptr::null_mut();
        sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut st, std::ptr::null_mut());
        let v = if sqlite3_step(st) == SQLITE_ROW {
            sqlite3_column_double(st, 0)
        } else {
            0.0
        };
        sqlite3_finalize(st);
        sqlite3_close(db);
        ret(format!("{}", v))
    }
}
