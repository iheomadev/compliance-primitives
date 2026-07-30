/**
 * Entry point — wires config, DB, and Indexer together and runs the
 * poll loop until SIGINT/SIGTERM.
 */

import { loadConfig } from "./config.js";
import { ComplianceDb } from "./db.js";
import { Indexer } from "./indexer.js";

async function main(): Promise<void> {
  const config = loadConfig();
  const db = await ComplianceDb.open(config.dbPath);
  const indexer = new Indexer(config, db);

  function shutdown(signal: string): void {
    console.log(`\nReceived ${signal}, shutting down…`);
    indexer.stop();
    db.close();
    process.exit(0);
  }

  process.on("SIGINT", () => shutdown("SIGINT"));
  process.on("SIGTERM", () => shutdown("SIGTERM"));

  indexer.start();
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
