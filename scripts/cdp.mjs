// Drive a running Glassdock window over the Chrome DevTools Protocol, for
// automated checks during development. No real mouse or keyboard input is
// generated: events are injected into the page only.
//
// Start the app with remote debugging enabled:
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
//   npm run tauri dev
//
// Usage:
//   node scripts/cdp.mjs targets
//   node scripts/cdp.mjs eval  <label> "<js expression>"
//   node scripts/cdp.mjs evalfile <label> <file.js>
//   node scripts/cdp.mjs shot  <label> <out.png>
//   node scripts/cdp.mjs click <label> <x> <y> [left|right|middle]
//   node scripts/cdp.mjs move  <label> <x> <y>
//   node scripts/cdp.mjs key   <label> <key>
//
// <label> is the Tauri window label (dock, settings, glasslab). Coordinates
// are CSS pixels relative to the window.

const PORT = process.env.GLASSDOCK_CDP_PORT ?? "9222";
const BASE = `http://127.0.0.1:${PORT}`;

async function listTargets() {
  const response = await fetch(`${BASE}/json/list`);
  return (await response.json()).filter((t) => t.type === "page");
}

function connect(target) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    let nextId = 1;
    const pending = new Map();
    socket.addEventListener("open", () =>
      resolve({
        send(method, params = {}) {
          const id = nextId++;
          socket.send(JSON.stringify({ id, method, params }));
          return new Promise((res, rej) => pending.set(id, { res, rej }));
        },
        close: () => socket.close(),
      }),
    );
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      if (message.error) waiter.rej(new Error(message.error.message));
      else waiter.res(message.result);
    });
    socket.addEventListener("error", () => reject(new Error("could not connect to the page")));
  });
}

async function evaluate(session, expression) {
  const { result, exceptionDetails } = await session.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails) {
    throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  }
  return result.value;
}

/** Find the page whose Tauri window label matches. */
async function sessionFor(label) {
  for (const target of await listTargets()) {
    const session = await connect(target);
    try {
      const found = await evaluate(
        session,
        "window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label ?? null",
      );
      if (found === label) return session;
    } catch {
      // Not a Glassdock page, or not ready yet.
    }
    session.close();
  }
  throw new Error(`no window with label "${label}" (is the app running with remote debugging?)`);
}

const [command, label, ...rest] = process.argv.slice(2);

try {
  if (command === "targets") {
    for (const target of await listTargets()) console.log(`${target.title}  ${target.url}`);
  } else if (command === "eval" || command === "evalfile") {
    const session = await sessionFor(label);
    const source =
      command === "evalfile"
        ? (await import("node:fs/promises")).readFile(rest[0], "utf8")
        : Promise.resolve(rest.join(" "));
    const value = await evaluate(session, await source);
    console.log(typeof value === "string" ? value : JSON.stringify(value, null, 2));
    session.close();
  } else if (command === "shot") {
    const { writeFile } = await import("node:fs/promises");
    const session = await sessionFor(label);
    // Keep the page's transparency instead of flattening onto white.
    await session.send("Emulation.setDefaultBackgroundColorOverride", {
      color: { r: 0, g: 0, b: 0, a: 0 },
    });
    const { data } = await session.send("Page.captureScreenshot", { format: "png" });
    await writeFile(rest[0], Buffer.from(data, "base64"));
    console.log(`saved ${rest[0]}`);
    session.close();
  } else if (command === "click" || command === "move") {
    const session = await sessionFor(label);
    const x = Number(rest[0]);
    const y = Number(rest[1]);
    const button = rest[2] ?? "left";
    await session.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    if (command === "click") {
      const common = { x, y, button, clickCount: 1 };
      await session.send("Input.dispatchMouseEvent", { type: "mousePressed", ...common });
      await session.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...common });
    }
    session.close();
  } else if (command === "key") {
    const session = await sessionFor(label);
    for (const type of ["keyDown", "keyUp"]) {
      await session.send("Input.dispatchKeyEvent", { type, key: rest[0], code: rest[0] });
    }
    session.close();
  } else {
    console.error("usage: node scripts/cdp.mjs targets|eval|shot|click|move|key ...");
    process.exitCode = 2;
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
