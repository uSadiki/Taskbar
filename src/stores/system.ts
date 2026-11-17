import { create } from "zustand";
import { api, events, report } from "../ipc/api";
import type { AudioState, BatteryState, MediaState, RadiosState } from "../ipc/types";

interface SystemState {
  media: MediaState | null;
  /** Object URL of the current album art, or null. */
  art: string | null;
  /** Average colour of the album art as "r g b", for tinting. */
  artColor: string | null;
  battery: BatteryState | null;
  radios: RadiosState | null;
  audio: AudioState | null;
  /** null when no display supports brightness control. */
  brightness: number | null;
  darkMode: boolean;
}

export const useSystem = create<SystemState>(() => ({
  media: null,
  art: null,
  artColor: null,
  battery: null,
  radios: null,
  audio: null,
  brightness: null,
  darkMode: false,
}));

/** Average colour of an image, from a tiny downscale. */
async function averageColor(blob: Blob): Promise<string | null> {
  try {
    const bitmap = await createImageBitmap(blob, { resizeWidth: 8, resizeHeight: 8 });
    const canvas = new OffscreenCanvas(8, 8);
    const context = canvas.getContext("2d");
    if (!context) return null;
    context.drawImage(bitmap, 0, 0);
    const { data } = context.getImageData(0, 0, 8, 8);
    let r = 0;
    let g = 0;
    let b = 0;
    for (let i = 0; i < data.length; i += 4) {
      r += data[i];
      g += data[i + 1];
      b += data[i + 2];
    }
    const n = data.length / 4;
    return `${Math.round(r / n)} ${Math.round(g / n)} ${Math.round(b / n)}`;
  } catch {
    return null;
  }
}

let artRev = -1;

/** Fetch the album art when its revision changes. */
async function loadArt(rev: number) {
  if (rev === artRev) return;
  artRev = rev;
  try {
    const buffer = await api.mediaArt();
    if (rev !== artRev) return; // a newer track arrived meanwhile
    const previous = useSystem.getState().art;
    if (buffer.byteLength === 0) {
      useSystem.setState({ art: null, artColor: null });
    } else {
      const blob = new Blob([buffer]);
      useSystem.setState({ art: URL.createObjectURL(blob), artColor: await averageColor(blob) });
    }
    if (previous) URL.revokeObjectURL(previous);
  } catch (error) {
    report("media_art")(error);
  }
}

function setMedia(media: MediaState) {
  useSystem.setState({ media });
  void loadArt(media.artRev);
}

/** Re-read everything Quick Toggles shows. Brightness and dark mode have no change events. */
export function refreshToggles() {
  api.togglesSnapshot().then(
    (s) =>
      useSystem.setState({
        radios: s.radios,
        audio: s.audio,
        brightness: s.brightness,
        darkMode: s.darkMode,
      }),
    report("toggles_snapshot"),
  );
}

let started = false;

/** Load the system widgets' state and follow changes. Idempotent. */
export function startSystemStore() {
  if (started) return;
  started = true;
  void events.mediaChanged(setMedia);
  void events.batteryChanged((battery) => useSystem.setState({ battery }));
  void events.radiosChanged((radios) => useSystem.setState({ radios }));
  void events.audioChanged((audio) => useSystem.setState({ audio }));
  api.mediaState().then(setMedia, report("media_state"));
  api.batteryState().then((battery) => useSystem.setState({ battery }), report("battery_state"));
  refreshToggles();
}
