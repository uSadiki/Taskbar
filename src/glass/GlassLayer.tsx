import { useEffect, useRef } from "react";
import { create } from "zustand";
import { api, events, report } from "../ipc/api";
import type { DockLayout } from "../ipc/types";
import { buildLumaGrid, type LumaGrid } from "./luma";
import { GlassRenderer, type GlassParams } from "./renderer";
import { scene } from "./scene";

interface GlassState {
  /** False when the wallpaper could not be read and the glass is a flat fill. */
  available: boolean;
  /** Wallpaper brightness, for choosing light or dark content. */
  grid: LumaGrid | null;
}

export const useGlass = create<GlassState>(() => ({ available: false, grid: null }));

interface GlassLayerProps {
  params: GlassParams;
  layout: DockLayout;
  /** Base colour when there is no wallpaper to refract. */
  fallback: [number, number, number];
}

/** The WebGL canvas behind the dock's content. Draws only when something changes. */
export function GlassLayer({ params, layout, fallback }: GlassLayerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<GlassRenderer | null>(null);
  const paramsRef = useRef(params);
  paramsRef.current = params;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let renderer: GlassRenderer;
    try {
      renderer = new GlassRenderer(canvas, paramsRef.current);
    } catch (error) {
      report("glass renderer")(error);
      return;
    }
    rendererRef.current = renderer;
    // Dev builds expose the renderer so scripted checks can inspect it.
    if (import.meta.env.DEV) (window as unknown as { __glass?: GlassRenderer }).__glass = renderer;

    const sync = () => renderer.setShapes(scene.shapes(), scene.blendDistance());
    const unsubscribe = scene.subscribe(sync);
    sync();

    let cancelled = false;
    let loading = false;
    let stale = false;
    const load = async () => {
      // One fetch at a time; a change during a fetch triggers one more.
      if (loading) {
        stale = true;
        return;
      }
      loading = true;
      try {
        const buffer = await api.glassBackdrop();
        if (cancelled) return;
        const header = new DataView(buffer, 0, 16);
        const width = header.getUint32(4, true);
        const height = header.getUint32(8, true);
        if (width > 0 && height > 0 && buffer.byteLength >= 16 + width * height * 4) {
          const pixels = new Uint8Array(buffer, 16, width * height * 4);
          renderer.setBackdrop(pixels, width, height);
          useGlass.setState({ available: true, grid: buildLumaGrid(pixels, width, height) });
        } else {
          renderer.setBackdrop(null, 0, 0);
          useGlass.setState({ available: false, grid: null });
        }
      } catch (error) {
        report("glass_backdrop")(error);
      } finally {
        loading = false;
        if (stale && !cancelled) {
          stale = false;
          void load();
        }
      }
    };
    void load();

    const unlisten = [
      events.backdrop(() => void load()),
      events.dockOrigin((o) => renderer.setOrigin(o.originX, o.originY)),
    ];
    const onResize = () => renderer.requestRender();
    window.addEventListener("resize", onResize);

    return () => {
      cancelled = true;
      unsubscribe();
      unlisten.forEach((p) => void p.then((fn) => fn()));
      window.removeEventListener("resize", onResize);
      renderer.dispose();
      rendererRef.current = null;
    };
  }, []);

  useEffect(() => rendererRef.current?.setParams(params), [params]);
  useEffect(() => rendererRef.current?.setFallback(fallback), [fallback]);
  useEffect(
    () => rendererRef.current?.setOrigin(layout.originX, layout.originY),
    [layout.originX, layout.originY],
  );

  return <canvas ref={canvasRef} className="pointer-events-none absolute inset-0 h-full w-full" />;
}
