"use client";

const ANSI_REGEX = /\u001b\[[0-9;]*m/g;

type AnsiStyle = {
  color?: string;
  backgroundColor?: string;
  fontWeight?: string;
};

const BASIC_COLORS = [
  "#000000",
  "#800000",
  "#008000",
  "#808000",
  "#000080",
  "#800080",
  "#008080",
  "#c0c0c0",
];

const BRIGHT_COLORS = [
  "#808080",
  "#ff0000",
  "#00ff00",
  "#ffff00",
  "#0000ff",
  "#ff00ff",
  "#00ffff",
  "#ffffff",
];

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function ansi256ToRgb(code: number): string | undefined {
  if (code < 0 || code > 255) return undefined;
  if (code < 8) return BASIC_COLORS[code];
  if (code < 16) return BRIGHT_COLORS[code - 8];
  if (code < 232) {
    const idx = code - 16;
    const r = Math.floor(idx / 36);
    const g = Math.floor((idx % 36) / 6);
    const b = idx % 6;
    const toChannel = (v: number) => (v === 0 ? 0 : 55 + v * 40);
    return `rgb(${toChannel(r)}, ${toChannel(g)}, ${toChannel(b)})`;
  }
  const gray = 8 + (code - 232) * 10;
  return `rgb(${gray}, ${gray}, ${gray})`;
}

function styleToCss(style: AnsiStyle): string {
  const parts: string[] = [];
  if (style.color) parts.push(`color: ${style.color}`);
  if (style.backgroundColor) parts.push(`background-color: ${style.backgroundColor}`);
  if (style.fontWeight) parts.push(`font-weight: ${style.fontWeight}`);
  return parts.join("; ");
}

function hasStyle(style: AnsiStyle): boolean {
  return !!(style.color || style.backgroundColor || style.fontWeight);
}

function applyCodes(style: AnsiStyle, codes: number[]): AnsiStyle {
  const next: AnsiStyle = { ...style };
  let idx = 0;
  while (idx < codes.length) {
    const code = codes[idx];
    if (code === 0) {
      next.color = undefined;
      next.backgroundColor = undefined;
      next.fontWeight = undefined;
      idx += 1;
      continue;
    }
    if (code === 1) {
      next.fontWeight = "bold";
      idx += 1;
      continue;
    }
    if (code === 22) {
      next.fontWeight = undefined;
      idx += 1;
      continue;
    }
    if (code >= 30 && code <= 37) {
      next.color = BASIC_COLORS[code - 30];
      idx += 1;
      continue;
    }
    if (code >= 90 && code <= 97) {
      next.color = BRIGHT_COLORS[code - 90];
      idx += 1;
      continue;
    }
    if (code === 39) {
      next.color = undefined;
      idx += 1;
      continue;
    }
    if (code >= 40 && code <= 47) {
      next.backgroundColor = BASIC_COLORS[code - 40];
      idx += 1;
      continue;
    }
    if (code >= 100 && code <= 107) {
      next.backgroundColor = BRIGHT_COLORS[code - 100];
      idx += 1;
      continue;
    }
    if (code === 49) {
      next.backgroundColor = undefined;
      idx += 1;
      continue;
    }
    if ((code === 38 || code === 48) && codes[idx + 1] === 5) {
      const colorCode = codes[idx + 2];
      const color = ansi256ToRgb(colorCode);
      if (color) {
        if (code === 38) {
          next.color = color;
        } else {
          next.backgroundColor = color;
        }
      }
      idx += 3;
      continue;
    }
    idx += 1;
  }
  return next;
}

export function ansiToHtml(input: string): string {
  if (!input) return "";
  const parts: string[] = [];
  let style: AnsiStyle = {};
  let openSpan = false;
  let lastIndex = 0;
  const matches = input.matchAll(ANSI_REGEX);

  for (const match of matches) {
    const matchIndex = match.index ?? 0;
    const text = input.slice(lastIndex, matchIndex);
    if (text) {
      parts.push(escapeHtml(text));
    }

    const codeText = match[0].slice(2, -1);
    const codes = codeText
      ? codeText.split(";").map((v) => Number.parseInt(v, 10)).filter((v) => !Number.isNaN(v))
      : [0];
    style = applyCodes(style, codes);

    if (openSpan) {
      parts.push("</span>");
      openSpan = false;
    }
    if (hasStyle(style)) {
      parts.push(`<span style="${styleToCss(style)}">`);
      openSpan = true;
    }
    lastIndex = matchIndex + match[0].length;
  }

  const tail = input.slice(lastIndex);
  if (tail) {
    parts.push(escapeHtml(tail));
  }

  if (openSpan) {
    parts.push("</span>");
  }

  return parts.join("");
}

export function stripLogPrefix(input: string): string {
  if (!input) return "";
  return input.replace(
    /^\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\s+\[[a-zA-Z]+\]\s+/,
    ""
  );
}
