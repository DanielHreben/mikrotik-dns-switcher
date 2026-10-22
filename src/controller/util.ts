import type { Context } from 'koa';

export interface ClientIPOptions {
  // The reverse proxy whose X-Real-IP header is trusted (e.g. nginx).
  trustedProxy?: string | undefined;
  // Trust X-Real-IP from any caller, so ?ip= in the page can act for another
  // device. Local development only.
  allowTargetIP: boolean;
}

function normalize(address: string | undefined): string {
  if (!address) return '';
  return address.startsWith('::ffff:')
    ? address.slice('::ffff:'.length)
    : address;
}

// Decide which device a request acts for: the X-Real-IP header when the
// request comes from the trusted proxy (or target IPs are allowed), otherwise
// the address the connection comes from.
export function resolveClientIP(
  remoteAddress: string | undefined,
  headerIP: string | undefined,
  options: ClientIPOptions,
): string {
  const remote = normalize(remoteAddress);
  const header = (headerIP ?? '').split(',')[0]?.trim() ?? '';
  const trusted =
    options.allowTargetIP ||
    (options.trustedProxy !== undefined && remote === options.trustedProxy);
  return trusted && header ? header : remote;
}

export function extractClientIP(
  ctx: Context,
  options: ClientIPOptions,
): string {
  return resolveClientIP(
    ctx.req.socket.remoteAddress,
    ctx.get('X-Real-IP'),
    options,
  );
}
