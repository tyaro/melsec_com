import { describe, it, expect, beforeEach } from 'vitest';
import { parseTarget, createInitialRows, setWordRow, latestWords } from '../components/monitor';

describe('monitor component basic behaviors', () => {
  beforeEach(() => {
    // clear DOM tbody
    const tbody = document.getElementById('monitor-tbody');
    if (tbody) tbody.innerHTML = '';
    for (const k in latestWords) delete latestWords[k];
  });

  it('parseTarget parses decimal and hex addresses', () => {
    expect(parseTarget('D0')).toEqual({ key: 'D', addr: 0 });
    expect(parseTarget('D10')).toEqual({ key: 'D', addr: 10 });
    expect(parseTarget('WFF')).toEqual({ key: 'W', addr: 0xFF });
  });

  it('createInitialRows makes 30 rows', () => {
    const container = document.createElement('table');
    const tbody = document.createElement('tbody');
    tbody.id = 'monitor-tbody';
    container.appendChild(tbody);
    document.body.appendChild(container);
    createInitialRows('D', 0, 30);
    const rows = document.querySelectorAll('#monitor-tbody tr');
    expect(rows.length).toBe(30);
  });

  it('setWordRow updates latestWords and renders bit cells', () => {
    const container = document.createElement('table');
    const tbody = document.createElement('tbody');
    tbody.id = 'monitor-tbody';
    container.appendChild(tbody);
    document.body.appendChild(container);
    setWordRow('D', 0, 0x8001);
    expect(latestWords['D:0']).toBe(0x8001 & 0xffff);
    const tr = document.getElementById('row-D-0');
    expect(tr).not.toBeNull();
    const bits = tr!.querySelectorAll('td.bit-cell.bit-on');
    // bit 15 and bit 0 are on (0x8001)
    expect(bits.length).toBeGreaterThanOrEqual(2);
  });
});
