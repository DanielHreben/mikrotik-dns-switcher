import Router from '@koa/router';
import http from 'http';
import Koa from 'koa';
import bodyParser from 'koa-bodyparser';
import serve from 'koa-static';

import { config } from './config.mts';

const app = new Koa({ proxy: true });
const router = new Router();

// Middleware
app.use(bodyParser());
app.use(serve('public'));

// Basic routes
router.get('/', async (ctx) => {
  ctx.body = {
    message: 'MikroTik DNS Switcher API',
    version: '1.0.0',
    endpoints: {
      'GET /api': 'API information',
      'GET /api/dns': 'Show current DNS server',
      'POST /api/dns/custom': 'Switch to custom DNS',
      'POST /api/dns/default': 'Remove custom DNS',
    },
  };
});

router.get('/api', async (ctx) => {
  ctx.body = {
    name: 'MikroTik DNS Switcher',
    version: '1.0.0',
    status: 'running',
  };
});

// Health check
router.get('/health', async (ctx) => {
  ctx.body = { status: 'ok' };
});

// Apply routes
app.use(router.routes());
app.use(router.allowedMethods());

// Error handling
app.on('error', (err) => {
  console.error('Server error:', err);
});

// Start server
await new Promise<void>((resolve, reject) => {
  http
    .createServer(app.callback())
    .on('error', reject)
    .listen(config.app.port, resolve);
});

console.log(
  `🚀 MikroTik DNS Switcher running on http://${config.app.host}:${config.app.port}`,
);
