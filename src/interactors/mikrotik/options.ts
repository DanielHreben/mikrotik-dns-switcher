import type { TlsOptions } from 'node:tls';

import type { IRosOptions } from 'routeros-api';

export const API_SSL_PORT = 8729;

// Connection options for RouterOSClient. With TLS the port must be given:
// routeros-api only defaults to 8729 when no port is set, but its client always
// fills in 8728, so TLS would otherwise go to the plain API port.
export function routerOSOptions(
  config: { host: string; username: string; password: string },
  tls: TlsOptions | undefined,
): IRosOptions {
  return {
    host: config.host,
    user: config.username,
    password: config.password,
    ...(tls && { tls, port: API_SSL_PORT }),
  };
}
