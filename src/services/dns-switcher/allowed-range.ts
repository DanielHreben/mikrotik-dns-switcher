// The addresses the switcher may manage, e.g. the router's DHCP pool. Anything
// outside it (the router, fixed devices, a server's other addresses) is refused
// before the router is contacted.

export interface AddressRange {
  from: string;
  to: string;
}

export class AddressNotAllowedError extends Error {
  public constructor(ip: string) {
    super(`Address not managed by the DNS switcher: ${ip}`);
    this.name = 'AddressNotAllowedError';
  }
}

const OCTET = '(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)';
const IPV4 = new RegExp(`^${OCTET}(\\.${OCTET}){3}$`);

function toNumber(ip: string): number | undefined {
  if (!IPV4.test(ip)) return undefined;
  return ip.split('.').reduce((n, octet) => n * 256 + Number(octet), 0);
}

// "192.168.88.20-192.168.88.99" (inclusive)
export function parseRange(spec: string): AddressRange {
  const parts = spec.split('-').map((part) => part.trim());
  const [from, to] = parts;
  const a = from === undefined ? undefined : toNumber(from);
  const b = to === undefined ? undefined : toNumber(to);
  if (parts.length !== 2 || a === undefined || b === undefined || a > b) {
    throw new Error(`Invalid address range: ${spec}`);
  }
  return { from: from as string, to: to as string };
}

// No range configured (local development) means every address is allowed.
export function isAllowedAddress(
  ip: string,
  range: AddressRange | undefined,
): boolean {
  if (!range) return true;
  const n = toNumber(ip);
  return (
    n !== undefined &&
    n >= (toNumber(range.from) as number) &&
    n <= (toNumber(range.to) as number)
  );
}
