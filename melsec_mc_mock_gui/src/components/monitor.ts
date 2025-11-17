declare const window: any;
declare global { interface Window { __TAURI__?: any } }
// safe invoke wrapper: in Tauri runtime window.__TAURI__.core.invoke exists;
// in tests it may be undefined, so provide a stub that throws to make failures explicit.
const invoke: (...args: any[]) => Promise<any> = (window && (window as any).__TAURI__ && (window as any).__TAURI__.core && (window as any).__TAURI__.core.invoke)
  ? (window as any).__TAURI__.core.invoke.bind((window as any).__TAURI__.core)
  : async () => { throw new Error('Tauri invoke not available in test environment'); };

export const latestWords: { [k:string]: number } = {};
let currentFormatInternal = 'U16';

// Currently selected monitor target; when non-null, incoming `monitor` events
// or polling results for other targets will be ignored.
export let currentMonitorTarget: { key: string; addr: number } | null = null;

import { getBitConfigForKey, formatDisplayAddr, parseTarget } from '../utils/device_helpers';
export { parseTarget } from '../utils/device_helpers';

export function setCurrentMonitorTarget(key: string | null, addr?: number) {
  if (!key) {
    currentMonitorTarget = null;
    updateBitHeadersForKey(null);
  } else {
    currentMonitorTarget = { key, addr: addr || 0 };
    updateBitHeadersForKey(key);
  }
}

// Debug helper: call `__monitorDebugDump()` in DevTools console to inspect rows.
try {
  (window as any).__monitorDebugDump = () => {
    try {
      const tbody = document.getElementById('monitor-tbody');
      const rows = tbody ? Array.from(tbody.querySelectorAll('tr')) as HTMLTableRowElement[] : [];
      const addrs = rows.map(r => {
        const parts = r.id.split('-');
        return parseInt(parts[parts.length - 1] || '0', 10);
      }).filter(n => !Number.isNaN(n)).sort((a,b) => a - b);
      const min = addrs.length ? addrs[0] : null;
      const max = addrs.length ? addrs[addrs.length - 1] : null;
      const missing: number[] = [];
      if (min !== null && max !== null) {
        const set = new Set(addrs);
        for (let i = min; i <= max; i++) if (!set.has(i)) missing.push(i);
      }
      console.log('monitorDebugDump', { currentRangeStart, currentRangeEnd, renderedRows: addrs.length, min, max, missingCount: missing.length, missingSample: missing.slice(0,50) });
      try { uiLog(`monitorDebugDump rows=${addrs.length} range=${min}-${max} missing=${missing.length}`); } catch (e) {}
      return { currentRangeStart, currentRangeEnd, renderedRows: addrs.length, min, max, missing };
    } catch (e) { console.error('monitorDebugDump failed', e); return null; }
  };
} catch (e) {}

// Update bit headers according to device family. Some devices (X/Y/B/W etc) are
// 16-bit aligned and display 16 columns (F..0). Others like M are decade coils
// and should display 10 columns (9..0).
function updateBitHeadersForKey(key: string | null) {
  const thead = document.querySelector('#monitor-table thead tr');
  if (!thead) return;
  // remove existing bit header cells (keep first and last two columns)
  // structure: TH(Device) [bit headers...] TH(Display) TH(RAW)
  // rebuild bit headers between the first TH and the last two THs
  // clear all children then reconstruct to avoid fragile DOM ops
  const displayTh = document.createElement('th'); displayTh.textContent = '表示';
  const rawTh = document.createElement('th'); rawTh.textContent = 'RAW';
  // determine bit labels
  const bitCfg = getBitConfigForKey(key);
  let bitLabels: string[] = [];
  if (bitCfg.bits === 16) {
    bitLabels = ['F','E','D','C','B','A','9','8','7','6','5','4','3','2','1','0'];
  } else {
    bitLabels = ['9','8','7','6','5','4','3','2','1','0'];
  }
  
  // rebuild header
  while (thead.firstChild) thead.removeChild(thead.firstChild);
  const thDevice = document.createElement('th'); thDevice.textContent = 'デバイス'; thead.appendChild(thDevice);
  for (const lb of bitLabels) {
    const th = document.createElement('th'); th.className = 'bit-header'; th.textContent = lb; thead.appendChild(th);
  }
  thead.appendChild(displayTh);
  thead.appendChild(rawTh);
}

