import type { Request, Response, NextFunction } from "express";
import { redis } from "./store/redis.js";
import { logger } from "./logger.js";

/**
 * IP 기준 간단 레이트리밋 (Redis INCR + TTL). Redis 장애 시 fail-open(허용)해
 * 일시적 Redis 문제로 정상 사용자를 막지 않는다. trust proxy 가 켜져 req.ip 가 실제 IP.
 */
export function rateLimit(opts: {
  name: string;
  max: number;
  windowSec: number;
  /** 한도 기준 키 (기본=IP). 사내 단일 NAT 대비해 가능하면 신원(verifier/세션)으로. */
  key?: (req: Request) => string;
}) {
  return async (req: Request, res: Response, next: NextFunction): Promise<void> => {
    try {
      const id = (opts.key?.(req) || req.ip || "unknown").slice(0, 200);
      const key = `rl:${opts.name}:${id}`;
      const n = await redis().incr(key);
      if (n === 1) await redis().expire(key, opts.windowSec);
      if (n > opts.max) {
        res.status(429).json({ error: "too many requests" });
        return;
      }
    } catch (err) {
      logger.warn({ err, name: opts.name }, "rateLimit redis error (fail-open)");
    }
    next();
  };
}
