export function loadConfig() {
    const rpcUrl = process.env.RPC_URL ?? "https://soroban-testnet.stellar.org";
    const networkPassphrase = process.env.NETWORK_PASSPHRASE ??
        "Test SDF Network ; September 2015";
    const allowlistContractId = process.env.ALLOWLIST_CONTRACT_ID ?? "";
    const denylistContractId = process.env.DENYLIST_CONTRACT_ID ?? "";
    const jurisdictionContractId = process.env.JURISDICTION_CONTRACT_ID ?? "";
    if (!allowlistContractId && !denylistContractId && !jurisdictionContractId) {
        console.warn("Warning: no contract IDs configured — nothing will be indexed.\n" +
            "Set at least one of ALLOWLIST_CONTRACT_ID, DENYLIST_CONTRACT_ID, " +
            "JURISDICTION_CONTRACT_ID in your environment.");
    }
    return {
        rpcUrl,
        networkPassphrase,
        allowlistContractId,
        denylistContractId,
        jurisdictionContractId,
        dbPath: process.env.DB_PATH ?? "compliance.db",
        pollIntervalMs: Number(process.env.POLL_INTERVAL_MS ?? "5000"),
        startLedger: Number(process.env.START_LEDGER ?? "0"),
    };
}