// getBitConfigForKey provided by shared helper

export function getCurrentFormat() { return currentFormatInternal; }
export function setCurrentFormat(fmt: string) { currentFormatInternal = fmt; refreshAllRows(); try { if (window.localStorage) window.localStorage.setItem('displayFormat', currentFormatInternal); } catch(e) {} }

export function refreshAllRows() {
  for (const k in latestWords) {
    const [key, addrStr] = k.split(':');
    const addr = parseInt(addrStr, 10);
    const w = latestWords[k];
    renderRowForWord(key, addr, w);
  }
  if (['U32','I32','F32'].includes(currentFormatInternal)) {
    for (const k in latestWords) {
      const [key, addrStr] = k.split(':');
      const addr = parseInt(addrStr, 10);
      if (addr % 2 === 0) {
        const trOdd = document.getElementById(`row-${key}-${addr+1}`);
        if (trOdd) trOdd.classList.add('paired-empty');
      }
    }
  } else {
    document.querySelectorAll('#monitor-tbody tr.paired-empty').forEach(r => r.classList.remove('paired-empty'));
  }
}

function uiLog(msg: string) {
  try {
    const out = document.getElementById('monitor-log') as HTMLPreElement | null;
    const ts = new Date().toISOString();
    if (out) out.textContent = `${ts} ${msg}\n` + out.textContent;
    else console.log('[MON]', ts, msg);
  } catch (e) { try { console.log('[MON]', msg, e); } catch(_) {} }
}

// parseTarget is provided by shared helper

export function createInitialRows(key: string, addr: number, count: number) {
  try { uiLog(`createInitialRows called key=${key} addr=${addr} count=${count}`); } catch (e) {}
  // create rows starting at addr (addr = top of view) for `count` words
  for (let i = 0; i < count; i++) {
    const wordAddr = addr + i;
    setWordRow(key, wordAddr, 0);
  }
  // initialize visible range tracking
  currentRangeStart = addr;
  currentRangeEnd = addr + Math.max(0, count - 1);
  // attach scroll handler to enable prepend/append on user scroll
  try { attachScrollHandler(key); } catch (e) {}
  // immediately fetch initial block including prefetch after to populate beyond 30 rows
  try { fetchRange(key, currentRangeStart || addr, Math.max(count + PREFETCH_AFTER, VISIBLE_COUNT + PREFETCH_AFTER)); } catch (e) {}
}

// Clear all rendered rows and internal cache
export function clearRows() {
  try {
    // clear internal cache
    for (const k in latestWords) delete latestWords[k];
    // remove DOM rows
    const tbody = document.getElementById('monitor-tbody') as HTMLTableSectionElement | null;
    if (tbody) {
      while (tbody.firstChild) tbody.removeChild(tbody.firstChild);
    }
    // stop any fallback polling to avoid get_words calls for removed devices
    try { stopFallbackPolling(); } catch (e) {}
    // clear current monitor target so incoming events are ignored until set
    currentMonitorTarget = null;
    // detach scroll handler and reset range tracking
    try { detachScrollHandler(); } catch (e) {}
    currentRangeStart = null;
    currentRangeEnd = null;
  } catch (e) {
    console.warn('clearRows failed', e);
  }
}

export function setWordRow(key: string, addr: number, word: number) {
  try {
    latestWords[`${key}:${addr}`] = word & 0xffff;
    // update visible range tracking
    try {
      if (currentRangeStart === null || addr < currentRangeStart) currentRangeStart = addr;
      if (currentRangeEnd === null || addr > currentRangeEnd) currentRangeEnd = addr;
    } catch (e) {}
    renderRowForWord(key, addr, word & 0xffff);
    if (['U32','I32','F32'].includes(currentFormatInternal)) {
      if (addr % 2 === 1) {
        const evenAddr = addr - 1;
        const evenKey = `${key}:${evenAddr}`;
        if (latestWords[evenKey] !== undefined) renderRowForWord(key, evenAddr, latestWords[evenKey]);
      } else {
        const oddKey = `${key}:${addr+1}`;
        if (latestWords[oddKey] !== undefined) renderRowForWord(key, addr+1, latestWords[oddKey]);
      }
    }
  } catch (err) { console.warn('setWordRow failed', err); }
}

