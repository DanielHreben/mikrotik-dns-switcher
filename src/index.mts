import Router from '@koa/router';
import http from 'http';
import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import serve from 'koa-static';

import { config } from './config.mts';
import { createLogger } from './logger.mts';
import { MikrotikInteractorBuilder } from './mikrotik-interactor/index.mts';
import { Service } from './service.mts';

const logger = createLogger();
const mikrotikBuilder = new MikrotikInteractorBuilder(config.mikrotik, logger);
const model = new Service(mikrotikBuilder, config, logger);

const app = new Koa({ proxy: true, proxyIpHeader: 'X-Real-IP' });
const apiRouter = new Router().prefix('/api');

// Middleware
app.use(bodyParser());
app.use(serve('public'));

// Basic routes
apiRouter.get('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const lease = await model.getDHCPLeaseByIP(ip);

  logger.info({ lease });

  ctx.body = {
    ok: true,
    data: {
      ip,
      lease: lease
        ? {
            id: lease.id,
            ip: lease.address,
            mac: lease.macAddress,
            comment: lease.comment,
          }
        : null,
    },
  };
});

apiRouter.put('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const lease = await model.createDHCPLease(ip);

  logger.info({ lease });

  ctx.body = {
    ok: true,
    data: {
      ip,
      lease: {
        id: lease.id,
        ip: lease.address,
        mac: lease.macAddress,
        comment: lease.comment,
      },
    },
  };
});

apiRouter.delete('/dns', async (ctx) => {
  const ip = ctx.request.ip;
  logger.info(ip);

  const lease = await model.deleteDHCPLeaseByIP(ip);

  ctx.body = {
    ok: true,
    data: {
      ip,
      lease: lease
        ? {
            id: lease.id,
            ip: lease.address,
            mac: lease.macAddress,
            comment: lease.comment,
          }
        : null,
    },
  };
});

// API routes
app.use(apiRouter.routes());
app.use(apiRouter.allowedMethods());
// Health check
app.use(
  new Router()
    .get('/health', (ctx) => {
      ctx.body = { status: 'ok' };
    })
    .routes(),
);

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
