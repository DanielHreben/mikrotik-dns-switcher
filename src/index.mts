import Router from '@koa/router';
import http from 'http';
import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import serve from 'koa-static';

import { config } from './config.mts';
import { createLogger } from './logger.mts';
import { MikroTik } from './mikrotik.mts';

const logger = createLogger();

const app = new Koa({ proxy: true, proxyIpHeader: 'X-Real-IP' });
const apiRouter = new Router().prefix('/api');

// Middleware
app.use(bodyParser());
app.use(serve('public'));

// Basic routes
apiRouter.get('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const lease = await MikroTik.execute(config, async (mikrotik) =>
    mikrotik.getDHCPLeaseByIP(ip),
  );

  logger.info({ lease });

  ctx.body = {
    ip,
    lease,
  };
});

async function findOrCreateDHCPOption(mikrotik: MikroTik) {
  const name = 'Custom DNS Server';
  const code = '6'; // DHCP option code for DNS servers
  const value =
    '0x' +
    config.app.customDns
      .split('.')
      .map((octet) => Number.parseInt(octet, 10).toString(16).padStart(2, '0'))
      .join('');
  logger.info({ name, code, value });
  const existingOption = await mikrotik.getDHCPOptionByName(name);
  if (existingOption) {
    return existingOption;
  }

  return mikrotik.createDHCPOption({ name, code, value });
}

apiRouter.put('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const updatedLease = await MikroTik.execute(config, async (mikrotik) => {
    let lease = await mikrotik.getDHCPLeaseByIP(ip);
    logger.info({ lease });

    if (lease?.dynamic) {
      logger.info('Lease is dynamic, removing it');
      await mikrotik.removeStaticDHCPLease(lease.id);
      lease = undefined;
    }

    if (!lease) {
      const mac = await mikrotik.findMacAddressByIP(ip);

      if (!mac) {
        throw new Error(
          `No DHCP lease found for IP ${ip} and no ARP entry found`,
        );
      }
      const option = await findOrCreateDHCPOption(mikrotik);
      lease = await mikrotik.createStaticDHCPLease({
        ip,
        mac: mac.macAddress,
        option: option.id,
      });

      logger.info('Created new static lease:', lease);
      return lease;
    }

    if (lease.comment === config.app.comment) {
      logger.info('Lease already static with correct comment - doing nothing');
      return lease;
    }

    logger.info('Lease is static but comment is incorrect');
    throw new Error('Lease is already static but comment is incorrect');
  });

  logger.info({ lease: updatedLease });

  ctx.body = {
    ip,
    lease: updatedLease,
  };
});

apiRouter.delete('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const result = await MikroTik.execute(config, async (mikrotik) => {
    const lease = await mikrotik.getDHCPLeaseByIP(ip);
    if (!lease) {
      throw new Error(`No DHCP lease found for IP ${ip}`);
    }

    return mikrotik.removeStaticDHCPLease(lease.id);
  });

  ctx.body = {
    ip,
    result,
  };
});

// // Health check
// app.use(async (ctx) => {
//   ctx.body = { status: 'ok' };
// });

// Apply routes
app.use(apiRouter.routes());
app.use(apiRouter.allowedMethods());

// Error handling
app.on('error', (err) => {
  logger.error('Server error:', err);
});

// Start server
await new Promise<void>((resolve, reject) => {
  http
    .createServer(app.callback())
    .on('error', reject)
    .listen(config.app.port, resolve);
});

logger.info(
  `🚀 MikroTik DNS Switcher running on http://localhost:${config.app.port}`,
);