function renderRowForWord(key: string, addr: number, word: number) {
  try {
    const tbody = document.getElementById('monitor-tbody') as HTMLTableSectionElement | null;
    if (!tbody) return;
    const rowId = `row-${key}-${addr}`;
    let tr = document.getElementById(rowId) as HTMLTableRowElement | null;
    if (!tr) {
      tr = document.createElement('tr');
      tr.id = rowId;
      const tdLabel = document.createElement('td');
      tdLabel.className = 'device-label';
      // use shared formatter for display address
      tdLabel.textContent = formatDisplayAddr(key, addr);
      tr.appendChild(tdLabel);
      // create bit cells according to device family
      const bitCfg = getBitConfigForKey(key);
      for (let i = bitCfg.bits - 1; i >= 0; i--) {
        const td = document.createElement('td');
        td.className = 'bit-cell bit-off';
        td.dataset.bitIndex = i.toString();
        tr.appendChild(td);
      }
      const tdFormat = document.createElement('td'); tdFormat.className = 'format-cell'; tr.appendChild(tdFormat);
      const tdRaw = document.createElement('td'); tdRaw.className = 'raw-cell'; tr.appendChild(tdRaw);
      // insert row into tbody in address order (ascending)
      try {
        const existing = Array.from(tbody.querySelectorAll('tr')) as HTMLTableRowElement[];
        let inserted = false;
        for (const ex of existing) {
          const parts = ex.id.split('-');
          const exAddr = parseInt(parts[parts.length - 1] || '0', 10);
          if (exAddr > addr) { tbody.insertBefore(tr, ex); inserted = true; break; }
        }
        if (!inserted) tbody.appendChild(tr);
      } catch (e) { tbody.appendChild(tr); }
      tr.addEventListener('click', () => { try { selectRow(key, addr); } catch (e) {} });
    }
    const bitCfg = getBitConfigForKey(key);
    const bitCells = tr.querySelectorAll('td.bit-cell');
    if (!bitCells || bitCells.length < bitCfg.bits) return;
    for (let i = 0; i < bitCfg.bits; i++) {
      const b = bitCfg.bits - 1 - i;
      const on = ((word >> b) & 1) === 1;
      const cell = bitCells[i];
      if (cell) {
        if (on) { cell.classList.remove('bit-off'); cell.classList.add('bit-on'); }
        else { cell.classList.remove('bit-on'); cell.classList.add('bit-off'); }
      }
    }
    const formatCell = tr.querySelector('td.format-cell') as HTMLTableCellElement | null;
    const rawCell = tr.querySelector('td.raw-cell') as HTMLTableCellElement | null;
    const u16 = word & 0xffff;
    const bitCfg2 = getBitConfigForKey(key);
    const hex = bitCfg2.bits === 16 ? `0x${u16.toString(16).toUpperCase().padStart(4,'0')}` : `0x${(u16 & ((1<<bitCfg2.bits)-1)).toString(16).toUpperCase()}`;
    let s16 = u16; if ((u16 & 0x8000) !== 0) s16 = u16 - 0x10000;
    tr.classList.remove('paired-empty');
    if (['U32','I32','F32'].includes(currentFormatInternal)) {
      if (addr % 2 === 0) {
        const keyHigh = `${key}:${addr+1}`;
        const low = latestWords[`${key}:${addr}`] !== undefined ? latestWords[`${key}:${addr}`] : u16;
        const high = latestWords[keyHigh] !== undefined ? latestWords[keyHigh] : undefined;
        if (high === undefined) {
          if (formatCell) formatCell.textContent = '';
          if (rawCell) rawCell.textContent = hex;
        } else {
          const low32 = low & 0xffff;
          const high32 = high & 0xffff;
          const u32 = ((high32 << 16) >>> 0) | (low32 & 0xffff);
          if (currentFormatInternal === 'U32') { if (formatCell) formatCell.textContent = `${u32 >>> 0}`; }
          else if (currentFormatInternal === 'I32') { const i32 = (u32 & 0x80000000) ? (u32 - 0x100000000) : u32; if (formatCell) formatCell.textContent = `${i32}`; }
          else if (currentFormatInternal === 'F32') { const buf = new ArrayBuffer(4); const dv = new DataView(buf); dv.setUint32(0, u32 >>> 0, true); const f = dv.getFloat32(0, true); if (formatCell) formatCell.textContent = `${f}`; }
          if (rawCell) rawCell.textContent = `0x${u32.toString(16).toUpperCase().padStart(8,'0')}`;
        }
        const trOdd = document.getElementById(`row-${key}-${addr+1}`);
        if (trOdd) trOdd.classList.add('paired-empty');
      } else {
        if (formatCell) formatCell.textContent = '';
        if (rawCell) rawCell.textContent = '';
        tr.classList.add('paired-empty');
      }
    } else {
    if (currentFormatInternal === 'BIN') { if (formatCell) formatCell.textContent = `0b${u16.toString(2).padStart(bitCfg2.bits,'0')}`; }
      else if (currentFormatInternal === 'U16') { if (formatCell) formatCell.textContent = `${u16}`; }
      else if (currentFormatInternal === 'I16') { if (formatCell) formatCell.textContent = `${s16}`; }
      else if (currentFormatInternal === 'HEX') { if (formatCell) formatCell.textContent = `${hex}`; }
      else if (currentFormatInternal === 'ASCII') { const hi = (u16 >> 8) & 0xff; const lo = u16 & 0xff; const a = (hi >= 32 && hi <= 126) ? String.fromCharCode(hi) : '.'; const b = (lo >= 32 && lo <= 126) ? String.fromCharCode(lo) : '.'; if (formatCell) formatCell.textContent = `${a}${b}`; }
      else { if (formatCell) formatCell.textContent = `${u16}`; }
      if (rawCell) rawCell.textContent = hex;
    }
  } catch (err) { console.warn('renderRowForWord failed', err); }
}

