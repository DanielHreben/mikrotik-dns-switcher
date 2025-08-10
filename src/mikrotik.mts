import type { RosApiMenu } from 'routeros-api';
import { RouterOSClient } from 'routeros-api';

import type { Config } from './config.mts';

interface DHCPLease {
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
}

export class MikroTik {
  private comment: string;
  private client: RosApiMenu;

  public static async execute<T>(
    config: Config,
    action: (client: MikroTik) => Promise<T>,
  ): Promise<T> {
    const api = new RouterOSClient({
      host: config.mikrotik.host,
      user: config.mikrotik.username,
      password: config.mikrotik.password,
    });

    const comment = config.app.comment;
    const client = await api.connect();

    try {
      return await action(new MikroTik(client, comment));
    } finally {
      api.close();
    }
  }

  private constructor(client: RosApiMenu, comment: string) {
    this.client = client;
    this.comment = comment;
  }

  public async getDHCPLeaseByIP(ip: string) {
    const result = (await this.client.menu('/ip dhcp-server lease').getOnly({
      address: ip,
    })) as DHCPLease | undefined;

    return result;
  }

  public async makeDHCPLeaseStatic(id: string, options: { option: string }) {
    const result = await this.client
      .menu('/ip dhcp-server lease')
      .exec('make-static', {
        id: id,
        comment: this.comment,
        'dhcp-option': options.option,
      });

    return result;
  }

  public async createStaticDHCPLease(options: {
    ip: string;
    mac: string;
    option: string;
  }) {
    // TODO: option!
    const lease = (await this.client.menu('/ip dhcp-server lease').add({
      address: options.ip,
      comment: this.comment,
      'mac-address': options.mac,
      'dhcp-option': options.option,
    })) as DHCPLease;

    return lease;
  }

  public async findMacAddressByIP(ip: string) {
    const arp = (await this.client.menu('/ip arp').getOnly({ address: ip })) as
      | { macAddress: string }
      | undefined;

    console.log({ arp });

    return arp;
  }

  public async removeStaticDHCPLease(id: string) {
    const result = await this.client.menu('/ip dhcp-server lease').remove(id);
    console.log('Removed static lease:', result);
    return result;
  }

  public async createDHCPOption(options: {
    name: string;
    code: string;
    value: string;
  }) {
    const result = (await this.client.menu('/ip dhcp-server option').add({
      name: options.name,
      code: options.code,
      value: options.value,
      comment: this.comment,
    })) as {
      id: string;
      name: string;
      code: string;
      value: string;
    };

    console.log('Created DHCP option:', result);
    return result;
  }

  public async getDHCPOptionByName(name: string) {
    const result = (await this.client
      .menu('/ip dhcp-server option')
      .getOnly({ name })) as
      | { id: string; name: string; value: string }
      | undefined;

    return result;
  }
}
