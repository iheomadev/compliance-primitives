/**
 * Decodes raw Soroban event topics/data (base64-encoded XDR) into typed
 * compliance events.
 *
 * Soroban #[contractevent] structs with #[topic] fields are published as
 * a topic array where:
 *   topic[0]  = ScVal::Symbol — the event name (e.g. "AllowAdd")
 *   topic[1+] = the fields annotated #[topic] in declaration order
 *   data      = ScVal — struct-value encoding of any non-topic fields
 *
 * For the five event types in this repo:
 *
 *   AllowAdd        topics: [Symbol("AllowAdd"), Address]          data: Void
 *   AllowRemove     topics: [Symbol("AllowRemove"), Address]        data: Void
 *   Blocked         topics: [Symbol("Blocked"), Address, Address]   data: I128
 *   DenyAdd         topics: [Symbol("DenyAdd"), Address]            data: Void
 *   DenyRemove      topics: [Symbol("DenyRemove"), Address]         data: Void
 *   JurisdictionSet topics: [Symbol("JurisdictionSet"), Address]    data: String(code)
 *
 * We parse XDR manually using DataView — no external XDR lib — because the
 * values we need are simple enough and we want zero extra dependencies.
 * If you need full XDR fidelity, swap in @stellar/stellar-base.
 */
function base64ToBytes(b64) {
    const bin = atob(b64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++)
        bytes[i] = bin.charCodeAt(i);
    return bytes;
}
class XdrReader {
    view;
    pos;
    constructor(bytes) {
        this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        this.pos = 0;
    }
    readU32() {
        const v = this.view.getUint32(this.pos);
        this.pos += 4;
        return v;
    }
    readI32() {
        const v = this.view.getInt32(this.pos);
        this.pos += 4;
        return v;
    }
    readI64() {
        const hi = BigInt(this.view.getInt32(this.pos));
        const lo = BigInt(this.view.getUint32(this.pos + 4));
        this.pos += 8;
        return (hi << 32n) | lo;
    }
    readU64() {
        const hi = BigInt(this.view.getUint32(this.pos));
        const lo = BigInt(this.view.getUint32(this.pos + 4));
        this.pos += 8;
        return (hi << 32n) | lo;
    }
    readBytes(n) {
        const slice = new Uint8Array(this.view.buffer, this.view.byteOffset + this.pos, n);
        this.pos += n;
        // XDR pads to 4-byte boundary
        const pad = (4 - (n % 4)) % 4;
        this.pos += pad;
        return slice;
    }
    readVarBytes() {
        const len = this.readU32();
        return this.readBytes(len);
    }
    readString() {
        return new TextDecoder().decode(this.readVarBytes());
    }
}
function decodeScVal(r) {
    const disc = r.readU32();
    switch (disc) {
        case 1 /* ScType.Void */:
            return { type: "Void" };
        case 15 /* ScType.Symbol */: {
            const value = r.readString();
            return { type: "Symbol", value };
        }
        case 14 /* ScType.String */: {
            const value = r.readString();
            return { type: "String", value };
        }
        case 3 /* ScType.U32 */:
            return { type: "U32", value: r.readU32() };
        case 10 /* ScType.I128 */: {
            // hi (i64) then lo (u64)
            const hi = r.readI64();
            const lo = r.readU64();
            const value = (hi << 64n) | lo;
            return { type: "I128", value };
        }
        case 18 /* ScType.Address */: {
            const addrType = r.readU32();
            if (addrType === 0 /* ScAddressType.Account */) {
                // AccountID: PublicKey (discriminant u32 = 0) + 32 raw bytes
                r.readU32(); // PublicKey type discriminant (ED25519 = 0)
                const raw = r.readBytes(32);
                return { type: "Address", value: encodeG(raw) };
            }
            else {
                // Contract: 32-byte hash
                const raw = r.readBytes(32);
                return { type: "Address", value: bufToHex(raw) };
            }
        }
        case 16 /* ScType.Vec */: {
            // nullable union: discriminant 1 = Some, 0 = None
            const present = r.readU32();
            if (!present)
                return { type: "Vec", value: [] };
            const len = r.readU32();
            const items = [];
            for (let i = 0; i < len; i++)
                items.push(decodeScVal(r));
            return { type: "Vec", value: items };
        }
        default:
            return { type: "Other", discriminant: disc };
    }
}
// ─── Address encoding helpers ────────────────────────────────────────────────
/** Hex-encode bytes (used for contract addresses) */
function bufToHex(bytes) {
    return Array.from(bytes)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}