let eventApiAvailable = false; let monitorFallbackId: any = null;
// Visible row range currently rendered (inclusive). null when none.
let currentRangeStart: number | null = null;
let currentRangeEnd: number | null = null;
// Prevent concurrent fetches
let isFetchingRange = false;
let monitorScrollElem: HTMLElement | null = null;
let monitorScrollHandler: ((ev: Event) => void) | null = null;
let monitorWheelHandler: ((ev: WheelEvent) => void) | null = null;
let monitorKeyHandler: ((ev: KeyboardEvent) => void) | null = null;
// scrolling/prefetch constants
const VISIBLE_COUNT = 30;
const PREFETCH_BEFORE = 30; // rows to prefetch when loading earlier addresses
const PREFETCH_AFTER = 60; // rows to prefetch when loading later addresses
const SCROLL_THRESHOLD_PX = 120;
// maximum number of rows to keep in DOM to avoid excessive memory/paint cost
const MAX_RENDERED_ROWS = 300;

function attachScrollHandler(key: string) {
  try {
    const tableEl = document.getElementById('monitor-table') as HTMLElement | null;
    if (!tableEl) { uiLog('attachScrollHandler: monitor-table not found'); return; }
    try { /* attachScrollHandler: found tableEl (suppressed) */ } catch (e) {}
    // find nearest scrollable ancestor (overflow:auto/scroll or scrollHeight>clientHeight)
    let el: HTMLElement | null = tableEl;
    function findScrollContainer(start: HTMLElement | null): HTMLElement | null {
      let cur = start;
      while (cur) {
        try {
          const style = window.getComputedStyle(cur);
          const ov = (style && style.overflowY) || '';
          if (ov === 'auto' || ov === 'scroll' || (cur.scrollHeight > cur.clientHeight)) return cur;
        } catch (e) {}
        cur = cur.parentElement as HTMLElement | null;
      }
      return null;
    }
    const container = findScrollContainer(tableEl.parentElement as HTMLElement | null) || findScrollContainer(tableEl) || tableEl.parentElement || tableEl;
    try { /* attachScrollHandler: resolved container (suppressed) */ } catch (e) {}
    el = container as HTMLElement;
    // remove previous
    if (monitorScrollElem && monitorScrollHandler) {
      try { monitorScrollElem.removeEventListener('scroll', monitorScrollHandler); } catch (e) {}
    }
    monitorScrollElem = el;
    try { uiLog(`attachScrollHandler: attached to ${monitorScrollElem && monitorScrollElem.id ? monitorScrollElem.id : monitorScrollElem && monitorScrollElem.tagName}`); } catch (e) {}
    monitorScrollHandler = (ev: Event) => {
      try {
        const st = (el.scrollTop || 0);
        const ch = el.clientHeight || 0;
        const sh = el.scrollHeight || 0;
        try { /* scroll handler update suppressed to avoid high-frequency logs */ } catch (e) {}
        // near top -> prepend
        if (st < SCROLL_THRESHOLD_PX) {
          if (currentRangeStart !== null && currentRangeStart > 0) prependRows(key, Math.min(PREFETCH_BEFORE, currentRangeStart));
        }
        // near bottom -> append
        if (sh - st - ch < SCROLL_THRESHOLD_PX) {
          appendRows(key, VISIBLE_COUNT);
        }
      } catch (e) { console.warn('monitor scroll handler error', e); }
    };
    el.addEventListener('scroll', monitorScrollHandler);
    // also listen for wheel on the container or table to ensure scrolls are detected
    monitorWheelHandler = (we: WheelEvent) => {
      try {
        // wheel events are frequent; avoid logging each event
        setTimeout(() => { try { monitorScrollHandler && monitorScrollHandler(new Event('scroll')); } catch (e) {} }, 10);
      } catch (e) { /* ignore */ }
    };
    try { el.addEventListener('wheel', monitorWheelHandler, { passive: true } as any); } catch (e) {}
    // Note: keyboard arrow handling for selection is implemented in main.ts to
    // avoid duplicate/conflicting handlers. Do not add a global arrow key
    // listener here to prevent mismatched cursor vs scroll behavior.
    monitorKeyHandler = null;
  } catch (e) { console.warn('attachScrollHandler failed', e); }
}

