import type { Context } from 'koa';

// Extract client IP from Koa context (considering proxy headers)
export function extractClientIP(ctx: Context): string {
  return ctx.request.ip;
}