/**
 * Encode a 32-byte Ed25519 public key as a Stellar G-address (StrKey).
 * We do the full StrKey base32 + checksum here so there's no dep on
 * @stellar/stellar-base.
 */
function encodeG(raw) {
    // Version byte for account (G) = 6 << 3 = 48 (0x30)
    const versionByte = 6 << 3; // 0x30
    const payload = new Uint8Array(raw.length + 1);
    payload[0] = versionByte;
    payload.set(raw, 1);
    const checksum = crc16xmodem(payload);
    const full = new Uint8Array(payload.length + 2);
    full.set(payload);
    full[payload.length] = checksum & 0xff;
    full[payload.length + 1] = (checksum >> 8) & 0xff;
    return base32Encode(full);
}
function crc16xmodem(data) {
    let crc = 0x0000;
    for (const byte of data) {
        crc ^= byte << 8;
        for (let i = 0; i < 8; i++) {
            crc = crc & 0x8000 ? (crc << 1) ^ 0x1021 : crc << 1;
            crc &= 0xffff;
        }
    }
    return crc;
}
const BASE32_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
function base32Encode(data) {
    let bits = 0;
    let value = 0;
    let output = "";
    for (const byte of data) {
        value = (value << 8) | byte;
        bits += 8;
        while (bits >= 5) {
            output += BASE32_ALPHABET[(value >>> (bits - 5)) & 31];
            bits -= 5;
        }
    }
    if (bits > 0) {
        output += BASE32_ALPHABET[(value << (5 - bits)) & 31];
    }
    return output;
}
// ─── Event decoding ───────────────────────────────────────────────────────────
const KNOWN_EVENTS = new Set([
    "AllowAdd",
    "AllowRemove",
    "Blocked",
    "DenyAdd",
    "DenyRemove",
    "JurisdictionSet",
]);
export function decodeEvent(raw) {
    try {
        // topics is an array of base64 XDR ScVal strings
        if (!raw.topic || raw.topic.length < 2)
            return null;
        const topics = raw.topic.map((t) => decodeScVal(new XdrReader(base64ToBytes(t))));
        // topics[0] must be a Symbol naming the event
        const nameVal = topics[0];
        if (nameVal.type !== "Symbol")
            return null;
        const eventType = nameVal.value;
        if (!KNOWN_EVENTS.has(eventType))
            return null;
        // topics[1] is always the primary address
        const addrVal = topics[1];
        if (addrVal.type !== "Address")
            return null;
        const address = addrVal.value;
        let addressTo = null;
        let amount = null;
        let jurisdiction = null;
        // Decode data field
        const dataVal = raw.value
            ? decodeScVal(new XdrReader(base64ToBytes(raw.value)))
            : { type: "Void" };
        if (eventType === "Blocked") {
            // topics[2] = to address
            const toVal = topics[2];
            if (toVal?.type === "Address")
                addressTo = toVal.value;
            if (dataVal.type === "I128")
                amount = dataVal.value.toString();
        }
        if (eventType === "JurisdictionSet") {
            if (dataVal.type === "String")
                jurisdiction = dataVal.value;
        }
        const timestamp = raw.ledgerClosedAt
            ? Math.floor(new Date(raw.ledgerClosedAt).getTime() / 1000)
            : null;
        return {
            ledgerSequence: raw.ledger,
            timestamp,
            contractId: raw.contractId,
            eventType,
            address,
            addressTo,
            amount,
            jurisdiction,
            rawTopics: JSON.stringify(raw.topic),
            rawData: raw.value ?? "",
        };
    }
    catch (err) {
        console.error(`Failed to decode event from contract ${raw.contractId}:`, err);
        return null;
    }
}
