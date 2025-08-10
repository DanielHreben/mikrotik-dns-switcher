import type { DHCPLeaseResponse } from '../../interactors/mikrotik/types.mts';

export const CLIENT_STATUS = {
  DEFAULT: 'DEFAULT',
  CUSTOM: 'CUSTOM',
  UNMANAGED: 'UNMANAGED',
} as const;

export function getClientStatus(lease: DHCPLeaseResponse, comment: string) {
  if (!lease) {
    return CLIENT_STATUS.DEFAULT;
  }
  if (lease.dynamic) {
    return CLIENT_STATUS.DEFAULT;
  }
  if (lease.comment !== comment) {
    return CLIENT_STATUS.UNMANAGED;
  }

  return CLIENT_STATUS.CUSTOM;
}