function detachScrollHandler() {
  try {
    if (monitorScrollElem && monitorScrollHandler) {
      try { monitorScrollElem.removeEventListener('scroll', monitorScrollHandler); } catch (e) {}
      try { if (monitorWheelHandler) monitorScrollElem.removeEventListener('wheel', monitorWheelHandler); } catch (e) {}
    }
  } catch (e) { }
  try { if (monitorKeyHandler) window.removeEventListener('keydown', monitorKeyHandler); } catch (e) {}
  monitorScrollElem = null; monitorScrollHandler = null; monitorWheelHandler = null; monitorKeyHandler = null;
}

async function fetchRange(key: string, start: number, count: number) {
  if (isFetchingRange) return;
  isFetchingRange = true;
  try {
    const vals: number[] = await invoke('get_words', { key, addr: start, count });
    for (let i = 0; i < vals.length; i++) setWordRow(key, start + i, vals[i] & 0xffff);
    // update range bounds
    if (currentRangeStart === null || start < currentRangeStart) currentRangeStart = start;
    if (currentRangeEnd === null || (start + vals.length - 1) > currentRangeEnd) currentRangeEnd = start + vals.length - 1;
  } catch (e) {
    // If the backend read failed (e.g. mock server stopped), still create placeholder rows
    // so scrolling/visual continuity is preserved and trimming won't remove the block.
    console.warn('fetchRange failed', e);
    try {
      for (let i = 0; i < count; i++) setWordRow(key, start + i, 0);
      if (currentRangeStart === null || start < currentRangeStart) currentRangeStart = start;
      if (currentRangeEnd === null || (start + count - 1) > currentRangeEnd) currentRangeEnd = start + count - 1;
    } catch (ee) { console.warn('fetchRange placeholder fill failed', ee); }
  }
  finally { isFetchingRange = false; }
}

function prependRows(key: string, count: number) {
  try {
    if (currentRangeStart === null) return;
    const newStart = Math.max(0, currentRangeStart - count);
    for (let a = newStart; a < currentRangeStart; a++) setWordRow(key, a, 0);
    // fetch the new block plus a bit of after-prefetch so scrolling is smooth
    fetchRange(key, newStart, Math.min(PREFETCH_BEFORE + PREFETCH_AFTER, currentRangeStart - newStart + PREFETCH_AFTER));
  } catch (e) { console.warn('prependRows failed', e); }
}

function appendRows(key: string, count: number) {
  try {
    // append a larger block to avoid small-step stalls
    const appendCount = Math.max(count, PREFETCH_AFTER);
    const start = (currentRangeEnd !== null) ? currentRangeEnd + 1 : 0;
    for (let a = start; a < start + appendCount; a++) setWordRow(key, a, 0);
    // fetch appended block with additional prefetch after
    fetchRange(key, start, appendCount + PREFETCH_AFTER);
    // trim if DOM grew too large
    try { trimRenderedRows(key); } catch (e) {}
  } catch (e) { console.warn('appendRows failed', e); }
}

