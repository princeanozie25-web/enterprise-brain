// A minimal, dependency-free extractor for the evidence PDF's footer text.
//
// Scope: the PDFs the AP-5 export endpoint produces (printpdf 0.3.4 via
// genpdf): Identity-H CID fonts named /F0../Fn with plaintext ToUnicode
// CMaps, content streams optionally Flate-compressed, text emitted as hex
// CID strings in Tj / TJ operators. This is NOT a general PDF reader; it
// reads exactly that shape, fails loudly otherwise, and exists so the
// EXPORT-FOOTER card can render the literal bytes the real PDF carries
// (the Rust AE-suite separately proves extraction fidelity with
// pdf-extract; this is the capture pipeline's own reader).

import { inflateSync } from "node:zlib";

interface FontMaps {
  /** font tag (e.g. "F2") -> glyph id -> string */
  byTag: Map<string, Map<number, string>>;
}

function parseObjects(bytes: Buffer): Map<number, Buffer> {
  const objects = new Map<number, Buffer>();
  const text = bytes.toString("latin1");
  const re = /(\d+) 0 obj/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    const start = match.index + match[0].length;
    const end = text.indexOf("endobj", start);
    if (end < 0) {
      continue;
    }
    objects.set(Number(match[1]), bytes.subarray(start, end));
    re.lastIndex = end;
  }
  return objects;
}

function streamPayload(object: Buffer): Buffer | null {
  const text = object.toString("latin1");
  const at = text.indexOf("stream");
  if (at < 0) {
    return null;
  }
  let start = at + "stream".length;
  if (text[start] === "\r") {
    start++;
  }
  if (text[start] === "\n") {
    start++;
  }
  const end = text.lastIndexOf("endstream");
  if (end < 0) {
    return null;
  }
  const raw = object.subarray(start, end);
  if (/\/Filter\s*\/FlateDecode/.test(text.slice(0, at))) {
    try {
      return inflateSync(raw);
    } catch {
      return null;
    }
  }
  return raw;
}

function parseCmap(payload: string): Map<number, string> {
  const map = new Map<number, string>();
  const hexToString = (hex: string): string => {
    let out = "";
    for (let i = 0; i + 4 <= hex.length; i += 4) {
      out += String.fromCharCode(parseInt(hex.slice(i, i + 4), 16));
    }
    return out;
  };
  for (const block of payload.matchAll(/beginbfchar([\s\S]*?)endbfchar/g)) {
    for (const pair of block[1].matchAll(/<([0-9a-fA-F]+)>\s*<([0-9a-fA-F]+)>/g)) {
      map.set(parseInt(pair[1], 16), hexToString(pair[2]));
    }
  }
  for (const block of payload.matchAll(/beginbfrange([\s\S]*?)endbfrange/g)) {
    for (const triple of block[1].matchAll(
      /<([0-9a-fA-F]+)>\s*<([0-9a-fA-F]+)>\s*<([0-9a-fA-F]+)>/g,
    )) {
      const lo = parseInt(triple[1], 16);
      const hi = parseInt(triple[2], 16);
      const base = parseInt(triple[3], 16);
      for (let g = lo; g <= hi && g - lo < 65536; g++) {
        map.set(g, String.fromCharCode(base + (g - lo)));
      }
    }
  }
  return map;
}

function collectFonts(objects: Map<number, Buffer>): FontMaps {
  const byTag = new Map<number, string>(); // ToUnicode obj -> tag
  for (const body of objects.values()) {
    const text = body.toString("latin1");
    if (!text.includes("/Type/Font") && !text.includes("/Type /Font")) {
      continue;
    }
    const tag = text.match(/\/BaseFont\s*\/(F\d+)/);
    const toUnicode = text.match(/\/ToUnicode\s+(\d+)\s+0\s+R/);
    if (tag && toUnicode) {
      byTag.set(Number(toUnicode[1]), tag[1]);
    }
  }
  const maps: FontMaps = { byTag: new Map() };
  for (const [objNum, tag] of byTag) {
    const object = objects.get(objNum);
    if (!object) {
      continue;
    }
    const payload = streamPayload(object) ?? object;
    maps.byTag.set(tag, parseCmap(payload.toString("latin1")));
  }
  return maps;
}

function decodeContent(content: string, fonts: FontMaps): string {
  let out = "";
  let current: Map<number, string> | undefined;
  // Tokens we care about: /Fk ... Tf (font switch), <hex> (CID string),
  // Tj/TJ (show), Td/TD/T*/ET (line-ish breaks).
  const re = /\/(F\d+)\s+[\d.]+\s+Tf|<([0-9a-fA-F]+)>|(\bT[dD*]\b|\bET\b)/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(content)) !== null) {
    if (match[1]) {
      current = fonts.byTag.get(match[1]);
    } else if (match[2]) {
      const hex = match[2];
      for (let i = 0; i + 4 <= hex.length; i += 4) {
        const glyph = parseInt(hex.slice(i, i + 4), 16);
        out += current?.get(glyph) ?? "";
      }
    } else if (match[3]) {
      out += "\n";
    }
  }
  return out;
}

/** Extracts all decodable text from an AP-5 evidence PDF. */
export function extractPdfText(bytes: Buffer): string {
  const objects = parseObjects(bytes);
  const fonts = collectFonts(objects);
  if (fonts.byTag.size === 0) {
    throw new Error("no Identity-H fonts with ToUnicode maps found — not an AP-5 export?");
  }
  let text = "";
  for (const body of objects.values()) {
    const header = body.toString("latin1");
    if (!header.includes("stream")) {
      continue;
    }
    const payload = streamPayload(body);
    if (!payload) {
      continue;
    }
    const content = payload.toString("latin1");
    if (!content.includes("Tf") || !(content.includes("Tj") || content.includes("TJ"))) {
      continue;
    }
    text += decodeContent(content, fonts);
  }
  return text;
}

const FOOTER_PREFIXES = [
  "actor:",
  "snapshot:",
  "index:",
  "content sha256:",
  "audit ordinals:",
  "page ",
] as const;

/** The first page's footer block, lines verbatim, joined with newlines. */
export function extractFooterBlock(text: string): string {
  const lines = text.split("\n").map((l) => l.trim());
  const found: string[] = [];
  for (const prefix of FOOTER_PREFIXES) {
    const line = lines.find((l) => l.startsWith(prefix));
    if (line) {
      found.push(line);
    }
  }
  if (found.length < 5) {
    throw new Error(
      `footer extraction found only ${found.length} of ${FOOTER_PREFIXES.length} expected lines`,
    );
  }
  return found.join("\n");
}
