// TypeScript version of the frontend main script.
// Minimal typing to avoid compile friction. This file is intended to be built
// to `dist/main.js` (see package.json scripts).

// Full TypeScript port of the previous main.js logic.
declare const window: any;
// allow referencing window.__TAURI__ in TS
declare global { interface Window { __TAURI__?: any } }
const { invoke } = (window as any).__TAURI__.core as any;

import { getCurrentFormat, setCurrentFormat, parseTarget, createInitialRows, startFallbackPolling, stopFallbackPolling, selectRow, setWordRow, isEventApiAvailable, initEventListeners } from './components/monitor';

const els: { [k: string]: HTMLElement | HTMLInputElement | null } = {} as any;

function logMonitor(msg: string) {
  const out = document.getElementById('monitor-log') as HTMLPreElement | null;
  const ts = new Date().toISOString();
  if (out) {
    out.textContent = `${ts} ${msg}\n` + out.textContent;
  } else {
    console.log('[LOG]', ts, msg);
  }
}

async function startMock(tcpPort: number, udpPort: number, timAwaitMs: number) {
  // IP is fixed to 0.0.0.0 per UX decision
  const ip = '0.0.0.0';
  try {
    await invoke('start_mock', { ip, tcpPort, udpPort, timAwaitMs });
    logMonitor(`[TS] start_mock invoked ip=${ip} tcp=${tcpPort} udp=${udpPort} tim=${timAwaitMs}`);
    const status = document.getElementById('server-status');
    if (status) {
      status.textContent = '起動中';
      (status as HTMLElement).style.color = 'green';
    }
  } catch (e) {
    logMonitor(`[TS] start_mock error: ${e}`);
    const status = document.getElementById('server-status');
    if (status) {
      status.textContent = '起動失敗';
      (status as HTMLElement).style.color = 'red';
    }
  }
}

async function startMonitorForTarget(targetKey: string, addr: number) {
  const backendTarget = `${targetKey}${addr}`;
  const interval_ms = 500; // fixed internally per spec
  try {
    await invoke('start_monitor', { target: backendTarget, intervalMs: interval_ms });
    logMonitor(`[TS] start_monitor ${backendTarget} interval=${interval_ms}`);
  } catch (e) {
    logMonitor(`[TS] start_monitor error: ${e}`);
  }
}

async function stopMonitor() {
  try {
    await invoke('stop_monitor');
    logMonitor('[TS] stop_monitor invoked');
  } catch (e) {
    logMonitor(`[TS] stop_monitor error: ${e}`);
  }
}