function trimRenderedRows(key: string) {
  try {
    const tbody = document.getElementById('monitor-tbody') as HTMLTableSectionElement | null;
    if (!tbody) return;
    const rows = Array.from(tbody.querySelectorAll('tr')) as HTMLTableRowElement[];
    if (rows.length <= MAX_RENDERED_ROWS) return;
    // determine keep window
    const keepStart = Math.max(0, (currentRangeStart !== null ? currentRangeStart : 0) - PREFETCH_AFTER);
    const keepEnd = (currentRangeEnd !== null ? currentRangeEnd : (VISIBLE_COUNT - 1)) + PREFETCH_AFTER;
    const center = Math.floor((keepStart + keepEnd) / 2);
    // map rows to {row, addr, distance}
    const mapped = rows.map(r => {
      const parts = r.id.split('-');
      const a = parseInt(parts[parts.length - 1] || '0', 10);
      const dist = Math.abs(a - center);
      return { r, a, dist };
    });
    // sort by distance descending (furthest first)
    mapped.sort((a,b) => b.dist - a.dist);
    // remove furthest rows until under limit
    let removeCount = mapped.length - MAX_RENDERED_ROWS;
    for (let i = 0; i < mapped.length && removeCount > 0; i++) {
      const it = mapped[i];
      // avoid removing rows inside keep window
      if (it.a >= keepStart && it.a <= keepEnd) continue;
      try { if (it.r.parentNode) it.r.parentNode.removeChild(it.r); } catch (e) {}
      removeCount--;
    }
  } catch (e) { console.warn('trimRenderedRows failed', e); }
}

// Ensure monitor UI is initialized: attach scroll handler and prefetch if needed.
export function ensureMonitorInitialized(key: string, addr: number) {
  try {
    if (currentRangeStart === null) currentRangeStart = addr;
    if (currentRangeEnd === null) currentRangeEnd = (addr + VISIBLE_COUNT - 1);
    try { attachScrollHandler(key); } catch (e) { console.warn('ensureMonitorInitialized attach failed', e); }
    // prefetch if we don't have enough range beyond visible
    const have = (currentRangeEnd !== null && currentRangeStart !== null) ? (currentRangeEnd - currentRangeStart + 1) : 0;
    if (have < VISIBLE_COUNT + PREFETCH_AFTER) {
      try { fetchRange(key, currentRangeStart || addr, Math.max(VISIBLE_COUNT + PREFETCH_AFTER, have)); } catch (e) { /* ignore */ }
    }
    try { uiLog(`ensureMonitorInitialized key=${key} start=${currentRangeStart} end=${currentRangeEnd}`); } catch (e) {}
  } catch (e) { console.warn('ensureMonitorInitialized failed', e); }
}
export async function startFallbackPolling(key: string, addr: number, intervalMs: number) {
  stopFallbackPolling();
  uiLog(`startFallbackPolling ${key}${addr} interval=${intervalMs}`);
  // use the current rendered range if available, otherwise fall back to provided addr
  monitorFallbackId = setInterval(async () => {
    try {
      const start = (currentRangeStart !== null) ? currentRangeStart : addr;
      const count = (currentRangeEnd !== null && currentRangeStart !== null) ? (currentRangeEnd - currentRangeStart + 1) : VISIBLE_COUNT;
      // prefetch extra after-range for smoother scrolling
      const fetchCount = Math.max(count + PREFETCH_AFTER, VISIBLE_COUNT + PREFETCH_AFTER);
      const vals = await invoke('get_words', { key: key, addr: start, count: fetchCount });
      for (let i = 0; i < vals.length; i++) setWordRow(key, start + i, vals[i] & 0xffff);
      // extend currentRangeEnd if we fetched beyond
      if (currentRangeStart === null) { currentRangeStart = start; }
      if (currentRangeEnd === null || (start + vals.length - 1) > currentRangeEnd) currentRangeEnd = start + vals.length - 1;
    } catch (e) { console.warn('fallback get_words failed', e); uiLog(`fallback get_words failed: ${e}`); }
  }, intervalMs);
}
export function stopFallbackPolling() { if (monitorFallbackId) { clearInterval(monitorFallbackId); monitorFallbackId = null; uiLog('stopFallbackPolling'); } }

