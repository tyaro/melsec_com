const { invoke } = window.__TAURI__.core;

let els = {};

function logMonitor(msg) {
  const out = document.getElementById('monitor-log');
  const ts = new Date().toISOString();
  if (out) {
    out.textContent = `${ts} ${msg}\n` + out.textContent;
  } else {
    // fallback to console if monitor-log not present
    console.log('[LOG]', ts, msg);
  }
}

async function startMock() {
  const ip = els.ip.value;
  const tcp_port = parseInt(els['tcp-port'].value || '5000', 10);
  const udp_port = parseInt(els['udp-port'].value || '5001', 10);
  const tim_await_ms = parseInt(els['tim-await'].value || '5000', 10);
  try {
    // Tauri maps snake_case Rust arg names to camelCase in JS
    await invoke('start_mock', { ip, tcpPort: tcp_port, udpPort: udp_port, timAwaitMs: tim_await_ms });
    logMonitor(`[JS] start_mock invoked ip=${ip} tcp=${tcp_port} udp=${udp_port} tim=${tim_await_ms}`);
    const status = document.getElementById('server-status');
    if (status) {
      status.textContent = '起動中';
      status.style.color = 'green';
    }
  } catch (e) {
    logMonitor(`[JS] start_mock error: ${e}`);
    const status = document.getElementById('server-status');
    if (status) {
      status.textContent = '起動失敗';
      status.style.color = 'red';
    }
  }
}

async function startMonitor() {
  const target = els['mon-target'].value.trim();
  const interval_ms = parseInt(els['mon-interval'].value || '500', 10);
  try {
    // interval_ms in Rust becomes intervalMs in JS
    await invoke('start_monitor', { target, intervalMs: interval_ms });
    logMonitor(`[JS] start_monitor ${target} interval=${interval_ms}`);
  } catch (e) {
    logMonitor(`[JS] start_monitor error: ${e}`);
  }
}

async function stopMonitor() {
  try {
    await invoke('stop_monitor');
    logMonitor('[JS] stop_monitor invoked');
  } catch (e) {
    logMonitor(`[JS] stop_monitor error: ${e}`);
  }
}

