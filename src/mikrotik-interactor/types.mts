type Response<T> =
  | ({
      $$path: string;
    } & T)
  | undefined;

export type DHCPLeaseResponse = Response<{
  $$path: string;
  id: string;
  address: string;
  macAddress: string;
  clientId: string;
  addressLists: string;
  server: string;
  dhcpOption: string;
  status: string;
  expiresAfter: string;
  lastSeen: string;
  age: string;
  activeAddress: string;
  activeMacAddress: string;
  activeClientId: string;
  activeServer: string;
  hostName: string;
  radius: boolean;
  dynamic: boolean;
  blocked: boolean;
  disabled: boolean;
  comment?: string;
}>;

export interface DHCPLeaseCreateOptions {
  address: string;
  comment: string;
  'mac-address': string;
  'dhcp-option': string;
}

export type ARPResponse = Response<{
  id: string;
  address: string;
  macAddress: string;
  interface: string;
  comment?: string;
}>;

export type DHCPOptionResponse = Response<{
  id: string;
  name: string;
  code: string;
  value: string;
  comment?: string;
}>;

export interface DHCPOptionCreateOptions {
  name: string;
  code: string;
  value: string;
  comment: string;
}