export function selectRow(key: string, addr: number, retries = 6) {
  const prev = document.querySelector('#monitor-tbody tr.selected-row') as HTMLTableRowElement | null;
  if (prev) prev.classList.remove('selected-row');
  const id = `row-${key}-${addr}`;
  const tr = document.getElementById(id) as HTMLTableRowElement | null;
  if (!tr) {
    if (retries > 0) { setTimeout(() => { try { selectRow(key, addr, retries - 1); } catch (e) {} }, 60); }
    return;
  }
  tr.classList.add('selected-row');
  try {
    // Prefer scrolling the monitor scroll container (if known) so selection
    // and scroll remain in sync. Fall back to scrollIntoView when container
    // not available.
    const container = monitorScrollElem || (document.getElementById('monitor-table') as HTMLElement | null)?.parentElement as HTMLElement | null;
    if (container && typeof container.getBoundingClientRect === 'function') {
      const rowRect = tr.getBoundingClientRect();
      const contRect = container.getBoundingClientRect();
      // If row is above visible area
      if (rowRect.top < contRect.top) {
        container.scrollTop = Math.max(0, (container.scrollTop || 0) + (rowRect.top - contRect.top));
      } else if (rowRect.bottom > contRect.bottom) {
        container.scrollTop = (container.scrollTop || 0) + (rowRect.bottom - contRect.bottom);
      }
    } else {
      try { tr.scrollIntoView({ block: 'nearest', inline: 'nearest' }); } catch (e) {}
    }
  } catch (e) { /* ignore */ }
  try { const mt = document.getElementById('monitor-table') as HTMLElement | null; if (mt && typeof (mt as any).focus === 'function') try { (mt as any).focus(); } catch (e) {} } catch (e) {}
  try { const ev = new CustomEvent('melsec_row_selected', { detail: { key, addr } }); document.dispatchEvent(ev); } catch (e) {}
}

export function isEventApiAvailable() { return eventApiAvailable; }

export async function initEventListeners() {
  if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) {
    try {
      uiLog('initEventListeners: Tauri event API available, registering listeners');
      await window.__TAURI__.event.listen('monitor', (event: any) => {
        const payload = event.payload; try {
          const addr = payload.addr; const key = payload.key; const vals: number[] = payload.vals || [];
          // ignore events for other targets when a current target is set
          if (currentMonitorTarget && (currentMonitorTarget.key !== key || currentMonitorTarget.addr !== addr)) {
            uiLog(`monitor event ignored for key=${key} addr=${addr} (current target ${currentMonitorTarget.key}${currentMonitorTarget.addr})`);
            return;
          }
          // Only log when we received an empty payload; avoid logging every monitor event.
          if (vals.length === 0) {
            try { uiLog(`monitor event received key=${key} addr=${addr} len=0`); } catch (e) {}
            setWordRow(key, addr, 0);
          } else {
            for (let i = 0; i < vals.length; i++) setWordRow(key, addr + i, vals[i] & 0xffff);
          }
        } catch (e) {}
      });
      await window.__TAURI__.event.listen('server-status', (event: any) => {
        const payload = event.payload; const status = document.getElementById('server-status'); if (status) { status.textContent = payload; (status as HTMLElement).style.color = (payload === '起動中') ? 'green' : 'black'; }
        try { uiLog(`server-status event: ${payload}`); } catch(e) {}
        try {
          if (payload === '起動中') {
            const mt = document.getElementById('monitor-table') as HTMLElement | null;
            if (mt && typeof (mt as any).focus === 'function') try { (mt as any).focus(); } catch (e) {}
            try {
              const rawEl = document.getElementById('mon-target') as HTMLInputElement | null;
              const raw = rawEl ? (rawEl.value || 'D') : 'D';
              let parsed: any = parseTarget(raw.toString().trim().toUpperCase());
              if (!parsed) parsed = { key: raw.replace(/[^A-Z]/g, ''), addr: 0 } as any;
              if (parsed) try { selectRow(parsed.key, parsed.addr); } catch (e) {}
            } catch (e) {}
          }
        } catch (e) {}
      });
      eventApiAvailable = true;
    } catch (e) { console.warn('event.listen not allowed, falling back to frontend polling', e); uiLog(`event.listen not allowed, falling back to polling: ${e}`); eventApiAvailable = false; }
  } else { console.warn('Tauri event API not available'); uiLog('Tauri event API not available'); eventApiAvailable = false; }
}

