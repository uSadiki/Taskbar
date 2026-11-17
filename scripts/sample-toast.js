// Development check: show a toast the way the backend sends one.
window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {
  event: "alert://show",
  payload: {
    id: "claude-fiveHour",
    title: "Claude 5-hour limit at 82%",
    body: "Resets Tue 14:00.",
    level: "warning",
    open: "claude",
  },
}).then(
  () => "toast sent",
  (e) => "ERR " + e,
)
