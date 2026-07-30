# compliance-indexer

An off-chain reference indexer that subscribes to events emitted by the three
`compliance-primitives` contracts and materialises them into a local SQLite
database. Issuers can query the database directly or build a thin API on top
of it — this is the foundation, not the full product.

## Stack choice

| Component | Choice | Why |
|-----------|--------|-----|
| Runtime | Node.js 20+ | Ships everywhere, native `fetch`, zero install friction |
| Language | TypeScript | Type safety without a heavy compile step (`tsx` for dev) |
| Database | SQLite (better-sqlite3) | Single file, no server, trivially swapped for Postgres |
| RPC | Raw JSON-RPC (`getEvents`) | Minimal deps; only one RPC method is needed |

To use Postgres instead of SQLite, swap `better-sqlite3` for `pg` and
translate the SQL in `src/db.ts` — the schema is plain ANSI SQL.

---

## Setup

```sh
cd tools/indexer
npm install

cp .env.example .env
# Edit .env — at minimum set one contract ID
```

Run in dev mode (no compile step):

```sh
npm run dev
```

Build + run:

```sh
npm run build
npm start
```

---

## Configuration (`.env`)

| Variable | Default | Description |
|----------|---------|-------------|
| `RPC_URL` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `NETWORK_PASSPHRASE` | testnet passphrase | Network passphrase |
| `ALLOWLIST_CONTRACT_ID` | _(empty)_ | Contract ID of your `allowlist-token` deployment |
| `DENYLIST_CONTRACT_ID` | _(empty)_ | Contract ID of your `denylist-gate` deployment |
| `JURISDICTION_CONTRACT_ID` | _(empty)_ | Contract ID of your `jurisdiction-flag` deployment |
| `DB_PATH` | `compliance.db` | SQLite file path |
| `POLL_INTERVAL_MS` | `5000` | How often to poll the RPC node |
| `START_LEDGER` | `0` | Ledger to start from (0 = auto ~24h ago) |

---

## Database schema

The indexer writes to four tables.

### `events` — raw audit log

Every compliance event that has ever been observed, in ledger order.

| Column | Type | Description |
|--------|------|-------------|
| `id` | `INTEGER` PK | Auto-increment |
| `ledger_sequence` | `INTEGER` | Ledger the event landed in |
| `timestamp` | `INTEGER` | Unix seconds (from ledger close time) |
| `contract_id` | `TEXT` | Emitting contract |
| `event_type` | `TEXT` | `AllowAdd` \| `AllowRemove` \| `Blocked` \| `DenyAdd` \| `DenyRemove` \| `JurisdictionSet` |
| `address` | `TEXT` | Primary subject address |
| `address_to` | `TEXT` | Secondary address (`Blocked` only) |
| `amount` | `TEXT` | Transfer amount as decimal string (`Blocked` only) |
| `jurisdiction` | `TEXT` | ISO jurisdiction code (`JurisdictionSet` only) |
| `raw_topics` | `TEXT` | JSON array of base64-XDR topic values |
| `raw_data` | `TEXT` | Base64-XDR data value |

### `allowlist` — current membership

| Column | Type | Description |
|--------|------|-------------|
| `contract_id` | `TEXT` | Which allowlist-token contract |
| `address` | `TEXT` | Address currently on the allowlist |

`AllowAdd` inserts; `AllowRemove` deletes. Query the full current set with:

```sql
SELECT address FROM allowlist WHERE contract_id = '<your-contract-id>';
```

### `denylist` — current membership

Same structure as `allowlist`.

```sql
SELECT address FROM denylist WHERE contract_id = '<your-contract-id>';
```

### `jurisdictions` — current assignments

| Column | Type |
|--------|------|
| `contract_id` | `TEXT` |
| `address` | `TEXT` |
| `code` | `TEXT` |

Last `JurisdictionSet` wins (upsert). Query with:

