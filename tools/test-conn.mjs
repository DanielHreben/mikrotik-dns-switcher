import { RouterOSClient } from 'routeros-api';
const client = new RouterOSClient({ host: '127.0.0.1', user: 'admin', password: 'test', port: 8728 });
try {
  const api = await client.connect();
  const menu = api.menu('/ip/dhcp-server/lease');
  const res = await menu.getOnly({ address: '192.168.88.42' });
  console.log(res);
  await client.close();
} catch (e) {
  console.error('ERR:', e.message);
}
