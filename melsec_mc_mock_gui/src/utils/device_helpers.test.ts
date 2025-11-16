import { describe, it, expect } from 'vitest';
import { parseTarget, formatDisplayAddr, getBitConfigForKey } from './device_helpers';

describe('device_helpers', () => {
  it('parseTarget handles X hex alignment', () => {
    expect(parseTarget('X0')).toEqual({ key: 'X', addr: 0 });
    expect(parseTarget('X1')).toEqual({ key: 'X', addr: 0 });
    expect(parseTarget('X10')).toEqual({ key: 'X', addr: 1 });
    expect(parseTarget('x11')).toEqual({ key: 'X', addr: 1 });
  });

  it('parseTarget handles M decimal decade alignment', () => {
    expect(parseTarget('M0')).toEqual({ key: 'M', addr: 0 });
    expect(parseTarget('M1')).toEqual({ key: 'M', addr: 0 });
    expect(parseTarget('M9')).toEqual({ key: 'M', addr: 0 });
    expect(parseTarget('M10')).toEqual({ key: 'M', addr: 1 });
  });

  it('formatDisplayAddr produces expected labels', () => {
    expect(formatDisplayAddr('X', 0)).toBe('X0');
    expect(formatDisplayAddr('X', 1)).toBe('X10');
    expect(formatDisplayAddr('M', 0)).toBe('M0');
    expect(formatDisplayAddr('M', 1)).toBe('M10');
  });

  it('getBitConfigForKey returns correct bit counts', () => {
    expect(getBitConfigForKey('X')).toEqual({ bits: 16, labelBase: 'hex' });
    expect(getBitConfigForKey('M')).toEqual({ bits: 10, labelBase: 'dec' });
    expect(getBitConfigForKey('D')).toEqual({ bits: 16, labelBase: 'hex' });
  });
});
