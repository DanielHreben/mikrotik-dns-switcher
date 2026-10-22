import { describe, expect, it } from 'vitest';

import { routerOSOptions } from './options.ts';

const config = { host: '192.168.88.1', username: 'u', password: 'p' };

describe('routerOSOptions', () => {
  it('uses API-SSL on 8729 when a CA is given', () => {
    const tls = { ca: 'PEM' };
    expect(routerOSOptions(config, tls)).toEqual({
      host: '192.168.88.1',
      user: 'u',
      password: 'p',
      tls,
      port: 8729,
    });
  });

  it('leaves the port to the library (plain 8728) without a CA', () => {
    expect(routerOSOptions(config, undefined)).toEqual({
      host: '192.168.88.1',
      user: 'u',
      password: 'p',
    });
  });
});
