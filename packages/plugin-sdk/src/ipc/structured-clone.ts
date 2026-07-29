/**
 * Portable HTML Structured Clone–style codec for UI ↔ Runtime IPC.
 * Wire envelope: { "$sca": "<base64 of UTF-8 JSON IR>" }
 *
 * Supports: primitives, plain objects/arrays, Date, Map, Set, ArrayBuffer,
 * TypedArray, DataView, cyclic refs, Error (name/message).
 * Rejects: Function, Promise, Symbol, WeakMap/WeakSet, DOM / host objects.
 */

export type ScaEnvelope = { $sca: string };

const TYPED_ARRAY_CTORS: Record<string, new (buffer: ArrayBufferLike, byteOffset?: number, length?: number) => ArrayBufferView> = {
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

function bytesToBase64(bytes: Uint8Array): string {
  if (typeof Buffer !== "undefined") {
    return Buffer.from(bytes).toString("base64");
  }
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]!);
  return btoa(binary);
}

function base64ToBytes(b64: string): Uint8Array {
  if (typeof Buffer !== "undefined") {
    return new Uint8Array(Buffer.from(b64, "base64"));
  }
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

function utf8Encode(text: string): Uint8Array {
  if (typeof TextEncoder !== "undefined") return new TextEncoder().encode(text);
  return Uint8Array.from(Buffer.from(text, "utf8"));
}

function utf8Decode(bytes: Uint8Array): string {
  if (typeof TextDecoder !== "undefined") return new TextDecoder().decode(bytes);
  return Buffer.from(bytes).toString("utf8");
}

function isPlainObject(value: object): boolean {
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

function reject(value: unknown): never {
  const kind =
    value === null
      ? "null"
      : typeof value === "object"
        ? (value as object).constructor?.name ?? "Object"
        : typeof value;
  throw new Error(`Failed to serialize arguments: ${kind} is not structured-cloneable`);
}

type Ir =
  | null
  | boolean
  | number
  | string
  | { t: "u" }
  | { t: "bi"; v: string }
  | { t: "num"; v: number }
  | { t: "date"; v: number }
  | { t: "r"; i: number }
  | { t: "a"; v: Ir[] }
  | { t: "o"; v: Array<[string, Ir]> }
  | { t: "m"; v: Array<[Ir, Ir]> }
  | { t: "s"; v: Ir[] }
  | { t: "ab"; v: string }
  | { t: "ta"; n: string; v: string; o?: number; l?: number }
  | { t: "dv"; v: string; o: number; l: number }
  | { t: "e"; n: string; m: string; c?: Ir };

function encodeValue(value: unknown, seen: Map<object, number>, out: Ir[]): Ir {
  if (value === undefined) return { t: "u" };
  if (value === null) return null;
  const ty = typeof value;
  if (ty === "boolean" || ty === "string") return value as boolean | string;
  if (ty === "number") {
    if (Number.isNaN(value as number) || !Number.isFinite(value as number)) {
      return { t: "num", v: value as number };
    }
    return value as number;
  }
  if (ty === "bigint") return { t: "bi", v: (value as bigint).toString() };
  if (ty === "symbol" || ty === "function") reject(value);
  if (ty !== "object") reject(value);

  const obj = value as object;
  if (seen.has(obj)) return { t: "r", i: seen.get(obj)! };

  if (typeof Promise !== "undefined" && value instanceof Promise) reject(value);
  if (typeof WeakMap !== "undefined" && value instanceof WeakMap) reject(value);
  if (typeof WeakSet !== "undefined" && value instanceof WeakSet) reject(value);

  // DOM / host platform objects typically have nodeType or fail plain-object checks later.
  if (typeof Element !== "undefined" && value instanceof Element) reject(value);
  if (typeof Node !== "undefined" && value instanceof Node) reject(value);

  if (value instanceof Date) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = { t: "date", v: value.getTime() };
    out.push(ir);
    return { t: "r", i };
  }

  if (value instanceof ArrayBuffer) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = { t: "ab", v: bytesToBase64(new Uint8Array(value)) };
    out.push(ir);
    return { t: "r", i };
  }

  if (ArrayBuffer.isView(value)) {
    const i = out.length;
    seen.set(obj, i);
    const view = value as ArrayBufferView;
    const bytes = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    if (typeof DataView !== "undefined" && value instanceof DataView) {
      const ir: Ir = {
        t: "dv",
        v: bytesToBase64(bytes),
        o: 0,
        l: view.byteLength,
      };
      out.push(ir);
      return { t: "r", i };
    }
    const name = (value as { constructor: { name: string } }).constructor.name;
    if (!TYPED_ARRAY_CTORS[name]) reject(value);
    const ir: Ir = { t: "ta", n: name, v: bytesToBase64(bytes) };
    out.push(ir);
    return { t: "r", i };
  }

  if (value instanceof Map) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = { t: "m", v: [] };
    out.push(ir);
    for (const [k, v] of value) {
      ir.v.push([encodeValue(k, seen, out), encodeValue(v, seen, out)]);
    }
    return { t: "r", i };
  }

  if (value instanceof Set) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = { t: "s", v: [] };
    out.push(ir);
    for (const item of value) {
      ir.v.push(encodeValue(item, seen, out));
    }
    return { t: "r", i };
  }

  if (value instanceof Error) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = {
      t: "e",
      n: value.name || "Error",
      m: value.message || "",
      c: value.cause !== undefined ? encodeValue(value.cause, seen, out) : undefined,
    };
    out.push(ir);
    return { t: "r", i };
  }

  if (Array.isArray(value)) {
    const i = out.length;
    seen.set(obj, i);
    const ir: Ir = { t: "a", v: [] };
    out.push(ir);
    for (const item of value) {
      ir.v.push(encodeValue(item, seen, out));
    }
    return { t: "r", i };
  }

  if (!isPlainObject(obj)) reject(value);

  const i = out.length;
  seen.set(obj, i);
  const ir: Ir = { t: "o", v: [] };
  out.push(ir);
  for (const key of Object.keys(value as Record<string, unknown>)) {
    ir.v.push([key, encodeValue((value as Record<string, unknown>)[key], seen, out)]);
  }
  return { t: "r", i };
}