```sql
SELECT address, code FROM jurisdictions WHERE contract_id = '<your-contract-id>';
-- Filter by jurisdiction:
SELECT address FROM jurisdictions WHERE contract_id = '...' AND code = 'US';
```

### `indexer_state`

Internal key/value table. Stores `last_ledger` so restarts resume without
re-scanning from the beginning.

---

## Example queries

```sql
-- All addresses currently on the denylist
SELECT address FROM denylist WHERE contract_id = 'C...';

-- Full history for one address
SELECT ledger_sequence, timestamp, event_type, contract_id
FROM events
WHERE address = 'G...' OR address_to = 'G...'
ORDER BY ledger_sequence;

-- Blocked transfer attempts in the last 1000 ledgers
SELECT ledger_sequence, address AS from_addr, address_to, amount
FROM events
WHERE event_type = 'Blocked'
  AND ledger_sequence > (SELECT CAST(value AS INTEGER) FROM indexer_state WHERE key = 'last_ledger') - 1000;

-- Addresses that have ever been on the allowlist but are no longer
SELECT DISTINCT e.address
FROM events e
WHERE e.event_type = 'AllowAdd'
  AND NOT EXISTS (
    SELECT 1 FROM allowlist a WHERE a.contract_id = e.contract_id AND a.address = e.address
  );
```

---

## How it works

1. On startup, reads `last_ledger` from `indexer_state`.
2. Calls `getEvents` on the Soroban RPC node, filtered to the configured
   contract IDs, from `last_ledger + 1` to `latestLedger`.
3. Decodes each event's XDR topics/data into typed structs.
4. Applies events to both the raw `events` log and the materialised state
   tables (`allowlist`, `denylist`, `jurisdictions`) inside a single SQLite
   transaction per poll cycle.
5. Persists the new `last_ledger` and sleeps until the next poll.

---

## Off-chain indexer vs. on-chain audit-log contract (#108)

These two approaches solve overlapping but different problems:

| | This indexer | On-chain audit-log (#108) |
|---|---|---|
| **Where data lives** | Off-chain SQLite/Postgres | On-chain Soroban storage |
| **Query flexibility** | Arbitrary SQL | Limited to contract views |
| **Cost** | Free (no ledger fees) | Every write costs XLM |
| **Trust model** | Operator must not tamper | Immutable, verifiable by anyone |
| **Queryable from contracts** | No | Yes (cross-contract call) |
| **Historical range** | Back to any indexed ledger | Back to contract deployment |
| **Operational complexity** | Requires a running service | Zero — it's just a contract |

**Use this indexer when** you need rich querying (filters, joins, aggregates),
want to power a dashboard or compliance reporting tool, or need to correlate
events across contracts without paying on-chain storage costs.

**Use the on-chain audit-log (#108) when** other contracts need to read the
compliance history directly, you need a tamper-proof on-chain record, or you
don't want to run a separate service.

**Use both** when you need both: the on-chain log for contract-to-contract
verifiability, and this indexer for cheap, flexible off-chain reporting.

---

## Assumptions and known limitations

- **RPC retention window**: Soroban RPC nodes only retain events for a finite
  window (~7 days on testnet, configurable on mainnet). If the indexer is
  offline for longer than the retention window, events in that gap will be
  permanently missed. For a production deployment, run the indexer
  continuously or bootstrap from an archive node.

- **No gap detection**: the indexer does not detect or fill gaps. If a gap
  occurs, it will silently resume from `last_ledger + 1` with no warning.
  A production implementation should compare `last_ledger` against the
  node's `oldestLedger` response and alert if data has been pruned.

- **Single-node RPC**: there is no failover between multiple RPC endpoints.
  If the configured node is down, polls will error and retry next tick.

- **No XDR SDK dependency**: topics/data are decoded with a hand-rolled XDR
  reader. This covers the five event types in this repo exactly. If contract
  events gain new non-topic fields, `src/decoder.ts` will need updating.

- **Not production-hardened**: no authentication, no rate-limit handling, no
  metrics, no alerting. Treat this as a starting point, not a finished service.