window.addEventListener('DOMContentLoaded', () => {
  // collect elements (including stop-mock)
  ['ip','tcp-port','udp-port','tim-await','start-mock','stop-mock','mon-target','mon-interval','mon-toggle'].forEach(id => {
    els[id] = document.getElementById(id);
  });

  // set-word elements
  ['set-key','set-addr','set-val','set-word'].forEach(id => {
    els[id] = document.getElementById(id);
  });

  els['start-mock'].addEventListener('click', (e) => { e.preventDefault(); startMock(); });
  els['stop-mock'].addEventListener('click', (e) => { e.preventDefault(); stopMock(); });
  // monitor toggle button: start or stop depending on state
  let monitorRunning = false;
  async function toggleMonitor() {
    const btn = els['mon-toggle'];
    if (!monitorRunning) {
      await startMonitor();
      monitorRunning = true;
      if (btn) btn.textContent = 'モニタ停止';
      // create initial rows D0..D29 (or base addr..addr+29) with zeros so UI shows monitoring immediately
      try {
        const target = els['mon-target'].value.trim().toUpperCase();
        const parsed = parseTarget(target);
        if (parsed) {
          const { key, addr } = parsed;
          createInitialRows(key, addr, 30);
        }
      } catch (e) {
        console.warn('failed to create initial rows', e);
      }
    } else {
      await stopMonitor();
      monitorRunning = false;
      if (btn) btn.textContent = 'モニタ開始';
    }
  }
  els['mon-toggle'].addEventListener('click', (e) => { e.preventDefault(); toggleMonitor(); });

  // set-word handler (temporary testing helper)
  async function setWord() {
    try {
      const key = (els['set-key'].value || 'D').toString();
      const addr = parseInt(els['set-addr'].value || '0', 10);
      const raw = (els['set-val'].value || '0').toString().trim();
      const parts = raw.split(',').map(s => s.trim()).filter(s => s.length > 0);
      const words = parts.map(p => {
        if (/^0x/i.test(p)) return parseInt(p.substring(2), 16) & 0xffff;
        if (/^[0-9]+$/.test(p)) return parseInt(p, 10) & 0xffff;
        // fallback: try parseInt
        const v = parseInt(p, 10);
        return Number.isNaN(v) ? 0 : (v & 0xffff);
      });
      // call backend set_words
      await invoke('set_words', { key: key, addr: addr, words: words });
      logMonitor(`[JS] set_words invoked key=${key} addr=${addr} words=${JSON.stringify(words)}`);
      // optimistic update for first word
      if (words.length > 0) {
        setWordRow(key, addr, words[0]);
      }
    } catch (e) {
      logMonitor(`[JS] set_words error: ${e}`);
    }
  }
  if (els['set-word']) els['set-word'].addEventListener('click', (e) => { e.preventDefault(); setWord(); });
  // pre-populate table rows D0..D29 on load so table is visible immediately
  try {
    createInitialRows('D', 0, 30);
  } catch (e) {
    console.warn('failed to create initial rows on load', e);
  }

  // in-memory cache of latest words per device (key: `${key}:${addr}` -> u16)
  const latestWords = {};
  // current display format (default U16). Options: BIN,U16,I16,HEX,ASCII,U32,I32,F32
  let currentFormat = 'U16';

  // initialize display-format toolbar buttons (after currentFormat is defined)
  try {
    const btns = document.querySelectorAll('#display-toolbar .fmt-btn');
    btns.forEach(b => {
      const fmt = b.dataset.fmt;
      if (fmt === currentFormat) b.classList.add('active');
      b.addEventListener('click', (ev) => {
        // set active class
        document.querySelectorAll('#display-toolbar .fmt-btn').forEach(x => x.classList.remove('active'));
        b.classList.add('active');
        currentFormat = fmt;
        // refresh all visible rows
        refreshAllRows();
      });
    });
  } catch (e) {
    console.warn('failed to initialize display toolbar', e);
  }

  // helper: refresh rendering for all existing rows according to currentFormat
  function refreshAllRows() {
    // iterate over known latestWords keys and re-render rows
    for (const k in latestWords) {
      const [key, addrStr] = k.split(':');
      const addr = parseInt(addrStr, 10);
      const w = latestWords[k];
      // call internal renderer (will consult currentFormat)
      renderRowForWord(key, addr, w);
    }
    // also ensure paired rows (odd rows when U32/I32/F32) are greyed
    if (['U32','I32','F32'].includes(currentFormat)) {
      // for every even addr present, mark addr+1 as paired-empty if exists
      for (const k in latestWords) {
        const [key, addrStr] = k.split(':');
        const addr = parseInt(addrStr, 10);
        if (addr % 2 === 0) {
          const trOdd = document.getElementById(`row-${key}-${addr+1}`);
          if (trOdd) trOdd.classList.add('paired-empty');
        }
      }
    } else {
      // remove paired-empty from all rows
      document.querySelectorAll('#monitor-tbody tr.paired-empty').forEach(r => r.classList.remove('paired-empty'));
    }
  }

  // helper: create or update a row for a word value (stores in cache then renders)
  function setWordRow(key, addr, word) {
    // store latest
    latestWords[`${key}:${addr}`] = word & 0xffff;
    renderRowForWord(key, addr, word & 0xffff);
    // when format is combined (U32/I32/F32) and this is the high-word (addr-1 even), refresh the pair's rendering
    if (['U32','I32','F32'].includes(currentFormat)) {
      if (addr % 2 === 1) {
        // this is odd addr (high word), ensure the even partner is refreshed too
        const evenAddr = addr - 1;
        const evenKey = `${key}:${evenAddr}`;
        if (latestWords[evenKey] !== undefined) {
          renderRowForWord(key, evenAddr, latestWords[evenKey]);
        }
      } else {
        // even addr updated, also try to refresh the odd partner
        const oddKey = `${key}:${addr+1}`;
        if (latestWords[oddKey] !== undefined) {
          renderRowForWord(key, addr+1, latestWords[oddKey]);
        }
      }
    }
  }

  // internal renderer which updates DOM for a single row according to currentFormat and latestWords
  function renderRowForWord(key, addr, word) {
    const tbody = document.getElementById('monitor-tbody');
    const rowId = `row-${key}-${addr}`;
    let tr = document.getElementById(rowId);
    if (!tr) {
      tr = document.createElement('tr');
      tr.id = rowId;
      // device label cell
      const tdLabel = document.createElement('td');
      tdLabel.className = 'device-label';
      tdLabel.textContent = `${key}${addr}`;
      tr.appendChild(tdLabel);
      // 16 bit cells
      for (let b = 15; b >= 0; b--) {
        const td = document.createElement('td');
        td.className = 'bit-cell bit-off';
        td.dataset.bitIndex = b.toString();
        tr.appendChild(td);
      }
      // formatted value cell (user-selectable format)
      const tdFormat = document.createElement('td');
      tdFormat.className = 'format-cell';
      tr.appendChild(tdFormat);
      // raw hex value cell
      const tdRaw = document.createElement('td');
      tdRaw.className = 'raw-cell';
      tr.appendChild(tdRaw);
      tbody.appendChild(tr);
    }
    // update bits and value
    let bitCells = tr.querySelectorAll('td.bit-cell');
    // bitCells ordered from bit15..bit0
    for (let i = 0; i < 16; i++) {
      const b = 15 - i;
      const on = ((word >> b) & 1) === 1;
      const cell = bitCells[i];
      if (on) {
        cell.classList.remove('bit-off');
        cell.classList.add('bit-on');
      } else {
        cell.classList.remove('bit-on');
        cell.classList.add('bit-off');
      }
    }
    const formatCell = tr.querySelector('td.format-cell');
    const rawCell = tr.querySelector('td.raw-cell');
    const u16 = word & 0xffff;
    const hex = `0x${u16.toString(16).toUpperCase().padStart(4,'0')}`;
    let s16 = u16;
    if ((u16 & 0x8000) !== 0) s16 = u16 - 0x10000;

    // Clear paired-empty if present, will be set later for odd rows when combined formats active
    tr.classList.remove('paired-empty');

    // Combined 32-bit formats: only render on even addresses (addr % 2 === 0)
    if (['U32','I32','F32'].includes(currentFormat)) {
      if (addr % 2 === 0) {
        // attempt to read partner word (addr+1)
        const keyHigh = `${key}:${addr+1}`;
        const low = latestWords[`${key}:${addr}`] !== undefined ? latestWords[`${key}:${addr}`] : u16;
        const high = latestWords[keyHigh] !== undefined ? latestWords[keyHigh] : undefined;
        if (high === undefined) {
          // partner missing: show placeholder for combined cell and raw for this word
          if (formatCell) formatCell.textContent = '';
          if (rawCell) rawCell.textContent = hex;
        } else {
          // combine: assume little-endian word pair (low at addr, high at addr+1)
          const low32 = low & 0xffff;
          const high32 = high & 0xffff;
          const u32 = ((high32 << 16) >>> 0) | (low32 & 0xffff);
          if (currentFormat === 'U32') {
            if (formatCell) formatCell.textContent = `${u32 >>> 0}`;
          } else if (currentFormat === 'I32') {
            // signed 32
            const i32 = (u32 & 0x80000000) ? (u32 - 0x100000000) : u32;
            if (formatCell) formatCell.textContent = `${i32}`;
          } else if (currentFormat === 'F32') {
            // interpret bits as float (little-endian)
            const buf = new ArrayBuffer(4);
            const dv = new DataView(buf);
            dv.setUint32(0, u32 >>> 0, true);
            const f = dv.getFloat32(0, true);
            if (Number.isFinite(f)) {
              if (formatCell) formatCell.textContent = `${f}`;
            } else {
              if (formatCell) formatCell.textContent = `${f}`;
            }
          }
          // RAW show combined hex (8 digits)
          if (rawCell) rawCell.textContent = `0x${u32.toString(16).toUpperCase().padStart(8,'0')}`;
        }
        // ensure partner row (odd) is greyed out
        const trOdd = document.getElementById(`row-${key}-${addr+1}`);
        if (trOdd) trOdd.classList.add('paired-empty');
      } else {
        // odd address: hide formatted/raw cell and grey row
        if (formatCell) formatCell.textContent = '';
        if (rawCell) rawCell.textContent = '';
        tr.classList.add('paired-empty');
      }
    } else {
      // non-combined formats (per-word)
      if (currentFormat === 'BIN') {
        // show binary 16-bit as 0bXXXXXXXXXXXX
        if (formatCell) formatCell.textContent = `0b${u16.toString(2).padStart(16,'0')}`;
      } else if (currentFormat === 'U16') {
        if (formatCell) formatCell.textContent = `${u16}`;
      } else if (currentFormat === 'I16') {
        if (formatCell) formatCell.textContent = `${s16}`;
      } else if (currentFormat === 'HEX') {
        if (formatCell) formatCell.textContent = `${hex}`;
      } else if (currentFormat === 'ASCII') {
        // two ASCII chars from high byte then low byte
        const hi = (u16 >> 8) & 0xff;
        const lo = u16 & 0xff;
        const a = (hi >= 32 && hi <= 126) ? String.fromCharCode(hi) : '.';
        const b = (lo >= 32 && lo <= 126) ? String.fromCharCode(lo) : '.';
        if (formatCell) formatCell.textContent = `${a}${b}`;
      } else {
        if (formatCell) formatCell.textContent = `${u16}`;
      }
      // RAW as single-word hex
      if (rawCell) rawCell.textContent = hex;
    }
  }

  // parse a combined target like D100 or W1FFF into {key, addr}
  function parseTarget(s) {
    if (!s) return null;
    const up = s.toUpperCase().trim();
    let i = 0;
    while (i < up.length && /[A-Z]/.test(up[i])) i++;
    if (i === 0) return null;
    const key = up.slice(0, i);
    const numPart = up.slice(i).trim();
    if (!numPart) return null;
    // if contains hex digit letters A-F assume hex, else decimal
    const isHex = /[A-F]/i.test(numPart);
    const addr = isHex ? parseInt(numPart, 16) : parseInt(numPart, 10);
    if (Number.isNaN(addr)) return null;
    return { key, addr };
  }

  // create `count` rows starting from key+addr with zero value
  function createInitialRows(key, addr, count) {
    for (let i = 0; i < count; i++) {
      setWordRow(key, addr + i, 0);
    }
  }

  // Try to use Tauri event.listen, otherwise fall back to polling get_words
  let eventApiAvailable = false;
  let monitorFallbackId = null;

  async function startFallbackPolling(key, addr, intervalMs) {
    stopFallbackPolling();
    const count = 30;
    monitorFallbackId = setInterval(async () => {
      try {
        const vals = await invoke('get_words', { key: key, addr: addr, count: count });
        // vals is array of numbers
        for (let i = 0; i < vals.length; i++) {
          const w = vals[i] & 0xffff;
          setWordRow(key, addr + i, w);
        }
      } catch (e) {
        console.warn('fallback get_words failed', e);
      }
    }, intervalMs);
  }

  function stopFallbackPolling() {
    if (monitorFallbackId) {
      clearInterval(monitorFallbackId);
      monitorFallbackId = null;
    }
  }

  (async () => {
    if (window.__TAURI__ && window.__TAURI__.event && window.__TAURI__.event.listen) {
      try {
        await window.__TAURI__.event.listen('monitor', (event) => {
          const payload = event.payload;
          console.log('[JS] monitor event received', payload); // debug hook (A)
          try {
            const addr = payload.addr;
            const key = payload.key;
            const vals = payload.vals || [];
            if (vals.length === 0) {
              // ensure at least one row is shown for the target address
              setWordRow(key, addr, 0);
            } else {
              for (let i = 0; i < vals.length; i++) {
                const w = vals[i] & 0xffff;
                setWordRow(key, addr + i, w);
              }
            }
          } catch (e) {
            // ignore monitor payload parse errors silently per spec
          }
        });

        await window.__TAURI__.event.listen('server-status', (event) => {
          const payload = event.payload;
          const status = document.getElementById('server-status');
          if (status) {
            status.textContent = payload;
            status.style.color = (payload === '起動中') ? 'green' : 'black';
          }
        });

        eventApiAvailable = true;
      } catch (e) {
        console.warn('event.listen not allowed, falling back to frontend polling', e);
        eventApiAvailable = false;
      }
    } else {
      console.warn('Tauri event API not available');
      eventApiAvailable = false;
    }
  })();

  // manage fallback polling when user toggles monitor
  els['mon-toggle'].addEventListener('click', (e) => {
    // small timeout allow toggleMonitor to change UI and call start_monitor
    setTimeout(() => {
      const btn = els['mon-toggle'];
      const isRunning = btn && btn.textContent && btn.textContent.includes('停止');
      if (!eventApiAvailable && isRunning) {
        // start fallback polling using mon-target
        try {
          const parsed = parseTarget((els['mon-target'].value || 'D0').toString());
          const interval_ms = parseInt(els['mon-interval'].value || '500', 10);
          if (parsed) startFallbackPolling(parsed.key, parsed.addr, interval_ms);
        } catch (e) { console.warn('failed to start fallback polling', e); }
      } else if (!eventApiAvailable && !isRunning) {
        stopFallbackPolling();
      }
    }, 50);
  });
});

async function stopMock() {
  try {
    await invoke('stop_mock');
    const status = document.getElementById('server-status');
    if (status) {
      status.textContent = '停止中';
      status.style.color = 'black';
    }
  } catch (e) {
    logMonitor(`[JS] stop_mock error: ${e}`);
  }
}