window.addEventListener('DOMContentLoaded', () => {
  // collect elements (IP is fixed and removed from UI)
  ['tcp-port','udp-port','tim-await','mock-toggle','mon-target','mon-toggle','auto-start-next'].forEach(id => {
    els[id] = document.getElementById(id) as any;
  });

  // allow pressing Enter when a row is selected (and focus is not on an input)
  window.addEventListener('keydown', async (ev: KeyboardEvent) => {
    if (ev.key !== 'Enter') return;
    const active = document.activeElement as HTMLElement | null;
    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable)) return;
    const selected = document.querySelector('#monitor-tbody tr.selected-row') as HTMLTableRowElement | null;
    if (!selected) return;
    const m = selected.id.match(/^row-(.+)-(\d+)$/);
    if (!m) return;
    ev.preventDefault();
    const k = m[1]; const a = parseInt(m[2], 10);
    // open edit popup for selected row (follow selection)
    try { showEditModal(k, a); } catch (err) { console.warn('showEditModal failed', err); }
  });

  ['set-key','set-addr','set-val','set-word'].forEach(id => {
    els[id] = document.getElementById(id) as any;
  });

  // mock toggle behaviour: Start Mock <-> Stop Mock
  let mockRunning = false;
  const mockBtn = els['mock-toggle'] as HTMLButtonElement;
  if (mockBtn) {
    mockBtn.addEventListener('click', async (ev) => {
      ev.preventDefault();
      const tcp_port = parseInt((els['tcp-port'] as HTMLInputElement).value || '5000', 10);
      const udp_port = parseInt((els['udp-port'] as HTMLInputElement).value || '5001', 10);
      const tim_await_ms = parseInt((els['tim-await'] as HTMLInputElement).value || '5000', 10);
      if (!mockRunning) {
        await startMock(tcp_port, udp_port, tim_await_ms);
        mockRunning = true;
        mockBtn.textContent = 'Stop Mock';
        mockBtn.style.background = '#d9534f';
        // persist auto-start preference
        try {
          const autoEl = els['auto-start-next'] as HTMLInputElement | null;
          if (autoEl && window.localStorage) {
            if (autoEl.checked) window.localStorage.setItem('autoStartNext', '1');
            else window.localStorage.removeItem('autoStartNext');
          }
        } catch (e) {}
        // start monitor for current target (30 items)
  const rawTarget = ((els['mon-target'] as HTMLInputElement).value || 'D').toString().trim().toUpperCase();
  let parsed: any = parseTarget(rawTarget);
  if (!parsed) parsed = { key: rawTarget.replace(/[^A-Z]/g, ''), addr: 0 } as any;
        try { createInitialRows(parsed.key, parsed.addr, 30); } catch (e) {}
        await startMonitorForTarget(parsed.key, parsed.addr);
  if (!isEventApiAvailable()) startFallbackPolling(parsed.key, parsed.addr, 500);
        // select and focus the initial row now that server is running
        try { selectRow(parsed.key, parsed.addr); const mt = document.getElementById('monitor-table') as HTMLElement | null; if (mt && typeof (mt as any).focus === 'function') (mt as any).focus(); } catch (e) {}
      } else {
        // stop monitor then stop mock
        try { await stopMonitor(); } catch (e) {}
        try { await invoke('stop_mock'); } catch (e) { logMonitor(`[TS] stop_mock error: ${e}`); }
        mockRunning = false;
        mockBtn.textContent = 'Start Mock';
        mockBtn.style.background = '#4da6ff';
        stopFallbackPolling();
        const status = document.getElementById('server-status'); if (status) { status.textContent = '停止中'; (status as HTMLElement).style.color = 'black'; }
      }
    });
    // give initial focus to Start Mock button
    try { mockBtn.focus(); } catch (e) {}
    // if auto-start preference is set, trigger click to start mock
    try {
      const autoEl = els['auto-start-next'] as HTMLInputElement | null;
      if (autoEl && autoEl.checked) {
        setTimeout(() => { try { if (!mockRunning) mockBtn.click(); } catch (e) {} }, 150);
      }
    } catch (e) { /* ignore */ }
  }

  // (initial rows creation moved further down after latestWords initialization)

  // initialize auto-start checkbox from localStorage and set initial focus to Start Mock
  try {
    const autoEl = els['auto-start-next'] as HTMLInputElement | null;
    if (autoEl && window.localStorage) {
      const saved = window.localStorage.getItem('autoStartNext');
      if (saved === '1') autoEl.checked = true;
    }
  } catch (e) { /* ignore */ }

  // Target change on Enter: update displayed range and, if running, restart monitor
  const monTargetEl = els['mon-target'] as HTMLInputElement;
  if (monTargetEl) {
    monTargetEl.addEventListener('keydown', async (e) => {
      if (e.key === 'Enter') {
        const raw = (monTargetEl.value || '').toString().trim().toUpperCase();
        let parsed: any = parseTarget(raw);
        if (!parsed) parsed = { key: raw.replace(/[^A-Z]/g, ''), addr: 0 } as any;
        try { createInitialRows(parsed.key, parsed.addr, 30); } catch (err) {}
        // if mock is running, restart monitor for new target
        if (mockRunning) {
          try { await stopMonitor(); } catch (err) {}
          await startMonitorForTarget(parsed.key, parsed.addr);
          if (!isEventApiAvailable()) startFallbackPolling(parsed.key, parsed.addr, 500);
        }
      }
    });
  }

  // set-word handler (temporary testing helper)
  async function setWord() {
    try {
      const key = ((els['set-key'] as HTMLInputElement).value || 'D').toString();
      const addr = parseInt((els['set-addr'] as HTMLInputElement).value || '0', 10);
      const raw = ((els['set-val'] as HTMLInputElement).value || '0').toString().trim();
      const parts = raw.split(',').map((s) => s.trim()).filter((s) => s.length > 0);
      const words = parts.map((p) => {
        if (/^0x/i.test(p)) return parseInt(p.substring(2), 16) & 0xffff;
        if (/^[0-9]+$/.test(p)) return parseInt(p, 10) & 0xffff;
        const v = parseInt(p, 10);
        return Number.isNaN(v) ? 0 : (v & 0xffff);
      });
      await invoke('set_words', { key: key, addr: addr, words: words });
      logMonitor(`[TS] set_words invoked key=${key} addr=${addr} words=${JSON.stringify(words)}`);
      if (words.length > 0) setWordRow(key, addr, words[0]);
    } catch (e) {
      logMonitor(`[TS] set_words error: ${e}`);
    }
  }
  if (els['set-word']) (els['set-word'] as HTMLElement).addEventListener('click', (e) => { e.preventDefault(); setWord(); });


  // restore display format from monitor component
  try { const saved = window.localStorage ? window.localStorage.getItem('displayFormat') : null; if (saved) setCurrentFormat(saved); } catch (e) { /* ignore */ }

  // initialize toolbar buttons (delegates format change to monitor component)
  try {
    const btns = document.querySelectorAll('#display-toolbar .fmt-btn');
    btns.forEach((b) => {
      if (!b || typeof (b as any).addEventListener !== 'function') return;
      const fmt = (b as HTMLElement).getAttribute('data-fmt') || '';
      if (fmt === getCurrentFormat()) b.classList.add('active');
      (b as HTMLElement).addEventListener('click', () => {
        document.querySelectorAll('#display-toolbar .fmt-btn').forEach(x => x.classList.remove('active'));
        b.classList.add('active');
        setCurrentFormat(fmt);
        // also sync edit popup buttons if present
        try {
          document.querySelectorAll('#edit-modal .write-type').forEach(x => x.classList.remove('active'));
          const pb = document.querySelector(`#edit-modal .write-type[data-typ="${fmt}"]`);
          if (pb) (pb as HTMLElement).classList.add('active');
          selectedWriteType = fmt;
        } catch (e) { /* ignore */ }
      });
    });
  } catch (e) { console.warn('failed to init toolbar', e); }

  // monitor functions have been moved to ./components/monitor and are imported above
  // --- Double-click edit modal handling ---
  const editModal = document.getElementById('edit-modal') as HTMLDivElement | null;
  const editTitle = document.getElementById('edit-modal-title') as HTMLHeadingElement | null;
  const editValue = document.getElementById('edit-value') as HTMLInputElement | null;
  const editCancel = document.getElementById('edit-cancel') as HTMLButtonElement | null;
  const editWrite = document.getElementById('edit-write') as HTMLButtonElement | null;
  let editTarget: { key: string; addr: number } | null = null;
  let selectedWriteType = 'U16';

  // follow selection: when a row is selected elsewhere, update editTarget and popup title
  document.addEventListener('melsec_row_selected', (ev: Event) => {
    try {
      const d = (ev as CustomEvent).detail as { key: string; addr: number };
      if (!d) return;
      // always update internal editTarget so opening the popup uses latest selection
      editTarget = { key: d.key, addr: d.addr };
      // always update popup title so it follows selection immediately when visible
      try {
        if (editTitle) editTitle.textContent = `Write ${d.key}${d.addr}`;
        // if popup visible, clear value and focus input so user can type immediately
        if (editModal && editModal.style.display && editModal.style.display !== 'none') {
          if (editValue) { editValue.value = ''; try { editValue.focus(); } catch (e) {} }
        }
      } catch (e) { /* ignore */ }
    } catch (e) { /* ignore */ }
  });

  function showEditModal(key: string, addr: number) {
    editTarget = { key, addr };
    if (editTitle) editTitle.textContent = `Write ${key}${addr}`;
    if (editValue) editValue.value = '';
  // set initial write-type to current monitor display format
  selectedWriteType = getCurrentFormat() || 'U16';
    // highlight current selection
    document.querySelectorAll('#edit-modal .write-type').forEach(b => b.classList.remove('active'));
    const btn = document.querySelector(`#edit-modal .write-type[data-typ="${selectedWriteType}"]`);
    if (btn) btn.classList.add('active');
    // show as popup (modaless)
    if (editModal) editModal.style.display = 'block';
    // focus the value input so user can type immediately
    try { setTimeout(() => { if (editValue) { editValue.focus(); editValue.select(); } }, 0); } catch (e) { /* ignore */ }
  }

  // --- drag support for edit popup ---
  (function setupEditDrag() {
    const box = document.getElementById('edit-modal-box') as HTMLDivElement | null;
    const title = document.getElementById('edit-modal-title') as HTMLElement | null;
    if (!box || !title) return;
    // style title as drag handle
    title.style.cursor = 'grab';
    let dragging = false;
    let offsetX = 0, offsetY = 0;
    function onMouseMove(ev: MouseEvent) {
      if (!dragging) return;
      const x = ev.clientX - offsetX;
      const y = ev.clientY - offsetY;
      // switch to left/top positioning if previously anchored to right/bottom
      box!.style.left = `${Math.max(0, x)}px`;
      box!.style.top = `${Math.max(0, y)}px`;
      box!.style.right = '';
      box!.style.bottom = '';
      box!.style.transform = 'none';
    }
    function onMouseUp() {
      if (!dragging) return;
      dragging = false;
      title!.style.cursor = 'grab';
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      // persist position
      try { savePopupPos(); } catch (e) {}
    }
    title.addEventListener('mousedown', (ev) => {
      ev.preventDefault();
      const rect = box.getBoundingClientRect();
      offsetX = ev.clientX - rect.left;
      offsetY = ev.clientY - rect.top;
      dragging = true;
      title.style.cursor = 'grabbing';
      window.addEventListener('mousemove', onMouseMove);
      window.addEventListener('mouseup', onMouseUp);
    });
    // also support touch
    title.addEventListener('touchstart', (ev) => {
      const t = ev.touches[0];
      if (!t) return;
      const rect = box.getBoundingClientRect();
      offsetX = t.clientX - rect.left;
      offsetY = t.clientY - rect.top;
      dragging = true;
      window.addEventListener('touchmove', touchMoveHandler, { passive: false });
      window.addEventListener('touchend', touchEndHandler);
    });
    function touchMoveHandler(ev: TouchEvent) {
      if (!dragging) return;
      ev.preventDefault();
      const t = ev.touches[0]; if (!t) return;
      const x = t.clientX - offsetX; const y = t.clientY - offsetY;
  box!.style.left = `${Math.max(0, x)}px`;
  box!.style.top = `${Math.max(0, y)}px`;
  box!.style.right = '';
  box!.style.bottom = '';
  box!.style.transform = 'none';
    }
    function touchEndHandler() { dragging = false; window.removeEventListener('touchmove', touchMoveHandler); window.removeEventListener('touchend', touchEndHandler); try { savePopupPos(); } catch (e) {} }
    // load persisted position if any
    try { loadPopupPos(); } catch (e) { /* ignore */ }
  })();

  function savePopupPos() {
    try {
      const box = document.getElementById('edit-modal-box') as HTMLDivElement | null;
      if (!box) return;
      const rect = box.getBoundingClientRect();
      const pos = { left: Math.max(0, Math.round(rect.left)), top: Math.max(0, Math.round(rect.top)) };
      try { if (window.localStorage) window.localStorage.setItem('editPopupPos', JSON.stringify(pos)); } catch (e) {}
    } catch (e) { /* ignore */ }
  }

  function loadPopupPos() {
    try {
      const raw = window.localStorage ? window.localStorage.getItem('editPopupPos') : null;
      if (!raw) return;
      const pos = JSON.parse(raw) as { left: number; top: number } | null;
      if (!pos) return;
      const box = document.getElementById('edit-modal-box') as HTMLDivElement | null;
      if (!box) return;
      // clamp to viewport to avoid off-screen positions
      const bw = box.offsetWidth || 300;
      const bh = box.offsetHeight || 120;
      const maxLeft = Math.max(0, (window.innerWidth || 800) - bw);
      const maxTop = Math.max(0, (window.innerHeight || 600) - bh);
      const left = Math.min(maxLeft, Math.max(0, pos.left));
      const top = Math.min(maxTop, Math.max(0, pos.top));
      box.style.left = `${left}px`;
      box.style.top = `${top}px`;
      box.style.right = '';
      box.style.bottom = '';
      box.style.transform = 'none';
      // persist clamped position back to storage
      try { if (window.localStorage) window.localStorage.setItem('editPopupPos', JSON.stringify({ left, top })); } catch (e) {}
    } catch (e) { /* ignore */ }
  }

  // Ensure popup remains within viewport on resize
  function clampAndSavePopupPos() {
    try {
      const box = document.getElementById('edit-modal-box') as HTMLDivElement | null;
      if (!box) return;
      const rect = box.getBoundingClientRect();
      const bw = box.offsetWidth || 300;
      const bh = box.offsetHeight || 120;
      const maxLeft = Math.max(0, (window.innerWidth || 800) - bw);
      const maxTop = Math.max(0, (window.innerHeight || 600) - bh);
      const left = Math.min(maxLeft, Math.max(0, Math.round(rect.left)));
      const top = Math.min(maxTop, Math.max(0, Math.round(rect.top)));
      box.style.left = `${left}px`;
      box.style.top = `${top}px`;
      box.style.right = '';
      box.style.bottom = '';
      box.style.transform = 'none';
      try { if (window.localStorage) window.localStorage.setItem('editPopupPos', JSON.stringify({ left, top })); } catch (e) {}
    } catch (e) { /* ignore */ }
  }

  window.addEventListener('resize', () => {
    try { clampAndSavePopupPos(); } catch (e) {}
  });

  function hideEditModal() {
    if (editModal) editModal.style.display = 'none';
    // save position on hide so last-known pos is kept
    try { savePopupPos(); } catch (e) {}
    editTarget = null;
  }

  // click handlers for write-type buttons
  document.querySelectorAll('#edit-modal .write-type').forEach((el) => {
    if (!el || typeof (el as any).addEventListener !== 'function') return;
    (el as HTMLElement).addEventListener('click', (_ev) => {
      const t = (el as HTMLElement).getAttribute('data-typ') || 'U16';
      selectedWriteType = t;
      // highlight popup buttons
      document.querySelectorAll('#edit-modal .write-type').forEach(b => b.classList.remove('active'));
      (el as HTMLElement).classList.add('active');
      // sync to main toolbar selection as well
      try {
        document.querySelectorAll('#display-toolbar .fmt-btn').forEach(x => x.classList.remove('active'));
        const mainBtn = document.querySelector(`#display-toolbar .fmt-btn[data-fmt="${t}"]`);
        if (mainBtn) (mainBtn as HTMLElement).classList.add('active');
  setCurrentFormat(t);
      } catch (e) { /* ignore */ }
    });
  });

  if (editCancel) editCancel.addEventListener('click', (e) => { e.preventDefault(); hideEditModal(); });

  // hide popup on Escape key when displayed
  window.addEventListener('keydown', (ev: KeyboardEvent) => {
    if (ev.key === 'Escape') {
      // only hide if edit modal is visible
      if (editModal && editModal.style.display && editModal.style.display !== 'none') hideEditModal();
    }
  });

  // allow arrow keys to move selected row even when the edit popup is open
  window.addEventListener('keydown', (ev: KeyboardEvent) => {
    try {
      if (ev.key !== 'ArrowUp' && ev.key !== 'ArrowDown') return;
      const popupVisible = !!(editModal && editModal.style.display && editModal.style.display !== 'none');
      const active = document.activeElement as HTMLElement | null;
      const mt = document.getElementById('monitor-table') as HTMLElement | null;
      const mtHasFocus = mt && active === mt;
      // only handle arrows when popup is visible OR the monitor table has focus
      if (!popupVisible && !mtHasFocus) return;
      ev.preventDefault();
      ev.stopPropagation();
      const rows = Array.from(document.querySelectorAll('#monitor-tbody tr')) as HTMLTableRowElement[];
      if (!rows || rows.length === 0) return;
      const sel = document.querySelector('#monitor-tbody tr.selected-row') as HTMLTableRowElement | null;
      let idx = sel ? rows.indexOf(sel) : -1;
      if (idx === -1) {
        // no selection, pick first row
        idx = 0;
      } else {
        if (ev.key === 'ArrowUp') idx = Math.max(0, idx - 1);
        else if (ev.key === 'ArrowDown') idx = Math.min(rows.length - 1, idx + 1);
      }
      const next = rows[idx];
      if (!next) return;
      const m = next.id.match(/^row-(.+)-(\d+)$/);
      if (!m) return;
      const k = m[1]; const a = parseInt(m[2], 10);
      // delegate to selectRow which will dispatch melsec_row_selected and focus back to popup input
      try { selectRow(k, a); } catch (e) { console.warn('selectRow failed', e); }
    } catch (e) { /* ignore */ }
  });

  if (editWrite) editWrite.addEventListener('click', async (e) => {
    e.preventDefault();
    if (!editTarget) return;
    const { key, addr } = editTarget;
    const raw = (editValue && editValue.value) ? editValue.value.trim() : '';
    try {
      let words: number[] = [];
      if (selectedWriteType === 'U16') {
        const v = /^0x/i.test(raw) ? parseInt(raw.substring(2), 16) : parseInt(raw, 10);
        words = [ (v & 0xffff) >>> 0 ];
      } else if (selectedWriteType === 'I16') {
        const v = parseInt(raw, 10);
        words = [ (v & 0xffff) >>> 0 ];
      } else if (selectedWriteType === 'HEX') {
        const v = /^0x/i.test(raw) ? parseInt(raw.substring(2), 16) : parseInt(raw, 16);
        words = [ (v & 0xffff) >>> 0 ];
      } else if (selectedWriteType === 'BIN') {
        const v = parseInt(raw.replace(/^0b/i, ''), 2);
        words = [ (v & 0xffff) >>> 0 ];
      } else if (selectedWriteType === 'ASCII') {
        const s = raw.padEnd(2, '\0').slice(0,2);
        const hi = s.charCodeAt(0) & 0xff;
        const lo = s.charCodeAt(1) & 0xff;
        const w = ((hi << 8) | lo) >>> 0;
        words = [ w & 0xffff ];
      } else if (selectedWriteType === 'U32' || selectedWriteType === 'I32' || selectedWriteType === 'F32') {
        // for 32-bit types, require writing to even address; combine into two words low,high
        // parse number
        let u32: number = 0;
        if (selectedWriteType === 'F32') {
          const f = parseFloat(raw);
          const buf = new ArrayBuffer(4);
          const dv = new DataView(buf);
          dv.setFloat32(0, f, true);
          u32 = dv.getUint32(0, true);
        } else if (selectedWriteType === 'U32') {
          u32 = Number(BigInt(raw));
        } else {
          // I32
          let iv = parseInt(raw, 10);
          if (iv < 0) iv = (iv >>> 0) as unknown as number;
          u32 = iv >>> 0;
        }
        const low = u32 & 0xffff;
        const high = (u32 >>> 16) & 0xffff;
        // if target addr is odd, shift to even and write pair starting at even address
        const baseAddr = (addr % 2 === 0) ? addr : addr - 1;
        words = [ low, high ];
        // perform set_words at baseAddr
        try {
          logMonitor(`[TS] invoking set_words key=${key} addr=${baseAddr} words=${JSON.stringify(words)}`);
          console.log('[TS] invoking set_words', { key, addr: baseAddr, words });
          await invoke('set_words', { key: key, addr: baseAddr, words: words });
        } catch (e) {
          logMonitor(`[TS] set_words error (U32 path): ${e}`);
          console.error('set_words error (U32 path)', e);
        }
        // optimistic UI update for both addresses
        setWordRow(key, baseAddr, low);
        setWordRow(key, baseAddr + 1, high);
        // do NOT hide the popup on write (keep it open per UX)
        return;
      }
      // single-word path
      try {
        logMonitor(`[TS] invoking set_words key=${key} addr=${addr} words=${JSON.stringify(words)}`);
        console.log('[TS] invoking set_words', { key, addr, words });
        await invoke('set_words', { key: key, addr: addr, words: words });
      } catch (e) {
        logMonitor(`[TS] set_words error: ${e}`);
        console.error('set_words error', e);
      }
      setWordRow(key, addr, words.length > 0 ? (words[0] & 0xffff) : 0);
      // do NOT hide the popup on write
    } catch (err) {
      console.warn('write failed', err);
    }
  });

  // allow pressing Enter in the edit input to submit the write
  if (editValue) {
    editValue.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        if (editWrite) (editWrite as HTMLButtonElement).click();
      }
    });
  }

  // attach double-click handler to table body
  const tbody = document.getElementById('monitor-tbody');
  if (tbody) {
    tbody.addEventListener('dblclick', (ev) => {
      let el = ev.target as HTMLElement | null;
      while (el && el.tagName !== 'TR') el = el.parentElement;
      if (!el) return;
      const id = el.id; // e.g. row-D-0 or row-D-0 (we set row id as row-${key}-${addr})
      if (!id) return;
      const m = id.match(/^row-(.+)-(\d+)$/);
      if (!m) return;
      const key = m[1];
      const addr = parseInt(m[2], 10);
      showEditModal(key, addr);
    });
  }

  function parseTarget(s: string | null) {
    if (!s) return null;
    const up = s.toUpperCase().trim();
    let i = 0; while (i < up.length && /[A-Z]/.test(up[i])) i++;
    if (i === 0) return null;
    const key = up.slice(0, i); const numPart = up.slice(i).trim(); if (!numPart) return null;
    const isHex = /[A-F]/i.test(numPart);
    const addr = isHex ? parseInt(numPart, 16) : parseInt(numPart, 10);
    if (Number.isNaN(addr)) return null;
    return { key, addr };
  }

  function createInitialRows(key: string, addr: number, count: number) {
    for (let i = 0; i < count; i++) setWordRow(key, addr + i, 0);
  }

  // Fallback polling is implemented in components/monitor

  // On UI startup, prefetch initial rows for the current monitor target so the table
  // has 30 rows ready immediately. This will try to call backend `get_words` and
  // populate rows; if that fails, fall back to creating empty rows.
  (async () => {
    try {
      const rawTarget = ((els['mon-target'] as HTMLInputElement).value || 'D').toString().trim().toUpperCase();
      let parsed: any = parseTarget(rawTarget);
      if (!parsed) parsed = { key: rawTarget.replace(/[^A-Z]/g, ''), addr: 0 } as any;
      const count = 30;
      try {
        const vals: number[] = await invoke('get_words', { key: parsed.key, addr: parsed.addr, count: count });
        if (Array.isArray(vals) && vals.length > 0) {
          for (let i = 0; i < vals.length; i++) setWordRow(parsed.key, parsed.addr + i, vals[i] & 0xffff);
          if (vals.length < count) createInitialRows(parsed.key, parsed.addr + vals.length, count - vals.length);
          logMonitor(`[TS] initial get_words populated ${vals.length} rows for ${parsed.key}${parsed.addr}`);
        } else {
          createInitialRows(parsed.key, parsed.addr, count);
          logMonitor(`[TS] initial get_words returned empty; created ${count} empty rows for ${parsed.key}${parsed.addr}`);
        }
      } catch (e) {
        // backend might not be running yet; create empty rows so UI has something
        createInitialRows(parsed.key, parsed.addr, count);
        logMonitor(`[TS] initial get_words failed; created ${count} empty rows for ${parsed.key}${parsed.addr}: ${e}`);
      }
    } catch (e) { /* ignore */ }

    // initialize monitor event listeners (monitor, server-status)
    try { await initEventListeners(); } catch (e) { console.warn('initEventListeners failed', e); }

    // Note: development-only auto listener removed to avoid duplicate monitor events.
    // Use the normal event registration inside components/monitor.ts (initEventListeners).
  })();

  const monToggleEl = els['mon-toggle'] as HTMLElement | null;
  if (monToggleEl) {
    monToggleEl.addEventListener('click', (_e) => {
    setTimeout(() => {
      const btn = els['mon-toggle'] as HTMLElement;
      const isRunning = btn && btn.textContent && btn.textContent.includes('停止');
      if (!isEventApiAvailable() && isRunning) {
        try {
          const raw = (els['mon-target'] as HTMLInputElement).value || 'D';
          let parsed: any = parseTarget(raw);
          if (!parsed) parsed = { key: raw.replace(/[^A-Z]/g, ''), addr: 0 } as any;
          if (parsed) startFallbackPolling(parsed.key, parsed.addr, 500);
        } catch (e) { console.warn('failed to start fallback polling', e); }
      } else if (!isEventApiAvailable() && !isRunning) stopFallbackPolling();
    }, 50);
    });
  }

});

// stopMock helper removed — stop is handled inline by mock toggle

// module boundary: no exports required
export {};
