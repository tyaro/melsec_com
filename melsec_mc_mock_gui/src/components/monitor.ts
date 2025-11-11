declare const window: any;
declare global { interface Window { __TAURI__?: any } }
// safe invoke wrapper: in Tauri runtime window.__TAURI__.core.invoke exists;
// in tests it may be undefined, so provide a stub that throws to make failures explicit.
const invoke: (...args: any[]) => Promise<any> = (window && (window as any).__TAURI__ && (window as any).__TAURI__.core && (window as any).__TAURI__.core.invoke)
  ? (window as any).__TAURI__.core.invoke.bind((window as any).__TAURI__.core)
  : async () => { throw new Error('Tauri invoke not available in test environment'); };

export const latestWords: { [k:string]: number } = {};
let currentFormatInternal = 'U16';

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

export function parseTarget(s: string | null) {
  if (!s) return null;
  const up = s.toUpperCase().trim();
  let i = 0; while (i < up.length && /[A-Z]/.test(up[i])) i++;
  if (i === 0) return null;
  let key = up.slice(0, i);
  let numPart = up.slice(i).trim();
  // handle cases like 'WFF' where the address is hex letters following a single-letter device code
  if (!numPart && key.length > 1) {
    numPart = key.slice(1);
    key = key[0];
  }
  if (!numPart) return null;
  const isHex = /[A-F]/i.test(numPart);
  const addr = isHex ? parseInt(numPart, 16) : parseInt(numPart, 10);
  if (Number.isNaN(addr)) return null;
  return { key, addr };
}

export function createInitialRows(key: string, addr: number, count: number) {
  for (let i = 0; i < count; i++) setWordRow(key, addr + i, 0);
}

export function setWordRow(key: string, addr: number, word: number) {
  try {
    latestWords[`${key}:${addr}`] = word & 0xffff;
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
      tdLabel.textContent = `${key}${addr}`;
      tr.appendChild(tdLabel);
      for (let b = 15; b >= 0; b--) {
        const td = document.createElement('td');
        td.className = 'bit-cell bit-off';
        td.dataset.bitIndex = b.toString();
        tr.appendChild(td);
      }
      const tdFormat = document.createElement('td'); tdFormat.className = 'format-cell'; tr.appendChild(tdFormat);
      const tdRaw = document.createElement('td'); tdRaw.className = 'raw-cell'; tr.appendChild(tdRaw);
      tbody.appendChild(tr);
      tr.addEventListener('click', () => { try { selectRow(key, addr); } catch (e) {} });
    }
    const bitCells = tr.querySelectorAll('td.bit-cell');
    if (!bitCells || bitCells.length < 16) return;
    for (let i = 0; i < 16; i++) {
      const b = 15 - i;
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
    const hex = `0x${u16.toString(16).toUpperCase().padStart(4,'0')}`;
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
      if (currentFormatInternal === 'BIN') { if (formatCell) formatCell.textContent = `0b${u16.toString(2).padStart(16,'0')}`; }
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
export async function startFallbackPolling(key: string, addr: number, intervalMs: number) {
  stopFallbackPolling(); const count = 30;
  uiLog(`startFallbackPolling ${key}${addr} interval=${intervalMs}`);
  monitorFallbackId = setInterval(async () => {
    try {
      const vals = await invoke('get_words', { key: key, addr: addr, count: count });
      for (let i = 0; i < vals.length; i++) setWordRow(key, addr + i, vals[i] & 0xffff);
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
  try { tr.scrollIntoView({ block: 'nearest', inline: 'nearest' }); } catch (e) {}
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
          // UI-visible debug: log minimal payload so developer can correlate with backend
          try { uiLog(`monitor event received key=${key} addr=${addr} vals0=${vals.length>0?vals[0]:'<empty>'} len=${vals.length}`); } catch(e) {}
          if (vals.length === 0) setWordRow(key, addr, 0);
          else for (let i = 0; i < vals.length; i++) setWordRow(key, addr + i, vals[i] & 0xffff);
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