function decodeIr(ir: Ir, table: unknown[]): unknown {
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
    case "r": {
      if (ir.i < 0 || ir.i >= table.length) {
        throw new Error("Failed to deserialize: invalid ref");
      }
      return table[ir.i];
    }
    default:
      throw new Error(`Failed to deserialize: unexpected tag ${String((ir as { t: string }).t)}`);
  }
}

function materializeSlot(slot: Ir, table: unknown[], index: number): void {
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
      table[index] = new Ctor(bytes.buffer, bytes.byteOffset, bytes.byteLength / (Ctor as unknown as { BYTES_PER_ELEMENT: number }).BYTES_PER_ELEMENT);
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
      throw new Error(`Failed to deserialize: bad slot tag`);
  }
}

function fillSlot(slot: Ir, table: unknown[], index: number): void {
  if (slot === null || typeof slot !== "object" || !("t" in slot)) return;
  switch (slot.t) {
    case "a": {
      const arr = table[index] as unknown[];
      for (let i = 0; i < slot.v.length; i += 1) {
        arr[i] = decodeIr(slot.v[i]!, table);
      }
      return;
    }
    case "o": {
      const obj = table[index] as Record<string, unknown>;
      for (const [k, v] of slot.v) {
        obj[k] = decodeIr(v, table);
      }
      return;
    }
    case "m": {
      const map = table[index] as Map<unknown, unknown>;
      for (const [k, v] of slot.v) {
        map.set(decodeIr(k, table), decodeIr(v, table));
      }
      return;
    }
    case "s": {
      const set = table[index] as Set<unknown>;
      for (const item of slot.v) {
        set.add(decodeIr(item, table));
      }
      return;
    }
    case "e": {
      const err = table[index] as Error & { cause?: unknown };
      if (slot.c !== undefined) err.cause = decodeIr(slot.c, table);
      return;
    }
    default:
      return;
  }
}

/** Serialize a value into a Host-opaque SCA envelope. */
export function scaEncode(value: unknown): ScaEnvelope {
  const seen = new Map<object, number>();
  const table: Ir[] = [];
  const root = encodeValue(value, seen, table);
  const payload = { root, table };
  const bytes = utf8Encode(JSON.stringify(payload));
  return { $sca: bytesToBase64(bytes) };
}

/** Decode a Host-opaque SCA envelope back to a JS value. */
export function scaDecode(envelope: ScaEnvelope): unknown {
  if (!envelope || typeof envelope.$sca !== "string") {
    throw new Error("Failed to deserialize: missing $sca envelope");
  }
  const payload = JSON.parse(utf8Decode(base64ToBytes(envelope.$sca))) as {
    root: Ir;
    table: Ir[];
  };
  const table: unknown[] = new Array(payload.table.length);
  for (let i = 0; i < payload.table.length; i += 1) {
    materializeSlot(payload.table[i]!, table, i);
  }
  for (let i = 0; i < payload.table.length; i += 1) {
    fillSlot(payload.table[i]!, table, i);
  }
  return decodeIr(payload.root, table);
}

export function isScaEnvelope(value: unknown): value is ScaEnvelope {
  return (
    !!value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    typeof (value as ScaEnvelope).$sca === "string" &&
    Object.keys(value as object).length === 1
  );
}

/** Encode invoke args array for the wire. */
export function scaEncodeArgs(args: unknown[]): ScaEnvelope {
  return scaEncode(args);
}

/** Decode invoke args array from the wire. */
export function scaDecodeArgs(envelope: ScaEnvelope): unknown[] {
  const value = scaDecode(envelope);
  return Array.isArray(value) ? value : [value];
}
