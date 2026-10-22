import { describe, expect, it } from 'vitest';

import { isAllowedAddress, parseRange } from './allowed-range.ts';

describe('parseRange', () => {
  it('parses an inclusive IPv4 range', () => {
    expect(parseRange('192.168.88.20-192.168.88.99')).toEqual({
      from: '192.168.88.20',
      to: '192.168.88.99',
    });
  });

  it('rejects malformed or reversed ranges', () => {
    expect(() => parseRange('192.168.88.20')).toThrow();
    expect(() => parseRange('192.168.88.99-192.168.88.20')).toThrow();
    expect(() => parseRange('192.168.88.20-192.168.88.300')).toThrow();
  });
});

describe('isAllowedAddress', () => {
  const pool = parseRange('192.168.88.20-192.168.88.99');

  it('allows addresses inside the range, ends included', () => {
    expect(isAllowedAddress('192.168.88.20', pool)).toBe(true);
    expect(isAllowedAddress('192.168.88.55', pool)).toBe(true);
    expect(isAllowedAddress('192.168.88.99', pool)).toBe(true);
  });

  it('refuses addresses outside the range', () => {
    expect(isAllowedAddress('192.168.88.19', pool)).toBe(false);
    expect(isAllowedAddress('192.168.88.100', pool)).toBe(false);
    expect(isAllowedAddress('192.168.88.101', pool)).toBe(false);
    expect(isAllowedAddress('172.19.0.1', pool)).toBe(false);
  });

  it('refuses anything that is not a plain IPv4 address', () => {
    expect(isAllowedAddress('', pool)).toBe(false);
    expect(isAllowedAddress('192.168.88.50 ', pool)).toBe(false);
    expect(isAllowedAddress('::1', pool)).toBe(false);
    expect(isAllowedAddress('192.168.88.050', pool)).toBe(false);
  });

  it('allows everything when no range is configured (local development)', () => {
    expect(isAllowedAddress('10.1.2.3', undefined)).toBe(true);
  });
});
