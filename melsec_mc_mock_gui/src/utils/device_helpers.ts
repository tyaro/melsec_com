// Helper utilities for device address parsing and display formatting
export const HEX_BASE_SYMBOLS = ['X','Y','B','W','SB','SW','DX','DY'];
export const DEC_BASE_SYMBOLS = ['M'];

export function getBitConfigForKey(key: string | null) {
  const keyUpper = (key || '').toUpperCase();
  if (HEX_BASE_SYMBOLS.includes(keyUpper)) return { bits: 16, labelBase: 'hex' };
  if (DEC_BASE_SYMBOLS.includes(keyUpper)) return { bits: 10, labelBase: 'dec' };
  return { bits: 16, labelBase: 'hex' };
}

export function formatDisplayAddr(key: string, wordAddr: number) {
  const keyUpper = (key || '').toUpperCase();
  if (HEX_BASE_SYMBOLS.includes(keyUpper)) {
    return `${key}${(wordAddr * 0x10).toString(16).toUpperCase()}`;
  } else if (DEC_BASE_SYMBOLS.includes(keyUpper)) {
    return `${key}${wordAddr * 10}`;
  }
  return `${key}${wordAddr}`;
}

export function parseTarget(s: string | null) {
  if (!s) return null;
  const up = s.toUpperCase().trim();
  let i = 0; while (i < up.length && /[A-Z]/.test(up[i])) i++;
  if (i === 0) return null;
  let key = up.slice(0, i);
  let numPart = up.slice(i).trim();
  if (!numPart && key.length > 1) {
    numPart = key.slice(1);
    key = key[0];
  }
  if (!numPart) return null;
  const has0x = /^0x/i.test(numPart);
  const containsHexLetters = /[A-F]/i.test(numPart);
  const keyUpper = key.toUpperCase();
  const isHex = has0x || containsHexLetters || HEX_BASE_SYMBOLS.includes(keyUpper);
  const isDecAligned = DEC_BASE_SYMBOLS.includes(keyUpper);
  const rawAddr = isHex ? (has0x ? parseInt(numPart.substring(2), 16) : parseInt(numPart, 16)) : parseInt(numPart, 10);
  if (Number.isNaN(rawAddr)) return null;
  let wordIndex = rawAddr;
  if (isHex) wordIndex = Math.floor(rawAddr / 0x10);
  else if (isDecAligned) wordIndex = Math.floor(rawAddr / 10);
  return { key, addr: wordIndex };
}
