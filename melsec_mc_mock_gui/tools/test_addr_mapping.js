// Quick test script to verify parseTarget and display address formatting
function parseTarget(s) {
  if (!s) return null;
  const up = s.toUpperCase().trim();
  let i = 0; while (i < up.length && /[A-Z]/.test(up[i])) i++;
  if (i === 0) return null;
  const key = up.slice(0, i);
  const numPart = up.slice(i).trim();
  if (!numPart) return null;
  const HEX_BASE_SYMBOLS = ['X','Y','B','W','SB','SW','DX','DY'];
  const DEC_BASE_SYMBOLS = ['M'];
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

function formatDisplayAddr(key, wordAddr) {
  const HEX_BASE_SYMBOLS = ['X','Y','B','W','SB','SW','DX','DY'];
  const DEC_BASE_SYMBOLS = ['M'];
  const keyUpper = (key || '').toUpperCase();
  if (HEX_BASE_SYMBOLS.includes(keyUpper)) {
    return `${key}${(wordAddr * 0x10).toString(16).toUpperCase()}`;
  } else if (DEC_BASE_SYMBOLS.includes(keyUpper)) {
    return `${key}${wordAddr * 10}`;
  }
  return `${key}${wordAddr}`;
}

function sampleRows(target, rows=5) {
  const p = parseTarget(target);
  console.log(`\nInput: ${target} -> parsed word index: ${p ? p.addr : '<null>'}`);
  if (!p) return;
  for (let i=0;i<rows;i++) {
    const wa = p.addr + i;
    console.log(` row ${i+1}: ${formatDisplayAddr(p.key, wa)} (word index ${wa})`);
  }
}

console.log('=== Address mapping tests ===');
['X0','X1','X2','X10','XF','X11','M0','M1','M5','M10','D0','D1'].forEach(t => sampleRows(t, 6));

console.log('\nEdge cases:');
['x0','xF','m9','m11','W0','W1','B10','DX1','DY20'].forEach(t => sampleRows(t,4));
