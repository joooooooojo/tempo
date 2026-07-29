/**
 * Portable HTML Structured Clone–style codec (plain ESM for Runtime bootstrap).
 * Envelope: { "$sca": "<base64>" }
 */

const TYPED_ARRAY_CTORS = {
  Int8Array,
  Uint8Array,
  Uint8ClampedArray,
  Int16Array,
  Uint16Array,
  Int32Array,
  Uint32Array,
  Float32Array,
  Float64Array,
  BigInt64Array,
  BigUint64Array,
};

function bytesToBase64(bytes) {
  return Buffer.from(bytes).toString("base64");
}

function base64ToBytes(b64) {
  return new Uint8Array(Buffer.from(b64, "base64"));
}

function isPlainObject(value) {
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

function reject(value) {
  const kind =
    value === null
      ? "null"
      : typeof value === "object"
        ? value.constructor?.name ?? "Object"
        : typeof value;
  throw new Error(`Failed to serialize arguments: ${kind} is not structured-cloneable`);
}

function encodeValue(value, seen, out) {
  if (value === undefined) return { t: "u" };
  if (value === null) return null;
  const ty = typeof value;
  if (ty === "boolean" || ty === "string") return value;
  if (ty === "number") {
    if (Number.isNaN(value) || !Number.isFinite(value)) return { t: "num", v: value };
    return value;
  }
  if (ty === "bigint") return { t: "bi", v: value.toString() };
  if (ty === "symbol" || ty === "function") reject(value);
  if (ty !== "object") reject(value);

  if (seen.has(value)) return { t: "r", i: seen.get(value) };
  if (value instanceof Promise || value instanceof WeakMap || value instanceof WeakSet) reject(value);

  if (value instanceof Date) {
    const i = out.length;
    seen.set(value, i);
    out.push({ t: "date", v: value.getTime() });
    return { t: "r", i };
  }

  if (value instanceof ArrayBuffer) {
    const i = out.length;
    seen.set(value, i);
    out.push({ t: "ab", v: bytesToBase64(new Uint8Array(value)) });
    return { t: "r", i };
  }

  if (ArrayBuffer.isView(value)) {
    const i = out.length;
    seen.set(value, i);
    const bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    if (value instanceof DataView) {
      out.push({ t: "dv", v: bytesToBase64(bytes), o: 0, l: value.byteLength });
      return { t: "r", i };
    }
    const name = value.constructor.name;
    if (!TYPED_ARRAY_CTORS[name]) reject(value);
    out.push({ t: "ta", n: name, v: bytesToBase64(bytes) });
    return { t: "r", i };
  }

  if (value instanceof Map) {
    const i = out.length;
    seen.set(value, i);
    const ir = { t: "m", v: [] };
    out.push(ir);
    for (const [k, v] of value) ir.v.push([encodeValue(k, seen, out), encodeValue(v, seen, out)]);
    return { t: "r", i };
  }

  if (value instanceof Set) {
    const i = out.length;
    seen.set(value, i);
    const ir = { t: "s", v: [] };
    out.push(ir);
    for (const item of value) ir.v.push(encodeValue(item, seen, out));
    return { t: "r", i };
  }

  if (value instanceof Error) {
    const i = out.length;
    seen.set(value, i);
    out.push({
      t: "e",
      n: value.name || "Error",
      m: value.message || "",
      c: value.cause !== undefined ? encodeValue(value.cause, seen, out) : undefined,
    });
    return { t: "r", i };
  }

  if (Array.isArray(value)) {
    const i = out.length;
    seen.set(value, i);
    const ir = { t: "a", v: [] };
    out.push(ir);
    for (const item of value) ir.v.push(encodeValue(item, seen, out));
    return { t: "r", i };
  }

  if (!isPlainObject(value)) reject(value);

  const i = out.length;
  seen.set(value, i);
  const ir = { t: "o", v: [] };
  out.push(ir);
  for (const key of Object.keys(value)) {
    ir.v.push([key, encodeValue(value[key], seen, out)]);
  }
  return { t: "r", i };
}

function decodeIr(ir, table) {
  if (ir === null || typeof ir === "boolean" || typeof ir === "number" || typeof ir === "string") {
    return ir;
  }
  if (typeof ir !== "object" || !("t" in ir)) reject(ir);
  switch (ir.t) {
    case "u":
      return undefined;
    case "bi":
      return BigInt(ir.v);
    case "num":
      return Number(ir.v);
    case "r":
      if (ir.i < 0 || ir.i >= table.length) throw new Error("Failed to deserialize: invalid ref");
      return table[ir.i];
    default:
      throw new Error(`Failed to deserialize: unexpected tag ${ir.t}`);
  }
}

function materializeSlot(slot, table, index) {
  if (slot === null || typeof slot !== "object" || !("t" in slot)) {
    table[index] = slot;
    return;
  }
  switch (slot.t) {
    case "date":
      table[index] = new Date(slot.v);
      return;
    case "ab":
      table[index] = base64ToBytes(slot.v).buffer;
      return;
    case "ta": {
      const Ctor = TYPED_ARRAY_CTORS[slot.n];
      if (!Ctor) throw new Error(`Failed to deserialize: unknown TypedArray ${slot.n}`);
      const bytes = base64ToBytes(slot.v);
      table[index] = new Ctor(
        bytes.buffer,
        bytes.byteOffset,
        bytes.byteLength / Ctor.BYTES_PER_ELEMENT,
      );
      return;
    }
    case "dv": {
      const bytes = base64ToBytes(slot.v);
      table[index] = new DataView(bytes.buffer, bytes.byteOffset, slot.l);
      return;
    }
    case "a":
      table[index] = new Array(slot.v.length);
      return;
    case "o":
      table[index] = {};
      return;
    case "m":
      table[index] = new Map();
      return;
    case "s":
      table[index] = new Set();
      return;
    case "e": {
      const err = new Error(slot.m);
      err.name = slot.n;
      table[index] = err;
      return;
    }
    default:
      throw new Error("Failed to deserialize: bad slot tag");
  }
}

function fillSlot(slot, table, index) {
  if (slot === null || typeof slot !== "object" || !("t" in slot)) return;
  switch (slot.t) {
    case "a": {
      const arr = table[index];
      for (let i = 0; i < slot.v.length; i += 1) arr[i] = decodeIr(slot.v[i], table);
      return;
    }
    case "o": {
      const obj = table[index];
      for (const [k, v] of slot.v) obj[k] = decodeIr(v, table);
      return;
    }
    case "m": {
      const map = table[index];
      for (const [k, v] of slot.v) map.set(decodeIr(k, table), decodeIr(v, table));
      return;
    }
    case "s": {
      const set = table[index];
      for (const item of slot.v) set.add(decodeIr(item, table));
      return;
    }
    case "e": {
      if (slot.c !== undefined) table[index].cause = decodeIr(slot.c, table);
      return;
    }
    default:
      return;
  }
}

export function scaEncode(value) {
  const seen = new Map();
  const table = [];
  const root = encodeValue(value, seen, table);
  const bytes = Buffer.from(JSON.stringify({ root, table }), "utf8");
  return { $sca: bytes.toString("base64") };
}

export function scaDecode(envelope) {
  if (!envelope || typeof envelope.$sca !== "string") {
    throw new Error("Failed to deserialize: missing $sca envelope");
  }
  const payload = JSON.parse(Buffer.from(envelope.$sca, "base64").toString("utf8"));
  const table = new Array(payload.table.length);
  for (let i = 0; i < payload.table.length; i += 1) materializeSlot(payload.table[i], table, i);
  for (let i = 0; i < payload.table.length; i += 1) fillSlot(payload.table[i], table, i);
  return decodeIr(payload.root, table);
}

export function isScaEnvelope(value) {
  return (
    !!value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    typeof value.$sca === "string" &&
    Object.keys(value).length === 1
  );
}

export function scaEncodeArgs(args) {
  return scaEncode(args);
}

export function scaDecodeArgs(envelope) {
  const value = scaDecode(envelope);
  return Array.isArray(value) ? value : [value];
}
