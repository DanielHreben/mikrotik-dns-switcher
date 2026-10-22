import { describe, expect, it } from 'vitest';

import { resolveClientIP } from './util.ts';

const PROXY = '172.19.0.100';

describe('resolveClientIP', () => {
  it('uses the X-Real-IP header when the request comes from the trusted proxy', () => {
    expect(
      resolveClientIP(PROXY, '192.168.88.42', {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe('192.168.88.42');
  });

  it('accepts the trusted proxy as an IPv4-mapped IPv6 address', () => {
    expect(
      resolveClientIP(`::ffff:${PROXY}`, '192.168.88.42', {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe('192.168.88.42');
  });

  it('ignores the header from any other caller and uses the connection address', () => {
    expect(
      resolveClientIP('::ffff:172.19.0.18', '192.168.88.42', {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe('172.19.0.18');
  });

  it('ignores the header when no proxy is trusted and target IPs are off', () => {
    expect(
      resolveClientIP('192.168.88.7', '192.168.88.42', {
        allowTargetIP: false,
      }),
    ).toBe('192.168.88.7');
  });

  it('uses the header from anyone when target IPs are allowed (local development)', () => {
    expect(
      resolveClientIP('127.0.0.1', '192.168.88.42', { allowTargetIP: true }),
    ).toBe('192.168.88.42');
  });

  it('falls back to the connection address when the header is missing or blank', () => {
    expect(
      resolveClientIP(PROXY, undefined, {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe(PROXY);
    expect(
      resolveClientIP(PROXY, '  ', {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe(PROXY);
  });

  it('uses only the first address of a comma-separated header', () => {
    expect(
      resolveClientIP(PROXY, ' 192.168.88.42, 10.0.0.1', {
        trustedProxy: PROXY,
        allowTargetIP: false,
      }),
    ).toBe('192.168.88.42');
  });
});
