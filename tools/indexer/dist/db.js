/**
 * Database layer — SQLite via sql.js (pure WebAssembly, no native build).
 *
 * sql.js operates in-memory and does not auto-persist to disk; we flush the
 * database file after every write batch so restarts pick up where they left
 * off.
 *
 * Schema
 * ──────
 *
 * events
 *   id              INTEGER PK AUTOINCREMENT
 *   ledger_sequence INTEGER NOT NULL
 *   timestamp       INTEGER            — Unix seconds (ledger close time)
 *   contract_id     TEXT NOT NULL
 *   event_type      TEXT NOT NULL      — AllowAdd | AllowRemove | Blocked |
 *                                        DenyAdd | DenyRemove | JurisdictionSet
 *   address         TEXT               — primary subject address
 *   address_to      TEXT               — secondary address (Blocked only)
 *   amount          TEXT               — i128 as decimal string (Blocked only)
 *   jurisdiction    TEXT               — ISO code (JurisdictionSet only)
 *   raw_topics      TEXT NOT NULL      — JSON array of base64-XDR topic strings
 *   raw_data        TEXT NOT NULL      — base64-XDR data value
 *
 * allowlist          — materialised current state
 *   contract_id TEXT NOT NULL
 *   address     TEXT NOT NULL
 *   PRIMARY KEY (contract_id, address)
 *
 * denylist           — materialised current state
 *   contract_id TEXT NOT NULL
 *   address     TEXT NOT NULL
 *   PRIMARY KEY (contract_id, address)
 *
 * jurisdictions      — materialised current state (last write wins)
 *   contract_id TEXT NOT NULL
 *   address     TEXT NOT NULL
 *   code        TEXT NOT NULL
 *   PRIMARY KEY (contract_id, address)
 *
 * indexer_state      — internal key/value (stores last_ledger)
 *   key   TEXT PK
 *   value TEXT NOT NULL
 */
import fs from "node:fs";
import initSqlJs from "sql.js";
// sql.js is loaded once as a module-level singleton
let SQL = null;
async function getSql() {
    if (!SQL)
        SQL = await initSqlJs();
    return SQL;
}
export class ComplianceDb {
    db;
    dbPath;
    constructor(dbPath) {
        this.dbPath = dbPath;
    }
    static async open(dbPath) {
        const sql = await getSql();
        const inst = new ComplianceDb(dbPath);
        if (fs.existsSync(dbPath)) {
            const data = fs.readFileSync(dbPath);
            inst.db = new sql.Database(data);
        }
        else {
            inst.db = new sql.Database();
        }
        inst.migrate();
        return inst;
    }
    migrate() {
        this.db.run(`
      CREATE TABLE IF NOT EXISTS events (
        id              INTEGER PRIMARY KEY AUTOINCREMENT,
        ledger_sequence INTEGER NOT NULL,
        timestamp       INTEGER,
        contract_id     TEXT    NOT NULL,
        event_type      TEXT    NOT NULL,
        address         TEXT,
        address_to      TEXT,
        amount          TEXT,
        jurisdiction    TEXT,
        raw_topics      TEXT    NOT NULL,
        raw_data        TEXT    NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_events_contract
        ON events (contract_id);
      CREATE INDEX IF NOT EXISTS idx_events_address
        ON events (address);
      CREATE INDEX IF NOT EXISTS idx_events_type
        ON events (event_type);
      CREATE INDEX IF NOT EXISTS idx_events_ledger
        ON events (ledger_sequence);

      CREATE TABLE IF NOT EXISTS allowlist (
        contract_id TEXT NOT NULL,
        address     TEXT NOT NULL,
        PRIMARY KEY (contract_id, address)
      );

      CREATE TABLE IF NOT EXISTS denylist (
        contract_id TEXT NOT NULL,
        address     TEXT NOT NULL,
        PRIMARY KEY (contract_id, address)
      );

      CREATE TABLE IF NOT EXISTS jurisdictions (
        contract_id TEXT NOT NULL,
        address     TEXT NOT NULL,
        code        TEXT NOT NULL,
        PRIMARY KEY (contract_id, address)
      );

      CREATE TABLE IF NOT EXISTS indexer_state (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );
    `);
        this.flush();
    }
    /** Flush the in-memory database to disk. */
    flush() {
        const data = this.db.export();
        fs.writeFileSync(this.dbPath, Buffer.from(data));
    }
    /** Apply a batch of events atomically, then flush to disk. */
    applyEvents(events) {
        this.db.run("BEGIN");
        try {
            for (const e of events) {
                this.insertEvent(e);
                this.updateState(e);
            }
            this.db.run("COMMIT");
        }
        catch (err) {
            this.db.run("ROLLBACK");
            throw err;
        }
        this.flush();
    }
    insertEvent(e) {
        this.db.run(`INSERT INTO events
         (ledger_sequence, timestamp, contract_id, event_type,
          address, address_to, amount, jurisdiction, raw_topics, raw_data)
       VALUES (?,?,?,?,?,?,?,?,?,?)`, [
            e.ledgerSequence,
            e.timestamp,
            e.contractId,
            e.eventType,
            e.address,
            e.addressTo,
            e.amount,
            e.jurisdiction,
            e.rawTopics,
            e.rawData,
        ]);
    }
    updateState(e) {
        switch (e.eventType) {
            case "AllowAdd":
                if (e.address) {
                    this.db.run("INSERT OR IGNORE INTO allowlist (contract_id, address) VALUES (?,?)", [e.contractId, e.address]);
                }
                break;
            case "AllowRemove":
                if (e.address) {
                    this.db.run("DELETE FROM allowlist WHERE contract_id = ? AND address = ?", [e.contractId, e.address]);
                }
                break;
            case "DenyAdd":
                if (e.address) {
                    this.db.run("INSERT OR IGNORE INTO denylist (contract_id, address) VALUES (?,?)", [e.contractId, e.address]);
                }
                break;
            case "DenyRemove":
                if (e.address) {
                    this.db.run("DELETE FROM denylist WHERE contract_id = ? AND address = ?", [e.contractId, e.address]);
                }
                break;
            case "JurisdictionSet":
                if (e.address && e.jurisdiction) {
                    this.db.run(`INSERT INTO jurisdictions (contract_id, address, code) VALUES (?,?,?)
             ON CONFLICT (contract_id, address) DO UPDATE SET code = excluded.code`, [e.contractId, e.address, e.jurisdiction]);
                }
                break;
            // Blocked: recorded in events log only, no state change
        }
    }
    getState(key) {
        const res = this.db.exec("SELECT value FROM indexer_state WHERE key = ?", [key]);
        if (res.length === 0 || res[0].values.length === 0)
            return undefined;
        return String(res[0].values[0][0]);
    }
    setState(key, value) {
        this.db.run(`INSERT INTO indexer_state (key, value) VALUES (?,?)
       ON CONFLICT (key) DO UPDATE SET value = excluded.value`, [key, value]);
        this.flush();
    }
    getLastIndexedLedger() {
        const v = this.getState("last_ledger");
        return v ? Number(v) : 0;
    }
    setLastIndexedLedger(ledger) {
        this.setState("last_ledger", String(ledger));
    }
    close() {
        this.flush();
        this.db.close();
    }
}
