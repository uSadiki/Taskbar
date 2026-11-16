import { animate } from "motion";
import type { GlassShape } from "./renderer";

type Listener = () => void;

interface Entry {
  shape: GlassShape;
  stop?: () => void;
}

interface MoveOptions {
  /** Start from this shape instead of the current one (a popover leaving its tile). */
  from?: GlassShape;
  /** Smooth-union distance at the start and end of the move, CSS pixels. */
  blend?: [number, number];
  /** Skip the spring and jump. */
  instant?: boolean;
}

const SPRING = { type: "spring" as const, stiffness: 420, damping: 34, mass: 1 };

function lerp(a: GlassShape, b: GlassShape, t: number): GlassShape {
  const mix = (x: number, y: number) => x + (y - x) * t;
  return {
    x: mix(a.x, b.x),
    y: mix(a.y, b.y),
    w: Math.max(0, mix(a.w, b.w)),
    h: Math.max(0, mix(a.h, b.h)),
    r: Math.max(0, mix(a.r, b.r)),
    frost: Math.min(1, Math.max(0, mix(a.frost ?? 0, b.frost ?? 0))),
  };
}

/**
 * The glass surfaces currently on screen and their spring animations. The
 * renderer subscribes and redraws only when something here changes.
 */
export class GlassScene {
  private entries = new Map<string, Entry>();
  private blend = 0;
  private listeners = new Set<Listener>();
  /** Set from the reduce-motion preference: moves become jumps. */
  reducedMotion = false;

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed() {
    this.listeners.forEach((l) => l());
  }

  /** Shapes in the order they were added (the dock first). */
  shapes(): GlassShape[] {
    return [...this.entries.values()].map((e) => e.shape);
  }

  blendDistance(): number {
    return this.blend;
  }

  get(id: string): GlassShape | undefined {
    return this.entries.get(id)?.shape;
  }

  /** Move a shape to `target`, creating it if needed. */
  set(id: string, target: GlassShape, options: MoveOptions = {}) {
    const entry = this.entries.get(id);
    entry?.stop?.();
    const start = options.from ?? entry?.shape;
    const [blendStart, blendEnd] = options.blend ?? [this.blend, this.blend];

    if (!start || options.instant || this.reducedMotion) {
      this.entries.set(id, { shape: target });
      this.blend = blendEnd;
      this.changed();
      return;
    }

    const next: Entry = { shape: start };
    this.entries.set(id, next);
    const controls = animate(0, 1, {
      ...SPRING,
      onUpdate: (t) => {
        next.shape = lerp(start, target, t);
        // The spring overshoots; the blend must not.
        this.blend = blendStart + (blendEnd - blendStart) * Math.min(1, Math.max(0, t));
        this.changed();
      },
      onComplete: () => {
        next.shape = target;
        next.stop = undefined;
        this.blend = blendEnd;
        this.changed();
      },
    });
    next.stop = () => controls.stop();
  }

  /** Remove a shape, optionally shrinking it into `to` first. */
  remove(id: string, options: { to?: GlassShape; blend?: [number, number] } = {}) {
    const entry = this.entries.get(id);
    if (!entry) return;
    entry.stop?.();
    if (!options.to || this.reducedMotion) {
      this.entries.delete(id);
      if (options.blend) this.blend = options.blend[1];
      this.changed();
      return;
    }
    const start = entry.shape;
    const [blendStart, blendEnd] = options.blend ?? [this.blend, this.blend];
    const controls = animate(0, 1, {
      ...SPRING,
      stiffness: 520,
      damping: 40,
      onUpdate: (t) => {
        entry.shape = lerp(start, options.to!, Math.min(1, t));
        this.blend = blendStart + (blendEnd - blendStart) * Math.min(1, Math.max(0, t));
        this.changed();
      },
      onComplete: () => {
        // Only delete if this entry is still the live one.
        if (this.entries.get(id) === entry) this.entries.delete(id);
        this.blend = blendEnd;
        this.changed();
      },
    });
    entry.stop = () => controls.stop();
  }
}

export const scene = new GlassScene();
