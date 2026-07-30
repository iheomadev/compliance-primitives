/**
 * Thin wrapper around Soroban RPC's getEvents endpoint.
 *
 * We deliberately use the raw HTTP JSON-RPC interface rather than pulling in
 * a full SDK — the only call we need is getEvents, and keeping deps minimal
 * makes this easier to port to other runtimes.
 *
 * Spec: https://developers.stellar.org/network/soroban-rpc/api-reference/methods/getEvents
 */
export class SorobanRpc {
    url;
    nextId = 1;
    constructor(url) {
        this.url = url;
    }
    async getEvents(params) {
        const body = {
            jsonrpc: "2.0",
            id: this.nextId++,
            method: "getEvents",
            params: {
                startLedger: params.startLedger,
                filters: params.filters,
                pagination: params.pagination ?? { limit: 200 },
            },
        };
        const res = await fetch(this.url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        if (!res.ok) {
            throw new Error(`RPC HTTP error ${res.status}: ${await res.text()}`);
        }
        const json = (await res.json());
        if (json.error) {
            throw new Error(`RPC error ${json.error.code}: ${json.error.message}`);
        }
        if (!json.result) {
            throw new Error("RPC returned neither result nor error");
        }
        return json.result;
    }
    /** Fetch the latest known ledger sequence (cheap liveness check). */
    async getLatestLedger() {
        const body = {
            jsonrpc: "2.0",
            id: this.nextId++,
            method: "getLatestLedger",
            params: {},
        };
        const res = await fetch(this.url, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        });
        if (!res.ok) {
            throw new Error(`RPC HTTP error ${res.status}`);
        }
        const json = (await res.json());
        if (json.error)
            throw new Error(`RPC error: ${json.error.message}`);
        if (!json.result)
            throw new Error("No result from getLatestLedger");
        return json.result.sequence;
    }
}
