import type { Context, Next } from 'koa';

interface ServiceDefinition<T, P, C> {
  validationRules?: Record<string, unknown>;
  execute: (deps: T) => (params: P, context: C) => Promise<unknown>;
}

interface ControllerDefaults {
  renderSuccess: (
    data: unknown,
    ctx: Context,
    next?: Next,
  ) => Promise<void> | void;
  renderError: (
    error: Error,
    ctx: Context,
    next?: Next,
  ) => Promise<void> | void;
  validator: (
    data: unknown,
    validationRules?: Record<string, unknown>,
  ) => unknown;
  extractParams: (ctx: Context) => Record<string, unknown>;
  extractContext: (ctx: Context) => unknown;
}

const defaults: ControllerDefaults = {
  renderSuccess: (data, ctx) => {
    ctx.body = {
      ok: true,
      data,
    };
  },

  renderError(error, ctx) {
    // eslint-disable-next-line no-console
    console.error(error);

    ctx.status = 500;
    ctx.body = {
      ok: false,
      error: {
        code: 'UNEXPECTED_ERROR',
        message: 'Something went wrong',
      },
    };
  },

  validator(data) {
    return data;
  },

  extractParams: (ctx): Record<string, unknown> => ({
    ...(ctx.params as Record<string, unknown>),
    ...(ctx.query as Record<string, unknown>),
    ...(ctx.request.body as Record<string, unknown>),
  }),

  extractContext: (ctx) => ctx.state,
};

interface ServiceBuilder<I, O> {
  extractParams(extractor: (ctx: Context) => I): ServiceBuilder<I, O>;
  renderError(
    renderer: (error: Error, ctx: Context, next?: Next) => Promise<void> | void,
  ): ServiceBuilder<I, O>;
  renderSuccess(
    renderer: (data: O, ctx: Context, next?: Next) => Promise<void> | void,
  ): ServiceBuilder<I, O>;
  buildHandler(): (ctx: Context) => Promise<void>;
  buildMiddleware(): (ctx: Context, next: Next) => Promise<void>;
}

export function createController<T>(deps: T) {
  return function controllerFactory<P, C>(
    service: ServiceDefinition<T, P, C>,
  ): ServiceBuilder<P, unknown> {
    let extractParams: (ctx: Context) => unknown = defaults.extractParams;
    const extractContext = defaults.extractContext;
    let renderSuccess = defaults.renderSuccess;
    let renderError = defaults.renderError;

    const handler = async (ctx: Context, next?: Next) => {
      try {
        const params = defaults.validator(
          extractParams(ctx),
          service.validationRules,
        ) as P;
        const context = extractContext(ctx) as C;

        const execute = service.execute(deps);
        const data = await execute(params, context);

        await renderSuccess(data, ctx, next);
      } catch (error) {
        await renderError(error as Error, ctx, next);
      }
    };

    const serviceBuilder: ServiceBuilder<P, unknown> = {
      extractParams(extractor) {
        extractParams = extractor;
        return this;
      },
      renderError(renderer) {
        renderError = renderer;
        return this;
      },
      renderSuccess(renderer) {
        renderSuccess = renderer;
        return this;
      },
      buildHandler() {
        return async (ctx: Context) => handler(ctx);
      },
      buildMiddleware() {
        return async (ctx: Context, next: Next) => handler(ctx, next);
      },
    };

    return serviceBuilder;
  };
}
